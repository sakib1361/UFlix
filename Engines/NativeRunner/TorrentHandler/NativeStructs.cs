using System.Runtime.InteropServices;

namespace NativeRunner.TorrentHandler
{
    internal static class NativeStructs
    {
        /// <summary>
        /// Represents a file contained within a torrent.
        /// </summary>
        [StructLayout(LayoutKind.Sequential)]
        public readonly struct TsTorrentFile
        {
            public readonly int index;

            public readonly long offset;
            public readonly long file_size;

            public readonly long modified_time;

            public readonly nint file_name;
            public readonly nint file_path;

            public readonly byte file_path_is_absolute;
            public readonly byte pad_file;
        }

        /// <summary>
        /// Represents a list of files contained within a torrent.
        /// </summary>
        [StructLayout(LayoutKind.Sequential)]
        public struct TsTorrentFileList
        {
            public int piece_length;
            public int piece_count;
            public int length;
            public nint items;
        }

        public enum TsEventType
        {
            PieceFinished = 1,
            MetadataReceived = 2,
            TorrentError = 3,
            FileError = 4,
            ResumeSaved = 5,
            TorrentFinished = 6
        }

        [StructLayout(LayoutKind.Sequential)]
        public unsafe struct TsEvent
        {
            public const int MessageSize = 240;

            public TsEventType type;
            public int piece;
            public long torrent_id;
            public fixed byte message[MessageSize];
        }

        /// <summary>
        /// Represents a single file contained within a torrent.
        /// </summary>
        [StructLayout(LayoutKind.Sequential, Pack = 8)]
        public readonly struct TsTorrentMetadata
        {
            [MarshalAs(UnmanagedType.LPUTF8Str)]
            public readonly string name;

            [MarshalAs(UnmanagedType.LPUTF8Str)]
            public readonly string creator;

            [MarshalAs(UnmanagedType.LPUTF8Str)]
            public readonly string comment;

            public readonly int total_files;
            public readonly long total_size;

            public readonly long creation_epoch;

            [MarshalAs(UnmanagedType.ByValArray, SizeConst = 20)]
            public readonly byte[] info_hash_sha1;

            [MarshalAs(UnmanagedType.ByValArray, SizeConst = 32)]
            public readonly byte[] info_hash_sha256;
        }

        [StructLayout(LayoutKind.Sequential)]
        public readonly struct TsTorrentStatus
        {
            public readonly TorrentState State;
            public readonly float Progress;
            public readonly long TotalSize;

            public readonly int PeerCount;
            public readonly int SeedCount;

            public readonly long BytesUploaded;
            public readonly long BytesDownloaded;

            public readonly long UploadRate;
            public readonly long DownloadRate;
        }

    }

    public enum FileDownloadPriority : byte
    {
        DoNotDownload = 0,
        Low = 1,
        Normal = 4,
        High = 7
    }

    public enum TorrentState
    {
        Unknown = 0,
        CheckingFiles = 1,
        CheckingResume = 2,
        DownloadingMetadata = 3,
        Downloading = 4,
        Seeding = 5,
        Finished = 6,
        Errored = 7
    }
}