use crate::{render, tree, watcher, AppState};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

#[derive(Serialize)]
pub struct Folder {
    pub root: PathBuf,
    pub tree: Vec<tree::Node>,
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

#[derive(Serialize, Deserialize)]
struct Persisted {
    last_root: PathBuf,
}

fn config_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("state.json"))
}

fn remember(app: &AppHandle, root: &Path) {
    if let Some(file) = config_file(app) {
        if let Some(dir) = file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string(&Persisted { last_root: root.to_path_buf() }) {
            let _ = fs::write(file, json);
        }
    }
}

fn recall(app: &AppHandle) -> Option<PathBuf> {
    let file = config_file(app)?;
    let json = fs::read_to_string(file).ok()?;
    let p: Persisted = serde_json::from_str(&json).ok()?;
    p.last_root.is_dir().then_some(p.last_root)
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
    remember(app, &root);

    let tree = tree::build(&root);
    Ok(Folder { root, tree })
}

/// Called once by the front-end on load. Opens the folder given on the command
/// line, else the last folder used, else nothing.
#[tauri::command]
pub fn startup(app: AppHandle, state: State<'_, AppState>) -> Result<Option<Folder>, String> {
    let from_args = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .filter(|p| p.is_dir() || (p.is_file() && tree::is_markdown(p)));
    let candidate = from_args.or_else(|| recall(&app));
    let Some(candidate) = candidate else { return Ok(None) };
    let root = if candidate.is_file() {
        candidate.parent().map(Path::to_path_buf).unwrap_or(candidate)
    } else {
        candidate
    };
    activate(&app, &state, root).map(Some)
}

#[tauri::command]
pub fn open_folder(app: AppHandle, state: State<'_, AppState>, path: PathBuf) -> Result<Folder, String> {
    activate(&app, &state, path)
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
