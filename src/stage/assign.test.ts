import { describe, expect, it } from "vitest";
import { BandCorrector, spreadBands, type AssignDoll } from "./assign";
import type { BandId } from "../shared/types";

const act = (...on: BandId[]): Record<BandId, boolean> => ({
  kick: on.includes("kick"),
  bass: on.includes("bass"),
  mid: on.includes("mid"),
  high: on.includes("high"),
  full: on.includes("full"),
});

const dolls = (bands: BandId[]): AssignDoll[] =>
  bands.map((band, i) => ({ id: `d${i}`, band, manualBand: false }));

describe("spreadBands", () => {
  it("cycles kick, mid, bass, high", () => {
    expect(spreadBands(6, "spread")).toEqual(["kick", "mid", "bass", "high", "kick", "mid"]);
  });
  it("uses full for a single doll and for together", () => {
    expect(spreadBands(1, "spread")).toEqual(["full"]);
    expect(spreadBands(3, "together")).toEqual(["full", "full", "full"]);
  });
});

describe("BandCorrector", () => {
  it("keeps assigned bands that are active", () => {
    const c = new BandCorrector();
    const d = dolls(["kick", "mid"]);
    const r = c.update(d, act("kick", "mid"), 0);
    expect(r.get("d0")).toBe("kick");
    expect(r.get("d1")).toBe("mid");
  });

  it("moves a doll off an inactive band to the least crowded active one", () => {
    const c = new BandCorrector();
    const d = dolls(["kick", "mid", "bass", "high", "kick"]);
    const r = c.update(d, act("kick", "mid"), 0);
    expect(r.get("d2")).toBe("mid"); // kick 2명, mid 1명 → mid
    expect(r.get("d3")).toBe("kick"); // 이제 kick 2, mid 2 → 먼저 나오는 kick
  });

  it("falls back to full when nothing is active", () => {
    const c = new BandCorrector();
    const r = c.update(dolls(["kick", "high"]), act(), 0);
    expect([...r.values()]).toEqual(["full", "full"]);
  });

  it("does not change again within the hold time", () => {
    const c = new BandCorrector(8000);
    const d = dolls(["high", "kick"]);
    expect(c.update(d, act("kick"), 0).get("d0")).toBe("kick");
    expect(c.update(d, act("kick", "high"), 3000).get("d0")).toBe("kick");
    expect(c.update(d, act("kick", "high"), 8000).get("d0")).toBe("high");
  });

  it("never overrides a manual band", () => {
    const c = new BandCorrector();
    const d: AssignDoll[] = [{ id: "a", band: "high", manualBand: true }];
    expect(c.update(d, act("kick"), 0).get("a")).toBe("high");
  });
});
