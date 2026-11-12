import { describe, expect, it } from "vitest";
import { HIT_PHASE, Pulse, shape, Spring, SQUASH_PART } from "./spring";

const DT = 1 / 120;
const BEAT = 60; // 0.5초 = 60스텝 (120BPM)

const beats = (from: number, to: number, every = BEAT) => {
  const v: number[] = [];
  for (let i = from; i < to; i += every) v.push(i);
  return v;
};

/**
 * kicks(스텝 번호)에 소리를 넣고 돌린다.
 * 반환: x 궤적과, 각 소리 시점에 인형이 "실제 박"에서 몇 ms 어긋나 있었는지.
 */
function run(kicks: number[], steps: number, periodS: number | null = 0.5) {
  const s = new Spring();
  const set = new Set(kicks);
  const xs: number[] = [];
  const errMs: number[] = [];
  for (let i = 0; i < steps; i++) {
    if (set.has(i)) {
      // 소리를 들은 순간 인형은 가장 쭈그러진 자리(HIT_PHASE)에 있어야 한다.
      const p = s.beatPosition - HIT_PHASE;
      errMs.push((p - Math.round(p)) * s_period(s) * 1000);
      s.kick(1, periodS);
    }
    s.step(DT);
    xs.push(s.x);
  }
  return { xs, errMs, spring: s };
}
const s_period = (s: Spring) => (s as unknown as { period: number }).period;
const mean = (v: number[]) => v.reduce((a, b) => a + b, 0) / v.length;
const absMax = (v: number[]) => Math.max(...v.map(Math.abs));

describe("shape", () => {
  it("is stretched on the beat, squashed at SQUASH_PART, and periodic", () => {
    expect(shape(0)).toBeCloseTo(0.7, 10);
    expect(shape(SQUASH_PART)).toBeCloseTo(-1, 10);
    expect(shape(1 - 1e-9)).toBeCloseTo(0.7, 5);
    expect(shape(3.25)).toBeCloseTo(shape(0.25), 10);
  });

  it("eases out on both the squash and the stretch", () => {
    const h = 0.005;
    const squashStart = Math.abs(shape(h) - shape(0));
    const squashEnd = Math.abs(shape(SQUASH_PART) - shape(SQUASH_PART - h));
    expect(squashStart).toBeGreaterThan(squashEnd * 5);
    const stretchStart = Math.abs(shape(SQUASH_PART + h) - shape(SQUASH_PART));
    const stretchEnd = Math.abs(shape(1 - 1e-9) - shape(1 - h));
    expect(stretchStart).toBeGreaterThan(stretchEnd * 5);
  });
});

