using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Navigation;
using System.Collections.ObjectModel;
using UFlix.Cross.Services;
using UFlix.Cross.Templates;
using UFlix.Engine.Services;
using Windows.ApplicationModel.DataTransfer;
using YoutubeExplode.Search;

namespace UFlix.Cross.Pages
{
    /// <summary>
    /// YouTube browser: a "For you" feed seeded by recent searches, topic chips, search with result filters, and channel uploads.
    /// Everything pages in from the scroll position; videos and playlists open in the player.
    /// </summary>
    public sealed partial class YoutubePage : Page
    {
        private const int PageSize = 24;

        private sealed record ViewState(string Query = null, SearchFilter Filter = SearchFilter.None, string Topic = null, string ChannelId = null, string ChannelTitle = null)
        {
            public bool IsHome => Query == null && ChannelId == null;
        }

        public ObservableCollection<string> SearchSuggestions { get; } = [];
        public ObservableCollection<YtItem> Items { get; } = [];
        public ObservableCollection<YtItem> ContinueItems { get; } = [];
        public ObservableCollection<string> History { get; } = [];

        private readonly YTEngine _yt = new();
        private readonly Stack<ViewState> _previous = new();
        private ViewState _view;
        private YtPager _pager;
        private ScrollViewer _scroll;
        private bool _loading;
        private bool _hasMore;
        private bool _syncingChips;
        private int _suggestVersion;
        private YtItem _menuItem;

        public YoutubePage()
        {
            InitializeComponent();
        }

        protected override void OnNavigatedTo(NavigationEventArgs e)
        {
            base.OnNavigatedTo(e);
            _previous.Clear();
            Show(new ViewState(), push: false);
        }

        protected override void OnNavigatedFrom(NavigationEventArgs e)
        {
            base.OnNavigatedFrom(e);
            _pager?.Dispose();
            _pager = null;
        }

        /// <summary>Called by the shell when the player closes, so progress and new entries show up.</summary>
        public void RefreshContinueWatching()
        {
            if (_view is { IsHome: true, Topic: null }) RefreshHomeSections();
        }

        // ---- views ----

        private void Show(ViewState view, bool push = true)
        {
            if (push && _view != null) _previous.Push(_view);
            _view = view;
            BackButton.Visibility = _previous.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
            SearchBox.Text = view.Query ?? "";

            _syncingChips = true;
            TopicChips.Visibility = view.IsHome ? Visibility.Visible : Visibility.Collapsed;
            FilterChips.Visibility = view.Query != null ? Visibility.Visible : Visibility.Collapsed;
            var chips = view.IsHome ? TopicChips : FilterChips;
            var tag = view.IsHome ? view.Topic : view.Filter.ToString();
            var chip = chips.Children.OfType<RadioButton>().FirstOrDefault(x => x.Tag as string == tag) ?? ForYouChip;
            chip.IsChecked = true;
            _syncingChips = false;

            ChannelHeader.Visibility = view.ChannelId != null ? Visibility.Visible : Visibility.Collapsed;
            if (view.ChannelId != null) LoadChannelHeader(view);
            if (view is { IsHome: true, Topic: null }) RefreshHomeSections();
            else ContinuePanel.Visibility = HistoryPanel.Visibility = Visibility.Collapsed;

            var (pager, title) = view switch
            {
                { ChannelId: { } id } => (YoutubeService.Uploads(id), "Videos"),
                { Query: { } q } => (YoutubeService.Search(q, view.Filter), $"Results for “{q}”"),
                { Topic: { } t } => (YoutubeService.Search(t, SearchFilter.Video), (chip.Content as string) ?? t),
                _ => (YoutubeService.ForYou(), "Recommended for you"),
            };
            Load(pager, title);
        }

        private void RefreshHomeSections()
        {
            ContinueItems.Clear();
            foreach (var item in YoutubeService.ContinueWatching()) ContinueItems.Add(item);
            ContinuePanel.Visibility = ContinueItems.Count > 0 ? Visibility.Visible : Visibility.Collapsed;

            History.Clear();
            foreach (var query in YoutubeService.SearchHistory) History.Add(query);
            HistoryPanel.Visibility = History.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
        }

