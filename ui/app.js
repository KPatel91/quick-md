// Quick MD front-end. No framework, no bundler: Tauri injects window.__TAURI__.
(() => {
  const { core, event, dialog, opener } = window.__TAURI__;
  const invoke = core.invoke;
  const win = window.__TAURI__.window.getCurrentWindow();

  const $ = (id) => document.getElementById(id);
  const els = {
    body: document.body,
    tree: $("tree"),
    folderName: $("folder-name"),
    reader: $("reader"),
    content: $("content"),
    empty: $("empty"),
  };

  const state = {
    root: null,          // absolute path of the open folder
    tree: [],            // nested nodes from Rust
    files: [],           // flattened file paths in display order
    current: null,       // absolute path of the open document
    collapsed: new Set(),// dir paths that are collapsed
    mermaidReady: null,  // Promise once mermaid.min.js is loaded
  };

  const prefs = {
    get(key, fallback) {
      try { const v = localStorage.getItem(key); return v === null ? fallback : JSON.parse(v); }
      catch { return fallback; }
    },
    set(key, value) { try { localStorage.setItem(key, JSON.stringify(value)); } catch {} },
  };

  // ---------- helpers ----------
  const dirname = (p) => p.slice(0, Math.max(0, p.lastIndexOf("/")));
  const basename = (p) => p.slice(p.lastIndexOf("/") + 1);
  const isMarkdown = (p) => /\.(md|markdown|mdown|mkd)$/i.test(p);

  function resolvePath(base, rel) {
    const parts = (rel.startsWith("/") ? rel : base + "/" + rel).split("/");
    const out = [];
    for (const part of parts) {
      if (part === "" || part === ".") continue;
      if (part === "..") out.pop();
      else out.push(part);
    }
    return "/" + out.join("/");
  }

  let toastTimer;
  function toast(msg) {
    let t = $("toast");
    if (!t) { t = document.createElement("div"); t.id = "toast"; document.body.appendChild(t); }
    t.textContent = msg;
    t.classList.add("show");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => t.classList.remove("show"), 2200);
  }

  const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
  const isDark = () => darkQuery.matches;

  // ---------- sidebar tree ----------
  function flatten(nodes, out = []) {
    for (const n of nodes) {
      if (n.kind === "dir") flatten(n.children, out);
      else out.push(n.path);
    }
    return out;
  }

  function renderTree() {
    els.tree.innerHTML = "";
    els.tree.appendChild(buildList(state.tree, 0));
    state.files = flatten(state.tree);
    highlightCurrent();
  }

  function buildList(nodes, depth) {
    const ul = document.createElement("ul");
    for (const n of nodes) {
      const li = document.createElement("li");
      const row = document.createElement("div");
      row.className = "row " + n.kind;
      row.style.setProperty("--depth", depth);
      row.dataset.path = n.path;
      if (n.kind === "dir") {
        row.innerHTML = `<svg class="chev" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 4.5l3 3 3-3"/></svg>`;
        if (state.collapsed.has(n.path)) li.classList.add("collapsed");
        row.addEventListener("click", () => {
          li.classList.toggle("collapsed");
          if (li.classList.contains("collapsed")) state.collapsed.add(n.path);
          else state.collapsed.delete(n.path);
          prefs.set("collapsed:" + state.root, [...state.collapsed]);
        });
      } else {
        row.addEventListener("click", () => openDoc(n.path));
      }
      const label = document.createElement("span");
      label.className = "label";
      label.textContent = n.kind === "file" ? n.name.replace(/\.(md|markdown|mdown|mkd)$/i, "") : n.name;
      label.title = n.name;
      row.appendChild(label);
      li.appendChild(row);
      if (n.kind === "dir") li.appendChild(buildList(n.children, depth + 1));
      ul.appendChild(li);
    }
    return ul;
  }

  function highlightCurrent() {
    for (const r of els.tree.querySelectorAll(".row.selected")) r.classList.remove("selected");
    if (!state.current) return;
    const row = els.tree.querySelector(`.row.file[data-path="${CSS.escape(state.current)}"]`);
    if (row) {
      row.classList.add("selected");
      // expand ancestors
      let li = row.closest("li");
      while (li) {
        li.classList.remove("collapsed");
        li = li.parentElement?.closest("li");
      }
      row.scrollIntoView({ block: "nearest" });
    }
  }

  // ---------- folder ----------
  function setFolder(folder) {
    state.root = folder.root;
    state.tree = folder.tree;
    state.collapsed = new Set(prefs.get("collapsed:" + state.root, []));
    els.folderName.textContent = basename(folder.root) || folder.root;
    els.folderName.title = folder.root;
    renderTree();
    prefs.set("lastRoot", state.root);
  }

  async function pickFolder() {
    const dir = await dialog.open({ directory: true, multiple: false, title: "Open Folder" });
    if (!dir) return;
    try {
      const folder = await invoke("open_folder", { path: dir });
      setFolder(folder);
      await openInitialDoc();
    } catch (e) {
      toast(String(e));
    }
  }

  async function refreshTree() {
    if (!state.root) return;
    try {
      state.tree = await invoke("get_tree");
      renderTree();
    } catch (e) {
      console.error(e);
    }
  }

  async function openInitialDoc() {
    if (!state.files.length) { showEmptyDoc("No Markdown files in this folder."); return; }
    const remembered = prefs.get("lastDoc:" + state.root, null);
    const preferred =
      (remembered && state.files.includes(remembered) && remembered) ||
      state.files.find((p) => /^(readme|index)\.(md|markdown)$/i.test(basename(p)) && dirname(p) === state.root) ||
      state.files[0];
    await openDoc(preferred);
  }

  // ---------- document ----------
  function showEmptyDoc(message) {
    state.current = null;
    els.content.hidden = true;
    els.empty.hidden = false;
    els.empty.querySelector("p").textContent = message || "Open a folder of Markdown files to start reading.";
    highlightCurrent();
  }

  async function openDoc(path, { keepScroll = false, anchor = null, line = null, query = null } = {}) {
    const scrollTop = els.reader.scrollTop;
    let doc;
    try {
      doc = await invoke("read_doc", { path });
    } catch (e) {
      toast(String(e));
      return;
    }
    const changed = state.current !== path;
    state.current = path;
    prefs.set("lastDoc:" + state.root, path);
    els.empty.hidden = true;
    els.content.hidden = false;
    els.content.innerHTML = doc.html;
    win.setTitle(doc.title || basename(path)).catch(() => {});
    highlightCurrent();
    renderMermaid();

    if (line) {
      jumpToLine(line, query);
    } else if (anchor) {
      scrollToAnchor(anchor);
    } else if (keepScroll && !changed) {
      els.reader.scrollTop = scrollTop;
    } else {
      els.reader.scrollTop = 0;
    }
  }

  function scrollToAnchor(id) {
    const target = document.getElementById(id) || document.getElementById(decodeURIComponent(id));
    if (target) {
      const top = target.getBoundingClientRect().top - els.reader.getBoundingClientRect().top + els.reader.scrollTop - 64;
      els.reader.scrollTo({ top, behavior: "smooth" });
    }
  }

  function stepDoc(delta) {
    if (!state.files.length) return;
    const i = state.files.indexOf(state.current);
    const next = i === -1 ? 0 : Math.min(state.files.length - 1, Math.max(0, i + delta));
    if (state.files[next] !== state.current) openDoc(state.files[next]);
  }

  // ---------- mermaid ----------
  function loadMermaid() {
    if (!state.mermaidReady) {
      state.mermaidReady = new Promise((resolve, reject) => {
        const s = document.createElement("script");
        s.src = "vendor/mermaid.min.js";
        s.onload = () => resolve(window.mermaid);
        s.onerror = () => reject(new Error("Failed to load Mermaid"));
        document.head.appendChild(s);
      });
    }
    return state.mermaidReady;
  }

  async function renderMermaid() {
    const nodes = els.content.querySelectorAll("pre.mermaid");
    if (!nodes.length) return;
    try {
      const mermaid = await loadMermaid();
      mermaid.initialize({
        startOnLoad: false,
        theme: isDark() ? "dark" : "neutral",
        fontFamily: "-apple-system, BlinkMacSystemFont, 'SF Pro Text', sans-serif",
      });
      await mermaid.run({ nodes: [...nodes] });
    } catch (e) {
      console.error(e);
    }
  }

  darkQuery.addEventListener("change", () => {
    if (state.current && els.content.querySelector("pre.mermaid")) {
      openDoc(state.current, { keepScroll: true });
    }
  });

  // ---------- links ----------
  els.content.addEventListener("click", (e) => {
    const a = e.target.closest("a[href]");
    if (!a) return;
    const href = a.getAttribute("href");
    if (!href) return;
    e.preventDefault();

    if (href.startsWith("#")) { scrollToAnchor(href.slice(1)); return; }
    if (/^(https?:|mailto:|tel:)/i.test(href)) { opener.openUrl(href).catch((err) => toast(String(err))); return; }
    if (/^[a-z][a-z0-9+.-]*:/i.test(href)) return; // other schemes: ignore

    const [pathPart, anchor] = href.split("#");
    const target = resolvePath(dirname(state.current || state.root), decodeURIComponent(pathPart));
    if (isMarkdown(target)) {
      openDoc(target, { anchor: anchor || null });
    } else {
      opener.openPath(target).catch((err) => toast(String(err)));
    }
  });

  // ---------- zoom ----------
  function setZoom(z) {
    z = Math.min(2, Math.max(0.7, Math.round(z * 20) / 20));
    document.documentElement.style.setProperty("--zoom", z);
    prefs.set("zoom", z);
  }
  const getZoom = () => parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--zoom")) || 1;

  // ---------- sidebar toggle ----------
  function setSidebar(visible) {
    els.body.classList.toggle("sidebar-hidden", !visible);
    prefs.set("sidebar", visible);
  }

  // ---------- search highlights ----------
  function clearHighlights() {
    for (const m of els.content.querySelectorAll("mark.hit")) {
      const parent = m.parentNode;
      parent.replaceChild(document.createTextNode(m.textContent), m);
      parent.normalize();
    }
  }

  // Wrap every case-insensitive occurrence of `query` inside `scope` in <mark class="hit">.
  function highlightMatches(scope, query) {
    if (!query) return [];
    const needle = query.toLowerCase();
    const walker = document.createTreeWalker(scope, NodeFilter.SHOW_TEXT, {
      acceptNode: (n) => {
        const p = n.parentElement;
        if (!p || p.closest("pre.mermaid, svg, mark.hit, script, style")) return NodeFilter.FILTER_REJECT;
        return n.nodeValue.toLowerCase().includes(needle) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
      },
    });
    const nodes = [];
    while (walker.nextNode()) nodes.push(walker.currentNode);
    const marks = [];
    for (const node of nodes) {
      const text = node.nodeValue;
      const lower = text.toLowerCase();
      const frag = document.createDocumentFragment();
      let last = 0, idx;
      while ((idx = lower.indexOf(needle, last)) !== -1) {
        frag.appendChild(document.createTextNode(text.slice(last, idx)));
        const mark = document.createElement("mark");
        mark.className = "hit";
        mark.textContent = text.slice(idx, idx + needle.length);
        frag.appendChild(mark);
        marks.push(mark);
        last = idx + needle.length;
      }
      frag.appendChild(document.createTextNode(text.slice(last)));
      node.parentNode.replaceChild(frag, node);
    }
    return marks;
  }

  // Find the innermost rendered element whose source range covers `line`
  // (comrak emits data-sourcepos="startLine:startCol-endLine:endCol").
  function elementAtLine(line) {
    let best = null, bestSpan = Infinity;
    for (const el of els.content.querySelectorAll("[data-sourcepos]")) {
      const m = /^(\d+):\d+-(\d+):\d+$/.exec(el.dataset.sourcepos);
      if (!m) continue;
      const start = +m[1], end = +m[2];
      if (line >= start && line <= end && end - start < bestSpan) {
        best = el; bestSpan = end - start;
      }
    }
    return best;
  }

  function jumpToLine(line, query) {
    const block = elementAtLine(line);
    const marks = highlightMatches(els.content, query);
    let target = block;
    if (block) {
      const inBlock = marks.find((m) => block.contains(m));
      if (inBlock) { inBlock.classList.add("hit-current"); target = inBlock; }
    } else if (marks.length) {
      marks[0].classList.add("hit-current");
      target = marks[0];
    }
    if (!target) return;
    const top = target.getBoundingClientRect().top - els.reader.getBoundingClientRect().top + els.reader.scrollTop;
    els.reader.scrollTo({ top: Math.max(0, top - els.reader.clientHeight * 0.3) });
  }

  // ---------- folder search (⌘F) ----------
  const search = {
    el: $("search"),
    input: $("search-input"),
    results: $("search-results"),
    hits: [],
    active: -1,
    timer: null,
    seq: 0,
  };

  function openSearch() {
    if (!state.root) { toast("Open a folder first"); return; }
    search.el.hidden = false;
    search.input.focus();
    search.input.select();
    if (search.input.value) runSearch();
  }

  function closeSearch() {
    search.el.hidden = true;
    search.input.blur();
  }

  function runSearch() {
    clearTimeout(search.timer);
    search.timer = setTimeout(async () => {
      const q = search.input.value.trim();
      const seq = ++search.seq;
      if (q.length < 2) { search.hits = []; search.results.innerHTML = ""; return; }
      search.results.innerHTML = `<div class="sr-status">Searching…</div>`;
      let hits;
      try { hits = await invoke("search", { query: q }); }
      catch (e) { search.results.innerHTML = `<div class="sr-empty">${escapeHtml(String(e))}</div>`; return; }
      if (seq !== search.seq) return; // a newer query superseded this one
      search.hits = hits;
      search.active = hits.length ? 0 : -1;
      renderResults(q);
    }, 120);
  }

  function escapeHtml(s) {
    return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
  }

  function markSnippet(text, q) {
    const lower = text.toLowerCase(), needle = q.toLowerCase();
    let out = "", last = 0, idx;
    while ((idx = lower.indexOf(needle, last)) !== -1) {
      out += escapeHtml(text.slice(last, idx)) + "<b>" + escapeHtml(text.slice(idx, idx + needle.length)) + "</b>";
      last = idx + needle.length;
    }
    return out + escapeHtml(text.slice(last));
  }

  function renderResults(q) {
    const box = search.results;
    box.innerHTML = "";
    if (!search.hits.length) {
      box.innerHTML = `<div class="sr-empty">No matches for “${escapeHtml(q)}”</div>`;
      return;
    }
    let lastFile = null;
    search.hits.forEach((hit, i) => {
      if (hit.path !== lastFile) {
        const head = document.createElement("div");
        head.className = "sr-file";
        head.textContent = hit.path.startsWith(state.root + "/") ? hit.path.slice(state.root.length + 1) : hit.path;
        box.appendChild(head);
        lastFile = hit.path;
      }
      const row = document.createElement("div");
      row.className = "sr-hit" + (i === search.active ? " active" : "");
      row.dataset.index = i;
      row.innerHTML = `<span class="sr-line">${hit.line}</span><span class="sr-text">${markSnippet(hit.text, q)}</span>`;
      row.addEventListener("click", () => goToHit(i));
      row.addEventListener("mousemove", () => setActive(i));
      box.appendChild(row);
    });
  }

  function setActive(i) {
    if (i === search.active) return;
    search.active = i;
    for (const r of search.results.querySelectorAll(".sr-hit")) r.classList.toggle("active", +r.dataset.index === i);
    search.results.querySelector(".sr-hit.active")?.scrollIntoView({ block: "nearest" });
  }

  async function goToHit(i) {
    const hit = search.hits[i];
    if (!hit) return;
    const q = search.input.value.trim();
    closeSearch();
    await openDoc(hit.path, { line: hit.line, query: q });
  }

  search.input.addEventListener("input", runSearch);
  search.input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); if (search.hits.length) setActive(Math.min(search.hits.length - 1, search.active + 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); if (search.hits.length) setActive(Math.max(0, search.active - 1)); }
    else if (e.key === "Enter") { e.preventDefault(); if (search.active >= 0) goToHit(search.active); }
    else if (e.key === "Escape") { e.preventDefault(); closeSearch(); }
  });
  search.el.addEventListener("mousedown", (e) => { if (e.target === search.el) closeSearch(); });

  // ---------- sidebar resize ----------
  const SIDEBAR_DEFAULT = 260, SIDEBAR_MIN = 180, SIDEBAR_MAX = 560;
  function setSidebarWidth(w) {
    w = Math.round(Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, w)));
    document.documentElement.style.setProperty("--sidebar-w", w + "px");
    return w;
  }
  (() => {
    const handle = $("sidebar-resizer");
    let startX = 0, startW = 0, width = prefs.get("sidebarWidth", SIDEBAR_DEFAULT);
    setSidebarWidth(width);
    handle.addEventListener("mousedown", (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      startX = e.clientX;
      startW = width;
      els.body.classList.add("resizing");
      const move = (ev) => { width = setSidebarWidth(startW + ev.clientX - startX); };
      const up = () => {
        els.body.classList.remove("resizing");
        window.removeEventListener("mousemove", move);
        window.removeEventListener("mouseup", up);
        prefs.set("sidebarWidth", width);
      };
      window.addEventListener("mousemove", move);
      window.addEventListener("mouseup", up);
    });
    handle.addEventListener("dblclick", () => { width = setSidebarWidth(SIDEBAR_DEFAULT); prefs.set("sidebarWidth", width); });
  })();

  // ---------- keyboard ----------
  window.addEventListener("keydown", (e) => {
    const mod = e.metaKey;
    if (mod && e.key.toLowerCase() === "f") { e.preventDefault(); openSearch(); }
    else if (e.key === "Escape" && search.el.hidden) { clearHighlights(); }
    else if (mod && e.key.toLowerCase() === "o") { e.preventDefault(); pickFolder(); }
    else if (mod && e.shiftKey && e.key.toLowerCase() === "l") { e.preventDefault(); setSidebar(els.body.classList.contains("sidebar-hidden")); }
    else if (mod && (e.key === "=" || e.key === "+")) { e.preventDefault(); setZoom(getZoom() + 0.1); }
    else if (mod && e.key === "-") { e.preventDefault(); setZoom(getZoom() - 0.1); }
    else if (mod && e.key === "0") { e.preventDefault(); setZoom(1); }
    else if (e.altKey && e.key === "ArrowDown") { e.preventDefault(); stepDoc(1); }
    else if (e.altKey && e.key === "ArrowUp") { e.preventDefault(); stepDoc(-1); }
    else if (mod && e.key.toLowerCase() === "r") { e.preventDefault(); if (state.current) openDoc(state.current, { keepScroll: true }); refreshTree(); }
  });

  // Block default browser-ish behaviours (context menu stays for text selection).
  document.addEventListener("dragover", (e) => e.preventDefault());
  document.addEventListener("drop", (e) => e.preventDefault());

  // ---------- buttons ----------
  $("toggle-sidebar").addEventListener("click", () => setSidebar(els.body.classList.contains("sidebar-hidden")));
  $("open-folder").addEventListener("click", pickFolder);
  $("empty-open").addEventListener("click", pickFolder);

  // ---------- file watcher ----------
  event.listen("fs-changed", ({ payload }) => {
    const paths = payload?.paths || [];
    const touchesCurrent = state.current && paths.includes(state.current);
    if (touchesCurrent) openDoc(state.current, { keepScroll: true });
    // Any structural change (new/removed files or dirs) warrants a tree refresh.
    const structural = paths.some((p) => p !== state.current);
    if (structural || touchesCurrent) refreshTree().then(() => {
      if (state.current && !state.files.includes(state.current)) showEmptyDoc("This file was removed.");
    });
  });

  // Drag-and-drop a folder onto the window.
  event.listen("tauri://drag-drop", async ({ payload }) => {
    const p = payload?.paths?.[0];
    if (!p) return;
    try {
      const folder = await invoke("open_folder", { path: isMarkdown(p) ? dirname(p) : p });
      setFolder(folder);
      if (isMarkdown(p)) await openDoc(p); else await openInitialDoc();
    } catch (e) { toast(String(e)); }
  });

  // ---------- startup ----------
  (async () => {
    setZoom(prefs.get("zoom", 1));
    setSidebar(prefs.get("sidebar", true));
    try {
      const folder = await invoke("startup");
      if (folder) {
        setFolder(folder);
        await openInitialDoc();
      }
    } catch (e) {
      toast(String(e));
    }
  })();
})();
