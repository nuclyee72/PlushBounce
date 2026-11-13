<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, imageUrl } from "../shared/api";
  import { BAND_COLORS, BAND_LABELS, DOLL_BAND_COLORS, DOLL_BAND_LABELS, DOLL_BANDS, DOLL_EXTENSIONS, LATENCY_MAX_MS, LATENCY_MIN_MS } from "../shared/types";
  import type { BandId, Doll, Line } from "../shared/types";
  import Icon from "./Icon.svelte";
  import Slider from "./Slider.svelte";
  import { addDolls, app, commit, fail, getRenderer, live } from "./store.svelte";

  const cfg = $derived(app.cfg!);
  const latency = $derived(Math.round(cfg.motion.latencyMs ?? 0));
  const dolls = $derived([...cfg.dolls].sort((a, b) => a.order - b.order));
  const selected = $derived(cfg.dolls.find((d) => d.id === app.selected) ?? null);
  // 원근·가까운 쪽·겹침은 선마다 따로다. 지금 다루는 선(없으면 첫 번째 선)의 값을 보여 주고 바꾼다.
  const line = $derived(cfg.lines.find((l) => l.id === app.line) ?? cfg.lines[0]);
  const editLine = (f: (l: Line) => void) =>
    commit((c) => {
      const l = c.lines.find((x) => x.id === line.id);
      if (l) f(l);
    });

  async function pick() {
    const picked = await open({
      multiple: true,
      filters: [{ name: "이미지", extensions: DOLL_EXTENSIONS }],
    });
    if (picked) await addDolls(Array.isArray(picked) ? picked : [picked]);
  }

  function remove(id: string) {
    if (app.selected === id) app.selected = null;
    api.removeDoll(id).then((c) => (app.cfg = c), fail);
  }

  const edit = (id: string, f: (d: Doll) => void) =>
    commit((c) => {
      const d = c.dolls.find((x) => x.id === id);
      if (d) f(d);
    });

  /** 움직임 설정을 바꾸면서 미리보기 인형을 튀겨 보여 준다. */
  function motion(mutate: Parameters<typeof commit>[0]) {
    commit(mutate);
    getRenderer()?.demo();
  }

  /** 인형 테두리가 깜빡이는 정도: 박 인형은 박마다. */
  const flashOf = (d: Doll) => (!d.visible ? 0 : d.band === "beat" ? live.beat : live.flash[d.band]);
  /** 대역의 범위 안내 (박은 대역이 아니다). */
  const bandTitle = (b: (typeof DOLL_BANDS)[number]) =>
    b === "beat" ? DOLL_BAND_LABELS[b] : `${BAND_LABELS[b]} ${cfg.audio.bands[b].lo}–${cfg.audio.bands[b].hi}Hz`;

  /**
   * 간격 자동 켜기/끄기. 켤 때는 이 선에서 자리를 직접 정해 둔 인형들도 줄로 돌려보내고,
   * 지금 보이는 왼쪽→오른쪽 순서 그대로 선 전체에 고르게 펼친다.
   */
  function toggleAutoGap() {
    const on = !line.autoGap;
    const lineId = line.id;
    const r = getRenderer();
    commit((c) => {
      const l = c.lines.find((x) => x.id === lineId);
      if (!l) return;
      l.autoGap = on;
      if (!on) return;
      const mine = c.dolls.filter((d) => d.line === lineId && d.visible && !d.manual.layout);
      const byX = [...mine].sort((a, b) => (r?.getPlaced(a.id)?.x ?? 0) - (r?.getPlaced(b.id)?.x ?? 0));
      const slots = mine.map((d) => d.order).sort((a, b) => a - b);
      byX.forEach((d, i) => {
        d.order = slots[i];
        d.at = null;
      });
    });
  }

  /** 간격을 손으로 바꾸면 자동은 꺼진다. */
  const setGap = (v: number) =>
    editLine((l) => {
      l.overlap = v;
      l.autoGap = false;
    });

  // ── 선 추가·삭제 ──
  /** 고른 선과 같은 모양의 새 선을 그 위(자리가 없으면 아래)에 만들고 고른다. */
  function addLine() {
    const base = line;
    const n = Math.max(0, ...cfg.lines.map((l) => +(l.id.match(/\d+$/)?.[0] ?? 0))) + 1;
    const id = `line${n}`;
    const ys = base.ridge.map((p) => p.y);
    const dy = Math.min(...ys) - 0.15 >= 0.2 ? -0.15 : Math.max(...ys) + 0.15 <= 1 ? 0.15 : -0.15;
    commit((c) =>
      c.lines.push({
        ...$state.snapshot(base),
        id,
        ridge: base.ridge.map((p) => ({ x: p.x, y: Math.min(1, Math.max(0, p.y + dy)) })) as Line["ridge"],
      }),
    );
    app.line = id;
  }

  /** 선을 지운다. 그 선에 있던 인형은 남은 첫 선으로 옮겨 간다(선 위 자리는 그대로). 마지막 선은 지울 수 없다. */
  function removeLine(id: string) {
    if (cfg.lines.length <= 1) return;
    commit((c) => {
      c.lines = c.lines.filter((l) => l.id !== id);
      for (const d of c.dolls) if (d.line === id) d.line = c.lines[0].id;
    });
    app.line = null;
  }

  // ── 타이밍 (모든 인형 공통): − / + 를 누르면 10ms씩, 누르고 있으면 계속 바뀐다. ──
  const clampLatency = (v: number) => Math.min(LATENCY_MAX_MS, Math.max(LATENCY_MIN_MS, Math.round(v)));
  const setLatency = (v: number) => {
    if (Number.isFinite(v)) motion((c) => (c.motion.latencyMs = clampLatency(v)));
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

  // ── 드래그로 순서 변경 (파일 드롭을 켜 두면 HTML5 드래그를 쓸 수 없어 포인터로 처리) ──
  let dragId = $state<string | null>(null);
  let moved = false;
  let startX = 0;

  function down(e: PointerEvent, id: string) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button.mini")) return;
    dragId = id;
    moved = false;
    startX = e.clientX;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!dragId) return;
    if (!moved && Math.abs(e.clientX - startX) < 6) return;
    moved = true;
    const over = document
      .elementsFromPoint(e.clientX, e.clientY)
      .map((el) => (el as HTMLElement).closest<HTMLElement>("[data-doll]"))
      .find((el) => el && el.dataset.doll !== dragId);
    const target = over?.dataset.doll;
    if (!target) return;
    const ids = dolls.map((d) => d.id);
    const from = ids.indexOf(dragId);
    const to = ids.indexOf(target);
    if (from < 0 || to < 0) return;
    ids.splice(to, 0, ...ids.splice(from, 1));
    commit((c) => c.dolls.forEach((d) => (d.order = ids.indexOf(d.id))));
  }
  function up(id: string) {
    if (dragId && !moved) app.selected = app.selected === id ? null : id;
    dragId = null;
  }
