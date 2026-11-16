<script lang="ts">
  import { onMount } from "svelte";
  import AudioTab from "./AudioTab.svelte";
  import DisplayTab from "./DisplayTab.svelte";
  import DollsTab from "./DollsTab.svelte";
  import Icon from "./Icon.svelte";
  import Preview from "./Preview.svelte";
  import UpdateBanner from "./UpdateBanner.svelte";
  import { app, fail, init } from "./store.svelte";

  const tabs = [
    ["dolls", "Dolls"],
    ["audio", "Audio"],
    ["display", "Display"],
  ] as const;

  const problem = $derived(app.error ?? app.runtime.displayError ?? app.runtime.audioError);

  onMount(() => {
    init().catch(fail);
  });
</script>

{#if app.cfg}
  <main>
    <section class="preview"><Preview /></section>
    <div class="tabs" role="tablist">
      {#each tabs as [id, label]}
        <button role="tab" aria-selected={app.tab === id} class:on={app.tab === id} onclick={() => (app.tab = id)}>{label}</button>
      {/each}
    </div>
    <section class="panel">
      {#if app.tab === "dolls"}
        <DollsTab />
      {:else if app.tab === "audio"}
        <AudioTab />
      {:else}
        <DisplayTab />
      {/if}
    </section>
    <div class="notices">
      <UpdateBanner />
      {#if problem}
        <div class="error" role="alert">
          <span>{problem}</span>
          {#if app.error}
            <button aria-label="Close" onclick={() => (app.error = null)}><Icon name="x" /></button>
          {/if}
        </div>
      {/if}
    </div>
  </main>
{/if}

<style>
  main {
    height: 100%;
    display: grid;
    grid-template-rows: minmax(160px, 44%) auto 1fr auto;
    gap: 10px;
    padding: 12px;
  }
  .preview {
    min-height: 0;
  }
  .tabs {
    display: flex;
    gap: 6px;
  }
  .tabs button {
    border-color: transparent;
    background: none;
    color: var(--dim);
    padding: 6px 14px;
  }
  .tabs button.on {
    background: var(--raised);
    color: var(--text);
    border-color: var(--line);
  }
  .panel > :global(*) {
    animation: rise 0.18s ease-out;
  }
  .panel {
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 2px;
  }
  .notices {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .notices:empty {
    display: none;
  }
  .error {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-radius: 10px;
    background: #3a2226;
    border: 1px solid #6b3138;
    color: #ffc9cd;
    user-select: text;
    -webkit-user-select: text;
  }
  .error span {
    flex: 1;
  }
  .error button {
    padding: 2px;
    background: none;
    border: 0;
  }
</style>
