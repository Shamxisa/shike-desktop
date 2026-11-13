import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { Settings, Snapshot, Timer } from "./types";

export function call<T = void>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri())
    return Promise.reject(
      new Error(
        "请使用 Windows 桌面应用启动，以连接本地后台。开发命令为 npm run desktop。",
      ),
    );
  return invoke<T>(command, args);
}

export function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function subscribe(
  onSnapshot: (value: Snapshot) => void,
  onTimer: (value: Timer) => void,
  onError: (message: string) => void,
) {
  if (!isTauri())
    throw new Error("请使用 npm run desktop 启动 Windows 桌面应用。");
  const cleanups = await Promise.all([
    listen<Snapshot>("state-changed", (event) => onSnapshot(event.payload)),
    listen<Timer>("timer-updated", (event) => onTimer(event.payload)),
    listen<string>("app-error", (event) => onError(event.payload)),
    listen<string>("tray-hint", (event) => onError(event.payload)),
  ]);
  return () => cleanups.forEach((cleanup) => cleanup());
}

export async function importMedia(
  kind: "image" | "audio",
): Promise<string | null> {
  const path = await open({
    multiple: false,
    filters: [
      {
        name: kind === "image" ? "背景图片" : "铃声文件",
        extensions:
          kind === "image"
            ? ["png", "jpg", "jpeg", "webp"]
            : ["wav", "mp3", "ogg", "flac"],
      },
    ],
  });
  if (!path) return null;
  return call<string>("import_media", { path, kind });
}

let settingsQueue: Promise<void> = Promise.resolve();
export function saveSettings(settings: Settings): Promise<void> {
  const operation = settingsQueue
    .catch(() => {})
    .then(() => call("save_settings", { settings }));
  settingsQueue = operation;
  return operation;
}

export function formatDuration(milliseconds: number): string {
  const total = Math.ceil(milliseconds / 1000);
  const hours = Math.floor(total / 3600);
  const parts = [Math.floor((total % 3600) / 60), total % 60].map((value) =>
    String(value).padStart(2, "0"),
  );
  if (hours) parts.unshift(String(hours).padStart(2, "0"));
  return parts.join(":");
}
