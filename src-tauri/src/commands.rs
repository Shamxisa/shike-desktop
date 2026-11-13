use crate::{ai, audio::AudioCommand, models::*, scheduler, storage, windows, AppResult, AppState};
use base64::Engine;
use chrono::Utc;
use std::{fs, io::BufReader, path::Path};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use uuid::Uuid;

pub fn broadcast(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    scheduler::sync_audio(&mut store, &state.audio, &state.media);
    let snapshot = store.snapshot();
    drop(store);
    app.emit("state-changed", snapshot)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> AppResult<Snapshot> {
    Ok(state.store.lock().map_err(|_| "数据暂不可用")?.snapshot())
}

#[tauri::command]
pub fn save_alarm(app: AppHandle, state: State<'_, AppState>, mut alarm: Alarm) -> AppResult<()> {
    scheduler::validate_alarm(&alarm)?;
    if alarm.id.is_empty() {
        alarm.id = Uuid::new_v4().to_string();
    }
    alarm.title = alarm.title.trim().into();
    alarm.weekdays.sort_unstable();
    alarm.weekdays.dedup();
    if !alarm.weekdays.is_empty() {
        alarm.date = None;
    }
    validate_sound(&state, &alarm.sound)?;
    alarm.next_at = scheduler::next_occurrence(&alarm, Utc::now().timestamp_millis())?;
    if alarm.enabled && alarm.next_at.is_none() {
        return Err("请选择未来的闹钟时间或日期".into());
    }
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    storage::save(&store.db, "alarm", &alarm.id, &alarm)?;
    if let Some(existing) = store.alarms.iter_mut().find(|item| item.id == alarm.id) {
        *existing = alarm;
    } else {
        store.alarms.push(alarm);
    }
    drop(store);
    broadcast(&app)
}

#[tauri::command]
pub fn delete_alarm(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    let reminders: Vec<String> = store
        .reminders
        .iter()
        .filter(|item| item.alarm_id.as_deref() == Some(&id))
        .map(|item| item.id.clone())
        .collect();
    let transaction = store.db.transaction().map_err(|error| error.to_string())?;
    storage::remove(&transaction, "alarm", &id)?;
    for reminder in reminders {
        storage::remove(&transaction, "reminder", &reminder)?;
    }
    let mut statement = transaction
        .prepare("DELETE FROM occurrences WHERE task_id=?1")
        .map_err(|error| error.to_string())?;
    statement
        .execute([&id])
        .map_err(|error| error.to_string())?;
    drop(statement);
    transaction.commit().map_err(|error| error.to_string())?;
    store.alarms.retain(|alarm| alarm.id != id);
    store
        .reminders
        .retain(|item| item.alarm_id.as_deref() != Some(&id));
    drop(store);
    broadcast(&app)
}

#[tauri::command]
pub fn timer_action(
    app: AppHandle,
    state: State<'_, AppState>,
    action: String,
    duration_ms: Option<u64>,
) -> AppResult<()> {
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    let now = Utc::now().timestamp_millis();
    let tick = scheduler::uptime_ms();
    if scheduler::update_timer(&mut store, now, tick)? {
        app.emit("state-changed", store.snapshot())
            .map_err(|error| error.to_string())?;
    }
    let mut timer = store.timer.clone();
    let anchor;
    match action.as_str() {
        "set" => {
            if timer.status == "running" || timer.status == "paused" {
                return Err("请先重置当前倒计时".into());
            }
            let duration = duration_ms.ok_or("缺少时长")?;
            if !(1_000..=359_999_000).contains(&duration) {
                return Err("时长需要为 1 秒到 99 小时 59 分 59 秒".into());
            }
            timer = Timer {
                duration_ms: duration,
                remaining_ms: duration,
                status: "ready".into(),
                end_at: None,
                revision: timer.revision,
                sampled_at: tick,
            };
            anchor = None;
        }
        "start" => {
            if timer.status == "running" {
                return Ok(());
            }
            if timer.status == "finished" {
                timer.remaining_ms = timer.duration_ms;
            }
            timer.status = "running".into();
            timer.end_at = Some(now + timer.remaining_ms as i64);
            anchor = Some((tick, timer.remaining_ms));
        }
        "pause" => {
            if timer.status != "running" {
                return Ok(());
            }
            timer.status = "paused".into();
            timer.end_at = None;
            anchor = None;
        }
        "reset" => {
            timer.remaining_ms = timer.duration_ms;
            timer.status = "ready".into();
            timer.end_at = None;
            anchor = None;
        }
        _ => return Err("倒计时操作无效".into()),
    }
    let clear_reminders = ["set", "start", "reset"].contains(&action.as_str());
    timer.revision = timer.revision.saturating_add(1);
    let reminders: Vec<String> = store
        .reminders
        .iter()
        .filter(|item| clear_reminders && item.source == "timer")
        .map(|item| item.id.clone())
        .collect();
    let transaction = store.db.transaction().map_err(|error| error.to_string())?;
    storage::save(&transaction, "timer", "main", &timer)?;
    for reminder in reminders {
        storage::remove(&transaction, "reminder", &reminder)?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    if clear_reminders {
        store.reminders.retain(|item| item.source != "timer");
    }
    store.timer = timer;
    store.timer_anchor = anchor;
    drop(store);
    broadcast(&app)
}

#[tauri::command]
pub fn reminder_action(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    snooze: bool,
) -> AppResult<()> {
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    let position = store
        .reminders
        .iter()
        .position(|reminder| reminder.id == id)
        .ok_or("提醒已处理")?;
    if snooze {
        let mut updated = store.reminders[position].clone();
        updated.status = "snoozed".into();
        updated.scheduled_at =
            Utc::now().timestamp_millis() + updated.snooze_minutes as i64 * 60_000;
        storage::save(&store.db, "reminder", &id, &updated)?;
        store.reminders[position] = updated;
    } else {
        storage::remove(&store.db, "reminder", &id)?;
        store.reminders.remove(position);
    }
    drop(store);
    broadcast(&app)
}

#[tauri::command]
pub fn dismiss_reminders(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    let ids = store
        .reminders
        .iter()
        .filter(|reminder| reminder.status != "snoozed")
        .map(|reminder| reminder.id.clone())
        .collect::<Vec<_>>();
    let transaction = store.db.transaction().map_err(|error| error.to_string())?;
    for id in ids {
        storage::remove(&transaction, "reminder", &id)?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    store
        .reminders
        .retain(|reminder| reminder.status == "snoozed");
    drop(store);
    broadcast(&app)
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    mut settings: Settings,
) -> AppResult<()> {
    if !["mist", "forest", "night", "custom"].contains(&settings.background.as_str())
        || !(25..=85).contains(&settings.opacity)
        || settings.blur > 120
        || !(65..=100).contains(&settings.text_opacity)
        || !settings.text_blur.is_finite()
        || !(0.0..=12.0).contains(&settings.text_blur)
    {
        return Err("外观参数无效".into());
    }
    if !settings.timer_volume.is_finite() || !(0.0..=1.0).contains(&settings.timer_volume) {
        return Err("音量无效".into());
    }
    if let Some(image) = &settings.background_image {
        media_path(&state, image)?;
    }
    validate_sound(&state, &settings.timer_sound)?;
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    settings.geometry = store.settings.geometry.clone();
    settings.full_geometry = store.settings.full_geometry.clone();
    settings.compact_geometry = store.settings.compact_geometry.clone();
    settings.compact = store.settings.compact;
    settings.pinned = store.settings.pinned;
    settings.tray_hint_shown = store.settings.tray_hint_shown;
    storage::save(&store.db, "settings", "main", &settings)?;
    store.settings = settings;
    drop(store);
    broadcast(&app)
}

#[tauri::command]
pub fn set_pinned(
    app: AppHandle,
    state: State<'_, AppState>,
    window: WebviewWindow,
    pinned: bool,
) -> AppResult<()> {
    window
        .set_always_on_top(pinned)
        .map_err(|error| error.to_string())?;
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    if window.label() == "main" {
        let mut settings = store.settings.clone();
        settings.pinned = pinned;
        storage::save(&store.db, "settings", "main", &settings)?;
        store.settings = settings;
    } else if let Some(id) = window.label().strip_prefix("note-") {
        let mut note = store
            .notes
            .iter()
            .find(|note| note.id == id)
            .cloned()
            .ok_or("便签不存在")?;
        note.pinned = pinned;
        storage::save(&store.db, "note", id, &note)?;
        if let Some(existing) = store.notes.iter_mut().find(|note| note.id == id) {
            *existing = note;
        }
    }
    drop(store);
    broadcast(&app)
}

#[tauri::command]
pub async fn set_compact(app: AppHandle, compact: bool) -> AppResult<()> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = sender.send(windows::set_compact(&handle, compact));
    })
    .map_err(|error| error.to_string())?;
    receiver.await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn create_note(app: AppHandle, state: State<'_, AppState>) -> AppResult<Note> {
    let now = Utc::now().timestamp_millis();
    let note = Note {
        id: Uuid::new_v4().to_string(),
        title: "新便签".into(),
        content: String::new(),
        created_at: now,
        updated_at: now,
        revision: 0,
        pinned: false,
        open: false,
        geometry: None,
    };
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    storage::save(&store.db, "note", &note.id, &note)?;
    store.notes.push(note.clone());
    drop(store);
    broadcast(&app)?;
    Ok(note)
}

#[tauri::command]
pub fn save_note(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    title: String,
    content: String,
    revision: u64,
) -> AppResult<Note> {
    if title.chars().count() > 120 || content.len() > 2_000_000 {
        return Err("便签内容过长".into());
    }
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    let index = store
        .notes
        .iter()
        .position(|note| note.id == id)
        .ok_or("便签已删除")?;
    let mut note = store.notes[index].clone();
    if note.revision != revision {
        return Err("便签已在另一个窗口更新，请保留当前内容并重新载入后保存".into());
    }
    note.title = title;
    note.content = content;
    note.updated_at = Utc::now().timestamp_millis();
    note.revision += 1;
    storage::save(&store.db, "note", &id, &note)?;
    store.notes[index] = note.clone();
    drop(store);
    broadcast(&app)?;
    Ok(note)
}

#[tauri::command]
pub fn delete_note(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    storage::remove(&store.db, "note", &id)?;
    store.notes.retain(|note| note.id != id);
    drop(store);
    if let Some(window) = app.get_webview_window(&format!("note-{id}")) {
        let _ = window.destroy();
    }
    broadcast(&app)
}

#[tauri::command]
pub async fn open_note(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    windows::open_note(&app, &id)?;
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    let mut note = store
        .notes
        .iter()
        .find(|note| note.id == id)
        .cloned()
        .ok_or("便签不存在")?;
    note.open = true;
    storage::save(&store.db, "note", &id, &note)?;
    if let Some(existing) = store.notes.iter_mut().find(|note| note.id == id) {
        *existing = note;
    }
    drop(store);
    broadcast(&app)
}

fn media_path(state: &AppState, name: &str) -> AppResult<std::path::PathBuf> {
    if name.is_empty()
        || Path::new(name).components().count() != 1
        || name.contains(['/', '\\', ':'])
    {
        return Err("媒体文件名无效".into());
    }
    let path = state.media.join(name);
    if !path.is_file() {
        return Err("媒体文件不存在，请重新选择".into());
    }
    Ok(path)
}

fn validate_sound(state: &AppState, sound: &Option<String>) -> AppResult<()> {
    if let Some(sound) = sound {
        media_path(state, sound)?;
    }
    Ok(())
}

#[tauri::command]
pub fn import_media(state: State<'_, AppState>, path: String, kind: String) -> AppResult<String> {
    let source = Path::new(&path);
    let metadata = fs::metadata(source).map_err(|_| "文件无法读取")?;
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_lowercase();
    let limit = if kind == "image" {
        10 * 1024 * 1024
    } else {
        30 * 1024 * 1024
    };
    if !metadata.is_file() || metadata.len() > limit {
        return Err("图片需小于 10 MB，音频需小于 30 MB".into());
    }
    match kind.as_str() {
        "image" => {
            if !["png", "jpg", "jpeg", "webp"].contains(&extension.as_str()) {
                return Err("请选择 PNG、JPG 或 WebP 图片".into());
            }
            image::ImageReader::open(source)
                .map_err(|_| "图片读取失败")?
                .with_guessed_format()
                .map_err(|_| "图片格式无效")?
                .decode()
                .map_err(|_| "图片格式无效")?;
        }
        "audio" => {
            if !["wav", "mp3", "ogg", "flac"].contains(&extension.as_str()) {
                return Err("请选择 WAV、MP3、OGG 或 FLAC 音频".into());
            }
            let file = fs::File::open(source).map_err(|_| "音频读取失败")?;
            rodio::Decoder::new(BufReader::new(file)).map_err(|_| "音频格式无效")?;
        }
        _ => return Err("文件类型无效".into()),
    }
    let name = format!("{}.{}", Uuid::new_v4(), extension);
    fs::copy(source, state.media.join(&name)).map_err(|_| "文件导入失败")?;
    Ok(name)
}

#[tauri::command]
pub fn image_data(state: State<'_, AppState>, name: String) -> AppResult<String> {
    let path = media_path(&state, &name)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let mime = match extension {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return Err("图片类型无效".into()),
    };
    let bytes = fs::read(path).map_err(|_| "图片读取失败")?;
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub fn preview_sound(
    state: State<'_, AppState>,
    name: Option<String>,
    volume: f32,
) -> AppResult<()> {
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
        return Err("音量无效".into());
    }
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    if store.reminders.iter().any(|item| item.status == "ringing") {
        return Err("请先处理正在响铃的提醒".into());
    }
    store.playing = None;
    let path = name
        .as_ref()
        .map(|name| media_path(&state, name))
        .transpose()?;
    state
        .audio
        .send(AudioCommand::Play {
            path,
            volume,
            repeat: false,
        })
        .map_err(|_| "声音服务不可用".into())
}

#[tauri::command]
pub fn stop_preview(state: State<'_, AppState>) -> AppResult<()> {
    let store = state.store.lock().map_err(|_| "数据暂不可用")?;
    if !store.reminders.iter().any(|item| item.status == "ringing") {
        let _ = state.audio.send(AudioCommand::Stop);
    }
    Ok(())
}

#[tauri::command]
pub fn save_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    mut profile: Profile,
) -> AppResult<Profile> {
    ai::validate_profile(&profile)?;
    if profile.id.is_empty() {
        profile.id = Uuid::new_v4().to_string();
    }
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    storage::save(&store.db, "profile", &profile.id, &profile)?;
    if let Some(existing) = store.profiles.iter_mut().find(|item| item.id == profile.id) {
        *existing = profile.clone();
    } else {
        store.profiles.push(profile.clone());
    }
    drop(store);
    broadcast(&app)?;
    Ok(profile)
}

