//! Walks a folder and produces a nested tree of Markdown files.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 16;
const SKIP_DIRS: &[&str] = &["node_modules", "target", "dist", "build", ".git"];

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Node {
    Dir {
        name: String,
        path: PathBuf,
        children: Vec<Node>,
    },
    File {
        name: String,
        path: PathBuf,
    },
}

impl Node {
    fn name(&self) -> &str {
        match self {
            Node::Dir { name, .. } | Node::File { name, .. } => name,
        }
    }
    fn is_dir(&self) -> bool {
        matches!(self, Node::Dir { .. })
    }
}

pub fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("md") | Some("markdown") | Some("mdown") | Some("mkd")
    )
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// Build the tree rooted at `root`. Directories come first, everything is
/// sorted case-insensitively, and directories containing no Markdown files
/// (at any depth) are pruned.
pub fn build(root: &Path) -> Vec<Node> {
    build_dir(root, 0)
}

fn build_dir(dir: &Path, depth: usize) -> Vec<Node> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut nodes: Vec<Node> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_hidden(&name) {
            continue;
        }
        let path = entry.path();
        // Follow symlinks for type detection, but don't recurse into symlinked dirs
        // to avoid cycles.
        let Ok(meta) = fs::metadata(&path) else { continue };
        if meta.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) || entry.file_type().map(|t| t.is_symlink()).unwrap_or(true) {
                continue;
            }
            let children = build_dir(&path, depth + 1);
            if !children.is_empty() {
                nodes.push(Node::Dir { name, path, children });
            }
        } else if meta.is_file() && is_markdown(&path) {
            nodes.push(Node::File { name, path });
        }
    }

    nodes.sort_by(|a, b| {
        b.is_dir()
            .cmp(&a.is_dir())
            .then_with(|| a.name().to_lowercase().cmp(&b.name().to_lowercase()))
    });
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, write};

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quickmd-tree-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn filters_sorts_and_prunes() {
        let root = tmp();
        write(root.join("zeta.md"), "").unwrap();
        write(root.join("Alpha.MD"), "").unwrap();
        write(root.join("notes.txt"), "").unwrap();
        write(root.join(".hidden.md"), "").unwrap();
        create_dir_all(root.join("docs/deep")).unwrap();
        write(root.join("docs/deep/guide.markdown"), "").unwrap();
        create_dir_all(root.join("empty")).unwrap();
        create_dir_all(root.join("node_modules")).unwrap();
        write(root.join("node_modules/README.md"), "").unwrap();
        create_dir_all(root.join(".git")).unwrap();
        write(root.join(".git/x.md"), "").unwrap();

        let tree = build(&root);
        let names: Vec<&str> = tree.iter().map(|n| n.name()).collect();
        assert_eq!(names, vec!["docs", "Alpha.MD", "zeta.md"]);
        match &tree[0] {
            Node::Dir { children, .. } => {
                assert_eq!(children.len(), 1);
                assert_eq!(children[0].name(), "deep");
            }
            _ => panic!("expected dir"),
        }
        let _ = fs::remove_dir_all(&root);
    }
}
