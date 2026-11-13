import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  AlarmClock,
  Bell,
  BellRing,
  BellOff,
  Check,
  Expand,
  FileText,
  Hourglass,
  Image,
  Maximize2,
  Minus,
  Pause,
  Pin,
  Play,
  RotateCcw,
  Settings2,
  Sparkles,
  X,
} from "lucide-react";
import { call, errorText, subscribe } from "./api";
import { receiveTimer, type CountdownSample } from "./countdown-clock";
import TimerReadout from "./components/TimerReadout";
import type { Settings, Snapshot, Timer } from "./types";
import Alarms from "./components/Alarms";
import Countdown from "./components/Countdown";
import Notes, { NoteEditor, type NoteEditorHandle } from "./components/Notes";
import Appearance from "./components/Appearance";
import AI from "./components/AI";

type Page = "alarms" | "timer" | "notes" | "appearance" | "ai";
const pages = [
  { id: "alarms", label: "闹钟", Icon: AlarmClock },
  { id: "timer", label: "倒计时", Icon: Hourglass },
  { id: "notes", label: "便签", Icon: FileText },
  { id: "appearance", label: "外观", Icon: Image },
  { id: "ai", label: "AI 服务", Icon: Sparkles },
] as const;
const noteId = new URLSearchParams(location.search).get("note");
const resizeEdges = [
  "North",
  "NorthEast",
  "East",
  "SouthEast",
  "South",
  "SouthWest",
  "West",
  "NorthWest",
] as const;
const appearanceFields = [
  "background",
  "backgroundImage",
  "opacity",
  "blur",
  "textOpacity",
  "textBlur",
  "hour12",
  "showSeconds",
] as const;

