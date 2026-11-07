using System.Diagnostics;
using UFlix.Engine.Models;
using YoutubeExplode;
using YoutubeExplode.Playlists;
using YoutubeExplode.Search;
using YoutubeExplode.Videos;
using YoutubeExplode.Videos.Streams;

namespace UFlix.Engine.Services
{
    public class YTEngine
    {

        private static readonly HttpClient _client = new()
        {
            Timeout = TimeSpan.FromSeconds(15),
        };
        private static readonly YoutubeClient _yt = new(_client);


        public async Task<string[]> GetSuggestions(string data)
        {
            var mUrl = $"https://clients1.google.com/complete/search?client=youtube&gs_ri=youtube&ds=yt&q={data}";

            var mist = new List<string>();
            if (!string.IsNullOrWhiteSpace(data) && data.Length > 1)
            {
                try
                {
                    var mRes = await _client.GetAsync(mUrl);
                    if (mRes.IsSuccessStatusCode)
                    {
                        var all = await mRes.Content.ReadAsStringAsync();
                        var split = all.Split('[');
                        foreach (var item in split)
                        {
                            var sub = item.Split('\"');
                            if (sub.Length == 3)
                            {
                                if (!string.IsNullOrWhiteSpace(sub[1]) && !sub[1].StartsWith("\\"))
                                {
                                    mist.Add(sub[1]);
                                }
                            }
                        }
                    }
                }
                catch
                {

                }
            }
            var datas = mist.Distinct().ToArray();
            return datas;
        }


        public async Task<MediaItem[]> GetPlaylistItem(string id, bool recursive)
        {
            var url = $"{id}_{recursive}";
            var cached = DBUtilities.Instance.GetCache<MediaItem[]>(url);
            if (cached.Length>0) return cached;
            var datas = await GetPlaylistItemYTExplode(id, recursive);
            if (datas.Length>0)
                DBUtilities.Instance.AddUpdateCache(url, datas, 7);
            return datas;
        }


        public async Task<MediaItem[]> GetSimilarVideo(MediaItem media)
        {
            var cacheId = "cache_"+media.Url;
            var cached = DBUtilities.Instance.GetCache<MediaItem[]>(cacheId);
            if (cached.Length>0) return cached;
            var videoResult = await _yt.Videos.GetAsync(media.Url);
            media.Description = videoResult.Description;
            media.Tag = videoResult.Keywords.FirstOrDefault() ?? videoResult.Title;

            var all = await Search(media.Tag);
            var dataSet = all.ToList();
            var subData = dataSet.Where(x => x.Url == media.Url || x.Title == media.Title);
            all = dataSet.Except(subData).ToArray();
            if (all.Length>0)
            {
                DBUtilities.Instance.AddUpdateCache(cacheId, all);
            }
            return all;
        }

        public async Task<MediaItem> ParseUrl(string data)
        {
            var pl = PlaylistId.TryParse(data);
            if (pl.HasValue)
            {
                var playlistResult = await _yt.Playlists.GetAsync(pl.Value);
                return new MediaItem()
                {
                    Url = playlistResult.Id,
                    Title = playlistResult.Title,
                    Author = playlistResult.Author.ChannelTitle,
                    Image = playlistResult.Thumbnails.MaxBy(x => x.Resolution.Width).Url,
                    MediaType = MediaType.PlayList
                };
            }
            var md = VideoId.TryParse(data);
            if (md.HasValue)
            {
                var videoResult = await _yt.Videos.GetAsync(md.Value);
                return new MediaItem()
                {
                    Url =  videoResult.Id,
                    Title = videoResult.Title,
                    Duration = videoResult.Duration,
                    Description = videoResult.Description,
                    Image = videoResult.Thumbnails.MaxBy(x => x.Resolution.Width).Url,
                    Author = videoResult.Author.ChannelTitle,
                    UploadDate = videoResult.UploadDate.DateTime,
                    MediaType = MediaType.Youtube,
                    ContentType = MediaContentType.Youtube,
                    Tag = videoResult.Keywords.FirstOrDefault() ?? videoResult.Title,
                };
            }
            return null;
        }

