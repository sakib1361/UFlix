using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text;
using UFlix.Engine.Models;
using UFlix.Engine.Services;

namespace UFlix.Engine.SubtitleSearchEngine
{
    internal class OpenSubtitleParser : ISubsearch
    {
        private readonly HttpClient _http;
        private string _token;
        private static readonly string API = AppSecrets.OpenSubtitlesApiKey;
        private static readonly string Username = AppSecrets.OpenSubtitlesUsername;
        private static readonly string Password = AppSecrets.OpenSubtitlesPassword;
        public OpenSubtitleParser()
        {
            _http = new HttpClient()
            {
                BaseAddress = new Uri("https://api.opensubtitles.com/api/v1/")
            };
            _http.DefaultRequestHeaders.Add("user-agent", "UFLix v1.0.0");
            _http.DefaultRequestHeaders.Add("Api-Key", API);
            _http.DefaultRequestHeaders.Add("Accept", "application/json");
        }
        private async Task<bool> Login()
        {
            if (!string.IsNullOrWhiteSpace(_token)) return true;
            var resp = await _http.PostAsJsonAsync("login", new
            {
                username = Username,
                password = Password
            });
            if (resp.IsSuccessStatusCode)
            {
                var data = await resp.Content.ReadFromJsonAsync<LoginResponse>();
                _token = data.token;
                _http.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", _token);
                return data.status == 200;
            }
            return false;
        }

        public async Task<SubtitleFile[]> SearchSubtitlesAsync(MediaItem media)
        {
            bool success =await Login();
            if (success == false) return [];
            var allFiles = new List<SubtitleFile>();
            var title = media.Title.Replace(":", "");
            var param = new List<string>();
            if (File.Exists(media.Url))
            {
                using Stream input = File.OpenRead(media.Url);
                var movieHash = ComputeMovieHash(input);
                param.Add($"moviehash={movieHash}");
                param.Add($"query={Uri.EscapeDataString(Path.GetFileNameWithoutExtension(media.Url))}");
            }
            else
            {
                param.Add($"query={Uri.EscapeDataString(title)}");
            }
            // Local files carry their path in Id: only a real IMDb id ("tt1234567") may go to the API.
            if (media.Id is { } id && id.StartsWith("tt") && id.Length > 2 && id[2..].All(char.IsDigit))
            {
                param.Add($"imdb_id={id[2..]}");
            }
            param.Add("languages=en");
            var query = "subtitles?"+string.Join('&', param);
            try
            {
                var resp = await _http.GetFromJsonAsync<SubtitleResult>(query);
                foreach (var item in resp.data)
                {
                    var sub = new SubtitleFile(() => Download(item))
                    {
                        Id = item.attributes.release,
                        Name = item.attributes.feature_details.title,
                        Description = $"Download {item.attributes.download_count}, Rating: {item.attributes.ratings}",
                    };
                    allFiles.Add(sub);
                }
                return allFiles.ToArray();
            }
            catch (Exception)
            {
                return [];
            }
        }




        public async Task<string> Download(SubtitleData media)
        {
            var postData = new
            {
                file_id = media.attributes.files.FirstOrDefault()?.file_id,
            };
            var downloadResponse = await _http.PostAsJsonAsync("download", postData);
            if (downloadResponse.IsSuccessStatusCode)
            {
                var downloadLinkData = await downloadResponse.Content.ReadFromJsonAsync<SubtitleDownload>();
                if (downloadLinkData.link != null)
                {
                    var allData = await _http.GetStringAsync(downloadLinkData.link);
                    var fileData = Path.GetTempFileName();
                    await File.WriteAllTextAsync(fileData, allData);
                    return fileData;
                }
            }
            else
            {
                _ = await downloadResponse.Content.ReadAsStringAsync();
            }
            return null;
        }


        private string ComputeMovieHash(Stream input, long length = -1)
        {

            long lhash, streamsize;
            streamsize = input.Length;
            lhash = length == -1 ? streamsize : length;

            long i = 0;
            var buffer = new byte[sizeof(long)];
            while (i < 65536 / sizeof(long) && (input.Read(buffer, 0, sizeof(long)) > 0))
            {
                i++;
                lhash += BitConverter.ToInt64(buffer, 0);
            }

            input.Position = Math.Max(0, streamsize - 65536);
            i = 0;
            while (i < 65536 / sizeof(long) && (input.Read(buffer, 0, sizeof(long)) > 0))
            {
                i++;
                lhash += BitConverter.ToInt64(buffer, 0);
            }

            var result = BitConverter.GetBytes(lhash);
            Array.Reverse(result);
            var stringBuilder = new StringBuilder();
            for (var counter = 0; counter < result.Length; counter++)
            {
                _=stringBuilder.Append(result[counter].ToString("x2"));
            }

            return stringBuilder.ToString();
        }

        private class LoginResponse
        {
            public string base_url { get; set; }
            public string token { get; set; }
            public int status { get; set; }
        }

        // Root myDeserializedClass = JsonConvert.DeserializeObject<Root>(myJsonResponse);


        internal class Attributes
        {
            public string subtitle_id { get; set; }
            public string language { get; set; }
            public long download_count { get; set; }
            public long new_download_count { get; set; }
            public long votes { get; set; }
            public float ratings { get; set; }
            public DateTime upload_date { get; set; }
            public string release { get; set; }
            public bool moviehash_match { get; set; }
            public List<SubFile> files { get; set; }
            public FeatureDetails feature_details { get; set; }
        }

        internal class SubtitleData
        {
            public string id { get; set; }
            public string type { get; set; }
            public Attributes attributes { get; set; }

        }

        internal class FeatureDetails
        {
            public string title { get; set; }
            public string movie_name { get; set; }
        }

        internal class SubFile
        {
            public int file_id { get; set; }
            public int cd_number { get; set; }
            public string file_name { get; set; }
        }

        private class SubtitleResult
        {
            public int total_pages { get; set; }
            public int total_count { get; set; }
            public int per_page { get; set; }
            public int page { get; set; }
            public SubtitleData[] data { get; set; }
        }


        internal class SubtitleDownload
        {
            public string link { get; set; }
            public string file_name { get; set; }
            public int requests { get; set; }
            public int remaining { get; set; }
            public string message { get; set; }
            public string reset_time { get; set; }
            public DateTime reset_time_utc { get; set; }
            public string Subtitle { get; internal set; }
        }
    }
}
