//! stage(렌더) 창. 모드나 모니터가 바뀌면 새로 만든다.

use super::wallpaper;
use crate::config::Mode;
use crate::state::{lock, Shared};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

static REBUILD: Mutex<()> = Mutex::new(());
/// 지금 stage 창이 덮는 화면 영역 (물리 픽셀: x, y, 너비, 높이).
pub static STAGE_RECT: Mutex<(i32, i32, u32, u32)> = Mutex::new((0, 0, 1, 1));

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub work_bottom: f32,
}

struct Mon {
    name: String,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    scale: f64,
    work_bottom: f32,
}

fn monitors(app: &AppHandle) -> Vec<Mon> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (pos, size, work) = (m.position(), m.size(), m.work_area());
            let bottom = (work.position.y - pos.y) as f32 + work.size.height as f32;
            Mon {
                name: m.name().cloned().filter(|n| !n.is_empty()).unwrap_or_else(|| format!("{}", i + 1)),
                x: pos.x,
                y: pos.y,
                w: size.width,
                h: size.height,
                scale: m.scale_factor().max(0.1),
                work_bottom: (bottom / size.height.max(1) as f32).clamp(0.3, 1.0),
            }
        })
        .collect()
}

pub fn monitor_infos(app: &AppHandle) -> Vec<MonitorInfo> {
    monitors(app)
        .into_iter()
        .map(|m| MonitorInfo { name: m.name, width: m.w, height: m.h, work_bottom: m.work_bottom })
        .collect()
}

/// 선택한 모니터의 작업 영역 비율을 상태에 반영하고 돌려준다.
pub fn refresh_work_bottom(app: &AppHandle, state: &Shared) -> f32 {
    let mons = monitors(app);
    let index = lock(&state.config).display.monitor;
    let wb = mons.get(index).or(mons.first()).map(|m| m.work_bottom).unwrap_or(0.955);
    *lock(&state.work_bottom) = wb;
    wb
}

pub fn current(app: &AppHandle, state: &Shared) -> Option<WebviewWindow> {
    let label = lock(&state.stage_label).clone();
    app.get_webview_window(&label)
}

/// 편집 모드에 맞춰 클릭 통과를 켜고 끈다 (펫 모드 전용).
pub fn apply_edit(app: &AppHandle, state: &Shared) {
    let edit = lock(&state.runtime).edit_mode;
    if let Some(win) = current(app, state) {
        let _ = win.set_ignore_cursor_events(!edit);
        if edit {
            let _ = win.set_focus();
        }
    }
}

fn build(app: &AppHandle, state: &Shared, mode: Mode) -> Result<WebviewWindow, String> {
    let mons = monitors(app);
    let index = lock(&state.config).display.monitor;
    let mon = mons.get(index).or(mons.first()).ok_or("Couldn't find the monitor.")?;
    let label = format!("stage-{}", state.stage_seq.fetch_add(1, Ordering::Relaxed));
    let pet = mode == Mode::Pet;

    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("stage.html".into()))
        .title("PlushBounce Stage")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(pet)
        .focused(false)
        // 펫 모드는 처음부터 보이는 창으로 만든다. 나중에 ShowWindow로 직접 띄우면 tao의 내부 상태와
        // 어긋나서, 클릭 통과를 바꿀 때 창이 다시 숨겨진다.
        .visible(pet)
        .position(mon.x as f64 / mon.scale, mon.y as f64 / mon.scale)
        .inner_size(mon.w as f64 / mon.scale, mon.h as f64 / mon.scale)
        .build()
        .map_err(|e| e.to_string())?;

    let _ = win.set_position(PhysicalPosition::new(mon.x, mon.y));
    let _ = win.set_size(PhysicalSize::new(mon.w, mon.h));
    *lock(&STAGE_RECT) = (mon.x, mon.y, mon.w, mon.h);

    let placed = if pet {
        let edit = lock(&state.runtime).edit_mode;
        let _ = win.set_ignore_cursor_events(!edit);
        #[cfg(target_os = "macos")]
        wallpaper::float_everywhere(&win);
        Ok(())
    } else {
        #[cfg(windows)]
        let r = win
            .hwnd()
            .map_err(|e| e.to_string())
            .and_then(|hwnd| wallpaper::attach(hwnd, (mon.x, mon.y, mon.w as i32, mon.h as i32)));
        #[cfg(target_os = "macos")]
        let r = wallpaper::attach(&win);
        if r.is_ok() {
            wallpaper::mark_attached(&state.config_file);
        }
        r
    };
    if let Err(e) = placed {
        let _ = win.destroy();
        return Err(e);
    }
    Ok(win)
}

/// 현재 설정의 모드·모니터에 맞는 stage 창을 새로 만들고 이전 창을 닫는다.
/// 바탕화면 모드가 실패하면 오류를 표시하고 펫 모드로 되돌린다.
pub fn rebuild(app: &AppHandle, state: &Shared) {
    let _guard = lock(&REBUILD);
    refresh_work_bottom(app, state);
    let mode = lock(&state.config).display.mode;
    if mode == Mode::Wallpaper && lock(&state.runtime).edit_mode {
        state.update_runtime(app, |r| r.edit_mode = false);
    }
    let old = current(app, state);

    let result = match build(app, state, mode) {
        Ok(win) => {
            if lock(&state.runtime).display_error.is_some() && mode == Mode::Wallpaper {
                state.update_runtime(app, |r| r.display_error = None);
            }
            Ok(win)
        }
        Err(e) if mode == Mode::Wallpaper => {
            state.update_runtime(app, |r| {
                r.display_error = Some(format!("Couldn't turn on wallpaper mode; switched back to pet mode. ({e})"));
            });
            lock(&state.config).display.mode = Mode::Pet;
            state.broadcast_config(app, "rust");
            build(app, state, Mode::Pet)
        }
        Err(e) => Err(e),
    };
    match result {
        Ok(win) => {
            *lock(&state.stage_label) = win.label().to_string();
            if let Some(old) = old {
                let _ = old.destroy();
            }
            // 바탕화면 모드에서 나왔다: 배경이 검게 남지 않도록 바탕화면을 다시 그리게 한다.
            if lock(&state.config).display.mode != Mode::Wallpaper {
                wallpaper::detached(&state.config_file);
            }
        }
        Err(e) => state.update_runtime(app, |r| r.display_error = Some(format!("Couldn't create the doll window. ({e})"))),
    }
}
