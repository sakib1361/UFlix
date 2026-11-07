using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using UFlix.Cross.Pages;
using UFlix.Cross.Templates;

namespace UFlix.Cross.Services
{
    // UFLIX_MEMPROBE=<log file> drives the app by itself: Home loads, scrolls to the end, then cycles the pages
    // (UFLIX_MEMPROBE_ROUNDS, default 5) and exits, logging managed/native memory, the Skia GPU cache and how many pages and
    // images are still alive. UFLIX_NO_IMAGES=1 skips image loading; UFLIX_MEMPROBE_HOLD=1 waits 60 s before exiting (for dumps).
    public static partial class MemoryProbe
    {
        [System.Runtime.InteropServices.LibraryImport("libc", EntryPoint = "malloc_trim")]
        private static partial int MallocTrim(nint pad);

        public static readonly bool NoImages = Environment.GetEnvironmentVariable("UFLIX_NO_IMAGES") == "1";
        private static readonly string LogPath = Environment.GetEnvironmentVariable("UFLIX_MEMPROBE");
        private static readonly List<WeakReference> Pages = [];
        private static readonly List<WeakReference> Images = [];

        private static readonly int Rounds = int.TryParse(Environment.GetEnvironmentVariable("UFLIX_MEMPROBE_ROUNDS"), out var r) ? r : 5;

        public static bool Enabled => !string.IsNullOrEmpty(LogPath);

        public static void TrackImage(object image)
        {
            if (Enabled) lock (Images) Images.Add(new WeakReference(image));
        }

        public static async void Run(MainWindow window)
        {
            if (!Enabled) return;
            File.WriteAllText(LogPath, $"# images={(NoImages ? "off" : "on")}\n");
            var frame = await WaitFor(() => HorizontalView.FindDescendant<Frame>(window.Content, _ => true));
            frame.Navigated += (_, e) => Pages.Add(new WeakReference(e.Content));
            if (frame.Content != null) Pages.Add(new WeakReference(frame.Content));

            await Task.Delay(10000);
            await Log("home loaded");

            await Log(await ScrollHomeToEnd(frame));

            List<Type> cycle = [typeof(DiscoverPage), typeof(YoutubePage), typeof(TvPage), typeof(HomePage)];
            for (var round = 1; round <= Rounds; round++)
            {
                foreach (var page in cycle)
                {
                    window.Navigate(page);
                    await Task.Delay(4000);
                }
                await Log($"page cycle {round}");
            }

            window.Navigate(typeof(TvPage));
            await Task.Delay(4000);
            await Log("parked on TvPage");
            if (Environment.GetEnvironmentVariable("UFLIX_MEMPROBE_HOLD") == "1") await Task.Delay(60000);
            Application.Current.Exit();
        }

        // separate method so the ScrollViewer isn't captured by Run's state machine (that would keep the HomePage alive)
        private static async Task<string> ScrollHomeToEnd(Frame frame)
        {
            var scroll = await WaitFor(() => frame.Content is HomePage home
                ? HorizontalView.FindDescendant<ScrollViewer>(HorizontalView.FindDescendant<ListView>(home, x => x.Name == "MediaList"), _ => true) : null);
            for (var i = 0; i < 200; i++)
            {
                var before = scroll.VerticalOffset;
                scroll.ChangeView(null, before + scroll.ViewportHeight, null, true);
                await Task.Delay(1500);
                if (scroll.VerticalOffset <= before && scroll.VerticalOffset + scroll.ViewportHeight >= scroll.ExtentHeight - 1) break;
            }
            await Task.Delay(5000);
            return $"home end {scroll.VerticalOffset:0}/{scroll.ExtentHeight:0}";
        }

        // Skia's GPU resource cache (textures of drawn images), found through SkiaSharp's handle table
        private static string GpuCache()
        {
            try
            {
                var asm = AppDomain.CurrentDomain.GetAssemblies().FirstOrDefault(a => a.GetName().Name == "SkiaSharp");
                var table = asm?.GetType("SkiaSharp.HandleDictionary")?.GetField("instances", System.Reflection.BindingFlags.Static | System.Reflection.BindingFlags.NonPublic)?.GetValue(null);
                if (table is not System.Collections.IDictionary dict) return "gpu=?";
                object ctx;
                lock (dict) ctx = dict.Values.Cast<WeakReference>().Select(w => w.Target).FirstOrDefault(t => t?.GetType().Name == "GRContext");
                if (ctx == null) return "gpu=none";
                var t = ctx.GetType();
                var args = new object[] { 0, 0L };
                t.GetMethod("GetResourceCacheUsage")!.Invoke(ctx, args);
                var limit = (long)t.GetMethod("GetResourceCacheLimit")!.Invoke(ctx, null)!;
                return $"gpu={(long)args[1] >> 20} MB/{args[0]} res (limit {limit >> 20} MB)";
            }
            catch (Exception ex) { return "gpu=" + ex.GetType().Name; }
        }

        private static async Task<T> WaitFor<T>(Func<T> get) where T : class
        {
            T value;
            while ((value = get()) == null) await Task.Delay(200);
            return value;
        }

        private static async Task Log(string step)
        {
            for (var i = 0; i < 3; i++)
            {
                GC.Collect(GC.MaxGeneration, GCCollectionMode.Forced, true, true);
                GC.WaitForPendingFinalizers();
                await Task.Delay(300);
            }
            var proc = Process.GetCurrentProcess();
            proc.Refresh();
            var livePages = Pages.Where(x => x.IsAlive).Select(x => x.Target?.GetType().Name).GroupBy(x => x)
                .Select(g => $"{g.Key}x{g.Count()}");
            int liveImages;
            lock (Images) liveImages = Images.Count(x => x.IsAlive);
            var rss = proc.WorkingSet64;
            var gpu = GpuCache();
            if (OperatingSystem.IsLinux()) { MallocTrim(0); proc.Refresh(); }
            File.AppendAllText(LogPath,
                $"{step,-24} managed={GC.GetTotalMemory(false) >> 20,4} MB  rss={rss >> 20,4} MB  trimmed={proc.WorkingSet64 >> 20,4} MB  " +
                $"private={proc.PrivateMemorySize64 >> 20,4} MB  images={liveImages,4}/{Images.Count}  pages=[{string.Join(" ", livePages)}]  {gpu}\n");
        }
    }
}
