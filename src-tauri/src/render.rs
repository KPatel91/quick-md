//! Markdown -> HTML rendering with GFM extensions, syntax highlighting and
//! Mermaid passthrough. Relative image paths are rewritten to Tauri asset URLs
//! so the webview can load them from disk.

use comrak::adapters::CodefenceRendererAdapter;
use comrak::nodes::Sourcepos;
use comrak::options::URLRewriter;
use comrak::plugins::syntect::{SyntectAdapter, SyntectAdapterBuilder};
use comrak::options::Plugins;
use comrak::{markdown_to_html_with_plugins, Options};
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use std::fmt::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Mirrors JavaScript's `encodeURIComponent`.
const URI_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

/// Convert an absolute file path into the URL Tauri's asset protocol accepts.
pub fn asset_url(path: &Path) -> String {
    let s = path.to_string_lossy();
    format!("asset://localhost/{}", utf8_percent_encode(&s, URI_COMPONENT))
}

fn is_external(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("data:")
        || lower.starts_with("asset:")
        || lower.starts_with("file:")
        || lower.starts_with("//")
}

/// Resolves relative image URLs against the document's directory.
struct ImageRewriter {
    base: PathBuf,
}

impl URLRewriter for ImageRewriter {
    fn to_html(&self, url: &str) -> String {
        if is_external(url) || url.starts_with('#') {
            return url.to_string();
        }
        let decoded = percent_decode_str(url).decode_utf8_lossy();
        let path = Path::new(decoded.as_ref());
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base.join(path)
        };
        asset_url(&normalize(&abs))
    }
}

/// Lexically normalise `.` and `..` components without touching the filesystem.
fn normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Emits ```mermaid blocks as `<pre class="mermaid">` for the front-end to render.
struct MermaidRenderer;

impl CodefenceRendererAdapter for MermaidRenderer {
    fn write(
        &self,
        output: &mut dyn Write,
        _lang: &str,
        _meta: &str,
        code: &str,
        _sourcepos: Option<Sourcepos>,
    ) -> fmt::Result {
        output.write_str("<pre class=\"mermaid\">")?;
        for ch in code.chars() {
            match ch {
                '&' => output.write_str("&amp;")?,
                '<' => output.write_str("&lt;")?,
                '>' => output.write_str("&gt;")?,
                c => output.write_char(c)?,
            }
        }
        output.write_str("</pre>\n")
    }
}

fn highlighter() -> &'static SyntectAdapter {
    static ADAPTER: OnceLock<SyntectAdapter> = OnceLock::new();
    ADAPTER.get_or_init(|| SyntectAdapterBuilder::new().css_with_class_prefix("hl-").build())
}

fn options<'c>(doc_dir: &Path) -> Options<'c> {
    let mut o = Options::default();
    o.extension.table = true;
    o.extension.tasklist = true;
    o.extension.footnotes = true;
    o.extension.strikethrough = true;
    o.extension.autolink = true;
    o.extension.alerts = true;
    o.extension.header_id_prefix = Some(String::new());
    o.extension.front_matter_delimiter = Some("---".into());
    o.extension.image_url_rewriter = Some(std::sync::Arc::new(ImageRewriter {
        base: doc_dir.to_path_buf(),
    }));
    o.parse.smart = true;
    o.parse.relaxed_tasklist_matching = true;
    o.render.r#unsafe = true; // local, trusted files: allow raw HTML
    o.render.github_pre_lang = true;
    o.render.tasklist_classes = true;
    o.render.sourcepos = true; // data-sourcepos lets the UI map search hits to elements
    o
}

/// Render Markdown source to HTML. `doc_dir` is the directory the document
/// lives in, used to resolve relative image paths.
pub fn to_html(markdown: &str, doc_dir: &Path) -> String {
    let opts = options(doc_dir);
    let mermaid = MermaidRenderer;
    let mut plugins = Plugins::default();
    plugins.render.codefence_syntax_highlighter = Some(highlighter());
    plugins.render.codefence_renderers.insert("mermaid".into(), &mermaid);
    markdown_to_html_with_plugins(markdown, &opts, &plugins)
}

/// The first level-one heading, if any.
pub fn first_h1(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        let t = line.trim_start();
        t.strip_prefix("# ")
            .map(|h| h.trim().trim_end_matches('#').trim().to_string())
            .filter(|h| !h.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gfm_extensions_render() {
        let html = to_html("| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n- [ ] todo\n\nText[^1]\n\n[^1]: note\n", Path::new("/tmp"));
        assert!(html.contains("<table"));
        assert!(html.contains("type=\"checkbox\""));
        assert!(html.contains("footnote"));
    }

    #[test]
    fn code_is_highlighted_with_prefixed_classes() {
        let html = to_html("```rust\nfn main() {}\n```\n", Path::new("/tmp"));
        assert!(html.contains("hl-storage hl-type"), "{html}");
        assert!(html.contains("<pre class=\"syntax-highlighting\">"), "{html}");
    }

    #[test]
    fn mermaid_blocks_pass_through() {
        let html = to_html("```mermaid\ngraph TD; A-->B\n```\n", Path::new("/tmp"));
        assert!(html.contains("<pre class=\"mermaid\">graph TD; A--&gt;B"), "{html}");
        assert!(!html.contains("hl-"));
    }

    #[test]
    fn relative_images_become_asset_urls() {
        let html = to_html("![pic](img/a%20b.png)\n![up](../x.png)\n![web](https://e.com/i.png)", Path::new("/docs/sub"));
        assert!(html.contains("src=\"asset://localhost/%2Fdocs%2Fsub%2Fimg%2Fa%20b.png\""), "{html}");
        assert!(html.contains("src=\"asset://localhost/%2Fdocs%2Fx.png\""), "{html}");
        assert!(html.contains("src=\"https://e.com/i.png\""), "{html}");
    }

    #[test]
    fn extracts_first_h1() {
        assert_eq!(first_h1("intro\n\n# Title #\n## sub"), Some("Title".into()));
        assert_eq!(first_h1("## only h2"), None);
    }
}
