using System.Globalization;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using MpvSharp.Native;

namespace MpvSharp;

/// <summary>
/// One libmpv instance. UI-free: a head embeds the video by passing its video-output options in <see cref="MpvOptions.Extra"/>.
/// All events are raised on the MpvSharp event thread; handlers must marshal to their UI thread themselves and should not block.
/// </summary>
public sealed class MpvPlayer : IAsyncDisposable
{
    private enum Observed : ulong
    {
        TimePos = 1,
        Duration,
        Pause,
        PausedForCache,
        EofReached,
        TrackList,
        SubDelay,
        DisplaySwapchain,
    }

    private static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

    private readonly nint _ctx;
    private readonly Thread _eventThread;
    private readonly object _loadLock = new();
    private TaskCompletionSource _load;
    private bool _awaitingStart;
    private string _streamUri;
    private volatile bool _disposed;
    private volatile bool _watchSwapChain;
    private long _swapChain;
    private IReadOnlyList<MpvTrack> _tracks = [];

    public TimeSpan Position { get; private set; }
    public TimeSpan Duration { get; private set; }
    public bool IsPaused { get; private set; } = true;
    public bool IsBuffering { get; private set; }
    public bool IsLoaded { get; private set; }
    public IReadOnlyList<MpvTrack> Tracks => _tracks;
    /// <summary>True when no bundled libmpv was found and the system copy was loaded instead (development only).</summary>
    public static bool UsesSystemLibrary => MpvNative.SystemLibraryUsed;

    public event Action<TimeSpan> PositionChanged;
    public event Action<TimeSpan> DurationChanged;
    public event Action<bool> PauseChanged;
    /// <summary>paused-for-cache: mpv ran out of buffered data and is waiting for the input.</summary>
    public event Action<bool> BufferingChanged;
    public event Action<EndReason, int> Ended;
    public event Action TracksChanged;
    /// <summary>Raised when mpv publishes (or re-publishes) its composition swapchain; see <see cref="WatchSwapChain"/>.</summary>
    public event Action<nint> SwapChainReady;
    public event Action<string> Log;

    private MpvPlayer(MpvOptions options)
    {
        _ctx = MpvNative.mpv_create();
        if (_ctx == 0) throw new InvalidOperationException("mpv_create failed (is libmpv present?)");

        Opt("terminal", "no");
        Opt("config", "no");
        Opt("idle", "yes");
        Opt("keep-open", "yes");
        Opt("osc", "no");
        Opt("ytdl", "no");
        Opt("input-default-bindings", "no");
        Opt("input-vo-keyboard", "no");
        Opt("input-cursor", "no");
        Opt("cursor-autohide", "no");
        Opt("stop-screensaver", "yes");
        Opt("demuxer-max-bytes", "32MiB");
        Opt("demuxer-max-back-bytes", "8MiB");
        Opt("hwdec", options.Hwdec);
        Opt("vo", options.Vo);
        Opt("volume-max", options.VolumeMax.ToString(Inv));
        if (!string.IsNullOrEmpty(options.Ao)) Opt("ao", options.Ao);
        foreach (var (k, v) in options.Extra) Opt(k, v);

        int r = MpvNative.mpv_initialize(_ctx);
        if (r < 0)
        {
            MpvNative.mpv_terminate_destroy(_ctx);
            throw new MpvException("mpv_initialize", r);
        }
        MpvNative.mpv_request_log_messages(_ctx, options.LogLevel);
        MpvNative.mpv_observe_property(_ctx, (ulong)Observed.TimePos, "time-pos", MpvFormat.Double);
        MpvNative.mpv_observe_property(_ctx, (ulong)Observed.Duration, "duration", MpvFormat.Double);
        MpvNative.mpv_observe_property(_ctx, (ulong)Observed.Pause, "pause", MpvFormat.Flag);
        MpvNative.mpv_observe_property(_ctx, (ulong)Observed.PausedForCache, "paused-for-cache", MpvFormat.Flag);
        MpvNative.mpv_observe_property(_ctx, (ulong)Observed.EofReached, "eof-reached", MpvFormat.Flag);
        MpvNative.mpv_observe_property(_ctx, (ulong)Observed.TrackList, "track-list", MpvFormat.String);
        MpvNative.mpv_observe_property(_ctx, (ulong)Observed.SubDelay, "sub-delay", MpvFormat.Double);
        MpvStreamSource.Register(_ctx);

        _eventThread = new Thread(EventLoop) { IsBackground = true, Name = "mpv-events" };
        _eventThread.Start();
    }

