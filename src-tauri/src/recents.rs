//! Persisted app state: the last opened folder and a most-recent-first list.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const MAX_RECENTS: usize = 10;

#[derive(Serialize, Deserialize, Default)]
struct Persisted {
    #[serde(default)]
    last_root: Option<PathBuf>,
    #[serde(default)]
    recents: Vec<PathBuf>,
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("state.json"))
}

fn load(app: &AppHandle) -> Persisted {
    file(app)
        .and_then(|f| fs::read_to_string(f).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, state: &Persisted) {
    let Some(f) = file(app) else { return };
    if let Some(dir) = f.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(state) {
        let _ = fs::write(f, json);
    }
}

/// Move `root` to the front of `list`, dropping any earlier copy and capping the length.
pub fn push(list: &mut Vec<PathBuf>, root: &Path) {
    list.retain(|p| p != root);
    list.insert(0, root.to_path_buf());
    list.truncate(MAX_RECENTS);
}

/// Record `root` as the last-opened folder and the newest recent.
pub fn remember(app: &AppHandle, root: &Path) {
    let mut state = load(app);
    push(&mut state.recents, root);
    state.last_root = Some(root.to_path_buf());
    save(app, &state);
}

/// Drop a folder that can no longer be opened.
pub fn forget(app: &AppHandle, root: &Path) {
    let mut state = load(app);
    state.recents.retain(|p| p != root);
    if state.last_root.as_deref() == Some(root) {
        state.last_root = None;
    }
    save(app, &state);
}

/// Empty the recents list. The last-opened folder still reopens on launch.
pub fn clear(app: &AppHandle) {
    let mut state = load(app);
    state.recents.clear();
    save(app, &state);
}

/// Recent folders that still exist, newest first.
pub fn recent(app: &AppHandle) -> Vec<PathBuf> {
    load(app).recents.into_iter().filter(|p| p.is_dir()).collect()
}

/// The folder to reopen on launch, if it still exists.
pub fn last(app: &AppHandle) -> Option<PathBuf> {
    load(app).last_root.filter(|p| p.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_dedupes_orders_and_caps() {
        let mut list: Vec<PathBuf> = Vec::new();
        for i in 0..12 {
            push(&mut list, Path::new(&format!("/notes/{i}")));
        }
        assert_eq!(list.len(), MAX_RECENTS);
        assert_eq!(list[0], Path::new("/notes/11"));

        push(&mut list, Path::new("/notes/5"));
        assert_eq!(list[0], Path::new("/notes/5"));
        assert_eq!(list.iter().filter(|p| **p == Path::new("/notes/5")).count(), 1);
        assert_eq!(list.len(), MAX_RECENTS);
    }

    #[test]
    fn old_state_files_still_load() {
        let p: Persisted = serde_json::from_str(r#"{"last_root":"/a/b"}"#).unwrap();
        assert_eq!(p.last_root, Some(PathBuf::from("/a/b")));
        assert!(p.recents.is_empty());
    }
}
