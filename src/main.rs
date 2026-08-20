#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod core;

use anyhow::{Result, bail};
use core::LANGUAGES;
use slint::{ModelRc, SharedString, VecModel};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    thread,
};

slint::include_modules!();

enum PendingModalAction {
    RemoveVoice(core::InstallState),
    UnverifiedDepot {
        game: PathBuf,
        steamcmd: PathBuf,
        username: String,
        language: core::VoiceLanguage,
    },
    BuildIncompatible(core::InstallState),
}

fn main() -> Result<()> {
    let ui = MainWindow::new()?;
    let steamcmd = core::steamcmd_directory();
    let game = core::find_steam_directory().and_then(|steam| core::find_apex_directory(&steam));
    ui.set_game_path(
        game.map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
            .into(),
    );
    ui.set_steamcmd_path(steamcmd.to_string_lossy().into_owned().into());
    ui.set_languages(ModelRc::from(Rc::new(VecModel::from(
        LANGUAGES
            .iter()
            .map(|l| SharedString::from(format!("{}（Depot {}）", l.name, l.depot_id)))
            .collect::<Vec<_>>(),
    ))));
    update_language(&ui, 3);
    refresh_status(&ui);

    let pending_action: Rc<RefCell<Option<PendingModalAction>>> = Rc::new(RefCell::new(None));

    ui.on_language_changed({
        let weak = ui.as_weak();
        move |index| {
            if let Some(ui) = weak.upgrade() {
                update_language(&ui, index);
                refresh_status(&ui);
            }
        }
    });

    ui.on_browse({
        let weak = ui.as_weak();
        move || {
            if let Some(folder) = rfd::FileDialog::new().pick_folder()
                && let Some(ui) = weak.upgrade()
            {
                ui.set_game_path(folder.to_string_lossy().into_owned().into());
                refresh_status(&ui);
            }
        }
    });

    ui.on_copy_launch_option({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                let option = ui.get_launch_option().to_string();
                match arboard::Clipboard::new()
                    .and_then(|mut clipboard| clipboard.set_text(&option))
                {
                    Ok(()) => set_status(&ui, "启动项已复制", &option),
                    Err(error) => set_status(&ui, "复制失败", &error.to_string()),
                }
            }
        }
    });

    ui.on_remove({
        let weak = ui.as_weak();
        let pending = Rc::clone(&pending_action);
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let Some(state) = core::load_state() else {
                set_status(
                    &ui,
                    "没有可删除的语音",
                    "当前没有由本工具记录的已安装语音文件。",
                );
                return;
            };
            *pending.borrow_mut() = Some(PendingModalAction::RemoveVoice(state.clone()));
            ui.set_modal_title("确认删除已安装语音".into());
            ui.set_modal_message(
                format!(
                    "将删除 {} 语音对应的 {} 个文件/链接。\n安装方式：{}",
                    state.language,
                    state.files.len(),
                    state.installation_method
                )
                .into(),
            );
            ui.set_modal_is_danger(true);
            ui.set_modal_primary_text("确认删除".into());
            ui.set_modal_secondary_text("".into());
            ui.set_modal_cancel_text("取消".into());
            ui.set_modal_visible(true);
        }
    });

    ui.on_install({
        let weak = ui.as_weak();
        let pending = Rc::clone(&pending_action);
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let index = ui.get_selected_language();
            let Some(&language) = LANGUAGES.get(index as usize) else {
                return;
            };
            let game = PathBuf::from(ui.get_game_path().as_str());
            let steamcmd = PathBuf::from(ui.get_steamcmd_path().as_str());
            let username = ui.get_username().to_string();
            if !core::audio_ship_directory(&game).is_dir() || core::read_build_id(&game).is_none() {
                set_status(
                    &ui,
                    "Apex Legends 目录无效",
                    "请选择包含 audio\\ship 且由 Steam 管理的 Apex Legends 目录。",
                );
                return;
            }
            if core::depot_exists(&steamcmd, language.depot_id)
                && core::read_depot_build_id(&steamcmd, language.depot_id).is_none()
            {
                *pending.borrow_mut() = Some(PendingModalAction::UnverifiedDepot {
                    game,
                    steamcmd,
                    username,
                    language,
                });
                ui.set_modal_title("发现未标记版本的 Depot 缓存".into());
                ui.set_modal_message(
                    format!(
                        "已存在 {}（Depot {}）缓存文件，但无法确认是否匹配当前游戏版本。\n您可以选择直接沿用已有缓存，或删除旧缓存后重新下载。",
                        language.name, language.depot_id
                    )
                    .into(),
                );
                ui.set_modal_is_danger(false);
                ui.set_modal_primary_text("沿用已有缓存".into());
                ui.set_modal_secondary_text("删除并重新下载".into());
                ui.set_modal_cancel_text("取消".into());
                ui.set_modal_visible(true);
                return;
            }
            run_job(&ui, move || install(game, steamcmd, username, language));
        }
    });

    ui.on_modal_primary_clicked({
        let weak = ui.as_weak();
        let pending = Rc::clone(&pending_action);
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let action = pending.borrow_mut().take();
            ui.set_modal_visible(false);
            match action {
                Some(PendingModalAction::RemoveVoice(state)) => {
                    run_job(&ui, move || {
                        core::remove_installed_voice(&state).map(|count| {
                            ("语音已删除".into(), format!("已删除 {count} 个文件/链接。"))
                        })
                    });
                }
                Some(PendingModalAction::UnverifiedDepot {
                    game,
                    steamcmd,
                    username,
                    language,
                }) => {
                    run_job(&ui, move || install(game, steamcmd, username, language));
                }
                Some(PendingModalAction::BuildIncompatible(state)) => {
                    match core::remove_installed_voice(&state) {
                        Ok(count) => set_status(
                            &ui,
                            "旧语音已清理",
                            &format!("已删除 {count} 个旧文件。请选择所需语言重新下载安装。"),
                        ),
                        Err(error) => set_status(&ui, "无法删除旧语音", &error.to_string()),
                    }
                }
                None => {}
            }
        }
    });

    ui.on_modal_secondary_clicked({
        let weak = ui.as_weak();
        let pending = Rc::clone(&pending_action);
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let action = pending.borrow_mut().take();
            ui.set_modal_visible(false);
            if let Some(PendingModalAction::UnverifiedDepot {
                game,
                steamcmd,
                username,
                language,
            }) = action
            {
                if let Err(error) = core::delete_depot(&steamcmd, language.depot_id) {
                    set_status(&ui, "无法删除旧 Depot", &error.to_string());
                    return;
                }
                run_job(&ui, move || install(game, steamcmd, username, language));
            }
        }
    });

    ui.on_modal_cancel_clicked({
        let weak = ui.as_weak();
        let pending = Rc::clone(&pending_action);
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let _ = pending.borrow_mut().take();
            ui.set_modal_visible(false);
        }
    });

    check_build_compatibility(&ui, &pending_action);
    ui.run()?;
    Ok(())
}

