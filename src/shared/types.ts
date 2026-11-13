export type BandId = "kick" | "bass" | "mid" | "high" | "full";
export const BAND_IDS: BandId[] = ["kick", "bass", "mid", "high", "full"];

export const BAND_COLORS: Record<BandId, string> = {
  kick: "#ff5d5d",
  bass: "#ff9f43",
  mid: "#3ddc97",
  high: "#4aa8ff",
  full: "#b58cff",
};

export const BAND_LABELS: Record<BandId, string> = {
  kick: "Kick",
  bass: "Bass",
  mid: "Mid",
  high: "High",
  full: "Full",
};

/** 인형이 따르는 대역: 분석 대역 + 인형 전용 "beat"(추정된 박마다, 반응 방식은 늘 박자). */
export type DollBand = BandId | "beat";
export const DOLL_BANDS: DollBand[] = [...BAND_IDS, "beat"];
export const DOLL_BAND_COLORS: Record<DollBand, string> = { ...BAND_COLORS, beat: "#ffd166" };
export const DOLL_BAND_LABELS: Record<DollBand, string> = { ...BAND_LABELS, beat: "Beat" };

/**
 * 인형이 자기 대역의 소리에 반응하는 방식 (대역과 따로 정한다).
 *  - beat "박자": 박을 예측하며 돌고, 박에 맞는 소리만 받는다. 대역끼리 박을 맞춘다. (기본)
 *  - every "모든 소리": 소리가 날 때마다 바로 (박 거르기 없이, 촘촘히). 점선 테두리로 보여 준다.
 */
export type React = "beat" | "every";
export const REACTS: React[] = ["beat", "every"];
export const REACT_LABELS: Record<React, string> = { beat: "Beat", every: "Every sound" };

/** 인형으로 넣을 수 있는 이미지 확장자 (Rust의 images::EXTENSIONS와 같다). */
export const DOLL_EXTENSIONS = [
  "png", "webp", "gif", "jpg", "jpeg", "jfif", "jpe", "bmp", "dib", "tif", "tiff", "ico", "tga", "qoi", "dds", "pbm", "pgm", "ppm", "pnm",
];

export type Pt = { x: number; y: number };

/** system: 시스템 전체. apps: 고른 앱들(실행 파일 이름)의 소리만 합쳐서. */
export type AudioSource = { kind: "system" } | { kind: "apps"; names: string[] };

export type Background =
  | { kind: "none" }
  | { kind: "color"; value: string }
  | { kind: "image"; path: string };

export type Doll = {
  id: string;
  image: string; // images 폴더 안의 파일명
  aspect: number; // 잘라낸 이미지의 너비 / 높이
  order: number;
  flip: boolean;
  visible: boolean;
  band: DollBand;
  react: React;
  x: number;
  y: number;
  scale: number; // 인형 높이 / 화면 높이 (수동 배치일 때)
  size: number; // 자동 배치 안에서의 개별 크기 배율 (기본 1)
  line: string; // 속한 선의 id
  /** 선 위에서 직접 정한 자리 (선 길이 비율, 0 = 왼쪽 끝). 없으면 자동 줄 세우기. */
  at?: number | null;
  manual: { layout: boolean; band: boolean };
};

export type LayoutCfg = {
  ridge: [Pt, Pt, Pt];
  near: "left" | "right";
  farScale: number;
  overlap: number;
  /** 간격 자동: 줄이 선 전체에 고르게 퍼지도록 간격을 정한다 (overlap 대신). */
  autoGap?: boolean;
};

/** 인형이 앉는 선 하나. 원근·겹침 같은 속성은 선마다 따로 갖는다. */
export type Line = LayoutCfg & { id: string };

export type Config = {
  audio: {
    source: AudioSource;
    bands: Record<BandId, { lo: number; hi: number; sensitivity: number }>;
    minOnsetIntervalMs: number;
    /** 간격 자동: 박에 맞춰 최소 온셋 간격을 정한다 (minOnsetIntervalMs 대신). */
    autoInterval?: boolean;
    gateDb: number; // 역치: 이보다 작은 입력(dBFS)에는 반응하지 않는다
  };
  display: {
    mode: "pet" | "wallpaper";
    monitor: number;
    globalScale: number;
    background: Background;
  };
  motion: {
    sync: "onset" | "tempo";
    assign: "spread" | "together";
    amount: number;
    stiffness: number;
    damping: number;
    /** 화면 지연 보정(ms). 양수면 인형이 늦게, 음수면 일찍 움직인다. */
    latencyMs: number;
  };
  lines: Line[];
  dolls: Doll[];
};

export type AudioFrame = {
  t: number;
  bands: Record<BandId, { energy: number; onset: boolean; strength: number; active: boolean }>;
  tempo: { bpm: number; beat: boolean } | null;
  silent: boolean;
  level: number; // 입력 크기 (dBFS)
  gapMs?: number; // 지금 쓰는 최소 온셋 간격 (간격 자동이면 박에 따라 바뀐다)
};

export type Runtime = {
  editMode: boolean;
  audioError: string | null;
  displayError: string | null;
};

export const LATENCY_MIN_MS = -200;
export const LATENCY_MAX_MS = 300;

export type MonitorInfo = {
  name: string;
  width: number;
  height: number;
  workBottom: number; // 작업 영역 아래쪽 끝 / 화면 높이 (0..1)
};

export type AudioSession = { pid: number; name: string; display: string };

export type ConfigChanged = { config: Config; origin: string };
