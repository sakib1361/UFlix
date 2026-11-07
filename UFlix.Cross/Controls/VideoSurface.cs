using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using MpvSharp;

namespace UFlix.Cross.Controls;

/// <summary>The only head-specific part of the player: which video-output options mpv gets, and how its output reaches XAML.</summary>
public interface IVideoSurface
{
    string Kind { get; }
    /// <summary>Video-output options, applied before mpv_initialize.</summary>
    IEnumerable<KeyValuePair<string, string>> MpvOptions();
    void Attach(MpvPlayer player);
    /// <summary>Called after the player is disposed: release native resources.</summary>
    void Release();
    /// <summary>Hides or shows the mouse cursor over the video only.</summary>
    void SetCursorVisible(bool visible);
}

#if WINDOWS10_0_17763_0_OR_GREATER
/// <summary>WinAppSDK head: mpv renders into a composition swapchain that this SwapChainPanel presents. No HWND, XAML owns input.</summary>
public sealed unsafe partial class VideoSurface : SwapChainPanel, IVideoSurface
{
    private static readonly Guid IID_ISwapChainPanelNative = new("63aad0b8-7c24-40ff-85a8-640d944cc325");
    private static readonly Guid IID_IDXGISwapChain2 = new("a8be2ac4-199f-4946-b331-79599fb98de7");
    private MpvPlayer _player;
    private nint _swapChain;

    public VideoSurface()
    {
        SizeChanged += (_, _) => OnResized();
    }

    public string Kind => "SwapChainPanel + mpv composition swapchain";

    public void SetCursorVisible(bool visible)
    {
        if (visible)
        {
            ProtectedCursor = null;
            return;
        }
        // a disposed system cursor renders as no cursor
        ProtectedCursor = Microsoft.UI.Input.InputSystemCursor.Create(Microsoft.UI.Input.InputSystemCursorShape.Arrow);
        ProtectedCursor.Dispose();
    }

    private (int W, int H, double Scale) Pixels()
    {
        double scale = XamlRoot?.RasterizationScale ?? 1.0;
        return (Math.Max(1, (int)Math.Round(ActualWidth * scale)), Math.Max(1, (int)Math.Round(ActualHeight * scale)), scale);
    }

    public IEnumerable<KeyValuePair<string, string>> MpvOptions()
    {
        var (w, h, _) = Pixels();
        yield return new("gpu-context", "d3d11");
        yield return new("d3d11-output-mode", "composition");
        yield return new("d3d11-composition-size", $"{w}x{h}");
        yield return new("force-window", "immediate");
    }

    public void Attach(MpvPlayer player)
    {
        _player = player;
        player.SwapChainReady += swapChain => DispatcherQueue.TryEnqueue(() => Bind(swapChain));
        player.WatchSwapChain();
    }

    public void Release()
    {
        _player = null;
        _swapChain = 0;
    }

    private void Bind(nint swapChain)
    {
        if (_player == null) return;
        _swapChain = swapChain;
        SetPanelSwapChain(swapChain);
        ApplyScale();
    }

    private void OnResized()
    {
        if (_player == null) return;
        var (w, h, _) = Pixels();
        _player.SetProperty("d3d11-composition-size", $"{w}x{h}");
        ApplyScale();
    }

    // mpv sizes the swapchain in physical pixels; the panel is in DIPs, so scale it back by 1/RasterizationScale
    private void ApplyScale()
    {
        if (_swapChain == 0) return;
        var (_, _, scale) = Pixels();
        SetSwapChainTransform(_swapChain, (float)(1.0 / scale));
    }

    private int SetPanelSwapChain(nint swapChain)
    {
        nint unknown = ((WinRT.IWinRTObject)this).NativeObject.ThisPtr;
        Guid iid = IID_ISwapChainPanelNative;
        int hr = Marshal.QueryInterface(unknown, in iid, out nint native);
        if (hr < 0) return hr;
        try { return ((delegate* unmanaged[MemberFunction]<nint, nint, int>)(*(void***)native)[3])(native, swapChain); }
        finally { Marshal.Release(native); }
    }

