using TMDbLib.Objects.General;
using UFlix.Engine.Models;
using UFlix.Engine.Services;

namespace UFlix.Engine.MediaProviders
{
    public abstract class TmdbProvider
    {
        public bool CanDelete => false;

        public bool CanDiscover => true;
        protected TmdbMediaService MediaService { get; } = new TmdbMediaService();
    }

    public class MovieTmdbProvider : TmdbProvider, IMediaProvider
    {
        public string Name => "Popular";

        public MediaContentType ContentType => MediaContentType.Movie;

        public async Task<MediaItem[]> LoadMoreItems(int page)
        {
            var items = await MediaService.GetRecentMovies(page);
            return items;
        }
    }

    public class ShowsTmdbProvider : TmdbProvider, IMediaProvider
    {
        public string Name => "Popular";

        public MediaContentType ContentType => MediaContentType.Shows;
        public async Task<MediaItem[]> LoadMoreItems(int page)
        {
            var items = await MediaService.GetRecentShows(page);
            return items;
        }
    }

    public class MovieGenreProvider(Genre genre) : TmdbProvider, IMediaProvider
    {
        private readonly Genre _genre = genre;

        public string Name => _genre.Name;

        public MediaContentType ContentType => MediaContentType.Movie;

        public async Task<MediaItem[]> LoadMoreItems(int page)
        {
            var items = await MediaService.GetMoviesByGenre(_genre, page);
            return items;
        }
    }

    public class ShowGenreProvider(Genre genre) : TmdbProvider, IMediaProvider
    {
        private readonly Genre _genre = genre;

        public string Name => _genre.Name;

        public MediaContentType ContentType => MediaContentType.Shows;

        public async Task<MediaItem[]> LoadMoreItems(int page)
        {
            var items = await MediaService.GetShowsByGenre(_genre, page);
            return items;
        }
    }
}
