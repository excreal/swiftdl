// Release builds are GUI apps: no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod browser;
mod commands;
mod manager;
mod media;
mod hls;
mod native_host;
mod settings;
mod torrent;
mod windows;

use manager::DownloadManager;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// SwiftDL lives in the tray like IDM: closing the main window keeps
/// downloads (and browser integration) running.
fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open SwiftDL", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit SwiftDL", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main").tooltip("SwiftDL").menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, event| match event.id().as_ref() {
        "open" => show_main_window(app),
        "quit" => {
            app.state::<Arc<DownloadManager>>().save();
            app.exit(0);
        }
        _ => {}
    })
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            show_main_window(tray.app_handle());
        }
    })
    .build(app)?;
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if native_host::is_native_host_launch(&args) {
        native_host::run();
        return;
    }

    tauri::Builder::default()
        // Must be first: a second launch (e.g. from the extension) hands its
        // args to this window and exits.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(inc) = native_host::parse_add_arg(&args).or_else(|| torrent::parse_open_arg(&args)) {
                app.state::<Arc<DownloadManager>>().incoming(inc);
            } else {
                show_main_window(app);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            let data_dir = app.path().app_data_dir()?;
            let downloads = app.path().download_dir().unwrap_or_else(|_| data_dir.join("Downloads"));
            let mgr = DownloadManager::new(app.handle().clone(), data_dir.clone(), downloads)?;
            if let Err(e) = browser::register(&data_dir) {
                eprintln!("browser integration not registered: {e:#}");
            }
            if let Err(e) = torrent::register_handlers() {
                eprintln!("magnet and .torrent handlers not registered: {e:#}");
            }
            if let Err(e) = browser::offer_extension(mgr.settings().offer_extension) {
                eprintln!("could not offer the browser extension: {e:#}");
            }
            app.manage(mgr.clone());
            if let Some(main) = app.get_webview_window("main") {
                windows::polish(&main);
            }
            if let Err(e) = build_tray(app.handle()) {
                eprintln!("tray icon not available: {e:#}");
            }
            mgr.start_ticker();
            // Launched by the browser extension: show only the small
            // download window, like IDM. Otherwise open the main window.
            match native_host::parse_add_arg(&args).or_else(|| torrent::parse_open_arg(&args)) {
                Some(inc) => mgr.incoming(inc),
                None => show_main_window(app.handle()),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::probe_url,
            commands::probe_media,
            commands::is_media_url,
            commands::is_torrent_url,
            commands::probe_torrent,
            commands::open_new_download,
            commands::default_dir,
            commands::add_download,
            commands::pause,
            commands::resume,
            commands::remove,
            commands::open_file,
            commands::open_folder,
            commands::set_settings,
            commands::open_url,
            commands::extension_dir,
            commands::open_extension_folder,
            commands::pick_folder,
            commands::integration_status,
            commands::open_store,
            commands::get_item,
            commands::confirm_info,
            commands::forget_confirm,
            commands::show_main,
        ])
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                let mgr = window.app_handle().state::<Arc<DownloadManager>>();
                mgr.save();
                if mgr.settings().close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("error while starting SwiftDL")
        .run(|_app, _event| {
            // macOS hands over magnet links and .torrent files as an event,
            // not as arguments.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = _event {
                for url in urls {
                    let link = if url.scheme() == "file" {
                        url.to_file_path().ok().map(|p| p.to_string_lossy().into_owned())
                    } else {
                        Some(url.to_string())
                    };
                    if let (Some(link), Some(mgr)) = (link, _app.try_state::<Arc<DownloadManager>>()) {
                        if torrent::is_torrent(&link, None) {
                            mgr.incoming(manager::Incoming { url: link, filename: None, referrer: None, cookies: None, user_agent: None, stream_kind: None, media_cookies: vec![] });
                        }
                    }
                }
            }
        });
}