    private static int SetSwapChainTransform(nint swapChain, float s)
    {
        Guid iid = IID_IDXGISwapChain2;
        int hr = Marshal.QueryInterface(swapChain, in iid, out nint p);
        if (hr < 0) return hr;
        try
        {
            float* m = stackalloc float[6] { s, 0, 0, s, 0, 0 };
            return ((delegate* unmanaged[MemberFunction]<nint, float*, int>)(*(void***)p)[34])(p, m);
        }
        finally { Marshal.Release(p); }
    }
}
#else
/// <summary>
/// Skia head: mpv draws into a native child window (--wid) hosted by Uno. On Linux that is an X11 window with gpu-context=x11vk
/// (x11egl fails next to Uno's GLX renderer on NVIDIA). XAML drawn over it is opaque with square corners.
/// </summary>
public sealed partial class VideoSurface : ContentControl, IVideoSurface
{
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern nint CreateWindowExW(int exStyle, string cls, string name, uint style, int x, int y, int w, int h, nint parent, nint menu, nint inst, nint param);
    [DllImport("user32.dll")] private static extern bool DestroyWindow(nint hwnd);
    [DllImport("libX11.so.6")] private static extern nint XOpenDisplay(nint name);
    [DllImport("libX11.so.6")] private static extern nint XDefaultRootWindow(nint display);
    [DllImport("libX11.so.6")] private static extern nint XCreateSimpleWindow(nint display, nint parent, int x, int y, uint w, uint h, uint border, nint borderColor, nint background);
    [DllImport("libX11.so.6")] private static extern int XDestroyWindow(nint display, nint window);
    [DllImport("libX11.so.6")] private static extern int XFlush(nint display);
    [DllImport("libX11.so.6")] private static extern int XCloseDisplay(nint display);
    [DllImport("libX11.so.6")] private static extern int XMapWindow(nint display, nint window);
    [DllImport("libX11.so.6")] private static extern int XQueryTree(nint display, nint window, out nint root, out nint parent, out nint children, out uint count);
    [DllImport("libX11.so.6")] private static extern int XFree(nint data);
    [DllImport("libX11.so.6")] private static extern int XSelectInput(nint display, nint window, long mask);
    [DllImport("libX11.so.6")] private static extern int XNextEvent(nint display, long[] ev);
    [DllImport("libX11.so.6")] private static extern nint XCreateBitmapFromData(nint display, nint drawable, byte[] data, uint width, uint height);
    [DllImport("libX11.so.6")] private static extern nint XCreatePixmapCursor(nint display, nint source, nint mask, ref XColor foreground, ref XColor background, uint x, uint y);
    [DllImport("libX11.so.6")] private static extern int XDefineCursor(nint display, nint window, nint cursor);
    [DllImport("libX11.so.6")] private static extern int XUndefineCursor(nint display, nint window);
    [DllImport("libX11.so.6")] private static extern int XFreeCursor(nint display, nint cursor);
    [DllImport("libX11.so.6")] private static extern int XFreePixmap(nint display, nint pixmap);

    [StructLayout(LayoutKind.Sequential)]
    private struct XColor
    {
        public nuint Pixel;
        public ushort Red, Green, Blue;
        public byte Flags, Pad;
    }

    private const long StructureNotifyMask = 1L << 17, SubstructureNotifyMask = 1L << 19;
    private const int CreateNotify = 16, DestroyNotify = 17;

    private nint _display;
    private nint _wid;
    private nint _blankCursor, _blankBits;

