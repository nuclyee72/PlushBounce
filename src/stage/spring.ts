/**
 * 인형 하나의 꿍실. x가 음수면 쭈그러진 상태, 양수면 펴진 상태 (-1..+STRETCH).
 *
 * 인형은 자기 박자로 계속 도는 진동자다 (관성). 들어온 소리는 위상과 주기를
 * "조금씩" 끌어당길 뿐 강제로 맞추지 않는다. 그래서
 *   - 소리가 살짝 이르거나 늦어도 모양이 튀지 않고,
 *   - 박과 동떨어진 소리 하나는 무시되고,
 *   - 박이 한두 개 빠져도 같은 박자로 계속 돈다.
 * 한 박 동안: 쭈그러지기(ease-out) → 소리 순간에 가장 쭈그러짐 → 펴지기(ease-out) → 다음 소리 직전에 다시 쭈그러지기 시작.
 */
export class Spring {
  x = 0;
  private running = false;
  /** 누적 위상. 정수 = 박. */
  private phase = 0;
  private period = 0.5;
  private amp = 0;
  private ampTarget = 0;
  private sinceKick = Infinity;
  /** 아직 반영하지 않은 위상 보정. 한 번에 넣지 않고 천천히 녹인다. */
  private correction = 0;
  /** 위상 보정을 녹이는 시간(초). 분석의 박에 맞출 때는 더 빨리. */
  private correctS = CORRECT_S;
  /** 멈춘 자세에서 처음 시작할 때(또는 다시 맞출 때)의 틈을 메우는 값. */
  private offset = 0;
  /** 연속으로 박과 동떨어져 들어온 소리 수. */
  private offGrid = 0;
  /** 연속으로 인형의 주기와 다른 간격으로 들어온 소리 수. */
  private mismatch = 0;
  private lastInterval = Infinity;
  /** 박에 맞게 들어온 소리 수 (처음 시작·다시 맞춘 뒤부터). */
  private locked = 0;
  /** 소리가 끊겼다(역치 아래): 박을 기다리지 않고 바로 잦아든다. */
  private hushed = false;
  /**
   * 화면 지연 보정(초). 양수면 움직임이 그만큼 늦게, 음수면 일찍 온다.
   * 인형은 박을 예측하며 돌기 때문에 일찍 움직이는 것도 가능하다.
   */
  lag = 0;

