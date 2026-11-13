//! 오디오 파이프라인: [캡처 스레드] → 채널 → [분석 스레드] → "audio-frame" 이벤트.

pub mod analysis;
pub mod capture;
pub mod sessions;

use crate::config::Bands;
use crate::state::{lock, Shared};
use analysis::{Analyzer, FrameOut, N_BANDS};
use serde::Serialize;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{sync_channel, Receiver};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub enum Msg {
    /// 새 스트림이 시작됨 (샘플레이트).
    Format(u32),
    Samples(Vec<f32>),
}

#[derive(Serialize, Clone, Copy, Default)]
struct BandFrame {
    energy: f32,
    onset: bool,
    strength: f32,
    active: bool,
}

#[derive(Serialize, Clone, Copy)]
struct TempoFrame {
    bpm: f32,
    beat: bool,
}

#[derive(Serialize, Clone)]
struct AudioFrame {
    t: f64,
    bands: Bands<BandFrame>,
    tempo: Option<TempoFrame>,
    silent: bool,
    /// 입력 크기 (dBFS). 역치와 같은 눈금이다.
    level: f32,
    /// 지금 쓰는 최소 온셋 간격(ms). 간격 자동이면 박에 따라 바뀐다.
    #[serde(rename = "gapMs")]
    gap_ms: f32,
}

/// 이벤트 한 번에 묶여 나가는 분석 프레임들. 온셋과 박은 OR로 합친다.
#[derive(Default)]
struct Merged {
    count: usize,
    bands: [BandFrame; N_BANDS],
    bpm: Option<f32>,
    beat: bool,
    silent: bool,
    urgent: bool,
    rms: f32,
    gap_ms: f32,
}

impl Merged {
    fn add(&mut self, f: &FrameOut) {
        self.count += 1;
        for (m, b) in self.bands.iter_mut().zip(f.bands.iter()) {
            m.energy = b.energy;
            m.active = b.active;
            if b.onset {
                m.onset = true;
                m.strength = m.strength.max(b.strength);
                self.urgent = true;
            }
        }
        self.bpm = f.bpm;
        self.beat |= f.beat;
        self.urgent |= f.beat;
        self.silent = f.silent;
        self.rms = self.rms.max(f.rms);
        self.gap_ms = f.gap_ms;
    }

    fn take(&mut self, t: f64) -> AudioFrame {
        let b = self.bands;
        let frame = AudioFrame {
            t,
            bands: Bands { kick: b[0], bass: b[1], mid: b[2], high: b[3], full: b[4] },
            tempo: self.bpm.map(|bpm| TempoFrame { bpm, beat: self.beat }),
            silent: self.silent,
            level: (20.0 * self.rms.max(1e-5).log10()).clamp(-100.0, 0.0),
            gap_ms: self.gap_ms,
        };
        *self = Merged::default();
        frame
    }
}

fn panic_text(p: Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "unknown error".into())
}

fn set_error(app: &AppHandle, state: &Shared, error: Option<String>) {
    if lock(&state.runtime).audio_error == error {
        return;
    }
    state.update_runtime(app, |r| r.audio_error = error);
}

pub fn start(app: AppHandle, state: Shared) {
    let (tx, rx) = sync_channel::<Msg>(256);

    {
        let (app, state) = (app.clone(), state.clone());
        let _ = std::thread::Builder::new().name("audio-analysis".into()).spawn(move || loop {
            // 분석이 panic해도 앱은 살아 있고, 상태를 표시한 뒤 다시 시작한다.
            match catch_unwind(AssertUnwindSafe(|| analysis_loop(&app, &state, &rx))) {
                Ok(()) => return, // 캡처 쪽이 사라짐 = 앱 종료
                Err(p) => set_error(&app, &state, Some(format!("Sound analysis stopped; restarting: {}", panic_text(p)))),
            }
            std::thread::sleep(Duration::from_millis(500));
        });
    }

    let _ = std::thread::Builder::new().name("audio-capture".into()).spawn(move || loop {
        let running = || set_error(&app, &state, None);
        let result = catch_unwind(AssertUnwindSafe(|| capture::run(&state, &tx, &running)));
        match result {
            Ok(Ok(())) => continue, // 소스·장치 변경: 바로 다시 연결
            Ok(Err(e)) => {
                let msg = if e.contains("not running") { e } else { format!("Couldn't capture sound. Check your output device. ({e})") };
                set_error(&app, &state, Some(msg));
            }
            Err(p) => set_error(&app, &state, Some(format!("Sound capture stopped; restarting: {}", panic_text(p)))),
        }
        std::thread::sleep(Duration::from_millis(1000));
    });
}

