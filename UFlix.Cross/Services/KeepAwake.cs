using System.Diagnostics;
using System.Runtime.InteropServices;

namespace UFlix.Cross.Services
{
    /// <summary>
    /// Keeps the display on and the machine awake while video plays. mpv's stop-screensaver can't do it here: the WinUI
    /// head has no mpv window, and on Linux the X11 screensaver calls don't reach a Wayland compositor through XWayland.
    /// Windows: SetThreadExecutionState (the caller's thread must stay alive, so call from the UI thread).
    /// Linux: session D-Bus org.freedesktop.ScreenSaver.Inhibit, falling back to the xdg-desktop-portal Inhibit;
    /// the inhibit is tied to our bus connection, so a crash releases it.
    /// </summary>
    public static partial class KeepAwake
    {
        private static volatile bool _active;

        public static void Set(bool active)
        {
            if (_active == active) return;
            _active = active;
            try
            {
                if (OperatingSystem.IsWindows())
                    SetThreadExecutionState(active ? EsContinuous | EsSystemRequired | EsDisplayRequired : EsContinuous);
                else if (OperatingSystem.IsLinux())
                    _ = Task.Run(Linux.Sync);
            }
            catch (Exception ex)
            {
                Debug.WriteLine($"[keepawake] {ex.Message}");
            }
        }

        private const uint EsContinuous = 0x80000000, EsSystemRequired = 0x1, EsDisplayRequired = 0x2;

        [LibraryImport("kernel32.dll")]
        private static partial uint SetThreadExecutionState(uint flags);

        private static unsafe partial class Linux
        {
            private const string Lib = "libdbus-1.so.3";
            private const int TypeString = 's', TypeUint32 = 'u', TypeObjectPath = 'o', TypeArray = 'a';
            // DBusMessageIter / DBusError are opaque stack structs (≤ 80 / 32 bytes on x64); over-allocate
            private const int IterSize = 128, ErrorSize = 64;

            private static readonly Lock _lock = new();
            private static IntPtr _conn;
            private static bool _unavailable;
            private static uint _cookie;
            private static string _portalHandle;

            /// <summary>Applies the latest requested state, so calls that run out of order still converge.</summary>
            internal static void Sync()
            {
                lock (_lock)
                {
                    try
                    {
                        if (_active) Inhibit();
                        else Release();
                    }
                    catch (Exception ex)
                    {
                        _unavailable = ex is DllNotFoundException or EntryPointNotFoundException;
                        Debug.WriteLine($"[keepawake] {ex.Message}");
                    }
                }
            }

            private static void Inhibit()
            {
                if (_cookie != 0 || _portalHandle != null || !Connect()) return;
                _cookie = Call("org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver", "org.freedesktop.ScreenSaver", "Inhibit",
                    iter => { AppendString(iter, "UFlix"); AppendString(iter, "Playing video"); },
                    iter => GetUInt32(iter));
                if (_cookie != 0) return;
                // flags: 4 = suspend, 8 = idle
                _portalHandle = Call("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Inhibit", "Inhibit",
                    iter =>
                    {
                        AppendString(iter, "");
                        AppendUInt32(iter, 4 | 8);
                        var dict = stackalloc byte[IterSize];
                        dbus_message_iter_open_container(iter, TypeArray, "{sv}", (IntPtr)dict);
                        dbus_message_iter_close_container(iter, (IntPtr)dict);
                    },
                    iter => GetString(iter, TypeObjectPath));
            }

            private static void Release()
            {
                if (_conn == IntPtr.Zero) return;
                if (_cookie != 0)
                {
                    var cookie = _cookie;
                    _cookie = 0;
                    Call<object>("org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver", "org.freedesktop.ScreenSaver", "UnInhibit",
                        iter => AppendUInt32(iter, cookie), null);
                }
                if (_portalHandle != null)
                {
                    var handle = _portalHandle;
                    _portalHandle = null;
                    Call<object>("org.freedesktop.portal.Desktop", handle, "org.freedesktop.portal.Request", "Close", null, null);
                }
            }

