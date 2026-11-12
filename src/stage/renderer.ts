import type { AudioFrame, BandId, Config, Doll, DollBand, Line, Pt } from "../shared/types";
import { BAND_IDS, DOLL_BAND_COLORS } from "../shared/types";
import { BandCorrector } from "./assign";
import { DollSprite } from "./doll";
import { atOnRidge, autoLayout, bezier, nearestOnRidge, type Placed } from "./layout";
import { shouldBounce, type BeatClock } from "./trigger";

export type RendererOpts = {
  imageUrl: (file: string) => string;
  /** 편집으로 설정이 바뀔 때 호출된다. */
  onEdit?: (cfg: Config) => void;
  /** 미리보기용: 투명 배경 대신 어두운 바탕을 깔고 대역 색 점을 그린다. */
  preview?: boolean;
};

type Handle =
  | { kind: "ridge"; line: string; index: 0 | 1 | 2 }
  /** 선 몸통: 끌면 선 전체가 옮겨진다. (x, y)는 직전 포인터 위치(화면 비율). */
  | { kind: "line"; line: string; x: number; y: number }
  | { kind: "size"; id: string }
  /** snap: 지금 달라붙어 있는 선의 id (놓으면 그 선의 줄에 들어간다). */
  | { kind: "doll"; id: string; dx: number; dy: number; snap: string | null; sx: number; sy: number; moved?: boolean };

const HANDLE_R = 9;
/** 선 몸통을 잡을 수 있는 거리(px). */
const LINE_GRAB_PX = 9;
/** 끝점 높이가 이만큼(px) 안이면 수평으로, 가운데가 이만큼 안이면 곧은 선으로 맞춘다. */
const LEVEL_SNAP_PX = 7;
/** 끌던 인형의 바닥이 선에서 이만큼(화면 높이 대비) 안에 오면 선에 달라붙는다. */
const SNAP_DIST = 0.04;
/** 이미 선에 붙어 있으면 이만큼 멀어져야 떨어진다 (선을 따라 좌우로 끌 때 미끄러지듯). */
const SNAP_KEEP = 0.08;
/**
 * 박자 인형의 박 맞추기 (사용자 요청: 이전 박을 반영한 보정을 크게).
 * 박이 잡혀 있으면 박(중음·고음은 반박) 격자에서 이만큼(박 단위, 최대 GRID_ALIGN_MAX_MS) 안의 소리를
 * 격자 위에서 들은 것으로 친다 → 대역이 달라도 같은 박에 함께 뛴다.
 */
const GRID_ALIGN_BEATS = 0.2;
const GRID_ALIGN_MAX_MS = 110;
/** 박을 아직 모를 때: 다른 대역의 소리가 이만큼(ms) 안에 함께 오면 같은 타격으로 본다. */
const CLUSTER_MS = 80;
/** "모든 소리" 인형이 다시 움직이기까지의 최소 간격(ms). */
const PULSE_GAP_MS = 70;
/** 끄는 동안 설정을 내보내는 간격(ms). 매 프레임 보내면 다른 창들이 버벅인다. */
const EDIT_SEND_MS = 70;
/** 그려지는 자리가 실제 자리를 따라가는 시간(초): 끄는 인형 / 나머지. */
const FOLLOW_DRAG_S = 0.045;
const FOLLOW_S = 0.11;
const IDLE_AFTER_MS = 2000;

export class StageRenderer {
  private ctx: CanvasRenderingContext2D;
  private cfg: Config | null = null;
  private deferredCfg: Config | null = null;
  private sprites = new Map<string, DollSprite>();
  private placed = new Map<string, Placed>();
  private drawOrder: Doll[] = [];
  // 곡이 바뀌면 3초 안에 새 곡에 맞춰 대역을 다시 고른다.
  private corrector = new BandCorrector(3000);
  private bands = new Map<string, DollBand>();
  /** 화면에 그려지는 자리. 실제 자리(placed)를 부드럽게 따라간다. */
  private shown = new Map<string, Placed>();
  private lastEditSent = 0;
  /** 가장 최근 타격 묶음 (대역 간 박 맞추기). */
  private lastHit: { t: number; band: BandId } | null = null;
  private pending: Record<BandId, number> = { kick: 0, bass: 0, mid: 0, high: 0, full: 0 };
  private editing = false;
  private drag: Handle | null = null;
  private hover: Handle | null = null;
  /** 지금 끌기에서 수평/곧은 선 맞춤이 걸렸는가 (안내선 표시용). */
  private leveled = false;
  private editDirty = false;
  private bg: HTMLImageElement | null = null;
  private bgUrl = "";
  private w = 1;
  private h = 1;
  private raf = 0;
  private lastT = 0;
  private lastDraw = 0;
  private silentSince: number | null = performance.now();
  private lastFrameAt = 0;
  private dirty = true;
  private lastDemo = 0;
  private clock: BeatClock = { period: null, lastBeat: 0 };
  private ro: ResizeObserver;
  private dead = false;
  selectedId: string | null = null;
  onSelect: ((id: string | null) => void) | null = null;
  /** 지금 다루는 선 (조작 창의 선 설정이 가리키는 선). */
  selectedLine: string | null = null;
  onSelectLine: ((id: string) => void) | null = null;

