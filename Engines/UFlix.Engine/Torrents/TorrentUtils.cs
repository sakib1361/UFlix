using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;

namespace UFlix.Engine.Torrents
{
    public static class TorrentUtils
    {
        private static readonly HashSet<string> MovieExts = new(StringComparer.OrdinalIgnoreCase)
        {
            "mp4", "m4v", "m4e", "mkv", "mpg", "mpeg", "mpv", "mp4p", "mpe", "m1v", "m2ts", "m2p", "m2v", "movhd", "moov",
            "movie", "movx", "mjp", "mjpeg", "mjpg", "amv", "asf", "3gp", "ogm", "ogg", "vob", "ts", "rm", "3gp2", "3gpp",
            "3g2", "f4v", "f4a", "f4p", "f4b", "mts", "gifv", "avi", "mov", "flv", "wmv", "qt", "avchd", "swf", "cam", "nsv",
            "ram", "x264", "xvid", "wmx", "wvx", "wx", "video", "viv", "vivo", "vid", "dat", "bik", "bix", "dmf", "divx", "webm",
        };

        private static readonly string[] Units = ["KB", "MB", "GB", "TB"];

        public static bool IsMovie(string path) => MovieExts.Contains(Path.GetExtension(path).TrimStart('.'));

        public static List<string> GetMoviesSorted(IEnumerable<string> files)
        {
            var movies = files.Where(IsMovie).ToList();
            movies.Sort(NaturalCompare);
            return movies;
        }

        public static string BytesToReadableString(long bytes)
        {
            if (bytes < 1024) return $"{bytes} B";
            double value = bytes;
            var unit = -1;
            while (value >= 1024 && unit < Units.Length - 1)
            {
                value /= 1024;
                unit++;
            }
            return $"{value:n1} {Units[unit]}";
        }

        // Case-insensitive compare where digit runs compare by numeric value, so "ep2" sorts before "ep10".
        private static int NaturalCompare(string a, string b)
        {
            int i = 0, j = 0;
            while (i < a.Length && j < b.Length)
            {
                if (char.IsAsciiDigit(a[i]) && char.IsAsciiDigit(b[j]))
                {
                    int si = i, sj = j;
                    while (i < a.Length && char.IsAsciiDigit(a[i])) i++;
                    while (j < b.Length && char.IsAsciiDigit(b[j])) j++;
                    var na = a.AsSpan(si, i - si).TrimStart('0');
                    var nb = b.AsSpan(sj, j - sj).TrimStart('0');
                    if (na.Length != nb.Length) return na.Length - nb.Length;
                    var cmp = na.SequenceCompareTo(nb);
                    if (cmp != 0) return cmp;
                    continue;
                }
                var c = char.ToLowerInvariant(a[i]).CompareTo(char.ToLowerInvariant(b[j]));
                if (c != 0) return c;
                i++;
                j++;
            }
            var rest = (a.Length - i) - (b.Length - j);
            return rest != 0 ? rest : string.CompareOrdinal(a, b);
        }
    }
}
