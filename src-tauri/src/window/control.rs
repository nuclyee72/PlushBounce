//! 조작 창의 크기·위치를 기억한다 (`app_config_dir/window.json`). 껐다 켜도 같은 자리, 같은 크기로 열린다.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

/// 창이 이만큼(px)은 어느 모니터 안에 보여야 저장된 자리를 쓴다 (모니터를 뺐을 때 화면 밖에 열리지 않게).
const MIN_VISIBLE: i32 = 120;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    /// 안쪽(내용) 크기, 물리 픽셀.
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub maximized: bool,
}

static CURRENT: Mutex<Option<Bounds>> = Mutex::new(None);
static DIRTY: AtomicBool = AtomicBool::new(false);
static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn load(path: &Path) -> Option<Bounds> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// 저장된 크기·위치를 조작 창에 적용한다. 창은 숨긴 채로 만들어져 있으므로 깜빡이지 않는다.
pub fn restore(app: &AppHandle, window: &WebviewWindow, path: PathBuf) {
    *lock(&FILE) = Some(path.clone());
    let Some(b) = load(&path) else { return };
    let visible = app.available_monitors().unwrap_or_default().iter().any(|m| {
        let (p, s) = (m.position(), m.size());
        let right = (b.x + b.width as i32).min(p.x + s.width as i32);
        let bottom = (b.y + 40).min(p.y + s.height as i32); // 제목 표시줄이 보이는가
        right - b.x.max(p.x) >= MIN_VISIBLE && bottom - b.y.max(p.y) > 0
    });
    if b.width >= 200 && b.height >= 150 {
        let _ = window.set_size(PhysicalSize::new(b.width, b.height));
    }
    if visible {
        let _ = window.set_position(PhysicalPosition::new(b.x, b.y));
    } else {
        let _ = window.center();
    }
    if b.maximized {
        let _ = window.maximize();
    }
    *lock(&CURRENT) = Some(b);
}

/// 창이 움직이거나 크기가 바뀔 때 부른다. 최대화·최소화 중에는 평소 크기를 덮어쓰지 않는다.
pub fn track(window: &tauri::Window) {
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let maximized = window.is_maximized().unwrap_or(false);
    let mut cur = lock(&CURRENT);
    let next = if maximized {
        match *cur {
            Some(b) => Bounds { maximized: true, ..b },
            None => return,
        }
    } else {
        let (Ok(p), Ok(s)) = (window.outer_position(), window.inner_size()) else { return };
        Bounds { x: p.x, y: p.y, width: s.width, height: s.height, maximized: false }
    };
    if *cur != Some(next) {
        *cur = Some(next);
        DIRTY.store(true, Ordering::Release);
    }
}

/// 바뀐 것이 있으면 파일에 쓴다 (설정 저장 스레드와 종료 때 부른다).
pub fn save_if_dirty() {
    if !DIRTY.swap(false, Ordering::AcqRel) {
        return;
    }
    let (Some(b), Some(path)) = (*lock(&CURRENT), lock(&FILE).clone()) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(&b) {
        let _ = std::fs::write(path, text);
    }
}

/// 설정 파일과 같은 폴더의 window.json.
pub fn file_for(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("window.json"))
}