    public static MpvPlayer Create(MpvOptions options = null) => new(options ?? new MpvOptions());

    public static Version ClientApiVersion
    {
        get
        {
            var v = MpvNative.mpv_client_api_version();
            return new Version((int)(v >> 16), (int)(v & 0xffff));
        }
    }

    private void Opt(string name, string value)
    {
        int r = MpvNative.mpv_set_option_string(_ctx, name, value);
        if (r < 0) Log?.Invoke($"[mpvsharp] option {name}={value}: {MpvNative.ErrorString(r)}");
    }

    // ---- embedding ----

    /// <summary>For d3d11-output-mode=composition: polls display-swapchain and raises <see cref="SwapChainReady"/> whenever mpv (re)publishes it.</summary>
    public void WatchSwapChain()
    {
        if (_watchSwapChain) return;
        _watchSwapChain = true;
        MpvNative.mpv_observe_property(_ctx, (ulong)Observed.DisplaySwapchain, "display-swapchain", MpvFormat.String);
        CheckSwapChain();
    }

    // event-driven, no polling: checked when watching starts, when the property notifies, and on every VIDEO_RECONFIG
    // (mpv re-creates the swapchain together with its VO)
    private void CheckSwapChain()
    {
        if (!_watchSwapChain || _disposed) return;
        if (long.TryParse(GetProperty("display-swapchain"), out var v) && v != 0 && Interlocked.Exchange(ref _swapChain, v) != v)
            SwapChainReady?.Invoke((nint)v);
    }

    // ---- loading ----

    public Task LoadAsync(string url, LoadOptions options = null) => LoadCore(url, options ?? new LoadOptions());

    /// <summary>Plays a .NET stream through mpv's stream_cb. <paramref name="cancel"/> (or <see cref="ICancellableStream"/>) is
    /// called by mpv on stop so a read blocked on missing data returns at once. The caller keeps ownership of the stream.</summary>
    public Task LoadAsync(Stream stream, string name, LoadOptions options = null, Action cancel = null)
    {
        var uri = MpvStreamSource.Add(stream, cancel);
        options ??= new LoadOptions();
        return LoadCore(uri, new LoadOptions
        {
            AudioUrl = options.AudioUrl,
            Subtitles = options.Subtitles,
            Start = options.Start,
            Title = options.Title ?? name,
        });
    }

    private async Task LoadCore(string url, LoadOptions options)
    {
        ThrowIfDisposed();
        TaskCompletionSource tcs;
        lock (_loadLock)
        {
            _load?.TrySetCanceled();
            _load = tcs = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            _awaitingStart = true;
            if (_streamUri != null) MpvStreamSource.Remove(_streamUri);
            _streamUri = url.StartsWith(MpvStreamSource.Protocol + "://") ? url : null;
        }
        IsLoaded = false;

        var fileOptions = new List<string>();
        if (options.Start is { } start && start > TimeSpan.Zero) fileOptions.Add(FileOpt("start", start.TotalSeconds.ToString("0.###", Inv)));
        if (!string.IsNullOrEmpty(options.Title)) fileOptions.Add(FileOpt("force-media-title", options.Title));

        SetProperty("pause", "yes");
        int r = MpvNative.Command(_ctx, "loadfile", url, "replace", "-1", string.Join(",", fileOptions));
        if (r < 0) throw new MpvException("loadfile", r);
        await tcs.Task.ConfigureAwait(false);

        if (!string.IsNullOrEmpty(options.AudioUrl)) Command("audio-add", options.AudioUrl, "select");
        foreach (var sub in options.Subtitles ?? []) AddSubtitle(sub, false);
    }

