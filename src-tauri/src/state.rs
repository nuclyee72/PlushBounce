//! 앱 전역 상태. 설정의 단일 원본은 여기의 `config`다.

use crate::config::Config;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub edit_mode: bool,
    pub audio_error: Option<String>,
    pub display_error: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct ConfigChanged {
    pub config: Config,
    pub origin: String,
}

pub struct AppState {
    pub config: Mutex<Config>,
    pub runtime: Mutex<Runtime>,
    /// 오디오 관련 설정이 바뀔 때마다 증가한다.
    pub audio_version: AtomicU64,
    /// 저장해야 할 변경이 있는가.
    pub dirty: AtomicBool,
    pub control_visible: AtomicBool,
    pub stage_seq: AtomicU32,
    pub stage_label: Mutex<String>,
    /// 선택한 모니터의 작업 영역 아래쪽 끝 / 화면 높이.
    pub work_bottom: Mutex<f32>,
    pub config_file: PathBuf,
    pub images_dir: PathBuf,
}

pub type Shared = Arc<AppState>;

/// 다른 스레드가 panic해도 잠금은 계속 쓸 수 있게 한다.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn config(&self) -> Config {
        lock(&self.config).clone()
    }

    pub fn runtime(&self) -> Runtime {
        lock(&self.runtime).clone()
    }

    pub fn update_runtime(&self, app: &AppHandle, f: impl FnOnce(&mut Runtime)) {
        let snapshot = {
            let mut r = lock(&self.runtime);
            f(&mut r);
            r.clone()
        };
        let _ = app.emit("runtime-changed", snapshot);
    }

    pub fn broadcast_config(&self, app: &AppHandle, origin: &str) {
        self.dirty.store(true, Ordering::Release);
        let _ = app.emit("config-changed", ConfigChanged { config: self.config(), origin: origin.to_string() });
    }

    pub fn save_now(&self) {
        self.dirty.store(false, Ordering::Release);
        let cfg = self.config();
        if let Err(e) = cfg.save(&self.config_file) {
            eprintln!("config save failed: {e}");
        }
    }
}
