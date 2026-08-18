using System.Runtime.InteropServices;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace ApexLegendsVoiceSwitcher;

public sealed partial class MainWindow : Window
{
    private bool busy;

    public MainWindow()
    {
        InitializeComponent();
        SystemBackdrop = new MicaBackdrop();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
        AppWindow.TitleBar.PreferredHeightOption = TitleBarHeightOption.Standard;
        ResizeAndCenter(1120, 1080);
        LanguageBox.ItemsSource = AppCore.Languages;
        LanguageBox.SelectedIndex = 3;

        string? steam = AppCore.FindSteamDirectory();
        string? game = steam is null ? null : AppCore.FindApexDirectory(steam);
        GamePathBox.Text = game ?? "";
        SteamCmdPathBox.Text = AppCore.SteamCmdDirectory;
        Activated += MainWindow_Activated;
        RefreshDepotStatus();
    }

    private async void MainWindow_Activated(object sender, WindowActivatedEventArgs args)
    {
        Activated -= MainWindow_Activated;
        ResizeAndCenter(1120, 1080);
        await EnforceBuildCompatibilityAsync();
    }

    private void ResizeAndCenter(int widthDip, int heightDip)
    {
        nint handle = WindowNative.GetWindowHandle(this);
        double scale = GetDpiForWindow(handle) / 96d;
        DisplayArea area = DisplayArea.GetFromWindowId(AppWindow.Id, DisplayAreaFallback.Primary);
        Windows.Graphics.RectInt32 work = area.WorkArea;
        int width = Math.Min((int)Math.Round(widthDip * scale), (int)(work.Width * 0.96));
        int height = Math.Min((int)Math.Round(heightDip * scale), (int)(work.Height * 0.96));
        AppWindow.MoveAndResize(new Windows.Graphics.RectInt32(
            work.X + (work.Width - width) / 2,
            work.Y + (work.Height - height) / 2,
            width,
            height));
    }

    [DllImport("user32.dll")]
    private static extern uint GetDpiForWindow(nint window);

    private async Task EnforceBuildCompatibilityAsync()
    {
        InstallState? state = AppCore.LoadState();
        if (state is null) return;
        string? build = AppCore.ReadBuildId(state.GameDirectory);
        if (build is not null && build == state.BuildId) return;

        ContentDialog dialog = NewDialog(
            "检测到 Apex Legends 已更新",
            $"已安装的 {state.Language} 语音属于游戏 Build {state.BuildId}，当前 Build 为 {build ?? "未知"}。旧语音可能不兼容，必须删除并重新下载。",
            "删除旧语音");
        await dialog.ShowAsync();
        try
        {
            int count = AppCore.RemoveInstalledVoice(state);
            SetStatus(InfoBarSeverity.Warning, "旧语音已删除", $"已删除 {count} 个文件。请选择语言重新下载。");
        }
        catch (Exception ex)
        {
            SetStatus(InfoBarSeverity.Error, "无法删除旧语音", ex.Message);
            RunOnUi(() => InstallButton.IsEnabled = false);
        }
    }

    private async void BrowseGame_Click(object sender, RoutedEventArgs e)
    {
        string? folder = await PickFolderAsync();
        if (folder is not null) RunOnUi(() => GamePathBox.Text = folder);
    }

    private void Language_Changed(object sender, SelectionChangedEventArgs e)
    {
        if (LanguageBox.SelectedItem is not VoiceLanguage language) return;
        LaunchOptionBox.Text = $"+miles_language {language.LaunchCode}";
        RefreshDepotStatus();
    }

    private void RefreshDepotStatus()
    {
        if (!DispatcherQueue.HasThreadAccess)
        {
            RunOnUi(RefreshDepotStatus);
            return;
        }
        if (LanguageBox?.SelectedItem is not VoiceLanguage language || SteamCmdPathBox is null) return;
        if (AppCore.DepotExists(SteamCmdPathBox.Text.Trim(), language.DepotId))
        {
            string? cachedBuild = AppCore.ReadDepotBuildId(SteamCmdPathBox.Text.Trim(), language.DepotId);
            string? gameBuild = string.IsNullOrWhiteSpace(GamePathBox?.Text) ? null : AppCore.ReadBuildId(GamePathBox.Text.Trim());
            bool stale = cachedBuild is not null && gameBuild is not null && cachedBuild != gameBuild;
            SetStatus(stale ? InfoBarSeverity.Warning : InfoBarSeverity.Success,
                stale ? "已有 Depot 与当前游戏版本不同" : "已发现下载好的语言 Depot",
                stale
                    ? $"缓存属于 Build {cachedBuild}，当前为 {gameBuild}；安装时将强制删除并重新下载。"
                    : $"{language.Name} Depot {language.DepotId} 已存在，不会重新下载。点击按钮将直接安装。 ");
            if (InstallButton is not null) InstallButton.Content = stale ? "更新并安装语音" : "安装已有语音";
        }
        else
        {
            SetStatus(InfoBarSeverity.Informational, "尚未下载",
                $"将通过 SteamCMD 下载 {language.Name} Depot {language.DepotId}。");
            if (InstallButton is not null) InstallButton.Content = "下载并安装语音";
        }
    }

