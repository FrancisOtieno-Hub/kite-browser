// Injected into the active content webview via webview.eval() from the
// toggle_find_in_page command. Runs entirely inside that page's own DOM -
// content tabs have no Tauri IPC, so this can't call back into Rust and
// doesn't need to: search, highlighting, and navigation between matches
// all happen locally in JS.
//
// Re-running this script (i.e. pressing Ctrl+F again) toggles it closed
// if already open, via the window.__kiteFindBar guard below.
(function () {
  const BAR_ID = "__kite_find_bar__";

  if (window.__kiteFindBar) {
    window.__kiteFindCleanup && window.__kiteFindCleanup();
    return;
  }

  const style = document.createElement("style");
  style.textContent = `
    #${BAR_ID} {
      position: fixed;
      top: 10px;
      right: 10px;
      z-index: 2147483647;
      display: flex;
      align-items: center;
      gap: 6px;
      background: #fff;
      border: 1px solid #d0d0d0;
      border-radius: 8px;
      box-shadow: 0 4px 16px rgba(0,0,0,0.15);
      padding: 6px 8px;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      font-size: 13px;
      color: #222;
    }
    #${BAR_ID} input {
      border: 1px solid #d0d0d0;
      border-radius: 5px;
      padding: 4px 6px;
      font-size: 13px;
      width: 160px;
      outline: none;
    }
    #${BAR_ID} input:focus {
      border-color: #3a7afe;
    }
    #${BAR_ID} button {
      border: none;
      background: transparent;
      cursor: pointer;
      font-size: 14px;
      padding: 2px 6px;
      border-radius: 4px;
      color: #333;
    }
    #${BAR_ID} button:hover {
      background: #f0f0f0;
    }
    #${BAR_ID} .kite-find-count {
      min-width: 46px;
      text-align: center;
      color: #666;
      font-variant-numeric: tabular-nums;
    }
    mark.kite-find-hl {
      background: #ffe066;
      color: inherit;
      padding: 0;
    }
    mark.kite-find-hl.kite-find-current {
      background: #ff9f1c;
    }
  `;
  document.documentElement.appendChild(style);

  const bar = document.createElement("div");
  bar.id = BAR_ID;
  bar.innerHTML = `
    <input type="text" placeholder="Find in page" autocomplete="off" />
    <span class="kite-find-count">0/0</span>
    <button data-action="prev" title="Previous match">&#8593;</button>
    <button data-action="next" title="Next match">&#8595;</button>
    <button data-action="close" title="Close">&#10005;</button>
  `;
  document.documentElement.appendChild(bar);
  window.__kiteFindBar = bar;

  const input = bar.querySelector("input");
  const countEl = bar.querySelector(".kite-find-count");

  let marks = [];
  let currentIndex = -1;

  function clearHighlights() {
    marks.forEach((mark) => {
      const parent = mark.parentNode;
      if (!parent) return;
      parent.replaceChild(document.createTextNode(mark.textContent), mark);
      parent.normalize();
    });
    marks = [];
    currentIndex = -1;
  }

  function collectTextNodes() {
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        const el = node.parentElement;
        if (!el || el.tagName === "SCRIPT" || el.tagName === "STYLE" || el.tagName === "NOSCRIPT") {
          return NodeFilter.FILTER_REJECT;
        }
        if (el.closest("#" + BAR_ID)) return NodeFilter.FILTER_REJECT;
        if (!node.nodeValue || !node.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    const nodes = [];
    let n;
    while ((n = walker.nextNode())) nodes.push(n);
    return nodes;
  }

  function runSearch(query) {
    clearHighlights();
    if (!query) {
      countEl.textContent = "0/0";
      return;
    }
    const lower = query.toLowerCase();

    collectTextNodes().forEach((node) => {
      const text = node.nodeValue;
      const lowerText = text.toLowerCase();
      const ranges = [];
      let start = 0;
      let idx;
      while ((idx = lowerText.indexOf(lower, start)) !== -1) {
        ranges.push([idx, idx + lower.length]);
        start = idx + lower.length;
      }
      if (!ranges.length) return;

      const frag = document.createDocumentFragment();
      let cursor = 0;
      ranges.forEach(([s, e]) => {
        if (s > cursor) frag.appendChild(document.createTextNode(text.slice(cursor, s)));
        const mark = document.createElement("mark");
        mark.className = "kite-find-hl";
        mark.textContent = text.slice(s, e);
        frag.appendChild(mark);
        marks.push(mark);
        cursor = e;
      });
      if (cursor < text.length) frag.appendChild(document.createTextNode(text.slice(cursor)));
      node.parentNode.replaceChild(frag, node);
    });

    if (marks.length) {
      currentIndex = 0;
      focusCurrent();
    } else {
      countEl.textContent = "0/0";
    }
  }

  function focusCurrent() {
    marks.forEach((m) => m.classList.remove("kite-find-current"));
    if (currentIndex < 0 || currentIndex >= marks.length) return;
    const mark = marks[currentIndex];
    mark.classList.add("kite-find-current");
    mark.scrollIntoView({ block: "center", behavior: "smooth" });
    countEl.textContent = `${currentIndex + 1}/${marks.length}`;
  }

  function goNext() {
    if (!marks.length) return;
    currentIndex = (currentIndex + 1) % marks.length;
    focusCurrent();
  }

  function goPrev() {
    if (!marks.length) return;
    currentIndex = (currentIndex - 1 + marks.length) % marks.length;
    focusCurrent();
  }

  function closeBar() {
    clearHighlights();
    style.remove();
    bar.remove();
    window.__kiteFindBar = null;
    window.__kiteFindCleanup = null;
  }
  window.__kiteFindCleanup = closeBar;

  let debounceTimer = null;
  input.addEventListener("input", () => {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => runSearch(input.value), 150);
  });

  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) goPrev();
      else goNext();
    } else if (e.key === "Escape") {
      e.preventDefault();
      closeBar();
    }
  });

  bar.querySelector('[data-action="prev"]').addEventListener("click", goPrev);
  bar.querySelector('[data-action="next"]').addEventListener("click", goNext);
  bar.querySelector('[data-action="close"]').addEventListener("click", closeBar);

  input.focus();
})();
