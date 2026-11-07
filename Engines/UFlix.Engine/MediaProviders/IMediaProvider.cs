using UFlix.Engine.Models;

namespace UFlix.Engine.MediaProviders
{
    public interface IMediaProvider
    {
        bool CanDelete { get; }
        bool CanDiscover { get; }
        string Name { get; }
        MediaContentType ContentType { get; }
        Task<MediaItem[]> LoadMoreItems(int page);
    }
}
