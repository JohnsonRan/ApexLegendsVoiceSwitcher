# Apex Legends Voice Switcher

一个基于 WinUI 3 的 Apex Legends 游戏语音切换工具。它保留 Steam 中设置的界面/文本语言，只安装另一种官方语音，并生成对应的 Steam 启动项。

## 工作方式

1. 自动检测 Steam 与 Apex Legends，也可以手动选择路径。
2. 如果运行目录下没有 `steamcmd`，自动从 Valve 官方地址下载并解压，再下载所选语言 Depot。
3. 优先在游戏 `audio\ship` 中创建硬链接；跨磁盘时尝试符号链接；无法创建链接时才移动文件。
4. 任意时候都可以点击“删除当前语音”，移除本工具记录的语音文件/链接。
5. 生成 `+miles_language <language>` 启动项。

不创建 `apex_voice_backups`：Steam 当前文本语言的文件保持不变，因此不需要备份。

## 为什么必须登录 Steam？

SteamCMD 可以只下载语言 Depot，避免切换 Steam 游戏语言后下载整套内容。Apex Legends 的 Depot 不允许匿名下载，所以需要登录一个已领取 Apex Legends 免费许可的 Steam 账号。密码和 Steam Guard 验证码只在 SteamCMD 窗口中输入，本工具不会读取或保存它们。

## 版本安全

工具读取 Steam 的 `appmanifest_1172470.acf` 中的 `buildid`，并记录每次安装/下载时的 Build ID。游戏 Build 变化后，工具会在启动时强制删除上次安装到游戏目录的语音链接/文件，并要求重新下载。

SteamCMD 的 `steamapps\content` 目录没有提供一个可直接对应 Apex 应用 Build ID 的本地标记，因此本工具会为自己下载或首次确认使用的 Depot 写入 `.apex-voice-build`。对于首次发现的、没有标记的现有 Depot，工具会明确提示版本无法确认，并允许使用或重新下载。

## 支持的语音 Depot

| 语言 | Depot ID | 启动项语言 |
|---|---:|---|
| 法语 | 1172472 | french |
| 德语 | 1172473 | german |
| 意大利语 | 1172474 | italian |
| 日语 | 1172475 | japanese |
| 韩语 | 1172476 | korean |
| 简体中文 | 1172477 | schinese |
| 波兰语 | 1172478 | polish |
| 俄语 | 1172479 | russian |
| 西班牙语 | 1172480 | spanish |

Depot ID 来源于仓库中的 `depots.png`。

## 构建

要求 Windows 10 1809 或更高版本以及 .NET 10 SDK：

```powershell
dotnet build -c Release
dotnet publish -c Release -r win-x64
```

WinUI 3 的原生运行时不能像普通 .NET DLL 那样简单合并；本项目使用官方支持的 unpackaged + self-contained + single-file 配置，把依赖打进一个 EXE。运行时会由 .NET 单文件加载器解压到类似 `%TEMP%\\.net\\ApexLegendsVoiceSwitcher\\<bundle-hash>\\` 的隔离目录（`.net` 是统一根目录，应用名和 Bundle 哈希用于区分程序及版本）。因此 EXE 较大且某个新版本首次启动可能稍慢，但发布目录不再散落 DLL、`i18n` 文件或 `.pri` 文件。

> 必须保持文件名 `ApexLegendsVoiceSwitcher.exe`。当前 WinUI 3 单文件运行时在 EXE 被重命名后可能于 `Microsoft.UI.Xaml.dll` 中崩溃。

发布目录：

```text
bin\Release\net10.0-windows10.0.19041.0\win-x64\publish
```
