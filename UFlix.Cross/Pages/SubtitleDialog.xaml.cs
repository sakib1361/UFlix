using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using System.Collections.ObjectModel;
using System.IO;
using UFlix.Engine.Models;
using UFlix.Engine.SubtitleSearchEngine;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace UFlix.Cross.Pages
{
    public sealed partial class SubtitleDialog : ContentDialog
    {
        public MediaItem Movie { get; }

        public string SubtitleFile { get; private set; }
        private readonly SubtitleSearchHandler _subService;
        public ObservableCollection<SubtitleFile> Subtitles { get; } = [];
        public SubtitleDialog(MediaItem movie)
        {
            Movie = movie;
            _subService = new SubtitleSearchHandler();
            this.InitializeComponent();
        }

        private async void ContentDialog_Loaded(object sender, RoutedEventArgs e)
        {
            PRing.Visibility = Visibility.Visible;
            var results = await _subService.SearchSubtitle(Movie);
            if (results != null)
            {
                Subtitles.Clear();
                foreach (var result in results)
                {
                    Subtitles.Add(result);
                }
            }
            PRing.Visibility = Visibility.Collapsed;
        }

        private async void Subtitle_Button_Click(object sender, RoutedEventArgs e)
        {
            if (sender is Button button && button.Tag is SubtitleFile subtitleFile)
            {
                var filePath = File.Exists(Movie.Url) ? Path.ChangeExtension(Movie.Url, "srt") : Movie.Title.Replace(":", "")+".srt";
                if (!string.IsNullOrEmpty(Movie.FolderPath))
                {
                    filePath = Path.Combine(Movie.FolderPath, filePath);
                }
                if (File.Exists(filePath)==false)
                {
                    var curFile = await subtitleFile.GetFile();
                    if (curFile != null)
                    {
                        File.Move(curFile, filePath);
                        SubtitleFile = filePath;
                        this.Hide();
                    }
                    PRing.Visibility = Visibility.Collapsed;
                }
                else
                {
                    SubtitleFile = filePath;
                    this.Hide();
                }
            }
        }
    }
}
