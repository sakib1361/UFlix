using Microsoft.UI.Xaml.Controls;
using System.Linq;
using UFlix.Engine.Models;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace UFlix.Cross.Pages
{
    /// <summary>
    /// An empty page that can be used on its own or navigated to within a Frame.
    /// </summary>
    public sealed partial class TorrentSelector : ContentDialog
    {
        private MediaItem _currentMedia;

        public string SelectedFile { get; private set; }
        public TorrentSelector(MediaItem currentMedia, string[] messages)
        {
            _currentMedia = currentMedia;
            this.InitializeComponent();
            Selector.ItemsSource = messages;

            if (currentMedia != null)
            {
                if (currentMedia.ContentType == MediaContentType.Movie)
                {
                    SelectedFile = messages.FirstOrDefault(x => new CustomFile(x).Name == currentMedia.Title);
                }
                else if (currentMedia.ContentType == MediaContentType.Shows)
                {
                    var mFile = new CustomFile(currentMedia.Title);
                    SelectedFile = messages.FirstOrDefault(x => new CustomFile(x).Episode == mFile.Episode);
                }
            }
            SelectedFile ??= messages.FirstOrDefault();
            Selector.SelectedItem = SelectedFile;
        }

        private void ContentDialog_PrimaryButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
        {
            SelectedFile = Selector.SelectedItem as string;
            if (_currentMedia.ContentType == MediaContentType.Shows)
            {
                var mFile = new CustomFile(_currentMedia.Title);
                var sFile = new CustomFile(SelectedFile);
                _currentMedia.Title = mFile.Title + $" S{sFile.Season:00} E{sFile.Episode:00}";
            }

            Hide();
        }
    }
}
