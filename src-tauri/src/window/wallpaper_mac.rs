//! macOS: 바탕화면 모드와 펫 창의 AppKit 설정.
//!
//! 바탕화면 모드는 stage 창의 레벨을 바탕화면 레벨(kCGDesktopWindowLevel)로 내린다. 바탕화면 아이콘은
//! 그보다 위 레벨이라 인형이 아이콘 뒤, 배경화면 앞에 그려진다. Windows처럼 다른 창에 붙이는 게 아니라서
//! 떼어 낼 때 바탕화면을 되살릴 일이 없다. 그래서 표시 파일(mark/detach/recover)은 아무것도 하지 않는다.

use objc2::msg_send;
use objc2::runtime::AnyObject;
use std::path::Path;
use tauri::WebviewWindow;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowLevelForKey(key: i32) -> i32;
}
const K_CG_DESKTOP_WINDOW_LEVEL_KEY: i32 = 2;

// NSWindowCollectionBehavior
const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
const STATIONARY: usize = 1 << 4;
const IGNORES_CYCLE: usize = 1 << 6;
const FULL_SCREEN_AUXILIARY: usize = 1 << 8;

pub fn mark_attached(_config_file: &Path) {}

pub fn detached(_config_file: &Path) {}

pub fn recover_after_unclean_exit(_config_file: &Path) {}

/// AppKit 호출은 메인 스레드에서 해야 한다. 창 포인터는 스레드를 넘길 수 없어서 정수로 옮긴다.
fn on_ns_window(win: &WebviewWindow, f: impl FnOnce(&AnyObject) + Send + 'static) -> Result<(), String> {
    let ns = win.ns_window().map_err(|e| e.to_string())? as usize;
    // 창이 그 사이 닫혀도 포인터가 살아 있도록 창 핸들을 함께 붙잡아 둔다.
    let keep = win.clone();
    win.run_on_main_thread(move || {
        let _keep = keep;
        if let Some(obj) = unsafe { (ns as *const AnyObject).as_ref() } {
            f(obj);
        }
    })
    .map_err(|e| e.to_string())
}

/// stage 창을 바탕화면 레벨로 내리고 보여 준다. 모든 데스크톱(Space)에 나오고, 마우스는 통과한다.
pub fn attach(win: &WebviewWindow) -> Result<(), String> {
    on_ns_window(win, |w| unsafe {
        let level = CGWindowLevelForKey(K_CG_DESKTOP_WINDOW_LEVEL_KEY) as isize;
        let _: () = msg_send![w, setLevel: level];
        let _: () = msg_send![w, setCollectionBehavior: CAN_JOIN_ALL_SPACES | STATIONARY | IGNORES_CYCLE];
        let _: () = msg_send![w, setIgnoresMouseEvents: true];
        // 포커스를 뺏지 않고 띄운다.
        let _: () = msg_send![w, orderFrontRegardless];
    })
}

/// 펫 창: 모든 데스크톱과 전체 화면 앱 위에도 나오게 하고, Cmd+` 창 순환에서 뺀다.
pub fn float_everywhere(win: &WebviewWindow) {
    let _ = on_ns_window(win, |w| unsafe {
        let _: () = msg_send![w, setCollectionBehavior: CAN_JOIN_ALL_SPACES | STATIONARY | IGNORES_CYCLE | FULL_SCREEN_AUXILIARY];
    });
}
