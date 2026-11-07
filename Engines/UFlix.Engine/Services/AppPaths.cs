using UFlix.Engine.Models;

namespace UFlix.Engine.Services
{
    public static class AppPaths
    {
        private static readonly string DataRoot = OperatingSystem.IsWindows()
            ? Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments)
            : XdgDir("XDG_DATA_HOME", ".local/share");

        private static readonly string DownloadRoot = OperatingSystem.IsWindows() ? null : XdgUserDir("DOWNLOAD", "Downloads");

        public static string DataFolder => Path.Combine(DataRoot, AppConstants.AppName);

        public static string DownloadFolder => OperatingSystem.IsWindows()
            ? DataFolder
            : Path.Combine(DownloadRoot, AppConstants.AppName);

        private static string Home => Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);

        private static string XdgDir(string variable, string fallback)
        {
            var value = Environment.GetEnvironmentVariable(variable);
            return Path.IsPathRooted(value) ? value : Path.Combine(Home, fallback);
        }

        // user-dirs.dirs holds lines such as XDG_DOWNLOAD_DIR="$HOME/Downloads"
        private static string XdgUserDir(string name, string fallback)
        {
            var key = $"XDG_{name}_DIR";
            var value = Environment.GetEnvironmentVariable(key);
            if (Path.IsPathRooted(value)) return value;
            try
            {
                var file = Path.Combine(XdgDir("XDG_CONFIG_HOME", ".config"), "user-dirs.dirs");
                if (File.Exists(file))
                {
                    foreach (var line in File.ReadLines(file))
                    {
                        if (!line.StartsWith(key + "=")) continue;
                        var path = line[(key.Length + 1)..].Trim('"').Replace("$HOME", Home);
                        if (Path.IsPathRooted(path)) return path;
                    }
                }
            }
            catch { }
            return Path.Combine(Home, fallback);
        }
    }
}
