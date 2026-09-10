pub mod db;
pub mod db_error;
pub mod resolve;

pub use db::{
    app_db_path, insert_ledger, migrate, open, redeem, write_heartbeat,
    write_heartbeat_at_default_path,
};
pub use db_error::{map_rusqlite, DbOpError};
pub use resolve::resolve_slot;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, RunEvent, WindowEvent,
};
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
        .setup(|app| {
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

            let icon = app.default_window_icon().cloned();
            let _tray = TrayIconBuilder::new()
                .icon(icon.unwrap())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main_window(app),
                    "pause_30" | "pause_60" | "pause_90" => {}
                    "end_day" => {
                        app.dialog()
                            .message("结束今天功能尚未实现。")
                            .title("结束今天")
                            .kind(MessageDialogKind::Info)
                            .show(|_| {});
                    }
                    "quit" => {
                        write_quit_heartbeat();
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
                .build(app)?;

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
        .run(|_app, event| {
            if let RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
