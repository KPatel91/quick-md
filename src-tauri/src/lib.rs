mod commands;
mod menu;
mod recents;
mod render;
mod tree;
mod watcher;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

/// Shared application state: the open root folder, its watcher, and the
/// "Open Recent" submenu so it can be rebuilt when the list changes.
#[derive(Default)]
pub struct AppState {
    pub root: Mutex<Option<PathBuf>>,
    pub watcher: Mutex<Option<watcher::Handle>>,
    pub recent_menu: Mutex<Option<tauri::menu::Submenu<tauri::Wry>>>,
    /// Set once the front-end has called `startup` and can receive events.
    pub ready: AtomicBool,
    /// A folder opened by the OS before the front-end was ready.
    pub launch: Mutex<Option<commands::Folder>>,
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
        .on_menu_event(|app, event| menu::handle(app, event.id().as_ref()))
        .setup(|app| {
            menu::install(app.handle())?;
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
        .build(tauri::generate_context!())
        .expect("error while building quick-md")
        .run(|_app, _event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = _event {
                commands::handle_opened(_app, urls);
            }
        });
}
