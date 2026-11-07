using YoutubeExplode.Videos.Streams;

namespace UFlix.Engine.Models
{
    public class MediaItem
    {
        public string Id { get; set; }
        public int TmdbId { get; set; }
        public string Url { get; set; }
        public string Title { get; set; }
        public string Description { get; set; }
        public string LikeCount { get; set; }
        public string ViewCount { get; set; }
        public TimeSpan? Duration { get; set; }
        public string Image { get; set; }
        public string Author { get; set; }
        public DateTime UploadDate { get; set; }
        public string Year { get; set; }
        public MediaType MediaType { get; set; } = MediaType.Torrent;
        public MediaContentType ContentType { get; set; }
        public int PlayListIndex { get; set; }
        public long Position { get; set; }
        public bool HasPlaylist => MediaType == MediaType.PlayList;
        public double Progress => Position/Duration?.TotalMilliseconds?? 10000;

        public double Rating { get; set; }
        public double CustomRating => Rating/2;

        public string FolderPath { get; set; }
        public DateTime LastPlayed { get; set; }
        public string Tag { get; set; }
        public string BackdropPath { get; set; }
        public long FileSize { get; set; }
        public string Fallback { get; set; }

        private object ExtraData;
        public void SetExtradata(object extra)
        {
            ExtraData = extra;
        }
        public object GetExtraData()
        {
            return ExtraData;
        }

        public MediaItem Clone()
        {
            return (MediaItem)MemberwiseClone();
        }
    }

    public enum MediaType
    {
        Torrent,
        Local,
        Youtube,
        PlayList,
        Web,
        Unknown,
    }

    public enum MediaContentType
    {
        Movie,
        Shows,
        Youtube
    }

    public class TrackItem
    {
        public TrackItem(TrackType track)
        {
            TrackType = track;
        }
        public IStreamInfo Stream { get; set; }
        public string Address { get; set; }
        public int Height { get; set; }
        public double Width { get; set; }
        public TrackType TrackType { get; }
        public string Codec { get; set; }
        public string Id { get; set; }
        public int Index { get; set; }
        public long BitRate { get; set; }
        public float FPS { get; set; }
        public string Description { get; set; }
        public bool Enabled { get; set; }
        public bool HasAction => Action!=null;

        public void SetAction(Action action)
        {
            Action = action;
        }

        public void Execute()
        {
            Action?.Invoke();
        }

        private Action Action;
    }

    public enum TrackType
    {
        Video,
        Audio,
        Both,
        Subtitle,
    }
}