/// 진단용: 환경 변수 `PLUSHBOUNCE_AUDIO_LOG`에 파일 경로를 주면 1초마다
/// 대역별 온셋 수·박 수·BPM을 한 줄씩 적는다.
struct DebugLog {
    file: std::fs::File,
    since: Instant,
    onsets: [u32; N_BANDS],
    beats: u32,
    bpm: Option<f32>,
    silent: bool,
}

impl DebugLog {
    fn from_env() -> Option<Self> {
        let path = std::env::var_os("PLUSHBOUNCE_AUDIO_LOG")?;
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok()?;
        Some(DebugLog { file, since: Instant::now(), onsets: [0; N_BANDS], beats: 0, bpm: None, silent: true })
    }

    fn add(&mut self, frames: &[FrameOut]) {
        use std::io::Write;
        for f in frames {
            for (n, b) in self.onsets.iter_mut().zip(f.bands.iter()) {
                *n += b.onset as u32;
            }
            self.beats += f.beat as u32;
            self.bpm = f.bpm;
            self.silent = f.silent;
        }
        if self.since.elapsed() >= Duration::from_secs(1) {
            let o = self.onsets;
            let _ = writeln!(
                self.file,
                "kick={} bass={} mid={} high={} full={} beats={} bpm={} silent={}",
                o[0], o[1], o[2], o[3], o[4], self.beats, self.bpm.map(|b| format!("{b:.1}")).unwrap_or("-".into()), self.silent
            );
            self.since = Instant::now();
            self.onsets = [0; N_BANDS];
            self.beats = 0;
        }
    }
}

fn analysis_loop(app: &AppHandle, state: &Shared, rx: &Receiver<Msg>) {
    let started = Instant::now();
    let mut analyzer: Option<Analyzer> = None;
    let mut version = u64::MAX;
    let mut frames: Vec<FrameOut> = Vec::with_capacity(16);
    let mut merged = Merged::default();
    let mut last_emit = Instant::now();
    let mut last_silent = false;
    let mut debug = DebugLog::from_env();

    while let Ok(msg) = rx.recv() {
        let samples = match msg {
            Msg::Format(rate) => {
                version = state.audio_version.load(Ordering::Acquire);
                analyzer = Some(Analyzer::new(rate, lock(&state.config).analysis_params()));
                continue;
            }
            Msg::Samples(s) => s,
        };
        let Some(a) = analyzer.as_mut() else { continue };
        let v = state.audio_version.load(Ordering::Acquire);
        if v != version {
            version = v;
            a.set_params(lock(&state.config).analysis_params());
        }
        frames.clear();
        a.push(&samples, &mut frames);
        for f in &frames {
            merged.add(f);
        }
        if let Some(d) = debug.as_mut() {
            d.add(&frames);
        }
        if merged.count == 0 {
            continue;
        }
        // 평소 약 60Hz. 온셋·박은 기다리지 않고 바로 보내고, 무음이 이어지면 10Hz로 낮춘다.
        let gap = if merged.urgent {
            Duration::from_millis(4)
        } else if merged.silent && last_silent {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(15)
        };
        if last_emit.elapsed() < gap {
            continue;
        }
        last_emit = Instant::now();
        let frame = merged.take(started.elapsed().as_secs_f64() * 1000.0);
        last_silent = frame.silent;
        let stage = lock(&state.stage_label).clone();
        if !stage.is_empty() {
            let _ = app.emit_to(stage.as_str(), "audio-frame", &frame);
        }
        if state.control_visible.load(Ordering::Relaxed) {
            let _ = app.emit_to("control", "audio-frame", &frame);
        }
    }
}