    public VideoSurface()
    {
        HorizontalContentAlignment = HorizontalAlignment.Stretch;
        VerticalContentAlignment = VerticalAlignment.Stretch;
        if (OperatingSystem.IsWindows())
        {
            const uint WS_POPUP = 0x80000000, WS_CLIPCHILDREN = 0x02000000, WS_CLIPSIBLINGS = 0x04000000;
            _wid = CreateWindowExW(0, "Static", "mpv-host", WS_POPUP | WS_CLIPCHILDREN | WS_CLIPSIBLINGS, 0, 0, 640, 360, 0, 0, 0, 0);
            Content = new Uno.UI.NativeElementHosting.Win32NativeWindow(_wid);
        }
        else
        {
            _display = XOpenDisplay(0);
            _wid = XCreateSimpleWindow(_display, XDefaultRootWindow(_display), 0, 0, 640, 360, 0, 0, 0);
            XFlush(_display);
            Content = new Uno.UI.NativeElementHosting.X11NativeWindow(_wid);
        }
    }

    public string Kind => OperatingSystem.IsWindows() ? "native HWND (Skia head)" : "native X11 window (Skia head)";

    public IEnumerable<KeyValuePair<string, string>> MpvOptions()
    {
        yield return new("wid", _wid.ToString());
        if (OperatingSystem.IsLinux()) yield return new("gpu-context", "x11vk");
    }

    public void Attach(MpvPlayer player)
    {
        if (OperatingSystem.IsLinux()) new Thread(MapMpvWindows) { IsBackground = true, Name = "mpv-x11-map" }.Start();
    }

    /// <summary>
    /// mpv (0.41) creates its video window as a child of --wid but leaves it unmapped inside Uno's X11 host, so nothing shows.
    /// Blocks on X events (no polling) and maps each child mpv creates; ends when the host window is destroyed in <see cref="Release"/>.
    /// Uses its own display connection: Xlib connections are not shared across threads.
    /// </summary>
    private void MapMpvWindows()
    {
        var display = XOpenDisplay(0);
        if (display == 0) return;
        var wid = _wid;
        var ev = new long[24];
        try
        {
            XSelectInput(display, wid, StructureNotifyMask | SubstructureNotifyMask);
            // a child created before the selection took effect
            if (XQueryTree(display, wid, out _, out _, out var children, out var count) != 0)
            {
                for (int i = 0; i < count; i++) XMapWindow(display, Marshal.ReadIntPtr(children, i * IntPtr.Size));
                if (children != 0) XFree(children);
            }
            XFlush(display);
            while (true)
            {
                XNextEvent(display, ev);
                var type = (int)ev[0];
                // XCreateWindowEvent / XDestroyWindowEvent: type, serial, send_event, display, parent|event, window
                if (type == CreateNotify) XMapWindow(display, (nint)ev[5]);
                else if (type == DestroyNotify && (nint)ev[5] == wid) break;
                XFlush(display);
            }
        }
        finally
        {
            XCloseDisplay(display);
        }
    }

    /// <summary>Sets a blank cursor on the host window; mpv's window inherits it (mpv leaves its own cursor as None).</summary>
    public void SetCursorVisible(bool visible)
    {
        if (!OperatingSystem.IsLinux() || _wid == 0) return;
        if (visible) XUndefineCursor(_display, _wid);
        else
        {
            if (_blankCursor == 0)
            {
                var black = new XColor();
                _blankBits = XCreateBitmapFromData(_display, _wid, [0], 1, 1);
                _blankCursor = XCreatePixmapCursor(_display, _blankBits, _blankBits, ref black, ref black, 0, 0);
            }
            XDefineCursor(_display, _wid, _blankCursor);
        }
        XFlush(_display);
    }

    public void Release()
    {
        Content = null;
        if (_wid == 0) return;
        if (OperatingSystem.IsWindows()) DestroyWindow(_wid);
        else
        {
            if (_blankCursor != 0)
            {
                XFreeCursor(_display, _blankCursor);
                XFreePixmap(_display, _blankBits);
            }
            XDestroyWindow(_display, _wid);
            XCloseDisplay(_display);
        }
        _wid = 0;
    }
}
#endif
