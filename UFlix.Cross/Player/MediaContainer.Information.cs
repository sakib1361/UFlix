using System.Globalization;

namespace UFlix.Cross.Player
{
    public partial class MediaContainer
    {
        private const string TimeFormat = @"hh\:mm\:ss";
        private bool _infoActive;

        /// <summary>Refreshes <see cref="Info"/> once a second while the info panel is open or the media is opening (torrent status).</summary>
        internal async void StartUI()
        {
            if (_infoActive) return;
            _infoActive = true;
            bool hasMeta = false;
            while ((_infoActive || Info.Opening) && !_disposed)
            {
                if (!hasMeta && _engine != null)
                {
                    hasMeta = true;
                    _engine.SetMetaData(true);
                }
                UpdatePlayerInfo();
                Info.Notify();
                await Task.Delay(1000);
            }
            _infoActive = false;
        }

        internal void CloseUI()
        {
            if (Info.Opening) return;
            _engine?.SetMetaData(false);
            _infoActive = false;
        }

        private void UpdatePlayerInfo()
        {
            var p = _player;
            if (p == null || CurrentMedia == null) return;
            string S(string name) => p.GetProperty(name);
            double D(string name) => double.TryParse(S(name), NumberStyles.Float, CultureInfo.InvariantCulture, out var v) ? v : 0;

            Info.Title = CurrentMedia.Title;
            Info.CurrentTIme = p.Position.ToString(TimeFormat);
            Info.Duration = p.Duration.ToString(TimeFormat);
            Info.BufferedDuration = $"{D("demuxer-cache-duration"):0.0} s";
            Info.Status = IsBuffering ? "Buffering" : State.ToString();
            Info.CanPlay = p.IsLoaded;
            Info.AudioVolume = _volume;
            Info.VolumeMax = 150;
            Info.AudioMute = _volume == 0;
            Info.AudioDevice = S("current-ao");
            Info.AudioOpened = !string.IsNullOrEmpty(S("audio-codec-name"));
            Info.AudioCodec = S("audio-codec-name");
            Info.AudioBitrate = D("audio-bitrate");
            Info.AudioChanelLayout = S("audio-params/hr-channels");
            Info.AudioSampleFormat = S("audio-params/format");
            Info.AudioSampleRate = D("audio-params/samplerate");
            Info.VideoOpened = !string.IsNullOrEmpty(S("video-codec"));
            Info.VideoCodec = S("video-format");
            Info.VideoWidth = D("video-params/w");
            Info.VideoHeight = D("video-params/h");
            Info.VideoPixelFormat = S("video-params/hw-pixelformat") ?? S("video-params/pixelformat");
            Info.VideoAspectRatio = D("video-params/aspect").ToString("N3");
            Info.FPS = Math.Round(D("container-fps"), 2);
            Info.VideoProcessor = S("hwdec-current") is { Length: > 0 } hw && hw != "no" ? $"HW ({hw})" : "SW";
            Info.VideoAccelaration = Info.VideoProcessor != "SW";
            Info.VideoBitrate = $"{D("video-bitrate") / 1000000.0:N2} Mb/s";
            Info.Bitrate = $"{(D("video-bitrate") + D("audio-bitrate")) / 1000000.0:N2} Mb/s";
            Info.FramesDropped = D("frame-drop-count");
            Info.SubtitleOpened = CurrentSubTrack != null;
        }
    }
}
