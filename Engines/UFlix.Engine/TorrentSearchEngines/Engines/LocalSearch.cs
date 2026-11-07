using TMDbLib.Objects.Movies;
using TMDbLib.Objects.Search;
using TMDbLib.Objects.TvShows;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using UFlix.Engine.TorrentSearchEngines.Base;

namespace UFlix.Engine.TorrentSearchEngines.Engines
{
    internal class LocalSearch : SearchEngineCommon
    {
        public override string Name => GetType().Name;

        protected override string FallbackUrl => string.Empty;
        static string[] FileExts = new string[]
        {
            "mp4", "m4v", "m4e", "mkv", "mpg", "mpeg", "mpv", "mp4p", "mpe", "m1v",
            "m2ts", "m2p", "m2v", "movhd", "moov", "movie", "movx", "mjp", "mjpeg", "mjpg",
            "amv", "asf", "m4v", "3gp", "ogm", "ogg", "vob", "ts", "rm", "3gp",
            "3gp2", "3gpp", "3g2", "f4v", "f4a", "f4p", "f4b", "mts", "m2ts", "gifv",
            "avi", "mov", "flv", "wmv", "qt", "avchd", "swf", "cam", "nsv", "ram",
            "rm", "x264", "xvid", "wmx", "wvx", "wx", "video", "viv", "vivo", "vid",
            "dat", "bik", "bix", "dmf", "divx"
        };
        internal override async Task<IEnumerable<TorrentInfo>> Search(Movie movie, bool forced)
        {
            if (string.IsNullOrWhiteSpace(DefaultUri))
            {
                await Task.Delay(100);
                return [];
            }
            var filepaths = DefaultUri.Split(',');
            var all = new List<TorrentInfo>();
            var title = new CustomFile(movie.Title.Replace(":", ""));
            foreach (var dir in filepaths)
            {
                foreach (var folder in Directory.EnumerateDirectories(dir))
                {
                    var name = Path.GetFileName(folder);
                    if (name.StartsWith(title.Title, StringComparison.InvariantCultureIgnoreCase))
                    {
                        var files = Directory.GetFiles(folder);
                        var largestFile = files.OrderByDescending(f => new FileInfo(f).Length).FirstOrDefault();

                        if (largestFile != null)
                        {
                            // Do something with the largest file
                            var tcFile = new CustomFile(largestFile);
                            all.Add(new TorrentInfo()
                            {
                                Quality = Quality.Other,
                                Title = folder,
                                Size = Utilites.MeasureFileSize(new FileInfo(largestFile).Length),
                                Url = largestFile,
                                MediaType = MediaType.Local,
                                Leechers = "1",
                                Seeders = "1",
                            });
                        }
                    }
                }
            }

            return all.ToArray();
        }

        internal override async Task<IEnumerable<TorrentInfo>> Search(TvShow currentShow, TvSeasonEpisode episode, bool forced)
        {
            if (string.IsNullOrWhiteSpace(DefaultUri))
            {
                await Task.Delay(100);
                return [];
            }
            var filepaths = DefaultUri.Split(',', StringSplitOptions.TrimEntries);
            var all = new List<TorrentInfo>();
            var title = new CustomFile(currentShow.Name.Replace(":", ""));
            foreach (var dir in filepaths)
            {
                foreach (var folder in Directory.EnumerateDirectories(dir))
                {
                    var name = Path.GetFileName(folder);
                    if (name.StartsWith(title.Name, StringComparison.InvariantCultureIgnoreCase) == false)
                    {
                        continue;
                    }
                    foreach (var file in Directory.EnumerateFiles(folder, "*.*", SearchOption.AllDirectories))
                    {
                        var ext = Path.GetExtension(file).Replace(".", "");
                        if (FileExts.Contains(ext) == false) continue;
                        var customFile = new CustomFile(file);
                        if (customFile.Episode == episode.EpisodeNumber && customFile.Season == episode.SeasonNumber)
                        {
                            all.Add(new TorrentInfo()
                            {
                                Quality = Quality.Other,
                                Title = Path.GetFileName(file),
                                Size = Utilites.MeasureFileSize(new FileInfo(file).Length),
                                Url = file,
                                MediaType = MediaType.Local,
                                Leechers = "1",
                                Seeders = "1",
                            });
                        }
                    }
                }
            }
            return all.ToArray();
        }
    }
}
