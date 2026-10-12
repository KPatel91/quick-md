mod commands;
mod render;
mod tree;
mod watcher;

use std::path::PathBuf;
use std::sync::Mutex;

/// Shared application state: the currently open root folder and its watcher.
#[derive(Default)]
pub struct AppState {
    pub root: Mutex<Option<PathBuf>>,
    pub watcher: Mutex<Option<watcher::Handle>>,
}

pub fn run() {
    // `quick-md --render file.md` prints the rendered HTML and exits. Handy for
    // debugging the renderer without launching a window.
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--render" {
        let path = std::path::Path::new(&args[2]);
        match std::fs::read_to_string(path) {
            Ok(src) => {
                let dir = path.parent().unwrap_or(std::path::Path::new("."));
                println!("{}", render::to_html(&src, dir));
                return;
            }
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                std::process::exit(1);
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                use tauri::Manager;
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window_vibrancy::apply_vibrancy(
                        &window,
                        window_vibrancy::NSVisualEffectMaterial::Sidebar,
                        None,
                        None,
                    );
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::startup,
            commands::open_folder,
            commands::get_tree,
            commands::read_doc,
            commands::search,
        ])
        .run(tauri::generate_context!())
        .expect("error while running quick-md");
}
