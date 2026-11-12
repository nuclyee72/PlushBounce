import type { LayoutCfg, Pt } from "../shared/types";

/** size: 인형별 크기 배율 (기본 1). at: 선 위에서 직접 정한 자리 (선 길이 비율). 없으면 자동 줄 세우기. */
export type LayoutItem = { id: string; aspect: number; size?: number; at?: number | null };
/** x, y: 하단 중앙의 화면 비율 좌표. scale: 인형 높이 / 화면 높이. */
export type Placed = { x: number; y: number; scale: number };

/** 간격 자동일 때 줄이 차지하는 선 길이의 비율, 간격 배수의 범위 (1 = 딱 붙음, 0.4 = 60% 겹침). */
const AUTO_FILL = 0.94;
const AUTO_GAP_MIN = 0.4;
const AUTO_GAP_MAX = 8;

/** 배율 1, 가장 가까운 자리에 있는 인형의 높이 (화면 높이 대비). */
export const BASE_HEIGHT = 0.2;

export function bezier(r: [Pt, Pt, Pt], t: number): Pt {
  const u = 1 - t;
  return {
    x: u * u * r[0].x + 2 * u * t * r[1].x + t * t * r[2].x,
    y: u * u * r[0].y + 2 * u * t * r[1].y + t * t * r[2].y,
  };
}

type Ridge = { len: number; at: (s: number) => Pt };

/** 능선을 호 길이로 매개변수화한다. 길이 단위는 화면 높이, 항상 왼쪽 끝에서 시작한다. */
export function measureRidge(ridge: [Pt, Pt, Pt], screenAspect: number, n = 160): Ridge {
  const r: [Pt, Pt, Pt] = ridge[0].x <= ridge[2].x ? ridge : [ridge[2], ridge[1], ridge[0]];
  const cum = new Float64Array(n + 1);
  let prev = bezier(r, 0);
  for (let i = 1; i <= n; i++) {
    const p = bezier(r, i / n);
    cum[i] = cum[i - 1] + Math.hypot((p.x - prev.x) * screenAspect, p.y - prev.y);
    prev = p;
  }
  const len = cum[n];
  const at = (s: number): Pt => {
    if (len <= 0) return bezier(r, 0);
    const target = Math.min(len, Math.max(0, s));
    let lo = 0;
    let hi = n;
    while (hi - lo > 1) {
      const mid = (lo + hi) >> 1;
      if (cum[mid] <= target) lo = mid;
      else hi = mid;
    }
    const seg = cum[hi] - cum[lo];
    const f = seg > 0 ? (target - cum[lo]) / seg : 0;
    return bezier(r, (lo + f) / n);
  };
  return { len, at };
}

/**
 * 점 p에서 가장 가까운 능선 위의 점. dist는 화면 높이 단위의 거리.
 * 끌던 인형을 선에 붙일 때(스냅) 쓴다.
 */
export function nearestOnRidge(ridge: [Pt, Pt, Pt], p: Pt, screenAspect: number, n = 96): Pt & { dist: number } {
  let best = { x: ridge[0].x, y: ridge[0].y, dist: Infinity };
  for (let i = 0; i <= n; i++) {
    const q = bezier(ridge, i / n);
    const dist = Math.hypot((q.x - p.x) * screenAspect, q.y - p.y);
    if (dist < best.dist) best = { x: q.x, y: q.y, dist };
  }
  return best;
}

/**
 * 점 p에서 가장 가까운 선 위 자리를 선 길이 비율(0 = 왼쪽 끝, 1 = 오른쪽 끝)로 돌려준다.
 * 선에 붙인 인형의 자리(`at`)를 정할 때 쓴다.
 */
export function atOnRidge(ridge: [Pt, Pt, Pt], p: Pt, screenAspect: number, n = 200): number {
  const r: [Pt, Pt, Pt] = ridge[0].x <= ridge[2].x ? ridge : [ridge[2], ridge[1], ridge[0]];
  let len = 0;
  let prev = bezier(r, 0);
  let best = { d: Math.hypot((prev.x - p.x) * screenAspect, prev.y - p.y), s: 0 };
  for (let i = 1; i <= n; i++) {
    const q = bezier(r, i / n);
    len += Math.hypot((q.x - prev.x) * screenAspect, q.y - prev.y);
    const d = Math.hypot((q.x - p.x) * screenAspect, q.y - p.y);
    if (d < best.d) best = { d, s: len };
    prev = q;
  }
  return len > 0 ? Math.min(1, Math.max(0, best.s / len)) : 0;
}

/**
 * 인형을 선에 다시 올릴 때의 새 순서. 같은 선의 인형들(왼쪽→오른쪽 x좌표)과 비교해
 * 놓은 자리(x)에 끼워 넣은 전체 id 순서를 돌려준다.
 */
export function insertByX(
  all: { id: string; order: number }[],
  onLine: { id: string; x: number }[],
  movedId: string,
  x: number,
): string[] {
  const ids = [...all].sort((a, b) => a.order - b.order).map((d) => d.id).filter((id) => id !== movedId);
  const mates = onLine.filter((d) => d.id !== movedId).sort((a, b) => a.x - b.x);
  const next = mates.find((d) => d.x > x);
  if (next) ids.splice(ids.indexOf(next.id), 0, movedId);
  else if (mates.length) ids.splice(ids.indexOf(mates[mates.length - 1].id) + 1, 0, movedId);
  else ids.push(movedId);
  return ids;
}

/** 능선 위 위치 u(0=왼쪽 끝, 1=오른쪽 끝)에서의 원근 배율. */
export function perspective(layout: LayoutCfg, u: number): number {
  const c = Math.min(1, Math.max(0, u));
  const d = layout.near === "left" ? c : 1 - c;
  return 1 + (layout.farScale - 1) * d;
}

