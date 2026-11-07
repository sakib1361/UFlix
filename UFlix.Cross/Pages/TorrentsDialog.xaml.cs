using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using System.Collections.ObjectModel;
using TMDbLib.Objects.Movies;
using UFlix.Cross.Services;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using UFlix.Engine.TorrentSearchEngines.Base;

namespace UFlix.Cross.Pages
{
    /// <summary>Every torrent in the session, played or downloaded, plus a by-name search whose results are always added as downloads.</summary>
    public sealed partial class TorrentsDialog : ContentDialog
    {
        public ObservableCollection<TorrentItem> Items => TorrentManager.Items;
        private readonly ObservableCollection<TorrentInfo> Results = [];
        private readonly TorrentSearchHandler SearchHandler = new();

        public TorrentsDialog()
        {
            this.InitializeComponent();
            ProviderCombo.ItemsSource = SearchHandler.Engines;
            ProviderCombo.SelectedIndex = 0;
            UpdateEmpty();
            Items.CollectionChanged += Items_CollectionChanged;
            Closed += (_, _) => Items.CollectionChanged -= Items_CollectionChanged;
        }

        private void Items_CollectionChanged(object sender, System.Collections.Specialized.NotifyCollectionChangedEventArgs e) => UpdateEmpty();

        private void UpdateEmpty()
        {
            EmptyText.Visibility = Items.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        }

        private void Pause_Click(object sender, RoutedEventArgs e)
        {
            if (sender is FrameworkElement { DataContext: TorrentItem item }) item.TogglePause();
        }

        private void Remove_Click(object sender, RoutedEventArgs e)
        {
            if (sender is FrameworkElement { DataContext: TorrentItem item }) TorrentManager.Remove(item);
        }

        private void SearchBox_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (e.Key != Windows.System.VirtualKey.Enter) return;
            e.Handled = true;
            Search();
        }

        private void Search_Click(object sender, RoutedEventArgs e) => Search();

        private async void Search()
        {
            var query = SearchBox.Text?.Trim();
            if (string.IsNullOrEmpty(query) || ProviderCombo.SelectedItem is not SearchEngineCommon engine) return;
            SearchHandler.Engine = engine;
            LoadIndicator.Visibility = Visibility.Visible;
            SearchEmptyText.Visibility = Visibility.Collapsed;
            Results.Clear();

            // Name-based providers search the text as typed; IMDb-based ones (Stremio) need the TMDB movie behind it.
            var res = await SearchHandler.SearchMovie(new Movie { Title = query }, false);
            if (res.Length == 0)
            {
                var movie = await TmdbMediaService.Instance.GetMovieInfo(new MediaItem { Title = query });
                if (movie != null) res = await SearchHandler.SearchMovie(movie, false);
            }

            foreach (var torrent in res.OrderBy(x => x.Quality)) Results.Add(torrent);
            LoadIndicator.Visibility = Visibility.Collapsed;
            SearchEmptyText.Text = "No results";
            SearchEmptyText.Visibility = Results.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        }

        private void DownloadResult_Click(object sender, RoutedEventArgs e)
        {
            if (sender is FrameworkElement { DataContext: TorrentInfo info }) Add(info);
        }

        private void PlayResult_Click(object sender, RoutedEventArgs e)
        {
            if (sender is not FrameworkElement { DataContext: TorrentInfo info }) return;
            Add(info);
            Hide();
            NavigationHelper.OpenPlayer(new MediaItem
            {
                Url = info.Url,
                Title = info.Title,
                MediaType = info.MediaType,
                ContentType = MediaContentType.Movie,
            });
        }

        /// <summary>Torrents become downloads before anything plays them, so closing the player keeps them downloading.</summary>
        private static void Add(TorrentInfo info)
        {
            if (info.MediaType == MediaType.Torrent) TorrentManager.Download(info.Url, info.Title);
        }
    }
}
