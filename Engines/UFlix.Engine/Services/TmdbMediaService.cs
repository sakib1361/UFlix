using System.Runtime.CompilerServices;
using TMDbLib.Client;
using TMDbLib.Objects.General;
using TMDbLib.Objects.Movies;
using TMDbLib.Objects.Search;
using TMDbLib.Objects.Trending;
using TMDbLib.Objects.TvShows;
using UFlix.Engine.Models;

namespace UFlix.Engine.Services
{
    public class TmdbMediaService
    {
        public const string ImagePath400 = "https://image.tmdb.org/t/p/w400/";
        public const string ImagePath300 = "https://image.tmdb.org/t/p/w300/";
        public const string ImagePath200 = "https://image.tmdb.org/t/p/w200/";

        public readonly static TmdbMediaService Instance = new TmdbMediaService();
        private static readonly string TMDBKey = AppSecrets.TmdbApiKey;
        private static readonly TMDbClient client;
        private readonly string ServiceName = "MediaService";
        static TmdbMediaService()
        {
            client = new TMDbClient(TMDBKey)
            {
                DefaultLanguage = "en-US",
            };
        }

        public async Task<Genre[]> GetMovieGenre()
        {
            var key = BuildKey(GetName());
            var genres = DBUtilities.Instance.GetCache<Genre[]>(key);
            if (genres.Length == 0)
            {
                try
                {
                    var genreData = await client.GetMovieGenresAsync();
                    genres = genreData.ToArray();
                    DBUtilities.Instance.AddUpdateCache(key, genres, 360);
                }
                catch { }
            }
            return genres;
        }

        public async Task<Genre[]> GetShowGenre()
        {
            var key = BuildKey(GetName());
            var genres = DBUtilities.Instance.GetCache<Genre[]>(key);
            if (genres.Length == 0)
            {
                try
                {
                    var genreData = await client.GetTvGenresAsync();
                    genres = [.. genreData];
                    DBUtilities.Instance.AddUpdateCache(key, genres, 360);
                }
                catch { }
            }
            return genres;
        }

        private static MediaItem[] ConvertMedia(IEnumerable<SearchMovie> movies)
        {
            return movies.Select(x => new MediaItem()
            {
                BackdropPath = ImagePath400 + x.BackdropPath,
                MediaType = Models.MediaType.Local,
                ContentType = MediaContentType.Movie,
                Image = ImagePath300 + x.PosterPath,
                Title = x.Title,
                UploadDate = x.ReleaseDate?.Date??DateTime.UtcNow,
                Year = x.ReleaseDate?.Year.ToString(),
                TmdbId = x.Id,
            }).ToArray();
        }

        private static MediaItem[] ConvertMedia(IEnumerable<SearchTv> movies)
        {
            return movies.Select(x => new MediaItem()
            {
                BackdropPath = ImagePath400 + x.BackdropPath,
                MediaType = Models.MediaType.Local,
                ContentType = MediaContentType.Shows,
                Image = ImagePath300 + x.PosterPath,
                Title = x.Name,
                UploadDate = x.FirstAirDate?.Date??DateTime.UtcNow,
                Year = x.FirstAirDate?.Year.ToString(),
                TmdbId = x.Id,
            }).ToArray();
        }


        public async Task<MediaItem[]> GetMoviesByGenre(Genre genre, int page)
        {
            var key = BuildKey(GetName(), genre.Name, page);
            var data = DBUtilities.Instance.GetCache<MediaItem[]>(key);
            if (data.Length == 0 || data[0].Image == null)
            {
                try
                {
                    var query = client.DiscoverMoviesAsync().IncludeWithAllOfGenre(new[] { genre });
                    var res = await query.Query(page+1);
                    data = ConvertMedia(res.Results);
                    DBUtilities.Instance.AddUpdateCache(key, data);
                }
                catch { }
            }
            return data;
        }

        public async Task<MediaItem[]> GetShowsByGenre(Genre genre, int page)
        {
            var key = BuildKey(GetName(), genre.Name, page);
            var data = DBUtilities.Instance.GetCache<MediaItem[]>(key);
            if (data.Length == 0 || data[0].Image == null)
            {
                try
                {
                    var query = client.DiscoverTvShowsAsync().WhereGenresInclude(new[] { genre });
                    var res = await query.Query(page+1);
                    data = ConvertMedia(res.Results);
                    DBUtilities.Instance.AddUpdateCache(key, data);
                }
                catch { }
            }
            return data;
        }

        public async Task<MediaItem[]> GetRecentMovies(int page)
        {
            var key = BuildKey(GetName(), page);
            var data = DBUtilities.Instance.GetCache<MediaItem[]>(key);
            if (data.Length == 0 || data[0].Image == null)
            {
                try
                {
                    var res = await client.GetTrendingMoviesAsync(TimeWindow.Week, page +1);
                    data = ConvertMedia(res.Results);
                    DBUtilities.Instance.AddUpdateCache(key, data);
                }
                catch { }
            }
            return data;
        }

