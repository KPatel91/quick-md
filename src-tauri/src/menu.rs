//! Native menu: adds "Open Folder…" and an "Open Recent" submenu to the File menu.

use crate::{recents, AppState};
use std::path::{Path, PathBuf};
use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager};

const OPEN_FOLDER: &str = "open-folder";
const CLEAR_RECENTS: &str = "clear-recents";
// A recent item's id carries its path, so a click always opens the folder that
// was shown even if the list changed in the meantime.
const RECENT_PREFIX: &str = "recent:";

/// Build the app menu (the platform default plus our File entries) and install it.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::default(app)?;
    let open = MenuItem::with_id(app, OPEN_FOLDER, "Open Folder…", true, Some("CmdOrCtrl+O"))?;
    let recent = Submenu::with_id(app, "open-recent", "Open Recent", true)?;
    let sep = PredefinedMenuItem::separator(app)?;

    for item in menu.items()? {
        if let MenuItemKind::Submenu(sub) = &item {
            if sub.text()? == "File" {
                sub.insert_items(&[&open, &recent, &sep], 0)?;
            }
        }
    }
    app.set_menu(menu)?;
    *app.state::<AppState>().recent_menu.lock().unwrap() = Some(recent);
    refresh(app);
    Ok(())
}

/// Rebuild the "Open Recent" entries from the saved list.
pub fn refresh(app: &AppHandle) {
    let state = app.state::<AppState>();
    let guard = state.recent_menu.lock().unwrap();
    let Some(sub) = guard.as_ref() else { return };

    if let Ok(items) = sub.items() {
        for item in items {
            let _ = sub.remove(&item);
        }
    }

    let list = recents::recent(app);
    if list.is_empty() {
        if let Ok(none) = MenuItem::with_id(app, "no-recents", "No Recent Folders", false, None::<&str>) {
            let _ = sub.append(&none);
        }
        return;
    }
    for path in &list {
        let id = format!("{RECENT_PREFIX}{}", path.display());
        if let Ok(item) = MenuItem::with_id(app, id, label_for(path, &list), true, None::<&str>) {
            let _ = sub.append(&item);
        }
    }
    if let Ok(sep) = PredefinedMenuItem::separator(app) {
        let _ = sub.append(&sep);
    }
    if let Ok(clear) = MenuItem::with_id(app, CLEAR_RECENTS, "Clear Menu", true, None::<&str>) {
        let _ = sub.append(&clear);
    }
}

/// Route a menu click to the front-end (or handle it here).
pub fn handle(app: &AppHandle, id: &str) {
    if id == OPEN_FOLDER {
        let _ = app.emit("menu-open-folder", ());
    } else if id == CLEAR_RECENTS {
        recents::clear(app);
        refresh(app);
    } else if let Some(path) = id.strip_prefix(RECENT_PREFIX) {
        let _ = app.emit("menu-open-recent", path);
    }
}

/// The folder's name, plus its parent when two recents share a name.
fn label_for(path: &Path, all: &[PathBuf]) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let ambiguous = all.iter().filter(|p| p.file_name() == path.file_name()).count() > 1;
    match path.parent() {
        Some(parent) if ambiguous => format!("{name} — {}", tilde(parent)),
        _ => name,
    }
}

fn tilde(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_disambiguated_only_when_needed() {
        let all = vec![
            PathBuf::from("/a/work/notes"),
            PathBuf::from("/b/home/notes"),
            PathBuf::from("/c/docs"),
        ];
        assert_eq!(label_for(&all[0], &all), "notes — /a/work");
        assert_eq!(label_for(&all[1], &all), "notes — /b/home");
        assert_eq!(label_for(&all[2], &all), "docs");
    }
}
