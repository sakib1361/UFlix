using System.Reflection;
using Microsoft.UI.Dispatching;
using UFlix.Engine.Services;
using Velopack;
using Velopack.Sources;

namespace UFlix.Cross.Services
{
    /// <summary>
    /// Velopack updates from the GitHub releases of the repo the build came from (UFlixUpdateRepo, set by CI).
    /// Only a copy Velopack installed (Setup.exe on Windows, the AppImage on Linux) can replace itself.
    /// The daily check is a one-shot timer re-armed after each check, so nothing ticks in between.
    /// </summary>
    public class AppUpdater
    {
        public static AppUpdater Instance { get; } = new();

        private static readonly TimeSpan CheckInterval = TimeSpan.FromDays(1);
        private static readonly TimeSpan StartupDelay = TimeSpan.FromSeconds(30);
        private static DBUtilities Db => DBUtilities.Instance;

        private readonly UpdateManager _manager;
        private DispatcherQueueTimer _timer;

        private AppUpdater()
        {
            RepoUrl = Metadata("UFlixUpdateRepo");
            if (string.IsNullOrEmpty(RepoUrl)) return;
            try
            {
                _manager = new UpdateManager(new GithubSource(RepoUrl, null, false));
            }
            catch
            {
                _manager = null;
            }
        }

        public string RepoUrl { get; }

        public string ReleasesUrl => string.IsNullOrEmpty(RepoUrl) ? null : RepoUrl.TrimEnd('/') + "/releases/latest";

        public string CurrentVersion => _manager?.CurrentVersion?.ToString() ?? Metadata("UFlixVersion");

        private static string Metadata(string key) => typeof(AppUpdater).Assembly.GetCustomAttributes<AssemblyMetadataAttribute>()
            .FirstOrDefault(a => a.Key == key)?.Value;

        /// <summary>The build knows its release feed (CI builds only).</summary>
        public bool HasFeed => !string.IsNullOrEmpty(RepoUrl);

        /// <summary>Installed by Velopack, so it can download and apply an update itself.</summary>
        public bool CanSelfUpdate => _manager?.IsInstalled == true;

        public UpdateInfo Pending { get; private set; }

        public bool AutoCheck
        {
            get => !Db.GetPermanent<bool>("UpdateNoAutoCheck");
            set
            {
                Db.AddUpdatePermanent(!value, "UpdateNoAutoCheck");
                Schedule(value);
            }
        }

        public void Start(DispatcherQueue queue)
        {
            if (!CanSelfUpdate) return;
            _timer = queue.CreateTimer();
            _timer.IsRepeating = false;
            _timer.Tick += (_, _) => _ = AutoCheckAsync();
            Schedule(AutoCheck);
        }

        /// <summary>Asks the feed for a newer release; null when up to date. Throws when the feed can't be reached.</summary>
        public async Task<UpdateInfo> CheckNowAsync()
        {
            if (!CanSelfUpdate) return null;
            // a failed check counts too, so being offline doesn't turn into retrying every few seconds
            Db.AddUpdatePermanent(DateTime.UtcNow, "UpdateLastCheck");
            try
            {
                Pending = await _manager.CheckForUpdatesAsync();
                return Pending;
            }
            finally
            {
                Schedule(AutoCheck);
            }
        }

        /// <summary>Downloads the pending update, then closes the app, applies it and starts the new version.</summary>
        public async Task InstallAsync(Action<int> progress)
        {
            if (Pending == null) return;
            await _manager.DownloadUpdatesAsync(Pending, progress);
            _manager.ApplyUpdatesAndRestart(Pending);
        }

        private async Task AutoCheckAsync()
        {
            try
            {
                var update = await CheckNowAsync();
                if (update != null)
                    NavigationHelper.Notify($"UFlix {update.TargetFullRelease.Version} is available: install it from Settings → Updates");
            }
            catch
            {
                // offline or rate-limited: the next check is a day away
            }
        }

        private void Schedule(bool enabled)
        {
            if (_timer == null) return;
            _timer.Stop();
            if (!enabled) return;
            var due = Db.GetPermanent<DateTime>("UpdateLastCheck") + CheckInterval - DateTime.UtcNow;
            _timer.Interval = due > StartupDelay ? due : StartupDelay;
            _timer.Start();
        }
    }
}