describe("Spring (beat-locked oscillator with inertia)", () => {
  const steady = run(beats(0, 1440), 1440);

  it("is fully squashed right when the hit is heard", () => {
    // 소리를 들은 순간이 가장 쭈그러진 순간이다 (ease-out 이전과 같은 타이밍).
    expect(absMax(steady.errMs.slice(12))).toBeLessThan(4);
  });

  it("hits bottom on the hit frame and starts squashing before it", () => {
    const xs = steady.xs;
    for (const k of beats(720, 1380)) {
      // 소리 프레임 앞뒤 3프레임(±25ms) 안에 가장 쭈그러진다.
      const win = xs.slice(k - 6, k + 7);
      const lowest = k - 6 + win.indexOf(Math.min(...win));
      expect(Math.abs(lowest - k)).toBeLessThanOrEqual(3);
      expect(xs[lowest]).toBeLessThan(-0.8);
      // 소리 0.15박 전에는 이미 내려가는 중이다.
      expect(xs[k - 9]).toBeGreaterThan(xs[k - 3]);
    }
  });

  it("repeats the same squash-and-stretch every beat", () => {
    const xs = steady.xs;
    for (let i = 720; i + BEAT < 1440; i++) expect(Math.abs(xs[i] - xs[i + BEAT])).toBeLessThan(0.02);
    expect(Math.min(...xs.slice(600))).toBeLessThan(-0.8);
    expect(Math.max(...xs.slice(600))).toBeGreaterThan(0.6);
  });

  it("locks to the actual hits even when the tempo estimate is a little off", () => {
    // 추정은 115BPM, 실제 소리는 120BPM.
    const { errMs } = run(beats(0, 2400), 2400, 60 / 115);
    expect(absMax(errMs.slice(25))).toBeLessThan(6);
  });

  it("ignores a tempo estimate that is badly wrong", () => {
    // 추정이 76BPM(배수로 잘못 잡힘)이어도 실제 소리(120BPM)를 따른다.
    const { errMs } = run(beats(0, 2400), 2400, 60 / 76);
    expect(absMax(errMs.slice(25))).toBeLessThan(6);
  });

  // 아래 테스트들은 인형이 박을 다 잡은 뒤(1500스텝 = 12.5초)에 일을 일으킨다.
  const LONG = 2400;
  const settled = run(beats(0, LONG), LONG);

  it("barely shifts when one hit comes early, and drifts back", () => {
    const kicks = beats(0, LONG).map((k) => (k === 1500 ? 1492 : k)); // 67ms 이르게
    const { errMs } = run(kicks, LONG);
    const after = errMs.slice(26); // 이른 소리 다음 박부터
    expect(absMax(after)).toBeLessThan(30);
    expect(absMax(after.slice(6))).toBeLessThan(8);
  });

  it("ignores a stray hit between beats", () => {
    const { xs } = run([...beats(0, LONG), 1530], LONG);
    for (let i = 0; i < LONG; i++) expect(Math.abs(xs[i] - settled.xs[i])).toBeLessThan(0.02);
  });

  it("keeps its groove through jittery hits", () => {
    let seed = 1;
    const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647) * 2 - 1;
    const jitter = beats(0, LONG).map((k) => (k < 1200 ? 0 : Math.round(rnd() * 5))); // 1200부터 ±42ms
    const kicks = beats(0, LONG).map((k, i) => k + jitter[i]);
    // 소리 대신 "박 시점의 인형 위상"을 잰다.
    const s = new Spring();
    const set = new Set(kicks);
    const out: number[] = [];
    for (let i = 0; i < LONG; i++) {
      if (i >= 1200 && i % BEAT === 0) {
        const p = s.beatPosition - HIT_PHASE;
        out.push(Math.abs(p - Math.round(p)) * 500);
      }
      if (set.has(i)) s.kick(1, 0.5);
      s.step(DT);
    }
    const inputErr = jitter.slice(20).map((j) => Math.abs(j) * DT * 1000);
    expect(Math.max(...out)).toBeLessThan(30);
    expect(mean(out)).toBeLessThan(mean(inputErr) * 0.6);
  });

  it("keeps going unchanged through a missing beat", () => {
    const kicks = beats(0, LONG).filter((k) => k !== 1500);
    const { xs } = run(kicks, LONG);
    for (let i = 0; i < LONG; i++) expect(Math.abs(xs[i] - settled.xs[i])).toBeLessThan(0.02);
  });

  it("only dips a little through two missing beats", () => {
    const kicks = beats(0, LONG).filter((k) => k !== 1500 && k !== 1560);
    const { xs } = run(kicks, LONG);
    const depth = (v: number[], from: number, to: number) => -Math.min(...v.slice(from, to));
    // 빠진 두 박 동안에도 쭈그러지는 깊이가 60% 넘게 남고, 돌아온 뒤에는 원래대로.
    expect(depth(xs, 1490, 1680) / depth(settled.xs, 1490, 1680)).toBeGreaterThan(0.6);
    expect(depth(xs, 1900, 2000) / depth(settled.xs, 1900, 2000)).toBeGreaterThan(0.97);
  });

  it("settles quickly when the sound is cut", () => {
    // 끊긴 뒤 1초(120스텝) 안에 거의 멈춘다.
    let still = 1200;
    const s2 = new Spring();
    for (let i = 0; i < 1200; i++) {
      if (i < 960 && i % BEAT === 0) s2.kick(1, 0.5);
      if (i >= 960) s2.hush();
      s2.step(DT);
      if (i > 960 && Math.abs(s2.x) < 0.05 && still === 1200) still = i;
    }
    expect(still - 960).toBeLessThan(120);
    expect(Math.abs(s2.x)).toBeLessThan(0.02);
  });

  it("shifts the motion by the latency setting without changing its shape", () => {
    const lagged = (lag: number) => {
      const s = new Spring();
      s.lag = lag;
      const xs: number[] = [];
      for (let i = 0; i < 1440; i++) {
        if (i % BEAT === 0) s.kick(1, 0.5);
        s.step(DT);
        xs.push(s.x);
      }
      return xs;
    };
    const base = lagged(0);
    const late = lagged(0.1); // 100ms = 12스텝 늦게
    const early = lagged(-0.05); // 50ms = 6스텝 일찍
    for (let i = 960; i < 1400; i++) {
      expect(Math.abs(late[i] - base[i - 12])).toBeLessThan(0.03);
      expect(Math.abs(early[i] - base[i + 6])).toBeLessThan(0.03);
    }
  });

  it("follows the hit interval when the tempo is unknown", () => {
    const { errMs } = run(beats(0, 2400, 72), 2400, null); // 0.6초 간격
    expect(absMax(errMs.slice(15))).toBeLessThan(8);
  });

  it("starts from rest without a big jump and settles after the music stops", () => {
    const { xs } = run(beats(0, 480), 1440);
    const ss = steady.xs;
    const steadyStep = Math.max(...ss.slice(601).map((x, i) => Math.abs(x - ss[600 + i])));
    // 처음 한 프레임은 시작 자세에서 메우는 몫이 더해지므로 약간 여유를 둔다.
    expect(Math.abs(xs[0])).toBeLessThanOrEqual(steadyStep * 1.2);
    for (let i = 1; i < xs.length; i++) expect(Math.abs(xs[i] - xs[i - 1])).toBeLessThanOrEqual(steadyStep * 1.2);
    expect(Math.abs(xs[1439])).toBeLessThan(0.02);
  });
});

