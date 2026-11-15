import { Pulse, Spring } from "./spring";

/** 화면에 그려지는 인형 하나의 런타임 상태 (이미지 + 스프링). */
export class DollSprite {
  readonly spring = new Spring();
  /** "모든 소리" 인형의 즉각 반응. */
  readonly pulse = new Pulse();
  private img: HTMLImageElement | null = null;
  private url = "";
  ready = false;
  /** 마지막으로 튄 시각 (미리보기의 대역 표시 깜빡임용). */
  flashAt = -1e9;
  /** 마지막으로 소리를 받아들인 시각 (박 거르기용. 타이밍 보정과 무관한 실제 시각). */
  acceptedAt = -1e9;
  /** 그릴 때의 불투명도 (마우스가 가까우면 줄어든다). */
  alpha = 1;

  setImage(url: string, onLoad: () => void): void {
    if (url === this.url) return;
    this.url = url;
    this.ready = false;
    const img = new Image();
    img.onload = () => {
      if (this.url !== url) return;
      this.img = img;
      this.ready = true;
      onLoad();
    };
    img.src = url;
  }

  /**
   * periodS: 박 간격(초). 모르면 null. shiftS: 다른 대역의 같은 타격에 맞추는 몫(초).
   * lagS: 타이밍 보정 — 대역 표시 깜빡임도 움직임과 같이 늦춘다.
   */
  bounce(strength: number, periodS: number | null, now: number, shiftS = 0, lagS = 0): void {
    this.spring.kick(strength, periodS, shiftS);
    this.acceptedAt = now;
    this.flashAt = now + Math.max(0, lagS) * 1000;
  }

  /** 박을 알 때 자기 소리가 들렸다: 박 시계의 자리에서 움직인다 (위상은 박이 정한다). */
  energize(strength: number, periodS: number, sinceBeatS: number, now: number, lagS = 0): void {
    this.spring.energize(strength, periodS, sinceBeatS);
    this.acceptedAt = now;
    this.flashAt = now + Math.max(0, lagS) * 1000;
  }

  /** "모든 소리": 박 예측 없이 바로 쭈그러진다. */
  hit(strength: number, now: number, lagS = 0): void {
    this.pulse.hit(strength, lagS);
    this.acceptedAt = now;
    this.flashAt = now + Math.max(0, lagS) * 1000;
  }

  // ── 기울기(Sway) ──
  // 움직임(쭈그러짐)이 시작될 때마다 무조건 반대쪽으로 튀어 가서, 다음 움직임까지 그 자리에 머문다.
  // 노래가 사라졌을 때(holdLean = false)만 가운데로 돌아온다.
  /** 지금 기울어 있는(또는 가는) 쪽 (+1 / -1). */
  private leanDir = 1;
  /** 목표 자리: ±1 = 그쪽으로 기울어 머묾, 0 = 가운데. */
  private leanTarget = 0;
  /** 지금 기울기와 그 속도 (살짝 넘쳤다가 자리 잡는 용수철). */
  private leanPos = 0;
  private leanVel = 0;
  /**
   * 새 움직임 찾기: 자세가 오르는 중(rising)에 지금까지의 가장 높은 곳(ext)에서 LEAN_STEP 넘게 내려가면
   * 새 쭈그러짐이 시작된 것이다. 내려가는 중에는 가장 낮은 곳에서 LEAN_STEP 넘게 오르면 다시 오르는 중으로.
   * 가운데(0)까지 돌아오기를 기다리지 않으므로, 소리가 촘촘해 자세가 다 펴지기 전에 다음 소리가 와도 매번 센다.
   */
  private leanRising = true;
  private leanExt = 0;
  /** 노래가 나오는 중: 기운 자리에서 기다린다. false면 가운데로 돌아온다 (렌더러가 정한다). */
  holdLean = true;

  step(dt: number): void {
    // 가만히 펴져 있는 동안에도 시간은 흘러야 한다 (소리가 끊기면 평소 자세로 돌아간다).
    if (!this.spring.resting) this.spring.step(dt);
    if (!this.pulse.resting) this.pulse.step(dt);
    // 움직임이 새로 시작되는 순간(인형은 박을 예측해 소리보다 먼저 쭈그러지기 시작한다)에 반대쪽으로 출발해서
    // 소리 순간 무렵 도착한다.
    const x = this.x;
    const LEAN_STEP = 0.08;
    if (this.leanRising) {
      if (x > this.leanExt) this.leanExt = x;
      else if (x < this.leanExt - LEAN_STEP) {
        this.leanRising = false;
        this.leanExt = x;
        this.leanDir = -this.leanDir;
        if (this.holdLean) this.leanTarget = this.leanDir;
      }
    } else if (x < this.leanExt) this.leanExt = x;
    else if (x > this.leanExt + LEAN_STEP) {
      this.leanRising = true;
      this.leanExt = x;
    }
    if (!this.holdLean) this.leanTarget = 0;
    // 덜 감쇠된 용수철로 따라간다 (튀어 가서 살짝 넘쳤다가 자리 잡기). 큰 dt는 잘게 나눠 안정하게.
    // 소리가 촘촘해도 따라갈 수 있게 빠르게(한 번 넘어가는 데 약 0.08초).
    const W = 34;
    const ZETA = 0.62;
    let left = Math.min(dt, 0.1);
    while (left > 1e-6) {
      const h = Math.min(left, 1 / 120);
      this.leanVel += (W * W * (this.leanTarget - this.leanPos) - 2 * ZETA * W * this.leanVel) * h;
      this.leanPos += this.leanVel * h;
      left -= h;
    }
    if (this.leanTarget === 0 && Math.abs(this.leanPos) < 1e-3 && Math.abs(this.leanVel) < 1e-2) {
      this.leanPos = 0;
      this.leanVel = 0;
    }
  }

