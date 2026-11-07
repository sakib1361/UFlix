using System.Collections.Concurrent;
using System.Runtime.CompilerServices;
using UFlix.Engine.Models;
using YoutubeExplode;
using YoutubeExplode.Channels;
using YoutubeExplode.Common;
using YoutubeExplode.Playlists;
using YoutubeExplode.Search;
using YoutubeExplode.Videos;
using YoutubeExplode.Videos.ClosedCaptions;

namespace UFlix.Engine.Services
{
    public enum YtKind
    {
        Video,
        Playlist,
        Channel,
    }

    /// <summary>One tile of the YouTube page: a video, a playlist or a channel.</summary>
    public sealed class YtItem
    {
        public string Id { get; set; }
        public YtKind Kind { get; set; }
        public string Title { get; set; }
        public string Author { get; set; }
        public string ChannelId { get; set; }
        public string Thumbnail { get; set; }
        public TimeSpan? Duration { get; set; }
        public double Progress { get; set; }
        public MediaItem Source { get; set; }

        public string Badge => Kind switch
        {
            YtKind.Video => YoutubeService.FormatDuration(Duration),
            YtKind.Playlist => "Playlist",
            _ => null,
        };

        public bool IsLive => Kind == YtKind.Video && Duration == null;
        public bool HasProgress => Progress > 0;
        public double ProgressPercent => Progress * 100;

        public MediaItem ToMedia() => Source ?? new MediaItem
        {
            Id = Id,
            Url = Id,
            Title = Title,
            Author = Author,
            Duration = Duration,
            Image = Kind == YtKind.Video ? YoutubeService.Thumb(Id, "hqdefault") : Thumbnail,
            MediaType = Kind == YtKind.Playlist ? MediaType.PlayList : MediaType.Youtube,
            ContentType = MediaContentType.Youtube,
        };
    }

    public sealed class YtVideoDetails
    {
        public string Id { get; init; }
        public string Title { get; init; }
        public string Author { get; init; }
        public string ChannelId { get; init; }
        public string Description { get; init; }
        public string Stats { get; init; }
    }

    public sealed class YtChannel
    {
        public string Id { get; init; }
        public string Title { get; init; }
        public string Avatar { get; init; }
    }

    /// <summary>Pages through one or more result streams, interleaving them and dropping duplicates. Pages are pulled on demand (incremental loading), never ahead.</summary>
    public sealed class YtPager : IDisposable
    {
        private readonly List<IAsyncEnumerator<YtItem>> _sources;
        private readonly HashSet<string> _seen = [];
        private readonly CancellationTokenSource _cts = new();

        public YtPager(params IEnumerable<Func<CancellationToken, IAsyncEnumerable<YtItem>>> sources)
        {
            _sources = [.. sources.Select(s => s(_cts.Token).GetAsyncEnumerator(_cts.Token))];
        }

        public async Task<YtItem[]> NextAsync(int count)
        {
            var page = new List<YtItem>();
            while (page.Count < count && _sources.Count > 0)
            {
                _cts.Token.ThrowIfCancellationRequested();
                for (var i = 0; i < _sources.Count && page.Count < count;)
                {
                    bool moved;
                    try
                    {
                        moved = await _sources[i].MoveNextAsync();
                    }
                    catch (Exception ex) when (ex is not OperationCanceledException)
                    {
                        System.Diagnostics.Debug.WriteLine(ex);
                        moved = false;
                    }
                    if (!moved)
                    {
                        _sources.RemoveAt(i);
                        continue;
                    }
                    var item = _sources[i].Current;
                    if (_seen.Add(item.Kind + item.Id)) page.Add(item);
                    i++;
                }
            }
            return [.. page];
        }

        public void Dispose() => _cts.Cancel();
    }

    /// <summary>YouTube data for the YouTube page and the player (YoutubeExplode; playback streams stay in BaseMediaContainer).</summary>
    public static class YoutubeService
    {
        private static readonly HttpClient _client = new() { Timeout = TimeSpan.FromSeconds(15) };
        private static readonly YoutubeClient _yt = new(_client);
        private static readonly ConcurrentDictionary<string, YtVideoDetails> _details = new();
        private static readonly ConcurrentDictionary<string, YtChannel> _channels = new();
        private const string HistoryKey = "YtSearchHistory";

        public static string[] Topics { get; } = ["Music", "Gaming", "News", "Live", "Podcasts", "Trailers", "Sports", "Technology", "Science", "Cooking"];

        // ---- search history (drives the "For you" feed) ----

        public static string[] SearchHistory => DBUtilities.Instance.GetPermanent<string[]>(HistoryKey) ?? [];

        public static void AddToHistory(string query)
        {
            query = query.Trim();
            if (query.Length == 0) return;
            string[] history = [query, .. SearchHistory.Where(x => !string.Equals(x, query, StringComparison.OrdinalIgnoreCase)).Take(11)];
            DBUtilities.Instance.AddUpdatePermanent(history, HistoryKey);
        }