describe("Spring alignment shift", () => {
  it("a hit heard 30ms late but shifted back lands on the same phase as an on-time hit", () => {
    const onTime = new Spring();
    const late = new Spring();
    for (let i = 0; i < 1440; i++) {
      if (i % BEAT === 0) onTime.kick(1, 0.5);
      // 4스텝(33ms) 늦게 들었지만, 다른 대역과 같은 타격이므로 33ms 앞선 것으로 친다.
      if (i % BEAT === 4) late.kick(1, 0.5, 4 * DT);
      onTime.step(DT);
      late.step(DT);
    }
    const d = late.beatPosition - onTime.beatPosition;
    expect(Math.abs(d - Math.round(d)) * 500).toBeLessThan(3);
  });
});

describe("Spring grid alignment", () => {
  it("a hit heard 25ms early but snapped to the grid lands on the same phase as an on-time hit", () => {
    const onTime = new Spring();
    const early = new Spring();
    for (let i = 0; i < 1440; i++) {
      if (i % BEAT === 0) onTime.kick(1, 0.5);
      if (i % BEAT === BEAT - 3) early.kick(1, 0.5, -3 * DT);
      onTime.step(DT);
      early.step(DT);
    }
    const d = early.beatPosition - onTime.beatPosition;
    expect(Math.abs(d - Math.round(d)) * 500).toBeLessThan(3);
  });
});

describe("Pulse (react to every sound)", () => {
  it("squashes within 50ms of each hit and recovers fully afterwards", () => {
    const p = new Pulse();
    const xs: number[] = [];
    for (let i = 0; i < 240; i++) {
      if (i === 12 || i === 24 || i === 36) p.hit(1); // 100ms 간격
      p.step(DT);
      xs.push(p.x);
    }
    for (const k of [12, 24, 36]) expect(Math.min(...xs.slice(k, k + 7))).toBeLessThan(-0.85);
    expect(Math.abs(xs[239])).toBeLessThan(1e-6);
    expect(p.resting).toBe(true);
    for (let i = 1; i < xs.length; i++) expect(Math.abs(xs[i] - xs[i - 1])).toBeLessThan(0.5);
  });

  it("delays the reaction by the timing setting", () => {
    const p = new Pulse();
    const xs: number[] = [];
    for (let i = 0; i < 60; i++) {
      if (i === 0) p.hit(1, 0.1);
      p.step(DT);
      xs.push(p.x);
    }
    expect(Math.abs(xs[10])).toBeLessThan(1e-6);
    expect(Math.min(...xs.slice(12, 22))).toBeLessThan(-0.85);
  });
});
