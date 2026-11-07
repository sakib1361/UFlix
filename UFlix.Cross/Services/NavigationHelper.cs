using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Controls;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using UFlix.Engine.MediaProviders;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using UFlix.Cross;
using UFlix.Cross.Pages;

namespace UFlix.Cross.Services
{
    public interface IPlayerControl
    {
        public UserControl CreatePlayer();
        void Play(UserControl player, MediaItem file);
    }


    public static class NavigationHelper
    {
        private static MainWindow _window;
        private static IPlayerControl _playerC;
        private static SemaphoreSlim sema = new(1);

        public static void Initialize(MainWindow window, IPlayerControl control)
        {
            _window = window;
            _playerC = control;
            Downloader.Init();
            ImageCache.Instance.Clear();
        }

        public static async void Execute(DispatcherQueueHandler handler)
        {
            await sema.WaitAsync();
            _window.DispatcherQueue.TryEnqueue(handler);
            sema.Release();
        }

        public static void Navigate(Type type, IMediaProvider e)
        {
            _window.Navigate(type, e);
        }

        public static void Notify(string message)
        {
            _window.Notify(message);
        }

        public static void OpenPlayer(MediaItem file)
        {
            var player = _playerC.CreatePlayer();
            _window.OpenPlayer(player);
            _playerC.Play(player, file);
        }

        public static void StopPlayer()
        {
            _window.StopPlayer();
        }

        public static MainWindow GetWindow()
        {
            return _window;
        }

        public static async Task<string> GetSelectedMedia(MediaItem item, List<string> files)
        {
            string mediaFile = null;
            TaskCompletionSource<bool> result = new TaskCompletionSource<bool>();
            if (files.Count > 1)
            {
                Execute(async () =>
                {
                    var dlg = new TorrentSelector(item, [.. files])
                    {
                        XamlRoot = GetWindow().Content.XamlRoot,
                    };
                    await dlg.ShowAsync();
                    mediaFile = dlg.SelectedFile;
                    result.SetResult(true);

                });
                await result.Task;
            }
            else
            {
                mediaFile = files.FirstOrDefault();
            }
            return mediaFile;
        }
    }
}
