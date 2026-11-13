<script lang="ts">
  import { BAND_COLORS, BAND_IDS, BAND_LABELS } from "../shared/types";
  import type { BandId } from "../shared/types";
  import Icon from "./Icon.svelte";
  import Slider from "./Slider.svelte";
  import Timeline from "./Timeline.svelte";
  import Timing from "./Timing.svelte";
  import { app, commit, live, refreshSessions } from "./store.svelte";

  const cfg = $derived(app.cfg!);
  const source = $derived(cfg.audio.source);
  /** 고른 앱들 (실행 파일 이름). 비어 있으면 시스템 전체. */
  const chosen = $derived(source.kind === "apps" ? source.names : []);
  const isOn = (name: string) => chosen.some((n) => n.toLowerCase() === name.toLowerCase());

  // 소리를 내는 앱 목록. 골라 둔 앱이 지금 꺼져 있어도 목록에 남겨 끌 수 있게 한다.
  const options = $derived.by(() => {
    const list = app.sessions.map((s) => ({ name: s.name, label: s.display, running: true }));
    for (const n of chosen) {
      if (!list.some((o) => o.name.toLowerCase() === n.toLowerCase())) {
        list.push({ name: n, label: n.replace(/\.exe$/i, ""), running: false });
      }
    }
    return list;
  });

  function toggleApp(name: string) {
    const next = isOn(name) ? chosen.filter((n) => n.toLowerCase() !== name.toLowerCase()) : [...chosen, name];
    commit((c) => (c.audio.source = next.length ? { kind: "apps", names: next } : { kind: "system" }));
  }

  // 역치 눈금 (dBFS). 막대와 손잡이가 같은 눈금을 쓴다.
  const GATE_MIN = -80;
  const GATE_MAX = -6;
  const pct = (db: number) => Math.min(100, Math.max(0, ((db - GATE_MIN) / (GATE_MAX - GATE_MIN)) * 100));
  let gateEl = $state<HTMLInputElement | null>(null);
  $effect(() => {
    // min/max가 적용된 뒤에 값을 넣는다 (Slider.svelte와 같은 이유).
    const v = cfg.audio.gateDb;
    if (gateEl && +gateEl.value !== v) gateEl.value = String(v);
  });

  // 감도 슬라이더는 오른쪽이 민감하도록 임계 배수 k(1..4)를 뒤집어 보여 준다.
  const toSlider = (k: number) => 5 - k;
  const autoGap = $derived(cfg.audio.autoInterval !== false);
  const setBand = (b: BandId, key: "lo" | "hi" | "sensitivity", v: number) => {
    if (Number.isFinite(v)) commit((c) => (c.audio.bands[b][key] = v));
  };
</script>

