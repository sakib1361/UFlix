using System.Runtime.InteropServices;

namespace NativeRunner.TorrentHandler
{
    public sealed class TorrentFileInfo
    {
        public int Index { get; }
        public long Offset { get; }
        public long FileSize { get; }
        public DateTimeOffset ModifiedTime { get; }
        public string FileName { get; }
        public string FilePath { get; }
        public bool IsAbsolutePath { get; }
        public bool IsPadFile { get; }

        internal TorrentFileInfo(NativeStructs.TsTorrentFile file)
        {
            Index = file.index;
            Offset = file.offset;
            FileSize = file.file_size;
            ModifiedTime = DateTimeOffset.FromUnixTimeSeconds(file.modified_time);
            FileName = Marshal.PtrToStringUTF8(file.file_name);
            FilePath = Marshal.PtrToStringUTF8(file.file_path);
            IsAbsolutePath = file.file_path_is_absolute != 0;
            IsPadFile = file.pad_file != 0;
        }
    }

    public sealed class TorrentFileList
    {
        public int PieceLength { get; }
        public int PieceCount { get; }
        public IReadOnlyList<TorrentFileInfo> Files { get; }

        internal TorrentFileList(NativeStructs.TsTorrentFileList native)
        {
            PieceLength = native.piece_length;
            PieceCount = native.piece_count;

            var files = new TorrentFileInfo[native.length];

            var size = Marshal.SizeOf<NativeStructs.TsTorrentFile>();
            for (var i = 0; i < native.length; i++)
            {
                var ptr = nint.Add(native.items, i * size);
                var nativeFile = Marshal.PtrToStructure<NativeStructs.TsTorrentFile>(ptr);
                files[i] = new TorrentFileInfo(nativeFile);
            }

            Files = files;
        }
    }

    public sealed class TorrentMetadata
    {
        public string Name { get; }
        public string Creator { get; }
        public string Comment { get; }

        public int TotalFiles { get; }
        public long TotalSize { get; }

        public DateTimeOffset CreationTime { get; }

        public byte[] InfoHashSha1 { get; }
        public byte[] InfoHashSha256 { get; }

        internal TorrentMetadata(NativeStructs.TsTorrentMetadata meta)
        {
            Name = meta.name;
            Creator = meta.creator;
            Comment = meta.comment;

            TotalFiles = meta.total_files;
            TotalSize = meta.total_size;

            CreationTime = DateTimeOffset.FromUnixTimeSeconds(meta.creation_epoch);

            InfoHashSha1 = (byte[])meta.info_hash_sha1.Clone();
            InfoHashSha256 = (byte[])meta.info_hash_sha256.Clone();
        }
    }

    public sealed class TorrentStatus
    {
        public TorrentState State { get; }
        public float Progress { get; }
        public long TotalSize { get; }
        public int PeerCount { get; }
        public int SeedCount { get; }

        public long BytesUploaded { get; }
        public long BytesDownloaded { get; }

        public long UploadRate { get; }
        public long DownloadRate { get; }

        internal TorrentStatus(NativeStructs.TsTorrentStatus status)
        {
            State = status.State;
            Progress = status.Progress;

            TotalSize = status.TotalSize;
            PeerCount = status.PeerCount;
            SeedCount = status.SeedCount;

            BytesUploaded = status.BytesUploaded;
            BytesDownloaded = status.BytesDownloaded;

            UploadRate = status.UploadRate;
            DownloadRate = status.DownloadRate;
        }

        public override string ToString()
        {
            return
                $@"Torrent Status
            ------------------------------
            State        : {State}
            Progress     : {Progress * 100:0.00} %
            Peers / Seeds: {PeerCount} / {SeedCount}

            Downloaded   : {FormatBytes(BytesDownloaded)}
            Uploaded     : {FormatBytes(BytesUploaded)}

            Down Speed   : {FormatBytes(DownloadRate)}/s
            Up Speed     : {FormatBytes(UploadRate)}/s";
        }

        private static string FormatBytes(long bytes)
        {
            const double KB = 1024.0;
            const double MB = KB * 1024;
            const double GB = MB * 1024;

            if (bytes < KB)
                return $"{bytes} B";

            if (bytes < MB)
                return $"{bytes / KB:0.##} KB";

            if (bytes < GB)
                return $"{bytes / MB:0.##} MB";

            return $"{bytes / GB:0.##} GB";
        }
    }


}