#[tauri::command]
pub fn delete_profile(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    ai::delete_key(&id)?;
    let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
    storage::remove(&store.db, "profile", &id)?;
    store.profiles.retain(|profile| profile.id != id);
    drop(store);
    broadcast(&app)
}

#[tauri::command]
pub fn complete_exit(
    app: AppHandle,
    state: State<'_, AppState>,
    window: WebviewWindow,
    success: bool,
) -> AppResult<()> {
    let mut waiting = state.exit_waiting.lock().map_err(|_| "退出状态暂不可用")?;
    let Some(windows) = waiting.as_mut() else {
        return Ok(());
    };
    if !success {
        *waiting = None;
        drop(waiting);
        let _ = window.show();
        let _ = window.set_focus();
        app.emit("app-error", "便签尚未保存，请处理保存失败后再退出")
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    windows.remove(window.label());
    if windows.is_empty() {
        *waiting = None;
        drop(waiting);
        app.exit(0);
    }
    Ok(())
}

#[tauri::command]
pub async fn close_window(
    app: AppHandle,
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> AppResult<()> {
    windows::track(&app, window.label(), window.label() == "main");
    if window.label() != "main" {
        return window.destroy().map_err(|error| error.to_string());
    }
    let first = {
        let mut store = state.store.lock().map_err(|_| "数据暂不可用")?;
        let first = !store.settings.tray_hint_shown;
        store.settings.tray_hint_shown = true;
        storage::save(&store.db, "settings", "main", &store.settings)?;
        first
    };
    if first {
        let _ = app.emit(
            "tray-hint",
            "窗口关闭后，时刻会在系统托盘继续运行。可在托盘菜单中退出。",
        );
        tokio::time::sleep(std::time::Duration::from_millis(1800)).await;
    }
    window.hide().map_err(|error| error.to_string())
}