  /**
   * 소리(박)가 들어왔다. strength 0..1. periodS: 추정된 박 간격(초). 모르면 null.
   * shiftS: 이 소리를 이만큼(초) 앞서 들은 것으로 친다(음수면 그만큼 뒤의 박에 속한 것으로).
   * 다른 대역의 같은 타격이나 박 격자에 맞출 때 쓴다.
   */
  kick(strength: number, periodS: number | null, shiftS = 0): void {
    const shift = Number.isFinite(shiftS) ? shiftS : 0;
    const interval = this.sinceKick - shift;
    this.sinceKick = Math.max(0, shift);
    this.hushed = false;
    this.ampTarget = 0.85 + 0.15 * Math.min(1, Math.max(0, strength));

    if (!this.running) {
      // 멈춰 있다가 처음 움직일 때만 위상을 바로 맞춘다.
      this.running = true;
      // 소리를 들은 순간이 가장 쭈그러진 순간이 되게 시작한다.
      if (periodS) this.period = clampPeriod(periodS);
      this.phase = HIT_PHASE + shift / this.period;
      this.amp = this.ampTarget;
      this.correction = 0;
      this.offGrid = 0;
      this.mismatch = 0;
      this.lastInterval = Infinity;
      this.locked = 0;
      this.offset = this.x - this.pose(this.phase) * this.amp;
      return;
    }

    // 위상 오차: 소리가 왔을 때 인형이 "가장 쭈그러진 자리"에서 얼마나 앞서(+) / 뒤처져(-) 있는가.
    // (ease-out 이전처럼, 소리를 들은 순간에 가장 쭈그러져 있는 것이 정상이다.)
    const heard = this.phase - shift / this.period;
    const f = heard - Math.floor(heard) - HIT_PHASE;
    const err = f - Math.round(f);
    const onGrid = Math.abs(err) <= MAX_ERR;

    // 소리 간격이 서로는 일정한데 인형의 주기와 계속 다르면 (박이 빠진 2배 간격은 제외)
    // 인형 쪽 템포가 틀린 것이다. 위상만으로는 우연히 맞아 보일 수 있어서 간격으로도 본다.
    const ratio = interval / this.period;
    const steadyInterval = Number.isFinite(this.lastInterval) && Math.abs(interval / this.lastInterval - 1) < 0.15;
    const mismatched = steadyInterval && Math.abs(ratio - 1) > 0.25 && Math.abs(ratio - 2) > 0.2;
    this.lastInterval = interval;
    this.mismatch = mismatched ? this.mismatch + 1 : 0;
    if (this.mismatch >= RESYNC_AFTER) {
      this.resync(interval, periodS);
      return;
    }

    if (!onGrid) {
      // 박과 동떨어진 소리. 하나둘은 무시하지만, 계속 그렇다면 인형 쪽 박이 틀린 것이다
      // (예: 처음에 템포를 배수로 잘못 잡고 시작) → 소리에 다시 맞춘다.
      this.offGrid++;
      if (this.offGrid >= RESYNC_AFTER) this.resync(interval, periodS);
      return;
    }
    this.offGrid = 0;
    this.locked++;

    // 주기는 실제로 들어오는 소리를 기준으로 맞춘다. 템포 추정값은 출발점으로만 쓴다
    // (추정이 몇 %만 틀려도 그걸 따르면 박보다 늘 일정하게 어긋난 채로 돈다).
    //  - 위상 오차로 속도 자체를 고친다: 늘 앞서면 느리게, 늘 뒤처지면 빠르게.
    this.period *= 1 + err * FREQ_GAIN;
    //  - 박을 처음 잡는 동안에만 소리 간격으로도 당긴다. 잡은 뒤에는 간격 하나하나의
    //    흔들림이 모양을 흔들지 않도록 쓰지 않는다 (박이 빠져 2배가 된 간격은 무시).
    if (this.locked <= LOCK_BEATS && Number.isFinite(interval) && Math.abs(interval / this.period - 1) < 0.3) {
      this.period += (interval - this.period) * PERIOD_GAIN;
    }
    this.period = clampPeriod(this.period);

    // 위상: 일부만 끌어당긴다.
    this.correction = -err * PHASE_GAIN;
    this.correctS = CORRECT_S;
  }

  /**
   * 분석이 찾은 박이 지금 왔다: 인형의 박을 그 박에 완전히 맞춘다 (지금이 가장 쭈그러진 자리,
   * 주기 = 박 간격). 박을 알 때는 박자 인형이 모두 이것만 따른다 — 박 기록과 인형이 같은 박에 움직인다.
   * 멈춰 있는 인형은 움직이게 하지 않는다 (그건 energize).
   */
  sync(periodS: number): void {
    this.period = clampPeriod(periodS);
    if (!this.running) return;
    const f = this.phase - Math.floor(this.phase) - HIT_PHASE;
    this.correction = -(f - Math.round(f));
    this.correctS = SYNC_CORRECT_S;
    this.offGrid = 0;
    this.mismatch = 0;
  }

