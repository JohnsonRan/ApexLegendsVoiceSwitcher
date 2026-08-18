use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    env, fs, io,
    path::{Component, Path, PathBuf},
    process::Command,
};

const APP_ID: u32 = 1_172_470;
const STEAMCMD_URL: &str = "https://client-update.steamstatic.com/installer/steamcmd.zip";

#[derive(Clone, Copy)]
pub struct VoiceLanguage {
    pub name: &'static str,
    pub depot_id: u32,
    pub launch_code: &'static str,
}

pub const LANGUAGES: [VoiceLanguage; 9] = [
    VoiceLanguage {
        name: "法语",
        depot_id: 1_172_472,
        launch_code: "french",
    },
    VoiceLanguage {
        name: "德语",
        depot_id: 1_172_473,
        launch_code: "german",
    },
    VoiceLanguage {
        name: "意大利语",
        depot_id: 1_172_474,
        launch_code: "italian",
    },
    VoiceLanguage {
        name: "日语",
        depot_id: 1_172_475,
        launch_code: "japanese",
    },
    VoiceLanguage {
        name: "韩语",
        depot_id: 1_172_476,
        launch_code: "korean",
    },
    VoiceLanguage {
        name: "简体中文",
        depot_id: 1_172_477,
        launch_code: "schinese",
    },
    VoiceLanguage {
        name: "波兰语",
        depot_id: 1_172_478,
        launch_code: "polish",
    },
    VoiceLanguage {
        name: "俄语",
        depot_id: 1_172_479,
        launch_code: "russian",
    },
    VoiceLanguage {
        name: "西班牙语",
        depot_id: 1_172_480,
        launch_code: "spanish",
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InstallState {
    pub build_id: String,
    pub game_directory: PathBuf,
    pub language: String,
    #[serde(default)]
    pub installation_method: String,
    pub files: Vec<PathBuf>,
}

fn data_directory() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("ApexLegendsVoiceSwitcher")
}

fn state_path() -> PathBuf {
    data_directory().join("installed-voice.json")
}

pub fn steamcmd_directory() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| env::current_dir().unwrap_or_default())
        .join("steamcmd")
}

pub fn find_steam_directory() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;

    for (key, value) in [
        (r"HKCU\Software\Valve\Steam", "SteamPath"),
        (r"HKLM\SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
    ] {
        let Ok(output) = Command::new("reg")
            .args(["query", key, "/v", value])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        if let Some(path) = text.lines().find_map(|line| {
            line.split_once("REG_SZ")
                .map(|(_, v)| PathBuf::from(v.trim()))
        }) && path.is_dir()
        {
            return Some(path);
        }
    }
    None
}

pub fn find_apex_directory(steam: &Path) -> Option<PathBuf> {
    for library in steam_libraries(steam) {
        let manifest = library
            .join("steamapps")
            .join(format!("appmanifest_{APP_ID}.acf"));
        let Ok(text) = fs::read_to_string(manifest) else {
            continue;
        };
        let Some(install_dir) = vdf_value(&text, "installdir") else {
            continue;
        };
        let game = library.join("steamapps/common").join(install_dir);
        if game.is_dir() {
            return Some(game);
        }
    }
    None
}

pub fn read_build_id(game: &Path) -> Option<String> {
    let common = game.parent()?;
    if !common
        .file_name()?
        .to_string_lossy()
        .eq_ignore_ascii_case("common")
    {
        return None;
    }
    let text =
        fs::read_to_string(common.parent()?.join(format!("appmanifest_{APP_ID}.acf"))).ok()?;
    vdf_value(&text, "buildid")
}

fn vdf_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut values = line.split('"').skip(1).step_by(2);
        match (values.next(), values.next()) {
            (Some(found), Some(value)) if found.eq_ignore_ascii_case(key) => {
                Some(value.replace(r"\\", r"\"))
            }
            _ => None,
        }
    })
}

fn steam_libraries(steam: &Path) -> Vec<PathBuf> {
    let mut result = vec![steam.to_path_buf()];
    if let Ok(text) = fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")) {
        for line in text.lines() {
            if let Some(path) = vdf_value(line, "path").map(PathBuf::from)
                && path.is_dir()
                && !result.iter().any(|p| p == &path)
            {
                result.push(path);
            }
        }
    }
    result
}

pub fn audio_ship_directory(game: &Path) -> PathBuf {
    game.join("audio/ship")
}
fn depot_directory(steamcmd: &Path, depot_id: u32) -> PathBuf {
    steamcmd.join(format!("steamapps/content/app_{APP_ID}/depot_{depot_id}"))
}
pub fn depot_ship_directory(steamcmd: &Path, depot_id: u32) -> PathBuf {
    depot_directory(steamcmd, depot_id).join("audio/ship")
}

pub fn depot_exists(steamcmd: &Path, depot_id: u32) -> bool {
    contains_file(&depot_ship_directory(steamcmd, depot_id)).unwrap_or(false)
}

