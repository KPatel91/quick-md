# Code

Highlighting is done in Rust at render time; the webview only receives spans.

```rust
use std::collections::HashMap;

/// Count words in `text`.
pub fn word_counts(text: &str) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word).or_insert(0) += 1;
    }
    counts
}
```

```javascript
export async function load(path) {
  const res = await fetch(`/api/${encodeURIComponent(path)}`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json(); // parsed body
}
```

```python
from dataclasses import dataclass

@dataclass
class Point:
    x: float
    y: float

    def norm(self) -> float:
        return (self.x ** 2 + self.y ** 2) ** 0.5
```

```bash
#!/usr/bin/env bash
set -euo pipefail
for f in *.md; do
  echo "rendering $f"
done
```

```json
{ "name": "quick-md", "fast": true, "deps": 0, "tags": ["rust", "tauri"] }
```

```
plain fenced block with no language
```

Indented code also works:

    SELECT id, title FROM docs WHERE title LIKE '%md%';
