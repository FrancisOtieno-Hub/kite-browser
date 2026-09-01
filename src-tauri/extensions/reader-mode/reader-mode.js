// Kite Reader Mode
//
// Uses Mozilla's Readability.js (vendored as readability.js, loaded first -
// see manifest.json's "js" array; main.rs concatenates a manifest's js
// files in order into one script, so `Readability` from that file is
// already in scope by the time this file's code runs) - the same
// extraction engine behind Firefox's own Reader View, rather than a
// weaker hand-rolled heuristic.
//
// This overlays a clean reading view on TOP of the page rather than
// replacing document.body - swapping body content would kill the site's
// own running JS, any playing video/audio, scroll position, etc. The
// original page sits untouched underneath; toggling just shows/hides the
// overlay. Nothing is persisted (like Firefox's Reader View, this is a
// manual per-page action, not a saved site preference).
//
// The toggle button docks into the shared tray (#kite-ext-tray) at the
// viewport's right-middle edge - same pattern as the other extensions.

(function () {
  var overlay = null;
  var parsedArticle = null; // cached so re-toggling doesn't re-run Readability
  var parseAttempted = false;

  function sanitizeArticleHtml(html) {
    var container = document.createElement('div');
    container.innerHTML = html;

    var scripts = container.querySelectorAll('script');
    for (var i = 0; i < scripts.length; i++) scripts[i].remove();

    var all = container.querySelectorAll('*');
    for (var j = 0; j < all.length; j++) {
      var el = all[j];
      for (var k = el.attributes.length - 1; k >= 0; k--) {
        var attr = el.attributes[k];
        if (/^on/i.test(attr.name)) el.removeAttribute(attr.name);
      }
    }
    return container.innerHTML;
  }

  function tryParseArticle() {
    if (parseAttempted) return parsedArticle;
    parseAttempted = true;
    try {
      // Readability.parse() mutates the document it's given - clone first
      // so the live page (and anything still running on it) isn't touched.
      var clone = document.cloneNode(true);
      var reader = new Readability(clone);
      parsedArticle = reader.parse();
    } catch (e) {
      console.warn('[kite-extension:reader-mode] parse failed', e);
      parsedArticle = null;
    }
    return parsedArticle;
  }

  function buildOverlay(article) {
    var el = document.createElement('div');
    el.id = 'kite-reader-mode-overlay';

    var closeBtn = document.createElement('button');
    closeBtn.type = 'button';
    closeBtn.id = 'kite-reader-mode-close';
    closeBtn.textContent = '\u2715';
    closeBtn.title = 'Close Reader Mode';
    closeBtn.addEventListener('click', function () {
      el.classList.remove('kite-reader-mode-open');
    });

    var content = document.createElement('div');
    content.id = 'kite-reader-mode-content';

    var title = document.createElement('h1');
    title.textContent = article.title || document.title;
    content.appendChild(title);

    if (article.byline) {
      var byline = document.createElement('div');
      byline.id = 'kite-reader-mode-byline';
      byline.textContent = article.byline;
      content.appendChild(byline);
    }

    var body = document.createElement('div');
    body.id = 'kite-reader-mode-body';
    body.innerHTML = sanitizeArticleHtml(article.content || '');
    content.appendChild(body);

    el.appendChild(closeBtn);
    el.appendChild(content);
    document.body.appendChild(el);
    return el;
  }

  function injectStyles() {
    if (document.getElementById('kite-reader-mode-style')) return;
    var style = document.createElement('style');
    style.id = 'kite-reader-mode-style';
    style.textContent = [
      '#kite-reader-mode-overlay {',
      '  all: initial;',
      '  position: fixed;',
      '  inset: 0;',
      '  z-index: 2147483646;', // one below the tray, so the tray stays clickable
      '  display: none;',
      '  overflow-y: auto;',
      '  background: #faf7f2;',
      '  color: #222;',
      '  font-family: Georgia, "Times New Roman", serif;',
      '}',
      '#kite-reader-mode-overlay.kite-reader-mode-open { display: block; }',
      '#kite-reader-mode-close {',
      '  all: initial;',
      '  position: fixed;',
      '  top: 16px;',
      '  right: 16px;',
      '  width: 32px;',
      '  height: 32px;',
      '  border-radius: 999px;',
      '  background: #222;',
      '  color: #fff;',
      '  display: flex;',
      '  align-items: center;',
      '  justify-content: center;',
      '  cursor: pointer;',
      '  font-family: system-ui, sans-serif;',
      '  font-size: 14px;',
      '}',
      '#kite-reader-mode-content {',
      '  max-width: 700px;',
      '  margin: 0 auto;',
      '  padding: 64px 24px 96px;',
      '  line-height: 1.6;',
      '  font-size: 19px;',
      '}',
      '#kite-reader-mode-content h1 {',
      '  font-size: 32px;',
      '  line-height: 1.25;',
      '  margin-bottom: 8px;',
      '}',
      '#kite-reader-mode-byline {',
      '  color: #777;',
      '  font-family: system-ui, sans-serif;',
      '  font-size: 14px;',
      '  margin-bottom: 32px;',
      '}',
      '#kite-reader-mode-body img, #kite-reader-mode-body video {',
      '  max-width: 100%;',
      '  height: auto;',
      '}',
      '#kite-reader-mode-body a { color: #1a5fb4; }',
      '#kite-reader-mode-body p { margin: 0 0 1.2em; }',
      '#kite-reader-mode-body pre {',
      '  overflow-x: auto;',
      '  padding: 12px;',
      '  background: #f0ece3;',
      '  font-family: monospace;',
      '}',
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

  function showFailureToast() {
    var toast = document.createElement('div');
    toast.id = 'kite-reader-mode-toast';
    toast.textContent = "Couldn't find readable article content on this page";
    Object.assign(toast.style, {
      all: 'initial',
      position: 'fixed',
      bottom: '24px',
      left: '50%',
      transform: 'translateX(-50%)',
      zIndex: 2147483647,
      padding: '8px 14px',
      borderRadius: '6px',
      background: '#1e1e1e',
      color: '#f2f2f2',
      fontSize: '12px',
      fontFamily: 'system-ui, sans-serif',
    });
    document.body.appendChild(toast);
    setTimeout(function () {
      toast.remove();
    }, 2600);
  }

  function addToggleButton() {
    if (document.getElementById('kite-reader-mode-toggle')) return;

    var trayStyle = document.createElement('style');
    trayStyle.textContent = [
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
    document.head.appendChild(trayStyle);

    var btn = document.createElement('button');
    btn.id = 'kite-reader-mode-toggle';
    btn.type = 'button';
    btn.className = 'kite-ext-tray-btn';
    btn.textContent = '\u{1F4D6}'; // open book
    btn.title = 'Toggle Reader Mode';
    btn.addEventListener('click', function () {
      if (overlay && overlay.classList.contains('kite-reader-mode-open')) {
        overlay.classList.remove('kite-reader-mode-open');
        return;
      }
      var article = tryParseArticle();
      if (!article || !article.content) {
        showFailureToast();
        return;
      }
      injectStyles();
      if (!overlay) overlay = buildOverlay(article);
      overlay.classList.add('kite-reader-mode-open');
    });
    kiteGetToggleTray().appendChild(btn);
  }

  if (document.body) {
    addToggleButton();
  } else {
    document.addEventListener('DOMContentLoaded', addToggleButton, { once: true });
  }
})();
