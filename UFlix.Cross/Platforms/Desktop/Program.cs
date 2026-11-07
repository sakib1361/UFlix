using System.Runtime.InteropServices;
using Uno.UI.Hosting;
using Velopack;

namespace UFlix.Cross;

public class Program
{
    [DllImport("libc", EntryPoint = "_exit")]
    private static extern void NativeExit(int status);

    [STAThread]
    public static void Main(string[] args)
    {
        // handles the installer's install/update/uninstall hooks (and exits for them); a no-op for a normal start
        VelopackApp.Build().Run();

        if (Environment.GetEnvironmentVariable("UFLIX_SOFTWARE_RENDER") == "1")
            Uno.UI.FeatureConfiguration.Rendering.UseOpenGLOnX11 = false;
        var host = UnoPlatformHostBuilder.Create()
            .App(() => new App())
            .UseX11()
            .UseWin32()
            .Build();

        host.Run();

        // NVIDIA's libGLX exit handler segfaults on the GLX context Uno's X11 host leaves behind. Registered last, so the other
        // ProcessExit handlers (torrent session) still run; then _exit skips the native atexit handlers.
        if (OperatingSystem.IsLinux())
            AppDomain.CurrentDomain.ProcessExit += (_, _) => NativeExit(Environment.ExitCode);
    }
}
