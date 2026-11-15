import { describe, expect, it } from "vitest";
import { DollSprite } from "./doll";

const DT = 1 / 120;
const PERIOD = 0.5;

/**
 * 박 수(hits.length)만큼 돌리고, 박마다 "다음 박 직전"의 기울기를 돌려준다 (그 박의 움직임 뒤 머무는 자리).
 * hits[i]가 true인 박에만 소리(액션)를 준다.
 */
function run(s: DollSprite, hits: boolean[]): number[] {
  const held: number[] = [];
  const per = Math.round(PERIOD / DT);
  for (let b = 0; b < hits.length; b++) {
    for (let i = 0; i < per; i++) {
      if (i === 0 && hits[b]) s.bounce(1, PERIOD, (b * per + i) * DT * 1000);
      s.step(DT);
      // 박 중간(움직임 사이): 기운 자리에 머물러 있어야 한다.
      if (i === Math.round(per * 0.35)) held.push(s.lean);
    }
  }
  return held;
}

describe("Sway action", () => {
  it("is upright at rest, flips side on every movement and waits there", () => {
    const s = new DollSprite();
    expect(Math.abs(s.lean)).toBe(0);
    const held = run(s, Array(8).fill(true)).slice(1);
    // 움직임 사이에는 가운데로 돌아가지 않고 한쪽으로 크게 기운 채 머문다.
    for (const v of held) expect(Math.abs(v)).toBeGreaterThan(0.8);
    for (let i = 1; i < held.length; i++) expect(Math.sign(held[i])).toBe(-Math.sign(held[i - 1]));
  });

  it("still flips on beats the doll keeps on its own (no new sound)", () => {
    const s = new DollSprite();
    const held = run(s, [true, true, true, true, false, false]);
    // 소리 없이 자기 박자로 이어지는 움직임도 무조건 반대로.
    for (let i = 2; i < held.length; i++) {
      if (Math.abs(held[i]) > 0.8 && Math.abs(held[i - 1]) > 0.8) expect(Math.sign(held[i])).toBe(-Math.sign(held[i - 1]));
    }
  });

  it("keeps flipping when sounds come too fast for the pose to straighten up", () => {
    // "모든 소리" 인형에 0.1초마다 소리: 자세가 펴지기 전에 다음 소리가 온다. 예전엔 아예 안 움직였다.
    for (const gap of [0.15, 0.1]) {
      const s = new DollSprite();
      const per = Math.round(gap / DT);
      let flips = 0;
      let prev = 0;
      for (let i = 0; i < Math.round(3 / DT); i++) {
        if (i % per === 0) s.hit(1, i * DT * 1000);
        s.step(DT);
        const side = Math.sign(s.lean);
        if (i * DT > 0.5 && side !== 0 && side !== prev) {
          if (prev !== 0) flips++;
          prev = side;
        }
      }
      const actions = Math.round(2.5 / gap);
      expect(flips).toBeGreaterThanOrEqual(actions - 2);
    }
  });

  it("goes back to the middle only when the music is gone", () => {
    const s = new DollSprite();
    run(s, Array(4).fill(true));
    expect(Math.abs(s.lean)).toBeGreaterThan(0.8);
    s.spring.hush();
    s.holdLean = false;
    for (let i = 0; i < Math.round(2 / DT); i++) s.step(DT);
    expect(Math.abs(s.lean)).toBeLessThan(0.02);
    expect(s.still).toBe(true);
  });
});