    private async void Remove_Click(object sender, RoutedEventArgs e)
    {
        if (busy) return;
        string game = GamePathBox.Text.Trim();
        InstallState? state = AppCore.LoadState();
        if (state is null || !Directory.Exists(AppCore.AudioShipDirectory(game)))
        {
            SetStatus(InfoBarSeverity.Informational, "没有可删除的语音", "当前没有由本工具记录的已安装语音文件。");
            return;
        }
        ContentDialogResult result = await ShowDialogAsync(
            "删除当前语音？",
            $"将删除 {state.Language} 语音对应的 {state.Files.Count} 个文件/链接。\n安装方式：{(string.IsNullOrWhiteSpace(state.InstallationMethod) ? "旧版本未记录" : state.InstallationMethod)}\n\nSteam 文本语言文件不会受到影响。",
            "删除语音", "取消");
        if (result != ContentDialogResult.Primary) return;
        try
        {
            int count = await Task.Run(() => AppCore.RemoveInstalledVoice(state));
            SetStatus(InfoBarSeverity.Success, "语音已删除", $"已删除 {count} 个文件/链接。 ");
        }
        catch (Exception ex)
        {
            SetStatus(InfoBarSeverity.Error, "删除失败", ex.Message);
        }
    }

    private async void Install_Click(object sender, RoutedEventArgs e)
    {
        if (busy || LanguageBox.SelectedItem is not VoiceLanguage language) return;
        string game = GamePathBox.Text.Trim();
        string steamCmd = SteamCmdPathBox.Text.Trim();
        string username = UsernameBox.Text.Trim();
        string? build = AppCore.ReadBuildId(game);
        if (!Directory.Exists(AppCore.AudioShipDirectory(game)) || build is null)
        {
            SetStatus(InfoBarSeverity.Error, "Apex Legends 目录无效", "请选择包含 audio\\ship 且由 Steam 管理的 Apex Legends 目录。");
            return;
        }

        busy = true;
        bool installed = false;
        InstallButton.IsEnabled = false;
        Progress.Visibility = Visibility.Visible;
        try
        {
            string source = AppCore.DepotShipDirectory(steamCmd, language.DepotId);
            if (AppCore.DepotExists(steamCmd, language.DepotId))
            {
                string? cachedBuild = AppCore.ReadDepotBuildId(steamCmd, language.DepotId);
                if (cachedBuild is not null && cachedBuild != build)
                {
                    await ShowDialogAsync("缓存版本已过期",
                        $"缓存 Depot 属于 Build {cachedBuild}，当前游戏为 Build {build}。为避免语音不兼容，必须删除并重新下载。",
                        "删除并重新下载");
                    AppCore.DeleteDepot(steamCmd, language.DepotId);
                }
                else if (cachedBuild is null)
                {
                    ContentDialogResult use = await ShowDialogAsync(
                        "发现未标记版本的 Depot",
                        $"已找到 {source}\n\nSteamCMD 的 content 目录本身不保存可直接对应 Apex Build ID 的元数据，因此无法确认它是否为当前版本。",
                        "使用已有文件", "重新下载");
                    if (use == ContentDialogResult.Secondary) AppCore.DeleteDepot(steamCmd, language.DepotId);
                }
            }

            if (!AppCore.DepotExists(steamCmd, language.DepotId))
            {
                if (string.IsNullOrWhiteSpace(username))
                {
                    SetStatus(InfoBarSeverity.Warning, "需要 Steam 用户名", "密码及 Steam Guard 将在 SteamCMD 窗口输入，本工具不会读取或保存。 ");
                    return;
                }
                SetStatus(InfoBarSeverity.Informational, "准备 SteamCMD", "首次运行会自动更新 SteamCMD。 ");
                await AppCore.EnsureSteamCmdAsync(steamCmd, new Progress<string>(text => SetStatus(InfoBarSeverity.Informational, "SteamCMD", text)));
                SetStatus(InfoBarSeverity.Informational, "请在 SteamCMD 登录", "按提示输入密码及 Steam Guard；下载约数 GB，请等待窗口自动关闭。 ");
                Progress<string> steamProgress = new(message =>
                    SetStatus(InfoBarSeverity.Informational, "SteamCMD 运行中", message));
                int exitCode = await AppCore.DownloadDepotAsync(
                    steamCmd, username, language.DepotId, steamProgress);
                if (exitCode != 0 || !AppCore.DepotExists(steamCmd, language.DepotId))
                    throw new InvalidOperationException($"SteamCMD 未完成 Depot 下载（退出码 {exitCode}）。检查账号是否领取 Apex Legends 免费许可及 SteamCMD 窗口错误。 ");
                AppCore.WriteDepotBuildId(steamCmd, language.DepotId, build);
            }

            SetStatus(InfoBarSeverity.Informational, "正在安装语音",
                "正在后台创建链接或移动文件；跨磁盘时可能需要几分钟，窗口仍可正常操作。 ");
            InstallState? previous = AppCore.LoadState();
            if (previous is not null) await Task.Run(() => AppCore.RemoveInstalledVoice(previous));
            if (AppCore.ReadDepotBuildId(steamCmd, language.DepotId) is null)
                AppCore.WriteDepotBuildId(steamCmd, language.DepotId, build);
            Progress<(int Done, int Total)> fileProgress = new(value =>
                RunOnUi(() => StatusBar.Message = $"已处理 {value.Done}/{value.Total} 个文件。跨磁盘复制大文件时，单个文件可能耗时较久。 "));
            InstallState state = await Task.Run(() => AppCore.InstallVoiceFiles(
                source, AppCore.AudioShipDirectory(game), build, language, fileProgress));
            SetStatus(InfoBarSeverity.Success, $"{language.Name}语音安装完成",
                $"已通过“{state.InstallationMethod}”安装 {state.Files.Count} 个语音文件。复制下方启动项后启动游戏。 ");
            installed = true;
        }
        catch (Exception ex)
        {
            SetStatus(InfoBarSeverity.Error, "操作失败", ex.Message);
        }
        finally
        {
            busy = false;
            RunOnUi(() =>
            {
                InstallButton.IsEnabled = true;
                Progress.Visibility = Visibility.Collapsed;
                if (!installed) RefreshDepotStatus();
            });
        }
    }

