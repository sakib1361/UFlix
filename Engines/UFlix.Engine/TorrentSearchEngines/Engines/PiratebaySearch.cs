using AngleSharp;
using System.Text.RegularExpressions;
using TMDbLib.Objects.Movies;
using TMDbLib.Objects.Search;
using TMDbLib.Objects.TvShows;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using UFlix.Engine.TorrentSearchEngines.Base;

namespace UFlix.Engine.TorrentSearchEngines.Engines
{
    internal class PiratebaySearch : SearchEngineCommon
    {
        public override string Name => GetType().Name;
        protected override string FallbackUrl { get; } = "https://tpb.party/";
        //https://piratebayproxy.info/

        internal override async Task<IEnumerable<TorrentInfo>> Search(Movie movie, bool forced)
        {
            var src = DefaultUri;
            var title = Uri.EscapeDataString(movie.Title);
            return await Search(title, forced);
        }

        internal override async Task<IEnumerable<TorrentInfo>> Search(TvShow currentShow, TvSeasonEpisode episode, bool forced)
        {
            var src = DefaultUri;
            var title = Uri.EscapeDataString($"{currentShow.Name} S{episode.SeasonNumber:00}E{episode.EpisodeNumber:00}");
            var res = await Search(title, forced);
            if (res.Length!=0) return res;
            title = Uri.EscapeDataString($"{currentShow.Name}");
            res = await Search(title, forced);
            return res;
        }

        internal override async Task<IEnumerable<TorrentInfo>> SearchFullSeason(TvShow currentShow, TvSeasonEpisode episode, bool forced)
        {
            var src = DefaultUri;
            var title = Uri.EscapeDataString($"{currentShow.Name} S{episode.SeasonNumber:00}");
            //var query = $"{src}/search/{title}/1/99/200";
            var res = await Search(title, forced);
            if (res.Length!=0) return res;
            title = Uri.EscapeDataString($"{currentShow.Name}");
            res = await Search(title, forced);
            return res;
        }

        private async Task<TorrentInfo[]> Search(string query, bool forced)
        {
            // var url = $"{DefaultUri}/s?q={query}&video=on&category=0";
            var url = $"{DefaultUri}/search/{query}/1/99/0";
            var data = DBUtilities.Instance.GetCache<TorrentInfo[]>(url);
            if (data.Length == 0 || forced)
            {
                var config = Configuration.Default.WithDefaultLoader();
                var context = BrowsingContext.New(config);
                var document = await context.OpenAsync(url);

                var allRes = new List<TorrentInfo>();
                var allNodes = document.QuerySelectorAll("#searchResult tbody tr");
                if (allNodes.Length <= 1) return [];

                foreach (var node in allNodes)
                {
                    var subNodes = node.QuerySelectorAll("td a");
                    if(subNodes == null || subNodes.Length < 2) continue;

                    var titleLink = node.QuerySelector("a.detLink") ?? node.QuerySelector("td:nth-child(2) > a");
                    var magnetNode = node.QuerySelector("a[href^='magnet']");
                    var sizeText = node.QuerySelector("font.detDesc")?.TextContent ?? node.QuerySelector("td:nth-child(5)")?.TextContent;

                    // Seeders and Leechers
                    string seedsText = null, leechText = null;
                    var rightAlignedTds = node.QuerySelectorAll("td[align='right']").ToArray();
                    if(rightAlignedTds.Length > 2)
                    {
                         seedsText = rightAlignedTds.ElementAtOrDefault(1)?.TextContent.Trim();
                         leechText = rightAlignedTds.ElementAtOrDefault(2)?.TextContent.Trim();
                    }
                    else
                    {
                         seedsText = rightAlignedTds.ElementAtOrDefault(0)?.TextContent.Trim();
                         leechText = rightAlignedTds.ElementAtOrDefault(1)?.TextContent.Trim();
                    }

                    if (titleLink == null || magnetNode == null) continue;

                    string titleText = titleLink.TextContent.Trim();
                    string magnet = magnetNode.GetAttribute("href") ?? "";
                    
                    var torrentInfo = new TorrentInfo()
                    {
                        Title = titleText,
                        Url = magnet,
                        Size = ParseSize(sizeText),
                        Seeders = seedsText,
                        Leechers = leechText,
                        Provider = Name,
                        Quality = TorrentInfo.ParseQuality(titleText),
                    };
                   
                    allRes.Add(torrentInfo);
                }

                data = allRes.ToArray();
                if (data.Length > 0)
                {
                    DBUtilities.Instance.AddUpdateCache(url, data);
                }
            }

            return data;
        }

        private string ParseSize(string text)
        {
            if (string.IsNullOrEmpty(text)) return "Unknown";

            var match = Regex.Match(text, @"(\d+(\.\d+)?\s?(GiB|MiB))", RegexOptions.IgnoreCase);
            if (match.Success) return match.Groups[1].Value;

            match = Regex.Match(text, @"Size\s+([^\s,]+)", RegexOptions.IgnoreCase);
            return match.Success ? match.Value : "Unknown";
        }
    }
}
