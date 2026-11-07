// Headless smoke test for tsdl + NativeRunner (doc/CrossPlatformDev.md).
// usage: dotnet run --project Engines/NativeRunner.Smoke -- [download folder]
using System.Diagnostics;
using NativeRunner.TorrentHandler;

const string Magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Big+Buck+Bunny"
    + "&tr=udp%3A%2F%2Fexplodie.org%3A6969&tr=udp%3A%2F%2Ftracker.opentrackr.org%3A1337"
    + "&tr=wss%3A%2F%2Ftracker.openwebtorrent.com&ws=https%3A%2F%2Fwebtorrent.io%2Ftorrents%2F"
    + "&xs=https%3A%2F%2Fwebtorrent.io%2Ftorrents%2Fbig-buck-bunny.torrent";

var folder = args.Length > 0 ? args[0] : Path.Combine(Path.GetTempPath(), "tsdlsmoke");
var failures = 0;
var clock = Stopwatch.StartNew();
void Check(bool ok, string what)
{
    Console.WriteLine($"[{clock.Elapsed.TotalSeconds,6:0.0}s] {(ok ? "PASS" : "FAIL")} {what}");
    if (!ok) failures++;
}

var session = LibSession.Instance;
Check(true, $"session created, tsdl {LibSession.NativeVersion}");

var bad = session.AddTorrent("magnet:?this-is-not-valid", folder);
Check(bad.IsDisposed && !string.IsNullOrEmpty(bad.LastError), $"bad magnet rejected without crashing ({bad.LastError})");

var handler = session.AddTorrent(Magnet, folder);
Check(!handler.IsDisposed, "magnet attached");

TorrentFileList files = null;
var metaDeadline = Stopwatch.StartNew();
while (files == null && metaDeadline.Elapsed < TimeSpan.FromMinutes(3))
{
    files = await handler.GetFileListAsync();
    if (files == null) await Task.Delay(250);
}
Check(files != null, "metadata received");
if (files == null) return Exit();

foreach (var f in files.Files) Console.WriteLine($"         #{f.Index} {f.FilePath} ({f.FileSize:N0} B)");
Console.WriteLine($"         pieces: {files.PieceCount} x {files.PieceLength:N0} B");

var video = files.Files.Where(f => f.FilePath.EndsWith(".mp4")).OrderByDescending(f => f.FileSize).First();
Check(await handler.HasPiece(-1) == false && await handler.HasPiece(int.MaxValue) == false, "out-of-range piece index is handled");

var sw = Stopwatch.StartNew();
using var stream = await handler.InitializeStream(video);
Check(stream != null, $"stream initialised in {sw.Elapsed.TotalSeconds:0.0}s, Length={stream?.Length:N0}");
if (stream == null) return Exit();

var libStream = stream as LibStream;
Check(libStream != null, "stream is a LibStream (file not complete yet)");

sw.Restart();
var buffer = new byte[256 * 1024];
long total = 0;
var reads = 0;
while (total < 4 * 1024 * 1024)
{
    var n = stream.Read(buffer, 0, buffer.Length);
    if (n <= 0) break;
    total += n;
    reads++;
}
Check(total >= 4 * 1024 * 1024, $"read {total:N0} B in {reads} reads, {sw.Elapsed.TotalSeconds:0.0}s");

if (libStream != null)
{
    Console.WriteLine($"         runway from 0: {libStream.ContiguousBytesFrom(0):N0} B");

    // find a missing piece far into the file so the read must block
    var target = video.FileSize * 3 / 4;
    Check(libStream.ContiguousBytesFrom(target) == 0, $"nothing downloaded yet at {target:N0}");
    libStream.Seek(target, SeekOrigin.Begin);

    var started = new ManualResetEventSlim();
    int blockedResult = -2;
    var blockedFor = Stopwatch.StartNew();
    var reader = new Thread(() => { started.Set(); blockedResult = stream.Read(buffer, 0, buffer.Length); });
    reader.Start();
    started.Wait();
    Thread.Sleep(1500);
    var stillBlocked = reader.IsAlive;
    var cancelAt = blockedFor.Elapsed;
    libStream.Cancel();
    var joined = reader.Join(2000);
    var wake = blockedFor.Elapsed - cancelAt;
    Check(stillBlocked, "read at a missing piece blocks");
    Check(joined && wake < TimeSpan.FromMilliseconds(200), $"Cancel() woke the blocked read in {wake.TotalMilliseconds:0} ms (returned {blockedResult})");
    Check(stream.Read(buffer, 0, buffer.Length) == 0, "reads after Cancel() return 0");
}

sw.Restart();
await handler.DisposeAsync();
Check(handler.IsDisposed, $"handler disposed (detach + resume save) in {sw.Elapsed.TotalMilliseconds:0} ms");

var resume = Directory.GetFiles(Path.Combine(folder, "_cache"), "*.resume");
Check(resume.Length > 0 && new FileInfo(resume[0]).Length > 0, $"resume data written ({string.Join(", ", resume.Select(Path.GetFileName))})");

sw.Restart();
session.Dispose();
Check(true, $"session destroyed in {sw.Elapsed.TotalMilliseconds:0} ms");
return Exit();

int Exit()
{
    Console.WriteLine(failures == 0 ? "ALL PASSED" : $"{failures} FAILED");
    return failures == 0 ? 0 : 1;
}