<!-- 소리 소스: 시스템 전체, 또는 켠(O) 앱들의 소리만 합쳐서 -->
<div class="sources" role="group" aria-label="Sound source">
  <button class="src" class:on={chosen.length === 0} aria-pressed={chosen.length === 0} onclick={() => commit((c) => (c.audio.source = { kind: "system" }))}>
    <Icon name="speaker" />Whole system
  </button>
  {#each options as o (o.name.toLowerCase())}
    {@const on = isOn(o.name)}
    <button class="src app" class:on class:gone={!o.running} aria-pressed={on} title={o.running ? o.name : `${o.name} (not running)`} onclick={() => toggleApp(o.name)}>
      <span class="ox" class:o={on}><Icon name={on ? "circle" : "x"} /></span>{o.label}
    </button>
  {/each}
  <button class="refresh" aria-label="Refresh" title="Refresh" onclick={refreshSessions}><Icon name="refresh" /></button>
  <span class="bpm" style="opacity:{live.bpm ? 0.55 + 0.45 * live.beat : 0.35}">{live.bpm ?? "–"} <small>BPM</small></span>
</div>

<!-- 역치: 막대가 입력 크기, 손잡이가 기준선. 기준선보다 작은 소리에는 인형이 반응하지 않는다.
     같은 줄 오른쪽: 타이밍(모든 인형 공통 지연 보정). -->
<div class="gate">
  <span class="name">Gate</span>
  <div class="track">
    <div class="input" class:under={live.input < cfg.audio.gateDb} style="width:{pct(live.input)}%"></div>
    <div class="cut" style="width:{pct(cfg.audio.gateDb)}%"></div>
    <input
      type="range"
      min={GATE_MIN}
      max={GATE_MAX}
      step="1"
      aria-label="Gate"
      bind:this={gateEl}
      oninput={(e) => commit((c) => (c.audio.gateDb = +e.currentTarget.value))}
    />
  </div>
  <span class="timing-slot"><Timing /></span>
  <span class="ends" aria-hidden="true"><span>Quiet sounds too</span><span>Loud only</span></span>
</div>

<!-- 최근 감지 기록: 대역마다 언제 소리가 잡혔는지 (막대 = 감지, 높이 = 세기, 옅은 면 = 크기) -->
<Timeline />

<div class="grid">
  <span></span><span class="head">Sensitivity<small>less ↔ more</small></span><span class="head">Hz</span>
  {#each BAND_IDS as b}
    <span class="name" style="--band:{BAND_COLORS[b]}"><i></i>{BAND_LABELS[b]}</span>
    <Slider min={1} max={4} step={0.05} label="{BAND_LABELS[b]} sensitivity" value={toSlider(cfg.audio.bands[b].sensitivity)} oninput={(v) => setBand(b, "sensitivity", 5 - v)} />
    <span class="hz">
      <input type="number" min="20" max="19000" aria-label="{BAND_LABELS[b]} from" value={cfg.audio.bands[b].lo} onchange={(e) => setBand(b, "lo", e.currentTarget.valueAsNumber)} />
      –
      <input type="number" min="21" max="20000" aria-label="{BAND_LABELS[b]} to" value={cfg.audio.bands[b].hi} onchange={(e) => setBand(b, "hi", e.currentTarget.valueAsNumber)} />
    </span>
  {/each}
  <!-- 간격: 한 대역에서 소리를 다시 잡기까지의 최소 시간. 자동이면 박에 맞춘다(16분음표보다 조금 짧게). 손으로 끌면 자동이 꺼진다. -->
  <span class="name">Gap</span>
  <span class="gaprow">
    <button class="auto" class:on={autoGap} aria-pressed={autoGap} title="Auto gap" aria-label="Auto gap" onclick={() => commit((c) => (c.audio.autoInterval = !autoGap))}
      >Auto</button
    >
    <span class="gapslider" class:muted={autoGap}>
      <Slider
        min={40}
        max={400}
        step={5}
        lo="Dense"
        hi="Sparse"
        label="Minimum onset gap"
        value={autoGap ? (live.gapMs ?? cfg.audio.minOnsetIntervalMs) : cfg.audio.minOnsetIntervalMs}
        oninput={(v) => commit((c) => ((c.audio.minOnsetIntervalMs = v), (c.audio.autoInterval = false)))}
      />
    </span>
  </span>
  <span class="hz ms">{autoGap ? (live.gapMs ?? "–") : Math.round(cfg.audio.minOnsetIntervalMs)} ms</span>
</div>

<style>
  .sources {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .src {
    padding: 5px 10px 5px 8px;
    border-radius: 999px;
    color: var(--dim);
  }
  .src.on {
    color: var(--accent);
  }
  .src.gone {
    border-style: dashed;
    opacity: 0.6;
  }
  .ox {
    display: inline-flex;
    width: 18px;
    height: 18px;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    background: var(--panel);
    color: var(--dim);
  }
  .ox :global(svg.icon) {
    width: 13px;
    height: 13px;
  }
  .ox.o {
    background: var(--accent);
    color: #1b1e29;
  }
  .ox.o :global(svg.icon) {
    stroke-width: 3;
  }
  .refresh {
    padding: 5px;
    border-radius: 999px;
  }
  .bpm {
    margin-left: auto;
    font-size: 20px;
    font-variant-numeric: tabular-nums;
  }
  .bpm small {
    font-size: 11px;
    color: var(--dim);
  }
  .timing-slot {
    grid-column: 3;
    grid-row: 1 / span 2;
    align-self: start;
    margin-left: 6px;
  }
  .gate {
    display: grid;
    grid-template-columns: 70px 1fr auto;
    align-items: center;
    gap: 2px 8px;
  }
  .gate .ends {
    grid-column: 2;
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--dim);
    opacity: 0.8;
  }
  .track {
    position: relative;
    height: 22px;
    border-radius: 6px;
    background: var(--raised);
    overflow: hidden;
  }
  .track .input {
    position: absolute;
    inset: 0 auto 0 0;
    background: #3ddc97;
  }
  .track .input.under {
    background: #5b6178;
  }
  /* 기준선 아래(무시되는 구간)를 어둡게 덮는다. */
  .track .cut {
    position: absolute;
    inset: 0 auto 0 0;
    background: rgba(10, 11, 16, 0.55);
    border-right: 2px solid var(--accent);
  }
  .track input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    opacity: 0;
    cursor: ew-resize;
  }
  .track:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .grid {
    display: grid;
    grid-template-columns: 70px 1fr auto;
    gap: 8px 14px;
    align-items: center;
    color: var(--dim);
    border-top: 1px solid var(--line);
    padding-top: 10px;
  }
  .name {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text);
  }
  .name i {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--band);
  }
  .hz {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-variant-numeric: tabular-nums;
  }
  .hz input {
    width: 64px;
    padding: 5px 6px;
    text-align: center;
  }
  .hz.ms {
    min-width: 64px;
    justify-content: flex-end;
    color: var(--text);
  }
  .head {
    display: inline-flex;
    align-items: baseline;
    gap: 8px;
  }
  .head small {
    font-size: 11px;
    opacity: 0.75;
  }
  .gaprow {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    min-width: 0;
  }
  .gaprow .auto {
    padding: 3px 10px;
    border-radius: 999px;
    font-size: 12px;
    flex: none;
  }
  .gapslider {
    flex: 1;
    min-width: 0;
    transition: opacity 0.15s;
  }
  .gapslider.muted {
    opacity: 0.45;
  }
</style>
