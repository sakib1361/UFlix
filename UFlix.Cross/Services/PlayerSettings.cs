using UFlix.Engine.Services;

namespace UFlix.Cross.Services
{
    /// <summary>mpv options exposed in Settings, stored next to the rest of the settings in the LiteDB permanent store.</summary>
    public class PlayerSettings
    {
        public static PlayerSettings Instance { get; } = new();

        private static DBUtilities Db => DBUtilities.Instance;

        public double SubtitleFontSize
        {
            get
            {
                var size = Db.GetPermanent<double>("MpvSubFontSize");
                return size <= 0 ? 44 : size;
            }
            set => Db.AddUpdatePermanent(value, "MpvSubFontSize");
        }

        public string SubtitleColor
        {
            get => Db.GetPermanent<string>("MpvSubColor") ?? "#FFFFFFFF";
            set => Db.AddUpdatePermanent(value, "MpvSubColor");
        }

        public string SubtitleFont
        {
            get => Db.GetPermanent<string>("MpvSubFont") ?? "";
            set => Db.AddUpdatePermanent(value, "MpvSubFont");
        }

        /// <summary>YouTube: play the next up-next video when one ends.</summary>
        public bool YoutubeAutoplay
        {
            get => !Db.GetPermanent<bool>("YtNoAutoplay");
            set => Db.AddUpdatePermanent(!value, "YtNoAutoplay");
        }
    }
}
