using System.Runtime.InteropServices;

namespace NativeRunner.TorrentHandler
{
    public class LibTorrentHandler : IDisposable, IAsyncDisposable
    {
        public string Url { get; }
        public string LastError { get; private set; }

        internal long Id { get; }

        private readonly LibSession _owner;
        private readonly nint _session;
        private readonly string _saveFolder;
        private readonly nint _handler;
        // serialises native calls against detach: no call may run on a freed handle
        private readonly Lock _gate = new();
        private volatile bool _disposed;
        private DateTime _lastUpdate;
        private TorrentFileList fileCol;
        private Stream stream;
        private bool _fileLocked;
        private Action<TorrentStatus> _onMeta;
        private byte[] _have = [];
        private TaskCompletionSource _pieceArrived = NewSignal();

        internal LibTorrentHandler(LibSession owner, nint session, string url, string saveFolder, string resumeFile)
        {
            _owner = owner;
            Url = url;
            _session = session;
            _saveFolder = saveFolder;

            if (url.StartsWith("magnet:", StringComparison.OrdinalIgnoreCase) || !File.Exists(url))
            {
                _handler = NativeTS.AttachMagnet(_session, url, saveFolder, resumeFile);
            }
            else
            {
                var info = NativeTS.TorrentInfoFromFile(url);
                if (info != 0)
                {
                    _handler = NativeTS.AttachTorrent(_session, info, saveFolder, resumeFile);
                    NativeTS.DestroyTorrentInfo(info);
                }
            }

            if (_handler == 0)
            {
                LastError = NativeTS.LastError();
                _disposed = true;
                return;
            }
            Id = NativeTS.TorrentId(_handler);
        }

        public bool IsDisposed => _disposed;

        /// <summary>Completes (and is replaced) whenever a piece of this torrent finishes, or on dispose.</summary>
        internal Task PieceArrived => Volatile.Read(ref _pieceArrived).Task;

        private static TaskCompletionSource NewSignal() => new(TaskCreationOptions.RunContinuationsAsynchronously);

        private void SignalPieces()
        {
            Interlocked.Exchange(ref _pieceArrived, NewSignal()).TrySetResult();
        }

        private bool TryNative<T>(Func<nint, T> call, out T result)
        {
            lock (_gate)
            {
                if (_disposed)
                {
                    result = default;
                    return false;
                }
                result = call(_handler);
                return true;
            }
        }

        internal unsafe void OnNativeEvent(in NativeStructs.TsEvent e)
        {
            switch (e.type)
            {
                case NativeStructs.TsEventType.PieceFinished:
                    var have = _have;
                    if ((uint)e.piece < (uint)have.Length) Volatile.Write(ref have[e.piece], 1);
                    SignalPieces();
                    break;
                case NativeStructs.TsEventType.TorrentError:
                case NativeStructs.TsEventType.FileError:
                    fixed (byte* message = e.message)
                    {
                        LastError = Marshal.PtrToStringUTF8((nint)message);
                    }
                    break;
            }
        }

        private TorrentStatus ReadStatus()
        {
            if (!TryNative(h => NativeTS.GetTorrentStatus(h, out var s) == NativeTS.Ok ? (NativeStructs.TsTorrentStatus?)s : null, out var status) || status == null)
            {
                return null;
            }
            return new TorrentStatus(status.Value);
        }

        public async Task<TorrentStatus> GetTorrentStatus()
        {
            while (!_disposed)
            {
                var status = ReadStatus();
                if (status != null && status.Progress != 0)
                {
                    return status;
                }
                await Task.Delay(250);
            }
            return null;
        }

        public Task<TorrentFileList> GetFileListAsync()
        {
            if (fileCol != null) return Task.FromResult(fileCol);

            var ok = TryNative(h =>
            {
                if (NativeTS.GetTorrentFileList(h, out var list) != NativeTS.Ok) return null;
                try
                {
                    return new TorrentFileList(list);
                }
                finally
                {
                    NativeTS.DestroyTorrentFileList(ref list);
                }
            }, out var files);

            if (!ok || files == null) return Task.FromResult<TorrentFileList>(null);

            RefreshPieces(files.PieceCount);
            fileCol = files;
            return Task.FromResult(fileCol);
        }

        private unsafe void RefreshPieces(int pieceCount)
        {
            var have = new byte[pieceCount];
            TryNative(h =>
            {
                fixed (byte* ptr = have)
                {
                    return NativeTS.GetPieces(h, ptr, have.Length);
                }
            }, out _);
            _have = have;
        }

