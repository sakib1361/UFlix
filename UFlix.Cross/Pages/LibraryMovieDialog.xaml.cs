using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using TMDbLib.Objects.Movies;
using UFlix.Cross.Services;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using UFlix.Engine.TorrentSearchEngines.Base;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace UFlix.Cross.Pages
{
    public sealed partial class LibraryMovieDialog : ContentDialog
    {
        public MediaItem File { get; set; }

        private readonly Movie CurrentMovie;
        private readonly double CustomRating;

        private readonly TorrentSearchHandler SearchHandler;
        private int _searchId;
        public LibraryMovieDialog(Movie movie)
        {
            CurrentMovie = movie;
            CustomRating = movie.VoteAverage/2;
            SearchHandler = new TorrentSearchHandler();
            this.InitializeComponent();
            ProviderCombo.ItemsSource = SearchHandler.Engines;
        }

        private void Content_Loaded(object sender, RoutedEventArgs e)
        {
            ContinueButton.Visibility = File == null ? Visibility.Collapsed : Visibility.Visible;
            ProviderCombo.SelectedIndex = 0;
        }

        private void Close_Click(object sender, RoutedEventArgs e)
        {
            File = null;
            this.Hide();
        }

        private void TorrentView_TorrentSelected(object sender, TorrentInfo info)
        {
            File = new MediaItem()
            {
                Id = CurrentMovie.ImdbId,
                TmdbId = CurrentMovie.Id,
                Url = info.Url,
                Image = TmdbMediaService.ImagePath300 + CurrentMovie.PosterPath,
                BackdropPath = TmdbMediaService.ImagePath300+CurrentMovie.BackdropPath,
                Description = CurrentMovie.Overview,
                Title = CurrentMovie.Title,
                Author = CurrentMovie.Status,
                MediaType = info.MediaType,
                Rating = CurrentMovie.VoteCount/2,
                Year = CurrentMovie.ReleaseDate?.Year.ToString(),
                ViewCount = CurrentMovie.VoteCount.ToString(),
            };
            this.Hide();
        }

        private void ProviderCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (e.AddedItems.Count>0 && e.AddedItems[0] is SearchEngineCommon common)
            {
                SearchHandler.Engine = common;
                HandleSearch();

            }
        }

        private async void HandleSearch(bool forced = false)
        {
            var id = ++_searchId;
            TorrentView.SetTorrents([]);
            LoadIndicator.Visibility = Visibility.Visible;
            var res = await SearchHandler.SearchMovie(CurrentMovie, forced);
            if (id != _searchId) return;
            LoadIndicator.Visibility = Visibility.Collapsed;
            TorrentView.SetTorrents(res);
        }

        private void ContinueButton_Click(object sender, RoutedEventArgs e)
        {
            Hide();
        }

        private void ForceButton_Click(object sender, RoutedEventArgs e)
        {
            HandleSearch(true);
        }

        private void CustomOpen_Click(object sender, RoutedEventArgs e)
        {
            if (string.IsNullOrEmpty(CustomFileText.Text)) return;
            File = new MediaItem()
            {
                Id = CurrentMovie.ImdbId,
                TmdbId = CurrentMovie.Id,
                Url = CustomFileText.Text,
                Image = TmdbMediaService.ImagePath300 + CurrentMovie.PosterPath,
                BackdropPath = TmdbMediaService.ImagePath300+CurrentMovie.BackdropPath,
                Description = CurrentMovie.Overview,
                Title = CurrentMovie.Title,
                Author = CurrentMovie.Status,
                MediaType = CustomFileText.Text.StartsWith("http") ? MediaType.Web : MediaType.Local,
                Rating = CurrentMovie.VoteCount/2,
                Year = CurrentMovie.ReleaseDate?.Year.ToString(),
                ViewCount = CurrentMovie.VoteCount.ToString(),
            };
            this.Hide();
        }
    }
}
