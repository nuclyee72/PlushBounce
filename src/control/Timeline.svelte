<script lang="ts">
  import { onMount } from "svelte";
  import { BAND_COLORS, BAND_IDS, BAND_LABELS } from "../shared/types";
  import { app, history, live } from "./store.svelte";

  /** 보여 주는 시간 폭 (ms). 오른쪽 끝이 지금이다. */
  const WINDOW_MS = 6000;
  const LABEL_W = 92;
  const ROW_H = 26;
  const GAP = 4;
  const AXIS_H = 16;
  const ROWS = BAND_IDS.length + 1; // 대역들 + 박

  let canvas: HTMLCanvasElement;
  let wrap: HTMLDivElement;

  const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

  onMount(() => {
    const ctx = canvas.getContext("2d")!;
    let raf = 0;
    let w = 0;
    const h = ROWS * (ROW_H + GAP) + AXIS_H;
    const resize = () => {
      w = wrap.clientWidth;
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
      canvas.style.width = `${w}px`;
      canvas.style.height = `${h}px`;
    };
    const ro = new ResizeObserver(resize);
    ro.observe(wrap);
    resize();

    const raised = css("--raised") || "#262a37";
    const dim = css("--dim") || "#8e94a8";
    const text = css("--text") || "#e8eaf1";
    const accent = css("--accent") || "#ffd166";

    const draw = () => {
      raf = requestAnimationFrame(draw);
      if (!w) return;
      const dpr = canvas.width / w;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      const now = performance.now();
      const x0 = LABEL_W;
      const laneW = Math.max(1, w - x0 - 4);
      const X = (t: number) => x0 + (1 - (now - t) / WINDOW_MS) * laneW;
      const rowY = (i: number) => i * (ROW_H + GAP);
      const lanesBottom = rowY(ROWS) - GAP;

      // 레인 바탕
      for (let i = 0; i < ROWS; i++) {
        ctx.fillStyle = raised;
        ctx.beginPath();
        ctx.roundRect(x0, rowY(i), laneW, ROW_H, 5);
        ctx.fill();
      }

      ctx.save();
      ctx.beginPath();
      ctx.rect(x0, 0, laneW, lanesBottom);
      ctx.clip();

      // 박: 모든 레인을 가로지르는 세로선
      ctx.strokeStyle = "rgba(255,255,255,0.10)";
      ctx.lineWidth = 1;
      for (const t of history.beats) {
        const x = Math.round(X(t)) + 0.5;
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, lanesBottom);
        ctx.stroke();
      }

      BAND_IDS.forEach((b, i) => {
        const y = rowY(i);
        const color = BAND_COLORS[b];
        // 에너지: 옅은 면
        ctx.beginPath();
        ctx.moveTo(x0, y + ROW_H);
        for (const s of history.energy) ctx.lineTo(X(s.t), y + ROW_H - 2 - s.e[b] * (ROW_H - 4));
        // 다음 프레임이 올 때까지 마지막 값을 "지금"까지 이어 그린다 (끝이 물러났다 튀어나오며 떨리지 않게).
        const last = history.energy[history.energy.length - 1];
        if (last) {
          const ly = y + ROW_H - 2 - last.e[b] * (ROW_H - 4);
          ctx.lineTo(X(now), ly);
          ctx.lineTo(X(now), y + ROW_H);
        }
        ctx.closePath();
        ctx.globalAlpha = 0.22;
        ctx.fillStyle = color;
        ctx.fill();
        // 감지(온셋): 진한 막대, 높이는 세기
        ctx.globalAlpha = 1;
        for (const o of history.onsets[b]) {
          const bh = (0.35 + 0.65 * o.s) * (ROW_H - 4);
          ctx.fillRect(X(o.t) - 1.5, y + ROW_H - 2 - bh, 3, bh);
        }
      });

      // 박 레인
      const by = rowY(BAND_IDS.length);
      ctx.fillStyle = accent;
      for (const t of history.beats) {
        ctx.beginPath();
        ctx.arc(X(t), by + ROW_H / 2, 4, 0, Math.PI * 2);
        ctx.fill();
      }

      // 역치 아래(무음) 구간은 어둡게. 무음 중에는 프레임이 0.1초마다만 와서, 마지막 구간을 프레임 시각에서
      // 끊으면 오른쪽 끝에 밝은 틈이 열렸다 닫혔다 하며 떨린다 → 무음이 이어지는 동안은 "지금"까지 칠한다.
      ctx.fillStyle = "rgba(10,11,16,0.55)";
      const lastSilent = history.silent[history.silent.length - 1];
      for (const s of history.silent) {
        const a = X(s.from - 60);
        const z = s === lastSilent && history.silentNow ? X(now) + 1 : X(s.to + 60);
        ctx.fillRect(a, 0, z - a, lanesBottom);
      }
      ctx.restore();

      // 이름표: 온셋이 오면 점이 깜빡인다
      ctx.font = "12px Pretendard, 'Segoe UI', 'Malgun Gothic', sans-serif";
      ctx.textBaseline = "middle";
      BAND_IDS.forEach((b, i) => {
        const cy = rowY(i) + ROW_H / 2;
        const f = live.flash[b];
        ctx.fillStyle = BAND_COLORS[b];
        ctx.globalAlpha = 0.35 + 0.65 * f;
        ctx.beginPath();
        ctx.arc(8, cy, 5 + 2 * f, 0, Math.PI * 2);
        ctx.fill();
        ctx.globalAlpha = 1;
        // 이름 아래에 그 대역의 주파수 범위를 작게 적는다.
        const band = app.cfg?.audio.bands[b];
        ctx.fillStyle = text;
        ctx.font = "12px Pretendard, 'Segoe UI', 'Malgun Gothic', sans-serif";
        ctx.fillText(BAND_LABELS[b], 20, band ? cy - 5 : cy);
        if (band) {
          const hz = (v: number) => (v >= 1000 ? `${+(v / 1000).toFixed(1)}k` : `${Math.round(v)}`);
          ctx.fillStyle = dim;
          ctx.font = "9.5px Pretendard, 'Segoe UI', sans-serif";
          ctx.fillText(`${hz(band.lo)}–${hz(band.hi)}Hz`, 20, cy + 7);
        }
      });
      ctx.font = "12px Pretendard, 'Segoe UI', 'Malgun Gothic', sans-serif";
      const beatY = rowY(BAND_IDS.length) + ROW_H / 2;
      ctx.fillStyle = accent;
      ctx.globalAlpha = 0.35 + 0.65 * live.beat;
      ctx.beginPath();
      ctx.arc(8, beatY, 5 + 2 * live.beat, 0, Math.PI * 2);
      ctx.fill();
      ctx.globalAlpha = 1;
      ctx.fillStyle = text;
      ctx.fillText("박", 20, beatY);

      // 시간 눈금
      ctx.fillStyle = dim;
      ctx.font = "10px Pretendard, 'Segoe UI', sans-serif";
      ctx.textBaseline = "alphabetic";
      for (let s = 0; s <= WINDOW_MS / 1000; s++) {
        const x = x0 + (1 - (s * 1000) / WINDOW_MS) * laneW;
        ctx.textAlign = s === 0 ? "right" : "center";
        ctx.fillText(s === 0 ? "지금" : `-${s}s`, s === 0 ? x0 + laneW : x, h - 3);
      }
      ctx.textAlign = "left";
    };
    raf = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  });
</script>

<div class="timeline">
  <div bind:this={wrap}>
    <canvas bind:this={canvas} aria-label="최근 감지 기록"></canvas>
  </div>
</div>

<style>
  .timeline {
    padding: 10px 10px 6px;
    background: var(--panel);
    border-radius: 12px;
  }
  canvas {
    display: block;
  }
</style>
