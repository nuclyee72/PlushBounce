//! 트레이·단축키·command가 함께 쓰는 동작들.

use crate::config::{BandId, React, Config, Doll, Manual, Mode, Source};
use crate::images;
use crate::state::{lock, Shared};
use crate::window::{stage, tray};
use std::path::Path;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};

pub fn open_control(app: &AppHandle, state: &Shared) {
    state.control_visible.store(true, Ordering::Relaxed);
    if let Some(w) = app.get_webview_window("control") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// 편집 모드는 펫 모드에서만 켤 수 있다. 끌 때 설정을 저장한다.
pub fn set_edit_mode(app: &AppHandle, state: &Shared, on: bool) {
    let on = on && lock(&state.config).display.mode == Mode::Pet;
    if lock(&state.runtime).edit_mode != on {
        state.update_runtime(app, |r| r.edit_mode = on);
    }
    stage::apply_edit(app, state);
    tray::sync(app, state);
    if !on {
        state.save_now();
    }
}

pub fn toggle_edit_mode(app: &AppHandle, state: &Shared) {
    let on = !lock(&state.runtime).edit_mode;
    set_edit_mode(app, state, on);
}

/// 새 설정을 적용·저장·브로드캐스트한다. 모드나 모니터가 바뀌면 stage 창을 다시 만든다.
/// 창을 만들 수 있으므로 메인 스레드에서 직접 부르지 않는다.
pub fn apply_config(app: &AppHandle, state: &Shared, mut new: Config, origin: &str) -> Config {
    let (rebuild, audio_changed) = {
        let work_bottom = *lock(&state.work_bottom);
        let mut cur = lock(&state.config);
        new.follow_mode_change(cur.display.mode, work_bottom);
        new.normalize();
        let rebuild = cur.display.mode != new.display.mode || cur.display.monitor != new.display.monitor;
        let audio_changed = cur.audio != new.audio;
        *cur = new;
        (rebuild, audio_changed)
    };
    if audio_changed {
        state.audio_version.fetch_add(1, Ordering::AcqRel);
    }
    state.broadcast_config(app, origin);
    if rebuild {
        stage::rebuild(app, state);
        tray::sync(app, state);
    }
    state.config()
}

/// 이미지들을 인형으로 추가한다. (새 설정, 열지 못한 파일 수)를 돌려준다.
pub fn add_dolls(app: &AppHandle, state: &Shared, paths: &[String]) -> (Config, usize) {
    let mut cfg = state.config();
    let mut order = cfg.dolls.iter().map(|d| d.order + 1).max().unwrap_or(0);
    let mut failed = 0;
    for p in paths {
        let id = images::new_id();
        match images::import(Path::new(p), &state.images_dir, &id) {
            Ok((image, aspect)) => {
                cfg.dolls.push(Doll {
                    id,
                    image,
                    aspect,
                    order,
                    flip: false,
                    visible: true,
                    band: BandId::Full,
                    react: React::Beat,
                    x: 0.5,
                    y: 0.8,
                    scale: 0.2,
                    size: 1.0,
                    line: String::new(),
                    at: None,
                    group: None,
                    manual: Manual::default(),
                });
                order += 1;
            }
            Err(_) => failed += 1,
        }
    }
    (apply_config(app, state, cfg, "rust"), failed)
}

/// 실행 인자 처리: 이미지 경로는 인형으로 추가, `--mode=pet|wallpaper`, `--edit=on|off`,
/// `--source=system|<실행 파일 이름>[,<실행 파일 이름>…]`.
/// 이미 실행 중일 때 다시 실행하면 그 인자가 여기로 온다. 창을 만들 수 있어 별도 스레드에서 돈다.
pub fn handle_args(app: &AppHandle, state: &Shared, args: Vec<String>) {
    let (app, state) = (app.clone(), state.clone());
    std::thread::spawn(move || {
        let is_image = |a: &String| images::is_supported(a) && Path::new(a).is_file();
        let images: Vec<String> = args.iter().skip(1).filter(|a| is_image(a)).cloned().collect();
        if !images.is_empty() {
            add_dolls(&app, &state, &images);
        }
        for a in &args {
            match a.as_str() {
                "--mode=pet" | "--mode=wallpaper" => {
                    let mut cfg = state.config();
                    cfg.display.mode = if a.ends_with("pet") { Mode::Pet } else { Mode::Wallpaper };
                    apply_config(&app, &state, cfg, "rust");
                }
                "--source=system" => {
                    let mut cfg = state.config();
                    cfg.audio.source = Source::System;
                    apply_config(&app, &state, cfg, "rust");
                }
                s if s.starts_with("--source=") => {
                    // 쉼표로 여러 앱을 줄 수 있다: --source=chrome.exe,spotify.exe
                    let names = s["--source=".len()..].split(',').map(|n| n.trim().to_string()).collect();
                    let mut cfg = state.config();
                    cfg.audio.source = Source::Apps { names };
                    apply_config(&app, &state, cfg, "rust");
                }
                "--edit=on" => set_edit_mode(&app, &state, true),
                "--edit=off" => set_edit_mode(&app, &state, false),
                _ => {}
            }
        }
    });
}

pub fn toggle_mode(app: &AppHandle, state: &Shared) {
    let (app, state) = (app.clone(), state.clone());
    std::thread::spawn(move || {
        let mut cfg = state.config();
        cfg.display.mode = if cfg.display.mode == Mode::Pet { Mode::Wallpaper } else { Mode::Pet };
        apply_config(&app, &state, cfg, "rust");
    });
}

pub fn quit(app: &AppHandle, state: &Shared) {
    state.save_now();
    app.exit(0);
}
