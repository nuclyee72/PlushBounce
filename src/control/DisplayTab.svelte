<script lang="ts">
  import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { api } from "../shared/api";
  import Icon from "./Icon.svelte";
  import { app, commit, fail } from "./store.svelte";

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
      fail("자동 실행을 바꾸지 못했습니다. 다시 시도해 주세요.");
    }
  }

  async function pickBackground() {
    const p = await open({ filters: [{ name: "이미지", extensions: ["png", "jpg", "jpeg", "jfif", "webp", "gif", "bmp", "avif", "ico"] }] });
    if (typeof p === "string") api.setBackgroundImage(p).then((c) => (app.cfg = c), fail);
  }

  async function exportPreset() {
    const p = await save({ defaultPath: "plushbounce-preset.json", filters: [{ name: "프리셋", extensions: ["json"] }] });
    if (p) api.exportPreset(p).catch(fail);
  }

  async function importPreset() {
    const p = await open({ filters: [{ name: "프리셋", extensions: ["json"] }] });
    if (typeof p === "string") api.importPreset(p).then((c) => (app.cfg = c), fail);
  }
</script>

<div class="row">
  <div class="seg" role="group" aria-label="표시 모드">
    <button class:on={!wallpaper} onclick={() => commit((c) => (c.display.mode = "pet"))}><Icon name="pet" />펫</button>
    <button class:on={wallpaper} onclick={() => commit((c) => (c.display.mode = "wallpaper"))}><Icon name="wallpaper" />바탕화면</button>
  </div>
  {#if app.monitors.length > 1}
    <select aria-label="모니터" value={cfg.display.monitor} onchange={(e) => commit((c) => (c.display.monitor = +e.currentTarget.value))}>
      {#each app.monitors as m, i}
        <option value={i}>{i + 1} · {m.width}×{m.height}</option>
      {/each}
    </select>
  {/if}
  {#if !wallpaper}
    <button class:on={app.runtime.editMode} title="Ctrl+Alt+P" onclick={() => api.setEditMode(!app.runtime.editMode).catch(fail)}>
      <Icon name="edit" />화면에서 편집
    </button>
  {/if}
</div>

{#if wallpaper}
  <div class="row">
    <div class="seg" role="group" aria-label="배경">
      <button class:on={bg.kind === "none"} title="투명" aria-label="투명" onclick={() => commit((c) => (c.display.background = { kind: "none" }))}>
        <i class="sw checker"></i>
      </button>
      <button
        class:on={bg.kind === "color"}
        title="단색"
        aria-label="단색"
        onclick={() => commit((c) => (c.display.background = { kind: "color", value: bg.kind === "color" ? bg.value : "#20263a" }))}
      >
        <i class="sw" style="background:{bg.kind === 'color' ? bg.value : '#20263a'}"></i>
      </button>
      <button class:on={bg.kind === "image"} title="이미지" aria-label="이미지" onclick={pickBackground}><Icon name="image" /></button>
    </div>
    {#if bg.kind === "color"}
      <input type="color" aria-label="배경색" value={bg.value} oninput={(e) => commit((c) => (c.display.background = { kind: "color", value: e.currentTarget.value }))} />
    {/if}
  </div>
{/if}

<div class="row">
  <button class:on={autostart} onclick={toggleAutostart}><Icon name="power" />시작 시 실행</button>
  <span class="spacer"></span>
  <button onclick={exportPreset}><Icon name="download" />프리셋 저장</button>
  <button onclick={importPreset}><Icon name="upload" />프리셋 열기</button>
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
  .spacer {
    flex: 1;
  }
</style>
