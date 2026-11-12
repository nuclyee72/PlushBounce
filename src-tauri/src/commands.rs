//! 프런트에서 부르는 command들.

use crate::actions;
use crate::audio::sessions::{self, AudioSession};
use crate::config::{Background, Config};
use crate::images;
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
    std::thread::spawn(sessions::list).join().map_err(|_| "앱 목록을 읽지 못했습니다".to_string())?
}

#[tauri::command]
pub async fn add_dolls(app: AppHandle, state: S<'_>, paths: Vec<String>) -> Result<Config, String> {
    let (cfg, failed) = actions::add_dolls(&app, &state, &paths);
    if failed > 0 {
        return Err(format!("이미지 {failed}개를 열지 못했습니다. PNG, JPG, WebP, BMP, GIF 파일을 넣어 주세요."));
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

/// 배경 이미지를 images 폴더로 복사하고 배경으로 지정한다.
#[tauri::command]
pub async fn set_background_image(app: AppHandle, state: S<'_>, path: String) -> Result<Config, String> {
    let src = PathBuf::from(&path);
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    // 배경은 WebView가 직접 그리므로 브라우저가 여는 형식만 받는다.
    if !["png", "jpg", "jpeg", "jfif", "webp", "gif", "bmp", "avif", "ico"].contains(&ext.as_str()) {
        return Err("배경으로 쓸 수 없는 파일입니다. PNG, JPG, WebP, BMP 이미지를 골라 주세요.".into());
    }
    let file = format!("bg-{}.{ext}", images::new_id());
    std::fs::create_dir_all(&state.images_dir).map_err(|e| e.to_string())?;
    std::fs::copy(&src, state.images_dir.join(&file)).map_err(|e| format!("배경 이미지를 복사하지 못했습니다. ({e})"))?;
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
    state.config().save(Path::new(&path)).map_err(|e| format!("프리셋을 저장하지 못했습니다. ({e})"))
}

/// 프리셋(전체 설정 JSON)을 불러온다. 이 PC에 이미지가 없는 인형은 뺀다.
#[tauri::command]
pub async fn import_preset(app: AppHandle, state: S<'_>, path: String) -> Result<Config, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("프리셋 파일을 읽지 못했습니다. ({e})"))?;
    let mut cfg: Config = serde_json::from_str(&text).map_err(|_| "프리셋 파일 형식이 맞지 않습니다.".to_string())?;
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