  /**
   * 박을 알 때 자기 소리가 들렸다: 위상은 건드리지 않고 움직일 힘(진폭)만 채운다.
   * 멈춰 있었다면 박 시계의 자리(sinceBeatS = 직전 박에서 지난 초)에서 시작한다.
   */
  energize(strength: number, periodS: number, sinceBeatS: number): void {
    this.sinceKick = 0;
    this.hushed = false;
    this.ampTarget = 0.85 + 0.15 * Math.min(1, Math.max(0, strength));
    if (this.running) return;
    this.running = true;
    this.period = clampPeriod(periodS);
    this.phase = HIT_PHASE + Math.max(0, sinceBeatS) / this.period;
    this.amp = this.ampTarget;
    this.correction = 0;
    this.offGrid = 0;
    this.mismatch = 0;
    this.lastInterval = Infinity;
    this.locked = 0;
    this.offset = this.x - this.pose(this.phase) * this.amp;
  }

  /** 인형의 박을 지금 소리에 바로 맞춘다. 모양이 튀지 않게 틈은 offset으로 메운다. */
  private resync(interval: number, periodS: number | null): void {
    const before = this.x;
    if (Number.isFinite(interval) && interval >= 0.25 && interval <= 1.5) this.period = interval;
    else if (periodS) this.period = clampPeriod(periodS);
    this.phase = Math.floor(this.phase) + HIT_PHASE;
    this.correction = 0;
    this.offGrid = 0;
    this.mismatch = 0;
    this.locked = 0;
    this.offset = before - this.pose(this.phase) * this.amp;
  }

  /** 소리가 끊겼다(무음). 다음 박을 기다리지 않고 지금부터 잦아든다. 소리가 다시 오면 풀린다. */
  hush(): void {
    this.hushed = true;
  }

  /** 지연 보정을 넣은 자세. */
  private pose(phase: number): number {
    return shape(phase - this.lag / this.period);
  }

  step(dt: number): void {
    const d = Math.min(dt, 0.1);
    this.sinceKick += d;
    if (this.running) {
      const c = this.correction * (1 - Math.exp(-d / this.correctS));
      this.correction -= c;
      this.phase += d / this.period + c;
      // 소리가 몇 박 동안 없거나 아예 끊기면 그때부터 잦아든다.
      if (this.hushed) this.ampTarget *= Math.exp(-d / HUSH_S);
      else if (this.sinceKick > HOLD_BEATS * this.period) this.ampTarget *= Math.exp(-d / (this.period * FADE_BEATS));
      this.amp += (this.ampTarget - this.amp) * (1 - Math.exp(-d / AMP_S));
      if (this.ampTarget < 0.01 && this.amp < 0.01) {
        this.running = false;
        this.amp = 0;
        this.ampTarget = 0;
      }
    }
    this.offset *= Math.exp(-d / 0.06);
    if (Math.abs(this.offset) < 1e-3) this.offset = 0;
    this.x = this.running ? this.pose(this.phase) * this.amp + this.offset : this.offset;
  }

  /** 누적 위상 (정수 = 박). 테스트·진단용. */
  get beatPosition(): number {
    return this.phase;
  }

  /** 움직임이 멈췄는가. */
  get still(): boolean {
    return this.resting;
  }

  /** 평소 자세로 완전히 돌아왔는가. */
  get resting(): boolean {
    return !this.running && this.offset === 0;
  }
}

/** 펴진 자세의 높이 (쭈그러진 깊이 대비). */
const STRETCH = 0.7;
/** 한 박 중 쭈그러지는 데 쓰는 몫. 나머지 동안 펴진다. */
export const SQUASH_PART = 0.2;
/**
 * 소리를 들은 순간 인형이 있어야 할 위상: 가장 쭈그러진 자리.
 * ease-out 이전(소리 = 가장 쭈그러진 순간)과 같은 타이밍이다. 쭈그러지기 시작하는 건
 * 그보다 SQUASH_PART박 앞 — 인형은 박을 예측하며 돌기 때문에 미리 내려가기 시작할 수 있다.
 */
