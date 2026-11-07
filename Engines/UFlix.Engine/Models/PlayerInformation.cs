using System.ComponentModel;

namespace UFlix.Engine.Models
{
    public class PlayerInformation : INotifyPropertyChanged
    {
        private static string[] StatNames;
       

        public string Title { get; set; }
        public string CurrentTIme { get; set; }
        public string Duration { get; set; }
        public string BufferedDuration { get; set; }
        public string Status { get; set; }
        public bool CanPlay { get; set; }
        public bool AudioEnabled { get; set; } = true;
        public bool AudioOpened { get; set; }
        public double AudioDelay { get; set; }
        public double AudioVolume { get; set; }
        public double VolumeMax { get; set; }
        public bool AudioMute { get; set; }
        public string AudioCodec { get; set; }
        public string AudioChanelLayout { get; set; }
        public string AudioSampleFormat { get; set; }
        public double AudioSampleRate { get; set; }
        public bool SubtitleEnabled { get; set; } = true;
        public bool SubtitleOpened { get; set; }
        public double SubtitleDelay { get; set; }
        public string SubtileCodec { get; set; }
        public string SubtileText { get; set; }
        public bool VideoEnabled { get; set; }
        public bool VideoOpened { get; set; }
        public string VideoCodec { get; set; }
        public string VideoAspectRatio { get; set; }
        public string VideoCustomRatio { get; set; }
        public string VideoPixelFormat { get; set; }
        public double VideoWidth { get; set; }
        public double VideoHeight { get; set; }
        public double FPS { get; set; }
        public double TotalFrame { get; set; }
        public string VideoProcessor { get; set; }
        public bool ZeroCopy { get; set; }
        public double VideoFrameDisplayed { get; set; }
        public double FPSCurrent { get; set; }
        public double FramesDropped { get; set; }
        public string Bitrate { get; set; }
        public string VideoBitrate { get; set; }
        public double AudioBitrate { get; set; }
        public string DownloadRate { get; set; }
        public string UploadRate { get; set; }
        public string Downloaded { get; set; }
        public string Uploaded { get; set; }
        public string ETA { get; set; }
        public string Peers { get; set; }
        public string AudioDevice { get; set; }
        public bool VideoAccelaration { get; set; }

        public bool ShowDisplay { get; set; }
        public double BufferProgress { get; set; }
        public bool Opening { get; set; }

        public event PropertyChangedEventHandler PropertyChanged;

        public void Notify()
        {
            if (PropertyChanged == null) return;
            StatNames ??= GetType().GetProperties().Select(x => x.Name).ToArray();

            foreach (var name in StatNames)
            {
                PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
            }
        }
    }
}
