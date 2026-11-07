using System.IO.Compression;
using System.Net.Http.Json;
using UFlix.Engine.Models;
using UFlix.Engine.Services;

namespace UFlix.Engine.SubtitleSearchEngine
{
    /// <summary>Searches subdl.com through its API (free key from a SubDL account, UFLIX_SUBDL_API_KEY in secrets.env).</summary>
    internal class SubdlParser : ISubsearch
    {
        private const string SearchUrl = "https://api.subdl.com/api/v1/subtitles?";
        private const string DownloadBase = "https://dl.subdl.com";
        private static readonly HttpClient client = new();

        public async Task<SubtitleFile[]> SearchSubtitlesAsync(MediaItem media)
        {
            var apiKey = AppSecrets.SubdlApiKey;
            if (string.IsNullOrWhiteSpace(apiKey)) return [];

            var isFile = File.Exists(media.Url);
            var parsed = new CustomFile(isFile ? Path.GetFileName(media.Url) : media.Title);
            var isShow = media.ContentType == MediaContentType.Shows || parsed.Episode != null;
            var param = new List<string>
            {
                $"api_key={Uri.EscapeDataString(apiKey)}",
                $"type={(isShow ? "tv" : "movie")}",
                "languages=EN",
                "subs_per_page=30",
            };
            if (media.Id is { } id && id.StartsWith("tt") && id.Length > 2 && id[2..].All(char.IsDigit))
                param.Add($"imdb_id={id}");
            else if (media.TmdbId > 0)
                param.Add($"tmdb_id={media.TmdbId}");
            else
            {
                param.Add($"film_name={Uri.EscapeDataString(parsed.Title ?? media.Title)}");
                if (!isShow && parsed.Year > 0) param.Add($"year={parsed.Year}");
            }
            if (isFile) param.Add($"file_name={Uri.EscapeDataString(Path.GetFileName(media.Url))}");
            if (isShow && parsed.Season != null) param.Add($"season_number={parsed.Season}");
            if (isShow && parsed.Episode != null) param.Add($"episode_number={parsed.Episode}");

            try
            {
                var resp = await client.GetFromJsonAsync<SearchResult>(SearchUrl + string.Join('&', param));
                if (resp?.status != true || resp.subtitles == null) return [];
                return [.. resp.subtitles.Where(x => !string.IsNullOrEmpty(x.url)).Select(x => new SubtitleFile(() => Download(x.url, parsed))
                {
                    Id = x.release_name,
                    Name = x.name ?? x.release_name,
                    Description = $"Lang: {x.lang}, By: {x.author}{(x.hi ? ", HI" : "")}{(x.full_season ? ", Full season" : "")}",
                })];
            }
            catch (Exception)
            {
                return [];
            }
        }

        // Each subtitle is a zip: extract it and pick the episode's file out of a season pack, else the first subtitle in it.
        private static async Task<string> Download(string url, CustomFile video)
        {
            var resp = await client.GetAsync(DownloadBase + url);
            if (!resp.IsSuccessStatusCode) return null;

            var extractedPath = Path.Combine(Path.GetTempPath(), AppConstants.AppName, "Subtitles", Path.GetRandomFileName());
            Directory.CreateDirectory(extractedPath);
            await using (var stream = await resp.Content.ReadAsStreamAsync())
            {
                using var zip = new ZipArchive(stream, ZipArchiveMode.Read);
                zip.ExtractToDirectory(extractedPath);
            }
            var files = Directory.EnumerateFiles(extractedPath, "*", SearchOption.AllDirectories).Where(SubtitleFinder.IsSubtitle).ToList();
            if (video.Episode != null)
            {
                var episode = files.FirstOrDefault(x => new CustomFile(Path.GetFileName(x)) is var c && c.Episode == video.Episode && (c.Season == null || video.Season == null || c.Season == video.Season));
                if (episode != null) return episode;
            }
            return files.FirstOrDefault();
        }

        private class SearchResult
        {
            public bool status { get; set; }
            public string error { get; set; }
            public SubdlSubtitle[] subtitles { get; set; }
        }

        private class SubdlSubtitle
        {
            public string release_name { get; set; }
            public string name { get; set; }
            public string lang { get; set; }
            public string author { get; set; }
            public string url { get; set; }
            public int? season { get; set; }
            public int? episode { get; set; }
            public bool hi { get; set; }
            public bool full_season { get; set; }
        }
    }
}
