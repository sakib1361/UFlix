using TMDbLib.Objects.Movies;
using TMDbLib.Objects.Search;
using TMDbLib.Objects.TvShows;
using UFlix.Engine.Models;
using UFlix.Engine.TorrentSearchEngines.Engines;

namespace UFlix.Engine.TorrentSearchEngines.Base
{
    public class TorrentSearchHandler
    {

        private const string Torrentio = "Stremio-Torrentio";
        private const string TorrentioUrl = "https://torrentio.strem.fun";

        private const string MediaFusion = "Stremio-TorrentsDB";
        private const string MediaFusionUrl = "https://torrentsdb.com/eyJwcm92aWRlcnMiOlsieXRzIiwiZXp0diIsIjEzMzd4IiwidG9ycmVudGNzdiIsInNrdG9ycmVudCIsIjF0YW1pbGJsYXN0ZXJzIiwiMXRhbWlsbXYiLCJ0aGVwaXJhdGViYXkiLCJraWNrYXNzdG9ycmVudHMiLCJleHRyZW1seW10b3JyZW50cyJdfQ==";

        private static readonly List<SearchEngineCommon> Handlers =
        [
            new PiratebaySearch(),
            new StremioProvider(Torrentio, TorrentioUrl),
            new StremioProvider(MediaFusion, MediaFusionUrl),
            new LocalSearch()
        ];

        public SearchEngineCommon Engine { get; set; }
        public List<SearchEngineCommon> Engines => Handlers;
        public async Task<TorrentInfo[]> SearchMovie(Movie movie, bool forced)
        {
            Engine ??= Handlers.FirstOrDefault();
            try
            {
                var res = await Engine.Search(movie, forced);
                return res.ToArray();
            }
            catch { }
            return Array.Empty<TorrentInfo>();
        }

        public async Task<TorrentInfo[]> SearchShow(TvShow currentShow, TvSeasonEpisode season, bool forced)
        {
            Engine ??= Handlers.FirstOrDefault();
            try
            {
                var res = await Engine.Search(currentShow, season, forced);
                return res.ToArray();
            }
            catch { }
            return Array.Empty<TorrentInfo>();
        }

        public async Task<TorrentInfo[]> SearchFullShow(TvShow currentShow, TvSeasonEpisode season, bool forced)
        {
            Engine ??= Handlers.FirstOrDefault();
            try
            {
                var res = await Engine.SearchFullSeason(currentShow, season, forced);
                return res.ToArray();
            }
            catch { }
            return Array.Empty<TorrentInfo>();
        }
    }
}
