using Microsoft.UI.Dispatching;
using Velopack;

namespace UFlix.Cross;

public static class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        // handles the installer's install/update/uninstall hooks (and exits for them); a no-op for a normal start
        VelopackApp.Build().Run();

        WinRT.ComWrappersSupport.InitializeComWrappers();
        Microsoft.UI.Xaml.Application.Start(_ =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread()));
            new App();
        });
    }
}
