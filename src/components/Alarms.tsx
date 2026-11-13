import { useState } from "react";
import { Bell, MoreHorizontal, Plus, Trash2, X } from "lucide-react";
import { call, errorText, importMedia } from "../api";
import type { Alarm } from "../types";

const weekdays = ["一", "二", "三", "四", "五", "六", "日"];
const emptyAlarm = (): Alarm => ({
  id: "",
  title: "",
  time: "07:30",
  date: null,
  weekdays: [0, 1, 2, 3, 4],
  enabled: true,
  sound: null,
  volume: 0.7,
  snoozeMinutes: 5,
  nextAt: null,
});

function repeatLabel(alarm: Alarm) {
  if (!alarm.weekdays.length)
    return alarm.date ? `${alarm.date} · 仅一次` : "仅一次";
  if (alarm.weekdays.length === 7) return "每天";
  if (alarm.weekdays.join(",") === "0,1,2,3,4") return "工作日";
  return `每周${alarm.weekdays.map((day) => weekdays[day]).join("、")}`;
}

export default function Alarms({
  alarms,
  notify,
}: {
  alarms: Alarm[];
  notify: (message: string) => void;
}) {
  const [editing, setEditing] = useState<Alarm | null>(null);
  const [saving, setSaving] = useState(false);
  const act = async (operation: () => Promise<unknown>) => {
    try {
      await operation();
    } catch (error) {
      notify(errorText(error));
    }
  };
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!editing || saving) return;
    setSaving(true);
    try {
      await call("save_alarm", { alarm: editing });
      setEditing(null);
    } catch (error) {
      notify(errorText(error));
    } finally {
      setSaving(false);
    }
  }
  return (
    <>
      <div className="section-heading">
        <div>
          <h2>我的闹钟</h2>
          <p>{alarms.filter((alarm) => alarm.enabled).length} 个已开启</p>
        </div>
        <button className="primary" onClick={() => setEditing(emptyAlarm())}>
          <Plus size={16} />
          新建闹钟
        </button>
      </div>
      {!alarms.length && (
        <div className="empty-state">
          <Bell size={28} />
          <h3>为下一个时刻留个提醒</h3>
          <p>添加起床、午休或其他日常闹钟。</p>
        </div>
      )}
      {[...alarms]
        .sort((a, b) => a.time.localeCompare(b.time))
        .map((alarm) => (
          <div
            key={alarm.id}
            className={`alarm-row ${alarm.enabled ? "" : "inactive"}`}
          >
            <button
              className="alarm-time"
              onClick={() => setEditing({ ...alarm })}
            >
              {alarm.time}
            </button>
            <button
              className="alarm-detail"
              onClick={() => setEditing({ ...alarm })}
            >
              <span>{alarm.title}</span>
              <small>{repeatLabel(alarm)}</small>
            </button>
            <label className="switch" aria-label={`${alarm.title}启用状态`}>
              <input
                type="checkbox"
                checked={alarm.enabled}
                onChange={() =>
                  void act(() =>
                    call("save_alarm", {
                      alarm: { ...alarm, enabled: !alarm.enabled },
                    }),
                  )
                }
              />
              <span />
            </label>
            <button
              className="icon-button"
              aria-label={`编辑${alarm.title}`}
              onClick={() => setEditing({ ...alarm })}
            >
              <MoreHorizontal size={18} />
            </button>
          </div>
        ))}
      {editing && (
        <div
          className="modal-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget && !saving)
              setEditing(null);
          }}
        >
          <section
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="alarm-editor-title"
          >
            <div className="section-heading">
              <h2 id="alarm-editor-title">
                {editing.id ? "编辑闹钟" : "新建闹钟"}
              </h2>
              <button
                className="icon-button"
                aria-label="关闭编辑"
                onClick={() => setEditing(null)}
              >
                <X size={18} />
              </button>
            </div>
            <form onSubmit={submit} className="form-stack">
              <div className="form-grid">
                <label>
                  时间
                  <input
                    autoFocus
                    type="time"
                    required
                    value={editing.time}
                    onChange={(event) =>
                      setEditing({ ...editing, time: event.target.value })
                    }
                  />
                </label>
                <label>
                  名称
                  <input
                    required
                    maxLength={120}
                    placeholder="给这个提醒起个名字"
                    value={editing.title}
                    onChange={(event) =>
                      setEditing({ ...editing, title: event.target.value })
                    }
                  />
                </label>
              </div>
              <fieldset>
                <legend>重复星期</legend>
                <div className="weekday-options">
                  {weekdays.map((day, index) => (
                    <button
                      key={day}
                      type="button"
                      aria-pressed={editing.weekdays.includes(index)}
                      onClick={() =>
                        setEditing({
                          ...editing,
                          date: null,
                          weekdays: editing.weekdays.includes(index)
                            ? editing.weekdays.filter(
                                (value) => value !== index,
                              )
                            : [...editing.weekdays, index].sort(),
                        })
                      }
                    >
                      {day}
                    </button>
                  ))}
                </div>
                <p className="muted">全部取消时为单次闹钟。</p>
              </fieldset>
              {!editing.weekdays.length && (
                <label>
                  日期
                  <input
                    type="date"
                    value={editing.date ?? ""}
                    onChange={(event) =>
                      setEditing({
                        ...editing,
                        date: event.target.value || null,
                      })
                    }
                  />
                  <small>留空则使用下一个该时间。</small>
                </label>
              )}
              <div className="form-grid">
                <label>
                  铃声
                  <select
                    value={editing.sound ? "custom" : "default"}
                    onChange={(event) => {
                      if (event.target.value === "default")
                        setEditing({ ...editing, sound: null });
                    }}
                  >
                    <option value="default">轻柔钟声</option>
                    {editing.sound && (
                      <option value="custom">已导入的铃声</option>
                    )}
                  </select>
                </label>
                <div className="button-field">
                  <button
                    type="button"
                    onClick={() =>
                      void act(async () => {
                        const sound = await importMedia("audio");
                        if (sound) setEditing({ ...editing, sound });
                      })
                    }
                  >
                    选择音频
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      void act(() =>
                        call("preview_sound", {
                          name: editing.sound,
                          volume: editing.volume,
                        }),
                      )
                    }
                  >
                    试听
                  </button>
                  <button
                    type="button"
                    onClick={() => void act(() => call("stop_preview"))}
                  >
                    停止
                  </button>
                </div>
              </div>
              <label>
                音量 <output>{Math.round(editing.volume * 100)}%</output>
                <input
                  type="range"
                  min="0"
                  max="1"
                  step="0.01"
                  value={editing.volume}
                  onChange={(event) =>
                    setEditing({
                      ...editing,
                      volume: Number(event.target.value),
                    })
                  }
                />
              </label>
              <label>
                稍后提醒间隔（分钟）
                <input
                  type="number"
                  min="1"
                  max="120"
                  required
                  value={editing.snoozeMinutes}
                  onChange={(event) =>
                    setEditing({
                      ...editing,
                      snoozeMinutes: Number(event.target.value),
                    })
                  }
                />
              </label>
              <div className="form-actions">
                {editing.id && (
                  <button
                    type="button"
                    className="danger"
                    disabled={saving}
                    onClick={() =>
                      void act(async () => {
                        await call("delete_alarm", { id: editing.id });
                        setEditing(null);
                      })
                    }
                  >
                    <Trash2 size={15} />
                    删除
                  </button>
                )}
                <button type="button" onClick={() => setEditing(null)}>
                  取消
                </button>
                <button className="primary" disabled={saving}>
                  {saving ? "保存中…" : "保存闹钟"}
                </button>
              </div>
            </form>
          </section>
        </div>
      )}
    </>
  );
}
