using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using UFlix.Cross.Pages;
using UFlix.Cross.Services;
using UFlix.Engine.Models;
using UFlix.Engine.Services;

namespace UFlix.Cross.Player
{
    public sealed partial class PlayerPage : UserControl
    {
        private readonly MediaContainer mediaContainer = new();
        private readonly WatchlistService _container = new();
        private readonly YTEngine _yt = new();
        private readonly TmdbMediaService _videoService = new();
        private readonly TaskCompletionSource _ready = new();
        private readonly DispatcherQueueTimer _idleTimer;
        private MediaItem _playList;
        private bool _isFullScreen;
        private bool _stopped;
        private bool _opening;
        private bool _updatingSlider;
        private bool _cursorHidden;
        private bool _theater;
        private bool _pickingFile;
        private MediaItem _openingMedia;
        private MediaItem _detailsFor;

        public PlayerPage()
        {
            this.InitializeComponent();
            Container.DataContext = mediaContainer;
            VolumePanel.DataContext = mediaContainer;
            BufferingPanel.DataContext = mediaContainer.Info;
            InfoPanel.DataContext = mediaContainer.Info;
            mediaContainer.MediaFinished += MediaContainer_MediaFinished;
            mediaContainer.MediaOpened += MediaContainer_MediaOpened;
            mediaContainer.TracksUpdated += (_, _) => UpdateStreams();
            mediaContainer.PositionTick += (_, _) => UpdateTimeline();
            mediaContainer.PropertyChanged += (_, e) =>
            {
                if (e.PropertyName is nameof(MediaContainer.IsBuffering)) UpdateBusy();
                else if (e.PropertyName is nameof(MediaContainer.Duration)) UpdateTimeline();
            };

            // one-shot, restarted on activity: nothing ticks while the user is idle
            _idleTimer = DispatcherQueue.CreateTimer();
            _idleTimer.Interval = TimeSpan.FromSeconds(3);
            _idleTimer.IsRepeating = false;
            _idleTimer.Tick += (_, _) => OnIdle();
            AutoplaySwitch.IsOn = PlayerSettings.Instance.YoutubeAutoplay;
            Loaded += PlayerPage_Loaded;
            // subscribed only while on screen, so the static event never keeps a closed player alive
            Loaded += (_, _) =>
            {
                NavigationHelper.FilePickerChanged -= OnFilePicker;
                NavigationHelper.FilePickerChanged += OnFilePicker;
            };
            Unloaded += (_, _) => NavigationHelper.FilePickerChanged -= OnFilePicker;
        }

        private void OnFilePicker(bool open)
        {
            _pickingFile = open;
            UpdateBusy();
        }

        private void PlayerPage_Loaded(object sender, RoutedEventArgs e)
        {
            if (_ready.Task.IsCompleted) return;
            try
            {
                mediaContainer.Init(Video);
                _idleTimer.Start();
                _ready.SetResult();
            }
            catch (Exception ex)
            {
                NavigationHelper.Notify($"mpv failed to start: {ex.Message}");
                _ready.SetException(ex);
            }
        }

        private void UpdateTimeline()
        {
            if (_stopped) return;
            _updatingSlider = true;
            MediaSlider.Maximum = Math.Max(mediaContainer.Duration.TotalSeconds, 0);
            MediaSlider.Value = Math.Min(mediaContainer.Position, MediaSlider.Maximum);
            _updatingSlider = false;
            DurationText.Text = mediaContainer.Duration.ToString(@"hh\:mm\:ss");
        }

        private void RestartIdleTimer()
        {
            _idleTimer.Stop();
            if (!_stopped) _idleTimer.Start();
        }

        private void OnIdle()
        {
            if (_stopped || _opening) return;
            if (_isFullScreen && ControllerPanel.Visibility == Visibility.Visible) SetControllerVisible(false);
            if (!_cursorHidden) SetCursorHidden(true);
        }

        private void MediaSlider_ValueChanged(object sender, RangeBaseValueChangedEventArgs e)
        {
            if (_updatingSlider || Math.Abs(e.NewValue - mediaContainer.Position) < 1.5) return;
            mediaContainer.Seek(e.NewValue);
        }

