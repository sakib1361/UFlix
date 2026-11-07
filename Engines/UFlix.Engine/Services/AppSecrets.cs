using System.Reflection;

namespace UFlix.Engine.Services
{
    /// <summary>
    /// API keys from the git-ignored secrets.env at the repo root, embedded into this assembly at build time
    /// (see UFlix.Engine.csproj). An environment variable of the same name overrides the embedded value.
    /// </summary>
    public static class AppSecrets
    {
        private static readonly Dictionary<string, string> values = Load();

        public static string TmdbApiKey => Get("UFLIX_TMDB_API_KEY");
        public static string OpenSubtitlesApiKey => Get("UFLIX_OPENSUBTITLES_API_KEY");
        public static string OpenSubtitlesUsername => Get("UFLIX_OPENSUBTITLES_USERNAME");
        public static string OpenSubtitlesPassword => Get("UFLIX_OPENSUBTITLES_PASSWORD");
        public static string SubdlApiKey => Get("UFLIX_SUBDL_API_KEY");

        public static string Get(string name)
        {
            var env = Environment.GetEnvironmentVariable(name);
            if (!string.IsNullOrWhiteSpace(env)) return env;
            return values.TryGetValue(name, out var value) ? value : string.Empty;
        }

        private static Dictionary<string, string> Load()
        {
            var result = new Dictionary<string, string>(StringComparer.Ordinal);
            using var stream = Assembly.GetExecutingAssembly().GetManifestResourceStream("UFlix.secrets.env");
            if (stream == null) return result;
            using var reader = new StreamReader(stream);
            while (reader.ReadLine() is { } line)
            {
                line = line.Trim();
                if (line.Length == 0 || line.StartsWith('#')) continue;
                var eq = line.IndexOf('=');
                if (eq <= 0) continue;
                var value = line[(eq + 1)..].Trim();
                if (value.Length >= 2 && (value[0] == '"' || value[0] == '\'') && value[^1] == value[0])
                    value = value[1..^1];
                result[line[..eq].Trim()] = value;
            }
            return result;
        }
    }
}
