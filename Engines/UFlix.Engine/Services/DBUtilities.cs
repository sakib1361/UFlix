using LiteDB;
using Newtonsoft.Json;
using System.Runtime.CompilerServices;
using UFlix.Engine.Models;

namespace UFlix.Engine.Services
{
    public class DBUtilities
    {
        public static DBUtilities Instance { get; } = new DBUtilities();
        private static SemaphoreSlim slim = new SemaphoreSlim(1);
        private readonly static string DBPath;
        static DBUtilities()
        {
            AppConstants.DBFolder = AppPaths.DataFolder;
            Directory.CreateDirectory(AppConstants.DBFolder);
            DBPath = Path.Combine(AppConstants.DBFolder, "LocalData.db");
        }
        public async void Open()
        {
            await slim.WaitAsync();
            using var db = new LiteDatabase(DBPath);
            var col = db.GetCollection<CachedStorage>();
            int del = col.DeleteMany(x => x.ExpiresOn < DateTime.UtcNow);
            slim.Release();
        }

        public CachedStorage GetCache(string key)
        {
            slim.Wait();
            using var db = new LiteDatabase(DBPath);
            var col = db.GetCollection<CachedStorage>();
            var cachedData = col.FindById(key);
            slim.Release();
            return cachedData;
        }

        public async void AddUpdateCache(CachedStorage cachedStorage)
        {
            await slim.WaitAsync();
            using var db = new LiteDatabase(DBPath);
            var col = db.GetCollection<CachedStorage>();
            col.Upsert(cachedStorage);
            slim.Release();
        }

        public T GetCache<T>(string key)
        {
            slim.Wait();
            using var db = new LiteDatabase(DBPath);
            var col = db.GetCollection<CachedStorage>();
            var cachedData = col.FindById(key);
            T data = Default<T>.Value;

            if (cachedData != null)
            {
                try
                {
                    var dataSet = JsonConvert.DeserializeObject<T>(cachedData.Data);
                    data = dataSet;
                }
                catch
                {
                    col.Delete(key);
                }
            }
            slim.Release();
            return data;
        }

        public async void AddUpdateCache(string key, object value, int expires = 3)
        {
            await slim.WaitAsync();
            using var db = new LiteDatabase(DBPath);
            var col = db.GetCollection<CachedStorage>();
            var old = col.FindById(key);
            old ??= new CachedStorage() { Id = key };
            old.Data = JsonConvert.SerializeObject(value);
            old.ExpiresOn = DateTime.UtcNow.AddDays(expires);
            col.Upsert(old);
            slim.Release();
        }



        public async void AddUpdatePermanent<T>(T data, [CallerMemberName] string key = "")
        {
            await slim.WaitAsync();
            using var db = new LiteDatabase(DBPath);
            var col = db.GetCollection<PermanentStorage>();
            var old = col.FindById(key);
            old ??= new PermanentStorage() { Id = key };
            old.Data = JsonConvert.SerializeObject(data);
            col.Upsert(old);
            slim.Release();
        }
        public T GetPermanent<T>([CallerMemberName] string key = "")
        {
            slim.Wait();
            using var db = new LiteDatabase(DBPath);
            var col = db.GetCollection<PermanentStorage>();
            var cachedData = col.FindById(key);
            T data = Default<T>.Value;
            if(cachedData != null)
            {
                try
                {
                    var dataSet = JsonConvert.DeserializeObject<T>(cachedData.Data);
                    data = dataSet;
                }
                catch
                {
                    col.Delete(key);
                }
            }
            slim.Release();
            return data;
        }

        public double Volume
        {
            get => GetPermanent<double>();
            set => AddUpdatePermanent(value);
        }

        public int YoutubeWidth
        {
            get => GetPermanent<int>();
            set => AddUpdatePermanent(value);
        }

        public int MaxFolderSize
        {
            get => GetPermanent<int>();
            set => AddUpdatePermanent(value);
        }

        public string YoutubeCodec
        {
            get => GetPermanent<string>();
            set => AddUpdatePermanent(value);
        }

        public string YoutubeDownloadPath
        {
            get => GetPermanent<string>();
            set => AddUpdatePermanent(value);
        }
        public string SubtitleProvider
        {
            get => GetPermanent<string>();
            set => AddUpdatePermanent(value);
        }
        public string TorrentDownloadPath
        {
            get => GetPermanent<string>();
            set => AddUpdatePermanent(value);
        }

        public string SubtitleUsername
        {
            get => GetPermanent<string>();
            set => AddUpdatePermanent(value);
        }

        public string SubtitlPassword
        {
            get => GetPermanent<string>();
            set => AddUpdatePermanent(value);
        }
        public string LastSearch
        {
            get => GetPermanent<string>();
            set => AddUpdatePermanent(value);
        }

        public TorrentSource[] TorrentSources
        {
            get => GetPermanent<TorrentSource[]>()?? [];
            set => AddUpdatePermanent(value);
        }

        public string TvLinks
        {
            get => GetPermanent<string>();
            set => AddUpdatePermanent<string>(value);   
        }

        public bool UseYtDLFirst
        {
            get => GetPermanent<bool?>() ?? true;
            set => AddUpdatePermanent<bool?>(value);
        }

        public TrackedTorrent[] TrackedTorrents
        {
            get => GetPermanent<TrackedTorrent[]>() ?? [];
            set => AddUpdatePermanent(value);
        }
        public bool UseSystemDecoder
        {
            get => GetPermanent<bool>();
            set => AddUpdatePermanent(value);
        }

        public string MpvProfile
        {
            get => GetPermanent<string>();
            set => AddUpdatePermanent(value);
        }
    }
}
