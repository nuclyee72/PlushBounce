import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, onAudioFrame, onConfigChanged, onRuntimeChanged } from "../shared/api";
import { BAND_IDS, DOLL_EXTENSIONS } from "../shared/types";
import type { AudioFrame, AudioSession, BandId, Config, MonitorInfo, Runtime } from "../shared/types";
import type { StageRenderer } from "../stage/renderer";

const zero = (): Record<BandId, number> => ({ kick: 0, bass: 0, mid: 0, high: 0, full: 0 });

export const app = $state({
  cfg: null as Config | null,
  runtime: { editMode: false, audioError: null, displayError: null } as Runtime,
  imagesDir: "",
  monitors: [] as MonitorInfo[],
  sessions: [] as AudioSession[],
  error: null as string | null,
  selected: null as string | null,
  /** 지금 다루는 선의 id. 없으면 첫 번째 선. */
  line: null as string | null,
  tab: "dolls" as "dolls" | "audio" | "display",
  dropping: false,
});

/** 실시간 미터 값. flash는 온셋 직후 1에서 0으로 줄어든다. */
export const live = $state({ level: zero(), flash: zero(), bpm: null as number | null, beat: 0, input: -100, gapMs: null as number | null });

/** 최근 분석 기록 (타임라인용). 매 프레임 바뀌므로 반응형이 아닌 일반 객체로 둔다. 시각은 performance.now() ms. */
export const HISTORY_MS = 8000;
export const history = {
  onsets: { kick: [], bass: [], mid: [], high: [], full: [] } as Record<BandId, { t: number; s: number }[]>,
  beats: [] as number[],
  /** 대역별 에너지 (0..1) 표본. */
  energy: [] as { t: number; e: Record<BandId, number> }[],
  /** 역치 아래(무음)였던 구간의 시작·끝. */
  silent: [] as { from: number; to: number }[],
};

function record(f: AudioFrame, now: number): void {
  const old = now - HISTORY_MS;
  for (const b of BAND_IDS) {
    const list = history.onsets[b];
    if (f.bands[b].onset) list.push({ t: now, s: f.bands[b].strength });
    while (list.length && list[0].t < old) list.shift();
  }
  if (f.tempo?.beat) history.beats.push(now);
  while (history.beats.length && history.beats[0] < old) history.beats.shift();
  const e = zero();
  for (const b of BAND_IDS) e[b] = f.silent ? 0 : f.bands[b].energy;
  history.energy.push({ t: now, e });
  while (history.energy.length && history.energy[0].t < old) history.energy.shift();
  const last = history.silent[history.silent.length - 1];
  if (f.silent) {
    if (last && now - last.to < 200) last.to = now;
    else history.silent.push({ from: now, to: now });
  }
  while (history.silent.length && history.silent[0].to < old) history.silent.shift();
}

let renderer: StageRenderer | null = null;
export const setRenderer = (r: StageRenderer | null) => (renderer = r);
export const getRenderer = () => renderer;

export function fail(e: unknown): void {
  app.error = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

let seq = 0;
function send(next: Config): void {
  const mine = ++seq;
  api
    .setConfig(next)
    .then((saved) => {
      // 그 사이 다른 변경이 없었을 때만 Rust가 정리한 값(자동 대역 등)을 받아들인다.
      if (mine === seq) app.cfg = saved;
    })
    .catch(fail);
}

/** 설정을 바꾸고 즉시 반영한다. */
export function commit(mutate: (c: Config) => void): void {
  if (!app.cfg) return;
  const next = $state.snapshot(app.cfg) as Config;
  mutate(next);
  app.cfg = next;
  send(next);
}

export function commitWhole(next: Config): void {
  app.cfg = next;
  send(next);
}

export async function refreshSessions(): Promise<void> {
  try {
    app.sessions = await api.sessions();
  } catch (e) {
    fail(e);
  }
}

export async function addDolls(paths: string[]): Promise<void> {
  const images = paths.filter((p) => DOLL_EXTENSIONS.includes(p.split(".").pop()?.toLowerCase() ?? ""));
  if (images.length === 0) {
    if (paths.length) fail("이미지 파일(PNG, JPG, WebP, BMP, GIF 등)을 넣어 주세요.");
    return;
  }
  try {
    app.cfg = await api.addDolls(images);
  } catch (e) {
    fail(e);
  }
}

export async function init(): Promise<void> {
  const [cfg, runtime, dir, monitors] = await Promise.all([
    api.getConfig(),
    api.getRuntime(),
    api.imagesDir(),
    api.monitors(),
  ]);
  app.imagesDir = dir;
  app.monitors = monitors;
  app.runtime = runtime;
  app.cfg = cfg;
  void refreshSessions();

  await onConfigChanged((c) => {
    if (c.origin !== "control") {
      seq++; // 진행 중이던 내 응답은 더 오래된 값이므로 버린다.
      app.cfg = c.config;
    }
  });
  await onRuntimeChanged((r) => {
    app.runtime = r;
  });

  let latest: AudioFrame | null = null;
  const onsetAt = zero();
  let beatAt = 0;
  await onAudioFrame((f) => {
    renderer?.onFrame(f);
    const now = performance.now();
    for (const b of BAND_IDS) if (f.bands[b].onset) onsetAt[b] = now;
    if (f.tempo?.beat) beatAt = now;
    record(f, now);
    latest = f;
  });
  const tick = () => {
    requestAnimationFrame(tick);
    const now = performance.now();
    for (const b of BAND_IDS) {
      const level = latest && !latest.silent ? latest.bands[b].energy : 0;
      if (Math.abs(live.level[b] - level) > 0.004) live.level[b] = level;
      const flash = Math.max(0, 1 - (now - onsetAt[b]) / 220);
      if (live.flash[b] !== flash) live.flash[b] = flash;
    }
    const bpm = latest?.tempo ? Math.round(latest.tempo.bpm) : null;
    if (live.bpm !== bpm) live.bpm = bpm;
    const gap = latest?.gapMs != null ? Math.round(latest.gapMs) : null;
    if (live.gapMs !== gap) live.gapMs = gap;
    // 입력 크기는 빨리 오르고 천천히 내려가게 보여 준다 (dB).
    const input = Math.max(latest ? latest.level : -100, live.input - 1.2);
    if (Math.abs(live.input - input) > 0.3) live.input = input;
    const beat = Math.max(0, 1 - (now - beatAt) / 220);
    if (live.beat !== beat) live.beat = beat;
  };
  requestAnimationFrame(tick);

  await getCurrentWebview().onDragDropEvent((e) => {
    const p = e.payload;
    if (p.type === "enter" || p.type === "over") app.dropping = true;
    else if (p.type === "leave") app.dropping = false;
    else if (p.type === "drop") {
      app.dropping = false;
      app.tab = "dolls";
      void addDolls(p.paths);
    }
  });
}
