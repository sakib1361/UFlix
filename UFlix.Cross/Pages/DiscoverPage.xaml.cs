using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
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
    public sealed partial class DiscoverPage : Page
    {
        public LoadingCollection<MediaItem> Medias { get; }

        private readonly List<IMediaProvider> Providers = [];
        private readonly MediaContentType[] ContentTypes = Enum.GetValues<MediaContentType>();
        private readonly ObservableCollection<string> Genres = [];
        private readonly TmdbMediaService _service = new();
        public IMediaProvider Current { get; set; }

        private bool _locked = true;
        public DiscoverPage()
        {
            this.InitializeComponent();
            Providers.AddRange(AppConstants.Providers);
            Medias = new LoadingCollection<MediaItem>(LoadFunction);
        }

        protected override void OnNavigatedTo(NavigationEventArgs e)
        {
            base.OnNavigatedTo(e);

            if (e.Parameter is IMediaProvider provider)
            {
                Current = provider;
            }
            else
            {
                Current = Providers.First();
            }

            MediaContentTypeCombo.SelectedItem = Current.ContentType;
            _locked = false;
        }

        // Uno's GridView never asks ISupportIncrementalLoading for more, so page in from the scroll position
        private void MediaGrid_Loaded(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        {
            if (HorizontalView.FindDescendant<ScrollViewer>(MediaGrid, _ => true) is { } scroll)
            {
                scroll.ViewChanged -= Scroll_ViewChanged;
                scroll.ViewChanged += Scroll_ViewChanged;
            }
        }

        private void Scroll_ViewChanged(object sender, ScrollViewerViewChangedEventArgs e)
        {
            if (sender is not ScrollViewer scroll || Medias.IsLoading || !Medias.HasMoreItems) return;
            if (scroll.VerticalOffset + scroll.ViewportHeight >= scroll.ExtentHeight - scroll.ViewportHeight)
            {
                _ = Medias.LoadMoreItemsAsync((uint)Medias.ItemCount);
            }
        }

        private async Task<MediaItem[]> LoadFunction(int arg)
        {
            ProgressLoadingRing.IsActive = true;
            if (Current == null) { return Array.Empty<MediaItem>(); }
            var res = await Current.LoadMoreItems(arg);
            ProgressLoadingRing.IsActive = false;
            return res;
        }

        private void MediaContentType_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            _locked = true;
            Genres.Clear();
            var contentType = ContentTypes[MediaContentTypeCombo.SelectedIndex];
            foreach (var item in Providers.Where(x => x.ContentType == contentType))
            {
                Genres.Add(item.Name);
            }
            var genre = Genres.FirstOrDefault(x => x == Current.Name);
            genre ??= Genres.FirstOrDefault();
            GenreCombo.SelectedItem = genre;
            _locked = false;
            UpdateCombo();

        }

        private void GenreCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            UpdateCombo();
        }

        private async void UpdateCombo()
        {
            if (_locked || MediaContentTypeCombo.SelectedIndex<0 || GenreCombo.SelectedIndex<0) return;
            _locked = true;

            var contentType = ContentTypes[MediaContentTypeCombo.SelectedIndex];
            var genreType = Genres[GenreCombo.SelectedIndex];
            var provider = Providers.FirstOrDefault(x => x.ContentType == contentType && x.Name == genreType);
            provider ??= Providers.FirstOrDefault();
            if (Current != provider)
            {
                Current = provider;
                await Medias.Reset();
            }
            _locked = false;
        }

        private async void GridView_ItemClick(object sender, ItemClickEventArgs e)
        {
            if (e.ClickedItem is not MediaItem media) return;
            ProgressLoadingRing.IsActive = true;
            if (media.ContentType == MediaContentType.Movie)
            {
                var movie = await _service.GetMovieInfo(media);
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
            else if (media.ContentType == MediaContentType.Shows)
            {
                var show = await _service.GetShowInfo(media);
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
            ProgressLoadingRing.IsActive = false;
        }
    }
}