    private void CopyLaunchOption_Click(object sender, RoutedEventArgs e)
    {
        DataPackage data = new();
        data.SetText(LaunchOptionBox.Text);
        Clipboard.SetContent(data);
        SetStatus(InfoBarSeverity.Success, "启动项已复制", LaunchOptionBox.Text);
    }

    private async Task<string?> PickFolderAsync()
    {
        FolderPicker picker = new();
        picker.FileTypeFilter.Add("*");
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        return (await picker.PickSingleFolderAsync())?.Path;
    }

    private async Task<ContentDialogResult> ShowDialogAsync(
        string title, string content, string primary, string? secondary = null)
    {
        TaskCompletionSource<ContentDialogResult> completion = new();
        RunOnUi(async () =>
        {
            try { completion.SetResult(await NewDialog(title, content, primary, secondary).ShowAsync()); }
            catch (Exception ex) { completion.SetException(ex); }
        });
        return await completion.Task;
    }

    private ContentDialog NewDialog(string title, string content, string primary, string? secondary = null) => new()
    {
        XamlRoot = Content.XamlRoot,
        Title = title,
        Content = content,
        PrimaryButtonText = primary,
        SecondaryButtonText = secondary ?? "",
        DefaultButton = ContentDialogButton.Primary
    };

    private void SetStatus(InfoBarSeverity severity, string title, string message) => RunOnUi(() =>
    {
        if (StatusBar is null) return;
        StatusBar.Severity = severity;
        StatusBar.Title = title;
        StatusBar.Message = message;
        StatusBar.IsOpen = true;
    });

    private void RunOnUi(Action action)
    {
        if (DispatcherQueue.HasThreadAccess) action();
        else if (!DispatcherQueue.TryEnqueue(() => action()))
            throw new InvalidOperationException("UI 调度器已经关闭。");
    }
}
