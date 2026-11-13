export interface Geometry {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface Alarm {
  id: string;
  title: string;
  time: string;
  date: string | null;
  weekdays: number[];
  enabled: boolean;
  sound: string | null;
  volume: number;
  snoozeMinutes: number;
  nextAt: number | null;
}
export interface Timer {
  durationMs: number;
  remainingMs: number;
  status: "ready" | "running" | "paused" | "finished";
  endAt: number | null;
  revision: number;
  sampledAt: number;
}
export interface Note {
  id: string;
  title: string;
  content: string;
  createdAt: number;
  updatedAt: number;
  revision: number;
  pinned: boolean;
  open: boolean;
  geometry: Geometry | null;
}
export interface Settings {
  background: "mist" | "forest" | "night" | "custom";
  backgroundImage: string | null;
  opacity: number;
  blur: number;
  textOpacity: number;
  textBlur: number;
  pinned: boolean;
  compact: boolean;
  hour12: boolean;
  showSeconds: boolean;
  trayHintShown: boolean;
  geometry: Geometry | null;
  fullGeometry: Geometry | null;
  compactGeometry: Geometry | null;
  timerSound: string | null;
  timerVolume: number;
}
export interface Profile {
  id: string;
  name: string;
  baseUrl: string;
  model: string;
  protocol: string;
}
export interface Reminder {
  id: string;
  title: string;
  source: string;
  alarmId: string | null;
  scheduledAt: number;
  status: "ringing" | "missed" | "snoozed";
  sound: string | null;
  volume: number;
  snoozeMinutes: number;
}
export interface Snapshot {
  alarms: Alarm[];
  timer: Timer;
  notes: Note[];
  settings: Settings;
  profiles: Profile[];
  reminders: Reminder[];
}
export interface AiEvent {
  requestId: string;
  status: string;
  delta: string;
  error: string | null;
}