        private async void LoadChannelHeader(ViewState view)
        {
            ChannelTitle.Text = view.ChannelTitle ?? "";
            ChannelAvatar.Source = null;
            try
            {
                var channel = await YoutubeService.GetChannelAsync(view.ChannelId);
                if (_view != view) return;
                ChannelTitle.Text = channel.Title;
                ChannelAvatar.Source = channel.Avatar;
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine(ex);
            }
        }

        private void ShowChannel(string channelId, string title)
        {
            if (string.IsNullOrEmpty(channelId) || _view?.ChannelId == channelId) return;
            Show(new ViewState(ChannelId: channelId, ChannelTitle: title));
        }

        private void ShowSearch(string query)
        {
            YoutubeService.AddToHistory(query);
            DBUtilities.Instance.LastSearch = query;
            Show(new ViewState(Query: query));
        }

        private void Open(YtItem item)
        {
            if (item.Kind == YtKind.Channel) ShowChannel(item.ChannelId ?? item.Id, item.Title);
            else NavigationHelper.OpenPlayer(item.ToMedia());
        }

        // ---- paging ----

        private void Load(YtPager pager, string title)
        {
            _pager?.Dispose();
            _pager = pager;
            _loading = false;
            _hasMore = true;
            Items.Clear();
            SectionTitle.Text = title;
            EmptyState.Visibility = Visibility.Collapsed;
            _scroll?.ChangeView(null, 0, null, true);
            _ = LoadMore();
        }

        private async Task LoadMore()
        {
            var pager = _pager;
            if (_loading || !_hasMore || pager == null) return;
            _loading = true;
            LoadingRing.IsActive = Items.Count == 0;
            MoreRing.IsActive = Items.Count > 0;
            var failed = false;
            try
            {
                var page = await pager.NextAsync(PageSize);
                if (pager != _pager) return;
                foreach (var item in page) Items.Add(item);
                // a short page means every source has ended
                _hasMore = page.Length == PageSize;
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch (Exception ex)
            {
                if (pager != _pager) return;
                _hasMore = false;
                failed = true;
                System.Diagnostics.Debug.WriteLine(ex);
            }
            finally
            {
                if (pager == _pager)
                {
                    _loading = false;
                    LoadingRing.IsActive = MoreRing.IsActive = false;
                }
            }

            if (Items.Count == 0)
            {
                EmptyText.Text = failed ? "Couldn't reach YouTube. Check the connection and try again." : "Nothing found.";
                EmptyState.Visibility = Visibility.Visible;
                return;
            }
            // a page that doesn't fill the viewport raises no scroll event, so ask for the next one now
            ResultsGrid.UpdateLayout();
            if (NearEnd()) await LoadMore();
        }

        private bool NearEnd() => _scroll != null && _scroll.ScrollableHeight - _scroll.VerticalOffset < 900;

        // Uno's GridView never asks ISupportIncrementalLoading for more, so page in from the scroll position
        private void ResultsGrid_Loaded(object sender, RoutedEventArgs e)
        {
            if (_scroll != null || HorizontalView.FindDescendant<ScrollViewer>(ResultsGrid, _ => true) is not { } scroll) return;
            _scroll = scroll;
            _scroll.ViewChanged += (_, _) =>
            {
                if (NearEnd()) _ = LoadMore();
            };
        }

        // ---- header ----

        private async void SearchBox_QuerySubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args)
        {
            var query = (args.ChosenSuggestion as string ?? args.QueryText)?.Trim();
            if (string.IsNullOrEmpty(query)) return;
            _suggestVersion++;
            SearchSuggestions.Clear();
            if (query.Contains("youtube.com/") || query.Contains("youtu.be/"))
            {
                try
                {
                    if (await YoutubeService.ResolveAsync(query) is { } item)
                    {
                        Open(item);
                        return;
                    }
                }
                catch (Exception ex)
                {
                    NavigationHelper.Notify(ex.Message);
                    return;
                }
            }
            ShowSearch(query);
        }

