//! 바탕화면 모드: stage 창을 바탕화면 아이콘 뒤 레이어에 붙인다.
//!
//! 두 가지 구조를 모두 처리한다 (Lively Wallpaper의 WinDesktopCore와 같은 방식).
//! - 구버전: Progman에 0x052C를 보내면 최상위 WorkerW가 생기고, 거기에 SetParent.
//! - 신버전(Windows 11 24H2~, "raised desktop"): Progman이 WS_EX_NOREDIRECTIONBITMAP을 가지며
//!   SHELLDLL_DefView와 WorkerW가 모두 Progman의 자식이다. 창을 Progman의 자식으로 만들고
//!   DefView 바로 아래, WorkerW 위에 놓는다.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::{w, BOOL};
use windows::Win32::Foundation::{COLORREF, ERROR_SUCCESS, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{RedrawWindow, ScreenToClient, RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, GetWindowLongPtrW, SendMessageTimeoutW, SetLayeredWindowAttributes, SetParent,
    SetWindowLongPtrW, SetWindowPos, SystemParametersInfoW, GWL_EXSTYLE, GWL_STYLE, HWND_BOTTOM, LWA_ALPHA, SMTO_NORMAL,
    SPIF_SENDCHANGE, SPI_GETDESKWALLPAPER, SPI_SETDESKWALLPAPER, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WS_CHILD, WS_EX_LAYERED, WS_EX_NOREDIRECTIONBITMAP, WS_POPUP,
};

/// 지금 stage 창이 바탕화면 레이어에 붙어 있는가.
static ATTACHED: AtomicBool = AtomicBool::new(false);

/// 붙어 있는 동안 남겨 두는 표시 파일. 앱이 제대로 끝나지 못하면(강제 종료·오류) 다음 실행 때 이것을 보고
/// 바탕화면을 되살린다.
fn marker(config_file: &Path) -> std::path::PathBuf {
    config_file.with_file_name("wallpaper-attached")
}

/// stage 창을 바탕화면 레이어에 붙였다.
pub fn mark_attached(config_file: &Path) {
    ATTACHED.store(true, Ordering::Release);
    let _ = std::fs::write(marker(config_file), b"1");
}

/// 바탕화면 레이어에서 창을 뗐다(창을 없앴다): 바탕화면을 다시 그리게 하고 표시를 지운다.
/// 붙어 있지 않았으면 아무것도 하지 않는다.
pub fn detached(config_file: &Path) {
    if ATTACHED.swap(false, Ordering::AcqRel) {
        restore_desktop();
        let _ = std::fs::remove_file(marker(config_file));
    }
}

/// 앱을 시작할 때: 지난번에 붙은 채로 끝났다면 바탕화면을 되살린다.
pub fn recover_after_unclean_exit(config_file: &Path) {
    if marker(config_file).exists() {
        restore_desktop();
        let _ = std::fs::remove_file(marker(config_file));
    }
}

/// 윈도우 배경이 "그림"(슬라이드쇼·Spotlight·단색이 아님)인가.
fn picture_background() -> bool {
    let mut kind: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Wallpapers"),
            w!("BackgroundType"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut kind as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    // 값이 없으면(예전 Windows) 그림으로 본다.
    r != ERROR_SUCCESS || kind == 0
}

/// 바탕화면 레이어에 창을 붙였다 떼면 Windows가 배경을 다시 그리지 않아 검게 남을 수 있다.
/// 지금 쓰는 배경 그림 파일을 그대로 다시 지정해 다시 그리게 한다 (설정은 바뀌지 않는다).
/// 슬라이드쇼·Spotlight처럼 그림 하나가 아니면 설정을 건드리지 않도록 다시 그리기만 요청한다.
pub fn restore_desktop() {
    unsafe {
        let mut buf = [0u16; 1024];
        let got = SystemParametersInfoW(SPI_GETDESKWALLPAPER, buf.len() as u32, Some(buf.as_mut_ptr() as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
        let path = String::from_utf16_lossy(&buf[..len]);
        if got.is_ok() && len > 0 && Path::new(&path).is_file() && picture_background() {
            let _ = SystemParametersInfoW(SPI_SETDESKWALLPAPER, 0, Some(buf.as_mut_ptr() as *mut _), SPIF_SENDCHANGE);
        }
        if let Ok(progman) = FindWindowW(w!("Progman"), None) {
            let _ = RedrawWindow(Some(progman), None, None, RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN | RDW_UPDATENOW);
        }
    }
}

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
