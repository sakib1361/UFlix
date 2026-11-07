using MpvSharp;
using NativeRunner.TorrentHandler;
using UFlix.Cross.TorrentEngines;
using UFlix.Engine.Models;
using UFlix.Engine.Services;

namespace UFlix.Cross.Player
{
    public partial class MediaContainer
    {
        private const long MinLowBytes = 2L * 1024 * 1024;
        private const long MinResumeBytes = 6L * 1024 * 1024;

        private ITorrentEngine _engine;
        private Stream _stream;
        private CancellationTokenSource _runway;
        private volatile bool _appPaused;

        public int TotalTorrentFiles { get; private set; }

        private void TorrentDispose()
        {
            (_stream as LibStream)?.Cancel();
            _stream?.Dispose();
            _engine?.Dispose();
            _engine = null;
            _stream = null;
        }

        private async Task OpenTorrent(MediaItem media, TimeSpan? start)
        {
            _engine = new LibTorrentEngine();
            _stream = await _engine.OpenTorrent(media, Info);
            if (_stream == null || _disposed) return;
            TotalTorrentFiles = _engine.TotalFiles;
            await LoadTorrentStream(media, start);
        }

        public async Task<bool> OpenNextTorrent()
        {
            StopRunway();
            _player.Stop();
            _stream?.Dispose();
            _stream = null;
            CurrentMedia.Position = 0;
            if (CurrentMedia.ContentType == MediaContentType.Shows)
            {
                var custom = new CustomFile(CurrentMedia.Title);
                CurrentMedia.Title = custom.Title + $" S{custom.Season:00} E{custom.Episode + 1:00}";
            }

            _stream = await _engine.OpenNextTorrent(CurrentMedia);
            if (_stream == null || _disposed) return false;
            MediaTracks.Clear();
            _captionTracks.Clear();
            CurrentSubTrack = null;
            CurrentAudioTrack = null;
            Info.SubtitleDelay = 0;
            _player.SubtitleDelay = TimeSpan.Zero;
            await LoadTorrentStream(CurrentMedia, null);
            if (_disposed || !IsLoaded) return false;
            _player.Volume = _volume;
            Play();
            OnTracksChanged();
            SelectDefaultSubtitle();
            UpdateKeepAwake();
            MediaOpened?.Invoke(this, EventArgs.Empty);
            return true;
        }

        private async Task LoadTorrentStream(MediaItem media, TimeSpan? start)
        {
            var lib = _stream as LibStream;
            await _player.LoadAsync(_stream, media.Title, new LoadOptions { Start = start, Subtitles = LoadSubtitle(media, _engine?.CurrentFile) }, lib == null ? null : lib.Cancel);
            if (lib != null) StartRunway(lib);
        }

        /// <summary>
        /// App-driven buffering for a torrent: pause when less than ~1.4 s of contiguous data is left past the playhead and resume at ~4 s.
        /// Runs on the thread pool (never a stream callback, which must not call libmpv).
        /// </summary>
        private void StartRunway(LibStream stream)
        {
            StopRunway();
            var cts = _runway = new CancellationTokenSource();
            _ = Task.Run(async () =>
            {
                while (!cts.IsCancellationRequested)
                {
                    try { await Task.Delay(100, cts.Token); } catch { break; }
                    var player = _player;
                    if (player is not { IsLoaded: true } || stream.IsCancelled) continue;
                    var duration = player.Duration.TotalSeconds;
                    if (duration <= 0) continue;
                    var bytesPerSecond = stream.Length / duration;
                    var playhead = (long)(player.Position.TotalSeconds / duration * stream.Length);
                    var ahead = stream.ContiguousBytesFrom(playhead);
                    var toEnd = playhead + ahead >= stream.Length;
                    if (!_appPaused)
                    {
                        if (!player.IsPaused && !toEnd && ahead < Math.Max(MinLowBytes, bytesPerSecond * 1.4))
                        {
                            _appPaused = true;
                            player.Pause();
                            Ui(() => Raise(nameof(IsBuffering)));
                        }
                    }
                    else if (toEnd || ahead >= Math.Max(MinResumeBytes, bytesPerSecond * 4))
                    {
                        _appPaused = false;
                        player.Play();
                        Ui(() => Raise(nameof(IsBuffering)));
                    }
                }
            });
        }

        private void StopRunway()
        {
            _runway?.Cancel();
            _runway = null;
            _appPaused = false;
        }
    }
}
