using System.Collections.Concurrent;
using System.Text;
using UFlix.Engine.Models;

namespace UFlix.Engine.Services
{
    public class ImageCache
    {
        public readonly static ImageCache Instance = new();
        private readonly HttpClient client = new();
        private readonly ConcurrentDictionary<string, string> tasks = new();
        private static SemaphoreSlim sema = new(1);
        private string _directory;

        private ImageCache()
        {
            _directory = Path.Combine(AppConstants.DBFolder, "Cache");
            Directory.CreateDirectory(_directory);
        }

        public void Clear()
        {
            foreach (var file in Directory.GetFiles(_directory))
            {
                var info = new FileInfo(file);
                if ((DateTime.Now - info.CreationTime).TotalDays > 7)
                {
                    try
                    {
                        File.Delete(file);
                    }
                    catch
                    {
                    }
                }
            }
        }


        public async Task<string> GetFromCacheAsync(string uri)
        {
            if(string.IsNullOrWhiteSpace(uri)) return null;
            var fileName = CreateMD5(uri);
            var filePath = Path.Combine(_directory, fileName);
            if (File.Exists(filePath)) return filePath;

            await sema.WaitAsync();
            if (tasks.ContainsKey(fileName))
            {
                sema.Release();
                while (tasks.ContainsKey(fileName))
                {
                    await Task.Delay(100);
                }
                return filePath;
            }


            tasks.TryAdd(fileName, filePath);
            sema.Release();
            try
            {
                using var streamresp = await client.GetAsync(uri);
                if (streamresp.IsSuccessStatusCode)
                {
                    using var fileWrite = File.OpenWrite(filePath);
                    using var fileRead = await streamresp.Content.ReadAsStreamAsync();
                    await fileRead.CopyToAsync(fileWrite);
                }
            }
            catch { }
            tasks.TryRemove(fileName, out _);
            return filePath;
        }

        public static string CreateMD5(string input)
        {
            // Use input string to calculate MD5 hash
            using var md5 = System.Security.Cryptography.MD5.Create();
            var inputBytes = Encoding.ASCII.GetBytes(input);
            var hashBytes = md5.ComputeHash(inputBytes);

            return Convert.ToHexString(hashBytes); // .NET 5 +
        }
    }
}
