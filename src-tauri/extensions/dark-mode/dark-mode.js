// Kite Dark Mode
//
// Runs inside build_extension_script's guard (match/exclude checks and the
// internal-page block already happened before this code executes), so this
// file only has to do the actual work: invert the page, un-invert media so
// photos/video don't look like negatives, and offer a small per-site toggle.
//
// Global default with a per-site override, stored in localStorage under
// STORAGE_KEY. localStorage is naturally scoped per-origin by the browser
// itself, so "off on example.com, still on everywhere else" falls out for
// free without needing a cross-site settings bridge (there isn't one yet -
// see ExtensionManifest.permissions in main.rs, reserved but unconsumed).
//
// The toggle button docks into a shared tray (#kite-ext-tray) at the
// viewport's right-middle edge, rather than planting its own fixed corner
// button. All Kite extensions run in the same page JS context (same-world
// injection, not isolated per-extension worlds), so this tray is visible
// to and shared by any other extension's script that runs in this page -
// see kiteGetToggleTray below. Right-middle was picked because it's a zone
// real site UI essentially never uses (nav/sign-in controls live in the
// corners, player controls are bottom-of-player not bottom-of-viewport).

(function () {
  var STORAGE_KEY = 'kite-dark-mode-disabled';
  var STYLE_ID = 'kite-dark-mode-style';

  var CSS = [
    'html.kite-dark-mode {',
    '  filter: invert(1) hue-rotate(180deg) !important;',
    '  background: #fff !important;',
    '}',
    // Re-invert anything that's already an image/video so it reads
    // normally instead of as a negative. The attribute-selector catches
    // inline background-image styles; it won't catch background images
    // set purely via an external stylesheet, which is an accepted gap for
    // a filter-based approach like this one.
    'html.kite-dark-mode img,',
    'html.kite-dark-mode video,',
    'html.kite-dark-mode picture,',
    'html.kite-dark-mode canvas,',
    'html.kite-dark-mode svg,',
    'html.kite-dark-mode embed,',
    'html.kite-dark-mode [style*="background-image"] {',
    '  filter: invert(1) hue-rotate(180deg) !important;',
    '}',
    // Shared tray styling - see kiteGetToggleTray. Defined identically in
    // every extension that uses the tray; CSS rules are idempotent so it's
    // harmless if more than one extension's stylesheet defines this.
    '#kite-ext-tray {',
    '  all: initial;',
    '  position: fixed;',
    '  top: 50%;',
    '  right: 10px;',
    '  transform: translateY(-50%);',
    '  z-index: 2147483647;',
    '  display: flex;',
    '  flex-direction: column;',
    '  gap: 8px;',
    '  align-items: center;',
    '}',
    '.kite-ext-tray-btn {',
    '  all: initial;',
    '  width: 32px;',
    '  height: 32px;',
    '  border-radius: 999px;',
    '  background: #1e1e1e;',
    '  color: #f2f2f2;',
    '  border: 1px solid rgba(255,255,255,0.15);',
    '  box-shadow: 0 2px 8px rgba(0,0,0,0.35);',
    '  display: flex;',
    '  align-items: center;',
    '  justify-content: center;',
    '  font-size: 15px;',
    '  font-family: system-ui, sans-serif;',
    '  cursor: pointer;',
    '  opacity: 0.45;',
    '  transition: opacity 0.15s ease;',
    '}',
    '.kite-ext-tray-btn:hover {',
    '  opacity: 1;',
    '}',
    // The toggle button itself must not be double-inverted back to
    // "inverted-looking" - it lives inside html.kite-dark-mode so the
    // outer filter already flips it once; this cancels that so it always
    // reads as a plain dark icon regardless of page state.
    'html.kite-dark-mode #kite-dark-mode-toggle {',
    '  filter: invert(1) hue-rotate(180deg) !important;',
    '}',
  ].join('\n');

  // Shared docking point for every Kite extension's toggle button - see
  // file header. Whichever extension's script runs first creates the
  // tray; the rest just append their own button into it.
  function kiteGetToggleTray() {
    var tray = document.getElementById('kite-ext-tray');
    if (tray) return tray;
    tray = document.createElement('div');
    tray.id = 'kite-ext-tray';
    document.body.appendChild(tray);
    return tray;
  }

  function isDisabledHere() {
    try {
      return window.localStorage.getItem(STORAGE_KEY) === '1';
    } catch (e) {
      // localStorage can throw in odd contexts (e.g. sandboxed iframes) -
      // fail open to "dark mode on" rather than crash the guard's try/catch
      // and skip the rest of the page's own scripts.
      return false;
    }
  }

  function setDisabledHere(disabled) {
    try {
      if (disabled) {
        window.localStorage.setItem(STORAGE_KEY, '1');
      } else {
        window.localStorage.removeItem(STORAGE_KEY);
      }
    } catch (e) {
      // Same reasoning as isDisabledHere - a failed write just means the
      // toggle won't survive a reload on this page, not a crash.
    }
  }

  function applyState(disabled) {
    document.documentElement.classList.toggle('kite-dark-mode', !disabled);
    var btn = document.getElementById('kite-dark-mode-toggle');
    if (btn) {
      btn.textContent = disabled ? '\u263E' : '\u2600'; // moon : sun
      btn.title = disabled
        ? 'Dark mode is off on this site - click to turn on'
        : 'Dark mode is on - click to turn off on this site';
    }
  }

  var disabled = isDisabledHere();

  // At run_at: document_start, WebView2 fires this script the instant the
  // Document object exists - which can be *before* the parser has created
  // <html> at all, so document.documentElement (and document.head) may
  // briefly be null. The Document node itself always exists, so watch it
  // for the moment <html> gets inserted rather than assuming it's there.
  function whenDocumentElementReady(cb) {
    if (document.documentElement) {
      cb();
      return;
    }
    var observer = new MutationObserver(function () {
      if (document.documentElement) {
        observer.disconnect();
        cb();
      }
    });
    observer.observe(document, { childList: true });
  }

  whenDocumentElementReady(function () {
    var style = document.createElement('style');
    style.id = STYLE_ID;
    style.textContent = CSS;
    (document.head || document.documentElement).appendChild(style);
    applyState(disabled);
  });

  // The toggle button needs document.body, which is later still - defer
  // just this part, independent of the style injection above.
  function addToggleButton() {
    if (document.getElementById('kite-dark-mode-toggle')) return;
    var btn = document.createElement('button');
    btn.id = 'kite-dark-mode-toggle';
    btn.type = 'button';
    btn.className = 'kite-ext-tray-btn';
    btn.addEventListener('click', function () {
      disabled = !disabled;
      setDisabledHere(disabled);
      applyState(disabled);
    });
    kiteGetToggleTray().appendChild(btn);
    applyState(disabled); // sets the correct icon now that the button exists
  }

  if (document.body) {
    addToggleButton();
  } else {
    document.addEventListener('DOMContentLoaded', addToggleButton, { once: true });
  }
})();
