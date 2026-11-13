import { useEffect, useRef, useState } from "react";
import { ImagePlus } from "lucide-react";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { errorText, importMedia, saveSettings } from "../api";
import type { Settings } from "../types";

export default function Appearance({
  settings,
  onPreview,
  notify,
}: {
  settings: Settings;
  onPreview: (settings: Settings) => void;
  notify: (message: string) => void;
}) {
  const [draft, setDraft] = useState(settings);
  const draftRef = useRef(settings);
  const [importing, setImporting] = useState(false);
  const [autostart, setAutostart] = useState(false);
  useEffect(() => {
    setDraft(settings);
    draftRef.current = settings;
  }, [settings]);
  useEffect(() => {
    void isEnabled()
      .then(setAutostart)
      .catch((error) => notify(errorText(error)));
  }, [notify]);
  function update(patch: Partial<Settings>, persist = true) {
    const next = { ...draftRef.current, ...patch };
    draftRef.current = next;
    setDraft(next);
    onPreview(next);
    if (persist)
      void saveSettings(next).catch((error) => notify(errorText(error)));
  }
  async function upload() {
    setImporting(true);
    try {
      const name = await importMedia("image");
      if (name) update({ background: "custom", backgroundImage: name });
    } catch (error) {
      notify(errorText(error));
    } finally {
      setImporting(false);
    }
  }
  return (
    <div className="form-stack">
      <div className="section-heading">
        <div>
          <h2>背景与玻璃质感</h2>
        </div>
        <button disabled={importing} onClick={() => void upload()}>
          <ImagePlus size={15} />
          {importing ? "正在导入…" : "本地图片"}
        </button>
      </div>
      <div className="background-options">
        {(
          [
            ["mist", "晨雾"],
            ["forest", "青林"],
            ["night", "夜色"],
          ] as const
        ).map(([value, label]) => (
          <button
            key={value}
            aria-pressed={draft.background === value}
            onClick={() => update({ background: value })}
          >
            <span className={`swatch ${value}`} />
            {label}
          </button>
        ))}
      </div>
      <p className="muted">背景图片支持 PNG、JPG 和 WebP，最大 10 MB。</p>
      {(
        [
          {
            field: "opacity",
            label: "面板不透明度",
            min: 25,
            max: 85,
            unit: "%",
          },
          {
            field: "blur",
            label: "背景模糊",
            min: 0,
            max: 120,
            step: 1,
            unit: " px",
          },
          {
            field: "textBlur",
            label: "文字模糊",
            min: 0,
            max: 12,
            step: 0.25,
            unit: " px",
          },
          {
            field: "textOpacity",
            label: "文字不透明度",
            min: 65,
            max: 100,
            unit: "%",
          },
        ] as const
      ).map((item) => (
        <label key={item.field} className="range-label">
          {item.label}
          <output>
            {draft[item.field]}
            {item.unit}
          </output>
          <input
            type="range"
            aria-label={item.label}
            min={item.min}
            max={item.max}
            step={"step" in item ? item.step : 1}
            value={draft[item.field] ?? 0}
            onChange={(event) =>
              update({ [item.field]: Number(event.target.value) }, false)
            }
            onPointerUp={() =>
              void saveSettings(draftRef.current).catch((error) =>
                notify(errorText(error)),
              )
            }
            onKeyUp={() =>
              void saveSettings(draftRef.current).catch((error) =>
                notify(errorText(error)),
              )
            }
            onBlur={() =>
              void saveSettings(draftRef.current).catch((error) =>
                notify(errorText(error)),
              )
            }
          />
        </label>
      ))}
      <div className="setting-row">
        <div>
          <h3>12 小时制</h3>
          <p>显示上午或下午</p>
        </div>
        <label className="switch">
          <input
            aria-label="12小时制"
            type="checkbox"
            checked={draft.hour12}
            onChange={(event) => update({ hour12: event.target.checked })}
          />
          <span />
        </label>
      </div>
      <div className="setting-row">
        <div>
          <h3>显示秒数</h3>
        </div>
        <label className="switch">
          <input
            aria-label="显示秒数"
            type="checkbox"
            checked={draft.showSeconds}
            onChange={(event) => update({ showSeconds: event.target.checked })}
          />
          <span />
        </label>
      </div>
      <div className="setting-row">
        <div>
          <h3>开机启动</h3>
          <p>登录 Windows 后在后台运行</p>
        </div>
        <label className="switch">
          <input
            aria-label="开机启动"
            type="checkbox"
            checked={autostart}
            onChange={(event) => {
              const value = event.target.checked;
              void (value ? enable() : disable())
                .then(() => setAutostart(value))
                .catch((error) => notify(errorText(error)));
            }}
          />
          <span />
        </label>
      </div>
    </div>
  );
}
