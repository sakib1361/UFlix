using System.Net.Http.Json;
using System.Text.RegularExpressions;
using UFlix.Engine.Models;

namespace UFlix.Engine.Services
{
    public class TVService
    {
        private const string DefaultLink = "https://iptv-org.github.io/iptv/countries/bd.m3u";

        private static HttpClient _httpClient = new HttpClient() { Timeout = TimeSpan.FromSeconds(15) };
        public async Task<MediaItem[]> GetTvs()
        {
            string[] curLinks = [DefaultLink];
            var allLinks = DBUtilities.Instance.TvLinks;
            if (allLinks!=null)
            {
                var subLinks = allLinks.Split(',');
                if (subLinks.Length > 0)
                {
                    curLinks = subLinks;
                }
            }

            var all = new List<MediaItem>();
            foreach (var link in curLinks)
            {
                var dataResult = await GetAsync(link.Trim());
                if (dataResult == null) continue;

                var subData = Parse(dataResult);
                foreach (var (name, logo, address) in subData)
                {
                    var mItem = new MediaItem()
                    {
                        Image = logo,
                        Title = name,
                        Url = address,
                        MediaType = MediaType.Unknown,
                        ContentType = MediaContentType.Movie,
                    };
                    all.Add(mItem);
                }
            }

            return all.OrderBy(c=>c.Title).ToArray();

        }

        private static readonly Regex LogoRegex = new(@"tvg-logo=""(?<logo>[^""]*)""", RegexOptions.Compiled);

        private static readonly Regex NameRegex = new(@",(?<name>.+)$", RegexOptions.Compiled);

        public static List<(string name, string logo, string address)> Parse(string data)
        {
            var results = new List<(string, string, string)>();

            if (string.IsNullOrWhiteSpace(data))
                return results;

            var lines = data.Split(new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries);

            for (int i = 0; i < lines.Length - 1; i++)
            {
                var line = lines[i];

                if (!line.StartsWith("#EXTINF", StringComparison.OrdinalIgnoreCase))
                    continue;

                var nextLine = lines[i + 1].Trim();

                // Extract logo
                var logoMatch = LogoRegex.Match(line);
                string logo = logoMatch.Success ? logoMatch.Groups["logo"].Value : "";

                // Extract name
                var nameMatch = NameRegex.Match(line);
                string name = nameMatch.Success ? nameMatch.Groups["name"].Value.Trim() : "";

                // URL validation (basic)
                if (!nextLine.StartsWith("http", StringComparison.OrdinalIgnoreCase))
                    continue;

                results.Add((name, logo, nextLine));

                i++; // skip URL line
            }

            return results;
        }
        protected static async Task<string> GetAsync(string url)
        {
            var cached = DBUtilities.Instance.GetCache(url);
            if (cached != null && (DateTime.UtcNow < cached.ExpiresOn))
                return cached.Data;

            try
            {
                var data = await _httpClient.GetStringAsync(url);
                if (data != null)
                {
                    cached = new CachedStorage() { Data = data, Id = url, ExpiresOn = DateTime.UtcNow.AddDays(3)};
                    DBUtilities.Instance.AddUpdateCache(cached);
                }
                return cached?.Data;
            }
            catch (Exception ex)
            {
                Console.WriteLine($"Unable to get information from server {ex}");
                //probably re-throw here :)
            }

            return null;
        }
    }
}