        private async void SearchBox_TextChanged(AutoSuggestBox sender, AutoSuggestBoxTextChangedEventArgs args)
        {
            if (args.Reason != AutoSuggestionBoxTextChangeReason.UserInput) return;
            var text = sender.Text;
            var version = ++_suggestVersion;
            if (string.IsNullOrWhiteSpace(text) || text.Length < 2 || text.Contains("://"))
            {
                SearchSuggestions.Clear();
                return;
            }
            var suggestions = await _yt.GetSuggestions(Uri.EscapeDataString(text));
            if (version != _suggestVersion) return;
            SearchSuggestions.Clear();
            foreach (var item in suggestions.Take(10)) SearchSuggestions.Add(item);
        }

        private void Back_Click(object sender, RoutedEventArgs e)
        {
            if (_previous.TryPop(out var view)) Show(view, push: false);
        }

        private void Home_Click(object sender, RoutedEventArgs e)
        {
            _previous.Clear();
            Show(new ViewState(), push: false);
        }

        private void Topic_Checked(object sender, RoutedEventArgs e)
        {
            if (_syncingChips || _view == null) return;
            Show(new ViewState(Topic: (sender as FrameworkElement)?.Tag as string), push: false);
        }

        private void Filter_Checked(object sender, RoutedEventArgs e)
        {
            if (_syncingChips || _view?.Query == null) return;
            if (Enum.TryParse<SearchFilter>((sender as FrameworkElement)?.Tag as string, out var filter))
                Show(_view with { Filter = filter }, push: false);
        }

        private void History_Click(object sender, RoutedEventArgs e)
        {
            if ((sender as FrameworkElement)?.Tag is string query) ShowSearch(query);
        }

        private void HistoryRemove_Click(object sender, RoutedEventArgs e)
        {
            if ((sender as FrameworkElement)?.Tag is not string query) return;
            YoutubeService.RemoveFromHistory(query);
            History.Remove(query);
            HistoryPanel.Visibility = History.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
        }

        // ---- cards ----

        private void Card_ItemClick(object sender, ItemClickEventArgs e)
        {
            if (e.ClickedItem is YtItem item) Open(item);
        }

        private void Author_Click(object sender, RoutedEventArgs e)
        {
            if ((sender as FrameworkElement)?.DataContext is YtItem item) ShowChannel(item.ChannelId, item.Kind == YtKind.Channel ? item.Title : item.Author);
        }

        private void More_Click(object sender, RoutedEventArgs e)
        {
            if (sender is FrameworkElement element) ShowMenu(element, null);
        }

        private void Card_RightTapped(object sender, RightTappedRoutedEventArgs e)
        {
            if (sender is not FrameworkElement element) return;
            e.Handled = true;
            ShowMenu(element, e.GetPosition(element));
        }

        private void ShowMenu(FrameworkElement element, Windows.Foundation.Point? position)
        {
            if (element.DataContext is not YtItem item) return;
            _menuItem = item;
            var menu = (MenuFlyout)Resources["CardMenu"];
            // "Go to channel" only where a channel is known and it isn't the one on screen
            menu.Items[1].Visibility = item.ChannelId != null && item.Kind != YtKind.Channel && item.ChannelId != _view?.ChannelId ? Visibility.Visible : Visibility.Collapsed;
            menu.ShowAt(element, new FlyoutShowOptions { Position = position });
        }

        private static string LinkOf(YtItem item) => item.Kind switch
        {
            YtKind.Playlist => $"https://www.youtube.com/playlist?list={item.Id}",
            YtKind.Channel => $"https://www.youtube.com/channel/{item.Id}",
            _ => $"https://www.youtube.com/watch?v={item.Id}",
        };

        private void MenuPlay_Click(object sender, RoutedEventArgs e)
        {
            if (_menuItem != null) Open(_menuItem);
        }

        private void MenuChannel_Click(object sender, RoutedEventArgs e)
        {
            if (_menuItem != null) ShowChannel(_menuItem.ChannelId, _menuItem.Author);
        }

        private void MenuCopy_Click(object sender, RoutedEventArgs e)
        {
            if (_menuItem == null) return;
            var package = new DataPackage();
            package.SetText(LinkOf(_menuItem));
            Clipboard.SetContent(package);
            NavigationHelper.Notify("Link copied");
        }

        private async void MenuBrowser_Click(object sender, RoutedEventArgs e)
        {
            if (_menuItem != null) await Windows.System.Launcher.LaunchUriAsync(new Uri(LinkOf(_menuItem)));
        }
    }
}
