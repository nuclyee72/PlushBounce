//! WASAPI 루프백 캡처 (시스템 전체, 또는 고른 앱들의 프로세스 트리를 합친 소리).

use super::{sessions, Msg};
use crate::config::Source;
use crate::state::{lock, Shared};
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::{Duration, Instant};
use wasapi::{AudioCaptureClient, AudioClient, DeviceEnumerator, Direction, Handle, SampleType, StreamMode, WaveFormat};

/// 이 시간 동안 패킷이 없으면 무음으로 보고 0을 채워 분석 시계를 계속 돌린다.
/// 앱을 여러 개 섞을 때는, 한 앱이 이만큼 조용하면 그 앱을 기다리지 않고 나머지만 섞는다.
const SILENCE_AFTER: Duration = Duration::from_millis(30);
/// 앱 하나가 이보다 많이 앞서 쌓이면 오래된 쪽을 버린다 (지연이 쌓이지 않게).
const MAX_BACKLOG_S: f32 = 0.2;
/// 앱 소리(프로세스 루프백)는 이 형식으로 받는다.
const APP_RATE: u32 = 48000;

fn default_device_id() -> Option<String> {
    DeviceEnumerator::new().ok()?.get_default_device(Direction::Render).ok()?.get_id().ok()
}

/// 캡처 스트림 하나 (시스템 전체 또는 앱 하나).
struct Stream {
    client: AudioClient,
    capture: AudioCaptureClient,
    event: Handle,
    channels: usize,
    bytes: VecDeque<u8>,
    /// mono로 섞은, 아직 내보내지 않은 샘플.
    pending: VecDeque<f32>,
    last_data: Instant,
}

impl Stream {
    fn open(mut client: AudioClient, rate: u32, channels: u16, buffer_hns: i64) -> Result<Stream, String> {
        let es = |e: wasapi::WasapiError| e.to_string();
        // 샘플레이트·채널 수는 그대로 두고 표본 형식만 float32로 받는다.
        let format = WaveFormat::new(32, 32, SampleType::Float, rate as usize, channels as usize, None);
        let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: buffer_hns };
        client.initialize_client(&format, Direction::Capture, mode, None).map_err(es)?;
        let event = client.set_get_eventhandle().map_err(es)?;
        let capture = client.get_audiocaptureclient().map_err(es)?;
        client.start_stream().map_err(es)?;
        Ok(Stream {
            client,
            capture,
            event,
            channels: channels as usize,
            bytes: VecDeque::new(),
            pending: VecDeque::new(),
            last_data: Instant::now() - SILENCE_AFTER * 2,
        })
    }

    /// 도착한 패킷을 모두 읽어 mono로 `pending`에 덧붙인다.
    fn pull(&mut self, now: Instant) -> Result<(), String> {
        let es = |e: wasapi::WasapiError| e.to_string();
        while self.capture.get_next_packet_size().map_err(es)?.unwrap_or(0) > 0 {
            self.capture.read_from_device_to_deque(&mut self.bytes).map_err(es)?;
        }
        let frame_bytes = self.channels * 4;
        let frames = self.bytes.len() / frame_bytes;
        if frames == 0 {
            return Ok(());
        }
        let bytes = &self.bytes.make_contiguous()[..frames * frame_bytes];
        for frame in bytes.chunks_exact(frame_bytes) {
            let mut sum = 0.0f32;
            for s in frame.chunks_exact(4) {
                sum += f32::from_le_bytes([s[0], s[1], s[2], s[3]]);
            }
            self.pending.push_back(sum / self.channels as f32);
        }
        self.bytes.drain(..frames * frame_bytes);
        self.last_data = now;
        Ok(())
    }

    fn fresh(&self, now: Instant) -> bool {
        now.duration_since(self.last_data) < SILENCE_AFTER
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let _ = self.client.stop_stream();
    }
}

/// 고른 앱들 중 지금 실행 중인 것: (이름, 뿌리 pid).
fn running_apps(names: &[String]) -> Vec<(String, u32)> {
    names.iter().filter_map(|n| sessions::find_root_pid(n).map(|pid| (n.clone(), pid))).collect()
}

fn short(name: &str) -> &str {
    name.strip_suffix(".exe").or_else(|| name.strip_suffix(".EXE")).unwrap_or(name)
}

