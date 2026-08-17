// This page runs inside a content-tab webview. Content tabs can browse to
// any external site, so - per capabilities/chrome.json - they get NO Tauri
// IPC access at all, by design; only the chrome webview (tab bar, toolbar,
// bookmarks bar, library panel) has that. Concretely that means this
// script can never call invoke(). Two consequences:
//
//   1. The search box below navigates with plain window.location, exactly
//      like clicking a link on any other page would.
//   2. The chosen search engine is pushed in by the Rust side via
//      webview.eval() when this page loads (see push_search_engine_to_home
//      in main.rs), since this page has no way to invoke("get_settings")
//      to fetch it directly. Bookmarks used to be pushed in here the same
//      way and rendered as a grid on this page - that's now a persistent
//      bar in the chrome instead (see index.html/main.js), shown on every
//      page rather than just this one, so there's nothing bookmark-related
//      left for this script to do.

const searchForm = document.getElementById("home-search-form");
const searchInput = document.getElementById("home-search-input");

// Mirrors the Rust-side search_url_for() in main.rs - kept in sync by hand
// for the same reason normalizeUrl below is.
let currentSearchEngine = window.__KITE_SEARCH_ENGINE__ || "google";

function searchUrlFor(engine, query) {
  const q = encodeURIComponent(query);
  if (engine === "bing") {
    return `https://www.bing.com/search?q=${q}`;
  }
  if (engine === "duckduckgo") {
    return `https://duckduckgo.com/?q=${q}`;
  }
  return `https://www.google.com/search?q=${q}`;
}

// Mirrors the Rust-side normalize_url() in main.rs - kept in sync by hand
// since this script has no way to call back into Rust for it.
function normalizeUrl(input) {
  const trimmed = input.trim();
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
    return trimmed;
  }
  if (trimmed.includes(".") && !trimmed.includes(" ")) {
    return `https://${trimmed}`;
  }
  return searchUrlFor(currentSearchEngine, trimmed);
}

searchForm.addEventListener("submit", (e) => {
  e.preventDefault();
  const value = searchInput.value.trim();
  if (!value) return;
  window.location.href = normalizeUrl(value);
});

// Same ordering caveat noted above for search engine - the Rust side may
// have already eval'd __KITE_SEARCH_ENGINE__ in before this script ran, or
// may still be about to, and this also picks up live changes made in
// Settings while this home tab is still open (parked behind the panel),
// since set_search_engine pushes to any open home tab.
window.addEventListener("kite-search-engine", () => {
  currentSearchEngine = window.__KITE_SEARCH_ENGINE__ || currentSearchEngine;
});