        public async Task EnsureDownloads(TorrentFileInfo[] files)
        {
            if (_disposed) return;
            var fileList = await GetFileListAsync();
            if (fileList == null) return;

            foreach (var file in fileList.Files)
            {
                var priority = files.Any(x => x.Index == file.Index) ? FileDownloadPriority.High : FileDownloadPriority.Low;
                if (!TryNative(h => NativeTS.SetFilePriority(h, file.Index, priority), out _)) return;
            }

            foreach (var file in files)
            {
                while (!_disposed && ReadFileProgress(file.Index) < file.FileSize)
                {
                    await Task.WhenAny(PieceArrived, Task.Delay(1000));
                }
            }
        }

        private long ReadFileProgress(int index)
        {
            return TryNative(h => NativeTS.GetFileProgress(h, index), out var progress) ? Math.Max(0, progress) : 0;
        }

        public Task<long> GetFileProgress(int index) => Task.FromResult(ReadFileProgress(index));

        public Task SaveResumeData()
        {
            _lastUpdate = DateTime.UtcNow;
            TryNative(NativeTS.SaveResumeData, out _);
            return Task.CompletedTask;
        }

        public void Dispose()
        {
            _ = DisposeAsync().AsTask();
        }

        /// <summary>Cancels the stream, saves resume data and detaches the torrent (off the caller's thread).</summary>
        public async ValueTask DisposeAsync()
        {
            if (!BeginDispose()) return;
            await Task.Run(() =>
            {
                lock (_gate)
                {
                    NativeTS.DetachTorrent(_session, _handler);
                }
            }).ConfigureAwait(false);
            GC.SuppressFinalize(this);
        }

        // the session frees every torrent itself when it is destroyed
        internal void OnSessionClosed()
        {
            BeginDispose();
        }

        private bool BeginDispose()
        {
            lock (_gate)
            {
                if (_disposed) return false;
                _disposed = true;
            }
            _owner.Forget(this);
            (stream as LibStream)?.Cancel();
            try { stream?.Dispose(); } catch { }
            SignalPieces();
            return _handler != 0;
        }

        public Task FocusPieces(int start, int end, int lastPiece)
        {
            TryNative(h => NativeTS.FocusPieces(h, start, end, lastPiece), out _);
            return Task.CompletedTask;
        }

        public async Task<Stream> InitializeStream(TorrentFileInfo info)
        {
            if (_disposed) return null;

            fileCol ??= await GetFileListAsync();
            if (fileCol == null) return null;

            var torrentFile = Path.Combine(_saveFolder, info.FilePath);
            if (ReadFileProgress(info.Index) >= info.FileSize && File.Exists(torrentFile))
            {
                stream = new FileStream(torrentFile, FileMode.Open, FileAccess.Read, FileShare.ReadWrite);
                return stream;
            }

            _fileLocked = true;
            var libstream = new LibStream(this, info, torrentFile, info.Offset, fileCol.PieceLength);
            stream = libstream;
            if (!await libstream.InitStream())
            {
                await libstream.DisposeAsync();
                return null;
            }
            return libstream;
        }

        /// <summary>Last status read by the session timer; null until the first tick.</summary>
        public TorrentStatus LastStatus { get; private set; }

        public bool IsPaused { get; private set; }

        public bool IsComplete => LastStatus is { State: TorrentState.Seeding or TorrentState.Finished };

        /// <summary>True while the session timer has something to do for this torrent: it is downloading or a player watches it.</summary>
        internal bool IsActive => !_disposed && (_onMeta != null || (!IsPaused && !IsComplete));

        internal TorrentStatus Update()
        {
            if (_disposed) return null;

            var status = ReadStatus();
            if (status == null) return null;
            LastStatus = status;

            if ((DateTime.UtcNow - _lastUpdate).TotalSeconds > 30)
            {
                _lastUpdate = DateTime.UtcNow;
                TryNative(NativeTS.SaveResumeData, out _);
            }

            if (IsComplete && _fileLocked)
            {
                _fileLocked = false;
                TryNative(NativeTS.ResetPieces, out _);
            }

            _onMeta?.Invoke(status);
            return status;
        }

        public void Pause()
        {
            if (TryNative(NativeTS.StopTorrent, out _)) IsPaused = true;
        }

