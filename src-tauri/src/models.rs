use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alarm {
    pub id: String,
    pub title: String,
    pub time: String,
    pub date: Option<String>,
    pub weekdays: Vec<u32>,
    pub enabled: bool,
    pub sound: Option<String>,
    pub volume: f32,
    pub snooze_minutes: u32,
    pub next_at: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timer {
    pub duration_ms: u64,
    pub remaining_ms: u64,
    pub status: String,
    pub end_at: Option<i64>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default, skip_deserializing)]
    pub sampled_at: u64,
}

impl Default for Timer {
    fn default() -> Self {
        Self {
            duration_ms: 1_500_000,
            remaining_ms: 1_500_000,
            status: "ready".into(),
            end_at: None,
            revision: 0,
            sampled_at: 0,
        }
    }
}

impl Timer {
    pub fn sample(&self, anchor: Option<(u64, u64)>, tick: u64) -> Self {
        let mut sample = self.clone();
        if let Some((start, remaining)) = anchor {
            sample.remaining_ms = remaining.saturating_sub(tick.saturating_sub(start));
        }
        sample.sampled_at = tick;
        sample
    }
}

#[cfg(test)]
mod timer_tests {
    use super::*;

    #[test]
    fn snapshot_uses_elapsed_time_even_between_scheduler_ticks() {
        let timer = Timer {
            status: "running".into(),
            remaining_ms: 10000,
            ..Timer::default()
        };
        let first = timer.sample(Some((1000, 10000)), 2350);
        assert_eq!(first.remaining_ms, 8650);
        assert_eq!(first.sampled_at, 2350);
        let expired = timer.sample(Some((1000, 10000)), 20000);
        assert_eq!(expired.remaining_ms, 0);
    }

    #[test]
    fn existing_timer_records_load_with_sync_defaults() {
        let timer: Timer = serde_json::from_str(
            r#"{"durationMs":20000,"remainingMs":5000,"status":"paused","endAt":null}"#,
        )
        .unwrap();
        assert_eq!(timer.revision, 0);
        assert_eq!(timer.sample(None, 1000).remaining_ms, 5000);
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct Settings {
    pub background: String,
    pub background_image: Option<String>,
    pub opacity: u32,
    pub blur: u32,
    pub text_opacity: u32,
    pub text_blur: f32,
    pub pinned: bool,
    pub compact: bool,
    pub hour12: bool,
    pub show_seconds: bool,
    pub tray_hint_shown: bool,
    pub geometry: Option<Geometry>,
    pub full_geometry: Option<Geometry>,
    pub compact_geometry: Option<Geometry>,
    pub timer_sound: Option<String>,
    pub timer_volume: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            background: "mist".into(),
            background_image: None,
            opacity: 46,
            blur: 25,
            text_opacity: 93,
            text_blur: 0.0,
            pinned: false,
            compact: false,
            hour12: false,
            show_seconds: true,
            tray_hint_shown: false,
            geometry: None,
            full_geometry: None,
            compact_geometry: None,
            timer_sound: None,
            timer_volume: 0.7,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub revision: u64,
    pub pinned: bool,
    pub open: bool,
    pub geometry: Option<Geometry>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub protocol: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: String,
    pub title: String,
    pub source: String,
    pub alarm_id: Option<String>,
    pub scheduled_at: i64,
    pub status: String,
    pub sound: Option<String>,
    pub volume: f32,
    pub snooze_minutes: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub alarms: Vec<Alarm>,
    pub timer: Timer,
    pub notes: Vec<Note>,
    pub profiles: Vec<Profile>,
    pub settings: Settings,
    pub reminders: Vec<Reminder>,
}
