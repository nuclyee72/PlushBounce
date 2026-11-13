<script lang="ts">
  type Props = {
    value: number;
    min: number;
    max: number;
    step: number;
    label?: string;
    /** 양 끝에 적는 결과 (예: "살짝" / "크게"). */
    lo?: string;
    hi?: string;
    oninput: (v: number) => void;
  };
  let { value, min, max, step, label, lo, hi, oninput }: Props = $props();
  let el: HTMLInputElement;

  // value를 min/max/step보다 먼저 넣으면 브라우저가 기본 범위(0–100, step 1)로 잘라 버린다.
  // 범위가 적용된 뒤에 값을 넣는다.
  $effect(() => {
    void min, max, step;
    if (el && +el.value !== value) el.value = String(value);
    fill();
  });
  /** 트랙을 지금 값까지 채운다 (app.css의 --p). */
  function fill() {
    if (el) el.style.setProperty("--p", String(max > min ? (+el.value - min) / (max - min) : 0));
  }
</script>

<span class="slider">
  <input bind:this={el} type="range" {min} {max} {step} aria-label={label} oninput={(e) => (fill(), oninput(+e.currentTarget.value))} />
  {#if lo || hi}
    <span class="ends" aria-hidden="true"><span>{lo}</span><span>{hi}</span></span>
  {/if}
</span>

<style>
  .slider {
    display: grid;
    min-width: 0;
  }
  .ends {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    font-weight: 400;
    line-height: 1.2;
    color: var(--dim);
    opacity: 0.8;
  }
</style>
