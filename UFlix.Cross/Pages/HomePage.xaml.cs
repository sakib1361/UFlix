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
            HorizontalView_OnMediaSelected(sender, args.ChosenSuggestion as MediaItem);
        }

        private async void SearchBox_TextChanged(AutoSuggestBox sender, AutoSuggestBoxTextChangedEventArgs args)
        {
            if (_locked == false && args.Reason == AutoSuggestionBoxTextChangeReason.UserInput)
            {
                _locked = true;
                SearchSuggestions.Clear();
                if (sender.Text.Length>2)
                {
                    ProgressLoadingRing.IsActive = true;
                    var movies = await _tmdbProvider.SearchMovie(sender.Text);
                    var shows = await _tmdbProvider.SearchShow(sender.Text);
                    ProgressLoadingRing.IsActive = false;
                    foreach (var item in movies)
                    {
                        SearchSuggestions.Add(item);
                    }
                    foreach (var item in shows)
                    {
                        SearchSuggestions.Add(item);
                    }
                }
                await Task.Delay(1000);
                _locked = false;
            }
        }

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