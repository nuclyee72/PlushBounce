import { describe, expect, it } from "vitest";
import type { BandId } from "../shared/types";
import { shouldBounce, type BeatClock } from "./trigger";

const P = 500; // 120BPM
type Hit = [time: number, strength: number];

/** 온셋들이 올 때 받아들여진 시각들. 박은 1000ms부터 P마다 온다. */
function accepted(hits: Hit[], band: BandId, tempo = true): number[] {
  const out: number[] = [];
  let last = -1e9;
  for (const [t, strength] of hits) {
    const clock: BeatClock = tempo ? { period: P, lastBeat: 1000 + Math.floor((t - 1000) / P) * P } : { period: null, lastBeat: 0 };
    if (shouldBounce(t, last, band, strength, clock)) {
      out.push(t);
      last = t;
    }
  }
  return out;
}

const range = (from: number, to: number, step: number) => {
  const v: number[] = [];
  for (let t = from; t < to; t += step) v.push(t);
  return v;
};
const weak = (ts: number[]): Hit[] => ts.map((t) => [t, 0.3]);

describe("shouldBounce", () => {
  it("bounces once per beat on a dense stream of hits, on the beat", () => {
    // 초당 8번(16분음표)의 온셋 → 초당 2번(박마다)만 꿍실
    const got = accepted(weak(range(2000, 6000, P / 4)), "full");
    expect(got).toEqual(range(2000, 6000, P));
  });

  it("ignores weak off-grid hits between beats for the kick", () => {
    const hits = range(2000, 5000, P).flatMap((t): Hit[] => [
      [t, 0.5],
      [t + P / 3, 0.3],
      [t + (2 * P) / 3, 0.3],
    ]);
    expect(accepted(hits, "kick")).toEqual(range(2000, 5000, P));
  });

  it("lets a high doll follow off-beat hats, once per beat", () => {
    const hats = range(2000 + P / 2, 6000, P); // 엇박에만
    expect(accepted(weak(hats), "high")).toEqual(hats);
    // 8분음표 하이햇이면 한 박에 한 번만.
    expect(accepted(weak(range(2000, 6000, P / 2)), "high").length).toBe(8);
  });

  it("tolerates hits that arrive slightly early or late", () => {
    const hits = range(2000, 5000, P).map((t, i) => t + (i % 2 ? 40 : -40));
    expect(accepted(weak(hits), "bass").length).toBe(hits.length);
  });

  it("takes a strong hit even off the grid, but still at most about once per beat", () => {
    const strong: Hit[] = range(2000 + 0.3 * P, 6000, P).map((t) => [t, 0.9]);
    expect(accepted(strong, "kick")).toEqual(strong.map((h) => h[0]));
    const dense: Hit[] = range(2000, 6000, P / 4).map((t) => [t, 0.9]);
    expect(accepted(dense, "kick").length).toBeLessThanOrEqual(11);
  });

  it("re-locks when the beat estimate is off and the hits are weak", () => {
    // 모든 소리가 박에서 0.3박 어긋나 있다: 막히더라도 1.5박 넘게 굶지는 않는다.
    const got = accepted(weak(range(2000 + 0.3 * P, 8000, P)), "kick");
    expect(got.length).toBeGreaterThanOrEqual(5);
    for (let i = 1; i < got.length; i++) expect(got[i] - got[i - 1]).toBeLessThanOrEqual(2 * P);
  });

  it("without a tempo, takes hits but never faster than the free gap", () => {
    const got = accepted(weak(range(0, 3000, 100)), "full", false);
    expect(got).toEqual(range(0, 3000, 300));
    expect(accepted(weak(range(0, 3000, 600)), "full", false).length).toBe(5);
  });
});