fn update_language(ui: &MainWindow, index: i32) {
    if let Some(language) = LANGUAGES.get(index as usize) {
        ui.set_launch_option(format!("+miles_language {}", language.launch_code).into());
    }
}

fn refresh_status(ui: &MainWindow) {
    let Some(language) = LANGUAGES.get(ui.get_selected_language() as usize) else {
        return;
    };
    let steamcmd_path = ui.get_steamcmd_path();
    let steamcmd = Path::new(steamcmd_path.as_str());
    if core::depot_exists(steamcmd, language.depot_id) {
        let cached = core::read_depot_build_id(steamcmd, language.depot_id);
        let game = core::read_build_id(Path::new(ui.get_game_path().as_str()));
        if cached.is_some() && game.is_some() && cached != game {
            ui.set_install_label("更新并安装语音".into());
            set_status(
                ui,
                "已有 Depot 与当前游戏版本不同",
                "安装时将删除并重新下载。",
            );
        } else {
            ui.set_install_label("安装已有语音".into());
            set_status(
                ui,
                "已发现下载好的语言 Depot",
                &format!(
                    "{} Depot {} 已存在，不会重新下载。",
                    language.name, language.depot_id
                ),
            );
        }
    } else {
        ui.set_install_label("下载并安装语音".into());
        set_status(
            ui,
            "尚未下载",
            &format!(
                "将通过 SteamCMD 下载 {} Depot {}。",
                language.name, language.depot_id
            ),
        );
    }
}

