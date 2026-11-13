//! 설정 스키마 (SPEC 10장), 기본값, 저장/복원, 자동 대역 배정.

use crate::audio::analysis::{BandParams, Params};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BandId {
    Kick,
    Bass,
    Mid,
    High,
    Full,
    /// 예전 형식(인형 전용 반응을 대역 자리에 두던 때). 읽을 때 `Full` + `React`로 옮긴다.
    Any,
    /// 인형 전용: 추정된 박마다 (대역과 상관없이). 반응 방식은 늘 박자다.
    Beat,
}

/// 인형이 자기 대역의 소리에 반응하는 방식.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum React {
    /// 박자: 박을 예측하며 돌고, 박에 맞는 소리만 받는다 (기본).
    #[default]
    Beat,
    /// 모든 소리: 소리가 날 때마다 바로 (박 거르기 없이, 촘촘히).
    Every,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Bands<T> {
    pub kick: T,
    pub bass: T,
    pub mid: T,
    pub high: T,
    pub full: T,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct BandCfg {
    pub lo: f32,
    pub hi: f32,
    pub sensitivity: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Source {
    System,
    /// 고른 앱들(실행 파일 이름)의 소리만 합쳐서 듣는다.
    Apps { names: Vec<String> },
    /// 예전 형식(앱 하나). 읽을 때만 쓰고 `Apps`로 옮긴다.
    Process { pid: u32, name: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AudioCfg {
    pub source: Source,
    pub bands: Bands<BandCfg>,
    pub min_onset_interval_ms: f32,
    /// 간격 자동: 박에 맞춰 최소 온셋 간격을 정한다 (위 값 대신).
    #[serde(default = "yes")]
    pub auto_interval: bool,
    /// 역치: 입력이 이 크기(dBFS)보다 작으면 무음으로 보고 반응하지 않는다.
    #[serde(default = "default_gate_db")]
    pub gate_db: f32,
}

pub const DEFAULT_GATE_DB: f32 = -74.0;

fn default_gate_db() -> f32 {
    DEFAULT_GATE_DB
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Pet,
    Wallpaper,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Background {
    None,
    Color { value: String },
    Image { path: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DisplayCfg {
    pub mode: Mode,
    pub monitor: usize,
    pub global_scale: f32,
    pub background: Background,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Sync {
    Onset,
    Tempo,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Assign {
    Spread,
    Together,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MotionCfg {
    pub sync: Sync,
    pub assign: Assign,
    pub amount: f32,
    pub stiffness: f32,
    pub damping: f32,
    /// 화면 지연 보정(ms). 양수면 인형이 그만큼 늦게, 음수면 일찍 움직인다.
    #[serde(rename = "latencyMs", default)]
    pub latency_ms: f32,
}

pub const LATENCY_MIN_MS: f32 = -200.0;
pub const LATENCY_MAX_MS: f32 = 300.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Near {
    Left,
    Right,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LayoutCfg {
    pub ridge: [Pt; 3],
    pub near: Near,
    pub far_scale: f32,
    pub overlap: f32,
}

/// 인형이 앉는 선 하나. 원근·겹침 같은 속성은 선마다 따로 갖는다.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub id: String,
    pub ridge: [Pt; 3],
    pub near: Near,
    pub far_scale: f32,
    pub overlap: f32,
    /// 간격 자동: 줄이 선 전체에 고르게 퍼지도록 간격을 정한다 (overlap 대신).
    #[serde(default)]
    pub auto_gap: bool,
}

const FIRST_LINE: &str = "line1";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Manual {
    pub layout: bool,
    pub band: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Doll {
    pub id: String,
    pub image: String,
    /// 잘라낸 이미지의 너비 / 높이.
    pub aspect: f32,
    pub order: u32,
    pub flip: bool,
    pub visible: bool,
    pub band: BandId,
    /// 반응 방식 (대역과 따로 정한다).
    #[serde(default)]
    pub react: React,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    /// 자동 배치 안에서의 개별 크기 배율.
    #[serde(default = "one")]
    pub size: f32,
    /// 속한 선의 id.
    #[serde(default)]
    pub line: String,
    /// 선 위에서 직접 정한 자리 (선 길이 비율, 0 = 왼쪽 끝). 없으면 자동 줄 세우기.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f32>,
    pub manual: Manual,
}

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Config {
    pub audio: AudioCfg,
    pub display: DisplayCfg,
    pub motion: MotionCfg,
    #[serde(default)]
    pub lines: Vec<Line>,
    /// 예전 형식(선이 하나뿐이던 때)의 설정. 읽을 때만 쓰고 `lines`로 옮긴다.
    #[serde(default, skip_serializing)]
    pub layout: Option<LayoutCfg>,
    pub dolls: Vec<Doll>,
}

const HILL: [Pt; 3] = [Pt { x: 0.08, y: 0.78 }, Pt { x: 0.5, y: 0.66 }, Pt { x: 0.92, y: 0.70 }];

/// 모드별 기본 능선. 펫 모드는 작업 표시줄 바로 위(`work_bottom`)의 수평선.
pub fn default_ridge(mode: Mode, work_bottom: f32) -> [Pt; 3] {
    match mode {
        Mode::Wallpaper => HILL,
        Mode::Pet => {
            let y = work_bottom.clamp(0.3, 1.0);
            [Pt { x: 0.08, y }, Pt { x: 0.5, y }, Pt { x: 0.92, y }]
        }
    }
}

fn same_ridge(a: &[Pt; 3], b: &[Pt; 3]) -> bool {
    a.iter().zip(b).all(|(p, q)| (p.x - q.x).abs() < 1e-4 && (p.y - q.y).abs() < 1e-4)
}

impl Config {
    pub fn default_with(work_bottom: f32) -> Self {
        let b = |lo, hi| BandCfg { lo, hi, sensitivity: 1.5 };
        Config {
            audio: AudioCfg {
                source: Source::System,
                bands: Bands {
                    kick: b(30.0, 120.0),
                    bass: b(120.0, 300.0),
                    mid: b(300.0, 2000.0),
                    high: b(2000.0, 8000.0),
                    full: b(30.0, 8000.0),
                },
                min_onset_interval_ms: 120.0,
                auto_interval: true,
                gate_db: DEFAULT_GATE_DB,
            },
            display: DisplayCfg { mode: Mode::Pet, monitor: 0, global_scale: 1.0, background: Background::None },
            motion: MotionCfg { sync: Sync::Onset, assign: Assign::Spread, amount: 0.12, stiffness: 300.0, damping: 16.0, latency_ms: 0.0 },
            lines: vec![Line {
                id: FIRST_LINE.into(),
                ridge: default_ridge(Mode::Pet, work_bottom),
                near: Near::Left,
                far_scale: 0.45,
                overlap: 0.15,
                auto_gap: true,
            }],
            layout: None,
            dolls: Vec::new(),
        }
    }

    pub fn analysis_params(&self) -> Params {
        let b = &self.audio.bands;
        let p = |c: &BandCfg| BandParams { lo: c.lo, hi: c.hi, sensitivity: c.sensitivity };
        Params {
            bands: [p(&b.kick), p(&b.bass), p(&b.mid), p(&b.high), p(&b.full)],
            min_onset_interval_ms: self.audio.min_onset_interval_ms,
            auto_interval: self.audio.auto_interval,
            silence_rms: 10f32.powf(self.audio.gate_db / 20.0),
        }
    }

    /// 값의 범위를 바로잡고, 순서를 다시 매기고, 자동 대역을 배정한다.
    pub fn normalize(&mut self) {
        // 배정 방식·트리거 방식 버튼은 없앴다(2026-11-13): 박자 인형이 모두 박에 맞춰 움직이게 되면서
        // 차이가 거의 없어졌다. 예전 파일의 값은 읽되 늘 따로(spread)·소리마다(onset)로 쓴다.
        self.motion.assign = Assign::Spread;
        self.motion.sync = Sync::Onset;
        let fix = |v: &mut f32, lo: f32, hi: f32, default: f32| {
            *v = if v.is_finite() { v.clamp(lo, hi) } else { default };
        };
        let b = &mut self.audio.bands;
        for c in [&mut b.kick, &mut b.bass, &mut b.mid, &mut b.high, &mut b.full] {
            fix(&mut c.lo, 20.0, 19000.0, 30.0);
            fix(&mut c.hi, 21.0, 20000.0, 8000.0);
            if c.hi <= c.lo {
                c.hi = c.lo + 1.0;
            }
            fix(&mut c.sensitivity, 1.0, 4.0, 1.5);
        }
        fix(&mut self.audio.min_onset_interval_ms, 40.0, 1000.0, 120.0);
        fix(&mut self.display.global_scale, 0.2, 4.0, 1.0);
        fix(&mut self.motion.amount, 0.0, 0.5, 0.12);
        fix(&mut self.motion.stiffness, 40.0, 1200.0, 300.0);
        fix(&mut self.motion.damping, 2.0, 80.0, 16.0);
        fix(&mut self.audio.gate_db, -80.0, -6.0, DEFAULT_GATE_DB);
        fix(&mut self.motion.latency_ms, LATENCY_MIN_MS, LATENCY_MAX_MS, 0.0);

        // 소스: 예전 형식(앱 하나)은 앱 목록으로 옮기고, 이름이 겹치거나 비면 정리한다.
        if let Source::Process { name, .. } = &self.audio.source {
            self.audio.source = Source::Apps { names: vec![name.clone()] };
        }
        if let Source::Apps { names } = &mut self.audio.source {
            let mut seen: Vec<String> = Vec::new();
            names.retain(|n| {
                let key = n.trim().to_lowercase();
                let keep = !key.is_empty() && !seen.contains(&key);
                seen.push(key);
                keep
            });
            if names.is_empty() {
                self.audio.source = Source::System;
            }
        }

        // 예전 형식의 선 하나짜리 설정을 옮기고, 선이 하나는 반드시 있게 한다.
        let legacy = self.layout.take();
        if self.lines.is_empty() {
            self.lines.push(match legacy {
                Some(l) => Line { id: FIRST_LINE.into(), ridge: l.ridge, near: l.near, far_scale: l.far_scale, overlap: l.overlap, auto_gap: false },
                None => Line {
                    id: FIRST_LINE.into(),
                    ridge: default_ridge(Mode::Pet, 0.955),
                    near: Near::Left,
                    far_scale: 0.45,
                    overlap: 0.15,
                    auto_gap: true,
                },
            });
        }
        for (i, line) in self.lines.iter_mut().enumerate() {
            if line.id.is_empty() {
                line.id = format!("line{}", i + 1);
            }
            fix(&mut line.far_scale, 0.1, 1.0, 0.45);
            fix(&mut line.overlap, 0.0, 0.8, 0.15);
            for p in &mut line.ridge {
                fix(&mut p.x, -1.0, 2.0, 0.5);
                fix(&mut p.y, -1.0, 2.0, 0.7);
            }
        }
        let first = self.lines[0].id.clone();
        for d in &mut self.dolls {
            if !self.lines.iter().any(|l| l.id == d.line) {
                d.line = first.clone();
            }
            fix(&mut d.aspect, 0.05, 20.0, 1.0);
            fix(&mut d.x, 0.0, 1.0, 0.5);
            fix(&mut d.y, 0.0, 1.0, 0.8);
            fix(&mut d.scale, 0.005, 1.0, 0.2);
            fix(&mut d.size, 0.2, 4.0, 1.0);
            d.at = d.at.filter(|a| a.is_finite()).map(|a| a.clamp(0.0, 1.0));
            // 예전 형식: 대역 자리에 있던 반응 방식을 따로 옮긴다.
            match d.band {
                BandId::Any => (d.band, d.react) = (BandId::Full, React::Every),
                BandId::Beat => d.react = React::Beat,
                _ => {}
            }
        }
        self.dolls.sort_by_key(|d| d.order);
        for (i, d) in self.dolls.iter_mut().enumerate() {
            d.order = i as u32;
        }
        self.assign_bands();
    }

    /// SPEC 7.2: 줄 순서대로 kick → mid → bass → high 반복. 1개이거나 together면 full.
    fn assign_bands(&mut self) {
        const SPREAD: [BandId; 4] = [BandId::Kick, BandId::Mid, BandId::Bass, BandId::High];
        let visible = self.dolls.iter().filter(|d| d.visible).count();
        let together = self.motion.assign == Assign::Together || visible <= 1;
        let mut i = 0;
        for d in self.dolls.iter_mut().filter(|d| d.visible) {
            if !d.manual.band {
                d.band = if together { BandId::Full } else { SPREAD[i % SPREAD.len()] };
            }
            i += 1;
        }
    }

    /// 모드가 바뀔 때, 능선이 이전 모드의 기본값 그대로면 새 모드의 기본값으로 바꾼다.
    pub fn follow_mode_change(&mut self, old_mode: Mode, work_bottom: f32) {
        // 선을 여러 개 만든 뒤에는 사용자가 직접 배치한 것이므로 건드리지 않는다.
        if old_mode == self.display.mode || self.lines.len() != 1 {
            return;
        }
        let ridge = &mut self.lines[0].ridge;
        let untouched = same_ridge(ridge, &default_ridge(old_mode, work_bottom))
            || (old_mode == Mode::Pet && ridge.iter().all(|p| (p.y - ridge[0].y).abs() < 1e-4 && p.y > 0.85));
        if untouched {
            *ridge = default_ridge(self.display.mode, work_bottom);
        }
    }

    pub fn load(path: &Path) -> Option<Config> {
        let text = std::fs::read_to_string(path).ok()?;
        let mut cfg: Config = serde_json::from_str(&text).ok()?;
        cfg.normalize();
        Some(cfg)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doll(i: u32) -> Doll {
        Doll {
            id: format!("d{i}"),
            image: String::new(),
            aspect: 1.0,
            order: i,
            flip: false,
            visible: true,
            band: BandId::Full,
            react: React::Beat,
            x: 0.5,
            y: 0.5,
            scale: 0.2,
            size: 1.0,
            line: String::new(),
            at: None,
            manual: Manual::default(),
        }
    }

    #[test]
    fn old_single_line_config_is_migrated() {
        let mut c = Config::default_with(0.95);
        c.dolls = (0..2).map(doll).collect();
        c.normalize();
        // 예전 파일: lines·gateDb·size·line이 없고 layout이 있다.
        let mut v = serde_json::to_value(&c).unwrap();
        let o = v.as_object_mut().unwrap();
        o.remove("lines");
        o.insert(
            "layout".into(),
            serde_json::json!({"ridge":[{"x":0.1,"y":0.5},{"x":0.5,"y":0.4},{"x":0.9,"y":0.6}],"near":"right","farScale":0.7,"overlap":0.3}),
        );
        o["audio"].as_object_mut().unwrap().remove("gateDb");
        for d in o["dolls"].as_array_mut().unwrap() {
            d.as_object_mut().unwrap().remove("line");
            d.as_object_mut().unwrap().remove("size");
        }
        let mut old: Config = serde_json::from_value(v).unwrap();
        old.normalize();
        assert_eq!(old.lines.len(), 1);
        assert_eq!(old.lines[0].near, Near::Right);
        assert!((old.lines[0].far_scale - 0.7).abs() < 1e-6 && (old.lines[0].ridge[1].y - 0.4).abs() < 1e-6);
        assert!(old.layout.is_none());
        assert!(old.dolls.iter().all(|d| d.line == old.lines[0].id && d.size == 1.0));
        assert_eq!(old.audio.gate_db, DEFAULT_GATE_DB);
        let saved = serde_json::to_value(&old).unwrap();
        assert!(saved.get("layout").is_none() && saved.get("lines").is_some());
    }

    #[test]
    fn old_single_app_source_becomes_an_app_list() {
        let mut c = Config::default_with(0.95);
        c.audio.source = Source::Process { pid: 7, name: "chrome.exe".into() };
        c.normalize();
        assert_eq!(c.audio.source, Source::Apps { names: vec!["chrome.exe".into()] });
        c.audio.source = Source::Apps { names: vec!["a.exe".into(), "A.EXE".into(), " ".into()] };
        c.normalize();
        assert_eq!(c.audio.source, Source::Apps { names: vec!["a.exe".into()] });
        c.audio.source = Source::Apps { names: vec![] };
        c.normalize();
        assert_eq!(c.audio.source, Source::System);
        // 지연 보정이 없던 파일은 0으로 읽는다.
        let mut v = serde_json::to_value(&c).unwrap();
        v["motion"].as_object_mut().unwrap().remove("latencyMs");
        assert_eq!(serde_json::from_value::<Config>(v).unwrap().motion.latency_ms, 0.0);
    }

    #[test]
    fn reaction_is_its_own_axis_and_line_positions_round_trip() {
        let mut c = Config::default_with(0.95);
        c.dolls = (0..4).map(doll).collect();
        // 예전 형식: 대역 자리에 any/beat
        c.dolls[0].band = BandId::Any;
        c.dolls[0].manual.band = true;
        c.dolls[1].band = BandId::Beat;
        c.dolls[1].manual.band = true;
        c.dolls[1].react = React::Every; // 박 인형은 모든 소리를 고를 수 없다 → 박자로 바로잡힌다
        c.dolls[2].at = Some(1.7);
        c.dolls[3].band = BandId::High;
        c.dolls[3].manual.band = true;
        c.dolls[3].react = React::Every;
        c.normalize();
        assert_eq!((c.dolls[0].band, c.dolls[0].react), (BandId::Full, React::Every));
        assert_eq!((c.dolls[1].band, c.dolls[1].react), (BandId::Beat, React::Beat));
        assert_eq!((c.dolls[3].band, c.dolls[3].react), (BandId::High, React::Every));
        assert_eq!(c.dolls[2].at, Some(1.0));
        let text = serde_json::to_string(&c).unwrap();
        assert!(text.contains("\"react\":\"every\"") && text.contains("\"at\":1.0"));
        assert_eq!(serde_json::from_str::<Config>(&text).unwrap(), c);
        // 반응 방식이 없던 파일은 박자로 읽는다.
        let mut v = serde_json::to_value(&c).unwrap();
        v["dolls"][3].as_object_mut().unwrap().remove("react");
        assert_eq!(serde_json::from_value::<Config>(v).unwrap().dolls[3].react, React::Beat);
    }

    #[test]
    fn dolls_on_a_missing_line_move_to_the_first() {
        let mut c = Config::default_with(0.95);
        c.dolls = vec![doll(0)];
        c.dolls[0].line = "gone".into();
        c.normalize();
        assert_eq!(c.dolls[0].line, c.lines[0].id);
    }

    #[test]
    fn spread_assignment_follows_order_and_keeps_manual() {
        let mut c = Config::default_with(0.95);
        c.dolls = (0..5).map(doll).collect();
        c.dolls[1].manual.band = true;
        c.dolls[1].band = BandId::High;
        c.normalize();
        let bands: Vec<BandId> = c.dolls.iter().map(|d| d.band).collect();
        assert_eq!(bands, [BandId::Kick, BandId::High, BandId::Bass, BandId::High, BandId::Kick]);

        // 예전 파일의 "함께"·"박자 고정"은 읽을 때 따로·소리마다로 바뀐다 (버튼을 없앴다).
        c.motion.assign = Assign::Together;
        c.motion.sync = Sync::Tempo;
        c.normalize();
        assert_eq!((c.motion.assign, c.motion.sync), (Assign::Spread, Sync::Onset));
        let bands: Vec<BandId> = c.dolls.iter().map(|d| d.band).collect();
        assert_eq!(bands, [BandId::Kick, BandId::High, BandId::Bass, BandId::High, BandId::Kick]);
    }

    #[test]
    fn single_doll_follows_full() {
        let mut c = Config::default_with(0.95);
        c.dolls = vec![doll(0)];
        c.normalize();
        assert_eq!(c.dolls[0].band, BandId::Full);
    }

    #[test]
    fn ridge_follows_mode_only_when_untouched() {
        let mut c = Config::default_with(0.95);
        c.display.mode = Mode::Wallpaper;
        c.follow_mode_change(Mode::Pet, 0.95);
        assert!(same_ridge(&c.lines[0].ridge, &HILL));

        c.lines[0].ridge[1].y = 0.4;
        c.display.mode = Mode::Pet;
        c.follow_mode_change(Mode::Wallpaper, 0.95);
        assert!((c.lines[0].ridge[1].y - 0.4).abs() < 1e-6);
    }

    #[test]
    fn config_round_trips_through_json() {
        let mut c = Config::default_with(0.95);
        c.dolls = (0..3).map(doll).collect();
        c.audio.source = Source::Apps { names: vec!["chrome.exe".into(), "spotify.exe".into()] };
        c.display.background = Background::Color { value: "#112233".into() };
        c.normalize();
        let text = serde_json::to_string(&c).unwrap();
        assert!(text.contains("\"minOnsetIntervalMs\"") && text.contains("\"farScale\"") && text.contains("\"globalScale\"") && text.contains("\"gateDb\""));
        assert_eq!(serde_json::from_str::<Config>(&text).unwrap(), c);
    }
}
