#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod audio;
mod commands;
mod config;
mod images;
mod samples;
mod state;
mod window;

use config::Config;
use state::{lock, AppState, Runtime, Shared};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(state) = app.try_state::<Shared>() {
                if args.len() <= 1 {
                    actions::open_control(app, &state);
                }
                actions::handle_args(app, &state, args);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if let Some(state) = app.try_state::<Shared>() {
                            actions::toggle_edit_mode(app, &state);
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let config_file = app.path().app_config_dir()?.join("config.json");
            let images_dir = app.path().app_data_dir()?.join("images");
            std::fs::create_dir_all(&images_dir)?;

            let work_bottom = window::stage::monitor_infos(&handle).first().map(|m| m.work_bottom).unwrap_or(0.955);
            let config = Config::load(&config_file).unwrap_or_else(|| {
                // 첫 실행: 샘플 인형 3개.
                let mut c = Config::default_with(work_bottom);
                c.dolls = samples::install(&images_dir);
                c.normalize();
                c
            });

            let hidden = std::env::args().any(|a| a == "--hidden");
            let state: Shared = Arc::new(AppState {
                config: Mutex::new(config),
                runtime: Mutex::new(Runtime::default()),
                audio_version: AtomicU64::new(0),
                dirty: AtomicBool::new(true),
                control_visible: AtomicBool::new(!hidden),
                stage_seq: AtomicU32::new(1),
                stage_label: Mutex::new(String::new()),
                work_bottom: Mutex::new(work_bottom),
                config_file,
                images_dir,
            });
            app.manage(state.clone());

            // 조작 창은 숨긴 채 만들어진다(tauri.conf.json). 지난번 크기·위치를 먼저 적용한 뒤 보여 준다.
            if let Some(w) = app.get_webview_window("control") {
                if let Some(path) = window::control::file_for(&handle) {
                    window::control::restore(&handle, &w, path);
                }
                if !hidden {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }

            // 실패해도(다른 앱이 단축키를 쓰는 중) 트레이로 같은 동작을 할 수 있다.
            let _ = app.global_shortcut().register(Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyP));

            window::tray::build(&handle, &state)?;
            window::stage::rebuild(&handle, &state);
            audio::start(handle.clone(), state.clone());
            window::cursor::start(handle.clone(), state.clone());
            actions::handle_args(&handle, &state, std::env::args().collect());

            // 변경이 있으면 잠시 뒤 한 번에 저장한다.
            let saver = state.clone();
            std::thread::Builder::new().name("config-saver".into()).spawn(move || loop {
                std::thread::sleep(Duration::from_millis(400));
                if saver.dirty.load(Ordering::Acquire) {
                    saver.save_now();
                }
                window::control::save_if_dirty();
            })?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // 조작 창은 닫아도 앱이 트레이에 남는다.
            if window.label() == "control" {
                match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        window::control::save_if_dirty();
                        let _ = window.hide();
                        if let Some(state) = window.try_state::<Shared>() {
                            state.control_visible.store(false, Ordering::Relaxed);
                        }
                    }
                    // 크기·위치를 기억한다 (껐다 켜도 그대로).
                    WindowEvent::Moved(_) | WindowEvent::Resized(_) => window::control::track(window),
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::get_runtime,
            commands::images_dir,
            commands::list_monitors,
            commands::list_audio_sessions,
            commands::add_dolls,
            commands::remove_doll,
            commands::replace_doll_image,
            commands::set_background_image,
            commands::set_edit_mode,
            commands::export_preset,
            commands::import_preset,
        ])
        .build(tauri::generate_context!())
        .expect("failed to start PlushBounce");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            window::control::save_if_dirty();
            if let Some(state) = app.try_state::<Shared>() {
                if lock(&state.runtime).edit_mode || state.dirty.load(Ordering::Acquire) {
                    state.save_now();
                }
            }
        }
    });
}
