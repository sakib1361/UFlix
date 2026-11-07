using System.ComponentModel;
using System.Text.RegularExpressions;


namespace UFlix.Engine.Models
{
    public class TorrentInfo
    {
        public string Title { get; set; }
        //Can be torrent or mg
        public string Url { get; set; }
        public string Seeders { get; set; }
        public string Leechers { get; set; }
        public Quality Quality { get; set; }
        public MediaType MediaType { get; set; } = MediaType.Torrent;
        public string Provider { get; set; }
        public string ProviderImage { get; set; }

        public string Description => $"{Provider} Size:{Size} {Seeders}/{Leechers}";

        public string Size { get; set; }


        public static Quality ParseQuality(string info)
        {
            if (string.IsNullOrEmpty(info)) return Quality.Other;
            if (info.Contains("2160", StringComparison.InvariantCultureIgnoreCase) ||
                info.Contains("4k", StringComparison.InvariantCultureIgnoreCase))
                return Quality.Q2160p;
            if (info.Contains("1440", StringComparison.InvariantCultureIgnoreCase))
                return Quality.Q1440p;
            else if (info.Contains("1080", StringComparison.InvariantCultureIgnoreCase))
                return Quality.Q1080p;
            else if (info.Contains("720", StringComparison.InvariantCultureIgnoreCase))
                return Quality.Q720p;
            else
                return Quality.Other;
        }

        public static string ParseSize(string info)
        {
            if (string.IsNullOrEmpty(info)) return string.Empty;
            var match = Regex.Match(info, "💾\\s*([\\d.]+\\s*[KMGT]?B)");
            if (match.Success)
            {
                return match.Groups[1].Value;
            }
            return string.Empty;
        }

        public static string ParseSeeder(string info)
        {
            if(string.IsNullOrEmpty(info)) return string.Empty;
            var match = Regex.Match(info, "👤\\s*(\\d+)");
            if (match.Success)
            {
                string seeders = match.Groups[1].Value;
               return seeders;
            }
            return string.Empty;
        }
    }

    public class TorrentInfoCollection
    {
        public Quality Quality { get; set; }
        public TorrentInfo[] Torrents { get; set; }
    }
    public enum Quality
    {
        [Description("4k")]
        Q2160p,
        [Description("1440p")]
        Q1440p,
        [Description("1080p")]
        Q1080p,
        [Description("720p")]
        Q720p,
        [Description("Other")]
        Other,
    }
}
