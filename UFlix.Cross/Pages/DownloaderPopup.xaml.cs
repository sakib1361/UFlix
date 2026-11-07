using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using System.Collections.ObjectModel;
using System.Linq;
using UFlix.Cross.Services;
using YoutubeDLSharp.Metadata;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace UFlix.Cross.Pages
{
    /// <summary>
    /// An empty page that can be used on its own or navigated to within a Frame.
    /// </summary>
    public sealed partial class DownloaderDialog : ContentDialog
    {
        private readonly Downloader _downloader;
        private VideoData videoData;

        public ObservableCollection<DownloadOperation> Downloads => Downloader.Downloads;
        public DownloaderDialog()
        {
            this.InitializeComponent();
            _downloader = new Downloader();
        }

        private async void ParseButton_Click(object sender, RoutedEventArgs e)
        {
            if (string.IsNullOrWhiteSpace(UriBox.Text)) return;
            ParseButton.IsEnabled = false;
            videoData = await _downloader.GetFormats(UriBox.Text);
            if (videoData != null)
            {
                var formats = videoData.Formats.Select(x => x.ToString());
                FormatBox.ItemsSource = formats;
                FormatBox.SelectedIndex = 0;
            };
            ParseButton.IsEnabled = true;
        }

        private void Downloader_Click(object sender, RoutedEventArgs e)
        {
            var format = FormatBox.SelectedItem as string;
            var currectFormat = videoData.Formats.FirstOrDefault(x => x.ToString() == format);
            if (currectFormat == null || videoData.Formats.Length == 1)
            {
                _downloader.AddDownload(UriBox.Text, videoData.Title, null, currectFormat);
            }
            else
            {
                _downloader.AddDownload(UriBox.Text, videoData.Title, currectFormat.FormatId, currectFormat);
            }
            FormatBox.ItemsSource = null;
            UriBox.Text = null;
        }

        private void Del_Click(object sender, RoutedEventArgs e)
        {
            if (sender is Button button && button.DataContext is DownloadOperation operation)
            {
                operation.Stop();
                _=Downloads.Remove(operation);
            }
        }
    }
}
