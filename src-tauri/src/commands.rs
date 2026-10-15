use crate::{menu, recents, render, tree, watcher, AppState};
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::Emitter;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

#[derive(Serialize, Clone)]
pub struct Folder {
    pub root: PathBuf,
    pub tree: Vec<tree::Node>,
    /// A specific document to show first (set when a file was opened directly).
    pub open_file: Option<PathBuf>,
}

#[derive(Serialize)]
pub struct Hit {
    pub path: PathBuf,
    pub line: usize,
    pub text: String,
}

#[derive(Serialize)]
pub struct Doc {
    pub html: String,
    pub title: String,
}

/// Set `path` as the active root: scope the asset protocol to it, start
/// watching it, remember it, and return its tree.
fn activate(app: &AppHandle, state: &AppState, path: PathBuf) -> Result<Folder, String> {
    let root = fs::canonicalize(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !root.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    app.asset_protocol_scope()
        .allow_directory(&root, true)
        .map_err(|e| e.to_string())?;

    let handle = watcher::watch(app.clone(), &root)?;
    *state.watcher.lock().unwrap() = Some(handle);
    *state.root.lock().unwrap() = Some(root.clone());
    recents::remember(app, &root);
    menu::refresh(app);

    let tree = tree::build(&root);
    Ok(Folder { root, tree, open_file: None })
}

/// Open a folder, or a Markdown file via its parent folder with the file selected.
fn open_target(app: &AppHandle, state: &AppState, target: PathBuf) -> Result<Folder, String> {
    if target.is_file() && tree::is_markdown(&target) {
        let file = fs::canonicalize(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        let parent = file.parent().ok_or("file has no parent folder")?.to_path_buf();
        let mut folder = activate(app, state, parent)?;
        folder.open_file = Some(file);
        Ok(folder)
    } else if target.is_dir() {
        activate(app, state, target)
    } else {
        Err(format!("{} is not a folder or Markdown file", target.display()))
    }
}

/// macOS "open" events: files or folders dropped on the Dock icon, or opened
/// from Finder. If the page is already running, tell it; otherwise park the
/// result for `startup` to pick up.
#[cfg(target_os = "macos")]
pub fn handle_opened(app: &AppHandle, urls: Vec<tauri::Url>) {
    let state = app.state::<AppState>();
    let Some(path) = urls.iter().find_map(|u| u.to_file_path().ok()) else { return };
    match open_target(app, &state, path) {
        Ok(folder) => {
            if state.ready.load(Ordering::SeqCst) {
                let _ = app.emit("open-external", &folder);
            } else {
                *state.launch.lock().unwrap() = Some(folder);
            }
        }
        Err(e) => {
            let _ = app.emit("open-error", e);
        }
    }
}

/// Called once by the front-end on load. Shows whatever the app was launched
/// with (Dock drop, Finder, command line), else the last folder used.
#[tauri::command]
pub fn startup(app: AppHandle, state: State<'_, AppState>) -> Result<Option<Folder>, String> {
    state.ready.store(true, Ordering::SeqCst);
    if let Some(folder) = state.launch.lock().unwrap().take() {
        return Ok(Some(folder));
    }
    let from_args = std::env::args_os().nth(1).map(PathBuf::from);
    if let Some(target) = from_args.filter(|p| p.is_dir() || tree::is_markdown(p)) {
        return open_target(&app, &state, target).map(Some);
    }
    match recents::last(&app) {
        Some(root) => activate(&app, &state, root).map(Some),
        None => Ok(None),
    }
}

#[tauri::command]
pub fn open_folder(app: AppHandle, state: State<'_, AppState>, path: PathBuf) -> Result<Folder, String> {
    let result = activate(&app, &state, path.clone());
    if result.is_err() {
        // A recent folder that was moved or deleted: drop it from the menu.
        recents::forget(&app, &path);
        menu::refresh(&app);
    }
    result
}

#[tauri::command]
pub fn get_tree(state: State<'_, AppState>) -> Result<Vec<tree::Node>, String> {
    let root = state.root.lock().unwrap().clone().ok_or("no folder open")?;
    Ok(tree::build(&root))
}

#[tauri::command]
pub fn read_doc(state: State<'_, AppState>, path: PathBuf) -> Result<Doc, String> {
    let root = state.root.lock().unwrap().clone().ok_or("no folder open")?;
    let abs = fs::canonicalize(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !abs.starts_with(&root) {
        return Err("path is outside the open folder".into());
    }
    let source = fs::read_to_string(&abs).map_err(|e| e.to_string())?;
    let dir = abs.parent().unwrap_or(&root);
    let title = render::first_h1(&source)
        .or_else(|| abs.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();
    Ok(Doc { html: render::to_html(&source, dir), title })
}

const MAX_HITS: usize = 400;
const MAX_HITS_PER_FILE: usize = 40;
const SNIPPET_LEN: usize = 160;

fn flatten(nodes: &[tree::Node], out: &mut Vec<PathBuf>) {
    for n in nodes {
        match n {
            tree::Node::Dir { children, .. } => flatten(children, out),
            tree::Node::File { path, .. } => out.push(path.clone()),
        }
    }
}

/// Case-insensitive substring search across every Markdown file in the open
/// folder. Returns one hit per matching line, in tree order. Runs off the main
/// thread so a big folder never blocks the UI.
#[tauri::command]
pub async fn search(state: State<'_, AppState>, query: String) -> Result<Vec<Hit>, String> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let root = state.root.lock().unwrap().clone().ok_or("no folder open")?;
    let mut files = Vec::new();
    flatten(&tree::build(&root), &mut files);

    let mut hits = Vec::new();
    'files: for path in files {
        let Ok(source) = fs::read_to_string(&path) else { continue };
        let mut in_file = 0;
        for (i, line) in source.lines().enumerate() {
            let lower = line.to_lowercase();
            if let Some(pos) = lower.find(&needle) {
                hits.push(Hit { path: path.clone(), line: i + 1, text: snippet(line, pos, needle.len()) });
                in_file += 1;
                if hits.len() >= MAX_HITS {
                    break 'files;
                }
                if in_file >= MAX_HITS_PER_FILE {
                    break;
                }
            }
        }
    }
    Ok(hits)
}

/// A window of the line around the match, trimmed and char-boundary safe.
fn snippet(line: &str, byte_pos: usize, needle_len: usize) -> String {
    let trimmed = line.trim();
    if trimmed.chars().count() <= SNIPPET_LEN {
        return trimmed.to_string();
    }
    let lead = line.len() - line.trim_start().len();
    let pos = byte_pos.saturating_sub(lead);
    let chars: Vec<char> = trimmed.chars().collect();
    let char_pos = trimmed[..pos.min(trimmed.len())].chars().count();
    let start = char_pos.saturating_sub(SNIPPET_LEN / 3);
    let end = (start + SNIPPET_LEN).min(chars.len());
    let mut out = String::new();
    if start > 0 { out.push('…'); }
    out.extend(&chars[start..end]);
    if end < chars.len() { out.push('…'); }
    let _ = needle_len;
    out
}
