using System.Diagnostics;
using System.IO.Compression;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.RegularExpressions;
using Microsoft.Win32;

namespace ApexLegendsVoiceSwitcher;

internal sealed record VoiceLanguage(string Name, int DepotId, string LaunchCode)
{
    public override string ToString() => $"{Name}（Depot {DepotId}）";
}

internal sealed class InstallState
{
    public string BuildId { get; set; } = "";
    public string GameDirectory { get; set; } = "";
    public string Language { get; set; } = "";
    public List<string> Files { get; set; } = [];
}

internal static class AppCore
{
    internal const int AppId = 1172470;
    internal static readonly VoiceLanguage[] Languages =
    [
        new("法语", 1172472, "french"),
        new("德语", 1172473, "german"),
        new("意大利语", 1172474, "italian"),
        new("日语", 1172475, "japanese"),
        new("韩语", 1172476, "korean"),
        new("简体中文", 1172477, "schinese"),
        new("波兰语", 1172478, "polish"),
        new("俄语", 1172479, "russian"),
        new("西班牙语", 1172480, "spanish")
    ];

    internal static string DataDirectory => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        nameof(ApexLegendsVoiceSwitcher));
    internal static string StatePath => Path.Combine(DataDirectory, "installed-voice.json");
    internal static string SteamCmdDirectory => Path.Combine(
        Path.GetDirectoryName(Environment.ProcessPath) ?? Environment.CurrentDirectory,
        "steamcmd");

    internal static string? FindSteamDirectory()
    {
        string[] keys =
        [
            @"HKEY_CURRENT_USER\Software\Valve\Steam",
            @"HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Valve\Steam"
        ];
        foreach (string key in keys)
        {
            string? path = Registry.GetValue(key, "SteamPath", null)?.ToString()
                ?? Registry.GetValue(key, "InstallPath", null)?.ToString();
            if (!string.IsNullOrWhiteSpace(path) && Directory.Exists(path)) return Path.GetFullPath(path);
        }
        return null;
    }

    internal static string? FindApexDirectory(string steamDirectory)
    {
        foreach (string library in GetSteamLibraries(steamDirectory))
        {
            string manifest = Path.Combine(library, "steamapps", $"appmanifest_{AppId}.acf");
            if (!File.Exists(manifest)) continue;
            string text = File.ReadAllText(manifest);
            Match match = Regex.Match(text, "\\\"installdir\\\"\\s+\\\"([^\\\"]+)\\\"");
            if (!match.Success) continue;
            string game = Path.Combine(library, "steamapps", "common", match.Groups[1].Value);
            if (Directory.Exists(game)) return game;
        }
        return null;
    }

    internal static string? FindManifest(string gameDirectory)
    {
        DirectoryInfo? common = Directory.GetParent(gameDirectory);
        if (common?.Name.Equals("common", StringComparison.OrdinalIgnoreCase) != true) return null;
        string manifest = Path.Combine(common.Parent!.FullName, $"appmanifest_{AppId}.acf");
        return File.Exists(manifest) ? manifest : null;
    }

    internal static string? ReadBuildId(string gameDirectory)
    {
        string? manifest = FindManifest(gameDirectory);
        if (manifest is null) return null;
        Match match = Regex.Match(File.ReadAllText(manifest), "\\\"buildid\\\"\\s+\\\"(\\d+)\\\"", RegexOptions.IgnoreCase);
        return match.Success ? match.Groups[1].Value : null;
    }

    internal static string AudioShipDirectory(string gameDirectory) =>
        Path.Combine(gameDirectory, "audio", "ship");

    internal static string DepotShipDirectory(string steamCmdDirectory, int depotId) =>
        Path.Combine(steamCmdDirectory, "steamapps", "content", $"app_{AppId}", $"depot_{depotId}", "audio", "ship");

    internal static bool DepotExists(string steamCmdDirectory, int depotId) =>
        Directory.Exists(DepotShipDirectory(steamCmdDirectory, depotId)) &&
        Directory.EnumerateFiles(DepotShipDirectory(steamCmdDirectory, depotId), "*", SearchOption.AllDirectories).Any();

    internal static string DepotDirectory(string steamCmdDirectory, int depotId) =>
        Path.Combine(steamCmdDirectory, "steamapps", "content", $"app_{AppId}", $"depot_{depotId}");

    internal static string? ReadDepotBuildId(string steamCmdDirectory, int depotId)
    {
        string path = Path.Combine(DepotDirectory(steamCmdDirectory, depotId), ".apex-voice-build");
        if (!File.Exists(path)) return null;
        string value = File.ReadAllText(path).Trim();
        return Regex.IsMatch(value, "^\\d+$") ? value : null;
    }

    internal static void WriteDepotBuildId(string steamCmdDirectory, int depotId, string buildId) =>
        File.WriteAllText(Path.Combine(DepotDirectory(steamCmdDirectory, depotId), ".apex-voice-build"), buildId);

    internal static void DeleteDepot(string steamCmdDirectory, int depotId)
    {
        string path = DepotDirectory(steamCmdDirectory, depotId);
        if (Directory.Exists(path)) Directory.Delete(path, true);
    }

    internal static async Task EnsureSteamCmdAsync(string directory, IProgress<string> progress)
    {
        string exe = Path.Combine(directory, "steamcmd.exe");
        if (File.Exists(exe)) return;
        Directory.CreateDirectory(directory);
        string zip = Path.Combine(directory, "steamcmd.zip");
        progress.Report("正在从 Valve 下载 SteamCMD…");
        using HttpClient client = new();
        await using (FileStream output = File.Create(zip))
            await (await client.GetStreamAsync("https://client-update.steamstatic.com/installer/steamcmd.zip")).CopyToAsync(output);
        await Task.Run(() => ZipFile.ExtractToDirectory(zip, directory, true));
        File.Delete(zip);
    }

    internal static async Task<int> DownloadDepotAsync(
        string steamCmdDirectory,
        string username,
        int depotId,
        IProgress<string>? progress = null)
    {
        string script = Path.Combine(steamCmdDirectory, "apex-voice-download.txt");
        await File.WriteAllLinesAsync(script,
        [
            "@ShutdownOnFailedCommand 1",
            "@NoPromptForPassword 0",
            $"login {username}",
            $"download_depot {AppId} {depotId}",
            "quit"
        ]);
        ProcessStartInfo start = new(Path.Combine(steamCmdDirectory, "steamcmd.exe"))
        {
            WorkingDirectory = steamCmdDirectory,
            UseShellExecute = true
        };
        start.ArgumentList.Add("+runscript");
        start.ArgumentList.Add(script);
        using Process process = Process.Start(start) ?? throw new InvalidOperationException("无法启动 SteamCMD。");
        using PeriodicTimer timer = new(TimeSpan.FromSeconds(2));
        Task wait = process.WaitForExitAsync();
        while (!wait.IsCompleted && await timer.WaitForNextTickAsync())
            progress?.Report(DepotExists(steamCmdDirectory, depotId)
                ? "Depot 文件已生成，正在等待 SteamCMD 完成校验并退出…"
                : "SteamCMD 正在登录或下载；请查看 SteamCMD 窗口。 ");
        await wait;
        File.Delete(script);
        return process.ExitCode;
    }

    internal static InstallState InstallVoiceFiles(
        string source,
        string destination,
        string buildId,
        VoiceLanguage language,
        IProgress<(int Done, int Total)>? progress = null)
    {
        Directory.CreateDirectory(destination);
        List<string> installed = [];
        string[] files = Directory.EnumerateFiles(source, "*", SearchOption.AllDirectories).ToArray();
        try
        {
            foreach (string file in files)
            {
                string relative = Path.GetRelativePath(source, file);
                string target = Path.Combine(destination, relative);
                Directory.CreateDirectory(Path.GetDirectoryName(target)!);
                if (File.Exists(target))
                    throw new IOException($"目标文件已存在，未覆盖：{relative}。请先在 Steam 验证游戏文件，或选择其他语音语言。");

                if (!CreateHardLink(target, file, IntPtr.Zero))
                {
                    try { File.CreateSymbolicLink(target, file); }
                    catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or PlatformNotSupportedException)
                    {
                        try { File.Move(file, target); }
                        catch (IOException)
                        {
                            File.Copy(file, target);
                            File.Delete(file);
                        }
                    }
                }
                installed.Add(relative);
                progress?.Report((installed.Count, files.Length));
            }
            if (installed.Count == 0) throw new InvalidOperationException("Depot audio/ship 内没有文件。");
        }
        catch
        {
            foreach (string relative in installed)
                File.Delete(Path.Combine(destination, relative));
            throw;
        }
        InstallState state = new()
        {
            BuildId = buildId,
            GameDirectory = Path.GetFullPath(Path.Combine(destination, "..", "..")),
            Language = language.Name,
            Files = installed
        };
        SaveState(state);
        return state;
    }

    [DllImport("Kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CreateHardLink(string newFileName, string existingFileName, IntPtr securityAttributes);

    internal static InstallState? LoadState()
    {
        if (!File.Exists(StatePath)) return null;
        try { return JsonSerializer.Deserialize<InstallState>(File.ReadAllText(StatePath)); }
        catch (JsonException) { return null; }
    }

    internal static void SaveState(InstallState state)
    {
        Directory.CreateDirectory(DataDirectory);
        File.WriteAllText(StatePath, JsonSerializer.Serialize(state, new JsonSerializerOptions { WriteIndented = true }));
    }

    internal static int RemoveInstalledVoice(InstallState state)
    {
        string ship = AudioShipDirectory(state.GameDirectory);
        string root = Path.GetFullPath(ship) + Path.DirectorySeparatorChar;
        int removed = 0;
        foreach (string relative in state.Files)
        {
            string file = Path.GetFullPath(Path.Combine(ship, relative));
            if (!file.StartsWith(root, StringComparison.OrdinalIgnoreCase)) continue;
            bool existed = File.Exists(file) || IsSymbolicLink(file);
            File.Delete(file);
            if (existed) removed++;
        }
        if (File.Exists(StatePath)) File.Delete(StatePath);
        return removed;
    }

    private static bool IsSymbolicLink(string path)
    {
        try { return new FileInfo(path).LinkTarget is not null; }
        catch (IOException) { return false; }
    }

    internal static IEnumerable<string> GetSteamLibraries(string steamDirectory)
    {
        yield return steamDirectory;
        string file = Path.Combine(steamDirectory, "steamapps", "libraryfolders.vdf");
        if (!File.Exists(file)) yield break;
        foreach (Match match in Regex.Matches(File.ReadAllText(file), "\\\"path\\\"\\s+\\\"([^\\\"]+)\\\""))
        {
            string path = match.Groups[1].Value.Replace("\\\\", "\\");
            if (Directory.Exists(path) && !path.Equals(steamDirectory, StringComparison.OrdinalIgnoreCase)) yield return path;
        }
    }
}
