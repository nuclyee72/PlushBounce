import { describe, expect, it } from "vitest";
import { atOnRidge, autoLayout, insertByX, nearestOnRidge, type LayoutItem } from "./layout";
import type { LayoutCfg } from "../shared/types";

const hill: LayoutCfg = {
  ridge: [
    { x: 0.08, y: 0.78 },
    { x: 0.5, y: 0.66 },
    { x: 0.92, y: 0.7 },
  ],
  near: "left",
  farScale: 0.45,
  overlap: 0.15,
};
const flat: LayoutCfg = {
  ...hill,
  ridge: [
    { x: 0.08, y: 0.955 },
    { x: 0.5, y: 0.955 },
    { x: 0.92, y: 0.955 },
  ],
};

const aspects = [0.8, 1.0, 1.3, 0.6, 1.1];
const items = (n: number): LayoutItem[] =>
  Array.from({ length: n }, (_, i) => ({ id: `d${i}`, aspect: aspects[i % aspects.length] }));

describe("snapping", () => {
  it("finds the closest point on the ridge", () => {
    const hit = nearestOnRidge(flat.ridge, { x: 0.3, y: 0.9 }, 16 / 9);
    expect(hit.x).toBeCloseTo(0.3, 1);
    expect(hit.y).toBeCloseTo(0.955, 5);
    expect(hit.dist).toBeCloseTo(0.055, 2);
    // 선의 끝을 지나치면 끝점에 붙는다.
    expect(nearestOnRidge(flat.ridge, { x: 0.99, y: 0.955 }, 16 / 9).x).toBeCloseTo(0.92, 5);
  });

  it("inserts a doll into the line order by where it was dropped", () => {
    const all = ["a", "b", "c", "d"].map((id, order) => ({ id, order }));
    const onLine = [
      { id: "a", x: 0.2 },
      { id: "b", x: 0.4 },
      { id: "c", x: 0.6 },
    ];
    expect(insertByX(all, onLine, "d", 0.5)).toEqual(["a", "b", "d", "c"]);
    expect(insertByX(all, onLine, "d", 0.1)).toEqual(["d", "a", "b", "c"]);
    expect(insertByX(all, onLine, "d", 0.9)).toEqual(["a", "b", "c", "d"]);
    expect(insertByX(all, onLine, "a", 0.5)).toEqual(["b", "a", "c", "d"]);
    expect(insertByX(all, [], "b", 0.5)).toEqual(["a", "c", "d", "b"]);
  });
});

describe("autoLayout", () => {
  for (const [name, layout] of [
    ["hill", hill],
    ["flat", flat],
    ["hill near right", { ...hill, near: "right" as const }],
  ] as const) {
    for (const n of [1, 5, 20]) {
      for (const aspect of [16 / 9, 4 / 3, 9 / 16]) {
        for (const gs of [0.5, 1, 3]) {
          it(`${name}: ${n} dolls stay on screen (aspect ${aspect.toFixed(2)}, scale ${gs})`, () => {
            const list = items(n);
            const placed = autoLayout(list, layout, aspect, gs);
            expect(placed.size).toBe(n);
            for (const item of list) {
              const p = placed.get(item.id)!;
              const halfW = (p.scale * item.aspect) / aspect / 2;
              expect(p.scale).toBeGreaterThan(0);
              expect(p.x - halfW).toBeGreaterThanOrEqual(0);
              expect(p.x + halfW).toBeLessThanOrEqual(1);
              expect(p.y - p.scale).toBeGreaterThanOrEqual(0);
              expect(p.y).toBeLessThanOrEqual(1);
            }
          });
        }
      }
    }
  }

  it("places dolls left to right in order", () => {
    const list = items(5);
    const placed = autoLayout(list, hill, 16 / 9, 1);
    const xs = list.map((d) => placed.get(d.id)!.x);
    for (let i = 1; i < xs.length; i++) expect(xs[i]).toBeGreaterThan(xs[i - 1]);
  });

  it("shrinks toward the far end, and not at all without perspective", () => {
    const list = Array.from({ length: 5 }, (_, i) => ({ id: `d${i}`, aspect: 1 }));
    const near = autoLayout(list, hill, 16 / 9, 1);
    const s = list.map((d) => near.get(d.id)!.scale);
    for (let i = 1; i < s.length; i++) expect(s[i]).toBeLessThan(s[i - 1]);

    const right = autoLayout(list, { ...hill, near: "right" }, 16 / 9, 1);
    const r = list.map((d) => right.get(d.id)!.scale);
    for (let i = 1; i < r.length; i++) expect(r[i]).toBeGreaterThan(r[i - 1]);

    const none = autoLayout(list, { ...hill, farScale: 1 }, 16 / 9, 1);
    const e = list.map((d) => none.get(d.id)!.scale);
    for (const v of e) expect(v).toBeCloseTo(e[0], 6);
  });

  it("applies each doll's own size and still stays on screen", () => {
    const list = Array.from({ length: 5 }, (_, i) => ({ id: `d${i}`, aspect: 1, size: i === 2 ? 2 : 1 }));
    const flatNoPersp = { ...flat, farScale: 1 };
    const placed = autoLayout(list, flatNoPersp, 16 / 9, 1);
    const s = list.map((d) => placed.get(d.id)!.scale);
    expect(s[2] / s[1]).toBeCloseTo(2, 5);
    expect(s[0]).toBeCloseTo(s[4], 6);
    for (const item of list) {
      const p = placed.get(item.id)!;
      expect(p.y - p.scale).toBeGreaterThanOrEqual(0);
    }
    const big = autoLayout(
      list.map((d) => ({ ...d, size: 4 })),
      flatNoPersp,
      16 / 9,
      3,
    );
    for (const item of list) {
      const p = big.get(item.id)!;
      const halfW = p.scale / (16 / 9) / 2;
      expect(p.x - halfW).toBeGreaterThanOrEqual(0);
      expect(p.x + halfW).toBeLessThanOrEqual(1);
      expect(p.y - p.scale).toBeGreaterThanOrEqual(0);
    }
  });

  it("keeps neighbours within the allowed overlap", () => {
    const list = Array.from({ length: 20 }, (_, i) => ({ id: `d${i}`, aspect: 1 }));
    const aspect = 16 / 9;
    const placed = autoLayout(list, flat, aspect, 1);
    for (let i = 1; i < list.length; i++) {
      const a = placed.get(list[i - 1].id)!;
      const b = placed.get(list[i].id)!;
      const dist = (b.x - a.x) * aspect;
      const mean = (a.scale + b.scale) / 2;
      expect(dist).toBeGreaterThan(mean * (1 - 0.15) * 0.98);
    }
  });
});

