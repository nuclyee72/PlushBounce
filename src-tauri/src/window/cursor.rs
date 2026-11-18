//! 마우스 위치를 stage 창에 알려 준다 (펫 모드의 stage 창은 클릭 통과라 스스로 마우스를 볼 수 없다).
//! 인형이 마우스 근처에서 흐려지는 데 쓴다.

use super::stage::STAGE_RECT;
use crate::state::{lock, Shared};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// 마우스의 화면 좌표 (물리 픽셀, 모니터 좌표와 같은 기준).
#[cfg(windows)]
fn cursor_pos(_app: &AppHandle) -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.ok().map(|_| (p.x, p.y))
}

#[cfg(not(windows))]
fn cursor_pos(app: &AppHandle) -> Option<(i32, i32)> {
    app.cursor_position().ok().map(|p| (p.x.round() as i32, p.y.round() as i32))
}

/// stage 창 기준 비율 좌표 (0..1 = 창 안).
#[derive(Serialize, Clone, Copy, PartialEq)]
struct CursorPos {
    x: f32,
    y: f32,
}

pub fn start(app: AppHandle, state: Shared) {
    let _ = std::thread::Builder::new().name("cursor-watch".into()).spawn(move || {
        let mut last: Option<(String, Option<(i32, i32)>)> = None;
        loop {
            let on = lock(&state.config).display.hover_fade && !lock(&state.runtime).edit_mode;
            let label = lock(&state.stage_label).clone();
            if !on || label.is_empty() {
                // 꺼질 때 한 번 "마우스 없음"을 보내 흐려진 인형을 되돌린다.
                if let Some((l, Some(_))) = &last {
                    let _ = app.emit_to(l.as_str(), "cursor", None::<CursorPos>);
                }
                last = None;
                std::thread::sleep(Duration::from_millis(250));
                continue;
            }
            let now = cursor_pos(&app);
            let changed = match &last {
                Some((l, prev)) => *l != label || *prev != now,
                None => true,
            };
            if changed {
                let (x, y, w, h) = *lock(&STAGE_RECT);
                let pos = now.map(|(px, py)| CursorPos { x: (px - x) as f32 / w.max(1) as f32, y: (py - y) as f32 / h.max(1) as f32 });
                let _ = app.emit_to(label.as_str(), "cursor", pos);
                last = Some((label, now));
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    });
}
