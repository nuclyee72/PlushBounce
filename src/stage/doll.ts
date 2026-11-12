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

  /** "모든 소리": 박 예측 없이 바로 쭈그러진다. */
  hit(strength: number, now: number, lagS = 0): void {
    this.pulse.hit(strength, lagS);
    this.acceptedAt = now;
    this.flashAt = now + Math.max(0, lagS) * 1000;
  }

  step(dt: number): void {
    // 가만히 펴져 있는 동안에도 시간은 흘러야 한다 (소리가 끊기면 평소 자세로 돌아간다).
    if (!this.spring.resting) this.spring.step(dt);
    if (!this.pulse.resting) this.pulse.step(dt);
  }

  /** 지금 자세 (음수 = 쭈그러짐). */
  get x(): number {
    return this.spring.x + this.pulse.x;
  }

  get still(): boolean {
    return this.spring.still && this.pulse.resting;
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
}
