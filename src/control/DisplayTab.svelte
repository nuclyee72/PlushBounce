<script lang="ts">
  import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { api } from "../shared/api";
  import Icon from "./Icon.svelte";
  import Slider from "./Slider.svelte";
  import { app, checkForUpdates, commit, fail, update } from "./store.svelte";

  const cfg = $derived(app.cfg!);
  const bg = $derived(cfg.display.background);
  const wallpaper = $derived(cfg.display.mode === "wallpaper");
  let autostart = $state(false);

  onMount(() => {
    isEnabled().then((v) => (autostart = v), () => {});
  });

  async function toggleAutostart() {
    try {
      await (autostart ? disable() : enable());
      autostart = await isEnabled();
    } catch {
      fail("Couldn't change run-at-startup. Please try again.");
    }
  }

  async function pickBackground() {
    const p = await open({ filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "jfif", "webp", "gif", "bmp", "avif", "ico"] }] });
    if (typeof p === "string") api.setBackgroundImage(p).then((c) => (app.cfg = c), fail);
  }

  async function exportPreset() {
    const p = await save({ defaultPath: "plushbounce-preset.json", filters: [{ name: "Preset", extensions: ["json"] }] });
    if (p) api.exportPreset(p).catch(fail);
  }

  async function importPreset() {
    const p = await open({ filters: [{ name: "Preset", extensions: ["json"] }] });
    if (typeof p === "string") api.importPreset(p).then((c) => (app.cfg = c), fail);
  }
</script>

<div class="row">
  <div class="seg" role="group" aria-label="Display mode">
    <button class:on={!wallpaper} onclick={() => commit((c) => (c.display.mode = "pet"))}><Icon name="pet" />Pet</button>
    <button class:on={wallpaper} onclick={() => commit((c) => (c.display.mode = "wallpaper"))}><Icon name="wallpaper" />Wallpaper</button>
  </div>
  {#if app.monitors.length > 1}
    <select aria-label="Monitor" value={cfg.display.monitor} onchange={(e) => commit((c) => (c.display.monitor = +e.currentTarget.value))}>
      {#each app.monitors as m, i}
        <option value={i}>{i + 1} · {m.width}×{m.height}</option>
      {/each}
    </select>
  {/if}
  {#if !wallpaper}
    <button class:on={app.runtime.editMode} title="Ctrl+Alt+P" onclick={() => api.setEditMode(!app.runtime.editMode).catch(fail)}>
      <Icon name="edit" />Edit on screen
    </button>
  {/if}
</div>

<!-- 마우스가 가까이 오면 인형이 흐려진다 (인형 뒤의 것을 누르기 쉽게). 슬라이더는 가장 흐려졌을 때. -->
<div class="row">
  <button class:on={cfg.display.hoverFade !== false} aria-pressed={cfg.display.hoverFade !== false} onclick={() => commit((c) => (c.display.hoverFade = c.display.hoverFade === false))}
    ><Icon name="eye" />Fade near mouse</button
  >
  <label class="field" class:muted={cfg.display.hoverFade === false}
    >Most transparent<Slider
      lo="Barely"
      hi="Invisible"
      min={0.1}
      max={1}
      step={0.01}
      value={cfg.display.hoverFadeMax ?? 0.7}
      oninput={(v) => commit((c) => ((c.display.hoverFadeMax = v), (c.display.hoverFade = true)))}
    /></label
  >
  <span class="pct">{Math.round((cfg.display.hoverFadeMax ?? 0.7) * 100)}%</span>
</div>

{#if wallpaper}
  <div class="row">
    <div class="seg" role="group" aria-label="Background">
      <button class:on={bg.kind === "none"} title="Transparent" aria-label="Transparent" onclick={() => commit((c) => (c.display.background = { kind: "none" }))}>
        <i class="sw checker"></i>
      </button>
      <button
        class:on={bg.kind === "color"}
        title="Solid color"
        aria-label="Solid color"
        onclick={() => commit((c) => (c.display.background = { kind: "color", value: bg.kind === "color" ? bg.value : "#20263a" }))}
      >
        <i class="sw" style="background:{bg.kind === 'color' ? bg.value : '#20263a'}"></i>
      </button>
      <button class:on={bg.kind === "image"} title="Image" aria-label="Image" onclick={pickBackground}><Icon name="image" /></button>
    </div>
    {#if bg.kind === "color"}
      <input type="color" aria-label="Background color" value={bg.value} oninput={(e) => commit((c) => (c.display.background = { kind: "color", value: e.currentTarget.value }))} />
    {/if}
  </div>
{/if}

<div class="row">
  <button class:on={autostart} onclick={toggleAutostart}><Icon name="power" />Run at startup</button>
  <span class="spacer"></span>
  <button onclick={exportPreset}><Icon name="download" />Save preset</button>
  <button onclick={importPreset}><Icon name="upload" />Load preset</button>
</div>

<div class="row">
  <span class="version">PlushBounce {update.version}</span>
  {#if update.info}
    <button class="on" disabled={update.installing} onclick={() => (update.dismissed = null)}><Icon name="download" />Update to {update.info.version}</button>
  {:else}
    <button disabled={update.checking} onclick={checkForUpdates}><Icon name="refresh" />{update.checking ? "Checking…" : "Check for updates"}</button>
    {#if update.status}<span class="version">{update.status}</span>{/if}
  {/if}
</div>

<style>
  .sw {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    border: 1px solid var(--line);
    display: block;
  }
  .checker {
    background: repeating-conic-gradient(#555b70 0% 25%, #2b2f3d 0% 50%) 50% / 10px 10px;
  }
  .version {
    color: var(--dim);
  }
  .spacer {
    flex: 1;
  }
  .field {
    flex: 1;
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: start;
    gap: 10px;
    max-width: 360px;
    transition: opacity 0.15s;
  }
  .field.muted {
    opacity: 0.45;
  }
  .pct {
    min-width: 40px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--dim);
  }
</style>