    // mpv's %length%value quoting: the value may contain commas, colons or '=' (URLs, titles)
    private static string FileOpt(string name, string value) => $"{name}=%{Encoding.UTF8.GetByteCount(value)}%{value}";

    // ---- transport ----

    public void Play() => SetProperty("pause", "no");
    public void Pause() => SetProperty("pause", "yes");
    public void Stop() => Command("stop");

    public void Seek(TimeSpan position, bool exact = false) =>
        Command("seek", Math.Max(0, position.TotalSeconds).ToString("0.###", Inv), exact ? "absolute+exact" : "absolute+keyframes");

    public double Volume
    {
        get => GetDouble("volume");
        set => SetDouble("volume", value);
    }

    public TimeSpan SubtitleDelay
    {
        get => TimeSpan.FromSeconds(GetDouble("sub-delay"));
        set => SetDouble("sub-delay", value.TotalSeconds);
    }

    public void AddSubtitle(string pathOrUrl, bool select = true) => Command("sub-add", pathOrUrl, select ? "select" : "auto");

    /// <summary>Selects a track; <paramref name="id"/> &lt; 0 turns that track type off.</summary>
    public void SelectTrack(TrackType type, long id) =>
        SetProperty(type switch { TrackType.Video => "vid", TrackType.Audio => "aid", _ => "sid" }, id < 0 ? "no" : id.ToString(Inv));

    // ---- raw access ----

    public string GetProperty(string name) => _disposed ? null : MpvNative.GetPropertyString(_ctx, name);

    private unsafe double GetDouble(string name)
    {
        if (_disposed) return 0;
        double v = 0;
        return MpvNative.mpv_get_property(_ctx, name, MpvFormat.Double, &v) < 0 ? 0 : v;
    }

    public void SetProperty(string name, string value)
    {
        if (_disposed) return;
        int r = MpvNative.mpv_set_property_string(_ctx, name, value);
        if (r < 0) Log?.Invoke($"[mpvsharp] set {name}={value}: {MpvNative.ErrorString(r)}");
    }

    private unsafe void SetDouble(string name, double value)
    {
        if (_disposed) return;
        int r = MpvNative.mpv_set_property(_ctx, name, MpvFormat.Double, &value);
        if (r < 0) Log?.Invoke($"[mpvsharp] set {name}={value}: {MpvNative.ErrorString(r)}");
    }

    public int Command(params string[] args)
    {
        if (_disposed) return MpvNative.ErrorGeneric;
        int r = MpvNative.Command(_ctx, args);
        if (r < 0) Log?.Invoke($"[mpvsharp] command {string.Join(' ', args)}: {MpvNative.ErrorString(r)}");
        return r;
    }

    // ---- events ----

    private unsafe void EventLoop()
    {
        while (true)
        {
            var ev = MpvNative.mpv_wait_event(_ctx, -1);
            try
            {
                switch (ev->EventId)
                {
                    case MpvEventId.Shutdown:
                        lock (_loadLock) _load?.TrySetCanceled();
                        return;
                    case MpvEventId.LogMessage:
                        var m = (MpvEventLogMessage*)ev->Data;
                        Log?.Invoke($"[{Marshal.PtrToStringUTF8(m->Prefix)}] {Marshal.PtrToStringUTF8(m->Text)?.TrimEnd()}");
                        break;
                    case MpvEventId.StartFile:
                        lock (_loadLock) _awaitingStart = false;
                        break;
                    case MpvEventId.VideoReconfig:
                        CheckSwapChain();
                        break;
                    case MpvEventId.FileLoaded:
                        IsLoaded = true;
                        lock (_loadLock) _load?.TrySetResult();
                        break;
                    case MpvEventId.EndFile:
                        OnEndFile((MpvEventEndFile*)ev->Data);
                        break;
                    case MpvEventId.PropertyChange:
                        OnProperty((Observed)ev->ReplyUserdata, (MpvEventProperty*)ev->Data);
                        break;
                }
            }
            catch (Exception ex)
            {
                Log?.Invoke($"[mpvsharp] event handler failed: {ex}");
            }
        }
    }

