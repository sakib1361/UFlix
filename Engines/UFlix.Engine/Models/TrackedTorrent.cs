namespace UFlix.Engine.Models
{
    public enum TorrentKind
    {
        Play,
        Download,
    }

    /// <summary>A torrent kept in the session, persisted so unfinished ones are re-added at the next start.</summary>
    public class TrackedTorrent
    {
        public string Url { get; set; }
        public string Title { get; set; }
        public TorrentKind Kind { get; set; }
        /// <summary>0–100.</summary>
        public double Progress { get; set; }
        public string SavePath { get; set; }
        public DateTime Added { get; set; }
    }
}