/// 캡처를 시작해 스트림이 끝나거나 다시 연결해야 할 때까지 돈다.
/// `Ok`는 소스/장치가 바뀌어 다시 시작해야 함을, `Err`는 실패를 뜻한다.
pub fn run(state: &Shared, tx: &SyncSender<Msg>, on_running: &dyn Fn()) -> Result<(), String> {
    let _ = wasapi::initialize_mta();
    let es = |e: wasapi::WasapiError| e.to_string();

    let version = state.audio_version.load(Ordering::Acquire);
    let source = lock(&state.config).audio.source.clone();

    let mut device_id: Option<String> = None;
    let mut apps: Vec<(String, u32)> = Vec::new();
    let mut streams: Vec<Stream> = Vec::new();
    let rate = match &source {
        Source::System => {
            let device = DeviceEnumerator::new().map_err(es)?.get_default_device(Direction::Render).map_err(es)?;
            device_id = device.get_id().ok();
            let client = device.get_iaudioclient().map_err(es)?;
            let mix = client.get_mixformat().map_err(es)?;
            let rate = mix.get_samplespersec();
            streams.push(Stream::open(client, rate, mix.get_nchannels(), 1_000_000)?);
            rate
        }
        Source::Apps { names } => {
            apps = running_apps(names);
            if apps.is_empty() {
                let list = names.iter().map(|n| short(n)).collect::<Vec<_>>().join(", ");
                return Err(format!("고른 앱({list})이 실행 중이 아닙니다. 앱을 켜거나 다른 소스를 고르세요."));
            }
            for (_, pid) in &apps {
                let client = AudioClient::new_application_loopback_client(*pid, true).map_err(es)?;
                streams.push(Stream::open(client, APP_RATE, 2, 0)?);
            }
            APP_RATE
        }
        Source::Process { name, .. } => {
            // normalize가 Apps로 옮기므로 여기 오지 않지만, 오면 앱 하나로 다룬다.
            apps = running_apps(std::slice::from_ref(name));
            let Some((_, pid)) = apps.first() else {
                return Err(format!("'{}'이(가) 실행 중이 아닙니다. 앱을 켜거나 다른 소스를 고르세요.", short(name)));
            };
            let client = AudioClient::new_application_loopback_client(*pid, true).map_err(es)?;
            streams.push(Stream::open(client, APP_RATE, 2, 0)?);
            APP_RATE
        }
    };
    let wanted: Vec<String> = match &source {
        Source::Apps { names } => names.clone(),
        Source::Process { name, .. } => vec![name.clone()],
        Source::System => Vec::new(),
    };

    tx.send(Msg::Format(rate)).map_err(|_| "분석 스레드가 멈췄습니다.".to_string())?;
    on_running();

    let max_backlog = (MAX_BACKLOG_S * rate as f32) as usize;
    let mut last_fed = Instant::now();
    let mut last_check = Instant::now();

    let send = |mono: Vec<f32>| -> Result<(), String> {
        match tx.try_send(Msg::Samples(mono)) {
            Ok(()) | Err(TrySendError::Full(_)) => Ok(()), // 분석이 밀리면 버린다 (지연을 쌓지 않는다).
            Err(TrySendError::Disconnected(_)) => Err("분석 스레드가 멈췄습니다.".to_string()),
        }
    };

    loop {
        // 이벤트가 오지 않아도(무음, 일부 드라이버) 깨어나 확인한다. 앱이 여럿이면 자주 본다.
        let _ = streams[0].event.wait_for_event(if streams.len() > 1 { 10 } else { 20 });
        let now = Instant::now();
        for s in &mut streams {
            s.pull(now)?;
            if s.pending.len() > max_backlog {
                let extra = s.pending.len() - max_backlog;
                s.pending.drain(..extra);
            }
        }

        // 지금 소리를 내고 있는 스트림이 모두 데이터를 줬을 때 그만큼 섞어 내보낸다.
        // 조용한 앱(패킷이 30ms 넘게 없음)은 기다리지 않는다.
        let fresh: Vec<usize> = (0..streams.len()).filter(|&i| streams[i].fresh(now)).collect();
        let n = if fresh.is_empty() {
            streams.iter().map(|s| s.pending.len()).max().unwrap_or(0)
        } else {
            fresh.iter().map(|&i| streams[i].pending.len()).min().unwrap_or(0)
        };
        if n > 0 {
            let mut mono = vec![0.0f32; n];
            for s in &mut streams {
                let k = n.min(s.pending.len());
                for (o, v) in mono.iter_mut().zip(s.pending.drain(..k)) {
                    *o += v;
                }
            }
            send(mono)?;
            last_fed = now;
        } else if fresh.is_empty() && now.duration_since(last_fed) >= SILENCE_AFTER {
            // 루프백은 아무 소리도 없으면 패킷을 주지 않는다.
            let gap = now.duration_since(last_fed).as_secs_f32().min(0.2);
            let k = (gap * rate as f32) as usize;
            if k > 0 {
                send(vec![0.0; k])?;
                last_fed = now;
            }
        }

        if state.audio_version.load(Ordering::Acquire) != version && lock(&state.config).audio.source != source {
            break;
        }
        if now.duration_since(last_check) >= Duration::from_secs(1) {
            last_check = now;
            if wanted.is_empty() {
                // 기본 출력 장치가 바뀌면 새 장치로 다시 연결한다.
                if default_device_id() != device_id {
                    break;
                }
            } else if running_apps(&wanted) != apps {
                // 고른 앱이 꺼졌거나 새로 켜졌거나 다시 켜져 pid가 바뀌었다.
                break;
            }
        }
    }
    Ok(())
}
