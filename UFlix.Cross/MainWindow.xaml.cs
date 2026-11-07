using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using System;
using System.Threading.Tasks;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using UFlix.Cross.Templates;
using UFlix.Cross.Pages;
using UFlix.Cross.Services;

namespace UFlix.Cross
{
    /// <summary>
    /// An empty window that can be used on its own or navigated to within a Frame.
    /// </summary>
    public sealed partial class MainWindow : Window
    {
        private bool _Loaded;
        private bool _fullScreen;
        private bool _locked;

        static MainWindow()
        {
            DBUtilities.Instance.Open();
            AppConstants.Init();
        }
        public MainWindow()
        {
            this.InitializeComponent();
        }

        private void Window_Activated(object sender, WindowActivatedEventArgs args)
        {
            if (!_Loaded)
            {
                _Loaded = true;
                Navigate(typeof(HomePage));
                TorrentManager.Initialize(DispatcherQueue);
#if WINDOWS10_0_17763_0_OR_GREATER
                AppWindow.TitleBar.BackgroundColor = Colors.Black;
                AppWindow.TitleBar.IconShowOptions = IconShowOptions.HideIconAndSystemMenu;
#else
                if (OperatingSystem.IsLinux())
                    AppWindow.SetIcon(Path.Combine(AppContext.BaseDirectory, "Assets", "uflix.png"));
#endif
                (AppWindow.Presenter as OverlappedPresenter)?.Maximize();
            }
        }

        public void Notify(string message)
        {
            _=DispatcherQueue.TryEnqueue(() =>
            {
                IAP.Show(message);
            });

        }

        public void OpenPlayer(UserControl control)
        {
            NavView.Visibility = Visibility.Collapsed;
            PlayerView.Content = control;
            PlayerView.Visibility = Visibility.Visible;
        }

        public async void StopPlayer()
        {
            NavView.Visibility = Visibility.Visible;
            PlayerView.Visibility = Visibility.Collapsed;
            PlayerView.Content = null;
            if (MFrame.Content is HomePage page)
            {
                await page.RefreshWatchlist();
            }
            else if (MFrame.Content is YoutubePage youtube)
            {
                youtube.RefreshContinueWatching();
            }
            ExitFullScreen();
        }

        public void Navigate(Type type, object param = null)
        {
            _=MFrame.Navigate(type, param);
        }

        public void EnterFullScreen()
        {
            if (_fullScreen) return;
            _fullScreen = true;
            this.SetTitleBar(null);
            AppWindow.SetPresenter(AppWindowPresenterKind.FullScreen);
            //if (AppWindow.Presenter is OverlappedPresenter overlappedPresenter)
            //{
            //    overlappedPresenter.Restore();
            //    overlappedPresenter.SetBorderAndTitleBar(false, false);
            //    overlappedPresenter.Maximize();
            //}
        }

        public void ExitFullScreen()
        {
            if (!_fullScreen) return;
            _fullScreen = false;
            AppWindow.SetPresenter(AppWindowPresenterKind.Default);
            //if (AppWindow.Presenter is OverlappedPresenter overlappedPresenter)
            //{
            //    overlappedPresenter.Restore();
            //    overlappedPresenter.SetBorderAndTitleBar(true, true);
            //    overlappedPresenter.Maximize();
            //}
        }

        private void NavView_BackRequested(NavigationView sender, NavigationViewBackRequestedEventArgs args)
        {
            if (MFrame.CanGoBack) MFrame.GoBack();
        }

        private async void NavView_ItemInvoked(NavigationView sender, NavigationViewItemInvokedEventArgs args)
        {
            if (_locked) return;
            var selectedItem = args.InvokedItemContainer as CustomNavItem;
            var targetPageType = selectedItem?.PageType;
            if (targetPageType.BaseType == typeof(Page))
                _=MFrame.Navigate(targetPageType);
            else if (targetPageType.BaseType == typeof(ContentDialog))
            {
                var dlg = Activator.CreateInstance(targetPageType) as ContentDialog;
                dlg.XamlRoot = Content.XamlRoot;
                _=await dlg.ShowAsync();
            }
        }

        private async void MFrame_Navigated(object sender, Microsoft.UI.Xaml.Navigation.NavigationEventArgs e)
        {
            await Task.Delay(500);
            _locked = true;
            var ctType = MFrame.CurrentSourcePageType;
            foreach (var item in NavView.MenuItems)
            {
                if (item is CustomNavItem navItem)
                {
                    navItem.IsSelected = navItem.PageType == ctType;
                }
            }
            _locked = false;
        }
    }
}