        private void UpdateBusy()
        {
            var busy = _opening || mediaContainer.IsBuffering;
            // while a torrent opens, CurrentMedia is still the previous item (or null), so use the item being opened
            var torrent = busy && (_openingMedia ?? mediaContainer.CurrentMedia)?.MediaType == MediaType.Torrent;
            ProgressIndicator.IsActive = busy;
            // the torrent file picker sits in the same spot: hide the buffering box behind it
            BufferingPanel.Visibility = torrent && !_pickingFile ? Visibility.Visible : Visibility.Collapsed;
            if (torrent) mediaContainer.StartUI();
            else if (InfoPanel.Visibility == Visibility.Collapsed) mediaContainer.CloseUI();
            if (busy) SetControllerVisible(true);
            else RestartIdleTimer();
        }

        private void SetOpening(bool opening)
        {
            _opening = opening;
            UpdateBusy();
        }

        private void MediaContainer_MediaOpened(object sender, EventArgs e)
        {
            UpdateStreams();
            LoadCaptions(mediaContainer.CurrentMedia);
            SetOpening(false);
            TitleText.Text = mediaContainer.Info.Title;
            UpdateTimeline();
            RestartIdleTimer();
        }

        private void MediaContainer_MediaFinished(object sender, EventArgs e)
        {
            DispatcherQueue.TryEnqueue(async () =>
            {
                if (_opening || _stopped) return;
                // e is null for the Next button, which always advances
                if (e != null && mediaContainer.CurrentMedia?.MediaType == MediaType.Youtube && !AutoplaySwitch.IsOn) return;
                SetOpening(true);
                var pos = MediaList.SelectedIndex;
                if (pos < MediaList.Items.Count - 1)
                {
                    SetOpening(false);
                    MediaList.SelectedIndex = pos + 1;
                }
                else if (mediaContainer.CurrentMedia.ContentType == MediaContentType.Shows)
                {
                    var hasMore = false;
                    if (mediaContainer.CurrentMedia.MediaType == MediaType.Torrent)
                        hasMore = await mediaContainer.OpenNextTorrent();
                    if (hasMore) SetOpening(false);
                    else ShowNext();
                }
                else
                {
                    Stop();
                }
            });
        }

        private async void ShowNext()
        {
            InfoPanel.Visibility = Visibility.Collapsed;
            mediaContainer.ClosePartials();
            var show = await _videoService.GetShowInfo(mediaContainer.CurrentMedia);
            var dlg = new LibraryShowDialog(show)
            {
                File = mediaContainer.CurrentMedia,
                XamlRoot = this.XamlRoot,
            };
            await dlg.ShowAsync();
            SetOpening(false);
            if (dlg.File != null && dlg.File != mediaContainer.CurrentMedia) Open(dlg.File);
            else Stop();
        }

        internal async void Open(MediaItem media)
        {
            try { await _ready.Task; }
            catch { NavigationHelper.StopPlayer(); return; }

            SidePanel.Visibility = Visibility.Collapsed;
            mediaContainer.Volume = DBUtilities.Instance.Volume is > 0 and var v ? v : 100;
            TitleText.Text = media.Title;

            if (media.MediaType == MediaType.PlayList)
            {
                _playList = media;
                NextButton.Visibility = Visibility.Visible;
                UpNextTitle.Text = media.Title;
                var medias = await _yt.GetPlaylistItem(media.Url, true);
                if (medias.Length == 0)
                {
                    NavigationHelper.Notify("Couldn't load the playlist");
                    NavigationHelper.StopPlayer();
                    return;
                }
                _playList.PlayListIndex = Math.Clamp(_playList.PlayListIndex, 0, medias.Length - 1);
                medias[_playList.PlayListIndex].Position = _playList.Position;
                MediaList.ItemsSource = medias;
                MediaList.SelectedIndex = _playList.PlayListIndex;
            }
            else if (media.MediaType == MediaType.Youtube)
            {
                NextButton.Visibility = Visibility.Visible;
                UpNextTitle.Text = "Up next";
                ShowDetails(media);
                MediaItem[] related;
                try { related = await YoutubeService.GetRelatedAsync(media); }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine(ex);
                    related = [];
                }
                MediaList.ItemsSource = new List<MediaItem>([media, .. related]);
                MediaList.SelectedIndex = 0;
            }
            else
            {
                MediaList.ItemsSource = new[] { media };
                MediaList.SelectedIndex = 0;
                _container.AddUpdate(media);
            }
        }