  constructor(
    private canvas: HTMLCanvasElement,
    private opts: RendererOpts,
  ) {
    this.ctx = canvas.getContext("2d")!;
    this.ro = new ResizeObserver(() => this.resize());
    this.ro.observe(canvas);
    this.resize();
    canvas.addEventListener("pointerdown", this.onDown);
    canvas.addEventListener("pointermove", this.onMove);
    canvas.addEventListener("pointerup", this.onUp);
    canvas.addEventListener("pointercancel", this.onUp);
    canvas.addEventListener("wheel", this.onWheel, { passive: false });
    canvas.addEventListener("pointerleave", this.onLeave);
    canvas.addEventListener("keydown", this.onKey);
    if (canvas.tabIndex < 0) canvas.tabIndex = 0;
    this.raf = requestAnimationFrame(this.tick);
  }

  destroy(): void {
    this.dead = true;
    cancelAnimationFrame(this.raf);
    this.ro.disconnect();
    const c = this.canvas;
    c.removeEventListener("pointerdown", this.onDown);
    c.removeEventListener("pointermove", this.onMove);
    c.removeEventListener("pointerup", this.onUp);
    c.removeEventListener("pointercancel", this.onUp);
    c.removeEventListener("wheel", this.onWheel);
    c.removeEventListener("pointerleave", this.onLeave);
    c.removeEventListener("keydown", this.onKey);
  }

  setConfig(cfg: Config): void {
    if (this.drag) {
      this.deferredCfg = cfg;
      return;
    }
    this.cfg = structuredClone(cfg);
    const ids = new Set(cfg.dolls.map((d) => d.id));
    for (const id of [...this.sprites.keys()]) {
      if (!ids.has(id)) {
        this.sprites.delete(id);
        this.shown.delete(id);
      }
    }
    for (const d of cfg.dolls) {
      let s = this.sprites.get(d.id);
      if (!s) this.sprites.set(d.id, (s = new DollSprite()));
      s.setImage(this.opts.imageUrl(d.image), () => (this.dirty = true));
    }
    const bg = cfg.display.background;
    const url = bg.kind === "image" && bg.path ? this.opts.imageUrl(bg.path) : "";
    if (url !== this.bgUrl) {
      this.bgUrl = url;
      this.bg = null;
      if (url) {
        const img = new Image();
        img.onload = () => {
          if (this.bgUrl === url) {
            this.bg = img;
            this.dirty = true;
          }
        };
        img.src = url;
      }
    }
    this.relayout();
  }

  /** 인형이 지금 놓여 있는 자리 (자동 배치 결과 포함). */
  getPlaced(id: string): Placed | undefined {
    return this.placed.get(id);
  }

  /**
   * 설정을 만지는 동안 결과를 보여 주기 위해 모든 인형을 한 번 튀긴다.
   * 꿍실 한 번이 끝난 뒤에만 다시 튀기므로, 슬라이더를 끄는 동안 계속 불러도 된다.
   */
  demo(): void {
    const now = performance.now();
    if (now - this.lastDemo < 500) return;
    this.lastDemo = now;
    for (const d of this.drawOrder) this.sprites.get(d.id)?.bounce(1, 0.5, now);
  }

  /** 지금 박 간격(초). 박을 모르면 null. */
  private periodS(): number | null {
    return this.clock.period ? this.clock.period / 1000 : null;
  }

  select(id: string | null): void {
    if (this.selectedId !== id) {
      this.selectedId = id;
      this.dirty = true;
    }
  }

  /** 지금 다루는 선 (조작 창에서 고른 선). null이면 첫 번째 선. */
  selectLine(id: string | null): void {
    if (this.selectedLine !== id) {
      this.selectedLine = id;
      this.dirty = true;
    }
  }

  setEditing(on: boolean): void {
    this.editing = on;
    this.drag = null;
    if (!on) this.selectedId = null;
    this.canvas.style.cursor = "default";
    this.dirty = true;
  }