        public async Task<MediaItem[]> Search(string data)
        {
            data = data.Trim().Replace('\\', '\0');
            var cached = DBUtilities.Instance.GetCache<MediaItem[]>(data);
            if (cached.Length>0) return cached;
            var all = new List<MediaItem>();

            await foreach (var result in _yt.Search.GetResultsAsync(data))
            {
                switch (result)
                {
                    case VideoSearchResult videoResult:
                        {
                            all.Add(new MediaItem()
                            {
                                Id = videoResult.Id,
                                Url =  videoResult.Id,
                                Title = videoResult.Title,
                                Duration = videoResult.Duration,
                                BackdropPath = videoResult.Thumbnails.MinBy(x => x.Resolution.Width).Url,
                                Image = videoResult.Thumbnails.MaxBy(x => x.Resolution.Width).Url,
                                Author = videoResult.Author.ChannelTitle,
                                MediaType = MediaType.Youtube,
                                ContentType = MediaContentType.Youtube,
                            });
                            break;
                        }
                    case PlaylistSearchResult playlistResult:
                        {
                            all.Add(new MediaItem()
                            {
                                Id = playlistResult.Id,
                                Url = playlistResult.Id,
                                Title = playlistResult.Title,
                                Author = playlistResult.Author.ChannelTitle,
                                BackdropPath = playlistResult.Thumbnails.MinBy(x => x.Resolution.Width).Url,
                                Image = playlistResult.Thumbnails.MaxBy(x => x.Resolution.Width).Url,
                                MediaType = MediaType.PlayList,
                                ContentType = MediaContentType.Youtube,
                            });
                            break;
                        }
                }
                if (all.Count > 30) break;
            }
            var curData = all.OrderByDescending(x => x.MediaType).ToArray();
            if (curData.Length > 0)
            {
                DBUtilities.Instance.AddUpdateCache(data, curData, 1);
            }
            return curData;
        }

        public async Task<StreamManifest> GetTrackAsync(string url)
        {
            var streamManifest = await _yt.Videos.Streams.GetManifestAsync(url);
            return streamManifest;
        }

        public async Task<Stream> GetStream(IStreamInfo streamInfo)
        {
            return await _yt.Videos.Streams.GetAsync(streamInfo);
        }

        private async Task<MediaItem[]> GetPlaylistItemYTExplode(string id, bool recursive)
        {
            var all = new List<MediaItem>();
            try
            {
                var pl = PlaylistId.TryParse(id);
                await foreach (var result in _yt.Playlists.GetVideosAsync(pl.Value))
                {
                    if (!recursive && all.Count > 30)
                        break;
                    all.Add(new MediaItem()
                    {
                        Url = result.Id,
                        Title = result.Title,
                        Author = result.Author.ChannelTitle,
                        Image = result.Thumbnails.LastOrDefault()?.Url,
                        MediaType = MediaType.Youtube,
                    });
                }
            }
            catch (Exception ex)
            {
                Debug.WriteLine(ex);
            }
            return all.ToArray();
        }
        //private async Task<MediaItem[]> GetPlaylistItemGoogle(string playlist, bool recursive)
        //{

        //    var items = new List<PlaylistItem>();
        //    using var youtubeService = GetServiceNoAuth();
        //    var search = youtubeService.PlaylistItems.List("snippet,contentDetails");
        //    search.PlaylistId = playlist;
        //    search.MaxResults = 50;

        //    try
        //    {
        //        var response = await search.ExecuteAsync();
        //        items.AddRange(response.Items);
        //        while (recursive && !string.IsNullOrWhiteSpace(response.NextPageToken))
        //        {
        //            search.PageToken = response.NextPageToken;
        //            response = await search.ExecuteAsync();
        //            items.AddRange(response.Items);
        //        }
        //    }
        //    catch (Exception ex)
        //    {
        //        Debug.WriteLine(ex);
        //    }
        //    var allResult = new List<MediaItem>();
        //    foreach (var item in items)
        //    {
        //        var video = new MediaItem()
        //        {
        //            Title = item.Snippet.Title,
        //            Id = item.Snippet.ResourceId.VideoId,
        //            Image = item.Snippet.Thumbnails?.High.Url,
        //            Description = item.Snippet.Description,
        //            Author = item.Snippet.VideoOwnerChannelTitle,
        //            MediaType = MediaType.Youtube
        //        };
        //        if (item.ContentDetails?.EndAt != null)
        //        {
        //            double.TryParse(item.ContentDetails.EndAt, out double duration);
        //            video.Duration = TimeSpan.FromSeconds(duration);
        //        }
        //        allResult.Add(video);
        //    }
        //    return allResult.ToArray();
        //}

        public async Task<string> Download(MediaItem media, string tempFile)
        {
            var streamManifest = await _yt.Videos.Streams.GetManifestAsync(media.Url);
            var audio = streamManifest.GetAudioOnlyStreams().GetWithHighestBitrate();
            await _yt.Videos.Streams.DownloadAsync(audio, tempFile);
            return tempFile;
        }

        public static string MakeValidFileName(string name)
        {
            var invalidChars = System.Text.RegularExpressions.Regex.Escape(new string(Path.GetInvalidFileNameChars()));
            var invalidRegStr = string.Format(@"([{0}]*\.+$)|([{0}]+)", invalidChars);

            return System.Text.RegularExpressions.Regex.Replace(name, invalidRegStr, "_");
        }
        //public async Task GetComments(string id)
        //{

        //}
    }
}
