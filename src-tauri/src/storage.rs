use crate::{models::*, AppResult};
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Serialize};
use std::{fs, path::Path};

pub fn save<T: Serialize>(db: &Connection, kind: &str, id: &str, value: &T) -> AppResult<()> {
    let json = serde_json::to_string(value).map_err(|error| error.to_string())?;
    db.execute("INSERT INTO records(kind,id,payload) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET payload=excluded.payload", params![kind,id,json])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn remove(db: &Connection, kind: &str, id: &str) -> AppResult<()> {
    db.execute(
        "DELETE FROM records WHERE kind=?1 AND id=?2",
        params![kind, id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn load<T: DeserializeOwned>(db: &Connection, kind: &str) -> AppResult<Vec<T>> {
    let mut statement = db
        .prepare("SELECT payload FROM records WHERE kind=?1 ORDER BY rowid")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([kind], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let json = row.map_err(|error| error.to_string())?;
        serde_json::from_str(&json).map_err(|error| format!("读取 {kind} 数据失败，{error}"))
    })
    .collect()
}

pub struct Store {
    pub db: Connection,
    pub alarms: Vec<Alarm>,
    pub notes: Vec<Note>,
    pub profiles: Vec<Profile>,
    pub reminders: Vec<Reminder>,
    pub timer: Timer,
    pub settings: Settings,
    pub timer_anchor: Option<(u64, u64)>,
    pub last_tick: u64,
    pub last_wall: i64,
    pub last_offset: i32,
    pub last_checkpoint: u64,
    pub playing: Option<String>,
    pub geometry_dirty: bool,
}

impl Store {
    pub fn open(path: &Path) -> AppResult<Self> {
        fs::create_dir_all(path).map_err(|error| error.to_string())?;
        let db = Connection::open(path.join("shike.sqlite3")).map_err(|error| error.to_string())?;
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(kind,id));
            CREATE TABLE IF NOT EXISTS occurrences(task_id TEXT NOT NULL,due_at INTEGER NOT NULL,PRIMARY KEY(task_id,due_at));
            PRAGMA user_version=1;").map_err(|error| error.to_string())?;
        let alarms = load(&db, "alarm")?;
        let notes = load(&db, "note")?;
        let profiles = load(&db, "profile")?;
        let reminders = load(&db, "reminder")?;
        let timer = load::<Timer>(&db, "timer")?
            .into_iter()
            .next()
            .unwrap_or_default();
        let settings = load::<Settings>(&db, "settings")?
            .into_iter()
            .next()
            .unwrap_or_default();
        let tick = crate::scheduler::uptime_ms();
        let now = chrono::Utc::now().timestamp_millis();
        let mut store = Self {
            db,
            alarms,
            notes,
            profiles,
            reminders,
            timer,
            settings,
            timer_anchor: None,
            last_tick: tick,
            last_wall: now,
            last_checkpoint: tick,
            last_offset: chrono::Local::now().offset().local_minus_utc(),
            playing: None,
            geometry_dirty: false,
        };
        if store.timer.status == "running" {
            let remaining = store.timer.end_at.unwrap_or(now).saturating_sub(now).max(0) as u64;
            store.timer.remaining_ms = remaining;
            store.timer_anchor = Some((tick, remaining));
        }
        Ok(store)
    }

    pub fn timer_snapshot(&self) -> Timer {
        self.timer
            .sample(self.timer_anchor, crate::scheduler::uptime_ms())
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            alarms: self.alarms.clone(),
            timer: self.timer_snapshot(),
            notes: self.notes.clone(),
            profiles: self.profiles.clone(),
            settings: self.settings.clone(),
            reminders: self.reminders.clone(),
        }
    }
}
