using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace MpvSharp.Native;

internal enum MpvFormat
{
    None = 0,
    String = 1,
    OsdString = 2,
    Flag = 3,
    Int64 = 4,
    Double = 5,
    Node = 6,
}

internal enum MpvEventId
{
    None = 0,
    Shutdown = 1,
    LogMessage = 2,
    GetPropertyReply = 3,
    SetPropertyReply = 4,
    CommandReply = 5,
    StartFile = 6,
    EndFile = 7,
    FileLoaded = 8,
    ClientMessage = 16,
    VideoReconfig = 17,
    AudioReconfig = 18,
    Seek = 20,
    PlaybackRestart = 21,
    PropertyChange = 22,
    QueueOverflow = 24,
    Hook = 25,
}

[StructLayout(LayoutKind.Sequential)]
internal struct MpvEvent
{
    public MpvEventId EventId;
    public int Error;
    public ulong ReplyUserdata;
    public nint Data;
}

[StructLayout(LayoutKind.Sequential)]
internal struct MpvEventProperty
{
    public nint Name;
    public MpvFormat Format;
    public nint Data;
}

[StructLayout(LayoutKind.Sequential)]
internal struct MpvEventEndFile
{
    public int Reason;
    public int Error;
    public long PlaylistEntryId;
    public long PlaylistInsertId;
    public int PlaylistInsertNumEntries;
}

[StructLayout(LayoutKind.Sequential)]
internal struct MpvEventLogMessage
{
    public nint Prefix;
    public nint Level;
    public nint Text;
    public int LogLevel;
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct MpvStreamCbInfo
{
    public nint Cookie;
    public delegate* unmanaged[Cdecl]<nint, byte*, ulong, long> Read;
    public delegate* unmanaged[Cdecl]<nint, long, long> Seek;
    public delegate* unmanaged[Cdecl]<nint, long> Size;
    public delegate* unmanaged[Cdecl]<nint, void> Close;
    public delegate* unmanaged[Cdecl]<nint, void> Cancel;
}

internal static unsafe partial class MpvNative
{
    private const string Lib = "mpv";

    public const int ErrorLoadingFailed = -13;
    public const int ErrorUnsupported = -18;
    public const int ErrorGeneric = -20;

    static MpvNative()
    {
        NativeLibrary.SetDllImportResolver(typeof(MpvNative).Assembly, Resolve);
    }

    /// <summary>
    /// $MPV_LIB (debugging only) → the app's bundled copy in runtimes/&lt;rid&gt;/native → the app folder. The system libmpv is only
    /// used when no bundled copy exists (development machines), and that is logged.
    /// </summary>
    private static nint Resolve(string name, Assembly assembly, DllImportSearchPath? path)
    {
        if (name != Lib) return 0;
        var file = OperatingSystem.IsWindows() ? "libmpv-2.dll" : OperatingSystem.IsMacOS() ? "libmpv.2.dylib" : "libmpv.so.2";
        var overridePath = Environment.GetEnvironmentVariable("MPV_LIB");
        if (!string.IsNullOrEmpty(overridePath) && NativeLibrary.TryLoad(overridePath, out var h)) return h;
        var baseDir = AppContext.BaseDirectory;
        foreach (var candidate in new[]
                 {
                     Path.Combine(baseDir, "runtimes", RuntimeInformation.RuntimeIdentifier, "native", file),
                     Path.Combine(baseDir, "runtimes", PortableRid, "native", file),
                     Path.Combine(baseDir, file),
                 })
        {
            if (File.Exists(candidate) && NativeLibrary.TryLoad(candidate, out h)) return h;
        }
        if (NativeLibrary.TryLoad(file, assembly, path, out h))
        {
            SystemLibraryUsed = true;
            return h;
        }
        return 0;
    }

    private static string PortableRid =>
        (OperatingSystem.IsWindows() ? "win" : OperatingSystem.IsMacOS() ? "osx" : "linux") + "-" +
        RuntimeInformation.ProcessArchitecture.ToString().ToLowerInvariant();

    public static bool SystemLibraryUsed { get; private set; }

    [LibraryImport(Lib, EntryPoint = "mpv_create")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    private static partial nint mpv_create_raw();

    [LibraryImport("libc", StringMarshalling = StringMarshalling.Utf8)]
    private static partial nint setlocale(int category, string locale);

    /// <summary>libmpv refuses to start unless LC_NUMERIC is "C" (mpv_create returns NULL on e.g. de_DE.UTF-8), so it is reset before every create.</summary>
    public static nint mpv_create()
    {
        if (!OperatingSystem.IsWindows()) setlocale(1 /* LC_NUMERIC */, "C");
        return mpv_create_raw();
    }

    [LibraryImport(Lib), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial ulong mpv_client_api_version();

    [LibraryImport(Lib), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial int mpv_initialize(nint ctx);

    [LibraryImport(Lib), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial void mpv_terminate_destroy(nint ctx);

    [LibraryImport(Lib), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial void mpv_wakeup(nint ctx);

    [LibraryImport(Lib), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial nint mpv_error_string(int error);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial int mpv_set_option_string(nint ctx, string name, string data);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial int mpv_set_property_string(nint ctx, string name, string data);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial int mpv_set_property(nint ctx, string name, MpvFormat format, void* data);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial int mpv_get_property(nint ctx, string name, MpvFormat format, void* data);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial nint mpv_get_property_string(nint ctx, string name);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial int mpv_observe_property(nint ctx, ulong replyUserdata, string name, MpvFormat format);

    [LibraryImport(Lib), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial void mpv_free(nint data);

    [LibraryImport(Lib), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial int mpv_command(nint ctx, nint* args);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial int mpv_request_log_messages(nint ctx, string minLevel);

    [LibraryImport(Lib), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial MpvEvent* mpv_wait_event(nint ctx, double timeout);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8), UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial int mpv_stream_cb_add_ro(nint ctx, string protocol, nint userData, delegate* unmanaged[Cdecl]<nint, byte*, MpvStreamCbInfo*, int> openFn);

    public static string ErrorString(int error) => Marshal.PtrToStringUTF8(mpv_error_string(error));

    public static string GetPropertyString(nint ctx, string name)
    {
        var p = mpv_get_property_string(ctx, name);
        if (p == 0) return null;
        try { return Marshal.PtrToStringUTF8(p); }
        finally { mpv_free(p); }
    }

    /// <summary>mpv_command with a NULL-terminated UTF-8 argv.</summary>
    public static int Command(nint ctx, params string[] args)
    {
        var argv = stackalloc nint[args.Length + 1];
        try
        {
            for (int i = 0; i < args.Length; i++) argv[i] = Marshal.StringToCoTaskMemUTF8(args[i]);
            argv[args.Length] = 0;
            return mpv_command(ctx, argv);
        }
        finally
        {
            for (int i = 0; i < args.Length; i++) Marshal.FreeCoTaskMem(argv[i]);
        }
    }
}
