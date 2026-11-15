//! 오디오 분석 (순수 로직, I/O 없음).
//!
//! mono f32 샘플을 넣으면 hop마다 대역별 에너지·온셋과 템포/박 정보를 낸다.

use realfft::{num_complex::Complex, RealFftPlanner, RealToComplex};
use std::collections::VecDeque;
use std::sync::Arc;

pub const WINDOW: usize = 1024;
pub const HOP: usize = 512;
pub const N_BANDS: usize = 5;
/// 대역 순서: kick, bass, mid, high, full
pub const FULL: usize = 4;

/// 프레임 RMS가 이보다 작으면 무음.
const SILENCE_RMS: f32 = 2e-4;
/// 역치 자동: 최근 이만큼(초)의 소리 크기를 본다.
const AUTO_GATE_WINDOW_S: f32 = 8.0;
/// 역치 자동: 최근 큰 소리(95번째 백분위)보다 이만큼(dB) 아래를 역치로 한다.
const AUTO_GATE_BELOW_DB: f32 = 30.0;
/// 크기가 거의 변하지 않는(위아래 차이가 이보다 작은) 소리는 음악이 아니라 잡음으로 보고 그 위에 역치를 둔다.
const AUTO_GATE_STEADY_DB: f32 = 6.0;
const AUTO_GATE_MIN_DB: f32 = -80.0;
const AUTO_GATE_MAX_DB: f32 = -24.0;
/// 이보다 작은 프레임(디지털 무음)은 역치 자동의 기록에 넣지 않는다.
const AUTO_GATE_IGNORE_DB: f32 = -95.0;
/// 스펙트럼 정규화 기준의 하한 (아주 작은 잡음을 크게 키우지 않도록).
const LEVEL_FLOOR: f32 = 1e-3;
/// log 압축 세기: ln(1 + LOG_GAIN · mag / level)
const LOG_GAIN: f32 = 30.0;
/// 온셋 임계의 고정 항 δ.
const ONSET_DELTA: f32 = 0.12;
/// 이웃 bin 최대값의 이 비율까지는 번진 것으로 본다.
const LEAK: f32 = 0.3;
/// 킥↔베이스: 이웃 대역 에너지가 이 배수보다 크면 번진 온셋으로 본다.
const NEIGHBOUR_RATIO: f32 = 8.0;
/// `active` 판정 창. 곡이 바뀌면 이 시간 안에 새 곡 기준으로 바뀐다.
/// 대역의 소리 크기(그 대역의 최근 최대 대비 0..1)가 이보다 작으면 온셋으로 치지 않는다.
/// 거의 소리가 없는 대역에서 잡음이 살짝 튄 것을 거른다 ("음이 있는 것 같은 곳에만").
const MIN_BAND_LEVEL: f32 = 0.12;
/// 최근 센 타격 대비 세기가 이보다 약한 온셋은 버린다 (뚜렷한 타격·음의 시작만).
const MIN_STRENGTH: f32 = 0.3;
/// 박이 잡혀 있을 때, 반박 격자에서 이만큼(박 단위) 넘게 벗어난 온셋은 OFF_GRID_STRENGTH 이상 세야 받는다.
const GRID_TOLERANCE: f64 = 0.13;
const OFF_GRID_STRENGTH: f32 = 0.52;
/// 박 위치의 큰 차이를 믿기까지 필요한 연속 추정 수(0.5초마다), 한 번에 옮기는 최대량(박 단위).
const PHASE_VOTES: u32 = 3;
const PHASE_MAX_STEP: f64 = 0.15;
/// 간격 자동: 박 간격의 이만큼을 최소 온셋 간격으로 (0.22박 ≈ 16분음표보다 조금 짧게 → 16분음표까지는 잡고 한 타격의 겹침은 거른다).
const AUTO_GAP_BEATS: f32 = 0.22;
const AUTO_GAP_MIN_MS: f32 = 70.0;
const AUTO_GAP_MAX_MS: f32 = 220.0;
/// 간격 자동인데 박을 아직 모를 때.
const AUTO_GAP_FALLBACK_MS: f32 = 120.0;
const ACTIVE_WINDOW_S: f32 = 5.0;
const ACTIVE_MIN_ONSETS: usize = 3;
/// 템포 추정에 쓰는 최근 구간(초). 길수록 안정적이지만 곡이 바뀐 것을 늦게 안다.
const TEMPO_WINDOW_S: f32 = 5.0;
/// 첫 추정까지 필요한 구간(초).
const TEMPO_WARMUP_S: f32 = 3.0;
/// 다른 템포로 갈아타려면 연속으로 이만큼(0.5초마다 한 번) 같은 추정이 나와야 한다.
const TEMPO_SWITCH_COUNT: u32 = 4;
/// 지금 템포와 배수 관계(2배·절반·2:3)인 값으로는 이만큼 연속(0.5초마다) 나와야 갈아탄다.
const TEMPO_SWITCH_RELATED: u32 = 8;
/// 이만큼(초) 무음이면 템포를 잊는다.
const TEMPO_FORGET_SILENT_S: f32 = 1.2;

/// 템포 사전 가중: 이 BPM 중심, log2 폭.
const TEMPO_PRIOR_BPM: f32 = 120.0;
const TEMPO_PRIOR_WIDTH: f32 = 0.55;
/// 절반 간격(두 배 빠른 템포)의 자기상관이 이 비율 이상이면 빠른 쪽을 박으로 본다.
const HALF_LAG_RATIO: f32 = 0.4;
/// 2/3 간격(1.5배 빠른 템포)의 자기상관이 이 비율 이상이면 빠른 쪽을 박으로 본다.
const THIRDS_LAG_RATIO: f32 = 0.6;
const BPM_MIN: f32 = 70.0;
const BPM_MAX: f32 = 180.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandParams {
    pub lo: f32,
    pub hi: f32,
    /// 임계 배수 k. 클수록 둔감하다.
    pub sensitivity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    pub bands: [BandParams; N_BANDS],
    pub min_onset_interval_ms: f32,
    /// 간격 자동: 박이 잡혀 있으면 최소 온셋 간격을 박 간격에 맞춘다 (AUTO_GAP_BEATS박, AUTO_GAP_MIN_MS~AUTO_GAP_MAX_MS).
    pub auto_interval: bool,
    /// 역치: 프레임 RMS가 이보다 작으면 무음으로 본다.
    pub silence_rms: f32,
    /// 역치 자동: silence_rms 대신 최근 소리 크기로 역치를 정한다.
    pub auto_gate: bool,
}

