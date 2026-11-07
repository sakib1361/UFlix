namespace MpvSharp;

public enum TrackType
{
    Video,
    Audio,
    Subtitle,
}

public enum EndReason
{
    Eof = 0,
    Stop = 2,
    Quit = 3,
    Error = 4,
    Redirect = 5,
}

public sealed record MpvTrack(long Id, TrackType Type, string Title, string Language, string Codec, bool IsExternal, bool IsSelected)
{
    public string Description
    {
        get
        {
            var name = !string.IsNullOrEmpty(Title) ? Title : !string.IsNullOrEmpty(Language) ? Language : $"Track {Id}";
            if (!string.IsNullOrEmpty(Language) && name != Language) name = $"{name} ({Language})";
            return IsExternal ? $"{name} [ext]" : name;
        }
    }
}

/// <summary>Per-file options for <see cref="MpvPlayer.LoadAsync(string, LoadOptions)"/>.</summary>
public sealed class LoadOptions
{
    /// <summary>A separate audio stream (YouTube DASH), added with audio-add once the file is loaded.</summary>
    public string AudioUrl { get; init; }
    public IReadOnlyList<string> Subtitles { get; init; } = [];
    public TimeSpan? Start { get; init; }
    public string Title { get; init; }
}

public sealed class MpvOptions
{
    public string Hwdec { get; init; } = "auto-safe";
    public string Vo { get; init; } = "gpu-next";
    public string Ao { get; init; }
    public double VolumeMax { get; init; } = 200;
    /// <summary>mpv log level forwarded to <see cref="MpvPlayer.Log"/> ("no", "error", "warn", "info", "v").</summary>
    public string LogLevel { get; init; } = "warn";
    /// <summary>Anything else, applied last before mpv_initialize (a head's video-output options go here).</summary>
    public IList<KeyValuePair<string, string>> Extra { get; init; } = [];
}

/// <summary>A stream that can wake a blocked Read and make later reads fail; mpv's stream_cb cancel_fn calls it on stop.</summary>
public interface ICancellableStream
{
    void CancelRead();
}

public sealed class MpvException(string message, int error) : Exception($"{message}: {Native.MpvNative.ErrorString(error)} ({error})")
{
    public int Error { get; } = error;
}
