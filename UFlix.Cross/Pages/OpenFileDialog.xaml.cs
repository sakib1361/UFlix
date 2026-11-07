using Microsoft.UI.Xaml.Controls;
using System;
using System.IO;
using UFlix.Engine.Models;
using UFlix.Cross.Services;
using Windows.Storage.Pickers;

namespace UFlix.Cross.Pages
{
    /// <summary>
    /// An empty page that can be used on its own or navigated to within a Frame.
    /// </summary>
    public sealed partial class OpenFileDialog : ContentDialog
    {
        public OpenFileDialog()
        {
            this.InitializeComponent();
        }

        private async void OpenFileButton_Click(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        {
            var picker = new FileOpenPicker();

#if WINDOWS10_0_17763_0_OR_GREATER
            var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(NavigationHelper.GetWindow());
            WinRT.Interop.InitializeWithWindow.Initialize(picker, hwnd);
#endif
            foreach (var ext in new[] { ".mp4", ".mkv", ".webm", ".avi", ".mov", ".m4v", ".ts" })
                picker.FileTypeFilter.Add(ext);
            var file = await picker.PickSingleFileAsync();
            if (file != null)
            {
                var media = new MediaItem()
                {
                    Id = file.Path,
                    Title = file.Name,
                    Url = file.Path,
                    Image = file.Path,
                    FolderPath = Path.GetDirectoryName(file.Path),
                    MediaType = MediaType.Local,
                    ContentType = MediaContentType.Movie,
                };
                NavigationHelper.OpenPlayer(media);
                Hide();
            }
        }

        private void ContentDialog_PrimaryButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
        {
            if (MText.Text == null) return;
            var selectedMedia = new MediaItem()
            {
                Title = Path.GetFileNameWithoutExtension(Uri.UnescapeDataString(MText.Text)),
                ContentType = MediaContentType.Movie,
                MediaType = MText.Text.StartsWith("http") ? MediaType.Web : MediaType.Local,
            };
            selectedMedia.Url = MText.Text;
            if (selectedMedia.Url.StartsWith("magnet:?"))
            {
                selectedMedia.MediaType = MediaType.Torrent;
            }
            else if (selectedMedia.Url.Contains("youtube.com"))
            {
                selectedMedia.MediaType = MediaType.Youtube;
            }
            NavigationHelper.OpenPlayer(selectedMedia);
            Hide();
        }
    }
}