impl Default for Params {
    fn default() -> Self {
        let b = |lo, hi| BandParams { lo, hi, sensitivity: 1.5 };
        Params {
            bands: [b(30.0, 120.0), b(120.0, 300.0), b(300.0, 2000.0), b(2000.0, 8000.0), b(30.0, 8000.0)],
            min_onset_interval_ms: 120.0,
            auto_interval: false,
            silence_rms: SILENCE_RMS,
            auto_gate: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BandOut {
    pub energy: f32,
    pub onset: bool,
    pub strength: f32,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameOut {
    pub bands: [BandOut; N_BANDS],
    pub bpm: Option<f32>,
    pub beat: bool,
    pub silent: bool,
    /// 프레임 RMS (역치와 비교되는 값).
    pub rms: f32,
    /// 이번 프레임에 쓴 최소 온셋 간격(ms). 간격 자동이면 박에 따라 바뀐다.
    pub gap_ms: f32,
    /// 이번 프레임에 쓴 역치(dBFS). 역치 자동이면 소리에 따라 바뀐다.
    pub gate_db: f32,
}

fn to_db(rms: f32) -> f32 {
    20.0 * rms.max(1e-6).log10()
}

/// 역치 자동: 최근 프레임 크기(dB)의 분포로 역치를 정한다.
struct AutoGate {
    hist: VecDeque<f32>,
    cap: usize,
    /// 다시 계산하기까지 남은 프레임 수.
    countdown: usize,
    every: usize,
    /// 지금 역치 (dB). 목표를 부드럽게 따라간다.
    db: f32,
    target: f32,
    /// 프레임마다 목표를 따라가는 비율.
    follow: f32,
}

impl AutoGate {
    fn new(fps: f32, start_db: f32) -> Self {
        AutoGate {
            hist: VecDeque::new(),
            cap: (AUTO_GATE_WINDOW_S * fps) as usize,
            countdown: 0,
            every: (fps / 4.0).max(1.0) as usize,
            db: start_db,
            target: start_db,
            follow: 1.0 - (-1.0 / (1.0 * fps)).exp(),
        }
    }

    fn push(&mut self, db: f32) -> f32 {
        if db.is_finite() && db > AUTO_GATE_IGNORE_DB {
            if self.hist.len() == self.cap {
                self.hist.pop_front();
            }
            self.hist.push_back(db);
        }
        if self.countdown == 0 {
            self.countdown = self.every;
            // 1초 넘게 들은 뒤에만 정한다. 디지털 무음이 이어지는 동안은 그대로 둔다.
            if self.hist.len() * 8 >= self.cap {
                let mut v: Vec<f32> = self.hist.iter().copied().collect();
                v.sort_unstable_by(f32::total_cmp);
                let at = |q: f32| v[((v.len() - 1) as f32 * q) as usize];
                let (low, loud) = (at(0.10), at(0.95));
                self.target = if loud - low < AUTO_GATE_STEADY_DB {
                    loud + 3.0
                } else {
                    loud - AUTO_GATE_BELOW_DB
                }
                .clamp(AUTO_GATE_MIN_DB, AUTO_GATE_MAX_DB);
            }
        }
        self.countdown -= 1;
        self.db += (self.target - self.db) * self.follow;
        self.db
    }
}

struct BandState {
    lo_bin: usize,
    hi_bin: usize, // 미포함
    peak: f32,
    hist: VecDeque<f32>,
    hist_sum: f32,
    f1: f32,
    f2: f32,
    thr1: f32,
    last_onset: i64,
    excess_peak: f32,
    onsets: VecDeque<u64>,
}

impl BandState {
    fn new() -> Self {
        BandState {
            lo_bin: 1,
            hi_bin: 2,
            peak: 0.0,
            hist: VecDeque::new(),
            hist_sum: 0.0,
            f1: 0.0,
            f2: 0.0,
            thr1: f32::MAX,
            last_onset: i64::MIN / 2,
            excess_peak: 0.0,
            onsets: VecDeque::new(),
        }
    }
}

pub struct Analyzer {
    fps: f32,
    rate: f32,
    decim: usize,
    decim_acc: f32,
    decim_n: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    win_norm: f32,
    input: Vec<f32>,
    scratch: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    mag: Vec<f32>,
    logmag: Vec<f32>,
    prev_log: Vec<f32>,
    level: f32,
    bands: [BandState; N_BANDS],
    params: Params,
    frame: u64,
    prev_silent: bool,
    prev_leaked: [bool; N_BANDS],
    tempo: Tempo,
    auto_gate: AutoGate,
}

impl Analyzer {
    pub fn new(sample_rate: u32, params: Params) -> Self {
        // 96k/192k 장치는 정수배로 줄여 48k 근처에서 분석한다.
        let decim = ((sample_rate as f32 / 48000.0).round() as usize).max(1);
        let rate = sample_rate as f32 / decim as f32;
        let fps = rate / HOP as f32;
        let window: Vec<f32> = (0..WINDOW)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / WINDOW as f32).cos())
            .collect();
        let win_norm = 2.0 / window.iter().sum::<f32>();
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(WINDOW);
        let spectrum = fft.make_output_vec();
        let nbins = spectrum.len();
        let mut a = Analyzer {
            fps,
            rate,
            decim,
            decim_acc: 0.0,
            decim_n: 0,
            fft,
            window,
            win_norm,
            input: Vec::with_capacity(WINDOW * 4),
            scratch: vec![0.0; WINDOW],
            spectrum,
            mag: vec![0.0; nbins],
            logmag: vec![0.0; nbins],
            prev_log: vec![0.0; nbins],
            level: LEVEL_FLOOR,
            bands: std::array::from_fn(|_| BandState::new()),
            params: params.clone(),
            frame: 0,
            prev_silent: true,
            prev_leaked: [false; N_BANDS],
            tempo: Tempo::new(fps),
            auto_gate: AutoGate::new(fps, to_db(params.silence_rms)),
        };
        a.set_params(params);
        a
    }

    #[cfg(test)]
    pub fn fps(&self) -> f32 {
        self.fps
    }

    pub fn set_params(&mut self, params: Params) {
        let nbins = self.spectrum.len();
        let bin_hz = self.rate / WINDOW as f32;
        for (state, p) in self.bands.iter_mut().zip(params.bands.iter()) {
            let lo = ((p.lo / bin_hz).ceil() as usize).clamp(1, nbins - 1);
            let hi = ((p.hi / bin_hz).ceil() as usize).clamp(lo + 1, nbins);
            state.lo_bin = lo;
            state.hi_bin = hi;
        }
        // 역치 자동을 켜면 지금 손으로 정한 역치에서 출발한다.
        if params.auto_gate && !self.params.auto_gate {
            self.auto_gate = AutoGate::new(self.fps, to_db(params.silence_rms));
        }
        self.params = params;
    }

    /// 샘플을 넣고, 완성된 프레임을 `out`에 덧붙인다.
    pub fn push(&mut self, samples: &[f32], out: &mut Vec<FrameOut>) {
        if self.decim == 1 {
            self.input.extend_from_slice(samples);
        } else {
            for &s in samples {
                self.decim_acc += s;
                self.decim_n += 1;
                if self.decim_n == self.decim {
                    self.input.push(self.decim_acc / self.decim as f32);
                    self.decim_acc = 0.0;
                    self.decim_n = 0;
                }
            }
        }
        let mut start = 0;
        while self.input.len() - start >= WINDOW {
            let frame = self.process(start);
            out.push(frame);
            start += HOP;
        }
        if start > 0 {
            self.input.drain(..start);
        }
    }

    fn process(&mut self, start: usize) -> FrameOut {
        let x = &self.input[start..start + WINDOW];
        // 가장 최근 hop 구간의 크기로 무음을 판단한다.
        let recent = &x[WINDOW - HOP..];
        let rms = (recent.iter().map(|v| v * v).sum::<f32>() / HOP as f32).sqrt();
        let auto_db = self.auto_gate.push(to_db(rms));
        let gate_db = if self.params.auto_gate { auto_db } else { to_db(self.params.silence_rms) };
        let silent = !(rms >= 10f32.powf(gate_db / 20.0).max(1e-5));

        for (o, (v, w)) in self.scratch.iter_mut().zip(x.iter().zip(self.window.iter())) {
            *o = if v.is_finite() { v * w } else { 0.0 };
        }
        // 길이가 맞으므로 실패하지 않는다.
        let _ = self.fft.process(&mut self.scratch, &mut self.spectrum);

        let full = (self.bands[FULL].lo_bin, self.bands[FULL].hi_bin);
        let mut peak = 0.0f32;
        for c in &self.spectrum[full.0..full.1] {
            peak = peak.max(c.norm() * self.win_norm);
        }
        // (mag 버퍼는 아래에서 채운다.)
        // 느린 최대값 추적: 곡의 볼륨과 무관하게 스펙트럼을 정규화한다.
        if !silent {
            let decay = (-1.0 / (3.0 * self.fps)).exp();
            self.level = peak.max(self.level * decay).max(LEVEL_FLOOR);
        }
        let gain = LOG_GAIN / self.level;
        std::mem::swap(&mut self.logmag, &mut self.prev_log);
        for (m, c) in self.mag.iter_mut().zip(self.spectrum.iter()) {
            *m = c.norm() * self.win_norm;
        }
        // 창 함수 때문에 센 성분이 옆 bin(±2)으로 번진다. 이웃보다 한참 약한 bin은 번진 것으로 보고
        // flux 계산에서 뺀다 (예: 60Hz 킥이 베이스 대역을 건드리지 않게).
        let nb = self.mag.len();
        for k in 0..nb {
            let lo = k.saturating_sub(2);
            let hi = (k + 3).min(nb);
            let mut around = 0.0f32;
            for j in lo..hi {
                if j != k {
                    around = around.max(self.mag[j]);
                }
            }
            let own = (self.mag[k] - LEAK * around).max(0.0);
            self.logmag[k] = (1.0 + gain * own).ln();
        }

        let n = self.frame;
        self.frame += 1;
        let was_silent = std::mem::replace(&mut self.prev_silent, silent);
        let mut out = FrameOut { silent, rms: if rms.is_finite() { rms } else { 0.0 }, gate_db, ..Default::default() };
        let hist_len = (0.5 * self.fps).round().max(4.0) as usize;
        // 최소 온셋 간격: 간격 자동이고 박이 잡혀 있으면 박 간격의 일부(16분음표보다 조금 짧게), 아니면 설정값.
        let gap_ms = match (self.params.auto_interval, self.tempo.period) {
            (true, Some(p)) => (AUTO_GAP_BEATS * p / self.fps * 1000.0).clamp(AUTO_GAP_MIN_MS, AUTO_GAP_MAX_MS),
            (true, None) => AUTO_GAP_FALLBACK_MS,
            (false, _) => self.params.min_onset_interval_ms,
        };
        out.gap_ms = gap_ms;
        let min_gap = (gap_ms / 1000.0 * self.fps).round() as i64;
        let active_frames = (ACTIVE_WINDOW_S * self.fps) as u64;
        let energy_decay = (-1.0 / (5.0 * self.fps)).exp();
        let excess_decay = (-1.0 / (4.0 * self.fps)).exp();

        // 킥과 베이스는 좁고 붙어 있어서, 한쪽의 타격 순간(짧은 창)에는 다른 쪽으로 번진다.
        // 이웃 대역이 훨씬 크면 그 온셋은 번진 것으로 보고 내지 않는다.
        let band_energy = |b: &BandState| -> f32 { self.mag[b.lo_bin..b.hi_bin].iter().map(|m| m * m).sum() };
        let (e_kick, e_bass) = (band_energy(&self.bands[0]), band_energy(&self.bands[1]));
        // 후보(직전 프레임)가 지금까지의 박 격자(반박 단위)에 얼마나 가까운가. 박을 모르면 None.
        let off_grid = self.tempo.grid_distance(n as f64 - 1.0).map(|d| d > GRID_TOLERANCE);
        let leaked_now = [e_bass > NEIGHBOUR_RATIO * e_kick, e_kick > NEIGHBOUR_RATIO * e_bass, false, false, false];
        // 후보는 직전 프레임이므로 그때의 비율도 함께 본다 (소리가 꺼지는 순간의 번짐을 거른다).
        let leaked: [bool; N_BANDS] = std::array::from_fn(|i| leaked_now[i] || self.prev_leaked[i]);
        self.prev_leaked = leaked_now;

        for (i, b) in self.bands.iter_mut().enumerate() {
            let mut energy = 0.0f32;
            let mut flux = 0.0f32;
            for k in b.lo_bin..b.hi_bin {
                let m = self.mag[k];
                energy += m * m;
                let d = self.logmag[k] - self.prev_log[k];
                if d > 0.0 {
                    flux += d;
                }
            }
            flux /= ((b.hi_bin - b.lo_bin) as f32).sqrt();
            if n == 0 {
                flux = 0.0; // 첫 프레임은 비교 대상이 없다.
            }

            if !silent {
                b.peak = energy.max(b.peak * energy_decay);
            }
            let floor = LEVEL_FLOOR * LEVEL_FLOOR;
            out.bands[i].energy = (energy / b.peak.max(floor)).sqrt().clamp(0.0, 1.0);

            // 직전 프레임(n-1)이 국소 최대인지 이번 프레임의 값으로 확정한다.
            let k = self.params.bands[i].sensitivity.max(0.1);
            let thr0 = if b.hist.is_empty() { f32::MAX } else { b.hist_sum / b.hist.len() as f32 * k + ONSET_DELTA };
            let cand = n as i64 - 1;
            // 후보는 직전 프레임이다. 타격이 프레임 끝에 걸치면 그 프레임은 아직 작게 재어질 수 있으므로
            // 직전·현재 중 하나라도 역치를 넘으면 받는다.
            if !(was_silent && silent) && !leaked[i] && b.f1 > b.thr1 && b.f1 >= b.f2 && b.f1 > flux && cand - b.last_onset >= min_gap {
                let excess = b.f1 - b.thr1;
                b.excess_peak = excess.max(b.excess_peak * excess_decay);
                let strength = (excess / b.excess_peak.max(1e-6)).sqrt().clamp(0.0, 1.0);
                // 뚜렷한 타격·음의 시작만 받는다: 대역에 실제 소리가 있고, 최근 타격 대비 충분히 세고,
                // 박이 잡혀 있으면 박(반박)에 맞거나 아주 센 소리.
                let need = if off_grid == Some(true) { OFF_GRID_STRENGTH } else { MIN_STRENGTH };
                if out.bands[i].energy >= MIN_BAND_LEVEL && strength >= need {
                    out.bands[i].onset = true;
                    out.bands[i].strength = strength;
                    b.last_onset = cand;
                    b.onsets.push_back(n);
                }
            } else {
                b.excess_peak *= excess_decay;
            }
            while b.onsets.front().is_some_and(|&t| n - t > active_frames) {
                b.onsets.pop_front();
            }
            out.bands[i].active = b.onsets.len() >= ACTIVE_MIN_ONSETS;

            b.hist.push_back(flux);
            b.hist_sum += flux;
            if b.hist.len() > hist_len {
                b.hist_sum -= b.hist.pop_front().unwrap_or(0.0);
            }
            if b.hist_sum < 0.0 {
                b.hist_sum = 0.0;
            }
            b.f2 = b.f1;
            b.f1 = flux;
            b.thr1 = thr0;
        }

        let (bpm, beat) = self.tempo.push(self.bands[FULL].f1, silent, n);
        out.bpm = bpm;
        out.beat = beat;
        out
    }
}

/// `full` 대역 flux의 자기상관으로 BPM을 추정하고 박의 위상을 추적한다.
struct Tempo {
    fps: f32,
    env: VecDeque<f32>,
    cap: usize,
    since_est: usize,
    silent_run: usize,
    period: Option<f32>,
    candidate: Option<(f32, u32)>,
    misses: u32,
    /// 박 위치(위상)의 큰 차이가 같은 쪽으로 연속 몇 번 나왔는가, 그리고 다음에 나올 것으로 예상되는 차이.
    phase_votes: u32,
    phase_expect: Option<f64>,
    next_beat: f64,
    last_beat: f64,
    buf: Vec<f32>,
}

impl Tempo {
    fn new(fps: f32) -> Self {
        let cap = (TEMPO_WINDOW_S * fps) as usize;
        Tempo {
            fps,
            env: VecDeque::with_capacity(cap + 1),
            cap,
            since_est: 0,
            silent_run: 0,
            period: None,
            candidate: None,
            misses: 0,
            phase_votes: 0,
            phase_expect: None,
            next_beat: 0.0,
            last_beat: -1e12,
            buf: Vec::with_capacity(cap),
        }
    }

    /// 프레임 위치가 지금까지의 박 격자(반박 단위)에서 몇 박 떨어져 있는가 (0..0.25). 박을 모르면 None.
    fn grid_distance(&self, frame: f64) -> Option<f64> {
        let p = self.period? as f64;
        if self.last_beat < -1e11 || p <= 0.0 {
            return None;
        }
        let half = (frame - self.last_beat) / p * 2.0;
        Some((half - half.round()).abs() / 2.0)
    }

    fn reset(&mut self) {
        self.env.clear();
        self.phase_votes = 0;
        self.phase_expect = None;
        self.period = None;
        self.candidate = None;
        self.misses = 0;
        self.since_est = 0;
    }

    fn push(&mut self, flux: f32, silent: bool, frame: u64) -> (Option<f32>, bool) {
        self.silent_run = if silent { self.silent_run + 1 } else { 0 };
        if self.silent_run as f32 > TEMPO_FORGET_SILENT_S * self.fps {
            if !self.env.is_empty() {
                self.reset();
            }
            return (None, false);
        }
        self.env.push_back(flux);
        if self.env.len() > self.cap {
            self.env.pop_front();
        }
        self.since_est += 1;
        if self.since_est as f32 >= 0.5 * self.fps && self.env.len() as f32 >= TEMPO_WARMUP_S * self.fps {
            self.since_est = 0;
            self.estimate(frame);
        }
        let Some(p) = self.period else { return (None, false) };
        let mut beat = false;
        if frame as f64 >= self.next_beat - 0.5 {
            beat = true;
            self.last_beat = self.next_beat;
            self.next_beat += p as f64;
            // 오래 밀린 경우 한 번에 따라잡는다.
            while self.next_beat < frame as f64 {
                self.next_beat += p as f64;
            }
        }
        (Some(60.0 * self.fps / p), beat)
    }

    fn estimate(&mut self, frame: u64) {
        let n = self.env.len();
        let mean = self.env.iter().sum::<f32>() / n as f32;
        // 평균을 빼고 5-탭 삼각 창으로 살짝 뭉갠다: 박 간격이 프레임의 정수배가 아니어도
        // (예: 150BPM = 37.5프레임) 자기상관 봉우리가 갈라지지 않는다.
        let raw_env: Vec<f32> = self.env.iter().map(|v| v - mean).collect();
        self.buf.clear();
        const TAPS: [f32; 5] = [1.0 / 9.0, 2.0 / 9.0, 3.0 / 9.0, 2.0 / 9.0, 1.0 / 9.0];
        for i in 0..n {
            let mut s = 0.0;
            for (j, w) in TAPS.iter().enumerate() {
                let idx = i as isize + j as isize - 2;
                if idx >= 0 && (idx as usize) < n {
                    s += w * raw_env[idx as usize];
                }
            }
            self.buf.push(s);
        }
        let x = &self.buf;
        let acf = |lag: usize| -> f32 {
            if lag >= n {
                return 0.0;
            }
            let mut s = 0.0;
            for i in lag..n {
                s += x[i] * x[i - lag];
            }
            s / (n - lag) as f32
        };
        let r0 = acf(0);
        let lag_min = (60.0 / BPM_MAX * self.fps).floor() as usize;
        let lag_max = (60.0 / BPM_MIN * self.fps).ceil() as usize;
        let raw: Vec<f32> = (lag_min - 1..=lag_max + 1).map(acf).collect();
        let mut best = (0usize, 0.0f32);
        for (j, lag) in (lag_min..=lag_max).enumerate() {
            let r = raw[j + 1];
            // 국소 최대만 후보로 삼는다.
            if r < raw[j] || r < raw[j + 2] {
                continue;
            }
            let bpm = 60.0 * self.fps / lag as f32;
            // 춤추는 박(대략 90~160BPM) 쪽으로 기울인다. 킥·스네어가 두 박마다 되풀이되는 곡에서
            // 두 박을 한 박으로 잡는(절반 템포) 실수가 잦아서, 예전(폭 0.8)보다 좁혔다.
            let prior = (-0.5 * ((bpm / TEMPO_PRIOR_BPM).log2() / TEMPO_PRIOR_WIDTH).powi(2)).exp();
            let double = if lag * 2 < n * 3 / 4 { acf(lag * 2) } else { r };
            let score = (r + 0.5 * double.max(0.0)) * prior;
            if score > best.1 {
                best = (lag, score);
            }
        }
        // 절반 간격(두 배 빠른 템포)에도 뚜렷한 봉우리가 있으면 그쪽이 실제 박이다.
        if best.0 > 0 {
            let half = best.0 / 2;
            let top = acf(best.0);
            let cand = (half.saturating_sub(1)..=half + 1)
                .filter(|l| *l >= lag_min && *l <= lag_max)
                .map(|l| (l, acf(l)))
                .fold((0usize, f32::MIN), |a, b| if b.1 > a.1 { b } else { a });
            if cand.0 > 0 && cand.1 >= HALF_LAG_RATIO * top {
                best.0 = cand.0;
            }
        }
        // 2:3 실수: 1.5박을 한 박으로 잡은 경우(반박마다 하이햇이 있으면 1.5박 간격에도 봉우리가 선다).
        // 2/3 간격(1.5배 빠른 템포)의 봉우리도 뚜렷하면 그쪽이 실제 박이다.
        if best.0 > 0 {
            let top = acf(best.0);
            let l = (best.0 as f32 * 2.0 / 3.0).round() as usize;
            let cand = (l.saturating_sub(1)..=l + 1)
                .filter(|x| *x >= lag_min && *x <= lag_max)
                .map(|x| (x, acf(x)))
                .fold((0usize, f32::MIN), |a, b| if b.1 > a.1 { b } else { a });
            if cand.0 > 0 && cand.1 >= THIRDS_LAG_RATIO * top {
                best.0 = cand.0;
            }
        }
        let confident = r0 > 1e-9 && best.0 > 0 && acf(best.0) / r0 >= 0.2;
        if !confident {
            self.misses += 1;
            if self.misses >= 4 {
                self.period = None;
                self.candidate = None;
            }
            return;
        }
        self.misses = 0;

        // 포물선 보간으로 소수 lag를 구한다.
        let j = best.0 - lag_min + 1;
        let (a, b, c) = (raw[j - 1], raw[j], raw[j + 1]);
        let denom = a - 2.0 * b + c;
        let frac = if denom.abs() > 1e-12 { (0.5 * (a - c) / denom).clamp(-0.5, 0.5) } else { 0.0 };
        let new_p = best.0 as f32 + frac;

        let close = |p: f32, q: f32| (p / q - 1.0).abs() < 0.06;
        match self.period {
            Some(p) if close(new_p, p) => {
                self.period = Some(0.7 * p + 0.3 * new_p);
                self.candidate = None;
            }
            Some(p) => match self.candidate {
                // 다른 템포가 연속으로 나올 때만 갈아탄다.
                Some((c, count)) if close(new_p, c) => {
                    // 실제 음악에서는 추정이 2:3, 1:2 같은 배수 사이를 오가기 쉽다. 2초 내내 같은
                    // 값이 나올 때만 갈아타서 박이 중간에 흔들리지 않게 한다.
                    // 지금 템포의 2배·절반·2:3 관계로 갈아타는 것은 대개 잘못 잡은 것이라(곡이 바뀐 게 아니라)
                    // 더 오래(4초) 같은 값이 나와야 갈아탄다.
                    let r = new_p / p;
                    let related = [2.0f32, 0.5, 1.5, 2.0 / 3.0].iter().any(|k| (r / k - 1.0).abs() < 0.06);
                    let need = if related { TEMPO_SWITCH_RELATED } else { TEMPO_SWITCH_COUNT };
                    if count + 1 >= need {
                        self.period = Some(new_p);
                        self.candidate = None;
                    } else {
                        self.candidate = Some((new_p, count + 1));
                    }
                }
                _ => self.candidate = Some((new_p, 1)),
            },
            None => self.period = Some(new_p),
        }
        let Some(p) = self.period else { return };

        // 위상: 가장 최근 박이 몇 프레임 전인지(phi)를 빗살 합으로 찾는다.
        let steps = p.ceil() as usize;
        let mut best_phi = (0usize, -1.0f32);
        for phi in 0..steps {
            let mut s = 0.0;
            let mut w = 1.0;
            let mut pos = (n - 1) as f32 - phi as f32;
            while pos >= 0.0 {
                let i = pos.round() as usize;
                let near = x[i].max(if i > 0 { x[i - 1] * 0.5 } else { 0.0 }).max(if i + 1 < n { x[i + 1] * 0.5 } else { 0.0 });
                s += w * near;
                w *= 0.92;
                pos -= p;
            }
            if s > best_phi.1 {
                best_phi = (phi, s);
            }
        }
        let now = frame as f64;
        let mut target = now - best_phi.0 as f64 + p as f64;
        while target <= now {
            target += p as f64;
        }
        if self.last_beat < -1e11 {
            self.next_beat = target;
            return;
        }
        // 기존 예측과의 차이를 한 박 안으로 접어서 조금씩 당긴다.
        let pf = p as f64;
        let mut diff = (target - self.next_beat) % pf;
        if diff > pf / 2.0 {
            diff -= pf;
        }
        if diff < -pf / 2.0 {
            diff += pf;
        }
        // 작은 차이는 조금씩 당긴다. 큰 차이(박과 엇박을 헷갈린 경우가 많다)는 같은 쪽으로 3번 연속(1.5초)
        // 나와야 믿고, 그때도 한 번에 0.15박씩만 옮긴다. 예전엔 바로 옮겨서 박이 반 박씩 튀었고,
        // 그 사이 박 하나를 건너뛰어 "박" 인형이 두 박에 한 번 움직이는 것처럼 보였다.
        let max_step = PHASE_MAX_STEP * pf;
        let step = if diff.abs() < 0.2 * pf {
            self.phase_votes = 0;
            0.35 * diff
        } else {
            let same = self.phase_expect.is_some_and(|e| (e - diff).abs() < 0.15 * pf);
            self.phase_votes = if same { self.phase_votes + 1 } else { 1 };
            if self.phase_votes >= PHASE_VOTES {
                diff.clamp(-max_step, max_step)
            } else {
                0.0
            }
        };
        self.phase_expect = (diff.abs() >= 0.2 * pf).then_some(diff - step);
        self.next_beat += step;
        // 박 간격이 0.8박보다 줄지 않게 한다 (한 박을 통째로 건너뛰지 않는다).
        if self.next_beat < self.last_beat + 0.8 * pf {
            self.next_beat = self.last_beat + 0.8 * pf;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48000;

    struct Rng(u32);
    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
            (self.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
        }
    }

    /// 짧은 감쇠 잡음(클릭)을 `times`(초)에 넣은 신호.
    fn clicks(times: &[f32], secs: f32, amp: f32) -> Vec<f32> {
        let mut out = vec![0.0f32; (secs * SR as f32) as usize];
        let mut rng = Rng(7);
        for &t in times {
            let s = (t * SR as f32) as usize;
            for i in 0..(0.03 * SR as f32) as usize {
                if s + i < out.len() {
                    out[s + i] += amp * rng.next() * (-(i as f32) / (0.006 * SR as f32)).exp();
                }
            }
        }
        out
    }

    /// 부드럽게 켜지고 꺼지는 사인 버스트를 더한다.
    fn add_bursts(out: &mut [f32], freq: f32, times: &[f32], dur: f32, amp: f32) {
        let ramp = 0.008 * SR as f32;
        let len = (dur * SR as f32) as usize;
        for &t in times {
            let s = (t * SR as f32) as usize;
            for i in 0..len {
                if s + i >= out.len() {
                    break;
                }
                let env = (i as f32 / ramp).min(1.0).min((len - i) as f32 / ramp);
                out[s + i] += amp * env * (2.0 * std::f32::consts::PI * freq * i as f32 / SR as f32).sin();
            }
        }
    }

    fn run(signal: &[f32]) -> (Vec<FrameOut>, f32) {
        let mut a = Analyzer::new(SR, Params::default());
        let mut out = Vec::new();
        // 실제 캡처처럼 10ms 단위로 넣는다.
        for chunk in signal.chunks(480) {
            a.push(chunk, &mut out);
        }
        (out, a.fps())
    }

    fn onset_times(frames: &[FrameOut], band: usize, fps: f32) -> Vec<f32> {
        frames.iter().enumerate().filter(|(_, f)| f.bands[band].onset).map(|(i, _)| i as f32 / fps).collect()
    }

    fn every(period: f32, start: f32, secs: f32) -> Vec<f32> {
        let mut v = Vec::new();
        let mut t = start;
        while t < secs - 0.1 {
            v.push(t);
            t += period;
        }
        v
    }

    #[test]
    fn counts_regular_clicks() {
        let times = every(0.5, 0.25, 10.0);
        let (frames, fps) = run(&clicks(&times, 10.0, 0.5));
        let onsets = onset_times(&frames, FULL, fps);
        assert_eq!(onsets.len(), times.len(), "onsets at {onsets:?}");
        for (o, t) in onsets.iter().zip(times.iter()) {
            assert!((o - t).abs() < 0.04, "onset {o} vs click {t}");
        }
    }

    #[test]
    fn onset_count_is_independent_of_volume() {
        let times = every(0.5, 0.25, 10.0);
        for amp in [0.9, 0.1, 0.01] {
            let (frames, fps) = run(&clicks(&times, 10.0, amp));
            let n = onset_times(&frames, FULL, fps).len();
            assert_eq!(n, times.len(), "amp {amp}");
        }
    }

    #[test]
    fn separates_low_and_high_bursts() {
        let secs = 10.0;
        let low = every(0.5, 0.25, secs);
        let high = every(0.5, 0.5, secs);
        let mut sig = vec![0.0f32; (secs * SR as f32) as usize];
        add_bursts(&mut sig, 60.0, &low, 0.12, 0.6);
        add_bursts(&mut sig, 5000.0, &high, 0.05, 0.3);
        let (frames, fps) = run(&sig);

        let kick = onset_times(&frames, 0, fps);
        let hi = onset_times(&frames, 3, fps);
        assert_eq!(kick.len(), low.len(), "kick onsets {kick:?}");
        assert_eq!(hi.len(), high.len(), "high onsets {hi:?}");
        for (o, t) in kick.iter().zip(low.iter()) {
            assert!((o - t).abs() < 0.06, "kick onset {o} vs burst {t}");
        }
        for (o, t) in hi.iter().zip(high.iter()) {
            assert!((o - t).abs() < 0.06, "high onset {o} vs burst {t}");
        }
        let last = frames.last().unwrap();
        assert!(last.bands[0].active && last.bands[3].active);
        assert!(!last.bands[1].active, "bass should stay inactive");
    }

    #[test]
    fn estimates_bpm_and_emits_steady_beats() {
        for bpm in [90.0f32, 120.0, 150.0] {
            let secs = 14.0;
            let times = every(60.0 / bpm, 0.2, secs);
            let (frames, fps) = run(&clicks(&times, secs, 0.5));
            let est = frames.last().unwrap().bpm.expect("tempo should be found");
            assert!((est - bpm).abs() < 2.0, "estimated {est} for {bpm}");

            // 마지막 6초 동안 박이 클릭과 같은 간격으로, 클릭 근처에서 나와야 한다.
            let beats: Vec<f32> =
                frames.iter().enumerate().filter(|(i, f)| f.beat && *i as f32 / fps > secs - 6.0).map(|(i, _)| i as f32 / fps).collect();
            let expected = 6.0 * bpm / 60.0;
            assert!((beats.len() as f32 - expected).abs() <= 1.0, "{} beats for {bpm} bpm", beats.len());
            for b in &beats {
                let d = times.iter().map(|t| (t - b).abs()).fold(f32::MAX, f32::min);
                assert!(d < 0.06, "beat at {b} is {d}s from a click ({bpm} bpm)");
            }
        }
    }

    /// 박 사이 간격들(초). 박을 잡은 뒤 from초부터.
    fn beat_gaps(frames: &[FrameOut], fps: f32, from: f32) -> Vec<f32> {
        let beats: Vec<f32> = frames.iter().enumerate().filter(|(i, f)| f.beat && *i as f32 / fps > from).map(|(i, _)| i as f32 / fps).collect();
        beats.windows(2).map(|w| w[1] - w[0]).collect()
    }

    /// 실제 곡에 가깝게: 박마다 킥(세기 들쭉날쭉), 2·4박 스네어, 반박마다 하이햇(가끔 킥보다 셈),
    /// 16분음표 고스트, 모든 타격에 ±15ms 흔들림, 바닥 잡음.
    fn groove(bpm: f32, secs: f32, seed: u32) -> Vec<f32> {
        let p = 60.0 / bpm;
        let mut rng = Rng(seed);
        let mut sig = vec![0.0f32; (secs * SR as f32) as usize];
        let jit = |r: &mut Rng| r.next() * 0.015;
        let mut kicks = Vec::new();
        let mut snares = Vec::new();
        let mut hats = Vec::new();
        let mut ghosts = Vec::new();
        for (i, t) in every(p, 0.2, secs).into_iter().enumerate() {
            kicks.push((t + jit(&mut rng), 0.35 + 0.25 * rng.next().abs()));
            if i % 2 == 1 {
                snares.push(t + jit(&mut rng));
            }
            hats.push((t + p / 2.0 + jit(&mut rng), 0.25 + 0.3 * rng.next().abs()));
            ghosts.push((t + p * 0.75 + jit(&mut rng), 0.08 + 0.1 * rng.next().abs()));
        }
        for (t, a) in &kicks {
            add_bursts(&mut sig, 60.0, &[*t], 0.1, *a);
        }
        for (t, a) in hats.iter().chain(ghosts.iter()) {
            for (o, v) in sig.iter_mut().zip(clicks(&[*t], secs, *a)) {
                *o += v;
            }
        }
        add_bursts(&mut sig, 220.0, &snares, 0.08, 0.35);
        for (o, v) in sig.iter_mut().zip(clicks(&snares, secs, 0.35)) {
            *o += v;
        }
        for o in sig.iter_mut() {
            *o += rng.next() * 0.01;
        }
        sig
    }

    #[test]
    fn beats_stay_even_in_a_busy_groove() {
        // 박이 반 박씩 튀거나 템포를 절반으로 잡으면 "박" 인형이 두 박에 한 번처럼 보인다.
        //  - 절반 템포로 잡으면 안 된다. 느린 곡(100BPM 미만)은 두 배로 잡는 것까지는 허용한다.
        //  - 박을 잡은 뒤(10초부터) 박 사이 간격이 잡은 템포대로 고르다 (15% 넘게 어긋난 간격이 10% 이하).
        let cases = [
            (85.0f32, 11u32), (92.0, 31), (96.0, 5), (100.0, 12), (109.0, 1), (109.0, 21), (118.0, 13), (124.0, 14), (128.0, 2),
            (128.0, 32), (135.0, 15), (142.0, 3), (142.0, 23), (145.0, 33), (150.0, 16), (156.0, 17), (160.0, 34), (163.0, 4),
            (163.0, 24), (170.0, 18), (176.0, 19),
        ];
        for (bpm, seed) in cases {
            let secs = 24.0;
            let (frames, fps) = run(&groove(bpm, secs, seed));
            let est = frames.last().unwrap().bpm.expect("tempo should be found");
            let ok_tempo = (est / bpm - 1.0).abs() < 0.03 || (bpm < 100.0 && (est / (2.0 * bpm) - 1.0).abs() < 0.03);
            assert!(ok_tempo, "{bpm}BPM estimated as {est:.1}");
            let p = 60.0 / est;
            let gaps = beat_gaps(&frames, fps, 10.0);
            let bad = gaps.iter().filter(|g| (*g / p - 1.0).abs() > 0.2).count();
            assert!(bad as f32 <= 0.1 * gaps.len() as f32, "{bpm}BPM (est {est:.1}): {bad}/{} uneven beat gaps {gaps:?}", gaps.len());
        }
    }

    #[test]
    fn auto_interval_follows_the_beat() {
        let times = every(0.5, 0.25, 10.0); // 120BPM → 0.22박 = 110ms
        let sig = clicks(&times, 10.0, 0.5);
        let mut a = Analyzer::new(SR, Params { auto_interval: true, min_onset_interval_ms: 400.0, ..Params::default() });
        let mut out = Vec::new();
        for chunk in sig.chunks(480) {
            a.push(chunk, &mut out);
        }
        assert!((out[0].gap_ms - AUTO_GAP_FALLBACK_MS).abs() < 1e-3, "before the beat is known");
        let last = out.last().unwrap().gap_ms;
        assert!((last - 110.0).abs() < 5.0, "gap {last}ms at 120BPM");
        // 설정값(400ms)이 아니라 자동 값이 쓰여서 클릭이 모두 잡힌다.
        assert_eq!(out.iter().filter(|f| f.bands[FULL].onset).count(), times.len());
    }

    #[test]
    fn faint_off_beat_ticks_are_not_onsets() {
        // 0.5초마다 센 클릭 + 그 사이 엇나간 자리(0.17박, 0.37박 …)에 아주 약한 잡음 틱.
        let secs = 12.0;
        let strong = every(0.5, 0.25, secs);
        let faint: Vec<f32> = strong.iter().filter(|t| **t < secs - 0.6).map(|t| t + 0.085 + 0.1 * ((t * 7.0) % 1.0)).collect();
        let mut sig = clicks(&strong, secs, 0.5);
        for (o, v) in sig.iter_mut().zip(clicks(&faint, secs, 0.03)) {
            *o += v;
        }
        let (frames, fps) = run(&sig);
        // 박을 잡은 뒤(6초 이후)에는 센 클릭만 온셋이다.
        let late = onset_times(&frames, FULL, fps).into_iter().filter(|t| *t > 6.0).collect::<Vec<_>>();
        let expected = strong.iter().filter(|t| **t > 6.0).count();
        assert_eq!(late.len(), expected, "onsets {late:?}");
        for o in &late {
            let d = strong.iter().map(|t| (t - o).abs()).fold(f32::MAX, f32::min);
            assert!(d < 0.04, "onset at {o} is not a strong click");
        }
    }

    #[test]
    fn follows_a_tempo_change_within_a_few_seconds() {
        // 120BPM 10초 → 90BPM 8초 (곡이 바뀜). 바뀐 뒤 5초 안에 새 템포를 따라야 한다.
        let mut times = every(0.5, 0.2, 10.0);
        times.extend(every(60.0 / 90.0, 10.0, 18.0));
        let (frames, fps) = run(&clicks(&times, 18.0, 0.5));
        let switched = frames
            .iter()
            .enumerate()
            .find(|(i, f)| *i as f32 / fps > 10.0 && f.bpm.is_some_and(|b| (b - 90.0).abs() < 3.0))
            .map(|(i, _)| i as f32 / fps)
            .expect("new tempo should be found");
        assert!(switched < 15.0, "switched at {switched}s");
    }

    #[test]
    fn keeps_beating_through_a_gap_in_the_drums() {
        // 8초간 클릭 후 3초간 약한 지속음만 (드럼이 쉬는 구간).
        let secs = 11.0;
        let times = every(0.5, 0.2, 8.0);
        let mut sig = clicks(&times, secs, 0.5);
        add_bursts(&mut sig, 440.0, &[0.0], secs - 0.05, 0.05);
        let (frames, fps) = run(&sig);
        let beats = frames.iter().enumerate().filter(|(i, f)| f.beat && *i as f32 / fps > 8.3).count();
        assert!((4..=7).contains(&beats), "{beats} beats in the gap");
    }

    #[test]
    fn sounds_below_the_gate_are_ignored() {
        let times = every(0.5, 0.25, 6.0);
        let sig = clicks(&times, 6.0, 0.05);
        let run_with = |gate_db: f32| {
            let mut a = Analyzer::new(SR, Params { silence_rms: 10f32.powf(gate_db / 20.0), ..Params::default() });
            let mut out = Vec::new();
            for chunk in sig.chunks(480) {
                a.push(chunk, &mut out);
            }
            out.iter().filter(|f| f.bands[FULL].onset).count()
        };
        assert_eq!(run_with(-74.0), times.len());
        assert_eq!(run_with(-20.0), 0);
    }

    #[test]
    fn auto_gate_follows_the_music_level() {
        // 음악 비슷한 신호: 220Hz 음 위에 0.5초마다(120BPM) 클릭. 같은 곡을 크게/작게 틀어 본다.
        let times = every(0.5, 0.1, 10.0);
        let clicks_sig = clicks(&times, 10.0, 0.5);
        let song: Vec<f32> =
            clicks_sig.iter().enumerate().map(|(i, c)| c + 0.1 * (i as f32 * 220.0 * std::f32::consts::TAU / SR as f32).sin()).collect();
        let run_at = |gain: f32| {
            let mut a = Analyzer::new(SR, Params { silence_rms: 10f32.powf(-74.0 / 20.0), auto_gate: true, ..Params::default() });
            let mut out = Vec::new();
            for chunk in song.chunks(480) {
                let c: Vec<f32> = chunk.iter().map(|v| v * gain).collect();
                a.push(&c, &mut out);
            }
            let late = &out[out.len() / 2..];
            (out.last().unwrap().gate_db, late.iter().filter(|f| f.bands[FULL].onset).count())
        };
        let (loud_gate, loud_onsets) = run_at(0.3);
        let (quiet_gate, quiet_onsets) = run_at(0.015); // -26dB
        // 역치는 곡의 크기를 따라 내려가고, 작게 틀어도 박은 계속 잡힌다.
        assert!(loud_gate > -60.0 && loud_gate < -24.0, "loud gate {loud_gate}");
        assert!((loud_gate - quiet_gate - 26.0).abs() < 6.0, "loud {loud_gate} quiet {quiet_gate}");
        let expected = times.iter().filter(|&&t| t > 5.0).count();
        assert!(loud_onsets + 2 >= expected && quiet_onsets + 2 >= expected, "{loud_onsets}/{quiet_onsets} of {expected}");

        // 크기가 일정한 잡음뿐이면 역치가 잡음 위로 올라가 반응하지 않는다.
        let mut rng = Rng(9);
        let noise: Vec<f32> = (0..SR as usize * 6).map(|_| rng.next() * 0.003).collect();
        let mut a = Analyzer::new(SR, Params { silence_rms: 10f32.powf(-90.0 / 20.0), auto_gate: true, ..Params::default() });
        let mut out = Vec::new();
        for chunk in noise.chunks(480) {
            a.push(chunk, &mut out);
        }
        assert!(out.iter().skip(out.len() * 2 / 3).all(|f| f.silent));
    }

    #[test]
    fn silence_produces_nothing() {
        let (frames, _) = run(&vec![0.0f32; SR as usize * 3]);
        assert!(frames.iter().all(|f| f.silent && !f.beat && f.bpm.is_none()));
        assert!(frames.iter().all(|f| f.bands.iter().all(|b| !b.onset && !b.active)));
    }

    #[test]
    fn noise_has_no_tempo() {
        let mut rng = Rng(3);
        let sig: Vec<f32> = (0..SR as usize * 8).map(|_| rng.next() * 0.2).collect();
        let (frames, _) = run(&sig);
        assert!(frames.last().unwrap().bpm.is_none());
    }

    #[test]
    fn high_sample_rates_are_decimated() {
        let a = Analyzer::new(96000, Params::default());
        assert!((a.fps() - 93.75).abs() < 0.01);
    }
}
