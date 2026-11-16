//! 자동 업데이트: GitHub 릴리스의 latest.json을 확인하고, 사용자가 허락하면 내려받아 설치한다.
//!
//! - 켜고 잠시 뒤 한 번, 그 뒤로 몇 시간마다 조용히 확인한다. 새 버전이 있으면 조작 창에 안내를 띄우고
//!   트레이 메뉴 문구를 바꿀 뿐, 사용자가 "Install"을 누르기 전에는 아무것도 내려받지 않는다.
//! - 설치는 NSIS 설치 파일을 passive 모드로 실행하고 앱을 끝낸다. 설치가 끝나면 설치 파일이 앱을 다시 켠다.

use crate::actions;
use crate::state::{lock, Shared};
use crate::window::tray;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const FIRST_CHECK: Duration = Duration::from_secs(15);
const RECHECK: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Default)]
pub struct UpdateState {
    pending: Mutex<Option<Update>>,
    busy: AtomicBool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

fn info(u: &Update) -> UpdateInfo {
    UpdateInfo { version: u.version.clone(), current_version: u.current_version.clone(), notes: u.body.clone() }
}

pub fn init(app: &AppHandle) {
    app.manage(UpdateState::default());
    let handle = app.clone();
    let _ = std::thread::Builder::new().name("update-check".into()).spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            // 오프라인 등으로 실패하면 다음 차례에 다시 해 본다.
            let _ = tauri::async_runtime::block_on(check(&handle));
            std::thread::sleep(RECHECK);
        }
    });
}

pub fn pending(app: &AppHandle) -> Option<UpdateInfo> {
    let s = app.try_state::<UpdateState>()?;
    let p = lock(&s.pending);
    p.as_ref().map(info)
}

/// 새 버전이 있으면 기억해 두고 조작 창·트레이에 알린다.
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let found = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    let result = found.as_ref().map(info);
    if let Some(s) = app.try_state::<UpdateState>() {
        *lock(&s.pending) = found;
    }
    tray::set_update(app, result.as_ref().map(|u| u.version.as_str()));
    let _ = app.emit("update-available", result.clone());
    Ok(result)
}

/// 기억해 둔 업데이트를 내려받아 설치한다. 성공하면 앱이 끝나므로 돌아오지 않는다.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let s = app.try_state::<UpdateState>().ok_or("Updater isn't ready.")?;
    if s.busy.swap(true, Ordering::AcqRel) {
        return Err("The update is already being installed.".into());
    }
    let result = install_inner(app).await;
    s.busy.store(false, Ordering::Release);
    result
}

async fn install_inner(app: &AppHandle) -> Result<(), String> {
    let Some(state) = app.try_state::<Shared>() else { return Err("App isn't ready.".into()) };
    let state = state.inner().clone();
    if lock(&app.state::<UpdateState>().pending).is_none() {
        return Err("There's no update to install.".into());
    }

    // 설치 파일을 실행하기 직전, 끝낼 때와 같은 정리를 한다 (바탕화면 모드 해제·설정 저장).
    // 업데이터는 std::process::exit으로 끝내므로 RunEvent::Exit이 오지 않는다.
    let cleanup = app.clone();
    let updater = app
        .updater_builder()
        .on_before_exit(move || {
            actions::shutdown(&cleanup, &state);
            cleanup.cleanup_before_exit();
        })
        .build()
        .map_err(|e| e.to_string())?;
    // on_before_exit은 이 업데이터가 돌려준 Update에만 붙으므로 한 번 더 확인한다
    // (그 사이 더 새 버전이 나왔다면 그것을 설치한다).
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Err("The update is no longer available.".into());
    };

    let progress = app.clone();
    let mut downloaded: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.emit("update-progress", UpdateProgress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| format!("Couldn't install the update: {e}"))
}