        public async Task<MediaItem[]> GetRecentShows(int page)
        {
            var key = BuildKey(GetName(), page);
            var data = DBUtilities.Instance.GetCache<MediaItem[]>(key);
            if (data.Length == 0 || data[0].Image == null)
            {
                try
                {
                    var res = await client.GetTrendingTvAsync(TimeWindow.Week, page + 1);
                    data = ConvertMedia(res.Results);
                    DBUtilities.Instance.AddUpdateCache(key, data);
                }
                catch { }
            }
            return data;
        }

        public async Task<Movie> GetMovieInfo(MediaItem current)
        {
            if(current.Id == null && current.TmdbId == 0)
            {
                var cFile = new CustomFile(current.Title);
                var searchResults = await SearchMovie(cFile.Title, cFile.Year);
                if(searchResults.Length == 0)
                    return null;    
                var newRes = searchResults.FirstOrDefault();
                current.Id = newRes.Id;
                current.TmdbId = newRes.TmdbId;
                current.Title = newRes.Title;
                current.Image = newRes.Image;
                current.BackdropPath = newRes.BackdropPath;
                current.Description = newRes.Description;
                current.Year = newRes.Year;
            }
            var key = current.TmdbId == 0 ? BuildKey(GetName(), current.Id) : BuildKey(GetName(), current.TmdbId);
            var data = DBUtilities.Instance.GetCache<Movie>(key);
            if (data == null)
            {
                try
                {
                    if (current.TmdbId == 0)
                        data = await client.GetMovieAsync(current.Id, MovieMethods.ExternalIds|MovieMethods.Credits);
                    else
                        data = await client.GetMovieAsync(current.TmdbId, MovieMethods.ExternalIds|MovieMethods.Credits);
                    if (data != null && data.Credits == null)
                    {
                        var cr = await client.GetMovieCreditsAsync(data.Id);
                        data.Credits = cr;
                    }
                    DBUtilities.Instance.AddUpdateCache(key, data, 360);
                }
                catch { }
            }
            return data;
        }

        public async Task<TvShow> GetShowInfo(MediaItem search, bool force = false)
        {
            var key = BuildKey(GetName(), search.TmdbId);
            var data = DBUtilities.Instance.GetCache<TvShow>(key);
            if (data == null || force)
            {
                try
                {
                    data = await client.GetTvShowAsync(search.TmdbId, TvShowMethods.ExternalIds|TvShowMethods.Credits);
                    DBUtilities.Instance.AddUpdateCache(key, data, 60);
                }
                catch { }
            }
            if (data != null && data.Seasons != null)
            {
                var special = data.Seasons.FirstOrDefault(x => x.Name == "Specials");
                if (special != null) data.Seasons.Remove(special);
            }
            return data;
        }

        public async Task<TvSeason> GetSeasonInfo(TvShow show, int season, bool force = false)
        {
            var key = BuildKey(GetName(), show.Id, season);
            var data = DBUtilities.Instance.GetCache<TvSeason>(key);
            if (data == null || force)
            {
                try
                {
                    data = await client.GetTvSeasonAsync(show.Id, season);
                    DBUtilities.Instance.AddUpdateCache(key, data, 60);
                }
                catch { }
            }
            else
            {
                try
                {
                    var newEp = data.Episodes.Any(x => x.AirDate.HasValue && x.AirDate.Value >= DateTime.UtcNow && string.IsNullOrEmpty(x.Overview));
                    if (newEp)
                    {
                        data = await client.GetTvSeasonAsync(show.Id, season);
                        DBUtilities.Instance.AddUpdateCache(key, data, 60);
                    }
                }
                catch { }
            }
            return data;
        }

        public async Task<MediaItem[]> SearchMovie(string title, int expyear = 0)
        {
            var key = BuildKey(GetName(), title);
            var data = DBUtilities.Instance.GetCache<MediaItem[]>(key);
            if (data.Length == 0)
            {
                try
                {
                    var resp = await client.SearchMovieAsync(title, includeAdult: true, year : expyear);
                    data = ConvertMedia(resp.Results);
                    DBUtilities.Instance.AddUpdateCache(key, data);
                }
                catch { }
            }
            return data;
        }
        public async Task<MediaItem[]> SearchShow(string title)
        {
            var key = BuildKey(GetName(), title);
            var data = DBUtilities.Instance.GetCache<MediaItem[]>(key);
            if (data.Length == 0)
            {
                try
                {
                    var resp = await client.SearchTvShowAsync(title, includeAdult: true);
                    data = ConvertMedia(resp.Results);
                    DBUtilities.Instance.AddUpdateCache(key, data);
                }
                catch { }
            }
            return data;
        }

        private string BuildKey(string name, params object[] args)
        {
            return ServiceName + "_" + name + "_" + string.Join(',', args);
        }

        private static string GetName([CallerMemberName] string name = "")
        {
            return name;
        }
    }
}
