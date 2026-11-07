using System.Collections.Concurrent;
using System.Security.Cryptography;
using System.Text;
using System.Timers;

namespace NativeRunner.TorrentHandler
{
    public sealed class LibSession : IDisposable
    {
        private static readonly Lazy<LibSession> _instance = new(() => new LibSession());
        public static LibSession Instance => _instance.Value;

        private const int EventBatch = 128;

        private readonly nint _session;
        private readonly System.Timers.Timer _timer;
        private readonly Thread _eventThread;
        private readonly ConcurrentDictionary<long, LibTorrentHandler> _byId = [];
        private int _updating;
        private volatile bool _disposed;

        public ConcurrentDictionary<string, LibTorrentHandler> Handlers { get; } = [];

        /// <summary>Raised on the timer thread with each torrent's fresh status, about once a second while any torrent is active.</summary>
        public event Action<LibTorrentHandler, TorrentStatus> StatusUpdated;

        public static string NativeVersion => NativeTS.Version();

        private LibSession()
        {
            _session = NativeTS.CreateSession();
            if (_session == 0)
            {
                throw new InvalidOperationException("tsdl session could not be created: " + NativeTS.LastError());
            }

            _eventThread = new Thread(PumpEvents) { IsBackground = true, Name = "tsdl events" };
            _eventThread.Start();

            _timer = new System.Timers.Timer(1000);
            _timer.Elapsed += Timer_Elapsed;

            AppDomain.CurrentDomain.ProcessExit += (_, _) => Dispose();
        }

        /// <summary>Starts the status timer; it stops itself once no torrent is downloading or watched.</summary>
        internal void Wake()
        {
            if (!_disposed) _timer.Start();
        }

        private void Timer_Elapsed(object sender, ElapsedEventArgs e)
        {
            if (Interlocked.Exchange(ref _updating, 1) == 1) return;
            try
            {
                foreach (var handler in Handlers.Values)
                {
                    var status = handler.Update();
                    if (status != null) StatusUpdated?.Invoke(handler, status);
                }
                if (!Handlers.Values.Any(x => x.IsActive))
                {
                    _timer.Stop();
                    // a torrent added or resumed during this tick must keep the timer alive
                    if (Handlers.Values.Any(x => x.IsActive)) _timer.Start();
                }
            }
            catch { }
            finally
            {
                Volatile.Write(ref _updating, 0);
            }
        }

        private unsafe void PumpEvents()
        {
            var events = new NativeStructs.TsEvent[EventBatch];
            while (!_disposed)
            {
                int count;
                fixed (NativeStructs.TsEvent* ptr = events)
                {
                    count = NativeTS.PollEvents(_session, ptr, events.Length, 250);
                }

                for (var i = 0; i < count; i++)
                {
                    if (_byId.TryGetValue(events[i].torrent_id, out var handler))
                    {
                        handler.OnNativeEvent(in events[i]);
                    }
                }
            }
        }

        /// <param name="folderPath">Where the torrent's files are downloaded.</param>
        /// <param name="resumeFolder">Where resume data is kept; defaults to <c>folderPath/_cache</c>.</param>
        public LibTorrentHandler AddTorrent(string url, string folderPath, string resumeFolder = null)
        {
            ObjectDisposedException.ThrowIf(_disposed, this);

            if (Handlers.TryGetValue(url, out LibTorrentHandler handler) && !handler.IsDisposed)
            {
                return handler;
            }

            resumeFolder ??= Path.Combine(folderPath, "_cache");
            Directory.CreateDirectory(folderPath);
            Directory.CreateDirectory(resumeFolder);
            var resumeFile = Path.Combine(resumeFolder, GetHashString(url) + ".resume");

            handler = new LibTorrentHandler(this, _session, url, folderPath, resumeFile);
            if (handler.IsDisposed)
            {
                return handler;
            }

            _byId[handler.Id] = handler;
            Handlers[url] = handler;
            Wake();
            return handler;
        }

        public void StopTorrent(string url)
        {
            _ = StopTorrentAsync(url);
        }

        public async Task StopTorrentAsync(string url)
        {
            if (Handlers.TryRemove(url, out var existHandler))
            {
                await existHandler.DisposeAsync();
            }
        }

        internal void Forget(LibTorrentHandler handler)
        {
            _byId.TryRemove(handler.Id, out _);
        }

        /// <summary>
        /// Saves resume data of every torrent, then destroys the native session. Runs on process exit.
        /// </summary>
        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;

            _timer.Stop();
            _timer.Dispose();
            _eventThread.Join();

            foreach (var handler in Handlers.Values)
            {
                handler.OnSessionClosed();
            }
            Handlers.Clear();
            _byId.Clear();

            NativeTS.DestroySession(_session);
        }

        private static string GetHashString(string inputString)
        {
            StringBuilder sb = new StringBuilder();
            foreach (byte b in SHA256.HashData(Encoding.UTF8.GetBytes(inputString)))
                sb.Append(b.ToString("X2"));

            return sb.ToString();
        }
    }
}
