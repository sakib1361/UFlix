using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using System;
using System.Collections.Generic;
using UFlix.Cross.Services;
using UFlix.Engine.Services;
using UFlix.Engine.SubtitleSearchEngine;
using UFlix.Engine.TorrentSearchEngines.Base;
using Velopack;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace UFlix.Cross.Pages
{
    /// <summary>
    /// An empty page that can be used on its own or navigated to within a Frame.
    /// </summary>
    public sealed partial class SettingsDialog : ContentDialog
    {
        public SettingsDialog()
        {
            this.InitializeComponent();
            CurrentEngines = new TorrentSearchHandler().Engines;
            ShowUpdateState();
        }

        public List<SearchEngineCommon> CurrentEngines { get; }
        public DBUtilities Settings => DBUtilities.Instance;
        public PlayerSettings Player => PlayerSettings.Instance;
        public AppUpdater Updates => AppUpdater.Instance;
        public IEnumerable<string> SubtitleProviders => SubtitleSearchHandler.SubtitleProviders.Keys;

        private void ShowUpdateState()
        {
            VersionText.Text = $"UFlix {Updates.CurrentVersion}";
            if (Updates.HasFeed)
                ReleasesLink.NavigateUri = new Uri(Updates.ReleasesUrl);

            if (!Updates.CanSelfUpdate)
            {
                AutoCheckBox.IsEnabled = false;
                CheckButton.IsEnabled = false;
                UpdateStatus.Text = Updates.HasFeed
                    ? "This copy wasn't installed by Setup.exe or run as the AppImage, so it can't update itself. Get new versions from the releases page."
                    : "Local build: no update feed.";
                ReleasesLink.Visibility = Updates.HasFeed ? Visibility.Visible : Visibility.Collapsed;
                return;
            }
            if (Updates.Pending != null) ShowAvailable(Updates.Pending);
        }

        private void ShowAvailable(UpdateInfo update)
        {
            UpdateStatus.Text = $"UFlix {update.TargetFullRelease.Version} is available.";
            InstallButton.Visibility = Visibility.Visible;
            ReleasesLink.Visibility = Visibility.Visible;
        }

        private async void CheckButton_Click(object sender, RoutedEventArgs e)
        {
            CheckButton.IsEnabled = false;
            UpdateStatus.Text = "Checking…";
            try
            {
                var update = await Updates.CheckNowAsync();
                if (update != null) ShowAvailable(update);
                else UpdateStatus.Text = "You have the latest version.";
            }
            catch (Exception ex)
            {
                UpdateStatus.Text = $"Couldn't check for updates: {ex.Message}";
            }
            CheckButton.IsEnabled = true;
        }

        private async void InstallButton_Click(object sender, RoutedEventArgs e)
        {
            InstallButton.IsEnabled = false;
            CheckButton.IsEnabled = false;
            UpdateProgress.Value = 0;
            UpdateProgress.Visibility = Visibility.Visible;
            UpdateStatus.Text = "Downloading… UFlix restarts when it's done.";
            try
            {
                await Updates.InstallAsync(p => DispatcherQueue.TryEnqueue(() => UpdateProgress.Value = p));
            }
            catch (Exception ex)
            {
                UpdateStatus.Text = $"Update failed: {ex.Message}";
                UpdateProgress.Visibility = Visibility.Collapsed;
                InstallButton.IsEnabled = true;
                CheckButton.IsEnabled = true;
            }
        }
    }
}
