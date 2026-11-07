using UFlix.Engine.MediaProviders;

namespace UFlix.Engine.Models
{
    public class AppConstants
    {
        public static string DBFolder { get; set; }
        /// <summary>Names the data folder (AppPaths). Set it at startup, before anything touches AppPaths or DBUtilities.</summary>
        public static string AppName { get; set; } = "UFlix";
        public static string[] Trackers { get; } = new[]
        {
            "udp://tracker.opentrackr.org:1337",
            "udp://tracker.tiny-vps.com:6969",
            "udp://tracker.openbittorrent.com:1337",
            "udp://tracker.leechers-paradise.org:6969",
            "udp://p4p.arenabg.com:1337",
            "udp://tracker.internetwarriors.net:1337",
            "udp://9.rarbg.to:2720",
            "udp://9.rarbg.me:2720",
            "udp://exodus.desync.com:6969",
            "udp://tracker.cyberia.is:6969",
            "udp://tracker.torrent.eu.org:451",
            "udp://open.stealth.si:80",
            "udp://tracker.moeking.me:6969",
            "udp://tracker.zerobytes.xyz:1337",
            "udp://explodie.org:6969",
            "udp://retracker.lanta-net.ru:2710",
            "udp://opentracker.i2p.rocks:6969",
            //"wss://tracker.openwebtorrent.com"
        };
        public static List<IMediaProvider> Providers { get; } =
        [

        ];
        public static string YtFile { get; set; }

        public static void Init()
        {
            if (Providers.Count == 0)
            {
                Providers.AddRange([
                    new MovieTmdbProvider(),
                    new ShowsTmdbProvider(),
                    new TopSeededMovieProvider(),
                    new TopSeededShowsProvider()]);
            }
        }
    }
}
