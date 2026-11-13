import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";

const source = readFileSync(
  new URL("../src/countdown-clock.ts", import.meta.url),
  "utf8",
);
const { outputText } = ts.transpileModule(source, {
  compilerOptions: {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ES2022,
  },
});
const { receiveTimer, remainingAt } = await import(
  `data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`
);
const timer = (patch = {}) => ({
  durationMs: 10000,
  remainingMs: 10000,
  status: "running",
  endAt: 10000,
  revision: 1,
  sampledAt: 100000,
  ...patch,
});

test("600 ms backend notifications still produce one second display steps", () => {
  let sample = receiveTimer(null, timer(), 0);
  for (let elapsed = 100; elapsed <= 10000; elapsed += 100) {
    if (elapsed % 600 === 0) {
      sample = receiveTimer(
        sample,
        timer({ remainingMs: 10000 - elapsed, sampledAt: 100000 + elapsed }),
        elapsed + 20,
      );
    }
    assert.equal(
      Math.ceil(remainingAt(sample, elapsed) / 1000),
      Math.ceil((10000 - elapsed) / 1000),
    );
  }
});

test("late packets and old snapshots cannot rewind a running countdown", () => {
  let sample = receiveTimer(null, timer(), 0);
  sample = receiveTimer(
    sample,
    timer({ remainingMs: 8000, sampledAt: 102000 }),
    2000,
  );
  const accepted = sample;
  sample = receiveTimer(
    sample,
    timer({ remainingMs: 9500, sampledAt: 100500 }),
    2100,
  );
  assert.equal(sample, accepted);
  sample = receiveTimer(
    sample,
    timer({ remainingMs: 7500, sampledAt: 102500 }),
    3500,
  );
  assert.equal(remainingAt(sample, 3500), 6500);
});

test("pause freezes the display and resume uses the preserved remainder", () => {
  let sample = receiveTimer(null, timer(), 0);
  sample = receiveTimer(
    sample,
    timer({
      status: "paused",
      remainingMs: 6750,
      revision: 2,
      sampledAt: 103250,
    }),
    3250,
  );
  assert.equal(remainingAt(sample, 20000), 6750);
  sample = receiveTimer(
    sample,
    timer({ remainingMs: 6750, revision: 3, sampledAt: 120000 }),
    20000,
  );
  assert.equal(remainingAt(sample, 21750), 5000);
});

test("reset and finish reject delayed data from a previous run", () => {
  let sample = receiveTimer(
    null,
    timer({ status: "ready", revision: 4 }),
    5000,
  );
  const reset = sample;
  sample = receiveTimer(
    sample,
    timer({ revision: 3, sampledAt: 999999 }),
    5100,
  );
  assert.equal(sample, reset);
  sample = receiveTimer(
    sample,
    timer({ status: "finished", remainingMs: 0, revision: 5 }),
    6000,
  );
  sample = receiveTimer(sample, timer({ remainingMs: 100, revision: 4 }), 6100);
  assert.equal(remainingAt(sample, 6200), 0);
});

test("long delivery gaps catch up to elapsed time and stop at zero", () => {
  const sample = receiveTimer(null, timer(), 1000);
  assert.equal(remainingAt(sample, 8500), 2500);
  assert.equal(remainingAt(sample, 12000), 0);
});
