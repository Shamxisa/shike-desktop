use crate::{
    audio::AudioCommand,
    models::*,
    storage::{self, Store},
    AppResult, AppState,
};
use chrono::{Datelike, Local, NaiveDate, NaiveTime, TimeZone, Utc};
use rusqlite::params;
use std::{sync::mpsc::Sender, thread, time::Duration};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

pub fn uptime_ms() -> u64 {
    #[cfg(windows)]
    {
        unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() }
    }
    #[cfg(not(windows))]
    {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_millis() as u64
    }
}

pub fn next_occurrence(alarm: &Alarm, after: i64) -> AppResult<Option<i64>> {
    if !alarm.enabled {
        return Ok(None);
    }
    let time =
        NaiveTime::parse_from_str(&alarm.time, "%H:%M").map_err(|_| "请输入有效的闹钟时间")?;
    let local = Local
        .timestamp_millis_opt(after)
        .single()
        .ok_or("系统时间无效")?;
    let explicit_date = alarm
        .date
        .as_ref()
        .map(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d"))
        .transpose()
        .map_err(|_| "闹钟日期无效")?;
    for days in 0..370 {
        let date = if let Some(date) = explicit_date {
            if days > 0 {
                break;
            }
            date
        } else {
            local.date_naive() + chrono::Duration::days(days)
        };
        if !alarm.weekdays.is_empty()
            && !alarm
                .weekdays
                .contains(&date.weekday().num_days_from_monday())
        {
            continue;
        }
        // DST gaps have no local instant. A repeated local hour fires once at its earliest instant.
        if let Some(instant) = Local.from_local_datetime(&date.and_time(time)).earliest() {
            let timestamp = instant.timestamp_millis();
            if timestamp > after {
                return Ok(Some(timestamp));
            }
        }
    }
    Ok(None)
}

pub fn validate_alarm(alarm: &Alarm) -> AppResult<()> {
    if alarm.title.trim().is_empty() || alarm.title.chars().count() > 120 {
        return Err("闹钟名称需要为 1 到 120 个字符".into());
    }
    if alarm.weekdays.iter().any(|day| *day > 6) {
        return Err("星期设置无效".into());
    }
    if !alarm.volume.is_finite() || !(0.0..=1.0).contains(&alarm.volume) {
        return Err("音量设置无效".into());
    }
    if !(1..=120).contains(&alarm.snooze_minutes) {
        return Err("稍后提醒间隔需要为 1 到 120 分钟".into());
    }
    NaiveTime::parse_from_str(&alarm.time, "%H:%M").map_err(|_| "请输入有效的闹钟时间")?;
    Ok(())
}

pub fn update_timer(store: &mut Store, now: i64, tick: u64) -> AppResult<bool> {
    let Some((start, remaining)) = store.timer_anchor else {
        return Ok(false);
    };
    store.timer.remaining_ms = remaining.saturating_sub(tick.saturating_sub(start));
    store.timer.end_at = Some(now + store.timer.remaining_ms as i64);
    if store.timer.remaining_ms > 0 {
        return Ok(false);
    }
    let mut finished = store.timer.clone();
    finished.status = "finished".into();
    finished.end_at = None;
    finished.revision = finished.revision.saturating_add(1);
    let reminder = Reminder {
        id: Uuid::new_v4().to_string(),
        title: "倒计时结束".into(),
        source: "timer".into(),
        alarm_id: None,
        scheduled_at: now,
        status: "ringing".into(),
        sound: store.settings.timer_sound.clone(),
        volume: store.settings.timer_volume,
        snooze_minutes: 5,
    };
    let transaction = store.db.transaction().map_err(|error| error.to_string())?;
    storage::save(&transaction, "timer", "main", &finished)?;
    storage::save(&transaction, "reminder", &reminder.id, &reminder)?;
    transaction.commit().map_err(|error| error.to_string())?;
    store.timer = finished;
    store.timer_anchor = None;
    store.reminders.push(reminder);
    Ok(true)
}

fn fire_alarm(store: &mut Store, alarm: &Alarm, due: i64, now: i64, missed: bool) -> AppResult<()> {
    let mut updated = alarm.clone();
    if updated.weekdays.is_empty() {
        updated.enabled = false;
    }
    updated.next_at = next_occurrence(&updated, now)?;
    let reminder = Reminder {
        id: Uuid::new_v4().to_string(),
        title: alarm.title.clone(),
        source: "alarm".into(),
        alarm_id: Some(alarm.id.clone()),
        scheduled_at: due,
        status: if missed { "missed" } else { "ringing" }.into(),
        sound: alarm.sound.clone(),
        volume: alarm.volume,
        snooze_minutes: alarm.snooze_minutes,
    };
    let transaction = store.db.transaction().map_err(|error| error.to_string())?;
    let inserted = transaction
        .execute(
            "INSERT OR IGNORE INTO occurrences(task_id,due_at) VALUES(?1,?2)",
            params![alarm.id, due],
        )
        .map_err(|error| error.to_string())?;
    storage::save(&transaction, "alarm", &alarm.id, &updated)?;
    if inserted > 0 {
        storage::save(&transaction, "reminder", &reminder.id, &reminder)?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    if let Some(existing) = store.alarms.iter_mut().find(|item| item.id == alarm.id) {
        *existing = updated;
    }
    if inserted > 0 {
        store.reminders.push(reminder);
    }
    Ok(())
}

pub fn sync_audio(store: &mut Store, audio: &Sender<AudioCommand>, media: &std::path::Path) {
    let active = store
        .reminders
        .iter()
        .find(|reminder| reminder.status == "ringing");
    let id = active.map(|reminder| reminder.id.clone());
    if id == store.playing {
        return;
    }
    store.playing = id;
    if let Some(reminder) = active {
        let path = reminder.sound.as_ref().map(|file| media.join(file));
        let _ = audio.send(AudioCommand::Play {
            path,
            volume: reminder.volume,
            repeat: true,
        });
    } else {
        let _ = audio.send(AudioCommand::Stop);
    }
}

fn step(store: &mut Store) -> AppResult<bool> {
    let tick = uptime_ms();
    let now = Utc::now().timestamp_millis();
    let tick_gap = tick.saturating_sub(store.last_tick);
    let wall_gap = now - store.last_wall;
    let offset = Local::now().offset().local_minus_utc();
    let clock_changed = (wall_gap - tick_gap as i64).abs() > 2_000 || offset != store.last_offset;
    let interrupted = tick_gap > 10_000 || wall_gap < 0;
    let mut changed = update_timer(store, now, tick)?;
    for alarm in store.alarms.clone().iter().filter(|alarm| alarm.enabled) {
        if let Some(due) = alarm.next_at {
            if due <= now {
                fire_alarm(store, alarm, due, now, interrupted || now - due > 10_000)?;
                changed = true;
            }
        }
    }
    if clock_changed {
        for alarm in store.alarms.clone().iter().filter(|alarm| alarm.enabled) {
            let mut updated = alarm.clone();
            updated.next_at = next_occurrence(&updated, now)?;
            storage::save(&store.db, "alarm", &alarm.id, &updated)?;
            if let Some(existing) = store.alarms.iter_mut().find(|item| item.id == alarm.id) {
                *existing = updated;
            }
        }
        changed = true;
    }
    for reminder in store
        .reminders
        .clone()
        .iter()
        .filter(|item| item.status == "snoozed" && item.scheduled_at <= now)
    {
        let mut updated = reminder.clone();
        updated.status = if interrupted || now - reminder.scheduled_at > 10_000 {
            "missed"
        } else {
            "ringing"
        }
        .into();
        storage::save(&store.db, "reminder", &updated.id, &updated)?;
        if let Some(existing) = store
            .reminders
            .iter_mut()
            .find(|item| item.id == updated.id)
        {
            *existing = updated;
        }
        changed = true;
    }
    if tick.saturating_sub(store.last_checkpoint) >= 1_000 {
        if store.timer.status == "running" {
            storage::save(&store.db, "timer", "main", &store.timer)?;
        }
        if store.geometry_dirty {
            let transaction = store.db.transaction().map_err(|error| error.to_string())?;
            storage::save(&transaction, "settings", "main", &store.settings)?;
            for note in &store.notes {
                storage::save(&transaction, "note", &note.id, note)?;
            }
            transaction.commit().map_err(|error| error.to_string())?;
            store.geometry_dirty = false;
        }
        store.last_checkpoint = tick;
    }
    store.last_tick = tick;
    store.last_wall = now;
    store.last_offset = offset;
    Ok(changed)
}

pub fn start(app: AppHandle) {
    thread::spawn(move || {
        let mut last_emit = 0;
        let mut last_error = 0;
        loop {
            thread::sleep(Duration::from_millis(200));
            let state = app.state::<AppState>();
            let Ok(mut store) = state.store.lock() else {
                break;
            };
            match step(&mut store) {
                Ok(changed) => {
                    sync_audio(&mut store, &state.audio, &state.media);
                    let snapshot = changed.then(|| store.snapshot());
                    let timer = store.timer_snapshot();
                    drop(store);
                    if let Some(snapshot) = snapshot {
                        let _ = app.emit("state-changed", snapshot);
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.request_user_attention(Some(
                                tauri::UserAttentionType::Informational,
                            ));
                        }
                    }
                    let tick = uptime_ms();
                    if tick.saturating_sub(last_emit) >= 500 {
                        let _ = app.emit("timer-updated", timer);
                        last_emit = tick;
                    }
                }
                Err(error) => {
                    if uptime_ms().saturating_sub(last_error) > 5_000 {
                        let _ = app.emit("app-error", format!("任务保存失败，{error}"));
                        last_error = uptime_ms();
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeating_alarm_moves_to_next_allowed_day() {
        let after = Local
            .with_ymd_and_hms(2026, 11, 13, 8, 0, 0)
            .unwrap()
            .timestamp_millis();
        let alarm = Alarm {
            id: "test".into(),
            title: "test".into(),
            time: "07:30".into(),
            date: None,
            weekdays: vec![0, 1, 2, 3, 4],
            enabled: true,
            sound: None,
            volume: 0.5,
            snooze_minutes: 5,
            next_at: None,
        };
        let next = Local
            .timestamp_millis_opt(next_occurrence(&alarm, after).unwrap().unwrap())
            .unwrap();
        assert_eq!(next.weekday().num_days_from_monday(), 0);
        assert_eq!(
            next.date_naive(),
            NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
        );
    }
}
