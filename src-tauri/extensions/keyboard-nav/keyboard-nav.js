// Kite Keyboard Navigation (Vimium-style)
//
// Scope note: this only covers page-level interaction (scrolling, link
// hints, page history, reload) - not browser-chrome actions like new
// tab/close tab/switch tab. Those live in main.js and are wired to Tauri
// commands (see new_tab.toml, close_tab.toml, etc.), which requires
// window.__TAURI__ - not exposed to arbitrary content webviews for very
// good security reasons (any site could otherwise drive the browser
// chrome). So "F" here opens a link via window.open() rather than a real
// new Kite tab; whether that becomes a new tab depends on how Kite's
// WebView2 layer handles NewWindowRequested, which is worth confirming.
//
// Known collision to test deliberately: YouTube's own player shortcuts
// use the exact same keys (f = fullscreen, j/k/l = seek/pause/seek). This
// is a well-known Vimium annoyance on video sites. Rather than hardcoding
// a YouTube exclusion, this ships with the same per-site tray toggle as
// the other extensions - turn it off on sites where it fights the site's
// own shortcuts.
//
// The toggle button docks into the shared tray (#kite-ext-tray) at the
// viewport's right-middle edge, same pattern as the other extensions.

(function () {
  var STORAGE_KEY = 'kite-keyboard-nav-disabled';
  var HINT_CHARS = 'asdfghjkl'.split('');
  var CHORD_TIMEOUT_MS = 600;

  var disabled = isDisabledHere();
  var hintMode = null; // null, or { openInNewTab: bool, items: [{el, hint, badge}], buffer: string }
  var chordBuffer = '';
  var chordTimer = null;

  function isDisabledHere() {
    try {
      return window.localStorage.getItem(STORAGE_KEY) === '1';
    } catch (e) {
      return false;
    }
  }

  function setDisabledHere(value) {
    try {
      if (value) {
        window.localStorage.setItem(STORAGE_KEY, '1');
      } else {
        window.localStorage.removeItem(STORAGE_KEY);
      }
    } catch (e) {
      // Not fatal - just won't survive a reload here.
    }
  }

  function isEditableTarget(el) {
    if (!el) return false;
    var tag = el.tagName;
    return (
      tag === 'INPUT' ||
      tag === 'TEXTAREA' ||
      tag === 'SELECT' ||
      el.isContentEditable
    );
  }

  // --- Hint generation -------------------------------------------------
  // Produces `count` short strings from `chars` such that no hint is a
  // prefix of another (so typing is unambiguous). Standard technique:
  // breadth-first expand a tree of candidate strings, and take a
  // contiguous slice once there are enough - anything already dequeued
  // has been "spent" (replaced by its children), so no taken hint can be
  // an ancestor of another taken hint.
  function generateHintStrings(count, chars) {
    if (count <= 0) return [];
    var hints = [''];
    var offset = 0;
    while (hints.length - offset < count || hints.length === 1) {
      var hint = hints[offset++];
      for (var i = 0; i < chars.length; i++) {
        hints.push(hint + chars[i]);
      }
    }
    return hints.slice(offset, offset + count);
  }

  function isVisible(el) {
    var rect = el.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return false;
    if (rect.bottom < 0 || rect.top > window.innerHeight) return false;
    if (rect.right < 0 || rect.left > window.innerWidth) return false;
    var style = window.getComputedStyle(el);
    return style.visibility !== 'hidden' && style.display !== 'none';
  }

  function collectHintableElements() {
    var selector =
      'a[href], button, input:not([type="hidden"]), select, textarea, ' +
      '[role="button"], [role="link"], [onclick], [tabindex]:not([tabindex="-1"])';
    var candidates = document.querySelectorAll(selector);
    var result = [];
    for (var i = 0; i < candidates.length; i++) {
      if (isVisible(candidates[i])) result.push(candidates[i]);
    }
    return result;
  }

  function exitHintMode() {
    if (!hintMode) return;
    var overlay = document.getElementById('kite-vimium-hints-overlay');
    if (overlay) overlay.remove();
    hintMode = null;
  }

  function activateHintTarget(el, openInNewTab) {
    if (openInNewTab && el.tagName === 'A' && el.href) {
      // window.open() does nothing here - Kite's WebView2 layer has no
      // NewWindowRequested handler wired to anything, confirmed in
      // testing. open_link_in_new_tab is a purpose-built Tauri command
      // (content.json/require_content-gated, same pattern as
      // report_context_menu) that calls the same create_tab_webview path
      // the native "Open link in new tab" menu item uses.
      if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke('open_link_in_new_tab', { href: el.href }).catch(function (err) {
          console.warn('[kite-extension:keyboard-nav] open_link_in_new_tab failed', err);
        });
      } else {
        el.click(); // fallback if IPC isn't available for some reason
      }
    } else {
      el.click();
      if (el.focus) el.focus();
    }
    exitHintMode();
  }

  function enterHintMode(openInNewTab) {
    exitHintMode();
    var elements = collectHintableElements();
    if (elements.length === 0) return;

    var hints = generateHintStrings(elements.length, HINT_CHARS);
    var overlay = document.createElement('div');
    overlay.id = 'kite-vimium-hints-overlay';

    var items = [];
    for (var i = 0; i < elements.length; i++) {
      var el = elements[i];
      var rect = el.getBoundingClientRect();
      var badge = document.createElement('div');
      badge.className = 'kite-vimium-hint-badge';
      badge.textContent = hints[i].toUpperCase();
      badge.style.left = Math.max(rect.left, 0) + 'px';
      badge.style.top = Math.max(rect.top, 0) + 'px';
      overlay.appendChild(badge);
      items.push({ el: el, hint: hints[i], badge: badge });
    }

    document.body.appendChild(overlay);
    hintMode = { openInNewTab: openInNewTab, items: items, buffer: '' };
  }

  function handleHintModeKey(key) {
    if (key === 'Escape') {
      exitHintMode();
      return;
    }
    if (!/^[a-z]$/i.test(key)) return; // ignore anything not a plain letter

    hintMode.buffer += key.toLowerCase();
    var remaining = [];
    for (var i = 0; i < hintMode.items.length; i++) {
      var item = hintMode.items[i];
      var matches = item.hint.indexOf(hintMode.buffer) === 0;
      item.badge.style.display = matches ? '' : 'none';
      if (matches) remaining.push(item);
    }

    if (remaining.length === 0) {
      exitHintMode();
    } else if (remaining.length === 1 && remaining[0].hint === hintMode.buffer) {
      activateHintTarget(remaining[0].el, hintMode.openInNewTab);
    }
  }

  // --- Chord tracking (gg, yy) ------------------------------------------
  function resetChord() {
    chordBuffer = '';
    if (chordTimer) {
      clearTimeout(chordTimer);
      chordTimer = null;
    }
  }

  function pushChordKey(key) {
    chordBuffer += key;
    if (chordTimer) clearTimeout(chordTimer);
    chordTimer = setTimeout(resetChord, CHORD_TIMEOUT_MS);
  }

  // --- Misc UI: toast + help overlay -------------------------------------
  function showToast(text) {
    var toast = document.getElementById('kite-vimium-toast');
    if (!toast) {
      toast = document.createElement('div');
      toast.id = 'kite-vimium-toast';
      document.body.appendChild(toast);
    }
    toast.textContent = text;
    toast.classList.add('kite-vimium-toast-visible');
    clearTimeout(toast._hideTimer);
    toast._hideTimer = setTimeout(function () {
      toast.classList.remove('kite-vimium-toast-visible');
    }, 1500);
  }

  function toggleHelpOverlay() {
    var existing = document.getElementById('kite-vimium-help');
    if (existing) {
      existing.remove();
      return;
    }
    var help = document.createElement('div');
    help.id = 'kite-vimium-help';
    help.innerHTML =
      '<div class="kite-vimium-help-title">Keyboard Navigation</div>' +
      '<div class="kite-vimium-help-row"><b>f</b> / <b>F</b> — click a link (F = new tab)</div>' +
      '<div class="kite-vimium-help-row"><b>j</b> / <b>k</b> — scroll down / up</div>' +
      '<div class="kite-vimium-help-row"><b>d</b> / <b>u</b> — half-page down / up</div>' +
      '<div class="kite-vimium-help-row"><b>h</b> / <b>l</b> — scroll left / right</div>' +
      '<div class="kite-vimium-help-row"><b>gg</b> / <b>G</b> — top / bottom of page</div>' +
      '<div class="kite-vimium-help-row"><b>H</b> / <b>L</b> — back / forward</div>' +
      '<div class="kite-vimium-help-row"><b>r</b> — reload</div>' +
      '<div class="kite-vimium-help-row"><b>yy</b> — copy URL</div>' +
      '<div class="kite-vimium-help-row"><b>?</b> — toggle this help</div>' +
      '<div class="kite-vimium-help-row kite-vimium-help-dim">Esc closes hints/help</div>';
    help.addEventListener('click', function () {
      help.remove();
    });
    document.body.appendChild(help);
  }

  // --- Main key handler ---------------------------------------------------
  function onKeyDown(e) {
    if (disabled) return;
    if (hintMode) {
      handleHintModeKey(e.key);
      if (e.key === 'Escape' || /^[a-z]$/i.test(e.key)) {
        e.preventDefault();
        e.stopPropagation();
      }
      return;
    }

    if (e.key === 'Escape') {
      if (document.activeElement && isEditableTarget(document.activeElement)) {
        document.activeElement.blur();
      }
      var help = document.getElementById('kite-vimium-help');
      if (help) help.remove();
      return;
    }

    // Everything below only applies outside form fields, and only for
    // plain keys (no modifier combos we'd otherwise steal from the page,
    // e.g. Ctrl+F for the browser's own find).
    if (isEditableTarget(document.activeElement)) return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;

    switch (e.key) {
      case 'j':
        window.scrollBy(0, 70);
        break;
      case 'k':
        window.scrollBy(0, -70);
        break;
      case 'd':
        window.scrollBy(0, window.innerHeight / 2);
        break;
      case 'u':
        window.scrollBy(0, -window.innerHeight / 2);
        break;
      case 'h':
        window.scrollBy(-70, 0);
        break;
      case 'l':
        window.scrollBy(70, 0);
        break;
      case 'G':
        window.scrollTo(0, document.body.scrollHeight);
        break;
      case 'H':
        window.history.back();
        break;
      case 'L':
        window.history.forward();
        break;
      case 'r':
        window.location.reload();
        break;
      case 'f':
        enterHintMode(false);
        break;
      case 'F':
        enterHintMode(true);
        break;
      case '?':
        toggleHelpOverlay();
        break;
      case 'g':
        if (chordBuffer === 'g') {
          window.scrollTo(0, 0);
          resetChord();
        } else {
          pushChordKey('g');
        }
        return; // don't fall through to resetChord() below
      case 'y':
        if (chordBuffer === 'y') {
          try {
            navigator.clipboard.writeText(window.location.href);
            showToast('Copied URL');
          } catch (err) {
            console.warn('[kite-extension:keyboard-nav] clipboard write failed', err);
          }
          resetChord();
        } else {
          pushChordKey('y');
        }
        return;
      default:
        return; // unrecognized key - don't touch the chord buffer or preventDefault
    }

    resetChord();
    e.preventDefault();
  }

  function injectStyles() {
    var style = document.createElement('style');
    style.textContent = [
      '#kite-vimium-hints-overlay {',
      '  all: initial;',
      '  position: fixed;',
      '  inset: 0;',
      '  z-index: 2147483647;',
      '  pointer-events: none;',
      '}',
      '.kite-vimium-hint-badge {',
      '  all: initial;',
      '  position: fixed;',
      '  background: #ffd54a;',
      '  color: #1a1a1a;',
      '  border: 1px solid #a8860f;',
      '  border-radius: 3px;',
      '  padding: 1px 4px;',
      '  font-family: system-ui, sans-serif;',
      '  font-size: 11px;',
      '  font-weight: bold;',
      '  line-height: 1.4;',
      '  box-shadow: 0 1px 3px rgba(0,0,0,0.4);',
      '  transform: translateY(-100%);',
      '}',
      '#kite-vimium-toast {',
      '  all: initial;',
      '  position: fixed;',
      '  bottom: 24px;',
      '  left: 50%;',
      '  transform: translateX(-50%) translateY(4px);',
      '  z-index: 2147483647;',
      '  padding: 6px 12px;',
      '  border-radius: 6px;',
      '  background: #1e1e1e;',
      '  color: #f2f2f2;',
      '  font-size: 12px;',
      '  font-family: system-ui, sans-serif;',
      '  opacity: 0;',
      '  transition: opacity 0.2s ease, transform 0.2s ease;',
      '  pointer-events: none;',
      '}',
      '#kite-vimium-toast.kite-vimium-toast-visible {',
      '  opacity: 0.92;',
      '  transform: translateX(-50%) translateY(0);',
      '}',
      '#kite-vimium-help {',
      '  all: initial;',
      '  position: fixed;',
      '  top: 50%;',
      '  left: 50%;',
      '  transform: translate(-50%, -50%);',
      '  z-index: 2147483647;',
      '  background: #1e1e1e;',
      '  color: #f2f2f2;',
      '  font-family: system-ui, sans-serif;',
      '  font-size: 13px;',
      '  padding: 20px 24px;',
      '  border-radius: 10px;',
      '  box-shadow: 0 4px 24px rgba(0,0,0,0.5);',
      '  cursor: pointer;',
      '  display: flex;',
      '  flex-direction: column;',
      '  gap: 6px;',
      '}',
      '.kite-vimium-help-title { font-weight: bold; margin-bottom: 6px; }',
      '.kite-vimium-help-dim { color: #999; margin-top: 6px; font-size: 11px; }',
      // Shared tray styling - identical in every extension using the
      // tray; CSS rules are idempotent so duplication across files is
      // harmless.
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
      '.kite-ext-tray-btn:hover { opacity: 1; }',
    ].join('\n');
    document.head.appendChild(style);
  }

  // Shared docking point for every Kite extension's toggle button.
  function kiteGetToggleTray() {
    var tray = document.getElementById('kite-ext-tray');
    if (tray) return tray;
    tray = document.createElement('div');
    tray.id = 'kite-ext-tray';
    document.body.appendChild(tray);
    return tray;
  }

  function addToggleButton() {
    if (document.getElementById('kite-keyboard-nav-toggle')) return;
    injectStyles();
    var btn = document.createElement('button');
    btn.id = 'kite-keyboard-nav-toggle';
    btn.type = 'button';
    btn.className = 'kite-ext-tray-btn';
    btn.textContent = disabled ? '\u{1F6AB}' : '\u2328\uFE0F'; // no-entry : keyboard
    btn.title = disabled
      ? 'Keyboard navigation is off on this site - click to turn back on'
      : 'Keyboard navigation is on - click to turn off on this site (press ? for keys)';
    btn.addEventListener('click', function () {
      disabled = !disabled;
      setDisabledHere(disabled);
      btn.textContent = disabled ? '\u{1F6AB}' : '\u2328\uFE0F';
      btn.title = disabled
        ? 'Keyboard navigation is off on this site - click to turn back on'
        : 'Keyboard navigation is on - click to turn off on this site (press ? for keys)';
      exitHintMode();
    });
    kiteGetToggleTray().appendChild(btn);
  }

  document.addEventListener('keydown', onKeyDown, { capture: true });

  if (document.body) {
    addToggleButton();
  } else {
    document.addEventListener('DOMContentLoaded', addToggleButton, { once: true });
  }
})();
