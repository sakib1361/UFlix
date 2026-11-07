using System.ComponentModel;
using System.Diagnostics;
using System.Windows.Input;
using Microsoft.UI.Dispatching;
using MpvSharp;
using UFlix.Cross.Controls;
using UFlix.Cross.Services;
using UFlix.Engine.Models;
using TrackType = UFlix.Engine.Models.TrackType;

namespace UFlix.Cross.Player
{
    public enum PlaybackState
    {
        Paused,
        Playing,
    }

    /// <summary>
    /// Playback on MpvPlayer for every MediaType. mpv raises its events on its own thread; everything that reaches
    /// bindings is marshalled to the UI dispatcher captured at construction.
    /// </summary>
    public partial class MediaContainer : BaseMediaContainer, INotifyPropertyChanged
    {
        private readonly DispatcherQueue _ui = DispatcherQueue.GetForCurrentThread();
        private MpvPlayer _player;
        private IVideoSurface _surface;
        private double _volume = 100;
        private double _speed = 1;
        private readonly Dictionary<string, long> _captionTracks = [];
        private bool _disposed;
        private long _lastSecond = -1;
        private readonly Lock _logLock = new();

        public event EventHandler MediaOpened;
        public event EventHandler MediaFinished;
        public event EventHandler TracksUpdated;
        /// <summary>Raised on the UI thread when playback crosses a whole second (drives the timeline; no timer).</summary>
        public event EventHandler PositionTick;
        public event PropertyChangedEventHandler PropertyChanged;

        public PlayerInformation Info { get; } = new();
        public MediaItem CurrentMedia { get; private set; }
        public bool HasPlaylist { get; private set; }
        public PlaybackState State => _player == null || _player.IsPaused ? PlaybackState.Paused : PlaybackState.Playing;
        public bool IsBuffering => _appPaused || (_player?.IsBuffering ?? false);
        public TimeSpan Duration => _player?.Duration ?? TimeSpan.Zero;
        public double Position => _player?.Position.TotalSeconds ?? 0;
        public bool IsLoaded => _player?.IsLoaded ?? false;

        public double Volume
        {
            get => _volume;
            set
            {
                _volume = Math.Clamp(value, 0, 150);
                if (_player != null) _player.Volume = _volume;
                Raise(nameof(Volume));
            }
        }

        /// <summary>Playback speed; kept across files for the whole player session.</summary>
        public double Speed
        {
            get => _speed;
            set
            {
                _speed = Math.Clamp(value, 0.25, 4);
                _player?.SetProperty("speed", _speed.ToString(System.Globalization.CultureInfo.InvariantCulture));
                Raise(nameof(Speed));
                ShowOsd($"Speed {_speed:0.##}x");
            }
        }

        public ICommand PlayCommand => new RelayCommand(Play);
        public ICommand PauseCommand => new RelayCommand(Pause);

        internal void Init(IVideoSurface surface)
        {
            _surface = surface;
            var settings = PlayerSettings.Instance;
            var extra = new List<KeyValuePair<string, string>>(surface.MpvOptions())
            {
                new("sub-font-size", settings.SubtitleFontSize.ToString(System.Globalization.CultureInfo.InvariantCulture)),
                new("sub-color", settings.SubtitleColor),
                new("sub-border-size", "2.5"),
                new("slang", "en,eng"),
                new("cache", "yes"),
            };
            if (!string.IsNullOrWhiteSpace(settings.SubtitleFont)) extra.Add(new("sub-font", settings.SubtitleFont));
            // UFLIX_MPV_LOG=<file> writes mpv's verbose log there (diagnostics)
            var logFile = Environment.GetEnvironmentVariable("UFLIX_MPV_LOG");
            _player = MpvPlayer.Create(new MpvOptions { Extra = extra, LogLevel = logFile == null ? "warn" : "v" });
            Action<string> log = logFile == null ? line => Debug.WriteLine("[mpv] " + line) : line => { lock (_logLock) File.AppendAllText(logFile, line + Environment.NewLine); };
            _player.Log += log;
            log($"[uflix] surface: {surface.Kind}, options: {string.Join(" ", extra.Select(x => $"{x.Key}={x.Value}"))}");
            if (MpvPlayer.UsesSystemLibrary) log("[uflix] no bundled libmpv found, using the system copy");
            _player.PauseChanged += _ => Ui(() => { Raise(nameof(State)); UpdateKeepAwake(); });
            _player.BufferingChanged += _ => Ui(() => Raise(nameof(IsBuffering)));
            _player.DurationChanged += _ => Ui(() => Raise(nameof(Duration)));
            _player.PositionChanged += position =>
            {
                var second = (long)position.TotalSeconds;
                if (Interlocked.Exchange(ref _lastSecond, second) != second) Ui(() => PositionTick?.Invoke(this, EventArgs.Empty));
            };
            _player.TracksChanged += () => Ui(OnTracksChanged);
            _player.Ended += (reason, error) => Ui(() => OnEnded(reason, error));
            surface.Attach(_player);
        }

