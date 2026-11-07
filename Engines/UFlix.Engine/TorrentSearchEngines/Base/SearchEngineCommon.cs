using TMDbLib.Objects.Movies;
using TMDbLib.Objects.Search;
using TMDbLib.Objects.TvShows;
using UFlix.Engine.Models;
using UFlix.Engine.Services;

namespace UFlix.Engine.TorrentSearchEngines.Base
{
    public abstract class SearchEngineCommon
    {
        public abstract string Name { get; }
        protected abstract string FallbackUrl { get; }
        public string DefaultUri
        {
            get => GetSource();
            set => SetSource(value);
        }

        internal abstract Task<IEnumerable<TorrentInfo>> Search(Movie movie, bool forced);
        internal abstract Task<IEnumerable<TorrentInfo>> Search(TvShow currentShow, TvSeasonEpisode episode, bool forced);
        internal virtual Task<IEnumerable<TorrentInfo>> SearchFullSeason(TvShow currentShow, TvSeasonEpisode episode, bool forced)
        {
            return Search(currentShow, episode, forced);
        }

        private void SetSource(string value)
        {
            var allSource = DBUtilities.Instance.TorrentSources;
            var curSource = allSource.FirstOrDefault(x => x.Name == Name);
            if (curSource == null)
            {
                curSource = new TorrentSource() { Name = Name };
                var subList = allSource.ToList();
                subList.Add(curSource);
                allSource = subList.ToArray();
            }
            curSource.Url = value;
            DBUtilities.Instance.TorrentSources = allSource;
        }

        private string GetSource()
        {
            var allSource = DBUtilities.Instance.TorrentSources;
            var curSource = allSource.FirstOrDefault(x => x.Name == Name);
            if (curSource == null)
            {
                curSource = new TorrentSource() { Name = Name, Url = FallbackUrl };
                var subList = allSource.ToList();
                subList.Add(curSource);
                allSource = subList.ToArray();
                DBUtilities.Instance.TorrentSources = allSource;
            }
            return curSource.Url;
        }
    }
}
