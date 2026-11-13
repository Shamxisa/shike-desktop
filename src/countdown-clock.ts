import type { Timer } from "./types";

export interface CountdownSample {
  timer: Timer;
  deadline: number | null;
}

export function receiveTimer(
  current: CountdownSample | null,
  timer: Timer,
  receivedAt: number,
): CountdownSample {
  if (current) {
    const revision = timer.revision ?? 0;
    const previousRevision = current.timer.revision ?? 0;
    if (revision < previousRevision) return current;
    if (
      revision === previousRevision &&
      (timer.sampledAt ?? 0) < (current.timer.sampledAt ?? 0)
    )
      return current;
  }
  if (timer.status !== "running") return { timer, deadline: null };

  const deadline = receivedAt + timer.remainingMs;
  const sameRun =
    current?.timer.status === "running" &&
    current.timer.revision === timer.revision;
  return {
    timer,
    // Delayed notifications must not move the visible countdown backwards.
    deadline:
      sameRun && current.deadline !== null
        ? Math.min(current.deadline, deadline)
        : deadline,
  };
}

export function remainingAt(sample: CountdownSample, now: number): number {
  if (sample.deadline === null) return sample.timer.remainingMs;
  return Math.max(0, sample.deadline - now);
}
