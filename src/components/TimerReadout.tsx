import { useEffect, useReducer } from "react";
import { formatDuration } from "../api";
import { remainingAt, type CountdownSample } from "../countdown-clock";

export default function TimerReadout({ sample }: { sample: CountdownSample }) {
  const [, redraw] = useReducer((value: number) => value + 1, 0);
  useEffect(() => {
    let timeout: ReturnType<typeof setTimeout> | undefined;
    function update() {
      if (timeout) clearTimeout(timeout);
      redraw();
      const remaining = remainingAt(sample, performance.now());
      if (sample.timer.status === "running" && remaining > 0) {
        const delay = Math.ceil(remaining % 1000) || 1000;
        timeout = setTimeout(update, Math.max(1, delay));
      }
    }
    update();
    document.addEventListener("visibilitychange", update);
    window.addEventListener("pageshow", update);
    return () => {
      if (timeout) clearTimeout(timeout);
      document.removeEventListener("visibilitychange", update);
      window.removeEventListener("pageshow", update);
    };
  }, [sample]);
  return <>{formatDuration(remainingAt(sample, performance.now()))}</>;
}
