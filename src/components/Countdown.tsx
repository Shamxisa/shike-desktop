import { useEffect, useState } from "react";
import { call, errorText, importMedia, saveSettings } from "../api";
import type { Settings, Timer } from "../types";

export default function Countdown({
  timer,
  settings,
  notify,
}: {
  timer: Timer;
  settings: Settings;
  notify: (message: string) => void;
}) {
  const [duration, setDuration] = useState([0, 25, 0]);
  const [busy, setBusy] = useState(false);
  const locked = timer.status === "running" || timer.status === "paused";
  useEffect(() => {
    const seconds = Math.floor(timer.durationMs / 1000);
    setDuration([
      Math.floor(seconds / 3600),
      Math.floor((seconds % 3600) / 60),
      seconds % 60,
    ]);
  }, [timer.durationMs]);
  async function apply(seconds: number) {
    if (!Number.isInteger(seconds) || seconds < 1) {
      notify("请设置大于零的时长");
      return;
    }
    setBusy(true);
    try {
      await call("timer_action", { action: "set", durationMs: seconds * 1000 });
    } catch (error) {
      notify(errorText(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <div className="section-heading">
        <h2>设置时长</h2>
      </div>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void apply(duration[0] * 3600 + duration[1] * 60 + duration[2]);
        }}
      >
        <div className="timer-fields">
          {["小时", "分钟", "秒"].map((label, index) => (
            <label key={label}>
              {label}
              <input
                type="number"
                min="0"
                max={index === 0 ? 99 : 59}
                step="1"
                required
                disabled={locked || busy}
                value={duration[index]}
                onChange={(event) =>
                  setDuration(
                    duration.map((value, position) =>
                      position === index ? Number(event.target.value) : value,
                    ),
                  )
                }
              />
            </label>
          ))}
        </div>
        <div className="preset-row">
          {[5, 10, 25].map((minutes) => (
            <button
              key={minutes}
              type="button"
              disabled={locked || busy}
              onClick={() => void apply(minutes * 60)}
            >
              {minutes} 分钟
            </button>
          ))}
          <button className="primary" disabled={locked || busy}>
            应用时长
          </button>
        </div>
      </form>
      <div className="setting-row">
        <div>
          <h3>结束铃声</h3>
          <p>{settings.timerSound ? "自选音频" : "轻柔钟声"}</p>
        </div>
        <div className="inline-actions">
          <button
            onClick={() =>
              void importMedia("audio")
                .then(async (sound) => {
                  if (sound)
                    await saveSettings({ ...settings, timerSound: sound });
                })
                .catch((error) => notify(errorText(error)))
            }
          >
            更换
          </button>
          {settings.timerSound && (
            <button
              onClick={() =>
                void saveSettings({ ...settings, timerSound: null }).catch(
                  (error) => notify(errorText(error)),
                )
              }
            >
              内置铃声
            </button>
          )}
          <button
            onClick={() =>
              void call("preview_sound", {
                name: settings.timerSound,
                volume: settings.timerVolume,
              }).catch((error) => notify(errorText(error)))
            }
          >
            试听
          </button>
          <button
            onClick={() =>
              void call("stop_preview").catch((error) =>
                notify(errorText(error)),
              )
            }
          >
            停止
          </button>
        </div>
      </div>
      <label className="range-label">
        铃声音量 <output>{Math.round(settings.timerVolume * 100)}%</output>
        <input
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={settings.timerVolume}
          onChange={(event) =>
            void saveSettings({
              ...settings,
              timerVolume: Number(event.target.value),
            }).catch((error) => notify(errorText(error)))
          }
        />
      </label>
    </>
  );
}
