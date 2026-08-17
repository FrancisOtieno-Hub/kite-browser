// Injected as an initialization script into every content-tab webview (see
// create_tab_webview in main.rs) - runs before the page's own scripts and
// re-runs automatically on every navigation, unlike find_in_page.js which
// is only eval'd on demand via Ctrl+F.
//
// Content tabs are otherwise IPC-locked, but this script is granted exactly
// two narrow, validated commands - report_context_menu and
// report_content_click - scoped to content-* webviews only
// (capabilities/content.json), and guarded again in main.rs via
// require_content() as defense in depth.
(function () {
  // Only text-entry targets get the editable menu (Paste/Select All, plus
  // Cut when there's a selection) - a checkbox, submit button, disabled
  // field, etc. has nothing to paste into, so it falls through to the
  // ordinary selection/page menu instead.
  function isTextEditable(el) {
    if (!el) return false;
    if (el.tagName === "TEXTAREA") return !el.disabled && !el.readOnly;
    if (el.tagName === "INPUT") {
      const nonText = ["checkbox", "radio", "submit", "button", "reset", "file", "range", "color", "image"];
      return !nonText.includes((el.type || "text").toLowerCase()) && !el.disabled && !el.readOnly;
    }
    return !!el.isContentEditable;
  }

  document.addEventListener(
    "contextmenu",
    (e) => {
      e.preventDefault();

      const linkEl = e.target.closest && e.target.closest("a[href]");
      const imgEl = e.target.closest && e.target.closest("img");
      const editableEl = e.target.closest && e.target.closest("input, textarea, [contenteditable]");
      const selectionText = window.getSelection ? window.getSelection().toString() : "";

      let targetType = "page";
      if (imgEl) targetType = "image";
      else if (linkEl) targetType = "link";
      else if (isTextEditable(editableEl)) targetType = "editable";
      else if (selectionText) targetType = "selection";

      const payload = {
        targetType,
        href: linkEl ? linkEl.href : null,
        src: imgEl ? imgEl.src : null,
        selectionText: selectionText || null,
        x: e.clientX,
        y: e.clientY,
      };

      if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke("report_context_menu", payload).catch((err) =>
          console.error("[kite] report_context_menu failed:", err)
        );
      }
    },
    true
  );

  // Lets chrome-side floating UI (currently just the new-tab right-click
  // menu) close itself on a click anywhere in the page - see
  // report_content_click's own comment in main.rs for why chrome can't
  // just listen for this on its own. Capture phase, and left/middle
  // clicks only (not right-click, which the contextmenu handler above
  // already covers, and firing here too would just be redundant).
  document.addEventListener(
    "mousedown",
    () => {
      if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke("report_content_click").catch((err) =>
          console.error("[kite] report_content_click failed:", err)
        );
      }
    },
    true
  );
})();