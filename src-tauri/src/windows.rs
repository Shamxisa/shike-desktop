use crate::{models::Geometry, storage, AppResult, AppState};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

static SHAPE_SIZE: std::sync::Mutex<Option<(usize, u32, u32, u32)>> = std::sync::Mutex::new(None);

pub fn shape(window: &WebviewWindow, compact: bool) -> AppResult<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};
        let handle = window.hwnd().map_err(|error| error.to_string())?.0;
        let size = window.outer_size().map_err(|error| error.to_string())?;
        let scale = window.scale_factor().map_err(|error| error.to_string())?;
        let diameter = (48.0 * scale).round() as u32;
        let key = compact.then_some((handle as usize, size.width, size.height, diameter));
        {
            let mut cached = SHAPE_SIZE.lock().map_err(|_| "窗口外形暂不可用")?;
            if *cached == key {
                return Ok(());
            }
            *cached = key;
        }
        unsafe {
            let region = if compact {
                let region = CreateRoundRectRgn(
                    0,
                    0,
                    size.width as i32 + 1,
                    size.height as i32 + 1,
                    diameter as i32,
                    diameter as i32,
                );
                if region.is_null() {
                    return Err("创建圆角窗口失败".into());
                }
                region
            } else {
                std::ptr::null_mut()
            };
            if SetWindowRgn(handle, region, 1) == 0 {
                if !region.is_null() {
                    DeleteObject(region);
                }
                return Err("设置窗口外形失败".into());
            }
            // Windows owns a successfully assigned region.
        }
    }
    Ok(())
}

pub fn refresh_shape(window: &WebviewWindow) -> AppResult<()> {
    let active = SHAPE_SIZE.lock().map_err(|_| "窗口外形暂不可用")?.is_some();
    if active {
        shape(window, true)?;
    }
    Ok(())
}

pub fn mode(window: &WebviewWindow, compact: bool, saved: Option<&Geometry>) -> AppResult<()> {
    if window.is_maximized().map_err(|error| error.to_string())? {
        window.unmaximize().map_err(|error| error.to_string())?;
    }
    window
        .set_max_size(None::<tauri::LogicalSize<f64>>)
        .map_err(|error| error.to_string())?;
    window
        .set_min_size(None::<tauri::LogicalSize<f64>>)
        .map_err(|error| error.to_string())?;
    window
        .set_shadow(!compact)
        .map_err(|error| error.to_string())?;
    window
        .set_resizable(true)
        .map_err(|error| error.to_string())?;
    window
        .set_maximizable(!compact)
        .map_err(|error| error.to_string())?;
    if compact {
        let scale = window.scale_factor().map_err(|error| error.to_string())?;
        let size = PhysicalSize::new(
            saved
                .map(|geometry| geometry.width)
                .unwrap_or((388.0 * scale).floor() as u32)
                .max((300.0 * scale).floor() as u32),
            saved
                .map(|geometry| geometry.height)
                .unwrap_or((242.0 * scale).floor() as u32)
                .max((220.0 * scale).floor() as u32),
        );
        window.set_size(size).map_err(|error| error.to_string())?;
        window
            .set_min_size(Some(tauri::LogicalSize::new(300.0, 220.0)))
            .map_err(|error| error.to_string())?;
    } else {
        window
            .set_min_size(Some(tauri::LogicalSize::new(360.0, 420.0)))
            .map_err(|error| error.to_string())?;
    }
    shape(window, compact)
}

pub fn geometry(window: &WebviewWindow) -> Option<Geometry> {
    let position = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    Some(Geometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

pub fn restore(window: &WebviewWindow, geometry: &Geometry) {
    let _ = window.set_size(PhysicalSize::new(
        geometry.width.max(300),
        geometry.height.max(240),
    ));
    let visible = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .any(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            geometry.x + 100 > position.x
                && geometry.y + 60 > position.y
                && geometry.x < position.x + size.width as i32 - 50
                && geometry.y < position.y + size.height as i32 - 50
        });
    if visible {
        let _ = window.set_position(PhysicalPosition::new(geometry.x, geometry.y));
    }
}

pub fn open_note(app: &AppHandle, id: &str) -> AppResult<()> {
    let label = format!("note-{id}");
    if let Some(window) = app.get_webview_window(&label) {
        window.show().map_err(|error| error.to_string())?;
        let _ = window.set_focus();
        return Ok(());
    }
    let note = app
        .state::<AppState>()
        .store
        .lock()
        .map_err(|_| "数据暂不可用")?
        .notes
        .iter()
        .find(|note| note.id == id)
        .cloned()
        .ok_or("便签不存在")?;
    let window = WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::App(format!("index.html?note={id}").into()),
    )
    .title(&note.title)
    .inner_size(380.0, 400.0)
    .min_inner_size(300.0, 240.0)
    .decorations(false)
    .always_on_top(note.pinned)
    .visible(false)
    .build()
    .map_err(|error| error.to_string())?;
    if let Some(geometry) = &note.geometry {
        restore(&window, geometry);
    }
    window.show().map_err(|error| error.to_string())?;
    Ok(())
}

pub fn track(app: &AppHandle, label: &str, closing: bool) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    let geometry = geometry(&window);
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Ok(mut store) = state.store.lock() else {
        return;
    };
    if label == "main" {
        if store.settings.compact {
            store.settings.compact_geometry = geometry.clone();
        }
        store.settings.geometry = geometry;
    } else if let Some(id) = label.strip_prefix("note-") {
        if let Some(note) = store.notes.iter_mut().find(|note| note.id == id) {
            note.geometry = geometry;
            if closing {
                note.open = false;
            }
        }
    }
    store.geometry_dirty = true;
    if closing {
        let result = if label == "main" {
            storage::save(&store.db, "settings", "main", &store.settings)
        } else if let Some(note) = store
            .notes
            .iter()
            .find(|note| format!("note-{}", note.id) == label)
        {
            storage::save(&store.db, "note", &note.id, note)
        } else {
            Ok(())
        };
        if let Err(error) = result {
            let _ = app.emit("app-error", error);
        }
        let snapshot = store.snapshot();
        drop(store);
        let _ = app.emit("state-changed", snapshot);
    }
}

pub fn closed(app: &AppHandle, label: &str) {
    let Some(id) = label.strip_prefix("note-") else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Ok(mut store) = state.store.lock() else {
        return;
    };
    if let Some(note) = store.notes.iter_mut().find(|note| note.id == id) {
        note.open = false;
    }
    if let Some(note) = store.notes.iter().find(|note| note.id == id) {
        if let Err(error) = storage::save(&store.db, "note", id, note) {
            let _ = app.emit("app-error", error);
        }
    }
    let snapshot = store.snapshot();
    drop(store);
    let _ = app.emit("state-changed", snapshot);
}
