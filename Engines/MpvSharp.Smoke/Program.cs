using System.Diagnostics;
using MpvSharp;

// Headless checks for MpvSharp (vo=null, ao=null): file playback, transport, tracks, stream_cb playback, stop while a read
// is blocked (cancel_fn), and a clean shutdown. Usage: MpvSharp.Smoke <media file with audio>   (MPV_LIB overrides libmpv)
if (args.Length == 0 || !File.Exists(args[0]))
{
    Console.WriteLine("usage: MpvSharp.Smoke <media file>");
    return 2;
}
var media = args[0];
int failures = 0;

void Check(string name, bool ok, string detail = "")
{
    Console.WriteLine($"{(ok ? "PASS" : "FAIL")}  {name}  {detail}");
    if (!ok) failures++;
}

static async Task<bool> WaitFor(Func<bool> condition, int ms)
{
    var sw = Stopwatch.StartNew();
    while (sw.ElapsedMilliseconds < ms)
    {
        if (condition()) return true;
        await Task.Delay(20);
    }
    return condition();
}

var options = new MpvOptions { Vo = "null", Ao = "null", Hwdec = "no", LogLevel = "warn" };
Console.WriteLine($"libmpv client API {MpvPlayer.ClientApiVersion}");

// 1. file path
await using (var player = MpvPlayer.Create(options))
{
    player.Log += l => Console.WriteLine("   mpv " + l);
    Console.WriteLine($"   system libmpv: {MpvPlayer.UsesSystemLibrary}");
    var sw = Stopwatch.StartNew();
    await player.LoadAsync(media, new LoadOptions { Title = "smoke" });
    Check("file: load", player.IsLoaded, $"{sw.ElapsedMilliseconds} ms");
    player.Play();
    Check("file: position advances", await WaitFor(() => player.Position > TimeSpan.FromSeconds(1), 5000), $"pos={player.Position}");
    Check("file: duration", player.Duration > TimeSpan.Zero, $"{player.Duration}");
    Check("file: tracks", player.Tracks.Any(t => t.Type == TrackType.Video) && player.Tracks.Any(t => t.Type == TrackType.Audio),
        string.Join(", ", player.Tracks.Select(t => $"{t.Type}#{t.Id}:{t.Codec}")));
    Check("file: title", player.GetProperty("media-title") == "smoke", player.GetProperty("media-title"));

    player.Pause();
    Check("pause", await WaitFor(() => player.IsPaused, 1000));
    player.Seek(TimeSpan.FromSeconds(6), exact: true);
    Check("seek", await WaitFor(() => Math.Abs(player.Position.TotalSeconds - 6) < 0.3, 3000), $"pos={player.Position}");

    player.Volume = 150;
    Check("volume > 100", Math.Abs(player.Volume - 150) < 0.01, $"{player.Volume}");
    player.SubtitleDelay = TimeSpan.FromSeconds(0.5);
    Check("sub-delay", Math.Abs(player.SubtitleDelay.TotalSeconds - 0.5) < 0.01);

    var ended = new TaskCompletionSource<EndReason>();
    player.Ended += (r, _) => ended.TrySetResult(r);
    player.Seek(player.Duration - TimeSpan.FromSeconds(1), exact: true);
    player.Play();
    var eof = await Task.WhenAny(ended.Task, Task.Delay(5000)) == ended.Task;
    Check("eof raises Ended", eof && ended.Task.Result == EndReason.Eof, $"pos={player.Position}");

    var bad = Task.Run(() => player.LoadAsync(media + ".missing"));
    Check("missing file fails", await WaitFor(() => bad.IsCompleted, 5000) && bad.IsFaulted, bad.Exception?.InnerException?.Message);
}

// 2. stream_cb: a stream that stalls after 1 MB until cancelled
await using (var player = MpvPlayer.Create(options))
{
    using var stream = new StallingStream(File.OpenRead(media), stallAfter: 1024 * 1024);
    await player.LoadAsync(stream, "stream.mkv");
    Check("stream: load", player.IsLoaded, $"length={stream.Length}");
    player.Play();
    Check("stream: blocked on stall", await WaitFor(() => stream.Blocked, 8000), $"pos={player.Position}");

    var stopped = new TaskCompletionSource();
    player.Ended += (r, _) => { if (r == EndReason.Stop) stopped.TrySetResult(); };
    var sw = Stopwatch.StartNew();
    player.Stop();
    var ok = await Task.WhenAny(stopped.Task, Task.Delay(3000)) == stopped.Task;
    Check("stream: stop while blocked returns", ok && stream.Cancelled, $"{sw.ElapsedMilliseconds} ms, cancel_fn={stream.Cancelled}");

    sw.Restart();
    await player.DisposeAsync();
    Check("dispose", sw.ElapsedMilliseconds < 3000, $"{sw.ElapsedMilliseconds} ms");
}

Console.WriteLine(failures == 0 ? "ALL PASSED" : $"{failures} FAILED");
return failures == 0 ? 0 : 1;

internal sealed class StallingStream(Stream inner, long stallAfter) : Stream, ICancellableStream
{
    private readonly ManualResetEventSlim _cancel = new();
    public bool Blocked { get; private set; }
    public bool Cancelled => _cancel.IsSet;

    public void CancelRead() => _cancel.Set();

    public override int Read(byte[] buffer, int offset, int count) => Read(buffer.AsSpan(offset, count));

    public override int Read(Span<byte> buffer)
    {
        if (Position >= stallAfter)
        {
            Blocked = true;
            _cancel.Wait();
            throw new OperationCanceledException();
        }
        return inner.Read(buffer[..(int)Math.Min(buffer.Length, stallAfter - Position)]);
    }

    public override bool CanRead => true;
    public override bool CanSeek => true;
    public override bool CanWrite => false;
    public override long Length => inner.Length;
    public override long Position { get => inner.Position; set => inner.Position = value; }
    public override long Seek(long offset, SeekOrigin origin) => inner.Seek(offset, origin);
    public override void Flush() { }
    public override void SetLength(long value) => throw new NotSupportedException();
    public override void Write(byte[] buffer, int offset, int count) => throw new NotSupportedException();

    protected override void Dispose(bool disposing)
    {
        if (disposing) inner.Dispose();
        base.Dispose(disposing);
    }
}