        public void Resume()
        {
            if (!TryNative(NativeTS.StartTorrent, out _)) return;
            IsPaused = false;
            _owner.Wake();
        }

        /// <summary>True when the piece is on disk. Uses the cached bitmap first, then asks libtorrent.</summary>
        internal bool HasPieceNow(int piece)
        {
            var have = _have;
            if ((uint)piece < (uint)have.Length && Volatile.Read(ref have[piece]) == 1) return true;

            if (!TryNative(h => NativeTS.HasPiece(h, piece), out var result) || result != 1) return false;
            if ((uint)piece < (uint)have.Length) Volatile.Write(ref have[piece], 1);
            return true;
        }

        public Task<bool> HasPiece(int piece) => Task.FromResult(HasPieceNow(piece));

        /// <summary>Bytes available on disk contiguously from the torrent-wide offset <paramref name="start"/>, up to <paramref name="end"/>.</summary>
        internal long ContiguousBytes(long start, long end, int pieceLength)
        {
            var position = start;
            while (position < end)
            {
                var piece = (int)(position / pieceLength);
                if (!HasPieceNow(piece)) break;
                position = (long)(piece + 1) * pieceLength;
            }
            return Math.Max(0, Math.Min(position, end) - start);
        }

        /// <summary>Waits until the piece is on disk. False when cancelled, timed out or disposed.</summary>
        internal async Task<bool> WaitForPieceAsync(int piece, int timeoutMs, CancellationToken token)
        {
            var deadline = timeoutMs < 0 ? long.MaxValue : Environment.TickCount64 + timeoutMs;
            while (!_disposed && !token.IsCancellationRequested)
            {
                var signal = PieceArrived;
                if (HasPieceNow(piece)) return true;

                var remaining = deadline - Environment.TickCount64;
                if (remaining <= 0) return false;
                // the native signal is the wake-up; the 1 s cap only covers a dropped event
                var wait = (int)Math.Min(remaining, 1000);
                try
                {
                    await signal.WaitAsync(TimeSpan.FromMilliseconds(wait), token).ConfigureAwait(false);
                }
                catch (TimeoutException) { }
                catch (OperationCanceledException) { return false; }
            }
            return false;
        }

        internal bool WaitForPiece(int piece, CancellationToken token)
        {
            while (!_disposed && !token.IsCancellationRequested)
            {
                var signal = PieceArrived;
                if (HasPieceNow(piece)) return true;
                try
                {
                    signal.Wait(1000, token);
                }
                catch (OperationCanceledException) { return false; }
            }
            return false;
        }

        public void SubscribeMeta(Action<TorrentStatus> onMetaUpdata)
        {
            _onMeta = onMetaUpdata;
            if (onMetaUpdata != null) _owner.Wake();
        }

        public Task SetPriority(int index, FileDownloadPriority priority)
        {
            TryNative(h => NativeTS.SetFilePriority(h, index, priority), out _);
            return Task.CompletedTask;
        }

        public async Task<string> InitializeFile(TorrentFileInfo info, Action<double> onProgress = null)
        {
            if (_disposed) return null;

            fileCol ??= await GetFileListAsync();
            if (fileCol == null) return null;

            var torrentFile = Path.Combine(_saveFolder, info.FilePath);

            var currentProgress = ReadFileProgress(info.Index);
            if (currentProgress >= info.FileSize && File.Exists(torrentFile))
            {
                onProgress?.Invoke(100);
                return torrentFile;
            }

            int pieceStart = (int)(info.Offset / fileCol.PieceLength);
            int lastPiece = (int)((info.Offset + info.FileSize - 1) / fileCol.PieceLength);
            int pieceEnd = (int)((info.Offset + 20L * 1024 * 1024) / fileCol.PieceLength);
            pieceEnd = Math.Max(pieceStart + 1, pieceEnd);

            await SetPriority(info.Index, FileDownloadPriority.High);
            await FocusPieces(pieceStart, pieceEnd, lastPiece);

            double threshold = Math.Min(info.FileSize * 0.05, 30.0 * 1024 * 1024);

            while (!_disposed)
            {
                var progressState = ReadFileProgress(info.Index);
                onProgress?.Invoke(Math.Min(progressState * 100.0 / threshold, 99.9));
                if (progressState >= threshold && HasPieceNow(pieceStart))
                    break;
                await Task.WhenAny(PieceArrived, Task.Delay(1000));
            }

            onProgress?.Invoke(100);
            return _disposed ? null : torrentFile;
        }
    }
}
