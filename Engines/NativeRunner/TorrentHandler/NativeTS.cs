using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace NativeRunner.TorrentHandler
{
    internal static unsafe partial class NativeTS
    {
        private const string LibraryName = "tsdl";

        public const int Ok = 0;
        public const int ErrorNoMetadata = -3;

        // Must run before any P/Invoke into tsdl, whichever type the app touches first.
#pragma warning disable CA2255
        [ModuleInitializer]
#pragma warning restore CA2255
        internal static void RegisterResolver()
        {
            NativeLibrary.SetDllImportResolver(typeof(NativeTS).Assembly, Resolve);
        }

        // $TSDL_LIB (file or folder) first, then runtimes/<rid>/native and the app folder, then the default probing.
        private static nint Resolve(string name, Assembly assembly, DllImportSearchPath? searchPath)
        {
            if (name != LibraryName) return 0;

            var fileName = OperatingSystem.IsWindows() ? "tsdl.dll" : OperatingSystem.IsMacOS() ? "libtsdl.dylib" : "libtsdl.so";
            var os = OperatingSystem.IsWindows() ? "win" : OperatingSystem.IsMacOS() ? "osx" : "linux";
            var rid = $"{os}-{RuntimeInformation.ProcessArchitecture.ToString().ToLowerInvariant()}";

            var candidates = new List<string>();
            var overridePath = Environment.GetEnvironmentVariable("TSDL_LIB");
            if (!string.IsNullOrWhiteSpace(overridePath))
            {
                candidates.Add(Directory.Exists(overridePath) ? Path.Combine(overridePath, fileName) : overridePath);
            }
            candidates.Add(Path.Combine(AppContext.BaseDirectory, "runtimes", rid, "native", fileName));
            candidates.Add(Path.Combine(AppContext.BaseDirectory, fileName));

            foreach (var candidate in candidates)
            {
                if (File.Exists(candidate) && NativeLibrary.TryLoad(candidate, out var handle)) return handle;
            }
            return NativeLibrary.TryLoad(name, assembly, searchPath, out var fallback) ? fallback : 0;
        }

        [LibraryImport(LibraryName, EntryPoint = "tsdl_version")]
        private static partial nint VersionPtr();
        public static string Version() => Marshal.PtrToStringUTF8(VersionPtr());

        [LibraryImport(LibraryName, EntryPoint = "tsdl_last_error")]
        private static partial nint LastErrorPtr();
        public static string LastError() => Marshal.PtrToStringUTF8(LastErrorPtr());

        [LibraryImport(LibraryName, EntryPoint = "tsdl_session_create")]
        public static partial nint CreateSession();

        [LibraryImport(LibraryName, EntryPoint = "tsdl_session_destroy")]
        public static partial void DestroySession(nint session);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_poll_events")]
        public static partial int PollEvents(nint session, NativeStructs.TsEvent* events, int capacity, int timeoutMs);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_torrent_info_from_file", StringMarshalling = StringMarshalling.Utf8)]
        public static partial nint TorrentInfoFromFile(string filePath);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_torrent_info_destroy")]
        public static partial void DestroyTorrentInfo(nint info);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_attach_magnet", StringMarshalling = StringMarshalling.Utf8)]
        public static partial nint AttachMagnet(nint session, string magnet, string savePath, string resumePath);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_attach_torrent", StringMarshalling = StringMarshalling.Utf8)]
        public static partial nint AttachTorrent(nint session, nint info, string savePath, string resumePath);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_detach_torrent")]
        public static partial int DetachTorrent(nint session, nint torrent);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_torrent_id")]
        public static partial long TorrentId(nint torrent);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_get_file_list")]
        public static partial int GetTorrentFileList(nint torrent, out NativeStructs.TsTorrentFileList files);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_destroy_file_list")]
        public static partial void DestroyTorrentFileList(ref NativeStructs.TsTorrentFileList files);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_get_file_priority")]
        public static partial int GetFilePriority(nint torrent, int fileIndex);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_set_file_priority")]
        public static partial int SetFilePriority(nint torrent, int fileIndex, FileDownloadPriority priority);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_has_piece")]
        public static partial int HasPiece(nint torrent, int pieceIndex);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_get_pieces")]
        public static partial int GetPieces(nint torrent, byte* have, int capacity);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_focus_pieces")]
        public static partial int FocusPieces(nint torrent, int start, int end, int lastPiece);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_reset_pieces")]
        public static partial int ResetPieces(nint torrent);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_save_resume_data")]
        public static partial int SaveResumeData(nint torrent);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_get_status")]
        public static partial int GetTorrentStatus(nint torrent, out NativeStructs.TsTorrentStatus status);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_start_torrent")]
        public static partial int StartTorrent(nint torrent);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_stop_torrent")]
        public static partial int StopTorrent(nint torrent);

        [LibraryImport(LibraryName, EntryPoint = "tsdl_file_progress")]
        public static partial long GetFileProgress(nint torrent, int fileIndex);
    }
}