fn install(
    game: PathBuf,
    steamcmd: PathBuf,
    username: String,
    language: core::VoiceLanguage,
) -> Result<(String, String)> {
    let build =
        core::read_build_id(&game).ok_or_else(|| anyhow::anyhow!("无法读取游戏 Build ID"))?;
    if core::depot_exists(&steamcmd, language.depot_id)
        && let Some(cached) = core::read_depot_build_id(&steamcmd, language.depot_id)
        && cached != build
    {
        core::delete_depot(&steamcmd, language.depot_id)?;
    }
    if !core::depot_exists(&steamcmd, language.depot_id) {
        if username.trim().is_empty() {
            bail!("需要 Steam 用户名。密码及 Steam Guard 将在 SteamCMD 窗口输入。")
        }
        core::ensure_steamcmd(&steamcmd)?;
        let exit = core::download_depot(&steamcmd, username.trim(), language.depot_id)?;
        if exit != 0 || !core::depot_exists(&steamcmd, language.depot_id) {
            bail!("SteamCMD 未完成 Depot 下载（退出码 {exit}）。")
        }
    }
    core::write_depot_build_id(&steamcmd, language.depot_id, &build)?;
    if let Some(previous) = core::load_state() {
        core::remove_installed_voice(&previous)?;
    }
    let state = core::install_voice_files(
        &core::depot_ship_directory(&steamcmd, language.depot_id),
        &core::audio_ship_directory(&game),
        &build,
        language,
    )?;
    Ok((
        format!("{}语音安装完成", language.name),
        format!(
            "已通过“{}”安装 {} 个语音文件。",
            state.installation_method,
            state.files.len()
        ),
    ))
}

fn run_job<F>(ui: &MainWindow, job: F)
where
    F: FnOnce() -> Result<(String, String)> + Send + 'static,
{
    ui.set_busy(true);
    set_status(ui, "正在处理", "耗时操作在后台运行，请等待。");
    let weak = ui.as_weak();
    thread::spawn(move || {
        let result = job();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_busy(false);
                match result {
                    Ok((title, message)) => set_status(&ui, &title, &message),
                    Err(error) => set_status(&ui, "操作失败", &format!("{error:#}")),
                }
            }
        });
    });
}

fn check_build_compatibility(ui: &MainWindow, pending: &Rc<RefCell<Option<PendingModalAction>>>) {
    let Some(state) = core::load_state() else {
        return;
    };
    if !build_changed(
        core::read_build_id(&state.game_directory).as_deref(),
        &state.build_id,
    ) {
        return;
    }
    *pending.borrow_mut() = Some(PendingModalAction::BuildIncompatible(state.clone()));
    ui.set_modal_title("检测到游戏版本已更新".into());
    ui.set_modal_message(
        format!(
            "检测到 Apex Legends 已更新（已安装语音基于旧版本 {}）。\n旧语音文件可能不兼容，必须清理已安装的 {} 语音文件后重新安装。",
            state.build_id, state.language
        )
        .into(),
    );
    ui.set_modal_is_danger(true);
    ui.set_modal_primary_text("清理旧语音".into());
    ui.set_modal_secondary_text("".into());
    ui.set_modal_cancel_text("".into());
    ui.set_modal_visible(true);
}

fn build_changed(current: Option<&str>, installed: &str) -> bool {
    current.is_some_and(|current| current != installed)
}

fn set_status(ui: &MainWindow, title: &str, message: &str) {
    ui.set_status_title(title.into());
    ui.set_status_message(message.into());
}

#[cfg(test)]
mod tests {
    use super::build_changed;

    #[test]
    fn only_reports_known_different_builds() {
        assert!(!build_changed(None, "123"));
        assert!(!build_changed(Some("123"), "123"));
        assert!(build_changed(Some("456"), "123"));
    }
}