export default function App() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [startupError, setStartupError] = useState("");
  const [page, setPage] = useState<Page>("alarms");
  const [display, setDisplay] = useState<"clock" | "timer">("clock");
  const [now, setNow] = useState(new Date());
  const [toast, setToast] = useState("");
  const [preview, setPreview] = useState<Settings | null>(null);
  const [image, setImage] = useState("");
  const editorRef = useRef<NoteEditorHandle>(null);
  const countdown = useRef<CountdownSample | null>(null);
  const toastTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);
  const notify = useCallback((message: string) => {
    setToast(message);
    if (toastTimeout.current) clearTimeout(toastTimeout.current);
    toastTimeout.current = setTimeout(() => setToast(""), 6500);
  }, []);
  const act = useCallback(
    async (command: string, args?: Record<string, unknown>) => {
      try {
        await call(command, args);
      } catch (error) {
        notify(errorText(error));
      }
    },
    [notify],
  );

  useEffect(() => {
    let alive = true;
    let cleanup: (() => void) | undefined;
    async function connect() {
      try {
        cleanup = await subscribe(
          (value) => {
            if (alive) {
              countdown.current = receiveTimer(
                countdown.current,
                value.timer,
                performance.now(),
              );
              setSnapshot({ ...value, timer: countdown.current.timer });
              setPreview((current) =>
                current &&
                appearanceFields.some(
                  (field) => current[field] !== value.settings[field],
                )
                  ? current
                  : null,
              );
            }
          },
          (timer: Timer) => {
            if (alive) {
              countdown.current = receiveTimer(
                countdown.current,
                timer,
                performance.now(),
              );
              const accepted = countdown.current.timer;
              setSnapshot((current) =>
                current ? { ...current, timer: accepted } : current,
              );
            }
          },
          notify,
        );
        const value = await call<Snapshot>("get_snapshot");
        if (alive) {
          countdown.current = receiveTimer(
            countdown.current,
            value.timer,
            performance.now(),
          );
          setSnapshot({ ...value, timer: countdown.current.timer });
        }
      } catch (error) {
        if (alive) setStartupError(errorText(error));
      }
    }
    void connect();
    const clock = setInterval(() => setNow(new Date()), 250);
    return () => {
      alive = false;
      cleanup?.();
      clearInterval(clock);
      if (toastTimeout.current) clearTimeout(toastTimeout.current);
    };
  }, [notify]);

  useEffect(() => {
    if (!isTauri()) return;
    let removeClose: (() => void) | undefined;
    let removeExit: (() => void) | undefined;
    let active = true;
    void getCurrentWindow()
      .onCloseRequested(async (event) => {
        event.preventDefault();
        if (editorRef.current && !(await editorRef.current.flush())) return;
        await call("close_window");
      })
      .then((cleanup) => {
        if (active) removeClose = cleanup;
        else cleanup();
      })
      .catch(() => {});
    void listen("app-exiting", async () => {
      const success = editorRef.current
        ? await editorRef.current.flush()
        : true;
      await call("complete_exit", { success });
    })
      .then((cleanup) => {
        if (active) removeExit = cleanup;
        else cleanup();
      })
      .catch(() => {});
    return () => {
      active = false;
      removeClose?.();
      removeExit?.();
    };
  }, []);

  const settings = preview ?? snapshot?.settings;
  useEffect(() => {
    let alive = true;
    setImage("");
    if (settings?.background === "custom" && settings.backgroundImage) {
      void call<string>("image_data", { name: settings.backgroundImage })
        .then((value) => {
          if (alive) setImage(value);
        })
        .catch((error) => notify(errorText(error)));
    }
    return () => {
      alive = false;
    };
  }, [settings?.background, settings?.backgroundImage, notify]);

  async function navigate(next: Page) {
    if (editorRef.current && !(await editorRef.current.flush())) return;
    setPage(next);
    setPreview(null);
    if (next === "alarms") setDisplay("clock");
    if (next === "timer") setDisplay("timer");
  }

  if (!snapshot || !settings)
    return (
      <main className="startup">
        <AlarmClock size={36} />
        <h1>时刻</h1>
        <p>{startupError || "正在连接本地服务…"}</p>
        {startupError && (
          <button onClick={() => location.reload()}>重新连接</button>
        )}
      </main>
    );
  const style = {
    "--glass-opacity": settings.opacity / 100,
    "--glass-blur": `${settings.blur}px`,
    "--text-alpha": settings.textOpacity / 100,
    "--text-blur": `${settings.textBlur ?? 0}px`,
  } as CSSProperties;
  const activeNote = snapshot.notes.find((note) => note.id === noteId);
  const pinned = noteId ? activeNote?.pinned : snapshot.settings.pinned;
  const timer = snapshot.timer;
  const nextAlarm = snapshot.alarms
    .filter((alarm) => alarm.enabled && alarm.nextAt)
    .sort((a, b) => a.nextAt! - b.nextAt!)[0];
  const states = {
    ready: "待开始",
    running: "进行中",
    paused: "已暂停",
    finished: "时间到",
  };
  const timeParts = new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: settings.hour12,
  }).formatToParts(now);
  const clock = timeParts
    .filter((part) => ["hour", "minute", "literal"].includes(part.type))
    .map((part) => part.value)
    .join("")
    .trim();
  const period = timeParts.find((part) => part.type === "dayPeriod")?.value;
  const date = new Intl.DateTimeFormat("zh-CN", {
    month: "long",
    day: "numeric",
    weekday: "long",
  }).format(now);
  const compact = !noteId && snapshot.settings.compact;
  const visibleReminders = noteId
    ? []
    : snapshot.reminders.filter((item) => item.status !== "snoozed");

  return (
    <main
      className={`desktop-app ${compact ? "compact" : ""} ${compact && display === "timer" ? "compact-timer" : ""} ${noteId ? "sticky-window" : ""}`}
      data-background={settings.background}
      style={style}
    >
      <div className="app-wallpaper" aria-hidden="true">
        {image && (
          <img
            src={image}
            alt=""
            onError={() => notify("背景图片加载失败，请重新选择图片")}
          />
        )}
      </div>
      {!noteId && !compact && (
        <aside className="sidebar">
          <div className="brand" data-tauri-drag-region>
            时刻<span>SHIKE</span>
          </div>
          <nav aria-label="功能导航">
            {pages.map(({ id, label, Icon }) => (
              <button
                key={id}
                aria-label={label}
                aria-pressed={page === id}
                onClick={() => void navigate(id)}
              >
                <Icon size={20} />
                <span>{label}</span>
              </button>
            ))}
          </nav>
        </aside>
      )}
      <div className="app-main" data-tauri-drag-region={compact || undefined}>
        {!compact && (
          <header
            className="titlebar"
            data-tauri-drag-region
            onDoubleClick={() => {
              if (!compact && !noteId) void getCurrentWindow().toggleMaximize();
            }}
          >
            <h1 data-tauri-drag-region>
              {noteId
                ? activeNote?.title || "便签"
                : pages.find((item) => item.id === page)?.label}
            </h1>
            <div className="window-actions">
              <button
                className={pinned ? "active" : ""}
                aria-label={pinned ? "取消置顶" : "置顶窗口"}
                aria-pressed={!!pinned}
                onClick={() => void act("set_pinned", { pinned: !pinned })}
              >
                <Pin size={15} />
                <span>{pinned ? "已置顶" : "置顶"}</span>
              </button>
              {!noteId && (
                <button
                  aria-label={compact ? "展开管理" : "小窗模式"}
                  title={compact ? "展开管理" : "小窗模式"}
                  onClick={() => void act("set_compact", { compact: !compact })}
                >
                  <Expand size={15} />
                  <span>{compact ? "展开管理" : "小窗模式"}</span>
                </button>
              )}
              <button
                className="icon-button"
                aria-label="最小化"
                onClick={() => void getCurrentWindow().minimize()}
              >
                <Minus size={16} />
              </button>
              {!compact && !noteId && (
                <button
                  className="icon-button"
                  aria-label="最大化或还原"
                  onClick={() => void getCurrentWindow().toggleMaximize()}
                >
                  <Maximize2 size={14} />
                </button>
              )}
              <button
                className="icon-button close-button"
                aria-label={noteId ? "关闭便签窗口" : "关闭到系统托盘"}
                onClick={() => void getCurrentWindow().close()}
              >
                <X size={17} />
              </button>
            </div>
          </header>
        )}
        <div
          className="glass-sheet"
          data-tauri-drag-region={compact || undefined}
        >
          {noteId ? (
            activeNote ? (
              <NoteEditor ref={editorRef} note={activeNote} notify={notify} />
            ) : (
              <div className="empty-state">
                <p>该便签已删除。</p>
              </div>
            )
          ) : (
            <>
              {(compact || page === "alarms" || page === "timer") && (
                <section
                  className="clock-scene"
                  data-tauri-drag-region={compact || undefined}
                >
                  <div
                    className="clock-meta"
                    data-tauri-drag-region={compact || undefined}
                  >
                    <div
                      className="display-picker"
                      role="group"
                      aria-label="显示内容"
                    >
                      <button
                        aria-pressed={display === "clock"}
                        onClick={() => setDisplay("clock")}
                      >
                        时钟
                      </button>
                      <button
                        aria-pressed={display === "timer"}
                        onClick={() => setDisplay("timer")}
                      >
                        倒计时
                      </button>
                    </div>
                    <span data-tauri-drag-region={compact || undefined}>
                      {date}
                    </span>
                  </div>
                  <div
                    className={`clock-face ${display === "timer" && timer.remainingMs >= 3_600_000 ? "long-time" : ""}`}
                    data-tauri-drag-region={compact || undefined}
                  >
                    <span data-tauri-drag-region={compact || undefined}>
                      {display === "timer"
                        ? countdown.current && (
                            <TimerReadout sample={countdown.current} />
                          )
                        : clock}
                    </span>
                    {display === "clock" && (
                      <div className="clock-suffix">
                        {settings.showSeconds && (
                          <small data-tauri-drag-region={compact || undefined}>
                            {String(now.getSeconds()).padStart(2, "0")}
                          </small>
                        )}
                        {period && (
                          <small className="day-period">{period}</small>
                        )}
                      </div>
                    )}
                  </div>
                  <div
                    className="next-alarm"
                    data-tauri-drag-region={compact || undefined}
                  >
                    <Bell size={14} />
                    <span data-tauri-drag-region={compact || undefined}>
                      {compact && toast
                        ? toast
                        : compact && visibleReminders.length
                          ? `到时提醒 · ${visibleReminders[0].title}`
                          : display === "timer"
                            ? `倒计时 · ${states[timer.status]}`
                            : nextAlarm
                              ? `下次提醒 ${new Date(nextAlarm.nextAt!).toLocaleString("zh-CN", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit", hour12: false })} · ${nextAlarm.title}`
                              : "尚未设置闹钟"}
                    </span>
                    {compact && visibleReminders.length > 0 && (
                      <button
                        className="compact-dismiss"
                        onClick={() => void act("dismiss_reminders")}
                      >
                        <BellOff size={12} />
                        关闭响铃
                      </button>
                    )}
                  </div>
                  {compact && (
                    <div className="compact-actions">
                      <button
                        aria-label={pinned ? "取消置顶" : "置顶窗口"}
                        aria-pressed={!!pinned}
                        title={pinned ? "取消置顶" : "置顶窗口"}
                        onClick={() =>
                          void act("set_pinned", { pinned: !pinned })
                        }
                      >
                        <Pin size={12} />
                        <span>{pinned ? "已置顶" : "置顶"}</span>
                      </button>
                      <button
                        aria-label="展开管理"
                        title="展开管理"
                        onClick={() =>
                          void act("set_compact", { compact: false })
                        }
                      >
                        <Expand size={12} />
                        <span>展开管理</span>
                      </button>
                    </div>
                  )}
                  {display === "timer" && (
                    <div className="timer-controls">
                      <button
                        className="primary"
                        onClick={() =>
                          void act("timer_action", {
                            action:
                              timer.status === "running" ? "pause" : "start",
                          })
                        }
                      >
                        {timer.status === "running" ? (
                          <Pause size={15} />
                        ) : (
                          <Play size={15} />
                        )}
                        {timer.status === "running"
                          ? "暂停"
                          : timer.status === "paused"
                            ? "继续"
                            : timer.status === "finished"
                              ? "再来一次"
                              : "开始"}
                      </button>
                      <button
                        onClick={() =>
                          void act("timer_action", { action: "reset" })
                        }
                      >
                        <RotateCcw size={14} />
                        重置
                      </button>
                    </div>
                  )}
                </section>
              )}
              {!compact && (
                <section className="page-content">
                  {page === "alarms" && (
                    <Alarms alarms={snapshot.alarms} notify={notify} />
                  )}
                  {page === "timer" && (
                    <Countdown
                      timer={timer}
                      settings={settings}
                      notify={notify}
                    />
                  )}
                  {page === "notes" && (
                    <Notes
                      notes={snapshot.notes}
                      editorRef={editorRef}
                      notify={notify}
                    />
                  )}
                  {page === "appearance" && (
                    <Appearance
                      settings={settings}
                      onPreview={setPreview}
                      notify={notify}
                    />
                  )}
                  {page === "ai" && (
                    <AI profiles={snapshot.profiles} notify={notify} />
                  )}
                </section>
              )}
            </>
          )}
          {!!visibleReminders.length && !compact && (
            <div className="reminders" aria-live="assertive">
              {visibleReminders.map((reminder) => (
                <section className="reminder" key={reminder.id}>
                  <div>
                    <BellRing size={20} />
                    <div>
                      <h3>{reminder.title}</h3>
                      <p>
                        {reminder.status === "missed"
                          ? "错过的提醒"
                          : "时间到了"}{" "}
                        ·{" "}
                        {new Date(reminder.scheduledAt).toLocaleTimeString(
                          "zh-CN",
                          { hour: "2-digit", minute: "2-digit" },
                        )}
                      </p>
                    </div>
                  </div>
                  <div className="inline-actions">
                    {reminder.source === "alarm" && (
                      <button
                        onClick={() =>
                          void act("reminder_action", {
                            id: reminder.id,
                            snooze: true,
                          })
                        }
                      >
                        稍后 {reminder.snoozeMinutes} 分钟
                      </button>
                    )}
                    <button
                      className="primary"
                      onClick={() =>
                        void act("reminder_action", {
                          id: reminder.id,
                          snooze: false,
                        })
                      }
                    >
                      <Check size={14} />
                      关闭提醒
                    </button>
                  </div>
                </section>
              ))}
            </div>
          )}
          {!noteId && !compact && (
            <footer>
              <span>
                <span className="status-dot" />
                本地服务运行中
              </span>
              <button onClick={() => void navigate("appearance")}>
                <Settings2 size={13} />
                设置
              </button>
            </footer>
          )}
        </div>
      </div>
      {compact &&
        resizeEdges.map((direction) => (
          <div
            key={direction}
            className={`resize-handle resize-${direction.toLowerCase()}`}
            aria-hidden="true"
            title="拖动调整小窗大小"
            onMouseDown={(event) => {
              if (event.button !== 0) return;
              event.preventDefault();
              event.stopPropagation();
              void getCurrentWindow()
                .startResizeDragging(direction)
                .catch((error) => notify(errorText(error)));
            }}
          />
        ))}
      {toast && !compact && (
        <div className="toast" role="status">
          <span>{toast}</span>
          <button
            className="icon-button"
            aria-label="关闭提示"
            onClick={() => setToast("")}
          >
            <X size={14} />
          </button>
        </div>
      )}
    </main>
  );
}
