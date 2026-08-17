// Injected as an initialization script into every content-tab webview,
// alongside context_menu.js (see create_tab_webview in main.rs) - runs
// before the page's own scripts and re-runs automatically on every
// navigation. Content tabs get zero Tauri IPC except the narrow
// report_favicon command this script uses (capabilities/content.json),
// following the same require_content() pattern report_context_menu uses.
//
// <link rel="icon"> tags aren't guaranteed to exist yet at document-start
// (when initialization scripts run), so this waits for the DOM to finish
// parsing before looking. Falls back to /favicon.ico at the origin root if
// no <link> is found, since plenty of sites rely on that convention
// without declaring it explicitly.
(function () {
  // WebView2 (and other backends) run initialization scripts in every
  // frame on the page, not just the top-level document - a page with a
  // cross-origin iframe (e.g. an embedded sign-in widget) would otherwise
  // get this running a second time inside that iframe, reporting the
  // iframe's own origin's favicon instead of the page's. Only the
  // top-level frame's report is meaningful for a tab's favicon.
  if (window.top !== window.self) {
    return;
  }

  function pickBestIcon() {
    const links = Array.from(
      document.querySelectorAll(
        'link[rel~="icon"], link[rel="shortcut icon"], link[rel="apple-touch-icon"]'
      )
    );
    if (!links.length) return null;

    function sizeOf(link) {
      const sizes = link.getAttribute("sizes");
      if (!sizes || sizes === "any") return 0;
      const match = sizes.match(/(\d+)x\d+/);
      return match ? parseInt(match[1], 10) : 0;
    }

    // Prefer the largest declared size - closer to what we'll actually
    // want once these render at tab-bar/bookmarks-bar scale.
    links.sort((a, b) => sizeOf(b) - sizeOf(a));
    return links[0].href || null; // .href resolves relative -> absolute
  }

  function report() {
    let href = pickBestIcon();
    if (!href) {
      href = new URL("/favicon.ico", window.location.origin).href;
    }
    if (window.__TAURI__ && window.__TAURI__.core) {
      window.__TAURI__.core.invoke("report_favicon", { href }).catch((err) =>
        console.error("[kite] report_favicon failed:", err)
      );
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", report, { once: true });
  } else {
    report();
  }
})();