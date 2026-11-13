<script lang="ts">
  import { LATENCY_MAX_MS, LATENCY_MIN_MS } from "../shared/types";
  import Icon from "./Icon.svelte";
  import { app, commit, getRenderer } from "./store.svelte";

  // 타이밍(모든 인형 공통): 인형이 소리보다 늦어 보이면 −, 빨라 보이면 +. 숫자를 두 번 누르면 0.
  // − / + 를 누르면 10ms씩, 누르고 있으면 계속 바뀐다. 바꿀 때 미리보기 인형을 튀겨 보여 준다.
  const latency = $derived(Math.round(app.cfg?.motion.latencyMs ?? 0));
  const clampLatency = (v: number) => Math.min(LATENCY_MAX_MS, Math.max(LATENCY_MIN_MS, Math.round(v)));
  const setLatency = (v: number) => {
    if (!Number.isFinite(v)) return;
    commit((c) => (c.motion.latencyMs = clampLatency(v)));
    getRenderer()?.demo();
  };
  let repeatTimer: ReturnType<typeof setTimeout> | null = null;
  let repeated = false;
  function holdStart(step: number) {
    repeated = false;
    stopHold();
    repeatTimer = setTimeout(function again() {
      repeated = true;
      setLatency((app.cfg?.motion.latencyMs ?? 0) + step);
      repeatTimer = setTimeout(again, 70);
    }, 400);
  }
  function stopHold() {
    if (repeatTimer) clearTimeout(repeatTimer);
    repeatTimer = null;
  }
  function nudge(step: number) {
    // 누르고 있다가 뗀 경우는 이미 바뀌었으므로 한 번 더 더하지 않는다.
    if (repeated) repeated = false;
    else setLatency(latency + step);
  }
</script>

<div class="timing" class:set={latency !== 0} role="group" aria-label="Timing" title="Timing: move dolls earlier (−) or later (+)">
  <Icon name="clock" />
  <button
    class="round"
    aria-label="Earlier"
    disabled={latency <= LATENCY_MIN_MS}
    onpointerdown={() => holdStart(-10)}
    onpointerup={stopHold}
    onpointerleave={stopHold}
    onclick={() => nudge(-10)}><Icon name="minus" /></button
  >
  <label class="val">
    <input
      type="number"
      step="10"
      min={LATENCY_MIN_MS}
      max={LATENCY_MAX_MS}
      aria-label="Timing ms"
      value={latency}
      onchange={(e) => setLatency(e.currentTarget.valueAsNumber)}
      ondblclick={() => setLatency(0)}
    /><small>ms</small>
  </label>
  <button
    class="round"
    aria-label="Later"
    disabled={latency >= LATENCY_MAX_MS}
    onpointerdown={() => holdStart(10)}
    onpointerup={stopHold}
    onpointerleave={stopHold}
    onclick={() => nudge(10)}><Icon name="plus" /></button
  >
</div>

<style>
  .timing {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 34px;
    padding: 0 4px 0 10px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: var(--panel);
    color: var(--dim);
    transition: border-color 0.15s;
  }
  .timing.set {
    border-color: color-mix(in srgb, var(--accent) 55%, var(--line));
    color: var(--accent);
  }
  .timing > :global(svg.icon) {
    width: 16px;
    height: 16px;
    margin-right: 2px;
  }
  .round {
    width: 26px;
    height: 26px;
    padding: 0;
    justify-content: center;
    border-radius: 50%;
    border: 0;
    background: var(--raised);
    color: var(--text);
    transition:
      background 0.12s,
      transform 0.08s;
  }
  .round :global(svg.icon) {
    width: 14px;
    height: 14px;
    stroke-width: 2.4;
  }
  .round:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 30%, var(--raised));
  }
  .round:active:not(:disabled) {
    transform: scale(0.9);
  }
  .val {
    display: inline-flex;
    align-items: baseline;
    gap: 1px;
    min-width: 56px;
    justify-content: center;
    font-variant-numeric: tabular-nums;
    cursor: text;
  }
  .val input {
    width: auto;
    min-width: 1ch;
    field-sizing: content;
    padding: 0;
    border: 0;
    background: none;
    text-align: center;
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    -moz-appearance: textfield;
  }
  .timing.set .val input {
    color: var(--accent);
  }
  .val input::-webkit-inner-spin-button {
    -webkit-appearance: none;
  }
  .val input:focus-visible {
    outline: none;
  }
  .timing:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .val small {
    font-size: 10px;
    color: var(--dim);
  }
</style>