  onFrame(f: AudioFrame): void {
    const cfg = this.cfg;
    if (!cfg) return;
    const now = performance.now();
    this.lastFrameAt = now;
    if (f.silent) {
      this.silentSince ??= now;
      // 소리가 끊기면(0.15초 넘게) 다음 박을 기다리지 않고 바로 잦아든다.
      // 설정을 만지는 동안의 시연 꿍실은 끊지 않는다.
      if (now - this.silentSince > 150 && now - this.lastDemo > 1200) {
        for (const s of this.sprites.values()) s.spring.hush();
      }
    } else this.silentSince = null;

    const active = {} as Record<BandId, boolean>;
    for (const b of BAND_IDS) active[b] = f.bands[b].active;
    this.bands = this.corrector.update(
      this.drawOrder.map((d) => ({ id: d.id, band: d.band, manualBand: d.manual.band })),
      active,
      now,
    );

    // 박 시계: 박이 온 시각과 박 간격. 온셋을 박 격자에 맞춰 거르는 데 쓴다.
    if (f.tempo) {
      this.clock.period = 60000 / f.tempo.bpm;
      if (f.tempo.beat) this.clock.lastBeat = now;
    } else {
      this.clock.period = null;
    }

    const lag = (cfg.motion.latencyMs ?? 0) / 1000;
    const period = this.periodS();
    const tempoMode = cfg.motion.sync === "tempo" && f.tempo !== null;
    for (const b of BAND_IDS) {
      if (f.bands[b].onset) this.pending[b] = Math.max(this.pending[b], f.bands[b].strength);
    }

    for (const d of this.drawOrder) {
      const s = this.sprites.get(d.id);
      if (!s) continue;
      const want = this.bands.get(d.id) ?? d.band;
      if (want === "beat") {
        // 박: 추정된 박마다 (대역과 상관없이). 박을 아직 모르면 전체 소리를 박처럼 걸러서 따른다.
        if (f.tempo) {
          if (f.tempo.beat) s.bounce(Math.max(0.7, this.pending.full), period, now, 0, lag);
        } else if (f.bands.full.onset && shouldBounce(now, s.acceptedAt, "full", f.bands.full.strength, this.clock)) {
          s.bounce(f.bands.full.strength, null, now, this.align("full", now), lag);
        }
        continue;
      }
      const band: BandId = want;
      const b = f.bands[band];
      if (d.react === "every") {
        // 모든 소리: 자기 대역의 소리가 날 때마다 박을 따지지 않고 바로, 아주 촘촘히.
        if (b.onset && now - s.acceptedAt >= PULSE_GAP_MS) s.hit(b.strength, now, lag);
        continue;
      }
      if (tempoMode) {
        // 박자 고정: 박에만. 전체 인형은 매 박, 나머지는 직전 한 박 안에 자기 소리가 있었을 때.
        if (f.tempo!.beat && (band === "full" || this.pending[band] > 0)) s.bounce(this.pending[band], period, now, 0, lag);
        continue;
      }
      // 박자: 자기 대역의 소리에 반응하되, 박이 잡혀 있으면 박에 맞는 소리에만 한 박에 한 번.
      if (b.onset && shouldBounce(now, s.acceptedAt, band, b.strength, this.clock)) {
        s.bounce(b.strength, period, now, this.align(band, now), lag);
      }
    }
    if (!tempoMode || f.tempo!.beat) for (const b of BAND_IDS) this.pending[b] = 0;
  }

  /**
   * 박자 인형의 박 맞추기. 돌려주는 값: 이 소리를 몇 초 앞서 들은 것으로 칠지 (음수 = 곧 올 박에 속함).
   *  - 박이 잡혀 있으면: 지금까지의 박으로 만든 격자(중음·고음은 반박 격자)에서 0.2박(최대 110ms) 안의 소리는
   *    격자 위에서 들은 것으로 친다. 대역마다 감지가 몇십 ms씩 달라도 모든 인형이 같은 박에 함께 뛴다.
   *  - 박을 모르면: 다른 대역의 소리가 80ms 안에 먼저 왔을 때 같은 타격으로 보고 그 순간에 맞춘다.
   * 그보다 멀리 떨어진 소리(엇박 하이햇 등)는 그대로 둔다.
   */
  private align(band: BandId, now: number): number {
    const { period: p, lastBeat } = this.clock;
    if (p && lastBeat > 0) {
      const grid = band === "mid" || band === "high" ? 0.5 : 1;
      const pos = (now - lastBeat) / p / grid;
      const off = (pos - Math.round(pos)) * grid * p;
      if (Math.abs(off) <= Math.min(GRID_ALIGN_BEATS * p, GRID_ALIGN_MAX_MS)) return off / 1000;
      return 0;
    }
    const h = this.lastHit;
    if (h && now - h.t <= CLUSTER_MS) return h.band === band ? 0 : (now - h.t) / 1000;
    this.lastHit = { t: now, band };
    return 0;
  }

  // ───────── 배치 ─────────

  private resize(): void {
    const dpr = window.devicePixelRatio || 1;
    const r = this.canvas.getBoundingClientRect();
    this.w = Math.max(1, r.width);
    this.h = Math.max(1, r.height);
    const pw = Math.round(this.w * dpr);
    const ph = Math.round(this.h * dpr);
    if (this.canvas.width !== pw || this.canvas.height !== ph) {
      this.canvas.width = pw;
      this.canvas.height = ph;
    }
    this.relayout();
  }