    private unsafe void OnEndFile(MpvEventEndFile* e)
    {
        var reason = (EndReason)e->Reason;
        lock (_loadLock)
        {
            // the END_FILE of the file being replaced arrives before our START_FILE: it isn't ours
            if (_awaitingStart) return;
            if (_load is { Task.IsCompleted: false } load)
            {
                if (reason == EndReason.Error) load.TrySetException(new MpvException("load failed", e->Error));
                else load.TrySetCanceled();
            }
            if (_streamUri != null)
            {
                MpvStreamSource.Remove(_streamUri);
                _streamUri = null;
            }
        }
        IsLoaded = false;
        Ended?.Invoke(reason, e->Error);
    }

    private unsafe void OnProperty(Observed id, MpvEventProperty* p)
    {
        if (p->Format == MpvFormat.None || p->Data == 0)
        {
            if (id == Observed.TrackList) SetTracks(null);
            return;
        }
        switch (id)
        {
            case Observed.TimePos:
                Position = TimeSpan.FromSeconds(*(double*)p->Data);
                PositionChanged?.Invoke(Position);
                break;
            case Observed.Duration:
                Duration = TimeSpan.FromSeconds(*(double*)p->Data);
                DurationChanged?.Invoke(Duration);
                break;
            case Observed.Pause:
                IsPaused = *(int*)p->Data != 0;
                PauseChanged?.Invoke(IsPaused);
                break;
            case Observed.PausedForCache:
                IsBuffering = *(int*)p->Data != 0;
                BufferingChanged?.Invoke(IsBuffering);
                break;
            case Observed.EofReached:
                // keep-open=yes: a file that plays to its end stays open and paused, so EOF shows up here instead of END_FILE
                if (*(int*)p->Data != 0) Ended?.Invoke(EndReason.Eof, 0);
                break;
            case Observed.TrackList:
                SetTracks(Marshal.PtrToStringUTF8(*(nint*)p->Data));
                break;
            case Observed.DisplaySwapchain:
                CheckSwapChain();
                break;
        }
    }

    private void SetTracks(string json)
    {
        var list = new List<MpvTrack>();
        if (!string.IsNullOrEmpty(json))
        {
            using var doc = JsonDocument.Parse(json);
            foreach (var t in doc.RootElement.EnumerateArray())
            {
                var type = Str(t, "type") switch { "video" => TrackType.Video, "audio" => TrackType.Audio, "sub" => TrackType.Subtitle, _ => (TrackType?)null };
                if (type == null || !t.TryGetProperty("id", out var idEl)) continue;
                list.Add(new MpvTrack(idEl.GetInt64(), type.Value, Str(t, "title"), Str(t, "lang"), Str(t, "codec"),
                    Bool(t, "external"), Bool(t, "selected")));
            }
        }
        _tracks = list;
        TracksChanged?.Invoke();

        static string Str(JsonElement e, string name) => e.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() : null;
        static bool Bool(JsonElement e, string name) => e.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.True;
    }

    private void ThrowIfDisposed() => ObjectDisposedException.ThrowIf(_disposed, this);

    /// <summary>quit → join the event thread → destroy. Destroying first can hang or crash when a VO or a stream callback is still active.</summary>
    public async ValueTask DisposeAsync()
    {
        if (_disposed) return;
        MpvNative.Command(_ctx, "quit");
        _disposed = true;
        await Task.Run(() =>
        {
            if (!_eventThread.Join(TimeSpan.FromSeconds(5))) MpvNative.mpv_wakeup(_ctx);
            MpvNative.mpv_terminate_destroy(_ctx);
        }).ConfigureAwait(false);
        if (_streamUri != null) MpvStreamSource.Remove(_streamUri);
    }
}
