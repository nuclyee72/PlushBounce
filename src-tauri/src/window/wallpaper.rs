//! 바탕화면 모드: stage 창을 바탕화면 아이콘 뒤 레이어에 붙인다.
//!
//! 두 가지 구조를 모두 처리한다 (Lively Wallpaper의 WinDesktopCore와 같은 방식).
//! - 구버전: Progman에 0x052C를 보내면 최상위 WorkerW가 생기고, 거기에 SetParent.
//! - 신버전(Windows 11 24H2~, "raised desktop"): Progman이 WS_EX_NOREDIRECTIONBITMAP을 가지며
//!   SHELLDLL_DefView와 WorkerW가 모두 Progman의 자식이다. 창을 Progman의 자식으로 만들고
//!   DefView 바로 아래, WorkerW 위에 놓는다.

use windows::core::{w, BOOL};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, GetWindowLongPtrW, SendMessageTimeoutW, SetLayeredWindowAttributes, SetParent,
    SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE, HWND_BOTTOM, LWA_ALPHA, SMTO_NORMAL, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, WS_CHILD, WS_EX_LAYERED, WS_EX_NOREDIRECTIONBITMAP, WS_POPUP,
};

const SPAWN_WORKERW: u32 = 0x052C;

#[derive(Default)]
struct Found {
    worker: Option<HWND>,
}

unsafe extern "system" fn enum_top(top: HWND, lparam: LPARAM) -> BOOL {
    let found = &mut *(lparam.0 as *mut Found);
    if FindWindowExW(Some(top), None, w!("SHELLDLL_DefView"), None).is_ok() {
        // 아이콘 창을 가진 최상위 창 바로 다음의 WorkerW가 배경화면 레이어다.
        found.worker = FindWindowExW(None, Some(top), w!("WorkerW"), None).ok();
    }
    BOOL(1)
}

/// `hwnd`를 아이콘 뒤에 붙이고 화면 좌표 (x, y, w, h)에 맞춘다.
pub fn attach(hwnd: HWND, rect: (i32, i32, i32, i32)) -> Result<(), String> {
    unsafe {
        let progman = FindWindowW(w!("Progman"), None).map_err(|_| "Couldn't find the desktop window.".to_string())?;
        SendMessageTimeoutW(progman, SPAWN_WORKERW, WPARAM(0xD), LPARAM(0x1), SMTO_NORMAL, 1000, None);

        let ex = GetWindowLongPtrW(progman, GWL_EXSTYLE) as u32;
        let raised = ex & WS_EX_NOREDIRECTIONBITMAP.0 != 0;

        let place = |parent: HWND, after: Option<HWND>| -> Result<(), String> {
            let mut p = POINT { x: rect.0, y: rect.1 };
            let _ = ScreenToClient(parent, &mut p);
            SetWindowPos(hwnd, after, p.x, p.y, rect.2, rect.3, SWP_NOACTIVATE | SWP_SHOWWINDOW)
                .map_err(|e| format!("Couldn't place the window on the desktop. ({e})"))
        };

        if raised {
            let defview = FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), None).ok();
            let worker = FindWindowExW(Some(progman), None, w!("WorkerW"), None)
                .map_err(|_| "Couldn't find the desktop layer (WorkerW).".to_string())?;
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            SetWindowLongPtrW(hwnd, GWL_STYLE, ((style & !WS_POPUP.0) | WS_CHILD.0) as isize);
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex | WS_EX_LAYERED.0) as isize);
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
            SetParent(hwnd, Some(progman)).map_err(|e| format!("Couldn't attach to the desktop. ({e})"))?;
            // DefView(아이콘) 바로 아래에 둔다.
            place(progman, defview)?;
            // WorkerW(원래 배경화면)는 항상 맨 아래.
            let _ = SetWindowPos(worker, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            return Ok(());
        }

        let mut found = Found::default();
        let _ = EnumWindows(Some(enum_top), LPARAM(&mut found as *mut Found as isize));
        let worker = found.worker.ok_or_else(|| "Couldn't find the desktop layer (WorkerW).".to_string())?;
        SetParent(hwnd, Some(worker)).map_err(|e| format!("Couldn't attach to the desktop. ({e})"))?;
        place(worker, None)
    }
}
