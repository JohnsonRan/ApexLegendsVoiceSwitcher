using Microsoft.UI.Xaml;

namespace ApexLegendsVoiceSwitcher;

public partial class App : Application
{
    private Window? window;

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, args) =>
        {
            Directory.CreateDirectory(AppCore.DataDirectory);
            File.AppendAllText(Path.Combine(AppCore.DataDirectory, "crash.log"),
                $"[{DateTimeOffset.Now:O}] {args.Exception}\n\n");
            args.Handled = true;
        };
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        window = new MainWindow();
        window.Activate();
    }
}