        public static void RemoveFromHistory(string query) =>
            DBUtilities.Instance.AddUpdatePermanent(SearchHistory.Where(x => x != query).ToArray(), HistoryKey);

        // ---- feeds ----

        public static YtPager Search(string query, SearchFilter filter = SearchFilter.None) => new(ct => SearchAsync(query, filter, ct));

        /// <summary>Interleaves the latest searches, so the home feed follows what the user watches.</summary>
        public static YtPager ForYou()
        {
            var seeds = SearchHistory.Take(3).ToList();
            if (seeds.Count == 0 && DBUtilities.Instance.LastSearch is { Length: > 0 } last) seeds.Add(last);
            return ForYou(seeds);
        }

        /// <summary>The "For you" feed from the given searches.</summary>
        public static YtPager ForYou(IReadOnlyCollection<string> seeds)
        {
            if (seeds.Count == 0) seeds = ["trending", "music", "documentary"];
            return new(seeds.Select(q => (Func<CancellationToken, IAsyncEnumerable<YtItem>>)(ct => SearchAsync(q, SearchFilter.Video, ct))));
        }

        public static YtPager Uploads(string channelId) => new(ct => UploadsAsync(channelId, ct));

        private static async IAsyncEnumerable<YtItem> SearchAsync(string query, SearchFilter filter, [EnumeratorCancellation] CancellationToken ct)
        {
            await foreach (var batch in _yt.Search.GetResultBatchesAsync(query, filter, ct))
            {
                foreach (var result in batch.Items)
                {
                    var item = FromResult(result);
                    if (item != null) yield return item;
                }
            }
        }

        private static async IAsyncEnumerable<YtItem> UploadsAsync(string channelId, [EnumeratorCancellation] CancellationToken ct)
        {
            await foreach (var video in _yt.Channels.GetUploadsAsync(channelId, ct))
                yield return FromVideo(video.Id, video.Title, video.Author, video.Duration);
        }

        private static YtItem FromResult(ISearchResult result) => result switch
        {
            VideoSearchResult v => FromVideo(v.Id, v.Title, v.Author, v.Duration),
            PlaylistSearchResult p => new YtItem
            {
                Id = p.Id,
                Kind = YtKind.Playlist,
                Title = p.Title,
                Author = p.Author?.ChannelTitle,
                ChannelId = p.Author?.ChannelId,
                Thumbnail = BestThumb(p.Thumbnails, 480),
            },
            ChannelSearchResult c => new YtItem
            {
                Id = c.Id,
                Kind = YtKind.Channel,
                Title = c.Title,
                Author = "Channel",
                ChannelId = c.Id,
                Thumbnail = BestThumb(c.Thumbnails, 176),
            },
            _ => null,
        };

        private static YtItem FromVideo(VideoId id, string title, Author author, TimeSpan? duration) => new()
        {
            Id = id,
            Kind = YtKind.Video,
            Title = title,
            Author = author?.ChannelTitle,
            ChannelId = author?.ChannelId,
            Duration = duration,
            Thumbnail = Thumb(id, "mqdefault"),
        };

        /// <summary>Continue-watching tiles from the shared watchlist (videos and playlists opened from YouTube).</summary>
        public static YtItem[] ContinueWatching() =>
        [
            .. new WatchlistService().GetWatchlistMediasAsync()
                .Where(x => x.MediaType is MediaType.Youtube or MediaType.PlayList && !string.IsNullOrEmpty(x.Url))
                .Take(20)
                .Select(x => new YtItem
                {
                    Id = x.Url,
                    Kind = x.MediaType == MediaType.PlayList ? YtKind.Playlist : YtKind.Video,
                    Title = x.Title,
                    Author = x.Author,
                    Duration = x.Duration,
                    Thumbnail = x.MediaType == MediaType.Youtube ? Thumb(x.Url, "mqdefault") : x.Image,
                    Progress = x.Duration is { TotalSeconds: > 0 } d ? Math.Clamp(x.Position / d.TotalSeconds, 0, 1) : 0,
                    Source = x,
                })
        ];

        // ---- details ----

        public static async Task<YtVideoDetails> GetVideoAsync(string id)
        {
            if (_details.TryGetValue(id, out var cached)) return cached;
            var video = await _yt.Videos.GetAsync(id);
            var stats = new List<string>();
            if (video.Engagement.ViewCount > 0) stats.Add($"{FormatCount(video.Engagement.ViewCount)} views");
            if (video.UploadDate.Year > 1970) stats.Add(FormatAge(video.UploadDate));
            if (video.Engagement.LikeCount > 0) stats.Add($"{FormatCount(video.Engagement.LikeCount)} likes");
            var details = new YtVideoDetails
            {
                Id = id,
                Title = video.Title,
                Author = video.Author.ChannelTitle,
                ChannelId = video.Author.ChannelId,
                Description = video.Description,
                Stats = string.Join("  ·  ", stats),
            };
            _details[id] = details;
            return details;
        }