        private async void MediaList_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_opening || _stopped || MediaList.SelectedItem is not MediaItem media) return;
            _openingMedia = media;
            ShowDetails(media);
            SetOpening(true);
            await mediaContainer.Open(media);
            _openingMedia = null;
            SetOpening(false);
            if (_stopped) return;
            MediaList.ScrollIntoView(MediaList.SelectedItem, ScrollIntoViewAlignment.Leading);
            UpdateSidePanel();
            if (NextButton.Visibility == Visibility.Collapsed && mediaContainer.TotalTorrentFiles > 1)
                NextButton.Visibility = Visibility.Visible;
        }

        private void UpdateStreams()
        {
            FlyoutStreams.Items.Clear();
            foreach (var group in mediaContainer.MediaTracks.GroupBy(x => x.TrackType).OrderBy(x => x.Key))
            {
                foreach (var item in group)
                {
                    if (item.HasAction == false) continue;
                    var menuItem = new MenuFlyoutItem { Tag = item, Text = item.Description };
                    menuItem.Click += (_, _) =>
                    {
                        item.Execute();
                        UpdateSelectedItems();
                    };
                    FlyoutStreams.Items.Add(menuItem);
                }
                FlyoutStreams.Items.Add(new MenuFlyoutSeparator());
            }
            if (mediaContainer.CurrentMedia?.MediaType is MediaType.Local or MediaType.Torrent)
            {
                var download = new MenuFlyoutItem { Text = "Download subtitle" };
                download.Click += (_, _) => DownloadSubtitle();
                FlyoutStreams.Items.Add(download);
            }
            UpdateSelectedItems();
        }

        private void UpdateSelectedItems()
        {
            foreach (var menuItem in FlyoutStreams.Items)
            {
                if (menuItem is not MenuFlyoutItem flyOut || menuItem.Tag is not TrackItem track) continue;
                var selected = track.TrackType == TrackType.Audio ? mediaContainer.CurrentAudioTrack == track : mediaContainer.CurrentSubTrack == track;
                flyOut.Icon = selected ? new SymbolIcon { Symbol = Symbol.Accept } : null;
            }
        }

        private async void DownloadSubtitle()
        {
            mediaContainer.Pause();
            var dlg = new SubtitleDialog(mediaContainer.CurrentMedia) { XamlRoot = this.XamlRoot };
            await dlg.ShowAsync();
            if (dlg.SubtitleFile != null)
            {
                await mediaContainer.AddSubtitleFile(dlg.SubtitleFile);
                UpdateStreams();
            }
            mediaContainer.Play();
        }

        private void UpdatePosition()
        {
            if (_opening || _stopped || MediaList.SelectedItem is not MediaItem media || !mediaContainer.IsLoaded) return;
            if (_playList == null)
            {
                media.Position = (long)mediaContainer.Position;
                _container.AddUpdate(media);
            }
            else
            {
                _playList.Image = media.Image;
                _playList.Position = (long)mediaContainer.Position;
                _playList.PlayListIndex = MediaList.SelectedIndex;
                _container.AddUpdate(_playList);
            }
            DBUtilities.Instance.Volume = mediaContainer.Volume;
        }

        private async void Stop()
        {
            if (_stopped) return;
            mediaContainer.Pause();
            UpdatePosition();
            _stopped = true;
            _idleTimer.Stop();
            if (_isFullScreen) NavigationHelper.GetWindow().ExitFullScreen();
            await mediaContainer.Close();
            NavigationHelper.StopPlayer();
        }

        private void Stop_Click(object sender, RoutedEventArgs e) => Stop();

        /// <summary>ScreenshotTour: the playback state, and the info (debug) panel toggle.</summary>
        internal MediaContainer Media => mediaContainer;

        internal void ToggleInfo() => Info_Click(null, null);

        private void Info_Click(object sender, RoutedEventArgs e)
        {
            if (InfoPanel.Visibility == Visibility.Visible)
            {
                mediaContainer.CloseUI();
                InfoPanel.Visibility = Visibility.Collapsed;
            }
            else
            {
                mediaContainer.StartUI();
                InfoPanel.Visibility = Visibility.Visible;
            }
        }

        private void BtnFullScreen_Click(object sender, RoutedEventArgs e) => ToggleFullScreen();

        /// <summary>Windowed: the control bar sits below the video. Fullscreen: the video spans both rows and the opaque bar auto-hides over it.</summary>
        private void ToggleFullScreen()
        {
            _isFullScreen = !_isFullScreen;
            if (_isFullScreen) NavigationHelper.GetWindow().EnterFullScreen();
            else NavigationHelper.GetWindow().ExitFullScreen();
            Grid.SetRowSpan(Video, _isFullScreen ? 2 : 1);
            UpdateSidePanel();
            SetControllerVisible(true);
            RestartIdleTimer();
        }

        /// <summary>The side panel (details, up next, playlist) shows beside the video, never over it: hidden in fullscreen and theater mode.</summary>
        private void UpdateSidePanel()
        {
            var available = mediaContainer.HasPlaylist;
            TheaterButton.Visibility = available && !_isFullScreen ? Visibility.Visible : Visibility.Collapsed;
            SidePanel.Visibility = available && !_isFullScreen && !_theater ? Visibility.Visible : Visibility.Collapsed;
            if (TheaterButton.Icon is FontIcon icon) icon.Glyph = _theater ? "\uE8A0" : "\uE89F";
        }

        private void ToggleTheater()
        {
            if (!mediaContainer.HasPlaylist || _isFullScreen) return;
            _theater = !_theater;
            UpdateSidePanel();
        }

        private void Theater_Click(object sender, RoutedEventArgs e) => ToggleTheater();

        private void Theater_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
        {
            args.Handled = true;
            ToggleTheater();
        }

        // ---- YouTube details ----

        private async void ShowDetails(MediaItem media)
        {
            if (media.MediaType != MediaType.Youtube)
            {
                _detailsFor = null;
                DetailsPanel.Visibility = Visibility.Collapsed;
                return;
            }
            if (_detailsFor == media) return;
            _detailsFor = media;
            DetailsPanel.Visibility = Visibility.Visible;
            DetailsTitle.Text = media.Title;
            DetailsChannel.Text = media.Author;
            DetailsAvatar.Source = null;
            DetailsStats.Text = "";
            DetailsDescription.Text = "";
            SetDescriptionExpanded(false);
            try
            {
                var details = await YoutubeService.GetVideoAsync(media.Url);
                if (_detailsFor != media) return;
                DetailsTitle.Text = details.Title;
                DetailsChannel.Text = details.Author;
                DetailsStats.Text = details.Stats;
                DetailsDescription.Text = details.Description;
                MoreButton.Visibility = string.IsNullOrWhiteSpace(details.Description) ? Visibility.Collapsed : Visibility.Visible;
                var channel = await YoutubeService.GetChannelAsync(details.ChannelId);
                if (_detailsFor == media) DetailsAvatar.Source = channel.Avatar;
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine(ex);
            }
        }

        private void SetDescriptionExpanded(bool expanded)
        {
            DescriptionScroll.MaxHeight = expanded ? 260 : 72;
            DescriptionScroll.VerticalScrollMode = expanded ? ScrollMode.Enabled : ScrollMode.Disabled;
            DescriptionScroll.VerticalScrollBarVisibility = expanded ? ScrollBarVisibility.Auto : ScrollBarVisibility.Hidden;
            if (!expanded) DescriptionScroll.ChangeView(null, 0, null, true);
            MoreButton.Content = expanded ? "Show less" : "Show more";
        }

        private void MoreDescription_Click(object sender, RoutedEventArgs e) => SetDescriptionExpanded(DescriptionScroll.MaxHeight < 100);

        private void Autoplay_Toggled(object sender, RoutedEventArgs e) => PlayerSettings.Instance.YoutubeAutoplay = AutoplaySwitch.IsOn;

        // ---- captions (YouTube CC → SRT → mpv) and speed ----

        private async void LoadCaptions(MediaItem media)
        {
            CaptionsFlyout.Items.Clear();
            CaptionsButton.Visibility = media?.MediaType == MediaType.Youtube ? Visibility.Visible : Visibility.Collapsed;
            if (media?.MediaType != MediaType.Youtube) return;
            YoutubeExplode.Videos.ClosedCaptions.ClosedCaptionTrackInfo[] tracks;
            try
            {
                tracks = await YoutubeService.GetCaptionsAsync(media.Url);
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine(ex);
                tracks = [];
            }
            if (mediaContainer.CurrentMedia != media || _stopped) return;

            var off = new ToggleMenuFlyoutItem { Text = "Off", IsChecked = true };
            off.Click += (_, _) =>
            {
                mediaContainer.HideSubtitles();
                CheckCaption(off);
            };
            CaptionsFlyout.Items.Add(off);
            if (tracks.Length == 0)
            {
                CaptionsFlyout.Items.Add(new MenuFlyoutItem { Text = "No captions for this video", IsEnabled = false });
                return;
            }
            CaptionsFlyout.Items.Add(new MenuFlyoutSeparator());
            foreach (var track in tracks)
            {
                var item = new ToggleMenuFlyoutItem { Text = track.IsAutoGenerated ? $"{track.Language.Name} (auto-generated)" : track.Language.Name };
                item.Click += async (_, _) =>
                {
                    CheckCaption(item);
                    try
                    {
                        var file = await YoutubeService.DownloadCaptionAsync(media.Url, track);
                        if (mediaContainer.CurrentMedia == media) await mediaContainer.ShowCaption(file);
                    }
                    catch (Exception ex)
                    {
                        NavigationHelper.Notify($"Captions failed: {ex.Message}");
                    }
                };
                CaptionsFlyout.Items.Add(item);
            }
        }

        private void CheckCaption(ToggleMenuFlyoutItem selected)
        {
            foreach (var item in CaptionsFlyout.Items.OfType<ToggleMenuFlyoutItem>()) item.IsChecked = item == selected;
        }

        private void SpeedFlyout_Opening(object sender, object e)
        {
            foreach (var item in SpeedFlyout.Items.OfType<ToggleMenuFlyoutItem>())
                item.IsChecked = Math.Abs(double.Parse((string)item.Tag, System.Globalization.CultureInfo.InvariantCulture) - mediaContainer.Speed) < 0.01;
        }

        private void Speed_Click(object sender, RoutedEventArgs e)
        {
            if (sender is FrameworkElement { Tag: string tag }) mediaContainer.Speed = double.Parse(tag, System.Globalization.CultureInfo.InvariantCulture);
        }

        private void SetControllerVisible(bool visible)
        {
            if (!_isFullScreen) visible = true;
            ControllerPanel.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
            mediaContainer.SetSubtitleMargin(_isFullScreen && visible);
        }

        private void SetCursorHidden(bool hidden)
        {
            _cursorHidden = hidden;
            Video.SetCursorVisible(!hidden);
        }

        private void ShowController()
        {
            RestartIdleTimer();
            if (_cursorHidden) SetCursorHidden(false);
            if (ControllerPanel.Visibility == Visibility.Collapsed)
            {
                SetControllerVisible(true);
                UpdatePosition();
            }
        }

        private void Root_PointerMoved(object sender, PointerRoutedEventArgs e) => ShowController();

        // handled on the root: over the native video window (Linux) Uno may not attribute the tap to the VideoSurface element
        private void Root_DoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
        {
            for (var element = e.OriginalSource as DependencyObject; element != null; element = VisualTreeHelper.GetParent(element))
            {
                if (element == ControllerPanel || element == SidePanel || element == InfoPanel || element == BufferingPanel) return;
            }
            e.Handled = true;
            ToggleFullScreen();
        }

        private void ESC_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
        {
            args.Handled = true;
            if (_isFullScreen) ToggleFullScreen();
            else Stop();
        }

        private void Space_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
        {
            args.Handled = true;
            mediaContainer.TogglePause();
        }

        private void Right_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
        {
            args.Handled = true;
            mediaContainer.SeekBy(10);
        }

        private void Left_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
        {
            args.Handled = true;
            mediaContainer.SeekBy(-10);
        }

        private void Up_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
        {
            args.Handled = true;
            mediaContainer.ChangeVolume(5);
        }

        private void Down_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
        {
            args.Handled = true;
            mediaContainer.ChangeVolume(-5);
        }

        private void Fullscreen_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
        {
            args.Handled = true;
            ToggleFullScreen();
        }

        private void SuboffsetUp_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
        {
            args.Handled = true;
            mediaContainer.IncreaseSubOffset();
        }

        private void SuboffsetDown_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
        {
            args.Handled = true;
            mediaContainer.DecreaseSubOffset();
        }

        private void Next_Click(object sender, RoutedEventArgs e)
        {
            mediaContainer.Pause();
            MediaContainer_MediaFinished(sender, null);
        }
    }
}
