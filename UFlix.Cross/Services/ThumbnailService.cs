using System;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using UFlix.Engine.Models;
using UFlix.Engine.Services;

namespace UFlix.Cross.Services
{
    /// <summary>Local video thumbnails: one frame at 10 % of the duration, grabbed with ffmpeg and cached as JPEG.</summary>
    public static class ThumbnailService
    {
        private static readonly SemaphoreSlim Gate = new(2);
        private static readonly string CacheFolder = Path.Combine(AppPaths.DataFolder, "Thumbs");

        public static async Task<string> GetAsync(string videoPath)
        {
            if (!File.Exists(videoPath)) return null;
            var info = new FileInfo(videoPath);
            var key = Convert.ToHexString(SHA1.HashData(Encoding.UTF8.GetBytes($"{info.FullName}|{info.Length}|{info.LastWriteTimeUtc.Ticks}")));
            var thumb = Path.Combine(CacheFolder, key + ".jpg");
            if (File.Exists(thumb)) return thumb;

            await Gate.WaitAsync();
            try
            {
                if (File.Exists(thumb)) return thumb;
                Directory.CreateDirectory(CacheFolder);
                var duration = await Run(Tool(FFmpegUtils.FfprobeBinaryName), "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", videoPath);
                var at = double.TryParse(duration?.Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out var d) ? d * 0.1 : 5;
                await Run(Tool(FFmpegUtils.FfmpegBinaryName), "-v", "error", "-y", "-ss", at.ToString("0.###", CultureInfo.InvariantCulture), "-i", videoPath,
                    "-frames:v", "1", "-vf", "scale=400:-2", thumb);
                return File.Exists(thumb) ? thumb : null;
            }
            catch (Exception ex)
            {
                Debug.WriteLine(ex);
                return null;
            }
            finally
            {
                Gate.Release();
            }
        }

        private static string Tool(string name)
        {
            if (AppConstants.YtFile != null)
            {
                var local = Path.Combine(Path.GetDirectoryName(AppConstants.YtFile), name);
                if (File.Exists(local)) return local;
            }
            return name;
        }

        private static async Task<string> Run(string file, params string[] args)
        {
            var psi = new ProcessStartInfo(file) { RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false, CreateNoWindow = true };
            foreach (var a in args) psi.ArgumentList.Add(a);
            using var p = Process.Start(psi);
            var output = await p.StandardOutput.ReadToEndAsync();
            await p.WaitForExitAsync();
            return output;
        }
    }
}
