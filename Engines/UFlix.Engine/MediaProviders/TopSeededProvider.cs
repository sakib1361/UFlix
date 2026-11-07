using UFlix.Engine.Models;
using UFlix.Engine.Services;

namespace UFlix.Engine.MediaProviders
{
    internal abstract class TopSeededProvider
    {
        private const string CinemetaUrl = "https://v3-cinemeta.strem.io";
        private const int PageSize = 100;

        public bool CanDelete => false;
        public bool CanDiscover => true;

        protected static async Task<MediaItem[]> LoadTop(string type, MediaContentType contentType, int page)
        {
            var skip = page * PageSize;
            var url = $"{CinemetaUrl}/catalog/{type}/top/skip={skip}.json";
            var response = await HttpUtilities.GetAsync<CatalogResponse>(url, false);
            if (response?.Metas == null) return [];

            return response.Metas
                .Where(x => !string.IsNullOrEmpty(x.Id))
                .Select(x => new MediaItem()
                {
                    Title = x.Name,
                    Id = x.Id,
                    Image = x.Poster,
                    BackdropPath = x.Background,
                    ContentType = contentType,
                    Year = x.ReleaseInfo?.Split('-').FirstOrDefault(),
                })
                .ToArray();
        }

        private class CatalogResponse
        {
            public CatalogMeta[] Metas { get; set; }
        }

        private class CatalogMeta
        {
            public string Id { get; set; }
            public string Name { get; set; }
            public string Poster { get; set; }
            public string Background { get; set; }
            public string ReleaseInfo { get; set; }
        }
    }

    internal class TopSeededMovieProvider : TopSeededProvider, IMediaProvider
    {
        public string Name => "Top Seeded";

        public MediaContentType ContentType => MediaContentType.Movie;

        public Task<MediaItem[]> LoadMoreItems(int page) => LoadTop("movie", MediaContentType.Movie, page);
    }

    internal class TopSeededShowsProvider : TopSeededProvider, IMediaProvider
    {
        public string Name => "Top Seeded";

        public MediaContentType ContentType => MediaContentType.Shows;

        public Task<MediaItem[]> LoadMoreItems(int page) => LoadTop("series", MediaContentType.Shows, page);
    }
}
