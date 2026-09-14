pub mod commands;
pub mod config;
pub mod db;
pub mod db_error;
pub mod keychain;
pub mod macos;
pub mod platform;
pub mod resolve;
pub mod sampler;
pub mod scheduler;
pub mod text_ai;
pub mod ticktick;
pub mod vision;

pub use db::{
    app_db_path, insert_ledger, migrate, open, write_heartbeat,
    write_heartbeat_at_default_path,
};
pub use db::redeem as db_redeem;
pub use scheduler::ensure_slot;
pub use db_error::{map_rusqlite, DbOpError};
pub use resolve::resolve_slot;

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::config::{load_settings, show_window_on_launch};
use sampler::PauseControl;

use commands::{
    archive_wish, continue_previous_workday, create_list, create_wish, end_today, freeze,
    get_app_report, get_month_report, get_permission_status, get_rhythm_report, get_settings,
    get_today, get_day_view, get_week, has_api_key, list_tasks, parse_task_line_cmd,
    open_privacy_settings, provider_key_status, redeem, report_misclassification, request_screen_recording, review_slot,
    save_settings, set_api_key, set_provider_api_key, set_quests, test_vision_provider, ticktick_begin_oauth,
    ticktick_disconnect, ticktick_finish_oauth, ticktick_set_client_secret,
    ticktick_status, ticktick_sync, ticktick_tree, toggle_task_done, update_wish, upsert_task,
};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, RunEvent, WindowEvent,
};

static ALLOW_EXIT: AtomicBool = AtomicBool::new(false);

fn update_tray_tooltip(app: &AppHandle) {
    let label = crate::commands::tray_tooltip_for_today_db().unwrap_or_else(|_| "0h 0m / 8h".into());
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(&label));
    }
}

fn start_tray_tooltip_updater(app: AppHandle) {
    thread::spawn(move || {
        loop {
            update_tray_tooltip(&app);
            thread::sleep(Duration::from_secs(30));
        }
    });
}
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn write_quit_heartbeat() {
    if let Err(err) = write_heartbeat_at_default_path() {
        eprintln!("heartbeat on quit failed: {err:?}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            end_today,
            freeze,
            get_today,
            get_day_view,
            get_week,
            get_month_report,
            get_rhythm_report,
            get_app_report,
            set_quests,
            list_tasks,
            upsert_task,
            toggle_task_done,
            parse_task_line_cmd,
            create_list,
            continue_previous_workday,
            review_slot,
            report_misclassification,
            redeem,
            create_wish,
            update_wish,
            archive_wish,
            get_settings,
            save_settings,
            set_api_key,
            set_provider_api_key,
            has_api_key,
            provider_key_status,
            test_vision_provider,
            get_permission_status,
            request_screen_recording,
            open_privacy_settings,
            ticktick_status,
            ticktick_set_client_secret,
            ticktick_begin_oauth,
            ticktick_finish_oauth,
            ticktick_disconnect,
            ticktick_sync,
            ticktick_tree,
        ])
        .setup(|app| {
            let pause = PauseControl::new();
            if let Some(db_path) = app_db_path() {
                if let Some(parent) = db_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                sampler::start_sampler_thread(db_path, pause.paused_flag());
            }
            app.manage(pause);

            if !crate::macos::screen_recording_granted() {
                let _ = crate::macos::request_screen_recording();
            }

            let open_i = MenuItem::with_id(app, "open", "打开", true, None::<&str>)?;
            let pause_30_i =
                MenuItem::with_id(app, "pause_30", "暂停 30", true, None::<&str>)?;
            let pause_60_i =
                MenuItem::with_id(app, "pause_60", "暂停 60", true, None::<&str>)?;
            let pause_90_i =
                MenuItem::with_id(app, "pause_90", "暂停 90", true, None::<&str>)?;
            let end_day_i =
                MenuItem::with_id(app, "end_day", "结束今天", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &open_i,
                    &pause_30_i,
                    &pause_60_i,
                    &pause_90_i,
                    &end_day_i,
                    &quit_i,
                ],
            )?;

            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("missing default window icon")?;
            let tray = TrayIconBuilder::with_id("main")
                .icon(icon)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main_window(app),
                    "pause_30" => {
                        if let Some(pause) = app.try_state::<PauseControl>() {
                            pause.pause_for(30 * 60);
                        }
                    }
                    "pause_60" => {
                        if let Some(pause) = app.try_state::<PauseControl>() {
                            pause.pause_for(60 * 60);
                        }
                    }
                    "pause_90" => {
                        if let Some(pause) = app.try_state::<PauseControl>() {
                            pause.pause_for(90 * 60);
                        }
                    }
                    "end_day" => {
                        app.dialog()
                            .message("确定结束今天？当前槽将立即结算，后续不再采样。")
                            .title("结束今天")
                            .kind(MessageDialogKind::Warning)
                            .show(move |confirmed| {
                                if confirmed {
                                    if let Err(err) = crate::commands::run_end_today() {
                                        eprintln!("end_today failed: {err}");
                                    }
                                }
                            });
                    }
                    "quit" => {
                        write_quit_heartbeat();
                        ALLOW_EXIT.store(true, Ordering::Relaxed);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app);

            match tray {
                Ok(_) => {}
                // A tray-only app with no tray has no way in at all: the window
                // starts hidden and 关闭 only hides it again, so the process
                // would run forever unreachable. Showing the window keeps the UI
                // reachable — Linux desktops with no AppIndicator host, mainly.
                Err(err) => {
                    eprintln!("tray unavailable ({err}); showing the window instead");
                    show_main_window(app.handle());
                }
            }

            update_tray_tooltip(app.handle());
            start_tray_tooltip_updater(app.handle().clone());
            if show_window_on_launch(load_settings().silent_start) {
                show_main_window(app.handle());
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            RunEvent::ExitRequested { api, .. } => {
                if !ALLOW_EXIT.load(Ordering::Relaxed) {
                    api.prevent_exit();
                }
            }
            // The red button does not close the window, it hides it
            // (CloseRequested -> prevent_close + hide), so the window outlives
            // it and the tray is not the only way back. macOS asks the app to
            // reopen via applicationShouldHandleReopen; without this arm the
            // Dock icon is inert because nothing answers the question.
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => show_main_window(app),
            _ => {}
        });
}

#[cfg(test)]
mod exit_tests {
    use super::*;

    #[test]
    fn allow_exit_starts_false() {
        ALLOW_EXIT.store(false, Ordering::Relaxed);
        assert!(!ALLOW_EXIT.load(Ordering::Relaxed));
    }
}
