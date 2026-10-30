use curator_core::app::{App, Snapshot};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Emitter, Manager};

struct Runtime {
    app: Arc<Mutex<App>>,
    busy: Arc<AtomicBool>,
}
#[tauri::command]
async fn discover_columns(library: String) -> Result<Vec<curator_core::model::Column>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        curator_core::library::discover(&library).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
struct Busy(Arc<AtomicBool>);
impl Drop for Busy {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
async fn dispatch(
    state: tauri::State<'_, Runtime>,
    action: String,
    payload: Value,
) -> Result<Snapshot, String> {
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("An operation is already running".into());
    }
    let busy = Busy(state.busy.clone());
    let shared = state.app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _busy = busy;
        let mut app = shared
            .lock()
            .map_err(|_| "Application state lock failed".to_string())?;
        let mut execute = || -> Result<Snapshot, String> {
            let string = |key: &str| {
                payload
                    .get(key)
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("Missing {key}"))
            };
            match action.as_str() {
                "snapshot" => {}
                "ai_settings" => app
                    .configure_ai(
                        serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?,
                "ai_models" => app.discover_ai_models().map_err(|e| e.to_string())?,
                "ai_extract" => app
                    .extract_ai(
                        string("source_id")?,
                        payload["start"].as_u64().ok_or("Missing segment start")? as usize,
                        payload["end"].as_u64().ok_or("Missing segment end")? as usize,
                        payload["universe"].as_str().unwrap_or(""),
                        payload["refresh"].as_bool().unwrap_or(false),
                    )
                    .map_err(|e| e.to_string())?,
                "ai_decide" => app
                    .decide_ai(
                        string("run_id")?,
                        payload["index"].as_u64().ok_or("Missing candidate index")? as usize,
                        string("status")?,
                    )
                    .map_err(|e| e.to_string())?,
                "ai_draft" => app
                    .materialize_ai(
                        string("run_id")?,
                        payload["index"].as_u64().ok_or("Missing candidate index")? as usize,
                        payload["book_id"].as_i64(),
                    )
                    .map_err(|e| e.to_string())?,
                "ai_clear_cache" => app.clear_ai_cache().map_err(|e| e.to_string())?,
                "scan" => app.refresh().map_err(|e| e.to_string())?,
                "settings" => app
                    .settings(serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
                "import" => {
                    app.import(
                        string("location")?,
                        payload["universe"].as_str().unwrap_or(""),
                        payload["remote"].as_bool().unwrap_or(false),
                    )
                    .map_err(|e| e.to_string())?;
                }
                "seed" => {
                    app.seed(Path::new(string("location")?))
                        .map_err(|e| e.to_string())?;
                }
                "save_reference" => app
                    .save_reference(
                        serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?,
                "delete_reference" => app
                    .delete_reference(string("id")?)
                    .map_err(|e| e.to_string())?,
                "pin" => app
                    .pin(
                        string("uuid")?,
                        serde_json::from_value(payload["mask"].clone())
                            .map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?,
                "aliases" => app
                    .aliases(serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
                "sequence" => {
                    app.sequence(string("universe")?)
                        .map_err(|e| e.to_string())?;
                }
                "apply" => {
                    app.apply(serde_json::from_value(payload.clone()).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                }
                "reverse" => {
                    app.reverse(string("id")?).map_err(|e| e.to_string())?;
                }
                "dismiss" => app.dismiss(string("id")?).map_err(|e| e.to_string())?,
                "export" => app
                    .export(Path::new(string("location")?))
                    .map_err(|e| e.to_string())?,
                _ => return Err("Unknown command".into()),
            }
            app.snapshot().map_err(|e| e.to_string())
        };
        execute()
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|application| {
            let root = std::env::var_os("CURATOR_WORKSPACE")
                .map(PathBuf::from)
                .unwrap_or(application.path().app_data_dir()?);
            let bridge = application
                .path()
                .resource_dir()?
                .join("resources/bridge.py");
            let mut core = App::open(&root, bridge)?;
            let handle = application.handle().clone();
            core.progress = Some(Box::new(move |update| {
                let _ = handle.emit("curator-progress", update);
            }));
            if let Some(library) = std::env::var_os("CURATOR_LIBRARY") {
                core.data.settings.library = library.to_string_lossy().into();
            }
            application.manage(Runtime {
                app: Arc::new(Mutex::new(core)),
                busy: Arc::new(AtomicBool::new(false)),
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.state::<Runtime>().busy.load(Ordering::SeqCst) {
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![dispatch, discover_columns])
        .run(tauri::generate_context!())
        .expect("Unable to start Calibre Curator");
}
