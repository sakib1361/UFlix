using Microsoft.UI.Xaml;
using System;
using System.IO;
using System.Threading.Tasks;
using UFlix.Cross.Pages;
using UFlix.Cross.Player;
using UFlix.Cross.Templates;
using UFlix.Engine.Models;

namespace UFlix.Cross.Services
{
    // UFLIX_TOUR=<file> drives the app through the README screenshots by itself (like MemoryProbe): Home, the search dropdown on
    // its Movies and Shows tabs, and the player (Big Buck Bunny, CC BY 3.0 Blender Foundation, streamed from the Internet
    // Archive) without and with the info panel. At each scene it appends "ready <name>" to the file and holds still for a few
    // seconds so an outside screenshot tool can capture the window, then the app exits. UFLIX_TOUR_URL / UFLIX_TOUR_SEEK (seconds)
    // override the video and the playback position. Run it with a scratch XDG_DATA_HOME so no personal watchlist/history shows.
    public static class ScreenshotTour
    {
        private const string DefaultUrl = "https://archive.org/download/BigBuckBunnyFULLHD60FPS/Big%20Buck%20Bunny%20-%20FULL%20HD%2060FPS.mp4";

        private static readonly string LogPath = Environment.GetEnvironmentVariable("UFLIX_TOUR");
        private static readonly TimeSpan Hold = TimeSpan.FromSeconds(8);

        public static bool Enabled => !string.IsNullOrEmpty(LogPath);

        public static async void Run(MainWindow window)
        {
            if (!Enabled) return;
            File.WriteAllText(LogPath, "");
            try
            {
                await Tour(window);
                Log("done");
            }
            catch (Exception ex)
            {
                Log("error " + ex.Message.ReplaceLineEndings(" "));
            }
            Application.Current.Exit();
        }

        private static async Task Tour(MainWindow window)
        {
            var home = await WaitFor(() => HorizontalView.FindDescendant<HomePage>(window.Content, _ => true), 60, "home page");
            await Task.Delay(12000);
            await Scene("home");

            await home.ShowSuggestions("Night of the Living Dead", true);
            await Task.Delay(4000);
            await Scene("search");
            await home.ShowSuggestions("The Twilight Zone", true);
            home.ShowSuggestionTab(MediaContentType.Shows);
            await Task.Delay(4000);
            await Scene("search-shows");
            home.ShowSuggestionTab(MediaContentType.Movie);
            await home.ShowSuggestions("", false);

            var url = Environment.GetEnvironmentVariable("UFLIX_TOUR_URL") is { Length: > 0 } u ? u : DefaultUrl;
            NavigationHelper.OpenPlayer(new MediaItem
            {
                Id = url,
                Url = url,
                Title = "Big Buck Bunny",
                MediaType = MediaType.Web,
                ContentType = MediaContentType.Movie,
            });

            var player = await WaitFor(() => HorizontalView.FindDescendant<PlayerPage>(window.Content, _ => true), 60, "player");
            await WaitUntil(() => player.Media.Position > 3 && !player.Media.IsBuffering, 600, "playback");
            var seek = double.TryParse(Environment.GetEnvironmentVariable("UFLIX_TOUR_SEEK"), out var s) ? s : 95;
            player.Media.Seek(seek);
            await WaitUntil(() => player.Media.Position > seek + 4 && !player.Media.IsBuffering, 600, "seek");
            await Scene("player");

            player.ToggleInfo();
            await Task.Delay(3000);
            await Scene("player-info");
            player.ToggleInfo();
            NavigationHelper.StopPlayer();
            await Task.Delay(2000);
        }

        private static async Task Scene(string name)
        {
            Log("ready " + name);
            await Task.Delay(Hold);
        }

        private static void Log(string line) => File.AppendAllText(LogPath, $"{DateTime.Now:HH:mm:ss} {line}\n");

        private static async Task<T> WaitFor<T>(Func<T> get, int seconds, string what) where T : class
        {
            for (var waited = 0; waited < seconds * 5; waited++)
            {
                if (get() is { } value) return value;
                await Task.Delay(200);
            }
            throw new TimeoutException($"no {what} after {seconds} s");
        }

        private static async Task WaitUntil(Func<bool> condition, int seconds, string what) =>
            await WaitFor(() => condition() ? new object() : null, seconds, what);
    }
}