describe("dolls placed at a spot on the line (at)", () => {
  const level = { ...flat, farScale: 1 };
  it("sits where it was put and keeps the row size rule", () => {
    const items: LayoutItem[] = [
      { id: "a", aspect: 0.8 },
      { id: "b", aspect: 0.8 },
      { id: "p", aspect: 0.8, at: 0.25 },
    ];
    const out = autoLayout(items, level, 16 / 9, 1);
    const p = out.get("p")!;
    expect(p.x).toBeCloseTo(0.08 + 0.25 * 0.84, 3);
    expect(p.y).toBeCloseTo(0.955, 3);
    expect(p.scale).toBeCloseTo(out.get("a")!.scale, 6);
    // 나머지는 그 인형이 없는 것처럼 줄을 선다.
    const alone = autoLayout(items.slice(0, 2), level, 16 / 9, 1);
    expect(out.get("a")).toEqual(alone.get("a"));
  });

  it("maps a point back to its spot on the line", () => {
    expect(atOnRidge(level.ridge, { x: 0.08 + 0.6 * 0.84, y: 0.9 }, 16 / 9)).toBeCloseTo(0.6, 2);
    expect(atOnRidge(level.ridge, { x: -1, y: 0.5 }, 16 / 9)).toBe(0);
    // 오른쪽에서 왼쪽으로 그린 선도 왼쪽 끝이 0이다.
    const rev = [level.ridge[2], level.ridge[1], level.ridge[0]] as typeof level.ridge;
    expect(atOnRidge(rev, { x: 0.08 + 0.6 * 0.84, y: 0.9 }, 16 / 9)).toBeCloseTo(0.6, 2);
  });
});

describe("auto spacing", () => {
  const level = { ...flat, farScale: 1, autoGap: true };
  const span = (out: Map<string, { x: number }>) => {
    const xs = [...out.values()].map((p) => p.x);
    return Math.max(...xs) - Math.min(...xs);
  };
  it("spreads a few dolls evenly across the whole line", () => {
    const items: LayoutItem[] = [0, 1, 2].map((i) => ({ id: "d" + i, aspect: 0.8 }));
    const out = autoLayout(items, level, 16 / 9, 1);
    const xs = items.map((i) => out.get(i.id)!.x);
    expect(xs[1] - xs[0]).toBeCloseTo(xs[2] - xs[1], 4);
    // 선 길이(0.84)의 대부분을 쓴다
    expect(span(out)).toBeGreaterThan(0.6);
    // 수동 간격(겹침 15%)이면 가운데에 모여 있다
    expect(span(autoLayout(items, { ...level, autoGap: false }, 16 / 9, 1))).toBeLessThan(0.3);
  });
  it("still fits many dolls on screen", () => {
    const items: LayoutItem[] = Array.from({ length: 20 }, (_, i) => ({ id: "d" + i, aspect: 0.8 }));
    const out = autoLayout(items, level, 16 / 9, 1);
    for (const p of out.values()) {
      expect(p.x).toBeGreaterThanOrEqual(0);
      expect(p.x).toBeLessThanOrEqual(1);
    }
  });
});
