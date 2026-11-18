//! macOS: 소리를 내는 앱 목록 (Core Audio 프로세스 객체 열거).
//!
//! 앱은 실행 파일 경로에서 가장 바깥쪽 `.app` 번들 이름으로 묶는다. 브라우저처럼 소리를 내는 도우미
//! 프로세스가 번들 안쪽에 있는 앱(`Google Chrome.app/…/Google Chrome Helper.app`)도 한 앱이 된다.
//! 번들 밖의 시스템 프로세스(예: Safari가 쓰는 WebKit GPU 프로세스)는 목록에 나오지 않는다.

use objc2_core_audio::{
    kAudioHardwarePropertyProcessObjectList, kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal,
    kAudioObjectSystemObject, kAudioProcessPropertyPID, AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize,
    AudioObjectID, AudioObjectPropertyAddress,
};
use serde::Serialize;
use std::ffi::c_void;
use std::ptr::NonNull;

#[derive(Serialize, Clone, Debug)]
pub struct AudioSession {
    pub pid: u32,
    pub name: String,
    pub display: String,
}

/// Core Audio에 오디오 클라이언트를 가진 프로세스 하나.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AudioProcess {
    pub object: AudioObjectID,
    pub pid: i32,
    /// 이 프로세스가 속한 앱(바깥쪽 `.app` 번들 이름). 번들 밖이면 `None`.
    pub app: Option<String>,
}

fn address(selector: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress { mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain }
}

/// 고정 크기 속성 하나를 읽는다. `T`는 0으로 채워도 되는 C 값(정수, 포인터 크기 정수, C 구조체)이어야 한다.
pub(crate) fn get_prop<T: Copy>(object: AudioObjectID, selector: u32) -> Option<T> {
    let addr = address(selector);
    let mut value: T = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<T>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast::<c_void>(),
        )
    };
    (status == 0).then_some(value)
}

/// 배열 속성(AudioObjectID 목록 등)을 읽는다.
pub(crate) fn get_list(object: AudioObjectID, selector: u32) -> Vec<AudioObjectID> {
    let addr = address(selector);
    let mut size: u32 = 0;
    let status = unsafe { AudioObjectGetPropertyDataSize(object, NonNull::from(&addr), 0, std::ptr::null(), NonNull::from(&mut size)) };
    if status != 0 || size == 0 {
        return Vec::new();
    }
    let mut out = vec![0 as AudioObjectID; size as usize / std::mem::size_of::<AudioObjectID>()];
    if out.is_empty() {
        return out;
    }
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::new(out.as_mut_ptr()).unwrap().cast::<c_void>(),
        )
    };
    if status != 0 {
        return Vec::new();
    }
    out.truncate(size as usize / std::mem::size_of::<AudioObjectID>());
    out
}

fn exe_path(pid: i32) -> Option<String> {
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) };
    if n <= 0 {
        return None;
    }
    buf.truncate(n as usize);
    String::from_utf8(buf).ok()
}

/// 실행 파일 경로에서 가장 바깥쪽 `.app` 번들 이름.
fn app_of(path: &str) -> Option<String> {
    path.split('/')
        .find_map(|part| part.strip_suffix(".app").filter(|n| !n.is_empty()))
        .map(str::to_string)
}

/// 지금 Core Audio에 붙어 있는 모든 프로세스.
pub fn processes() -> Vec<AudioProcess> {
    get_list(kAudioObjectSystemObject as AudioObjectID, kAudioHardwarePropertyProcessObjectList)
        .into_iter()
        .filter_map(|object| {
            let pid: i32 = get_prop(object, kAudioProcessPropertyPID)?;
            let app = exe_path(pid).as_deref().and_then(app_of);
            Some(AudioProcess { object, pid, app })
        })
        .collect()
}

/// 고른 앱들(번들 이름)에 속한 프로세스 객체들. 정렬되어 있어 바뀌었는지 비교할 수 있다.
pub fn objects_for(names: &[String]) -> Vec<AudioObjectID> {
    let mut out: Vec<AudioObjectID> = processes()
        .into_iter()
        .filter(|p| p.app.as_deref().is_some_and(|a| names.iter().any(|n| n.eq_ignore_ascii_case(a))))
        .map(|p| p.object)
        .collect();
    out.sort_unstable();
    out
}

/// 오디오 클라이언트를 가진 앱 목록 (앱 이름 단위로 묶음).
pub fn list() -> Result<Vec<AudioSession>, String> {
    let me = std::process::id() as i32;
    let own_app = exe_path(me).as_deref().and_then(app_of);
    let mut out: Vec<AudioSession> = Vec::new();
    for p in processes() {
        if p.pid == me {
            continue;
        }
        let Some(app) = p.app else { continue };
        if own_app.as_deref().is_some_and(|o| o.eq_ignore_ascii_case(&app)) {
            continue;
        }
        if out.iter().any(|s| s.name.eq_ignore_ascii_case(&app)) {
            continue;
        }
        out.push(AudioSession { pid: p.pid as u32, display: app.clone(), name: app });
    }
    out.sort_by_key(|s| s.display.to_lowercase());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_is_the_outermost_bundle() {
        assert_eq!(
            app_of("/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Helpers/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper").as_deref(),
            Some("Google Chrome")
        );
        assert_eq!(app_of("/Applications/Spotify.app/Contents/MacOS/Spotify").as_deref(), Some("Spotify"));
        assert_eq!(app_of("/usr/libexec/coreaudiod"), None);
        assert_eq!(app_of("/weird/.app/x"), None);
    }
}
