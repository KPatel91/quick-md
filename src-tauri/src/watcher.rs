//! Debounced filesystem watcher that forwards change events to the webview.

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub type Handle = Debouncer<RecommendedWatcher>;

#[derive(Serialize, Clone)]
struct ChangePayload {
    paths: Vec<PathBuf>,
}

pub fn watch(app: AppHandle, root: &Path) -> Result<Handle, String> {
    let mut debouncer = new_debouncer(Duration::from_millis(250), move |res: DebounceEventResult| {
        if let Ok(events) = res {
            let mut paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
            paths.sort();
            paths.dedup();
            let _ = app.emit("fs-changed", ChangePayload { paths });
        }
    })
    .map_err(|e| e.to_string())?;
    debouncer
        .watcher()
        .watch(root, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;
    Ok(debouncer)
}
