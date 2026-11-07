using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Threading.Tasks;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using YoutubeDLSharp;
using YoutubeExplode.Videos.Streams;

namespace UFlix.Cross.Services
{
    public partial class BaseMediaContainer
    {
        protected readonly YTEngine yT = new();
        public List<TrackItem> MediaTracks { get; } = [];
        public TrackItem CurrentSubTrack { get; protected set; }
        public TrackItem CurrentAudioTrack { get; protected set; }
        protected static string[] LoadSubtitle(MediaItem mediaItem, string videoPath = null)
        {
            if (videoPath == null && File.Exists(mediaItem.Url)) videoPath = mediaItem.Url;
            if (videoPath == null)
            {
                var folder = Directory.Exists(mediaItem.FolderPath) ? mediaItem.FolderPath : Path.GetDirectoryName(mediaItem.Url);
                if (folder == null) return [];
                videoPath = Path.Combine(folder, mediaItem.Title ?? "");
            }
            return SubtitleFinder.Find(videoPath, mediaItem.Title);
        }

        /// <summary>IPTV channels and HLS playlists: live, so no resume position and no sidecar subtitle lookup.</summary>
        protected static bool IsLiveStream(MediaItem media)
        {
            if (media.MediaType == MediaType.Unknown) return true;
            return Uri.TryCreate(media.Url, UriKind.Absolute, out var uri)
                && uri.AbsolutePath.EndsWith(".m3u8", StringComparison.OrdinalIgnoreCase);
        }

        protected async Task<string[]> GetWebSubFiles(string mainUrl)
        {
            var subFilesTask = new List<Task>();
            var fileList = new List<string>();
            using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(5) };
            var searchExt = new[] { "en.srt", "srt" };

            foreach (var ext in searchExt)
                subFilesTask.Add(DownloadFile(http, Path.ChangeExtension(mainUrl, ext), fileList));
            await Task.WhenAll(subFilesTask);
            return [.. fileList];
        }

        protected async Task<string[]> GetWebSubFiles(params string[] urls)
        {
            var subFilesTask = new List<Task>();
            var fileList = new List<string>();
            using var http = new HttpClient();
           
            foreach (var ext in urls)
                subFilesTask.Add(DownloadFile(http, ext, fileList));
            await Task.WhenAll(subFilesTask);
            return [.. fileList];
        }

        private static async Task DownloadFile(HttpClient client, string subUrl, List<string> files)
        {
            try
            {
                var resp = await client.GetAsync(subUrl);
                if (resp.IsSuccessStatusCode)
                {
                    var contentData = await resp.Content.ReadAsStringAsync();
                    if (contentData.Length > 1024*10)
                    {
                        var destFile = Path.Combine(Path.GetTempPath(), Path.GetFileName(subUrl));
                        if (File.Exists(destFile)) File.Delete(destFile);
                        await File.WriteAllTextAsync(destFile, contentData);
                        files.Add(destFile);
                    }
                }
            }
            catch(Exception ex) 
            {
                Debug.WriteLine(ex);
            }
        }

        protected async Task<bool> TryAndRunWithYTDL(MediaItem media)
        {
            var yt = new YoutubeDL()
            {
                YoutubeDLPath = AppConstants.YtFile,
            };
            try
            {

                var fullUri = $"https://www.youtube.com/watch?v={media.Url}";
                var dataSet = await yt.RunVideoDataFetch(fullUri);
                var hasHd = dataSet.Data.Formats.Any(x => x.Height.HasValue && x.Height>=720) ? 720 : 240;
                foreach (var track in dataSet.Data.Formats
                                             .Where(x => x.Height.HasValue && x.Height.Value>=hasHd && x.AudioCodec == "none")
                                             .OrderByDescending(x => x.Height.Value))
                {

                    //if (track.ManifestUrl != null) continue;
                    var extStream = new TrackItem(TrackType.Video)
                    {
                        Id = track.Url,
                        Address = track.Url,
                        BitRate = (long)(track.VideoBitrate??0),
                        Codec = track.VideoCodec??"unk",
                        //Language = Language.Get(fmt.language),
                        Width = track.Width??0,
                        Height = track.Height??0,
                        FPS = track.FrameRate??0,
                    };


                    extStream.Description = YoutubeTrackName(extStream, track.ManifestUrl != null);
                    MediaTracks.Add(extStream);
                }
                var aTrack = dataSet.Data.Formats.Where(x => x.AudioCodec != "none" && x.VideoCodec == "none").MaxBy(x => x.AudioBitrate);
                var extAudio = new TrackItem(TrackType.Audio)
                {
                    Address = aTrack.Url,
                    BitRate= (long)(aTrack.AudioBitrate??0),
                    Codec =aTrack.AudioCodec??"unk",
                };
                extAudio.Description = $"{extAudio.Width}x{extAudio.Height} @{extAudio.FPS} {extAudio.Codec}";
                MediaTracks.Add(extAudio);
                return true;
            }
            catch
            {
                return false;
            }
        }

        /// <summary>Quality menu label, e.g. "1080p60 · VP9" (" · HLS" marks yt-dlp's m3u8 variants).</summary>
        protected static string YoutubeTrackName(TrackItem track, bool hls)
        {
            var codec = track.Codec ?? "";
            var name = codec.StartsWith("avc") ? "AVC" : codec.StartsWith("vp9") || codec.StartsWith("vp09") ? "VP9" : codec.StartsWith("av01") ? "AV1" : codec.ToUpperInvariant();
            return $"{track.Height}p{(track.FPS > 30 ? Math.Round(track.FPS).ToString() : "")}  ·  {name}{(hls ? "  ·  HLS" : "")}";
        }

        protected async Task<bool> TryAndRunWithNative(MediaItem media)
        {
            try
            {
                var dataSet = await yT.GetTrackAsync(media.Url);
                var hasHd = dataSet.GetVideoOnlyStreams().Any(x => x.VideoResolution.Height>=720) ? 720 : 240;
                foreach (var video in dataSet.GetVideoOnlyStreams()
                                            .Where(x => x.VideoResolution.Height>=hasHd)
                                            .OrderByDescending(x => x.VideoResolution.Height))
                {
                    var extStream = new TrackItem(TrackType.Video)
                    {
                        Address = video.Url,
                        BitRate = video.Bitrate.BitsPerSecond,
                        Codec = video.VideoCodec.ToString(),
                        //Language = Language.Get(fmt.language),
                        Width = video.VideoResolution.Width,
                        Height = video.VideoResolution.Height,
                        FPS = video.VideoQuality.Framerate,
                        Stream = video
                    };
                    extStream.Description = YoutubeTrackName(extStream, false);
                    MediaTracks.Add(extStream);
                }
                var audio = dataSet.GetAudioOnlyStreams().TryGetWithHighestBitrate();
                var extAudio = new TrackItem(TrackType.Audio)
                {
                    Address = audio.Url,
                    BitRate= audio.Bitrate.BitsPerSecond,
                    Codec = audio.Container.Name,
                    Stream = audio
                };
                extAudio.Description = $"{extAudio.Width}x{extAudio.Height} @{extAudio.FPS} {extAudio.Codec}";
                MediaTracks.Add(extAudio);
                return true;
            }
            catch (Exception ex)
            {
                Debug.WriteLine(ex);
                return false;
            }
        }
    }
}
