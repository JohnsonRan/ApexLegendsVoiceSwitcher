using Microsoft.UI.Xaml;

namespace ApexLegendsVoiceSwitcher;

internal static class Program
{
    [STAThread]
    private static void Main()
    {
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(_ => { new App(); });
    }
}
