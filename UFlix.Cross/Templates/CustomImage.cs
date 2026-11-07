using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using System;
using System.IO;
using UFlix.Engine.Services;
using UFlix.Cross.Services;

namespace UFlix.Cross.Templates
{
    public partial class CustomImage : ContentControl
    {
        private readonly Image _image;
        private int _version;
        private string _loaded;
        public bool IsTMDB { get; set; } = false;

        // decode width in view pixels; 0 decodes at full size. Uno keeps decoded bitmaps per (uri, size), so small tiles stay small
        public int DecodeWidth { get; set; }

        public Stretch Stretch
        {
            get => _image.Stretch;
            set => _image.Stretch = value;
        }
        public CustomImage()
        {
            _image = new Image();
            Content = _image;
            RegisterPropertyChangedCallback(VisibilityProperty, (_, _) => Load());
        }

        public string Source
        {
            get { return (string)GetValue(SourceProperty); }
            set { SetValue(SourceProperty, value); }
        }
        public static readonly DependencyProperty SourceProperty = DependencyProperty.Register(
            nameof(Source),
            typeof(string),
            typeof(CustomImage),
            new PropertyMetadata(null, OnSourceChanged));

        private static void OnSourceChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is CustomImage image) image.Load();
        }

        private async void Load()
        {
            var source = Source;
            if (string.IsNullOrWhiteSpace(source) || MemoryProbe.NoImages)
            {
                _version++;
                _loaded = null;
                _image.Source = null;
                return;
            }
            // collapsed images (e.g. the backdrop of non-YouTube tiles) are never fetched or decoded
            if (Visibility == Visibility.Collapsed || source == _loaded) return;

            var version = ++_version;
            _loaded = source;
            _image.Source = null;
            if (IsTMDB) source = TmdbMediaService.ImagePath400 + source;

            var file = source.StartsWith("http")
                ? await ImageCache.Instance.GetFromCacheAsync(source)
                : await ThumbnailService.GetAsync(source);
            if (version != _version || string.IsNullOrEmpty(file) || !File.Exists(file)) return;

            var bitmap = new BitmapImage { DecodePixelType = DecodePixelType.Logical };
            if (DecodeWidth > 0) bitmap.DecodePixelWidth = DecodeWidth;
            bitmap.UriSource = new Uri(file);
            _image.Source = bitmap;
            MemoryProbe.TrackImage(bitmap);
        }
    }
}
