# Tables and extras

## Table

| Feature            | Status | Notes                         |
|--------------------|:------:|-------------------------------|
| Tables             |   ✅   | GFM pipes with alignment      |
| Task lists         |   ✅   | Checkboxes are read-only      |
| Footnotes          |   ✅   | Rendered at the bottom        |
| Math               |   ❌   | Deliberately out of scope     |

## Task list

- [x] Open a folder
- [x] Render Markdown
- [ ] Write the next note
- [ ] Ship it

## Footnotes

Quick MD renders Markdown with comrak[^comrak] and highlights code with syntect[^syntect].

[^comrak]: A CommonMark + GFM parser written in Rust.
[^syntect]: Sublime Text syntax definitions, also in Rust.

## Alerts

> [!NOTE]
> Alerts use the GitHub syntax.

> [!TIP]
> Press ⌥↓ and ⌥↑ to move between files.

> [!WARNING]
> Editing is out of scope. This is a reader.