fn contains_file(root: &Path) -> Result<bool> {
    if !root.is_dir() {
        return Ok(false);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if contains_file(&entry.path())? {
                return Ok(true);
            }
        } else {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn read_depot_build_id(steamcmd: &Path, depot_id: u32) -> Option<String> {
    let value =
        fs::read_to_string(depot_directory(steamcmd, depot_id).join(".apex-voice-build")).ok()?;
    let value = value.trim();
    value
        .chars()
        .all(|c| c.is_ascii_digit())
        .then(|| value.to_owned())
}

pub fn write_depot_build_id(steamcmd: &Path, depot_id: u32, build_id: &str) -> Result<()> {
    fs::write(
        depot_directory(steamcmd, depot_id).join(".apex-voice-build"),
        build_id,
    )?;
    Ok(())
}

pub fn delete_depot(steamcmd: &Path, depot_id: u32) -> Result<()> {
    let path = depot_directory(steamcmd, depot_id);
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

pub fn ensure_steamcmd(directory: &Path) -> Result<()> {
    if directory.join("steamcmd.exe").is_file() {
        return Ok(());
    }
    fs::create_dir_all(directory)?;
    let zip_path = directory.join("steamcmd.zip");
    let mut response = ureq::get(STEAMCMD_URL)
        .call()
        .context("下载 SteamCMD 失败")?
        .into_body()
        .into_reader();
    let mut output = fs::File::create(&zip_path)?;
    io::copy(&mut response, &mut output)?;
    let mut archive = zip::ZipArchive::new(fs::File::open(&zip_path)?)?;
    archive.extract(directory)?;
    fs::remove_file(zip_path)?;
    Ok(())
}

pub fn download_depot(steamcmd: &Path, username: &str, depot_id: u32) -> Result<i32> {
    use std::os::windows::process::CommandExt;
    if username.is_empty()
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        bail!("Steam 用户名只能包含英文字母、数字和下划线。");
    }
    let script = steamcmd.join("apex-voice-download.txt");
    fs::write(
        &script,
        format!(
            "@ShutdownOnFailedCommand 1\n@NoPromptForPassword 0\nlogin {username}\ndownload_depot {APP_ID} {depot_id}\nquit\n"
        ),
    )?;
    let status = Command::new(steamcmd.join("steamcmd.exe"))
        .current_dir(steamcmd)
        .args(["+runscript", script.to_string_lossy().as_ref()])
        .creation_flags(0x10)
        .status()
        .context("无法启动 SteamCMD")?;
    let _ = fs::remove_file(script);
    Ok(status.code().unwrap_or(-1))
}

pub fn install_voice_files(
    source: &Path,
    destination: &Path,
    build_id: &str,
    language: VoiceLanguage,
) -> Result<InstallState> {
    fs::create_dir_all(destination)?;
    let files = walk_files(source)?;
    if files.is_empty() {
        bail!("Depot audio/ship 内没有文件。")
    }
    let mut installed = Vec::new();
    let mut methods = BTreeSet::new();
    let result = (|| -> Result<()> {
        for file in files {
            let relative = file.strip_prefix(source)?.to_path_buf();
            let target = destination.join(&relative);
            if fs::symlink_metadata(&target).is_ok() {
                bail!(
                    "目标文件已存在，未覆盖：{}。请先在 Steam 验证游戏文件，或选择其他语音语言。",
                    relative.display()
                )
            }
            fs::create_dir_all(target.parent().unwrap())?;
            if fs::hard_link(&file, &target).is_ok() {
                methods.insert("硬链接");
            } else if std::os::windows::fs::symlink_file(&file, &target).is_ok() {
                methods.insert("符号链接");
            } else {
                if fs::rename(&file, &target).is_err() {
                    fs::copy(&file, &target)?;
                    fs::remove_file(&file)?;
                }
                methods.insert("移动文件");
            }
            installed.push(relative);
        }
        Ok(())
    })();
    if let Err(error) = result {
        for relative in &installed {
            let _ = fs::remove_file(destination.join(relative));
        }
        return Err(error);
    }
    let method = if methods.len() == 1 {
        methods.into_iter().next().unwrap().to_owned()
    } else {
        format!(
            "混合方式（{}）",
            methods.into_iter().collect::<Vec<_>>().join("、")
        )
    };
    let state = InstallState {
        build_id: build_id.to_owned(),
        game_directory: destination
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| anyhow!("游戏路径无效"))?
            .to_path_buf(),
        language: language.name.to_owned(),
        installation_method: method,
        files: installed,
    };
    save_state(&state)?;
    Ok(state)
}

pub fn load_state() -> Option<InstallState> {
    serde_json::from_slice(&fs::read(state_path()).ok()?).ok()
}

fn save_state(state: &InstallState) -> Result<()> {
    fs::create_dir_all(data_directory())?;
    fs::write(state_path(), serde_json::to_vec_pretty(state)?)?;
    Ok(())
}

pub fn remove_installed_voice(state: &InstallState) -> Result<usize> {
    let ship = audio_ship_directory(&state.game_directory);
    let mut removed = 0;
    for relative in &state.files {
        if relative.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            continue;
        }
        let file = ship.join(relative);
        if fs::symlink_metadata(&file).is_ok() {
            fs::remove_file(file)?;
            removed += 1;
        }
    }
    if state_path().exists() {
        fs::remove_file(state_path())?;
    }
    Ok(removed)
}

fn walk_files(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                dirs.push(entry.path());
            } else {
                files.push(entry.path());
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::{Result, contains_file, vdf_value};
    use std::{env, fs};

    #[test]
    fn parses_vdf_values() {
        assert_eq!(
            vdf_value(r#""buildid" "12345""#, "buildid").as_deref(),
            Some("12345")
        );
        assert_eq!(
            vdf_value(r#""path" "D:\\SteamLibrary""#, "path").as_deref(),
            Some(r"D:\SteamLibrary")
        );
    }

    #[test]
    fn detects_nested_file() -> Result<()> {
        let root = env::temp_dir().join(format!("apex-voice-test-{}", std::process::id()));
        let nested = root.join("nested");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&nested)?;
        assert!(!contains_file(&root)?);
        fs::write(nested.join("voice.mstr"), [])?;
        assert!(contains_file(&root)?);
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
