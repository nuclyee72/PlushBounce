//! 트레이 아이콘: 조작 창 열기 / 편집 모드 토글 / 모드 전환 / 종료.

use crate::actions;
use crate::config::Mode;
use crate::state::{lock, Shared};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

pub struct TrayItems {
    edit: CheckMenuItem<Wry>,
    mode: MenuItem<Wry>,
}

fn mode_text(mode: Mode) -> &'static str {
    match mode {
        Mode::Pet => "Switch to wallpaper mode",
        Mode::Wallpaper => "Switch to pet mode",
    }
}

pub fn build(app: &AppHandle, state: &Shared) -> tauri::Result<()> {
    let mode = lock(&state.config).display.mode;
    let open = MenuItem::with_id(app, "open", "Open controls", true, None::<&str>)?;
    let edit = CheckMenuItem::with_id(app, "edit", "Edit mode", mode == Mode::Pet, false, Some("Ctrl+Alt+P"))?;
    let mode_item = MenuItem::with_id(app, "mode", mode_text(mode), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &edit, &mode_item, &sep, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main").tooltip("PlushBounce").menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let s = state.clone();
    builder = builder.on_menu_event(move |app, event| match event.id().as_ref() {
        "open" => actions::open_control(app, &s),
        "edit" => actions::toggle_edit_mode(app, &s),
        "mode" => actions::toggle_mode(app, &s),
        "quit" => actions::quit(app, &s),
        _ => {}
    });
    let s = state.clone();
    builder = builder.on_tray_icon_event(move |tray, event| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            actions::open_control(tray.app_handle(), &s);
        }
    });
    builder.build(app)?;
    app.manage(TrayItems { edit, mode: mode_item });
    Ok(())
}

/// 메뉴의 체크·문구를 현재 상태에 맞춘다.
pub fn sync(app: &AppHandle, state: &Shared) {
    let Some(items) = app.try_state::<TrayItems>() else { return };
    let mode = lock(&state.config).display.mode;
    let edit = lock(&state.runtime).edit_mode;
    let _ = items.edit.set_checked(edit);
    let _ = items.edit.set_enabled(mode == Mode::Pet);
    let _ = items.mode.set_text(mode_text(mode));
}