  /** 기울기(Sway) 동작의 값 (-1..1 근처, 부호 = 방향). */
  get lean(): number {
    return this.leanPos;
  }

  /** 기울기가 아직 움직이는 중인가 (멈춰 있지 않으면 계속 그려야 한다). */
  get leaning(): boolean {
    return Math.abs(this.leanVel) > 1e-2 || Math.abs(this.leanTarget - this.leanPos) > 1e-3;
  }

  /** 지금 자세 (음수 = 쭈그러짐). */
  get x(): number {
    return this.spring.x + this.pulse.x;
  }

  get still(): boolean {
    return this.spring.still && this.pulse.resting && !this.leaning;
  }

  /**
   * (px, py): 하단 중앙의 픽셀 좌표. h: 그릴 높이(px). x: 스프링 값 + idle 값.
   * 바닥은 고정하고 위쪽만 눌렸다 늘어난다.
   */
  draw(
    ctx: CanvasRenderingContext2D,
    px: number,
    py: number,
    h: number,
    aspect: number,
    flip: boolean,
    x: number,
    amount: number,
  ): void {
    if (!this.ready || !this.img) return;
    const sy = Math.max(0.2, 1 + x * amount);
    const sx = Math.max(0.2, 1 - x * amount * 0.5);
    const w = h * aspect;
    ctx.save();
    ctx.translate(px, py);
    ctx.scale(flip ? -sx : sx, sy);
    ctx.drawImage(this.img, -w / 2, -h, w, h);
    ctx.restore();
  }

  /**
   * 기울기(Sway): 바닥은 고정하고 위쪽이 옆으로 밀리며 조금 좁아진다 (사다리꼴).
   * lean: -1..1 (부호 = 방향). width: 꼭대기가 밀리는 정도 (인형 너비 대비).
   * 캔버스는 한 번의 변환으로 사다리꼴을 못 그리므로 가로 띠로 나눠 그린다.
   */
  drawSway(
    ctx: CanvasRenderingContext2D,
    px: number,
    py: number,
    h: number,
    aspect: number,
    flip: boolean,
    lean: number,
    width: number,
  ): void {
    if (!this.ready || !this.img) return;
    const w = h * aspect;
    const k = lean * width;
    if (Math.abs(k) < 0.002) {
      this.draw(ctx, px, py, h, aspect, flip, 0, 0);
      return;
    }
    const img = this.img;
    const iw = img.naturalWidth;
    const ih = img.naturalHeight;
    // 사다리꼴 = 기울이기(옆으로 밀림, 높이에 비례) + 위로 갈수록 좁아짐.
    // 기울이기는 한 번의 변환으로 되지만 좁아짐은 안 되므로, 화면의 가로 띠마다 그 높이의 너비로 그린다.
    // 띠 경계에 줄(층)이 생기지 않도록
    //  - 띠를 화면 픽셀 줄에 딱 맞춰 잘라(경계가 반투명하게 겹치지 않음),
    //  - 띠마다 원본의 해당 부분만이 아니라 이미지 전체를 그 띠로 잘라 그린다(이웃 띠와 같은 원본을 이어서 읽음).
    // 이웃 띠와의 차이는 띠 안의 너비 변화뿐이라 1픽셀보다 훨씬 작다.
    const base = ctx.getTransform();
    // 화면(장치) 픽셀에서 인형의 아래·위 끝.
    const bottomDev = base.d * py + base.f;
    const hDev = base.d * h;
    const BAND = 4;
    const y0 = Math.floor(bottomDev - hDev);
    const y1 = Math.ceil(bottomDev);
    const dir = flip ? -1 : 1;
    for (let y = y0; y < y1; y += BAND) {
      // 이 띠 가운데의 높이 비율 (바닥 0, 꼭대기 1).
      const t = Math.min(1, Math.max(0, (bottomDev - (y + BAND / 2)) / hDev));
      const bw = w * (1 - Math.abs(k) * 0.5 * t);
      ctx.save();
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.beginPath();
      ctx.rect(-1e5, y, 2e5, BAND);
      ctx.clip();
      ctx.setTransform(base);
      ctx.translate(px, py);
      if (flip) ctx.scale(-1, 1);
      // 원본 (u, v) → x = 기울기 밀림(높이에 비례) + 이 띠의 너비로 가운데 정렬, y = 바닥 기준.
      // 뒤집힌 인형도 화면에서 같은 방향으로 기울게 dir을 곱한다.
      ctx.transform(bw / iw, 0, (-dir * k * w) / ih, h / ih, dir * k * w - bw / 2, -h);
      ctx.drawImage(img, 0, 0);
      ctx.restore();
    }
  }
}
