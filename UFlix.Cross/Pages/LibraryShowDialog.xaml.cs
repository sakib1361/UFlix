using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using TMDbLib.Objects.Search;
using TMDbLib.Objects.TvShows;
using UFlix.Cross.Services;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using UFlix.Engine.TorrentSearchEngines.Base;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace UFlix.Cross.Pages
{
    /// <summary>
    /// An empty page that can be used on its own or navigated to within a Frame.
    /// </summary>
    public sealed partial class LibraryShowDialog : ContentDialog
    {
        public MediaItem File { get; set; }
        private readonly TmdbMediaService _container;
        private readonly TorrentSearchHandler SearchHandler;
        private readonly string CurrentGenre;
        private readonly double CurrentRating;
        private TvShow CurrentShow;
        private TvSeasonEpisode _episode;
        private readonly ObservableCollection<TvSeasonEpisode> TvEpisodes = [];
        private int _searchId;
        public LibraryShowDialog(TvShow show)
        {
            _container = new TmdbMediaService();
            SearchHandler = new TorrentSearchHandler();
            CurrentGenre = string.Join(',', show.Genres.Select(x => x.Name));
            CurrentRating = show.VoteAverage/2;
            CurrentShow = show;
            this.InitializeComponent();
            ProviderCombo.ItemsSource = SearchHandler.Engines;
        }

        private void ContentDialog_Loaded(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        {
            if (File != null)
            {
                ContinueButton.Content = $"Continue {File.Title}";
                var customFile = new CustomFile(File.Title);
                if (customFile != null && customFile.Season.HasValue && customFile.Episode.HasValue)
                {
                    SeasonCombo.SelectedItem = CurrentShow.Seasons.FirstOrDefault(x => x.SeasonNumber == customFile.Season.Value);
                }
                CustomTextBox.Text = File.Url;
                ContinueButton.Visibility = Microsoft.UI.Xaml.Visibility.Visible;
            }
            ProviderCombo.SelectedIndex = 0;
        }

        private void Close_Click(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        {
            File = null;
            Hide();
        }

        private async void ComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (e.AddedItems.Count > 0 && e.AddedItems[0] is SearchTvSeason search)
            {
                var res = await _container.GetSeasonInfo(CurrentShow, search.SeasonNumber);
                if (res != null)
                {
                    TvEpisodes.Clear();
                    foreach (var item in res.Episodes)
                    {
                        TvEpisodes.Add(item);
                    }
                }
            }
        }

        private void ListView_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (e.AddedItems.Count>0 && e.AddedItems[0] is TvSeasonEpisode episode)
            {
                _episode = episode;
                HandleSearch();
            }
        }

        private void ShowTorrents(IEnumerable<TorrentInfo> torrents) => TorrentView.SetTorrents(torrents);

        private async void HandleSearch(bool forced = false)
        {
            var id = ++_searchId;
            ShowTorrents([]);
            if (CurrentShow.ExternalIds == null)
            {
                var searchShow = new MediaItem() { TmdbId = CurrentShow.Id, }; ;
                CurrentShow = await _container.GetShowInfo(searchShow, true);
            }
            LoadIndicator.Visibility = Microsoft.UI.Xaml.Visibility.Visible;
            var res = await SearchHandler.SearchShow(CurrentShow, _episode, forced);
            if (id != _searchId) return;
            LoadIndicator.Visibility = Microsoft.UI.Xaml.Visibility.Collapsed;
            ShowTorrents(res);
        }

        private void TorrentView_TorrentSelected(object sender, TorrentInfo info)
        {
            _episode ??= TvEpisodes.FirstOrDefault();
            File = new MediaItem()
            {
                Url = info.Url,
                Image = TmdbMediaService.ImagePath300 + CurrentShow.PosterPath,
                BackdropPath = TmdbMediaService.ImagePath300+CurrentShow.BackdropPath,
                Description = CurrentShow.Overview,
                Title = CurrentShow.Name + $" S{_episode.SeasonNumber:00} E{_episode.EpisodeNumber:00}",
                Author = CurrentShow.Status,
                Id = CurrentShow.ExternalIds.ImdbId,
                TmdbId = CurrentShow.Id,
                MediaType = info.MediaType,
                ContentType = MediaContentType.Shows,
                Rating = CurrentShow.VoteCount/2,
                Year = CurrentShow.FirstAirDate?.Year.ToString(),
                ViewCount = CurrentShow.VoteCount.ToString(),
            };
            this.Hide();
        }

        private void ProviderCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (e.AddedItems.Count>0 && e.AddedItems[0] is SearchEngineCommon common)
            {
                SearchHandler.Engine = common;
                if (_episode != null)
                {
                    HandleSearch();
                }
            }
        }

        private void ContinueButton_Click(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        {
            Hide();
        }

        private void CustomUriButton_Click(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        {
            var url = CustomTextBox.Text;
            if (!string.IsNullOrEmpty(url))
            {
                File = new MediaItem()
                {
                    Url = url,
                    Image = TmdbMediaService.ImagePath300 + CurrentShow.PosterPath,
                    BackdropPath = TmdbMediaService.ImagePath300+CurrentShow.BackdropPath,
                    Description = CurrentShow.Overview,
                    Title = CurrentShow.Name + $" S{_episode.SeasonNumber:00} E{_episode.EpisodeNumber:00}",
                    Author = CurrentShow.Status,
                    Id = CurrentShow.ExternalIds.ImdbId,
                    TmdbId = CurrentShow.Id,
                    MediaType = url.StartsWith("magnet:") ? MediaType.Torrent : MediaType.Web,
                    ContentType = MediaContentType.Shows,
                    Rating = CurrentShow.VoteCount/2,
                    Year = CurrentShow.FirstAirDate?.Year.ToString(),
                    ViewCount = CurrentShow.VoteCount.ToString(),
                };
                Hide();
            }
        }

        private void ForceButton_Click(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        {
            HandleSearch(true);
        }

        private async void CastButton_Checked(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        {
            if (CastGrid.ItemsSource != null) return;
            if (CurrentShow.Credits == null)
            {
                var show = await _container.GetShowInfo(new MediaItem() { TmdbId = CurrentShow.Id }, true);
                if (show != null) CurrentShow = show;
            }
            CastGrid.ItemsSource = CurrentShow.Credits?.Cast;
        }

        private async void FullSeason_Click(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        {
            var id = ++_searchId;
            ShowTorrents([]);
            _episode = null;
            EpisodeList.SelectedItem = null;
            if (CurrentShow.ExternalIds == null)
            {
                var searchShow = new MediaItem() { TmdbId = CurrentShow.Id, }; ;
                CurrentShow = await _container.GetShowInfo(searchShow, true);
            }
            LoadIndicator.Visibility = Microsoft.UI.Xaml.Visibility.Visible;
            var ep = TvEpisodes.FirstOrDefault();
            var res = await SearchHandler.SearchFullShow(CurrentShow, ep, true);
            if (id != _searchId) return;
            LoadIndicator.Visibility = Microsoft.UI.Xaml.Visibility.Collapsed;
            ShowTorrents(res);
        }
    }
}
