using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using UFlix.Engine.Models;

namespace UFlix.Engine.Services
{
    /// <summary>
    /// Finds the subtitle files that belong to a video: next to it, in a Subs/Sub/Subtitles folder beside it,
    /// or in a folder inside that one named after the video or with the same SxxEyy (RARBG style "Subs/&lt;video name&gt;/2_English.srt").
    /// </summary>
    public static class SubtitleFinder
    {
        private static readonly string[] SubExtensions = [".srt", ".ass", ".ssa", ".vtt"];
        private static readonly string[] SubFolders = ["subs", "sub", "subtitles", "subtitle"];

        public static bool IsSubtitle(string path) =>
            SubExtensions.Contains(Path.GetExtension(path), StringComparer.OrdinalIgnoreCase);

        public static string[] Find(string videoPath, string episodeHint = null)
        {
            var folder = Path.GetDirectoryName(videoPath);
            if (string.IsNullOrEmpty(folder) || !Directory.Exists(folder)) return [];
            var candidates = new List<string>();
            try
            {
                candidates.AddRange(Directory.EnumerateFiles(folder));
                foreach (var subDir in Directory.EnumerateDirectories(folder).Where(IsSubFolder))
                {
                    candidates.AddRange(Directory.EnumerateFiles(subDir));
                    foreach (var dir in Directory.EnumerateDirectories(subDir))
                        candidates.AddRange(Directory.EnumerateFiles(dir));
                }
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
            return Select(videoPath, candidates, episodeHint);
        }

        /// <summary>Picks the subtitles of <paramref name="videoPath"/> out of <paramref name="paths"/> (full paths, e.g. a torrent's file list).</summary>
        public static string[] Select(string videoPath, IEnumerable<string> paths, string episodeHint = null)
        {
            var folder = Normalize(Path.GetDirectoryName(videoPath));
            var videoName = Path.GetFileNameWithoutExtension(videoPath);
            var video = new CustomFile(videoName);
            if (video.Episode == null && episodeHint != null) video = new CustomFile(episodeHint);

            var beside = new List<string>();
            var inSubs = new List<string>();
            var inEpisode = new List<string>();
            foreach (var path in paths.Where(IsSubtitle))
            {
                var dir = Normalize(Path.GetDirectoryName(path));
                if (dir == folder) beside.Add(path);
                else if (Normalize(Path.GetDirectoryName(dir)) == folder && IsSubFolder(dir)) inSubs.Add(path);
                else
                {
                    var subDir = Path.GetDirectoryName(dir);
                    if (Normalize(Path.GetDirectoryName(subDir)) == folder && IsSubFolder(subDir) && MatchesVideo(Path.GetFileName(dir), videoName, video))
                        inEpisode.Add(path);
                }
            }
            return [.. inEpisode, .. PreferNamed(beside, videoName), .. PreferNamed(inSubs, videoName)];
        }

        private static bool IsSubFolder(string dir) =>
            SubFolders.Contains(Path.GetFileName(dir), StringComparer.OrdinalIgnoreCase);

        private static bool MatchesVideo(string dirName, string videoName, CustomFile video)
        {
            if (string.Equals(dirName, videoName, StringComparison.OrdinalIgnoreCase)) return true;
            if (video.Episode == null) return false;
            var custom = new CustomFile(dirName);
            return custom.Episode == video.Episode && (custom.Season == null || video.Season == null || custom.Season == video.Season);
        }

        // A season pack keeps every episode's subtitles in one folder: keep only the ones named after this video when there are any.
        private static IEnumerable<string> PreferNamed(List<string> files, string videoName)
        {
            var named = files.Where(x => Path.GetFileName(x).StartsWith(videoName, StringComparison.OrdinalIgnoreCase)).ToList();
            return named.Count > 0 ? named : files;
        }

        private static string Normalize(string path) =>
            string.IsNullOrEmpty(path) ? "" : Path.TrimEndingDirectorySeparator(Path.GetFullPath(path));
    }
}
