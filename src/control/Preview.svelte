<script lang="ts">
  import { onMount } from "svelte";
  import { imageUrl } from "../shared/api";
  import type { Config } from "../shared/types";
  import { StageRenderer } from "../stage/renderer";
  import { app, commitWhole, setRenderer } from "./store.svelte";

  let wrap: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let renderer = $state.raw<StageRenderer | null>(null);
  let box = $state({ w: 0, h: 0 });

  const aspect = $derived.by(() => {
    const m = app.monitors[app.cfg?.display.monitor ?? 0] ?? app.monitors[0];
    return m ? m.width / m.height : 16 / 9;
  });
  const size = $derived.by(() => {
    const w = Math.min(box.w, box.h * aspect);
    return { w: Math.floor(w), h: Math.floor(w / aspect) };
  });

  onMount(() => {
    const ro = new ResizeObserver(() => (box = { w: wrap.clientWidth, h: wrap.clientHeight }));
    ro.observe(wrap);
    const r = new StageRenderer(canvas, {
      preview: true,
      imageUrl: (file) => imageUrl(app.imagesDir, file),
      onEdit: (c) => commitWhole(c),
    });
    r.setEditing(true);
    r.onSelect = (id) => {
      app.selected = id;
    };
    r.onSelectLine = (id) => {
      app.line = id;
    };
    renderer = r;
    setRenderer(r);
    return () => {
      ro.disconnect();
      setRenderer(null);
      r.destroy();
    };
  });

  $effect(() => {
    if (renderer && app.cfg) renderer.setConfig($state.snapshot(app.cfg) as Config);
  });
  $effect(() => {
    renderer?.select(app.selected);
  });
  $effect(() => {
    renderer?.selectLine(app.line);
  });
</script>

<div class="wrap" bind:this={wrap}>
  <canvas bind:this={canvas} style="width:{size.w}px;height:{size.h}px"></canvas>
</div>

<style>
  .wrap {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 0;
  }
  canvas {
    border-radius: 10px;
    border: 1px solid var(--line);
    touch-action: none;
  }
</style>
