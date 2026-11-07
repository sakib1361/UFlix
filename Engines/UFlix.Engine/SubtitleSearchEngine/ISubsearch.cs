using UFlix.Engine.Models;
using UFlix.Engine.Services;

namespace UFlix.Engine.SubtitleSearchEngine
{
    public class SubtitleSearchHandler
    {
        public static readonly Dictionary<string, ISubsearch> SubtitleProviders = new()
        {
            {"OpenSubtitle", new OpenSubtitleParser() },
            {"SubDL", new SubdlParser() },
        };

        public Task<SubtitleFile[]> SearchSubtitle(MediaItem mediaItem)
        {
            // A provider saved in settings may since have been removed: fall back to OpenSubtitles.
            if (!SubtitleProviders.TryGetValue(DBUtilities.Instance.SubtitleProvider ?? "", out var currentProvider))
                currentProvider = SubtitleProviders["OpenSubtitle"];
            return currentProvider.SearchSubtitlesAsync(mediaItem);
        }
    }

    public interface ISubsearch
    {
        public Task<SubtitleFile[]> SearchSubtitlesAsync(MediaItem mediaItem);
    }
}
