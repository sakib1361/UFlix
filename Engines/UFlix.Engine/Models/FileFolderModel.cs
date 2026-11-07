namespace UFlix.Engine.Models
{
    internal class FileFolderInfo
    {
        public bool IsFolder { get; }
        public string Path { get; }
        public DateTime LastWriteDateTime { get; private set; }
        public long Size { get; }


        public FileFolderInfo(string path, bool isFolder)
        {
            Path = path;
            IsFolder = isFolder;
            if (isFolder)
            {
                var dirInfo = new DirectoryInfo(Path);
                LastWriteDateTime = dirInfo.LastWriteTime;
                Size = DirSize(dirInfo);
            }
            else
            {
                var info = new FileInfo(Path);
                Size = info.Length;
                LastWriteDateTime = info.LastWriteTime;
            }
        }

        private long DirSize(DirectoryInfo d)
        {
            long size = 0;
            // Add file sizes.
            FileInfo[] fis = d.GetFiles();
            foreach (FileInfo fi in fis)
            {
                size += fi.Length;
                LastWriteDateTime = MaxTime(LastWriteDateTime, fi.LastWriteTime);
            }
            // Add subdirectory sizes.
            DirectoryInfo[] dis = d.GetDirectories();
            foreach (DirectoryInfo di in dis)
            {
                size += DirSize(di);
            }
            return size;
        }

        private static DateTime MaxTime(DateTime dt1, DateTime dt2)
        {
            return dt1 > dt2 ? dt1 : dt2;
        }

        internal void Delete()
        {
            if (IsFolder) Directory.Delete(Path, true);
            else File.Delete(Path);
        }
    }
}