</script>

  <div class="row">
    <div class="seg" role="group" aria-label="배정 방식">
      <button class:on={cfg.motion.assign === "spread"} title="따로" aria-label="따로" onclick={() => commit((c) => (c.motion.assign = "spread"))}>
        <span class="dots">{#each ["kick", "mid", "bass", "high"] as b}<i style="background:{BAND_COLORS[b as BandId]}"></i>{/each}</span>
      </button>
      <button class:on={cfg.motion.assign === "together"} title="함께" aria-label="함께" onclick={() => commit((c) => (c.motion.assign = "together"))}>
        <span class="dots">{#each [0, 1, 2, 3] as _}<i style="background:{BAND_COLORS.full}"></i>{/each}</span>
      </button>
    </div>
    <div class="seg" role="group" aria-label="트리거 방식">
      <button class:on={cfg.motion.sync === "onset"} title="소리마다" aria-label="소리마다" onclick={() => commit((c) => (c.motion.sync = "onset"))}>
        <Icon name="onset" />
      </button>
      <button class:on={cfg.motion.sync === "tempo"} title="박자 고정" aria-label="박자 고정" onclick={() => commit((c) => (c.motion.sync = "tempo"))}>
        <Icon name="tempo" />
        {#if live.bpm}<span class="bpm" style="opacity:{0.6 + 0.4 * live.beat}">{live.bpm}</span>{/if}
      </button>
    </div>
    <!-- 타이밍: 모든 인형에 공통. 인형이 소리보다 늦어 보이면 −, 빨라 보이면 +. 숫자를 두 번 누르면 0. -->
    <div class="timing" class:set={latency !== 0} role="group" aria-label="타이밍" title="타이밍">
      <Icon name="clock" />
      <button
        class="round"
        aria-label="일찍"
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
          aria-label="타이밍 ms"
          value={latency}
          onchange={(e) => setLatency(e.currentTarget.valueAsNumber)}
          ondblclick={() => setLatency(0)}
        /><small>ms</small>
      </label>
      <button
        class="round"
        aria-label="늦게"
        disabled={latency >= LATENCY_MAX_MS}
        onpointerdown={() => holdStart(10)}
        onpointerup={stopHold}
        onpointerleave={stopHold}
        onclick={() => nudge(10)}><Icon name="plus" /></button
      >
    </div>
  </div>

  <!-- 타이밍 줄 아래는 모두 한 덩어리로 스크롤된다 (인형 목록은 따로 스크롤하지 않고 줄을 바꿔 늘어선다). -->
  <div class="scroll">
  <div class="sliders">
    <!-- 끄는 동안 미리보기 인형이 계속 꿍실거려서 결과가 바로 보인다. -->
    <label class="field">깊이<Slider lo="살짝" hi="푹" min={0} max={0.4} step={0.005} value={cfg.motion.amount} oninput={(v) => motion((c) => (c.motion.amount = v))} /></label>
  </div>

  <!-- 선: 고르기·추가·삭제. 아래 속성은 고른 선의 것이다(선마다 따로 저장된다). -->
  <fieldset class="line">
    <legend>선</legend>
    <div class="lines" role="group" aria-label="선 고르기">
      {#each cfg.lines as l, i (l.id)}
        <button class="lchip" class:on={l.id === line.id} onclick={() => (app.line = l.id)}>
          <i class="stroke"></i>{i + 1}
        </button>
      {/each}
      <button class="icon" aria-label="선 추가" title="선 추가" onclick={addLine}><Icon name="plus" /></button>
      <button
        class="icon del"
        aria-label="이 선 삭제"
        title="이 선 삭제"
        disabled={cfg.lines.length <= 1}
        onclick={() => removeLine(line.id)}><Icon name="trash" /></button
      >
    </div>
    <div class="seg" role="group" aria-label="가까운 쪽">
      <button class:on={line.near === "left"} title="왼쪽이 가깝게" aria-label="왼쪽이 가깝게" onclick={() => editLine((l) => (l.near = "left"))}>
        <span class="near"><i class="big"></i><i></i></span>
      </button>
      <button class:on={line.near === "right"} title="오른쪽이 가깝게" aria-label="오른쪽이 가깝게" onclick={() => editLine((l) => (l.near = "right"))}>
        <span class="near"><i></i><i class="big"></i></span>
      </button>
    </div>
    <label class="field">원근<Slider lo="모두 같게" hi="멀수록 작게" min={0} max={0.9} step={0.01} value={1 - line.farScale} oninput={(v) => editLine((l) => (l.farScale = 1 - v))} /></label>
    <!-- 간격: 자동이면 선 전체에 고르게 펼친다. 손으로 끌면 자동이 꺼진다. -->
    <div class="field gap">
      간격
      <div class="gaprow">
        <button class="icon" class:on={!!line.autoGap} aria-pressed={!!line.autoGap} aria-label="간격 자동" title="간격 자동" onclick={toggleAutoGap}
          ><Icon name="spread" /></button
        >
        <span class="gapslider" class:muted={!!line.autoGap}>
          <Slider lo="나란히" hi="겹치게" min={0} max={0.6} step={0.01} value={line.overlap} oninput={setGap} />
        </span>
      </div>
    </div>
  </fieldset>

  <div class="strip" class:hot={app.dropping}>
    {#each dolls as d (d.id)}
      <div
        class="thumb"
        class:sel={d.id === app.selected}
        class:off={!d.visible}
        class:drag={d.id === dragId}
        class:dashed={d.react === "every"}
        data-doll={d.id}
        role="button"
        tabindex="0"
        aria-label="인형"
        style="--band:{DOLL_BAND_COLORS[d.band]};--flash:{flashOf(d)}"
        onpointerdown={(e) => down(e, d.id)}
        onpointermove={move}
        onpointerup={() => up(d.id)}
        onpointercancel={() => (dragId = null)}
        onkeydown={(e) => e.key === "Enter" && (app.selected = app.selected === d.id ? null : d.id)}
      >
        <img src={imageUrl(app.imagesDir, d.image)} alt="" draggable="false" style={d.flip ? "transform:scaleX(-1)" : ""} />
        <button class="mini eye" aria-label={d.visible ? "숨기기" : "보이기"} onclick={() => edit(d.id, (x) => (x.visible = !x.visible))}>
          <Icon name={d.visible ? "eye" : "eyeOff"} />
        </button>
        <button class="mini del" aria-label="삭제" onclick={() => remove(d.id)}><Icon name="x" /></button>
      </div>
    {/each}
    {#if dolls.length === 0}
      <button class="drop" class:hot={app.dropping} onclick={pick} aria-label="이미지 추가">
        <Icon name="image" />
        <span>이미지 놓기</span>
      </button>
    {:else}
      <button class="thumb add" aria-label="이미지 추가" onclick={pick}><Icon name="plus" /></button>
    {/if}
  </div>

  {#if selected}
    {@const s = selected}
    <section class="adv-box" aria-label="이 인형 설정">
      <div class="adv">
        <!-- 이 인형의 두 가지 설정: 어떤 소리(대역)에 / 어떻게(박자·모든 소리) 반응하는가 -->
        <div class="row">
          {#each DOLL_BANDS as b}
            <button
              class="chip"
              class:beat={b === "beat"}
              class:on={s.band === b}
              style="--band:{DOLL_BAND_COLORS[b]}"
              title={bandTitle(b)}
              onclick={() => edit(s.id, (d) => ((d.band = b), (d.manual.band = true), b === "beat" && (d.react = "beat")))}>
              {#if b === "beat"}<Icon name="tempo" />{:else}<i></i>{/if}{DOLL_BAND_LABELS[b]}
            </button>
          {/each}
          <!-- 박 인형은 늘 박자로 움직인다 (모든 소리를 고를 수 없다). -->
          <div class="seg react" role="group" aria-label="반응 방식">
            <button class:on={s.react !== "every" || s.band === "beat"} onclick={() => edit(s.id, (d) => (d.react = "beat"))}><Icon name="tempo" />박자</button>
            <button
              class="every"
              class:on={s.react === "every" && s.band !== "beat"}
              disabled={s.band === "beat"}
              onclick={() => edit(s.id, (d) => (d.react = "every"))}><Icon name="onset" />모든 소리</button
            >
          </div>
          <span class="tools">
          <button class:on={s.flip} aria-label="좌우 반전" title="좌우 반전" onclick={() => edit(s.id, (d) => (d.flip = !d.flip))}><Icon name="flip" /></button>
          <button
            aria-label="자동으로 되돌리기"
            title="자동으로 되돌리기"
            disabled={!s.manual.layout && !s.manual.band && s.size === 1 && s.at == null}
            onclick={() => edit(s.id, (d) => ((d.manual = { layout: false, band: false }), (d.size = 1), (d.at = null)))}
          >
            <Icon name="auto" />
          </button>
          </span>
        </div>
      </div>
    </section>
  {/if}
  </div>

<style>
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
    /* 고른 인형의 테두리(outline)가 잘리지 않게 */
    padding: 4px 4px 4px 2px;
  }
  .drop {
    width: 100%;
    height: 100%;
    min-height: 160px;
    flex-direction: column;
    justify-content: center;
    border: 2px dashed var(--line);
    background: none;
    color: var(--dim);
    font-size: 15px;
    border-radius: 14px;
  }
  .drop :global(svg.icon) {
    width: 40px;
    height: 40px;
  }
  .drop.hot,
  .strip.hot {
    border-color: var(--accent);
    color: var(--accent);
  }
  .sliders {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px 18px;
    align-items: start;
  }
  .sliders :global(label.field) {
    align-items: start;
  }
  .line {
    display: grid;
    grid-template-columns: auto 1fr 1fr;
    gap: 8px 18px;
    align-items: start;
    margin: 0;
    padding: 8px 12px 10px;
    border: 1px solid var(--line);
    border-radius: 12px;
  }
  .lines {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .lchip {
    padding: 4px 10px;
    gap: 6px;
    font-variant-numeric: tabular-nums;
  }
  .lchip .stroke {
    width: 14px;
    height: 3px;
    border-radius: 2px;
    background: currentColor;
    opacity: 0.8;
  }
  .lines .icon {
    padding: 4px;
  }
  .lines .icon :global(svg.icon) {
    width: 16px;
    height: 16px;
  }
  .lines .del:not(:disabled):hover {
    color: var(--danger);
  }
  .strip .drop {
    flex: 1;
    height: 72px;
    min-height: 0;
    flex-direction: row;
    font-size: 14px;
    border-radius: 12px;
  }
  .strip .drop :global(svg.icon) {
    width: 24px;
    height: 24px;
  }
  .line legend {
    padding: 0 6px;
    color: var(--dim);
  }
  .line :global(label.field) {
    align-items: start;
  }
  .dots,
  .near {
    display: inline-flex;
    gap: 3px;
    align-items: flex-end;
    height: 18px;
  }
  .dots i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    align-self: center;
  }
  .near i {
    width: 8px;
    height: 8px;
    border-radius: 50% 50% 3px 3px;
    background: currentColor;
  }
  .near i.big {
    width: 15px;
    height: 16px;
  }
  .bpm {
    font-variant-numeric: tabular-nums;
    font-size: 12px;
  }
  .strip {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    padding: 8px;
    border: 2px dashed transparent;
    border-radius: 12px;
    background: var(--panel);
  }
  .thumb {
    position: relative;
    flex: none;
    width: 72px;
    height: 72px;
    padding: 6px;
    border-radius: 12px;
    background: var(--raised);
    border: 3px solid var(--band, var(--line));
    box-shadow: 0 0 calc(var(--flash, 0) * 14px) var(--band, transparent);
    filter: brightness(calc(1 + var(--flash, 0) * 0.35));
    cursor: grab;
    touch-action: none;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
  }
  .thumb.sel {
    outline: 2px solid #fff;
    outline-offset: 2px;
  }
  .thumb.off img {
    opacity: 0.3;
  }
  .thumb.drag {
    opacity: 0.6;
    cursor: grabbing;
  }
  .thumb.add {
    border: 2px dashed var(--line);
    justify-content: center;
    color: var(--dim);
    cursor: pointer;
    filter: none;
    box-shadow: none;
  }
  .mini {
    position: absolute;
    padding: 2px;
    border-radius: 6px;
    background: rgba(20, 22, 29, 0.85);
    border: 0;
    opacity: 0;
  }
  .mini :global(svg.icon) {
    width: 14px;
    height: 14px;
  }
  .thumb:hover .mini,
  .mini:focus-visible,
  .thumb.off .eye {
    opacity: 1;
  }
  .eye {
    left: 2px;
    bottom: 2px;
  }
  .del {
    right: 2px;
    top: 2px;
  }
  .del:hover {
    color: var(--danger);
  }
  .adv-box {
    border-top: 1px solid var(--line);
    padding-top: 10px;
  }
  .adv {
    display: grid;
    gap: 12px;
  }
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
  .timing .round {
    width: 26px;
    height: 26px;
    padding: 0;
    justify-content: center;
    border-radius: 50%;
    border: 0;
    background: var(--raised);
    color: var(--text);
    transition: background 0.12s, transform 0.08s;
  }
  .timing .round :global(svg.icon) {
    width: 14px;
    height: 14px;
    stroke-width: 2.4;
  }
  .timing .round:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 30%, var(--raised));
  }
  .timing .round:active:not(:disabled) {
    transform: scale(0.9);
  }
  .timing .val {
    display: inline-flex;
    align-items: baseline;
    gap: 1px;
    min-width: 56px;
    justify-content: center;
    font-variant-numeric: tabular-nums;
    cursor: text;
  }
  .timing .val input {
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
  .timing .val input::-webkit-inner-spin-button {
    -webkit-appearance: none;
  }
  .timing .val input:focus-visible {
    outline: none;
  }
  .timing:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .timing .val small {
    font-size: 10px;
    color: var(--dim);
  }
  .gap {
    display: grid;
    grid-template-columns: 48px 1fr;
    gap: 8px;
    align-items: start;
    color: var(--dim);
  }
  .gaprow {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    min-width: 0;
  }
  .gaprow .icon {
    padding: 4px;
    flex: none;
  }
  .gapslider {
    flex: 1;
    min-width: 0;
    transition: opacity 0.15s;
  }
  .gapslider.muted {
    opacity: 0.4;
  }
  .chip.beat :global(svg.icon) {
    width: 14px;
    height: 14px;
    color: var(--band);
  }
  .thumb {
    transition:
      transform 0.14s ease-out,
      border-color 0.15s,
      opacity 0.15s;
  }
  .thumb:hover:not(.drag) {
    transform: translateY(-2px);
  }
  .thumb.dashed {
    border-style: dashed;
  }
  .tools {
    display: inline-flex;
    gap: 6px;
    margin-left: auto;
  }
  .seg button:disabled {
    opacity: 0.35;
  }
  .chip i {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--band);
  }
  .chip.on {
    border-color: var(--band);
    color: var(--text);
    background: color-mix(in srgb, var(--band) 22%, var(--raised));
  }
</style>
