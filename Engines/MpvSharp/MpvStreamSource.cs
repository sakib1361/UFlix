using System.Collections.Concurrent;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using MpvSharp.Native;

namespace MpvSharp;

/// <summary>
/// Serves .NET streams to mpv through mpv_stream_cb_add_ro as "uflix://&lt;id&gt;". The callbacks run on mpv's demuxer thread:
/// they must never call back into libmpv and must never let an exception escape.
/// </summary>
internal static unsafe class MpvStreamSource
{
    public const string Protocol = "uflix";

    private sealed record Entry(Stream Stream, Action Cancel);

    private static readonly ConcurrentDictionary<long, Entry> Streams = new();
    private static long _nextId;

    public static void Register(nint ctx)
    {
        int r = MpvNative.mpv_stream_cb_add_ro(ctx, Protocol, 0, &Open);
        if (r < 0) throw new MpvException("stream_cb_add_ro", r);
    }

    public static string Add(Stream stream, Action cancel)
    {
        var id = Interlocked.Increment(ref _nextId);
        if (cancel == null && stream is ICancellableStream cancellable) cancel = cancellable.CancelRead;
        Streams[id] = new Entry(stream, cancel);
        return $"{Protocol}://{id}";
    }

    public static void Remove(string uri)
    {
        if (TryParse(uri, out var id)) Streams.TryRemove(id, out _);
    }

    private static bool TryParse(string uri, out long id)
    {
        id = 0;
        return uri != null && uri.StartsWith(Protocol + "://") && long.TryParse(uri.AsSpan(Protocol.Length + 3), out id);
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static int Open(nint user, byte* uri, MpvStreamCbInfo* info)
    {
        try
        {
            if (!TryParse(Marshal.PtrToStringUTF8((nint)uri), out var id) || !Streams.TryGetValue(id, out var entry)) return MpvNative.ErrorLoadingFailed;
            info->Cookie = GCHandle.ToIntPtr(GCHandle.Alloc(entry));
            info->Read = &Read;
            info->Seek = &Seek;
            info->Size = &Size;
            info->Close = &Close;
            info->Cancel = &Cancel;
            return 0;
        }
        catch { return MpvNative.ErrorLoadingFailed; }
    }

    private static Entry Get(nint cookie) => (Entry)GCHandle.FromIntPtr(cookie).Target;

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static long Read(nint cookie, byte* buffer, ulong size)
    {
        try { return Get(cookie).Stream.Read(new Span<byte>(buffer, (int)Math.Min(size, int.MaxValue))); }
        catch { return -1; }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static long Seek(nint cookie, long offset)
    {
        try { return Get(cookie).Stream.Seek(offset, SeekOrigin.Begin); }
        catch { return MpvNative.ErrorGeneric; }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static long Size(nint cookie)
    {
        try { return Get(cookie).Stream.Length; }
        catch { return MpvNative.ErrorUnsupported; }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void Close(nint cookie)
    {
        try { GCHandle.FromIntPtr(cookie).Free(); }
        catch { }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void Cancel(nint cookie)
    {
        try { Get(cookie).Cancel?.Invoke(); }
        catch { }
    }
}
