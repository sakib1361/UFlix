using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Threading.Tasks;
using UFlix.Engine.MediaProviders;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using UFlix.Cross.Services;
using UFlix.Cross.Templates;


// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace UFlix.Cross.Pages
{
    /// <summary>
    /// An empty page that can be used on its own or navigated to within a Frame.
    /// </summary>
    public sealed partial class HomePage : Page
    {
        private readonly TmdbMediaService _tmdbProvider;
        private MediaItem[] _movieResults = [];
        private MediaItem[] _showResults = [];
        private MediaContentType _filter = MediaContentType.Movie;
        private bool _locked;

        public LoadingCollection<IMediaProvider> MediaProviders { get; }


        public ObservableCollection<MediaItem> SearchSuggestions { get; } = [];
        public HomePage()
        {
            InitializeComponent();
            MediaProviders = new LoadingCollection<IMediaProvider>(OnLoadRequested, 10);
            _tmdbProvider = new TmdbMediaService();
        }

        private async void HorizontalView_OnMediaSelected(object sender, MediaItem media)
        {
            ProgressLoadingRing.IsActive = true;

            if (media.ContentType == MediaContentType.Movie)
            {
                var movie = await _tmdbProvider.GetMovieInfo(media);
                if(movie == null)
                {
                    NavigationHelper.OpenPlayer(media);
                }
                else
                {
                    var dlg = new LibraryMovieDialog(movie)
                    {
                        XamlRoot = this.XamlRoot
                    };
                    if (media.Position>0)
                    {
                        dlg.File = media;
                    }
                    _=await dlg.ShowAsync();
                    if (dlg.File != null)
                    {
                        NavigationHelper.OpenPlayer(dlg.File);
                    }
                }
               
            }
            else if (media.ContentType == MediaContentType.Shows)
            {
                var show = await _tmdbProvider.GetShowInfo(media);
                if (show == null)
                {
                    ProgressLoadingRing.IsActive = false;
                    return;
                }
                var dlg = new LibraryShowDialog(show)
                {
                    XamlRoot = this.XamlRoot
                };
                if (media.Position>0)
                {
                    dlg.File = media;
                }
                _=await dlg.ShowAsync();
                if (dlg.File != null)
                {
                    NavigationHelper.OpenPlayer(dlg.File);
                }
            }
            else
            {
                NavigationHelper.OpenPlayer(media);
            }
            ProgressLoadingRing.IsActive = false;
        }

        private async Task<IMediaProvider[]> OnLoadRequested(int page)
        {
            var subList = new List<IMediaProvider>();
            if (page == 0)
            {
                var provider = new WatchlistProvider();
                var res = await provider.LoadMoreItems(0);

                if (res.Length>0)
                    subList.Add(provider);
            }
            await LoadGenres();
            subList.AddRange(AppConstants.Providers.Skip(page*MediaProviders.ItemCount).Take(MediaProviders.ItemCount));
            return [.. subList];
        }


        private async Task LoadGenres()
        {
            var genreLoaded = AppConstants.Providers.Any(x => x is MovieGenreProvider);
            if (genreLoaded) { return; }
            var movieGenres = await _tmdbProvider.GetMovieGenre();
            var showGenres = await _tmdbProvider.GetShowGenre();
            var max = Math.Max(movieGenres.Length, showGenres.Length);
            for (var i = 0; i < max; i++)
            {
                if (movieGenres.Length>i)
                {
                    var movieGProvider = new MovieGenreProvider(movieGenres[i]);
                    AppConstants.Providers.Add(movieGProvider);
                }
                if (showGenres.Length>i)
                {
                    var showGProvider = new ShowGenreProvider(showGenres[i]);
                    AppConstants.Providers.Add(showGProvider);
                }
            }
        }

        private void HorizontalView_OnExploreClicked(object sender, IMediaProvider e)
        {
            NavigationHelper.Navigate(typeof(DiscoverPage), e);
        }

        private void SearchBox_QuerySubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args)
        {
            // Enter without picking a suggestion opens the first one in the current tab
            var media = args.ChosenSuggestion as MediaItem ?? SearchSuggestions.FirstOrDefault();
            if (media != null) HorizontalView_OnMediaSelected(sender, media);
        }

        private async void SearchBox_TextChanged(AutoSuggestBox sender, AutoSuggestBoxTextChangedEventArgs args)
        {
            if (_locked == false && args.Reason == AutoSuggestionBoxTextChangeReason.UserInput)
            {
                _locked = true;
                await LoadSuggestions(sender.Text);
                await Task.Delay(1000);
                _locked = false;
            }
        }

        private async Task LoadSuggestions(string text)
        {
            if (text.Length > 2)
            {
                ProgressLoadingRing.IsActive = true;
                var movies = _tmdbProvider.SearchMovie(text);
                var shows = _tmdbProvider.SearchShow(text);
                await Task.WhenAll(movies, shows);
                ProgressLoadingRing.IsActive = false;
                _movieResults = movies.Result ?? [];
                _showResults = shows.Result ?? [];
                // keep the chosen tab, unless it came back empty and the other one didn't
                if (_filter == MediaContentType.Movie && _movieResults.Length == 0 && _showResults.Length > 0) _filter = MediaContentType.Shows;
                else if (_filter == MediaContentType.Shows && _showResults.Length == 0 && _movieResults.Length > 0) _filter = MediaContentType.Movie;
            }
            else
            {
                _movieResults = _showResults = [];
            }
            ApplyFilter();
        }

        /// <summary>Fills the suggestion list from the Movies or Shows results and updates the two tab buttons.</summary>
        private void ApplyFilter()
        {
            // replaced in place: going through an empty list would make the box close it
            var items = _filter == MediaContentType.Shows ? _showResults : _movieResults;
            for (var i = 0; i < items.Length; i++)
            {
                if (i < SearchSuggestions.Count) SearchSuggestions[i] = items[i];
                else SearchSuggestions.Add(items[i]);
            }
            while (SearchSuggestions.Count > items.Length) SearchSuggestions.RemoveAt(SearchSuggestions.Count - 1);
            MoviesFilter.Content = $"Movies ({_movieResults.Length})";
            ShowsFilter.Content = $"Shows ({_showResults.Length})";
            MoviesFilter.IsChecked = _filter == MediaContentType.Movie;
            ShowsFilter.IsChecked = _filter == MediaContentType.Shows;
            TypeFilter.Visibility = _movieResults.Length + _showResults.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
        }

        private void Filter_Click(object sender, RoutedEventArgs e) => SelectFilter(sender == ShowsFilter ? MediaContentType.Shows : MediaContentType.Movie);

        private void SelectFilter(MediaContentType type)
        {
            _filter = type;
            ApplyFilter();
            SearchBox.IsSuggestionListOpen = SearchSuggestions.Count > 0;
        }

        /// <summary>ScreenshotTour: fills the search box and opens its suggestion list as if the text had been typed.</summary>
        internal async Task ShowSuggestions(string text, bool open)
        {
            SearchBox.Text = text;
            if (open) await LoadSuggestions(text);
            else
            {
                _movieResults = _showResults = [];
                ApplyFilter();
            }
            SearchBox.IsSuggestionListOpen = open;
        }

        /// <summary>ScreenshotTour: switches the open suggestion list to the Movies or Shows tab.</summary>
        internal void ShowSuggestionTab(MediaContentType type) => SelectFilter(type);

        public async Task RefreshWatchlist()
        {
            if (MediaProviders[0] is WatchlistProvider)
            {
                MediaProviders.RemoveAt(0);
            }
            var watchProvider = new WatchlistProvider();
            var res = await watchProvider.LoadMoreItems(0);
            if (res.Length > 0)
            {
                MediaProviders.Insert(0, watchProvider);
            }
        }
    }
}