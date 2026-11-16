<script lang="ts">
  import Icon from "./Icon.svelte";
  import { installUpdate, update } from "./store.svelte";

  const info = $derived(update.info);
  const pct = $derived(update.progress == null ? null : Math.round(update.progress * 100));
</script>

{#if info && (update.installing || update.dismissed !== info.version)}
  <div class="update" role="status">
    <Icon name="download" />
    <div class="text">
      {#if update.installing}
        <strong>Installing PlushBounce {info.version}…</strong>
        <span>{pct == null ? "Downloading" : `Downloading ${pct}%`}. The app will restart by itself.</span>
      {:else}
        <strong>PlushBounce {info.version} is available</strong>
        <span>You have {info.currentVersion}. Your dolls and settings are kept.</span>
        {#if info.notes}
          <details>
            <summary>What's new</summary>
            <pre>{info.notes}</pre>
          </details>
        {/if}
      {/if}
    </div>
    {#if update.installing}
      <div class="bar" style="--p:{update.progress ?? 0}" class:unknown={update.progress == null}></div>
    {:else}
      <button onclick={() => (update.dismissed = info.version)}>Later</button>
      <button class="on" onclick={installUpdate}>Install and restart</button>
    {/if}
  </div>
{/if}

<style>
  .update {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--raised);
    border: 1px solid var(--accent);
    color: var(--text);
  }
  .update > :global(.icon) {
    color: var(--accent);
    margin-top: 2px;
    flex: none;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .text span,
  summary {
    color: var(--dim);
  }
  summary {
    cursor: pointer;
    margin-top: 4px;
  }
  pre {
    margin: 6px 0 0;
    max-height: 140px;
    overflow: auto;
    white-space: pre-wrap;
    font: inherit;
    font-size: 0.92em;
    user-select: text;
    -webkit-user-select: text;
  }
  .bar {
    align-self: center;
    width: 120px;
    height: 6px;
    border-radius: 3px;
    background: linear-gradient(to right, var(--accent) 0 calc(var(--p) * 100%), var(--line) calc(var(--p) * 100%) 100%);
  }
  .bar.unknown {
    background: var(--line);
    animation: pulse 1s ease-in-out infinite alternate;
  }
  @keyframes pulse {
    to {
      background: var(--accent);
    }
  }
</style>