  private relayout(): void {
    const cfg = this.cfg;
    this.dirty = true;
    if (!cfg) return;
    const visible = cfg.dolls.filter((d) => d.visible).sort((a, b) => a.order - b.order);
    // 선마다 그 선에 속한 인형을 따로 줄 세운다 (원근·겹침은 선의 속성).
    this.placed = new Map();
    for (const line of cfg.lines) {
      const auto = visible.filter((d) => !d.manual.layout && d.line === line.id);
      const placed = autoLayout(
        auto.map((d) => ({ id: d.id, aspect: d.aspect, size: d.size, at: d.at })),
        line,
        this.w / this.h,
        cfg.display.globalScale,
      );
      for (const [id, p] of placed) this.placed.set(id, p);
    }
    for (const d of visible) {
      if (d.manual.layout) this.placed.set(d.id, { x: d.x, y: d.y, scale: d.scale });
    }
    // 큰(가까운) 인형이 앞에 오도록 작은 것부터 그린다.
    this.drawOrder = visible.sort(
      (a, b) => this.placed.get(a.id)!.scale - this.placed.get(b.id)!.scale || b.order - a.order,
    );
  }

  // ───────── 그리기 ─────────

  private tick = (t: number): void => {
    if (this.dead) return;
    this.raf = requestAnimationFrame(this.tick);
    const cfg = this.cfg;
    if (!cfg) return;
    const dt = this.lastT ? (t - this.lastT) / 1000 : 0;
    this.lastT = t;

    const now = performance.now();
    // 프레임이 오지 않으면(오디오 오류 등) 무음으로 본다.
    if (now - this.lastFrameAt > 500) this.silentSince ??= now;
    const silentFor = this.silentSince === null ? 0 : now - this.silentSince;

    let moving = false;
    const lag = (cfg.motion.latencyMs ?? 0) / 1000;
    const dragged = this.drag?.kind === "doll" ? this.drag.id : null;
    for (const d of this.drawOrder) {
      const s = this.sprites.get(d.id);
      if (!s) continue;
      s.spring.lag = lag;
      s.step(dt);
      if (!s.still) moving = true;
      // 그려지는 자리는 실제 자리를 부드럽게 따라간다 (끌기·선에 붙기·줄 다시 세우기가 미끄러지듯).
      const target = this.placed.get(d.id);
      if (!target) continue;
      const cur = this.shown.get(d.id);
      if (!cur) {
        this.shown.set(d.id, { ...target });
        continue;
      }
      const far = Math.abs(target.x - cur.x) + Math.abs(target.y - cur.y) + Math.abs(target.scale - cur.scale);
      if (far < 2e-4 || dt <= 0) {
        if (far < 2e-4) Object.assign(cur, target);
        continue;
      }
      const k = 1 - Math.exp(-dt / (d.id === dragged ? FOLLOW_DRAG_S : FOLLOW_S));
      cur.x += (target.x - cur.x) * k;
      cur.y += (target.y - cur.y) * k;
      cur.scale += (target.scale - cur.scale) * k;
      moving = true;
    }
    if (this.editDirty && this.cfg && (!this.drag || now - this.lastEditSent >= EDIT_SEND_MS)) {
      this.editDirty = false;
      this.lastEditSent = now;
      this.opts.onEdit?.(structuredClone(this.cfg));
    }

    // 무음 + 정지 상태에서는 프레임을 낮춘다.
    // 무음이 이어지면 숨쉬기(idle) 동작이 있으므로 그 동안도 매 프레임 그린다 (예전엔 15fps라 뚝뚝 끊겨 보였다).
    // 정말 아무것도 움직이지 않는 동안(소리가 막 끊긴 뒤 숨쉬기가 시작되기 전)만 프레임을 낮춘다.
    const breathing = silentFor > IDLE_AFTER_MS;
    const busy = moving || breathing || this.dirty || this.drag !== null || silentFor === 0;
    if (!busy && t - this.lastDraw < 66) return;
    this.lastDraw = t;
    this.dirty = false;
    this.draw(cfg, now, silentFor);
  };

  private draw(cfg: Config, now: number, silentFor: number): void {
    const ctx = this.ctx;
    const dpr = this.canvas.width / this.w;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, this.w, this.h);
    this.drawBackground(cfg);
    if (this.editing && !this.opts.preview) {
      ctx.fillStyle = "rgba(0,0,0,0.18)";
      ctx.fillRect(0, 0, this.w, this.h);
    }