            private static bool Connect()
            {
                if (_conn != IntPtr.Zero) return true;
                if (_unavailable) return false;
                var error = stackalloc byte[ErrorSize];
                dbus_error_init((IntPtr)error);
                // private connection: nothing else in the process shares it, and it lives until exit
                _conn = dbus_bus_get_private(0 /* DBUS_BUS_SESSION */, (IntPtr)error);
                if (_conn == IntPtr.Zero)
                {
                    _unavailable = true;
                    dbus_error_free((IntPtr)error);
                    return false;
                }
                dbus_connection_set_exit_on_disconnect(_conn, 0);
                return true;
            }

            private static T Call<T>(string destination, string path, string iface, string method, Action<IntPtr> append, Func<IntPtr, T> read)
            {
                var msg = dbus_message_new_method_call(destination, path, iface, method);
                if (msg == IntPtr.Zero) return default;
                var error = stackalloc byte[ErrorSize];
                dbus_error_init((IntPtr)error);
                IntPtr reply = IntPtr.Zero;
                try
                {
                    if (append != null)
                    {
                        var iter = stackalloc byte[IterSize];
                        dbus_message_iter_init_append(msg, (IntPtr)iter);
                        append((IntPtr)iter);
                    }
                    reply = dbus_connection_send_with_reply_and_block(_conn, msg, 3000, (IntPtr)error);
                    if (reply == IntPtr.Zero || read == null) return default;
                    var riter = stackalloc byte[IterSize];
                    return dbus_message_iter_init(reply, (IntPtr)riter) != 0 ? read((IntPtr)riter) : default;
                }
                finally
                {
                    if (reply != IntPtr.Zero) dbus_message_unref(reply);
                    dbus_message_unref(msg);
                    dbus_error_free((IntPtr)error);
                }
            }

            private static void AppendString(IntPtr iter, string value)
            {
                var str = Marshal.StringToCoTaskMemUTF8(value);
                try { dbus_message_iter_append_basic(iter, TypeString, (IntPtr)(&str)); }
                finally { Marshal.FreeCoTaskMem(str); }
            }

            private static void AppendUInt32(IntPtr iter, uint value) => dbus_message_iter_append_basic(iter, TypeUint32, (IntPtr)(&value));

            private static uint GetUInt32(IntPtr iter)
            {
                if (dbus_message_iter_get_arg_type(iter) != TypeUint32) return 0;
                uint value = 0;
                dbus_message_iter_get_basic(iter, (IntPtr)(&value));
                return value;
            }

            private static string GetString(IntPtr iter, int type)
            {
                if (dbus_message_iter_get_arg_type(iter) != type) return null;
                IntPtr value = IntPtr.Zero;
                dbus_message_iter_get_basic(iter, (IntPtr)(&value));
                return Marshal.PtrToStringUTF8(value);
            }

            [LibraryImport(Lib)] private static partial void dbus_error_init(IntPtr error);
            [LibraryImport(Lib)] private static partial void dbus_error_free(IntPtr error);
            [LibraryImport(Lib)] private static partial IntPtr dbus_bus_get_private(int type, IntPtr error);
            [LibraryImport(Lib)] private static partial void dbus_connection_set_exit_on_disconnect(IntPtr conn, int exit);
            [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
            private static partial IntPtr dbus_message_new_method_call(string destination, string path, string iface, string method);
            [LibraryImport(Lib)] private static partial void dbus_message_unref(IntPtr msg);
            [LibraryImport(Lib)] private static partial IntPtr dbus_connection_send_with_reply_and_block(IntPtr conn, IntPtr msg, int timeoutMs, IntPtr error);
            [LibraryImport(Lib)] private static partial void dbus_message_iter_init_append(IntPtr msg, IntPtr iter);
            [LibraryImport(Lib)] private static partial int dbus_message_iter_append_basic(IntPtr iter, int type, IntPtr value);
            [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
            private static partial int dbus_message_iter_open_container(IntPtr iter, int type, string signature, IntPtr sub);
            [LibraryImport(Lib)] private static partial int dbus_message_iter_close_container(IntPtr iter, IntPtr sub);
            [LibraryImport(Lib)] private static partial int dbus_message_iter_init(IntPtr msg, IntPtr iter);
            [LibraryImport(Lib)] private static partial int dbus_message_iter_get_arg_type(IntPtr iter);
            [LibraryImport(Lib)] private static partial void dbus_message_iter_get_basic(IntPtr iter, IntPtr value);
        }
    }
}