        /// <summary>"Up next" for the player: videos only (search results also carry author-less mixes and playlists), without the video itself.</summary>
        public static async Task<MediaItem[]> GetRelatedAsync(MediaItem media, int count = 30)
        {
            var video = await _yt.Videos.GetAsync(media.Url);
            var query = video.Keywords.FirstOrDefault() ?? video.Title;
            var related = new List<MediaItem>();
            await foreach (var item in SearchAsync(query, SearchFilter.Video, CancellationToken.None))
            {
                if (item.Kind != YtKind.Video || item.Id == media.Url || item.Title == media.Title) continue;
                related.Add(item.ToMedia());
                if (related.Count == count) break;
            }
            return [.. related];
        }

        public static async Task<YtChannel> GetChannelAsync(string channelId)
        {
            if (_channels.TryGetValue(channelId, out var cached)) return cached;
            var channel = await _yt.Channels.GetAsync(channelId);
            var result = new YtChannel { Id = channel.Id, Title = channel.Title, Avatar = BestThumb(channel.Thumbnails, 176) };
            _channels[channelId] = result;
            return result;
        }

        /// <summary>Resolves a pasted link: a video, a playlist, a channel id or an @handle.</summary>
        public static async Task<YtItem> ResolveAsync(string text)
        {
            text = text.Trim();
            if (VideoId.TryParse(text) is { } videoId && !text.Contains("list="))
            {
                var video = await _yt.Videos.GetAsync(videoId);
                return FromVideo(video.Id, video.Title, video.Author, video.Duration);
            }
            if (PlaylistId.TryParse(text) is { } playlistId)
            {
                var playlist = await _yt.Playlists.GetAsync(playlistId);
                return new YtItem { Id = playlist.Id, Kind = YtKind.Playlist, Title = playlist.Title, Author = playlist.Author?.ChannelTitle, Thumbnail = BestThumb(playlist.Thumbnails, 480) };
            }
            Channel channel = null;
            if (ChannelId.TryParse(text) is { } channelId) channel = await _yt.Channels.GetAsync(channelId);
            else if (text.Contains("youtube.com/") && ChannelHandle.TryParse(text) is { } handle) channel = await _yt.Channels.GetByHandleAsync(handle);
            return channel == null ? null : new YtItem { Id = channel.Id, Kind = YtKind.Channel, Title = channel.Title, ChannelId = channel.Id, Thumbnail = BestThumb(channel.Thumbnails, 176) };
        }

        // ---- captions ----

        public static async Task<ClosedCaptionTrackInfo[]> GetCaptionsAsync(string videoId)
        {
            var manifest = await _yt.Videos.ClosedCaptions.GetManifestAsync(videoId);
            return [.. manifest.Tracks.OrderBy(x => x.IsAutoGenerated).ThenBy(x => x.Language.Name)];
        }

        /// <summary>Downloads a caption track as SRT into the temp folder for mpv's sub-add.</summary>
        public static async Task<string> DownloadCaptionAsync(string videoId, ClosedCaptionTrackInfo track)
        {
            var file = Path.Combine(Path.GetTempPath(), $"uflix-yt-{videoId}-{track.Language.Code}{(track.IsAutoGenerated ? "-auto" : "")}.srt");
            if (!File.Exists(file)) await _yt.Videos.ClosedCaptions.DownloadAsync(track, file);
            return file;
        }

        // ---- formatting ----

        public static string Thumb(string videoId, string size) => $"https://i.ytimg.com/vi/{videoId}/{size}.jpg";

        private static string BestThumb(IReadOnlyList<Thumbnail> thumbs, int width)
        {
            var thumb = thumbs.Where(x => x.Resolution.Width >= width).MinBy(x => x.Resolution.Width) ?? thumbs.MaxBy(x => x.Resolution.Width);
            var url = thumb?.Url;
            return url != null && url.StartsWith("//") ? "https:" + url : url;
        }

        public static string FormatDuration(TimeSpan? duration) => duration switch
        {
            null => "LIVE",
            { TotalHours: >= 1 } d => d.ToString(@"h\:mm\:ss"),
            { } d => d.ToString(@"m\:ss"),
        };

        public static string FormatCount(long count) => count switch
        {
            >= 1_000_000_000 => $"{count / 1e9:0.#}B",
            >= 1_000_000 => $"{count / 1e6:0.#}M",
            >= 1_000 => $"{count / 1e3:0.#}K",
            _ => count.ToString(),
        };

        public static string FormatAge(DateTimeOffset date)
        {
            var age = DateTimeOffset.Now - date;
            return age.TotalDays switch
            {
                >= 730 => $"{(int)(age.TotalDays / 365)} years ago",
                >= 365 => "1 year ago",
                >= 60 => $"{(int)(age.TotalDays / 30)} months ago",
                >= 30 => "1 month ago",
                >= 14 => $"{(int)(age.TotalDays / 7)} weeks ago",
                >= 2 => $"{(int)age.TotalDays} days ago",
                >= 1 => "yesterday",
                _ => "today",
            };
        }
    }
}