/**
 * 인형을 순서대로 능선 위에 줄 세운다. 줄이 능선보다 길면 전체를 줄이고,
 * 어떤 인형도 화면 밖으로 나가지 않게 한다.
 * 자리를 직접 정한 인형(`at`)은 줄에서 빠져 그 자리에 앉는다. 크기는 줄과 같은 규칙(원근·전체 크기·개별 배율)을 따른다.
 */
export function autoLayout(
  items: LayoutItem[],
  layout: LayoutCfg,
  screenAspect: number,
  globalScale: number,
): Map<string, Placed> {
  const out = new Map<string, Placed>();
  if (items.length === 0) return out;
  const row = items.filter((i) => i.at == null);
  const pinned = items.filter((i) => i.at != null);
  const { placed, k, ridge, L } = layoutRow(row, layout, screenAspect, globalScale);
  row.forEach((item, i) => out.set(item.id, placed[i]));
  for (const item of pinned) {
    const c = Math.min(1, Math.max(0, item.at!)) * L;
    const p = ridge.at(c);
    // 화면 위로 넘치지 않게만 줄인다.
    const h = Math.min(p.y, BASE_HEIGHT * globalScale * k * perspective(layout, c / L) * (item.size ?? 1));
    out.set(item.id, { x: Math.min(1, Math.max(0, p.x)), y: Math.min(1, Math.max(0, p.y)), scale: Math.max(0.005, h) });
  }
  return out;
}

/** 줄 세우기. 줄 전체에 적용된 배율 k도 돌려준다 (자리를 정한 인형이 같은 크기 규칙을 따르도록). */
function layoutRow(items: LayoutItem[], layout: LayoutCfg, screenAspect: number, globalScale: number) {
  const ridge = measureRidge(layout.ridge, screenAspect);
  const L = Math.max(ridge.len, 1e-6);
  let gap = 1 - Math.min(0.9, Math.max(0, layout.overlap));

  const heightAt = (k: number, c: number) => BASE_HEIGHT * globalScale * k * perspective(layout, c / L);

  // 배율 k, 시작 위치 offset으로 놓았을 때의 중심들과 줄의 오른쪽 끝.
  const place = (k: number, offset: number) => {
    const centers: number[] = [];
    let prevC = 0;
    let prevW = 0;
    for (let i = 0; i < items.length; i++) {
      const a = items[i].aspect * (items[i].size ?? 1);
      let w = i === 0 ? heightAt(k, offset) * a : prevW;
      let c = 0;
      for (let it = 0; it < 6; it++) {
        c = i === 0 ? offset + w / 2 : prevC + ((prevW + w) / 2) * gap;
        w = heightAt(k, c) * a;
      }
      centers.push(c);
      prevC = c;
      prevW = w;
    }
    return { centers, end: prevC + prevW / 2 };
  };

  // 간격 자동: 줄이 선 전체(양 끝 약간 남김)에 고르게 퍼지는 간격을 찾는다.
  // 인형이 많아 겹치지 않고는 안 들어가면 AUTO_GAP_MIN까지 겹치고, 그래도 넘치면 아래에서 전체를 줄인다.
  if (layout.autoGap && items.length >= 2) {
    const target = L * AUTO_FILL;
    let lo = AUTO_GAP_MIN;
    let hi = AUTO_GAP_MAX;
    gap = lo;
    if (place(1, 0).end < target) {
      for (let it = 0; it < 30; it++) {
        const mid = (lo + hi) / 2;
        gap = mid;
        if (place(1, 0).end < target) lo = mid;
        else hi = mid;
      }
      gap = lo;
    }
  }

  const solve = (k: number) => {
    let offset = 0;
    let res = place(k, 0);
    for (let it = 0; it < 8; it++) {
      const span = res.end - offset;
      offset = Math.max(0, (L - span) / 2);
      res = place(k, offset);
    }
    return { ...res, offset };
  };

  const fits = (k: number) => {
    const res = solve(k);
    if (res.end - res.offset > L + 1e-9) return null;
    const placed: Placed[] = [];
    for (let i = 0; i < items.length; i++) {
      const c = res.centers[i];
      const p = ridge.at(c);
      const h = heightAt(k, c) * (items[i].size ?? 1);
      const halfW = (h * items[i].aspect) / screenAspect / 2;
      if (p.x - halfW < 0 || p.x + halfW > 1 || p.y - h < 0 || p.y > 1) return null;
      placed.push({ x: p.x, y: p.y, scale: h });
    }
    return placed;
  };

  if (items.length === 0) return { placed: [] as Placed[], k: 1, ridge, L };
  let k = 1;
  let placed = fits(1);
  if (!placed) {
    let lo = 0;
    let hi = 1;
    for (let it = 0; it < 30; it++) {
      const mid = (lo + hi) / 2;
      const r = fits(mid);
      if (r) {
        lo = mid;
        placed = r;
      } else {
        hi = mid;
      }
    }
    k = lo;
  }
  if (!placed) {
    k = 1e-3;
    // 능선이 화면 가장자리에 붙어 있는 등 어떤 크기로도 맞지 않는 경우: 아주 작게라도 화면 안에 둔다.
    const res = solve(1e-3);
    placed = items.map((_, i) => {
      const p = ridge.at(res.centers[i]);
      return { x: Math.min(0.999, Math.max(0.001, p.x)), y: Math.min(1, Math.max(0.01, p.y)), scale: 0.005 };
    });
  }
  return { placed, k, ridge, L };
}

