using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using UFlix.Cross.Player;
using UFlix.Cross.Services;
using UFlix.Engine.Models;

namespace UFlix.Cross;

public partial class App : Application
{
    private MainWindow _window;

    public App()
    {
        this.InitializeComponent();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        _window = new MainWindow();
        NavigationHelper.Initialize(_window, new PlayerController());
        _window.Activate();
        MemoryProbe.Run(_window);
        AppUpdater.Instance.Start(_window.DispatcherQueue);

        // "UFlix.Cross <file>" plays a local file straight away (desktop file associations)
        var file = Environment.GetCommandLineArgs().Skip(1).FirstOrDefault(File.Exists);
        if (file != null)
        {
            _window.DispatcherQueue.TryEnqueue(() => NavigationHelper.OpenPlayer(new MediaItem
            {
                Id = file,
                Title = Path.GetFileName(file),
                Url = file,
                Image = file,
                FolderPath = Path.GetDirectoryName(Path.GetFullPath(file)),
                MediaType = MediaType.Local,
                ContentType = MediaContentType.Movie,
            }));
        }
    }
}

public class PlayerController : IPlayerControl
{
    public UserControl CreatePlayer() => new PlayerPage();

    public void Play(UserControl player, MediaItem file) => (player as PlayerPage)?.Open(file);
}
