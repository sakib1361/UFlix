using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using System;
using System.Linq;
using System.Threading;
using UFlix.Engine.MediaProviders;
using UFlix.Engine.Models;
using UFlix.Engine.Services;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace UFlix.Cross.Templates
{
    public sealed partial class HorizontalView : UserControl
    {
        public HorizontalView()
        {
            this.InitializeComponent();
        }

        public event EventHandler<MediaItem> OnMediaSelected;
        public event EventHandler<IMediaProvider> OnExploreClicked;

        public IMediaProvider SourceProvider
        {
            get { return (IMediaProvider)GetValue(SourceProperty); }
            set { SetValue(SourceProperty, value); }
        }
        public static readonly DependencyProperty SourceProperty = DependencyProperty.Register(
            nameof(SourceProvider),
            typeof(IMediaProvider),
            typeof(HorizontalView),
            new PropertyMetadata(null, OnSourceChanged));
        private ScrollViewer scroll;

        // the ListView's template may not be applied yet when SourceProvider is set (Uno sets it before Loaded), so look it up on use
        private ScrollViewer Scroll => scroll ??= FindDescendant<ScrollViewer>(MList, _ => true);

        private static void OnSourceChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is HorizontalView view && e.NewValue is IMediaProvider mediaSource)
            {
                view.OnLoad(mediaSource);
            }
        }

        private async void OnLoad(IMediaProvider mediaSource)
        {
            if (SourceProvider != null)
            {
                NameBox.Text = mediaSource.Name;
                ContentBox.Text = mediaSource.ContentType.ToString();
                var items = await mediaSource.LoadMoreItems(0);
                MList.ItemsSource = items.Take(20);
            }
        }


        private void LeftButton_Click(object sender, RoutedEventArgs e)
        {
            if (Scroll is { } scroll && scroll.HorizontalOffset>0)
            {
                scroll.ScrollToHorizontalOffset(Math.Max(0, scroll.HorizontalOffset - 500));
            }
        }

        private void RightButton_Click(object sender, RoutedEventArgs e)
        {
            if (Scroll is { } scroll && scroll.HorizontalOffset<scroll.ExtentWidth)
            {
                scroll.ScrollToHorizontalOffset(Math.Min(scroll.ExtentWidth, scroll.HorizontalOffset + 500));
            }
        }

        private void MList_ItemClick(object sender, ItemClickEventArgs e)
        {
            if (e.ClickedItem is MediaItem mediaItem)
            {
                OnMediaSelected?.Invoke(this, mediaItem);
            }
        }

        private void StackPanel_PointerEntered(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            if (SourceProvider.CanDelete && sender is StackPanel stackpanel)
            {
                var btn = FindDescendant<Button>(stackpanel, x => x.Tag is string tag && tag == "DelButton");
                if (btn != null) btn.Opacity = 1;
            }
        }

        private void StackPanel_PointerExited(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            if (SourceProvider.CanDelete && sender is StackPanel stackpanel)
            {
                var btn = FindDescendant<Button>(stackpanel, x => x.Tag is string tag && tag == "DelButton");
                if (btn != null) btn.Opacity = 0;
            }
        }

        private async void Del_Click(object sender, RoutedEventArgs e)
        {
            var et = new WatchlistService();
            if (sender is Button button && button.DataContext is MediaItem mediaItem)
            {
                et.Delete(mediaItem);
                MList.ItemsSource = await SourceProvider.LoadMoreItems(0);
            }
        }

        private void ExploreButton_Click(object sender, RoutedEventArgs e)
        {
            OnExploreClicked?.Invoke(sender, SourceProvider);
        }

        public static T FindDescendant<T>(DependencyObject parent, Func<T, bool> predicate) where T : DependencyObject
        {
            if (parent == null) return default;

            int childCount = VisualTreeHelper.GetChildrenCount(parent);
            for (int i = 0; i < childCount; i++)
            {
                var child = VisualTreeHelper.GetChild(parent, i);

                if (child is T tChild && predicate(tChild))
                {
                    return tChild;
                }

                var descendant = FindDescendant(child, predicate);
                if (descendant != null)
                {
                    return descendant;
                }
            }

            return default;
        }
    }
}
