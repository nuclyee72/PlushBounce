import type { BandId } from "../shared/types";

/** 지금 추정된 박. period가 null이면 템포를 모르는 상태다. 시간 단위는 ms. */
export type BeatClock = { period: number | null; lastBeat: number };

/** 템포를 모를 때: 한 번 꿍실한 뒤 이만큼은 다음 소리를 받지 않는다. */
export const FREE_GAP_MS = 300;
/** 박(또는 반박) 위치에서 이만큼(박 단위) 안에 들어온 소리는 박에 맞은 것으로 본다. */
export const GRID_TOLERANCE = 0.16;
/** 박에서 벗어난 소리라도 이만큼 세면 받는다 (박 추정이 조금 틀려도 센 타격은 놓치지 않는다). */
export const STRONG = 0.6;
/** 한 번 꿍실한 뒤 이만큼(박 단위)은 다음 소리를 받지 않는다 → 한 박에 한 번. */
export const REFRACTORY_BEATS = 0.75;
/** 이만큼(박 단위) 한 번도 못 받았으면 다음 소리는 그냥 받는다. */
export const RELOCK_BEATS = 1.5;

/**
 * 온셋이 왔을 때 이 인형이 꿍실해야 하는가.
 *
 * 실제 음악에서는 한 대역에 온셋이 초당 7~8번씩 잡힌다. 그때마다 다 쭈그러지면
 * "두두두"가 되어 박자가 안 보인다. 그래서 박이 잡혀 있으면 한 박에 한 번만,
 * 박 격자에 맞는 소리(또는 뚜렷하게 센 소리)에 반응한다 — 둠칫, 둠칫.
 * 킥·베이스·전체는 정박에, 중음·고음은 정박 또는 엇박(반박)에 맞는다:
 * 하이햇처럼 엇박에 나오는 소리를 따르는 인형은 엇박에 꿍실해서 서로 엇갈린다.
 */
export function shouldBounce(now: number, lastAccept: number, band: BandId, strength: number, clock: BeatClock): boolean {
  const since = now - lastAccept;
  if (!clock.period || clock.lastBeat <= 0) return since >= FREE_GAP_MS;
  const p = clock.period;
  if (since < REFRACTORY_BEATS * p) return false;
  if (since > RELOCK_BEATS * p || strength >= STRONG) return true;
  const grid = band === "mid" || band === "high" ? 0.5 : 1;
  const pos = (now - clock.lastBeat) / p / grid;
  const dist = Math.abs(pos - Math.round(pos)) * grid;
  return dist <= GRID_TOLERANCE;
}