export const HIT_PHASE = SQUASH_PART;
/** 들어온 소리마다 주기·위상을 끌어당기는 비율. 작을수록 관성이 크다. */
const PERIOD_GAIN = 0.15;
const PHASE_GAIN = 0.35;
/** 위상 오차로 속도를 고치는 비율. */
const FREQ_GAIN = 0.06;
/** 연속으로 이만큼 박과 동떨어진 소리가 오면 인형의 박을 소리에 다시 맞춘다. */
const RESYNC_AFTER = 3;
/** 처음 이만큼의 박 동안만 소리 간격으로 주기를 당긴다. */
const LOCK_BEATS = 6;
/** 박에서 이만큼(박 단위) 넘게 벗어난 소리는 위상에 반영하지 않는다. */
const MAX_ERR = 0.3;
/** 위상 보정을 녹이는 시간(초). */
const CORRECT_S = 0.12;
/** 분석의 박에 맞출 때 위상 보정을 녹이는 시간(초). 박마다 오차를 거의 다 없앤다. */
const SYNC_CORRECT_S = 0.04;
/** 소리가 이만큼(박 수) 없어도 그대로 돈다 (박 하나가 빠져도 모양이 그대로다). */
const HOLD_BEATS = 2.5;
/** 그 뒤 잦아드는 시간(박 수). */
const FADE_BEATS = 0.5;
/** 소리가 아예 끊겼을 때 잦아드는 시간(초). */
const HUSH_S = 0.25;
/** 진폭이 목표를 따라가는 시간(초). */
const AMP_S = 0.18;

const clampPeriod = (p: number) => Math.min(1.5, Math.max(0.25, p));
const easeOut = (t: number) => 1 - (1 - t) ** 3;

/**
 * 위상 → 자세. 박(정수)에서 가장 펴져 있다가 쭈그러지기 시작하고,
 * SQUASH_PART에서 가장 쭈그러지고, 다시 펴져서 다음 박 직전에 가장 펴진다.
 * 두 구간 모두 ease-out (빠르게 시작해 천천히 도착).
 */
export function shape(phase: number): number {
  const f = phase - Math.floor(phase);
  if (f < SQUASH_PART) return STRETCH + (-1 - STRETCH) * easeOut(f / SQUASH_PART);
  return -1 + (STRETCH + 1) * easeOut((f - SQUASH_PART) / (1 - SQUASH_PART));
}

/**
 * "모든 소리" 인형의 움직임: 소리가 날 때마다 바로 쭈그러졌다가(50ms) 펴진다(240ms).
 * 박을 예측하지 않고 들은 대로만 움직인다. 촘촘한 소리에는 다 펴지기 전에 다시 쭈그러진다.
 */
export class Pulse {
  x = 0;
  private t = 0;
  private queue: { at: number; depth: number }[] = [];
  private hitAt = -Infinity;
  private from = 0;
  private depth = 0;

  /** strength 0..1. delayS: 이만큼 늦게 움직인다 (타이밍 보정, 음수는 0으로 친다). */
  hit(strength: number, delayS = 0): void {
    this.queue.push({ at: this.t + Math.max(0, delayS), depth: 0.75 + 0.25 * Math.min(1, Math.max(0, strength)) });
  }

  step(dt: number): void {
    this.t += Math.min(dt, 0.1);
    while (this.queue.length && this.queue[0].at <= this.t) {
      const h = this.queue.shift()!;
      this.from = this.x;
      this.depth = h.depth;
      this.hitAt = h.at;
    }
    const s = this.t - this.hitAt;
    if (s < PULSE_DOWN_S) this.x = this.from + (-this.depth - this.from) * easeOut(s / PULSE_DOWN_S);
    else if (s < PULSE_DOWN_S + PULSE_UP_S) this.x = -this.depth * (1 - easeOut((s - PULSE_DOWN_S) / PULSE_UP_S));
    else this.x = 0;
  }

  get resting(): boolean {
    return this.queue.length === 0 && this.t - this.hitAt >= PULSE_DOWN_S + PULSE_UP_S;
  }
}

const PULSE_DOWN_S = 0.05;
const PULSE_UP_S = 0.24;
