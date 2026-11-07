using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using System;
using System.Collections.Generic;
using System.Linq;
using UFlix.Cross.Services;
using UFlix.Engine.Models;

namespace UFlix.Cross.Templates
{
    // Quality strip + one torrent list. Replaces a Pivot, which on Uno keeps the old
    // tabs' content drawn under the new ones when its items are replaced.
    public sealed partial class TorrentQualityView : UserControl
    {
        private Quality? _quality;

        public TorrentQualityView()
        {
            this.InitializeComponent();
        }

        public event EventHandler<TorrentInfo> TorrentSelected;

        public Thickness HeaderMargin
        {
            get => QualityList.Margin;
            set => QualityList.Margin = value;
        }

        public void SetTorrents(IEnumerable<TorrentInfo> torrents)
        {
            var groups = torrents.GroupBy(x => x.Quality).OrderBy(x => x.Key)
                .Select(x => new TorrentInfoCollection { Quality = x.Key, Torrents = x.ToArray() })
                .ToList();
            var keep = _quality;
            QualityList.ItemsSource = groups;
            QualityList.SelectedItem = groups.FirstOrDefault(x => x.Quality == keep) ?? groups.FirstOrDefault();
            if (groups.Count == 0) TorrentList.ItemsSource = null;
        }

        private void QualityList_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (QualityList.SelectedItem is TorrentInfoCollection group)
            {
                _quality = group.Quality;
                TorrentList.ItemsSource = group.Torrents;
            }
        }

        private void TorrentList_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (e.AddedItems.Count > 0 && e.AddedItems[0] is TorrentInfo info) TorrentSelected?.Invoke(this, info);
        }

        private void DownloadTorrent_Click(object sender, RoutedEventArgs e)
        {
            if (sender is FrameworkElement { DataContext: TorrentInfo info }) TorrentManager.Download(info.Url, info.Title);
        }
    }
}
