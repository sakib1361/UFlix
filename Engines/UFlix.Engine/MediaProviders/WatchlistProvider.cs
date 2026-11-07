using UFlix.Engine.Models;
using UFlix.Engine.Services;

namespace UFlix.Engine.MediaProviders
{
    public class WatchlistProvider : IMediaProvider
    {
        public string Name => "WatchList";
        public bool CanDelete => true;

        public bool CanDiscover => false;

        public MediaContentType ContentType => MediaContentType.Movie;


        public Task<MediaItem[]> LoadMoreItems(int page)
        {
            var service = new WatchlistService();
            var items = service.GetWatchlistMediasAsync();
            return Task.FromResult(items);
        }
    }
}
