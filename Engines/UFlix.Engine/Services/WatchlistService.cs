using UFlix.Engine.Models;

namespace UFlix.Engine.Services
{
    public class WatchlistService
    {
        readonly string id = "ContinueWatch";
        public MediaItem[] GetWatchlistMediasAsync()
        {
            return DBUtilities.Instance.GetCache<MediaItem[]>(id);
        }

        public void AddUpdate(MediaItem media)
        {
            if (media.MediaType == MediaType.Unknown) return;
            media.LastPlayed = DateTime.Now;
            var medias = GetWatchlistMediasAsync();
            var dataSet = medias.Where(x => (DateTime.Now - x.LastPlayed).TotalDays<10).ToList();
            var old = medias.FirstOrDefault(x => x.Id == media.Id);
            if (old != null)
            {
                _=dataSet.Remove(old);
            }
            dataSet.Insert(0, media);
            DBUtilities.Instance.AddUpdateCache(id, dataSet.ToArray(), 60);
        }

        public void Delete(MediaItem media)
        {
            var medias = GetWatchlistMediasAsync();
            var dataSet = medias.ToList();
            var old = medias.FirstOrDefault(x => x.Id == media.Id);
            if (old != null)
            {
                _=dataSet.Remove(old);
            }
            DBUtilities.Instance.AddUpdateCache(id, dataSet.ToArray(), 60);
        }
    }
}
