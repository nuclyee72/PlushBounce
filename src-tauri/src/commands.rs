//! 프런트에서 부르는 command들.

use crate::actions;
use crate::audio::sessions::{self, AudioSession};
use crate::config::{Background, Config};
use crate::images;
use crate::updater;
use crate::state::{Runtime, Shared};
use crate::window::stage::{self, MonitorInfo};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State, WebviewWindow};

type S<'a> = State<'a, Shared>;

#[tauri::command]
pub fn get_config(state: S) -> Config {
    state.config()
}

#[tauri::command]
pub async fn set_config(app: AppHandle, window: WebviewWindow, state: S<'_>, config: Config) -> Result<Config, String> {
    Ok(actions::apply_config(&app, &state, config, window.label()))
}

#[tauri::command]
pub fn get_runtime(state: S) -> Runtime {
    state.runtime()
}

#[tauri::command]
pub fn images_dir(state: S) -> String {
    state.images_dir.to_string_lossy().into_owned()
}

#[tauri::command]
pub fn list_monitors(app: AppHandle) -> Vec<MonitorInfo> {
    stage::monitor_infos(&app)
}

#[tauri::command]
pub async fn list_audio_sessions() -> Result<Vec<AudioSession>, String> {
    // COM을 쓰므로 전용 스레드에서 돈다.
    std::thread::spawn(sessions::list).join().map_err(|_| "Couldn't read the list of apps.".to_string())?
}

#[tauri::command]
pub async fn add_dolls(app: AppHandle, state: S<'_>, paths: Vec<String>) -> Result<Config, String> {
    let (cfg, failed) = actions::add_dolls(&app, &state, &paths);
    if failed > 0 {
        return Err(format!("Couldn't open {failed} image(s). Please add PNG, JPG, WebP, BMP or GIF files."));
    }
    Ok(cfg)
}

fn remove_image(dir: &Path, file: &str) {
    // 샘플과 경로가 섞인 이름은 지우지 않는다.
    if !file.is_empty() && !file.contains(['/', '\\']) && !file.starts_with("sample-") {
        let _ = std::fs::remove_file(dir.join(file));
    }
}

#[tauri::command]
pub async fn remove_doll(app: AppHandle, state: S<'_>, id: String) -> Result<Config, String> {
    let mut cfg = state.config();
    if let Some(i) = cfg.dolls.iter().position(|d| d.id == id) {
        let doll = cfg.dolls.remove(i);
        if !cfg.dolls.iter().any(|d| d.image == doll.image) {
            remove_image(&state.images_dir, &doll.image);
        }
    }
    Ok(actions::apply_config(&app, &state, cfg, "rust"))
}

/// 인형의 이미지만 새 파일로 바꾼다. 자리·크기·대역 같은 설정은 그대로 둔다.
#[tauri::command]
pub async fn replace_doll_image(app: AppHandle, state: S<'_>, id: String, path: String) -> Result<Config, String> {
    if !state.config().dolls.iter().any(|d| d.id == id) {
        return Err("That doll no longer exists.".into());
    }
    let (image, aspect) = images::import(Path::new(&path), &state.images_dir, &images::new_id())
        .map_err(|_| "Couldn't open that image. Please choose a PNG, JPG, WebP, BMP or GIF file.".to_string())?;
    // 가져오는 사이 설정이 바뀌었을 수 있으므로 지금 설정에서 다시 찾는다.
    let mut cfg = state.config();
    let Some(doll) = cfg.dolls.iter_mut().find(|d| d.id == id) else {
        remove_image(&state.images_dir, &image);
        return Err("That doll no longer exists.".into());
    };
    let old = std::mem::replace(&mut doll.image, image);
    doll.aspect = aspect;
    if !cfg.dolls.iter().any(|d| d.image == old) {
        remove_image(&state.images_dir, &old);
    }
    Ok(actions::apply_config(&app, &state, cfg, "rust"))
}

/// 배경 이미지를 images 폴더로 복사하고 배경으로 지정한다.
#[tauri::command]
pub async fn set_background_image(app: AppHandle, state: S<'_>, path: String) -> Result<Config, String> {
    let src = PathBuf::from(&path);
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    // 배경은 WebView가 직접 그리므로 브라우저가 여는 형식만 받는다.
    if !["png", "jpg", "jpeg", "jfif", "webp", "gif", "bmp", "avif", "ico"].contains(&ext.as_str()) {
        return Err("This file can't be used as a background. Please choose a PNG, JPG, WebP or BMP image.".into());
    }
    let file = format!("bg-{}.{ext}", images::new_id());
    std::fs::create_dir_all(&state.images_dir).map_err(|e| e.to_string())?;
    std::fs::copy(&src, state.images_dir.join(&file)).map_err(|e| format!("Couldn't copy the background image. ({e})"))?;
    let mut cfg = state.config();
    if let Background::Image { path: old } = &cfg.display.background {
        if old.starts_with("bg-") {
            remove_image(&state.images_dir, old);
        }
    }
    cfg.display.background = Background::Image { path: file };
    Ok(actions::apply_config(&app, &state, cfg, "rust"))
}

#[tauri::command]
pub async fn set_edit_mode(app: AppHandle, state: S<'_>, on: bool) -> Result<(), String> {
    actions::set_edit_mode(&app, &state, on);
    Ok(())
}

#[tauri::command]
pub async fn export_preset(state: S<'_>, path: String) -> Result<(), String> {
    state.config().save(Path::new(&path)).map_err(|e| format!("Couldn't save the preset. ({e})"))
}

/// 프리셋(전체 설정 JSON)을 불러온다. 이 PC에 이미지가 없는 인형은 뺀다.
#[tauri::command]
pub async fn import_preset(app: AppHandle, state: S<'_>, path: String) -> Result<Config, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("Couldn't read the preset file. ({e})"))?;
    let mut cfg: Config = serde_json::from_str(&text).map_err(|_| "The preset file format is not valid.".to_string())?;
    let dir = state.images_dir.clone();
    let exists = |file: &str| !file.contains(['/', '\\']) && dir.join(file).is_file();
    cfg.dolls.retain(|d| exists(&d.image));
    if let Background::Image { path } = &cfg.display.background {
        if !exists(path) {
            cfg.display.background = Background::None;
        }
    }
    // 현재 모드와 다르면 apply_config가 stage 창을 다시 만든다.
    Ok(actions::apply_config(&app, &state, cfg, "rust"))
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub fn get_update(app: AppHandle) -> Option<updater::UpdateInfo> {
    updater::pending(&app)
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<updater::UpdateInfo>, String> {
    updater::check(&app).await.map_err(|e| format!("Couldn't check for updates: {e}"))
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    updater::install(&app).await
}
