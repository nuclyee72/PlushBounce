//! 소리를 내는 앱 목록 (IAudioSessionManager2 세션 열거)과 프로세스 찾기.

use serde::Serialize;
use std::collections::HashMap;
use wasapi::{DeviceEnumerator, Direction};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

#[derive(Serialize, Clone, Debug)]
pub struct AudioSession {
    pub pid: u32,
    pub name: String,
    pub display: String,
}

struct Proc {
    parent: u32,
    exe: String,
}

fn process_table() -> HashMap<u32, Proc> {
    let mut map = HashMap::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return map };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                let exe = String::from_utf16_lossy(&e.szExeFile[..len]);
                map.insert(e.th32ProcessID, Proc { parent: e.th32ParentProcessID, exe });
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    map
}

/// 같은 실행 파일 이름을 가진 프로세스 트리의 뿌리 pid.
/// 브라우저처럼 자식 프로세스가 소리를 내는 앱도 트리 전체를 잡을 수 있게 한다.
fn root_pid(table: &HashMap<u32, Proc>, name: &str) -> Option<u32> {
    let same = |exe: &str| exe.eq_ignore_ascii_case(name);
    let mut roots: Vec<(u32, usize)> = table
        .iter()
        .filter(|(_, p)| same(&p.exe))
        .filter(|(_, p)| !table.get(&p.parent).is_some_and(|pp| same(&pp.exe)))
        .map(|(&pid, _)| {
            let children = table.values().filter(|c| c.parent == pid && same(&c.exe)).count();
            (pid, children)
        })
        .collect();
    // 자식이 가장 많은 쪽, 같으면 pid가 작은 쪽.
    roots.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    roots.first().map(|r| r.0)
}

pub fn find_root_pid(name: &str) -> Option<u32> {
    root_pid(&process_table(), name)
}

/// 모든 활성 출력 장치의 오디오 세션을 실행 파일 단위로 모은다.
pub fn list() -> Result<Vec<AudioSession>, String> {
    let _ = wasapi::initialize_mta();
    let table = process_table();
    let me = std::process::id();
    let mut out: Vec<AudioSession> = Vec::new();

    let enumerator = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let devices = enumerator.get_device_collection(Direction::Render).map_err(|e| e.to_string())?;
    let count = devices.get_nbr_devices().map_err(|e| e.to_string())?;
    for i in 0..count {
        let Ok(device) = devices.get_device_at_index(i) else { continue };
        let Ok(manager) = device.get_iaudiosessionmanager() else { continue };
        let Ok(sessions) = manager.get_audiosessionenumerator() else { continue };
        for j in 0..sessions.get_count().unwrap_or(0) {
            let Ok(session) = sessions.get_session(j) else { continue };
            let Ok(pid) = session.get_process_id() else { continue };
            if pid == 0 || pid == me {
                continue;
            }
            let Some(proc) = table.get(&pid) else { continue };
            if out.iter().any(|s| s.name.eq_ignore_ascii_case(&proc.exe)) {
                continue;
            }
            let shown = session.get_display_name().unwrap_or_default();
            let display = if shown.trim().is_empty() || shown.starts_with('@') {
                proc.exe.strip_suffix(".exe").or_else(|| proc.exe.strip_suffix(".EXE")).unwrap_or(&proc.exe).to_string()
            } else {
                shown
            };
            out.push(AudioSession { pid: root_pid(&table, &proc.exe).unwrap_or(pid), name: proc.exe.clone(), display });
        }
    }
    out.sort_by_key(|s| s.display.to_lowercase());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_the_top_process_with_the_same_name() {
        let mut t = HashMap::new();
        let mut add = |pid, parent, exe: &str| t.insert(pid, Proc { parent, exe: exe.to_string() });
        add(1, 0, "explorer.exe");
        add(10, 1, "chrome.exe");
        add(11, 10, "chrome.exe");
        add(12, 10, "Chrome.exe");
        add(20, 1, "chrome.exe"); // 자식 없는 다른 인스턴스
        add(30, 1, "notepad.exe");
        assert_eq!(root_pid(&t, "chrome.exe"), Some(10));
        assert_eq!(root_pid(&t, "notepad.exe"), Some(30));
        assert_eq!(root_pid(&t, "missing.exe"), None);
    }
}
