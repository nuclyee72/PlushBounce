import type { BandId, DollBand } from "../shared/types";

const SPREAD: BandId[] = ["kick", "mid", "bass", "high"];
const CANDIDATES: BandId[] = ["kick", "bass", "mid", "high"];

/** 줄 순서대로의 자동 대역 배정 (Rust의 config::assign_bands와 같은 규칙). */
export function spreadBands(count: number, assign: "spread" | "together"): BandId[] {
  if (assign === "together" || count <= 1) return new Array<BandId>(count).fill("full");
  return Array.from({ length: count }, (_, i) => SPREAD[i % SPREAD.length]);
}

export type AssignDoll = { id: string; band: DollBand; manualBand: boolean };

/**
 * 곡에 맞춘 임시 보정. 배정된 대역에 소리가 거의 없으면 소리가 있는 대역 중
 * 인형이 가장 적은 대역을 따른다. 인형별로 minHoldMs 간격으로만 바꾼다.
 */
export class BandCorrector {
  private current = new Map<string, DollBand>();
  private changedAt = new Map<string, number>();

  constructor(private minHoldMs = 8000) {}

  update(dolls: AssignDoll[], active: Record<BandId, boolean>, nowMs: number): Map<string, DollBand> {
    const ids = new Set(dolls.map((d) => d.id));
    for (const id of [...this.current.keys()]) {
      if (!ids.has(id)) {
        this.current.delete(id);
        this.changedAt.delete(id);
      }
    }
    const count: Record<DollBand, number> = { kick: 0, bass: 0, mid: 0, high: 0, full: 0, beat: 0 };
    for (const d of dolls) {
      if (d.manualBand || !this.current.has(d.id)) this.current.set(d.id, d.band);
      count[this.current.get(d.id)!]++;
    }
    for (const d of dolls) {
      // 직접 고른 것, 전체, 박(인형 전용)은 보정하지 않는다.
      if (d.manualBand || d.band === "full" || d.band === "beat") {
        if (this.current.get(d.id) !== d.band) {
          count[this.current.get(d.id)!]--;
          count[d.band]++;
          this.current.set(d.id, d.band);
        }
        continue;
      }
      const band = d.band;
      const cur = this.current.get(d.id)!;
      let want: DollBand;
      if (active[band]) want = band;
      else if (cur !== band && cur !== "full" && cur !== "beat" && active[cur]) want = cur;
      else {
        const options = CANDIDATES.filter((b) => active[b]);
        want = options.length
          ? options.reduce((best, b) => (count[b] < count[best] ? b : best))
          : "full";
      }
      if (want === cur) continue;
      const last = this.changedAt.get(d.id);
      if (last !== undefined && nowMs - last < this.minHoldMs) continue;
      count[cur]--;
      count[want]++;
      this.current.set(d.id, want);
      this.changedAt.set(d.id, nowMs);
    }
    return this.current;
  }

  reset(): void {
    this.current.clear();
    this.changedAt.clear();
  }
}
