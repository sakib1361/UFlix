using NativeRunner.TorrentHandler;
using System;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using UFlix.Cross.Services;

namespace UFlix.Cross.TorrentEngines
{
    public partial class LibTorrentEngine : ITorrentEngine
    {
        private PlayerInformation _info;
        private LibTorrentHandler _manager;
        private bool _disposed;
        private string _url;
        private readonly string torrentPath = TorrentManager.TorrentFolder;
        public int TotalFiles { get; private set; }
        public string CurrentFile { get; private set; }

        public void Dispose()
        {
            if (_url != null)
            {
                var url = _url;
                NavigationHelper.Execute(() => TorrentManager.Release(url));
            }
            _manager = null;
            _disposed = true;
        }

        public async Task<Stream> OpenNextTorrent(MediaItem item)
        {
            if (_disposed) return null;
            var fileCol = await _manager.GetFileListAsync();
            var subFiles = fileCol.Files.Select(x => Path.GetFileName(x.FilePath)).ToList();

            var sortedPaths = TorrentUtils.GetMoviesSorted(subFiles);
            if (sortedPaths.Count <2) return null;
            var selected = await NavigationHelper.GetSelectedMedia(item, sortedPaths);

            var index = subFiles.IndexOf(selected);
            if (index == -1) return null;
            var torFile = fileCol.Files[index];
            await DownloadSubtitles(fileCol, torFile);
            if (_disposed) return null;

            var stream = await _manager.InitializeStream(torFile);
            return stream;
        }

        public async Task<Stream> OpenTorrent(MediaItem item, PlayerInformation info)
        {
            _info = info;
            _url = item.Url;
            var opened = new TaskCompletionSource<LibTorrentHandler>();
            NavigationHelper.Execute(() => opened.SetResult(TorrentManager.Open(item.Url, item.Title)));
            _manager = await opened.Task;
            if (_manager == null || _manager.IsDisposed || _disposed) return null;
            SetMetaData(true);
            var meta = await _manager.GetTorrentStatus();

            if(meta == null || _disposed) return null;
            var fileCol = await _manager.GetFileListAsync();

            if (fileCol == null || _disposed) return null;


            var subFiles = fileCol.Files.Select(x => Path.GetFileName(x.FilePath)).ToList();

            var sortedPaths = TorrentUtils.GetMoviesSorted(subFiles);
            TotalFiles = sortedPaths.Count;


            var selected = await NavigationHelper.GetSelectedMedia(item, sortedPaths);

            var index = subFiles.IndexOf(selected);
            var torFile = fileCol.Files[index];
            item.FolderPath = Path.GetDirectoryName(Path.Combine(torrentPath, torFile.FilePath));
            await DownloadSubtitles(fileCol, torFile);
            if (_disposed) return null;
            var stream = await _manager.InitializeStream(torFile);
            return stream;
        }

        private async Task DownloadSubtitles(TorrentFileList fileCol, TorrentFileInfo video)
        {
            CurrentFile = Path.Combine(torrentPath, video.FilePath);
            var subPaths = SubtitleFinder.Select(CurrentFile, fileCol.Files.Select(x => Path.Combine(torrentPath, x.FilePath)));
            var subFiles = fileCol.Files.Where(x => subPaths.Contains(Path.Combine(torrentPath, x.FilePath))).Take(20).ToArray();
            if (subFiles.Length > 0) await _manager.EnsureDownloads(subFiles);
        }

        public void SetMetaData(bool isEnabled)
        {
            if (isEnabled) _manager.SubscribeMeta(OnMetaUpdata);
            else _manager.SubscribeMeta(null);
        }

        private void OnMetaUpdata(TorrentStatus meta)
        {
            double eta = 0;
            if (meta.DownloadRate > 0)
            {
                eta = (meta.TotalSize - meta.BytesDownloaded) / meta.DownloadRate;
            }
            _info.DownloadRate = TorrentUtils.BytesToReadableString(meta.DownloadRate) + "/s";
            _info.Downloaded = $"{TorrentUtils.BytesToReadableString(meta.BytesDownloaded)} / " +
                            $"{TorrentUtils.BytesToReadableString(meta.TotalSize)}";

            _info.BufferProgress = Math.Min(Math.Round(meta.BytesDownloaded*100.0/(50 * 1024 * 1024), 2), 100);

            _info.ETA           = TimeSpan.FromSeconds(eta).ToString(@"hh\:mm\:ss");
            _info.Peers = $"{meta.SeedCount}/{meta.PeerCount}";

            _info.UploadRate = TorrentUtils.BytesToReadableString(meta.UploadRate)+"/s";
            _info.Uploaded = TorrentUtils.BytesToReadableString(meta.BytesUploaded);
        }
    }
}
