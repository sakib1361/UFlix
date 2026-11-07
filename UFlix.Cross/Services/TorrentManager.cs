using Microsoft.UI.Dispatching;
using NativeRunner.TorrentHandler;
using System.Collections.ObjectModel;
using System.ComponentModel;
using UFlix.Engine.Models;
using UFlix.Cross.TorrentEngines;
using UFlix.Engine.Services;

namespace UFlix.Cross.Services
{
    /// <summary>
    /// Owns the app-wide libtorrent session and the tracker records of every torrent in it, played or downloaded.
    /// Unfinished torrents are re-added at start; a played torrent that is closed before it completes is dropped.
    /// </summary>
    public static class TorrentManager
    {
        private static DispatcherQueue _dispatcher;

        public static ObservableCollection<TorrentItem> Items { get; } = [];

        public static string TorrentFolder
        {
            get
            {
                var path = DBUtilities.Instance.TorrentDownloadPath;
                if (string.IsNullOrWhiteSpace(path)) path = AppPaths.DownloadFolder;
                return Path.Combine(path, "Torrents");
            }
        }

        /// <summary>Runs once the window is up: trims the torrent folder, then creates the session and restores unfinished torrents, off the UI thread.</summary>
        public static void Initialize(DispatcherQueue dispatcher)
        {
            _dispatcher = dispatcher;
            _ = Task.Run(() =>
            {
                Utilites.ClearFolder();
                try
                {
                    var session = LibSession.Instance;
                    session.StatusUpdated += OnStatusUpdated;
                    var records = DBUtilities.Instance.TrackedTorrents.Where(x => x.Progress < 100).ToArray();
                    foreach (var record in records)
                    {
                        var handler = session.AddTorrent(record.Url, record.SavePath ?? TorrentFolder);
                        if (!handler.IsDisposed) Ui(() => { if (Find(record.Url) == null) Items.Add(new TorrentItem(record, handler)); });
                    }
                    Ui(Save);
                }
                catch (Exception ex)
                {
                    NavigationHelper.Notify("Torrent session failed: " + ex.Message);
                }
            });
        }

        /// <summary>Returns the session's handler for the torrent, adding it as a played torrent when it isn't there yet. UI thread.</summary>
        public static LibTorrentHandler Open(string url, string title)
        {
            var item = Find(url);
            if (item != null && !item.Handler.IsDisposed)
            {
                if (item.IsPaused) item.TogglePause();
                return item.Handler;
            }
            if (item != null) Items.Remove(item);
            return Add(url, title, TorrentKind.Play)?.Handler;
        }

        /// <summary>Called when the player closes a torrent: an unfinished played torrent is removed from the session and the tracker. UI thread.</summary>
        public static void Release(string url)
        {
            var item = Find(url);
            if (item == null) return;
            item.Handler.SubscribeMeta(null);
            if (item.Kind == TorrentKind.Play && !item.Handler.IsComplete) Remove(item);
        }

        /// <summary>Downloads the whole torrent; a torrent already playing becomes a download. UI thread.</summary>
        public static void Download(string url, string title)
        {
            var item = Find(url);
            if (item != null && !item.Handler.IsDisposed)
            {
                item.Kind = TorrentKind.Download;
                if (item.IsPaused) item.TogglePause();
                Save();
            }
            else
            {
                if (item != null) Items.Remove(item);
                item = Add(url, title, TorrentKind.Download);
            }
            if (item != null) NavigationHelper.Notify("Downloading " + title);
        }

        public static void Remove(TorrentItem item)
        {
            Items.Remove(item);
            Save();
            _ = LibSession.Instance.StopTorrentAsync(item.Url);
        }

        private static TorrentItem Add(string url, string title, TorrentKind kind)
        {
            var record = new TrackedTorrent
            {
                Url = url,
                Title = title,
                Kind = kind,
                SavePath = TorrentFolder,
                Added = DateTime.UtcNow,
            };
            var handler = LibSession.Instance.AddTorrent(url, record.SavePath);
            if (handler.IsDisposed)
            {
                NavigationHelper.Notify("Torrent could not be added: " + handler.LastError);
                return null;
            }
            var item = new TorrentItem(record, handler);
            Items.Add(item);
            Save();
            return item;
        }

        private static TorrentItem Find(string url) => Items.FirstOrDefault(x => x.Url == url);

        private static void Save()
        {
            DBUtilities.Instance.TrackedTorrents = [.. Items.Select(x => x.Record)];
        }

        private static void OnStatusUpdated(LibTorrentHandler handler, TorrentStatus status)
        {
            Ui(() =>
            {
                var item = Items.FirstOrDefault(x => x.Handler == handler);
                if (item != null && item.Update(status)) Save();
            });
        }

        private static void Ui(Action action)
        {
            _dispatcher.TryEnqueue(() => action());
        }
    }

    public class TorrentItem : INotifyPropertyChanged
    {
        public TrackedTorrent Record { get; }
        public LibTorrentHandler Handler { get; }

        public string Url => Record.Url;
        public string Title => Record.Title;
        public double Progress => Record.Progress;
        public bool IsPaused => Handler.IsPaused;
        public string State { get; private set; } = "Starting";
        public string Details { get; private set; }

        public TorrentKind Kind
        {
            get => Record.Kind;
            set
            {
                Record.Kind = value;
                Raise(nameof(Kind));
            }
        }

        public event PropertyChangedEventHandler PropertyChanged;

        public TorrentItem(TrackedTorrent record, LibTorrentHandler handler)
        {
            Record = record;
            Handler = handler;
        }

        /// <summary>True when the whole-percent progress changed, so the tracker record is worth saving.</summary>
        internal bool Update(TorrentStatus status)
        {
            var old = Math.Floor(Record.Progress);
            Record.Progress = Math.Round(status.Progress * 100, 2);
            State = IsPaused ? "Paused" : status.State switch
            {
                TorrentState.DownloadingMetadata => "Fetching metadata",
                TorrentState.CheckingFiles or TorrentState.CheckingResume => "Checking",
                TorrentState.Seeding or TorrentState.Finished => "Completed",
                TorrentState.Errored => "Error",
                _ => "Downloading",
            };
            Details = $"{TorrentUtils.BytesToReadableString(status.BytesDownloaded)} / {TorrentUtils.BytesToReadableString(status.TotalSize)}"
                + $"  ↓ {TorrentUtils.BytesToReadableString(status.DownloadRate)}/s  ↑ {TorrentUtils.BytesToReadableString(status.UploadRate)}/s"
                + $"  Peers {status.SeedCount}/{status.PeerCount}";
            Raise(nameof(Progress));
            Raise(nameof(State));
            Raise(nameof(Details));
            return Math.Floor(Record.Progress) != old;
        }

        public void TogglePause()
        {
            if (Handler.IsPaused) Handler.Resume();
            else Handler.Pause();
            State = IsPaused ? "Paused" : "Downloading";
            Raise(nameof(IsPaused));
            Raise(nameof(State));
        }

        private void Raise(string name) => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
    }
}
