# Quick MD

A fast, minimalist Markdown reader for macOS. Point it at a folder and read.

- **Tiny and quick.** Rust backend, the system WebKit for rendering, no bundled browser. The app is a few megabytes and opens instantly.
- **Reads, doesn't edit.** No toolbar, no modes. Just your notes, typeset.
- **Folder-aware.** A collapsible sidebar lists every `.md` file in the folder tree. Files update live when they change on disk.
- **Folder search.** ⌘F searches every file in the folder. Pick a hit and it opens the file at that exact line with the match highlighted.
- **GitHub-flavoured Markdown.** Tables, task lists, footnotes, strikethrough, alerts, autolinks, front matter.
- **Code and diagrams.** Syntax highlighting is done in Rust at render time. Mermaid diagrams render in place.
- **Light and dark.** Follows the system appearance.
- **Reading progress.** A small percentage in the corner shows how far through the document you are.

## Build and run

You need Xcode (or just the Command Line Tools), Rust, and Node.

```bash
# one-time
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
npm install

# run in development
npm run dev

# build the .app
npm run build
```

The built app lands in `src-tauri/target/release/bundle/macos/Quick MD.app`. Drag it to Applications.

No Apple developer account is needed to build and run it on your own Mac. If you share the built app with someone else, they will need to right-click it and choose Open the first time, because it isn't notarised.

## Using it

| Action | Shortcut |
|---|---|
| Open folder | ⌘ O |
| Search the whole folder | ⌘ F |
| Toggle sidebar | ⌘ ⇧ L |
| Next / previous file | ⌥ ↓ / ⌥ ↑ |
| Zoom in / out / reset | ⌘ + / ⌘ − / ⌘ 0 |
| Reload | ⌘ R |
| Clear search highlights | Esc |

Drag the sidebar's right edge to resize it; double-click the edge to reset. You can also drop a folder or a `.md` file onto the window, or launch it from the terminal:

```bash
open -a "Quick MD" --args ~/notes
```

The last folder you opened reopens on launch.

## How it works

```
ui/            static HTML, CSS and JS (no framework, no bundler)
src-tauri/     Rust: folder walking, Markdown rendering, file watching
sample/        fixtures that exercise every supported feature
```

Markdown is parsed by [comrak](https://github.com/kivikakk/comrak) and code is highlighted by [syntect](https://github.com/trishume/syntect), both in Rust. The webview only receives finished HTML. [Mermaid](https://mermaid.js.org) is the one bundled JavaScript library and is loaded only when a document contains a diagram.

To debug the renderer without opening a window:

```bash
./src-tauri/target/debug/quick-md --render sample/guides/code.md
```

## Licence

MIT
