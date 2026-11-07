using System.Net.Http.Json;

namespace UFlix.Engine.Services
{
    internal static class HttpUtilities
    {
        private static readonly HttpClient _httpClient = new();

        internal static async Task<T[]> GetAsyncArray<T>(string url, bool forced)
        {
            var cached = DBUtilities.Instance.GetCache<T[]>(url);
            if (cached.Length > 0 && forced == false) return cached;
            try
            {
                var data = await _httpClient.GetFromJsonAsync<T[]>(url);
                DBUtilities.Instance.AddUpdateCache(url, data);
                return data;
            }
            catch (Exception ex)
            {
                Console.WriteLine($"Unable to get information from server {ex}");
                //probably re-throw here :)
            }

            return [];
        }

        internal static async Task<T> GetAsync<T>(string url, bool forced)
        {
            var cached = DBUtilities.Instance.GetCache<T>(url);
            if (cached != null && forced == false) return cached;
            try
            {
                var data = await _httpClient.GetFromJsonAsync<T>(url);
                DBUtilities.Instance.AddUpdateCache(url, data);
                return data;
            }
            catch (Exception ex)
            {
                Console.WriteLine($"Unable to get information from server {ex}");
            }
            return default;
        }
    }
}
