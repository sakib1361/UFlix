using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using UFlix.Engine.Models;

namespace UFlix.Engine.Services
{
    public static class Utilites
    {
        public static void ClearFolder()
        {
            var folder = DBUtilities.Instance.TorrentDownloadPath;
            if (string.IsNullOrWhiteSpace(folder))
            {
                folder = AppPaths.DownloadFolder;
            };
            try
            {
                folder = Path.Combine(folder, "Torrents");
                Directory.CreateDirectory(folder);
                int maxSize = DBUtilities.Instance.MaxFolderSize;
                if (maxSize == 0) return;

                var topDatas = new List<FileFolderInfo>();
                ListInfos(folder, topDatas);


                long maxByteSize = (long)maxSize * 1024*1024*1024;
                long currentSize = topDatas.Sum(x => x.Size);

                if (currentSize > maxByteSize)
                {
                    foreach (var item in topDatas.OrderBy(x => x.LastWriteDateTime))
                    {
                        currentSize-=item.Size;
                        item.Delete();
                        if (currentSize<=maxByteSize) break;
                    }
                }
            }
            catch (Exception ex)
            {
                Debug.WriteLine($"Error clearing folder: {ex.Message}");
            }
        }

        private static void ListInfos(string path, List<FileFolderInfo> datas)
        {
            foreach (var dir in Directory.GetDirectories(path))
            {
                if (dir.EndsWith("_incomplete"))
                {
                    ListInfos(dir, datas);
                    continue;
                }
                datas.Add(new FileFolderInfo(dir, true));
            }
            foreach (var file in Directory.GetFiles(path))
            {
                datas.Add(new FileFolderInfo(file, false));
            }
        }

        public static string MeasureFileSize(long length)
        {
            string[] sizes = { "B", "KB", "MB", "GB", "TB" };
            double len = length;
            int order = 0;

            while (len >= 1024 && order < sizes.Length - 1)
            {
                order++;
                len = len / 1024;
            }

            return $"{len:0.##} {sizes[order]}";
        }

        public static string Hash(string s)
        {
            return Convert.ToHexString(SHA1.HashData(Encoding.UTF8.GetBytes(s)));
        }
    }
}