        private void Ui(Action action)
        {
            if (!_disposed) _ui.TryEnqueue(() => { if (!_disposed) action(); });
        }

        private void Raise(string name) => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));

        private void UpdateKeepAwake() => KeepAwake.Set(!_disposed && _player is { IsLoaded: true, IsPaused: false });

        public void Play()
        {
            _appPaused = false;
            _player?.Play();
            Raise(nameof(IsBuffering));
        }

        public void Pause() => _player?.Pause();

        public void TogglePause()
        {
            if (State == PlaybackState.Paused) Play();
            else Pause();
        }

        public void Seek(double seconds)
        {
            if (Info.Opening || _player is not { IsLoaded: true }) return;
            _player.Seek(TimeSpan.FromSeconds(Math.Clamp(seconds, 0, Math.Max(0, Duration.TotalSeconds - 1))));
        }

        /// <summary>Keyboard seek: feedback goes to mpv's OSD text so the control bar stays hidden.</summary>
        public void SeekBy(double seconds)
        {
            if (Info.Opening || _player is not { IsLoaded: true }) return;
            var target = Math.Clamp(Position + seconds, 0, Math.Max(0, Duration.TotalSeconds - 1));
            Seek(target);
            ShowOsd($"{(seconds > 0 ? "+" : "")}{seconds:0} s   {TimeSpan.FromSeconds(target):hh\\:mm\\:ss} / {Duration:hh\\:mm\\:ss}");
        }

        public void ChangeVolume(double delta)
        {
            Volume += delta;
            ShowOsd($"Volume {Volume:0}");
        }

        private void ShowOsd(string text) => _player?.Command("show-text", text, "1200");

        public void SetSubtitleMargin(bool controllerVisible) => _player?.SetProperty("sub-margin-y", controllerVisible ? "110" : "22");

        public void IncreaseSubOffset() => SetSubOffset(Info.SubtitleDelay + 0.5);

        public void DecreaseSubOffset() => SetSubOffset(Info.SubtitleDelay - 0.5);

        private void SetSubOffset(double seconds)
        {
            Info.SubtitleDelay = seconds;
            if (_player != null) _player.SubtitleDelay = TimeSpan.FromSeconds(seconds);
            ShowOsd($"Subtitle delay {seconds:+0.0;-0.0;0} s");
        }

        internal async Task Open(MediaItem media)
        {
            CurrentMedia = media;
            Info.Opening = true;
            Info.SubtitleDelay = 0;
            Info.Title = media.Title;
            Info.Notify();
            StartUI();

            StopRunway();
            _player.Stop();
            TorrentDispose();
            MediaTracks.Clear();
            _captionTracks.Clear();
            CurrentSubTrack = null;
            CurrentAudioTrack = null;
            HasPlaylist = false;
            _player.SubtitleDelay = TimeSpan.Zero;

            var live = IsLiveStream(media);
            var start = !live && media.Position > 0 ? TimeSpan.FromSeconds(media.Position) : (TimeSpan?)null;
            try
            {
                switch (media.MediaType)
                {
                    case MediaType.Youtube:
                        await OpenYoutube(media, start);
                        break;
                    case MediaType.Torrent:
                        await OpenTorrent(media, start);
                        break;
                    case MediaType.Local:
                        await _player.LoadAsync(media.Url, new LoadOptions { Title = media.Title, Start = start, Subtitles = LoadSubtitle(media) });
                        break;
                    default:
                        await _player.LoadAsync(media.Url, new LoadOptions { Title = media.Title, Start = start, Subtitles = live ? [] : await GetWebSubFiles(media.Url) });
                        break;
                }
            }
            catch (TaskCanceledException)
            {
            }
            catch (Exception ex)
            {
                NavigationHelper.Notify(ex.Message);
            }

            Info.Opening = false;
            Info.Notify();
            CloseUI();
            if (_disposed || !IsLoaded) return;

            if (start != null && Duration.TotalSeconds - start.Value.TotalSeconds < 10) _player.Seek(TimeSpan.Zero);
            _player.Volume = _volume;
            Play();
            OnTracksChanged();
            SelectDefaultSubtitle();
            Raise(nameof(Duration));
            UpdateKeepAwake();
            MediaOpened?.Invoke(this, EventArgs.Empty);
        }

        private void OnEnded(EndReason reason, int error)
        {
            if (Info.Opening) return;
            KeepAwake.Set(false);
            if (reason == EndReason.Error) NavigationHelper.Notify($"Playback failed ({error})");
            if (reason == EndReason.Eof) MediaFinished?.Invoke(this, EventArgs.Empty);
        }

        // ---- tracks (YouTube builds its own list of stream variants instead) ----

        private void OnTracksChanged()
        {
            if (_player == null || CurrentMedia == null || CurrentMedia.MediaType == MediaType.Youtube) return;
            MediaTracks.Clear();
            int n = 0;
            foreach (var t in _player.Tracks.Where(t => t.Type == MpvSharp.TrackType.Subtitle))
            {
                var track = new TrackItem(TrackType.Subtitle) { Id = t.Id.ToString(), Index = (int)t.Id, Codec = t.Language, Description = $"{++n}.{t.Description}" };
                track.SetAction(() => OnSubTrackSelected(track));
                MediaTracks.Add(track);
                if (t.IsSelected) CurrentSubTrack = track;
            }
            n = 0;
            foreach (var t in _player.Tracks.Where(t => t.Type == MpvSharp.TrackType.Audio))
            {
                var track = new TrackItem(TrackType.Audio) { Id = t.Id.ToString(), Index = (int)t.Id, Codec = t.Language, Description = $"{++n}.{t.Description}" };
                track.SetAction(() => OnAudioSelected(track));
                MediaTracks.Add(track);
                if (t.IsSelected) CurrentAudioTrack = track;
            }
            Info.SubtitleOpened = CurrentSubTrack != null;
            TracksUpdated?.Invoke(this, EventArgs.Empty);
        }

        private void SelectDefaultSubtitle()
        {
            if (CurrentSubTrack != null || CurrentMedia.MediaType == MediaType.Youtube) return;
            var subs = MediaTracks.Where(x => x.TrackType == TrackType.Subtitle).ToList();
            var first = subs.FirstOrDefault(x => x.Description.Contains("eng", StringComparison.InvariantCultureIgnoreCase)) ?? subs.FirstOrDefault();
            if (first != null) OnSubTrackSelected(first);
        }

        private void OnSubTrackSelected(TrackItem track)
        {
            CurrentSubTrack = CurrentSubTrack?.Id == track.Id ? null : track;
            _player.SelectTrack(MpvSharp.TrackType.Subtitle, CurrentSubTrack == null ? -1 : track.Index);
        }

        private void OnAudioSelected(TrackItem track)
        {
            CurrentAudioTrack = track;
            _player.SelectTrack(MpvSharp.TrackType.Audio, track.Index);
        }

        public async Task AddSubtitleFile(string fileName)
        {
            if (_player is not { IsLoaded: true }) return;
            _player.AddSubtitle(fileName);
            await Task.Delay(250);
            var added = _player.Tracks.LastOrDefault(t => t.Type == MpvSharp.TrackType.Subtitle && t.IsSelected);
            OnTracksChanged();
            if (added != null) CurrentSubTrack = MediaTracks.FirstOrDefault(x => x.TrackType == TrackType.Subtitle && x.Index == added.Id);
        }

        /// <summary>Shows a downloaded caption file (YouTube CC); a file added before is re-selected instead of added again.</summary>
        public async Task ShowCaption(string fileName)
        {
            if (_player is not { IsLoaded: true }) return;
            if (_captionTracks.TryGetValue(fileName, out var id))
            {
                _player.SelectTrack(MpvSharp.TrackType.Subtitle, id);
                return;
            }
            _player.AddSubtitle(fileName);
            await Task.Delay(250);
            var added = _player?.Tracks.LastOrDefault(t => t.Type == MpvSharp.TrackType.Subtitle && t.IsSelected);
            if (added != null) _captionTracks[fileName] = added.Id;
        }

        public void HideSubtitles() => _player?.SelectTrack(MpvSharp.TrackType.Subtitle, -1);

        // ---- lifetime ----

        /// <summary>Stops playback but keeps the player, e.g. while the next-episode dialog is open.</summary>
        internal void ClosePartials()
        {
            CloseUI();
            StopRunway();
            _player?.Stop();
            KeepAwake.Set(false);
            TorrentDispose();
        }

        internal async Task Close()
        {
            if (_disposed) return;
            CloseUI();
            StopRunway();
            _disposed = true;
            KeepAwake.Set(false);
            var player = _player;
            _player = null;
            // quit → join → destroy before the torrent stream goes away, so no stream_cb read can touch a disposed stream
            if (player != null) await player.DisposeAsync();
            TorrentDispose();
            _surface?.Release();
        }
    }
}
