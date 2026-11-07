using System.Web;
using TMDbLib.Objects.Movies;
using TMDbLib.Objects.Search;
using TMDbLib.Objects.TvShows;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using UFlix.Engine.TorrentSearchEngines.Base;

namespace UFlix.Engine.TorrentSearchEngines.Engines
{
    internal class StremioProvider : SearchEngineCommon
    {


        public override string Name { get; }
        protected override string FallbackUrl { get; }

        public StremioProvider(string name, string fallback)
        {
            Name = name;
            FallbackUrl = fallback;
        }

        internal override async Task<IEnumerable<TorrentInfo>> Search(Movie movie, bool forced)
        {
            var url = $"{FallbackUrl}/stream/movie/{movie.ExternalIds.ImdbId}.json";
            return await GetFiles(url, forced);
        }

        internal override async Task<IEnumerable<TorrentInfo>> Search(TvShow currentShow, TvSeasonEpisode episode, bool forced)
        {
            var url = $"{FallbackUrl}/stream/series/{currentShow.ExternalIds.ImdbId}:{episode.SeasonNumber}:{episode.EpisodeNumber}.json";
            return await GetFiles(url, forced);
        }

        private async Task<TorrentInfo[]> GetFiles(string url, bool forced)
        {
            var resp = await HttpUtilities.GetAsync<MediaFusionResponse>(url, forced);
            if (resp == null || resp.Streams == null) return [];

            var files = new List<TorrentInfo>();
            foreach (var stream in resp.Streams)
            {
                if (string.IsNullOrEmpty(stream.MainTitle)) continue;
                string magnet = $"magnet:?xt=urn:btih:{stream.InfoHash}";
                var data = new TorrentInfo()
                {
                    Quality = TorrentInfo.ParseQuality(stream.MainTitle),
                    Title = stream.MainTitle.Split("\n").FirstOrDefault()?.Trim(),
                    Size = TorrentInfo.ParseSize(stream.MainTitle),
                    Seeders = TorrentInfo.ParseSeeder(stream.MainTitle),
                    Leechers = "0",
                    Provider = Name,
                    Url = magnet,
                };
                files.Add(data);
            }
            return files.ToArray();
        }

        internal class MediaFusionResponse
        {
            public MediaFusionInfo[] Streams { get; set; }
        }

        internal class MediaFusionInfo
        {
            public string Name { get; set; }
            public string MainTitle => Title ??Description;
            public string Title { get; set; }
            public string Description { get; set; }
            public string InfoHash { get; set; }
        }
    }
}