    const idle = Math.min(1, Math.max(0, (silentFor - IDLE_AFTER_MS) / 1500));
    this.drawOrder.forEach((d, i) => {
      const p = this.shown.get(d.id) ?? this.placed.get(d.id);
      const s = this.sprites.get(d.id);
      if (!p || !s) return;
      const breath = idle > 0 ? idle * 0.22 * Math.sin(now / 560 + i * 0.9) : 0;
      const px = p.x * this.w;
      const py = p.y * this.h;
      s.draw(ctx, px, py, p.scale * this.h, d.aspect, d.flip, s.x + breath, cfg.motion.amount);
      if (this.opts.preview) {
        // 깜빡임도 타이밍 보정만큼 늦게 (flashAt이 미래일 수 있다).
        const flash = now < s.flashAt ? 0 : Math.max(0, 1 - (now - s.flashAt) / 180);
        const color = DOLL_BAND_COLORS[this.bands.get(d.id) ?? d.band];
        ctx.beginPath();
        ctx.arc(px, py + 5, 2.5 + flash * 2.5, 0, Math.PI * 2);
        ctx.globalAlpha = 0.55 + 0.45 * flash;
        // "모든 소리" 인형은 속이 빈 고리로 (조작 창의 점선 테두리와 같은 뜻).
        if (d.react === "every") {
          ctx.lineWidth = 1.5;
          ctx.strokeStyle = color;
          ctx.stroke();
        } else {
          ctx.fillStyle = color;
          ctx.fill();
        }
        ctx.globalAlpha = 1;
      }
    });
    if (this.editing) {
      this.drawHandles(cfg);
      this.drawSelection();
    }
  }

  private drawBackground(cfg: Config): void {
    const ctx = this.ctx;
    const bg = cfg.display.background;
    const wallpaper = cfg.display.mode === "wallpaper";
    if (this.opts.preview) {
      const g = ctx.createLinearGradient(0, 0, 0, this.h);
      g.addColorStop(0, "#2b3040");
      g.addColorStop(1, "#1b1e29");
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, this.w, this.h);
    }
    if (!wallpaper) return;
    if (bg.kind === "color") {
      ctx.fillStyle = bg.value;
      ctx.fillRect(0, 0, this.w, this.h);
    } else if (bg.kind === "image" && this.bg) {
      const s = Math.max(this.w / this.bg.width, this.h / this.bg.height);
      const dw = this.bg.width * s;
      const dh = this.bg.height * s;
      ctx.drawImage(this.bg, (this.w - dw) / 2, (this.h - dh) / 2, dw, dh);
    }
  }

  // ───────── 편집 핸들 ─────────

  /** 선의 핸들 3개 (픽셀): 양 끝점과 곡선의 중점. */
  private handlePoints(line: Line): [Pt, Pt, Pt] {
    const r = line.ridge;
    const px = (p: Pt): Pt => ({ x: p.x * this.w, y: p.y * this.h });
    return [px(r[0]), px(bezier(r, 0.5)), px(r[2])];
  }

  /** 선택한 인형의 상자 (픽셀). 오른쪽 위 모서리가 크기 핸들이다. */
  private selectionBox(): { doll: Doll; left: number; top: number; w: number; h: number; bottom: number } | null {
    const doll = this.drawOrder.find((d) => d.id === this.selectedId);
    const p = doll && (this.shown.get(doll.id) ?? this.placed.get(doll.id));
    if (!doll || !p) return null;
    const h = p.scale * this.h;
    const w = h * doll.aspect;
    return { doll, left: p.x * this.w - w / 2, top: p.y * this.h - h, w, h, bottom: p.y * this.h };
  }

  private drawSelection(): void {
    const box = this.selectionBox();
    if (!box) return;
    const ctx = this.ctx;
    ctx.strokeStyle = "rgba(255,255,255,0.85)";
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 3]);
    ctx.strokeRect(box.left, box.top, box.w, box.h);
    ctx.setLineDash([]);
    // 크기 핸들: 대각선 화살표가 그려진 둥근 손잡이.
    const x = box.left + box.w;
    const y = box.top;
    ctx.beginPath();
    ctx.arc(x, y, 8, 0, Math.PI * 2);
    ctx.fillStyle = "#ffffff";
    ctx.fill();
    ctx.lineWidth = 2;
    ctx.strokeStyle = "rgba(20,22,30,0.9)";
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(x - 3, y + 3);
    ctx.lineTo(x + 3, y - 3);
    ctx.moveTo(x, y - 3);
    ctx.lineTo(x + 3, y - 3);
    ctx.lineTo(x + 3, y);
    ctx.moveTo(x - 3, y);
    ctx.lineTo(x - 3, y + 3);
    ctx.lineTo(x, y + 3);
    ctx.lineWidth = 1.5;
    ctx.stroke();
  }

  /** 인형 하나의 크기를 화면상 높이 newH(px)로 맞춘다. 자동 배치 인형은 줄에 남은 채 배율만 바뀐다. */
  private resizeDoll(doll: Doll, newH: number): void {
    const placed = this.placed.get(doll.id);
    if (!placed) return;
    const cur = Math.max(1, placed.scale * this.h);
    const target = Math.min(this.h, Math.max(12, newH));
    if (doll.manual.layout) doll.scale = Math.min(1, Math.max(0.01, target / this.h));
    else doll.size = Math.min(4, Math.max(0.2, (doll.size || 1) * (target / cur)));
    this.editDirty = true;
    this.relayout();
  }

  private drawHandles(cfg: Config): void {
    const ctx = this.ctx;
    const snapped = this.drag?.kind === "doll" ? this.drag.snap : null;
    const active = this.drag ?? this.hover;
    const activeLine = active && (active.kind === "ridge" || active.kind === "line") ? active.line : null;
    for (const line of cfg.lines) {
      const r = line.ridge;
      // 인형이 달라붙은 선은 노랗게, 잡을 수 있는(가리킨) 선은 굵게 보여 준다.
      const hot = line.id === snapped;
      const touched = line.id === activeLine;
      const many = cfg.lines.length > 1;
      const current = many && line.id === (this.selectedLine ?? cfg.lines[0].id);
      ctx.lineWidth = hot ? 4 : touched ? 3.5 : current ? 2.5 : 2;
      ctx.strokeStyle = hot ? "#ffd166" : touched || current ? "#ffffff" : many ? "rgba(255,255,255,0.45)" : "rgba(255,255,255,0.7)";
      if (touched) {
        ctx.shadowColor = "rgba(0,0,0,0.5)";
        ctx.shadowBlur = 6;
      }
      ctx.beginPath();
      ctx.moveTo(r[0].x * this.w, r[0].y * this.h);
      ctx.quadraticCurveTo(r[1].x * this.w, r[1].y * this.h, r[2].x * this.w, r[2].y * this.h);
      ctx.stroke();
      ctx.shadowBlur = 0;

      // 수평·곧은 선에 맞춰졌으면 점선 안내선을 길게 그어 알려 준다.
      if (touched && this.leveled && this.drag) {
        const a = { x: r[0].x * this.w, y: r[0].y * this.h };
        const b = { x: r[2].x * this.w, y: r[2].y * this.h };
        const len = Math.hypot(b.x - a.x, b.y - a.y) || 1;
        const ux = (b.x - a.x) / len;
        const uy = (b.y - a.y) / len;
        const ext = this.w + this.h;
        ctx.save();
        ctx.setLineDash([6, 5]);
        ctx.lineWidth = 1;
        ctx.strokeStyle = "#ffd166";
        ctx.beginPath();
        ctx.moveTo(a.x - ux * ext, a.y - uy * ext);
        ctx.lineTo(b.x + ux * ext, b.y + uy * ext);
        ctx.stroke();
        ctx.restore();
      }

      // 선이 여럿이면 왼쪽 끝 옆에 번호를 적는다 (조작 창의 선 번호와 같다).
      if (many) {
        const left = r[0].x <= r[2].x ? r[0] : r[2];
        ctx.font = "bold 11px Pretendard, 'Segoe UI', sans-serif";
        ctx.textAlign = "right";
        ctx.textBaseline = "middle";
        ctx.fillStyle = current ? "#ffffff" : "rgba(255,255,255,0.55)";
        ctx.fillText(String(cfg.lines.indexOf(line) + 1), left.x * this.w - 12, left.y * this.h);
        ctx.textAlign = "left";
      }

      this.handlePoints(line).forEach((p, i) => {
        const on = active?.kind === "ridge" && active.line === line.id && active.index === i;
        ctx.beginPath();
        ctx.arc(p.x, p.y, on ? 9 : touched ? 7.5 : 6.5, 0, Math.PI * 2);
        ctx.fillStyle = on ? "#ffd166" : "#ffffff";
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = "rgba(20,22,30,0.9)";
        ctx.stroke();
      });
    }
  }

  /** 점에서 선(곡선)까지의 거리(px). */
  private distToLine(line: Line, x: number, y: number): number {
    const r = line.ridge;
    const n = 64;
    let best = Infinity;
    let prev = { x: r[0].x * this.w, y: r[0].y * this.h };
    for (let i = 1; i <= n; i++) {
      const q = bezier(r, i / n);
      const cur = { x: q.x * this.w, y: q.y * this.h };
      const dx = cur.x - prev.x;
      const dy = cur.y - prev.y;
      const L = dx * dx + dy * dy;
      const t = L > 0 ? Math.min(1, Math.max(0, ((x - prev.x) * dx + (y - prev.y) * dy) / L)) : 0;
      best = Math.min(best, Math.hypot(prev.x + t * dx - x, prev.y + t * dy - y));
      prev = cur;
    }
    return best;
  }

  private hit(x: number, y: number): Handle | null {
    const cfg = this.cfg;
    if (!cfg) return null;
    const near = (p: Pt) => Math.hypot(p.x - x, p.y - y) <= HANDLE_R + 4;
    const box = this.selectionBox();
    if (box && near({ x: box.left + box.w, y: box.top })) return { kind: "size", id: box.doll.id };
    for (const line of cfg.lines) {
      const hp = this.handlePoints(line);
      for (const i of [0, 1, 2] as const) if (near(hp[i])) return { kind: "ridge", line: line.id, index: i };
    }
    for (let i = this.drawOrder.length - 1; i >= 0; i--) {
      const d = this.drawOrder[i];
      const p = this.placed.get(d.id);
      if (!p) continue;
      const hh = p.scale * this.h;
      const ww = hh * d.aspect;
      const px = p.x * this.w;
      const py = p.y * this.h;
      if (x >= px - ww / 2 && x <= px + ww / 2 && y >= py - hh && y <= py) {
        return { kind: "doll", id: d.id, dx: x - px, dy: y - py, snap: null, sx: x, sy: y };
      }
    }
    // 선 몸통은 인형보다 뒤에 있다 (인형을 잡는 것이 먼저).
    for (const line of cfg.lines) {
      if (this.distToLine(line, x, y) <= LINE_GRAB_PX) return { kind: "line", line: line.id, x: x / this.w, y: y / this.h };
    }
    return null;
  }

  private pos(e: PointerEvent | WheelEvent): Pt {
    const r = this.canvas.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  private onDown = (e: PointerEvent): void => {
    if (!this.editing || e.button !== 0) return;
    const p = this.pos(e);
    const h = this.hit(p.x, p.y);
    const picked = h?.kind === "doll" || h?.kind === "size" ? h.id : null;
    if (picked !== this.selectedId) {
      this.selectedId = picked;
      this.onSelect?.(picked);
    }
    // 선의 핸들이나 그 선의 인형을 누르면 그 선이 "지금 다루는 선"이 된다.
    const line =
      h?.kind === "ridge" || h?.kind === "line" ? h.line : picked ? (this.cfg?.dolls.find((d) => d.id === picked)?.line ?? null) : null;
    if (line && line !== this.selectedLine) {
      this.selectedLine = line;
      this.onSelectLine?.(line);
    }
    this.dirty = true;
    if (!h) return;
    this.drag = h;
    this.leveled = false;
    this.canvas.setPointerCapture(e.pointerId);
    this.canvas.focus({ preventScroll: true });
    this.setCursor(h, true);
    e.preventDefault();
  };

  private setCursor(h: Handle | null, dragging = false): void {
    this.canvas.style.cursor = !h
      ? "default"
      : h.kind === "doll" || h.kind === "line"
        ? dragging
          ? "grabbing"
          : "grab"
        : h.kind === "ridge"
          ? "move"
          : "nesw-resize";
  }

  private onLeave = (): void => {
    if (this.hover && !this.drag) {
      this.hover = null;
      this.dirty = true;
    }
  };

  /** 화살표 키로 선택한 인형을 1px씩(Shift: 10px) 옮긴다. */
  private onKey = (e: KeyboardEvent): void => {
    if (!this.editing || !this.cfg || this.drag) return;
    const dirs: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
    const v = dirs[e.key];
    const doll = this.cfg.dolls.find((x) => x.id === this.selectedId);
    const placed = doll && this.placed.get(doll.id);
    if (!v || !doll || !placed) return;
    e.preventDefault();
    const step = e.shiftKey ? 10 : 1;
    if (!doll.manual.layout) {
      doll.manual.layout = true;
      Object.assign(doll, { x: placed.x, y: placed.y, scale: placed.scale });
    }
    doll.x = Math.min(1, Math.max(0, doll.x + (v[0] * step) / this.w));
    doll.y = Math.min(1, Math.max(0, doll.y + (v[1] * step) / this.h));
    this.relayout();
    this.opts.onEdit?.(structuredClone(this.cfg));
  };

  private onMove = (e: PointerEvent): void => {
    if (!this.editing || !this.cfg) return;
    const p = this.pos(e);
    const d = this.drag;
    if (!d) {
      const h = this.hit(p.x, p.y);
      this.setCursor(h);
      const key = (x: Handle | null) =>
        !x ? "" : x.kind === "ridge" ? `r:${x.line}:${x.index}` : x.kind === "line" ? `l:${x.line}` : `${x.kind}:${x.id}`;
      if (key(h) !== key(this.hover)) {
        this.hover = h;
        this.dirty = true;
      }
      return;
    }
    const cfg = this.cfg;
    const c01 = (v: number) => Math.min(1, Math.max(0, v));
    const nx = c01(p.x / this.w);
    const ny = c01(p.y / this.h);
    if (d.kind === "line") {
      // 선 몸통: 선 전체를 옮긴다 (줄에 선 인형들도 따라온다). 화면 밖으로는 나가지 않게.
      const r = cfg.lines.find((l) => l.id === d.line)?.ridge;
      if (!r) return;
      const xs = r.map((q) => q.x);
      const ys = r.map((q) => q.y);
      const dx = Math.min(1 - Math.max(...xs), Math.max(-Math.min(...xs), p.x / this.w - d.x));
      const dy = Math.min(1 - Math.max(...ys), Math.max(-Math.min(...ys), p.y / this.h - d.y));
      for (const q of r) {
        q.x += dx;
        q.y += dy;
      }
      d.x += dx;
      d.y += dy;
    } else if (d.kind === "ridge") {
      const r = cfg.lines.find((l) => l.id === d.line)?.ridge;
      if (!r) return;
      this.leveled = false;
      if (d.index === 1) {
        // 가운데 핸들은 곡선의 중점에 있다: B(0.5) = (P0 + 2·P1 + P2) / 4
        let mx = nx;
        let my = ny;
        // 양 끝을 이은 직선 가까이 오면 곧은 선으로 맞춘다.
        const cx = (r[0].x + r[2].x) / 2;
        const cy = (r[0].y + r[2].y) / 2;
        const ax = (r[2].x - r[0].x) * this.w;
        const ay = (r[2].y - r[0].y) * this.h;
        const len = Math.hypot(ax, ay) || 1;
        // 직선에서 수직으로 떨어진 거리(px, 부호 있음)
        const off = ((mx - cx) * this.w * -ay + (my - cy) * this.h * ax) / len;
        if (Math.abs(off) < LEVEL_SNAP_PX) {
          mx -= ((-ay / len) * off) / this.w;
          my -= ((ax / len) * off) / this.h;
          this.leveled = true;
        }
        r[1] = { x: 2 * mx - cx, y: 2 * my - cy };
      } else {
        const other = r[d.index === 0 ? 2 : 0];
        const old = r[d.index];
        let ty = ny;
        // 다른 끝과 높이가 거의 같으면 수평으로 맞춘다.
        if (Math.abs((ty - other.y) * this.h) < LEVEL_SNAP_PX) {
          ty = other.y;
          this.leveled = true;
        }
        // 제어점을 끝점이 움직인 만큼의 절반 따라 옮긴다: 휜 모양이 그대로 유지된다
        // (곧은 선은 곧은 채로, 언덕은 언덕인 채로 늘어나거나 기울어진다).
        r[1] = { x: r[1].x + (nx - old.x) / 2, y: r[1].y + (ty - old.y) / 2 };
        r[d.index] = { x: nx, y: ty };
      }
    } else if (d.kind === "size") {
      // 바닥은 그대로 두고, 손잡이를 끈 높이만큼 인형이 커지거나 작아진다.
      const doll = cfg.dolls.find((x) => x.id === d.id);
      const placed = this.placed.get(d.id);
      if (doll && placed) this.resizeDoll(doll, placed.y * this.h - p.y);
      return;
    } else {
      const doll = cfg.dolls.find((x) => x.id === d.id);
      const placed = this.placed.get(d.id);
      if (!doll || !placed) return;
      // 누를 때의 손떨림(3px)은 끌기로 치지 않는다 (누르기만 해도 줄에서 빠지지 않게).
      if (!d.moved && Math.hypot(p.x - d.sx, p.y - d.sy) < 3) return;
      d.moved = true;
      // Shift: 처음 움직인 쪽(가로 또는 세로)으로만 옮긴다.
      let px = p.x;
      let py = p.y;
      if (e.shiftKey) {
        if (Math.abs(p.x - d.sx) >= Math.abs(p.y - d.sy)) py = d.sy;
        else px = d.sx;
      }
      const want = { x: c01((px - d.dx) / this.w), y: c01((py - d.dy) / this.h) };
      // 선 가까이 가면 선에 붙는다. 붙은 동안에는 선을 따라 좌우로만 미끄러지고(그 자리가 인형의 자리가 된다),
      // 크기도 그 자리의 원근을 따른다. 멀리 끌어내면 떨어져 자유 배치가 된다.
      const prev = d.snap;
      d.snap = null;
      let best = Infinity;
      for (const line of cfg.lines) {
        const hit = nearestOnRidge(line.ridge, want, this.w / this.h);
        const limit = line.id === prev ? SNAP_KEEP : SNAP_DIST;
        if (hit.dist < limit && hit.dist < best) {
          best = hit.dist;
          d.snap = line.id;
        }
      }
      if (d.snap) {
        const line = cfg.lines.find((l) => l.id === d.snap)!;
        doll.line = line.id;
        doll.at = atOnRidge(line.ridge, want, this.w / this.h);
        doll.manual.layout = false;
      } else {
        if (!doll.manual.layout) {
          doll.manual.layout = true;
          doll.scale = placed.scale;
        }
        doll.x = want.x;
        doll.y = want.y;
      }
    }
    this.editDirty = true;
    this.relayout();
  };

  private onUp = (e: PointerEvent): void => {
    const d = this.drag;
    if (!d) return;
    // 선에 붙인 채 놓은 인형은 그 자리(at)에 남는다. 순서(=자동 대역)는 바꾸지 않는다 — 자리만 옮겼는데 색이 바뀌면 헷갈린다.
    this.drag = null;
    this.leveled = false;
    const p = this.pos(e);
    this.setCursor(this.hit(p.x, p.y));
    this.dirty = true;
    this.deferredCfg = null;
    if (this.canvas.hasPointerCapture(e.pointerId)) this.canvas.releasePointerCapture(e.pointerId);
    if (this.editDirty && this.cfg) {
      this.editDirty = false;
      this.opts.onEdit?.(structuredClone(this.cfg));
    }
  };

  private onWheel = (e: WheelEvent): void => {
    if (!this.editing || !this.cfg) return;
    const p = this.pos(e);
    const h = this.hit(p.x, p.y);
    if (h?.kind !== "doll") return;
    e.preventDefault();
    const doll = this.cfg.dolls.find((x) => x.id === h.id);
    const placed = this.placed.get(h.id);
    if (!doll || !placed) return;
    this.resizeDoll(doll, placed.scale * this.h * Math.exp(-e.deltaY * 0.0012));
  };
}
