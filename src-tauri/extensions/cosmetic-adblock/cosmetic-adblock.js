// Kite Cosmetic Ad Blocking
//
// This is deliberately separate from blocklist.txt: that list stops the
// network request for an ad from ever firing, but plenty of sites still
// leave behind an empty container, a "Advertisement" label, or a
// pre-reserved slot with padding once the ad inside it never loads. This
// extension hides that leftover chrome with plain CSS.
//
// Runs inside build_extension_script's guard, same as dark-mode - match /
// exclude / internal-page checks already happened before this executes.
//
// The toggle button docks into a shared tray (#kite-ext-tray) at the
// viewport's right-middle edge - see kiteGetToggleTray, identical pattern
// to dark-mode.js. All Kite extensions run in the same page JS context, so
// this tray is shared across whichever of them are installed.

(function () {
  var STORAGE_KEY = 'kite-adblock-disabled';
  var STYLE_ID = 'kite-adblock-style';

  // Curated selectors for common, well-known ad-serving markup patterns
  // (Google Ads/AdSense/GPT, Amazon, Criteo, Taboola, Outbrain, and generic
  // "this is obviously an ad slot" class/id/attribute conventions). Kept
  // narrow and specific on purpose - broad substring matches like
  // [class*="ad-"] catch too much real content ("gradient", "leader",
  // "header", "load-more") and do more harm than good.
  var SELECTORS = [
    // Google AdSense / Ad Manager (GPT)
    'ins.adsbygoogle',
    '.adsbygoogle',
    'iframe[id^="google_ads_iframe"]',
    'iframe[src*="googlesyndication.com"]',
    'iframe[src*="doubleclick.net"]',
    '[id^="div-gpt-ad"]',
    '[id*="google_ads_iframe"]',

    // Amazon ad units
    'iframe[src*="amazon-adsystem.com"]',
    '[id^="amzn-assoc-ad"]',

    // Criteo / Taboola / Outbrain (all already host-blocked in
    // blocklist.txt, but their empty containers linger without this)
    'iframe[src*="criteo.com"]',
    '.criteo-ad',
    '.taboola',
    '[id^="taboola-"]',
    '.OUTBRAIN',
    '.outbrain-widget',

    // Generic AppNexus / Xandr / other common ad-server iframe hosts
    'iframe[src*="adnxs.com"]',
    'iframe[src*="adsystem.com"]',
    'iframe[src*="advertising.com"]',

    // AMP ad elements
    'amp-ad',
    'amp-embed[type="doubleclick"]',

    // Generic, explicitly ad-labelled containers - conservative enough
    // that legitimate content is unlikely to use these exact class names
    '.ad-slot',
    '.ad-container',
    '.ad-wrapper',
    '.advertisement',
    '.advert',
    'div[data-ad-slot]',
    'div[data-ad-client]',
    'aside[aria-label="Advertisement" i]',
    'div[aria-label="Advertisement" i]',
    'section[aria-label="Sponsored" i]',
  ];

  // Every selector is prefixed with html:not(.kite-adblock-off) so the
  // per-site toggle can turn hiding off without touching every matched
  // element individually - just add/remove one class on <html>.
  var CSS =
    SELECTORS.map(function (s) {
      return 'html:not(.kite-adblock-off) ' + s;
    }).join(',\n') +
    ' {\n' +
    '  display: none !important;\n' +
    '}\n' +
    // Shared tray styling - see kiteGetToggleTray. Defined identically in
    // every extension that uses the tray; CSS rules are idempotent so it's
    // harmless if more than one extension's stylesheet defines this.
    '#kite-ext-tray {\n' +
    '  all: initial;\n' +
    '  position: fixed;\n' +
    '  top: 50%;\n' +
    '  right: 10px;\n' +
    '  transform: translateY(-50%);\n' +
    '  z-index: 2147483647;\n' +
    '  display: flex;\n' +
    '  flex-direction: column;\n' +
    '  gap: 8px;\n' +
    '  align-items: center;\n' +
    '}\n' +
    '.kite-ext-tray-btn {\n' +
    '  all: initial;\n' +
    '  width: 32px;\n' +
    '  height: 32px;\n' +
    '  border-radius: 999px;\n' +
    '  background: #1e1e1e;\n' +
    '  color: #f2f2f2;\n' +
    '  border: 1px solid rgba(255,255,255,0.15);\n' +
    '  box-shadow: 0 2px 8px rgba(0,0,0,0.35);\n' +
    '  display: flex;\n' +
    '  align-items: center;\n' +
    '  justify-content: center;\n' +
    '  font-size: 15px;\n' +
    '  font-family: system-ui, sans-serif;\n' +
    '  cursor: pointer;\n' +
    '  opacity: 0.45;\n' +
    '  transition: opacity 0.15s ease;\n' +
    '}\n' +
    '.kite-ext-tray-btn:hover {\n' +
    '  opacity: 1;\n' +
    '}\n';

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
      // Fail open (blocking stays on) rather than let a storage error
      // escape this function and abort the whole content script.
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
      // A failed write just means the toggle won't survive a reload here.
    }
  }

  function applyState(disabled) {
    document.documentElement.classList.toggle('kite-adblock-off', disabled);
    var btn = document.getElementById('kite-adblock-toggle');
    if (btn) {
      btn.textContent = disabled ? '\u{1F6AB}' : '\u{1F6E1}\uFE0F'; // no-entry : shield
      btn.title = disabled
        ? 'Cosmetic ad blocking is off on this site - click to turn back on'
        : 'Cosmetic ad blocking is on - click to turn off on this site';
    }
  }

  var disabled = isDisabledHere();

  // Same document_start timing hazard as dark-mode.js: the Document object
  // can exist before <html> does, so document.documentElement may briefly
  // be null. Wait for it via MutationObserver on document itself rather
  // than assuming it's present.
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

  // The toggle button needs document.body, which is later still.
  function addToggleButton() {
    if (document.getElementById('kite-adblock-toggle')) return;
    var btn = document.createElement('button');
    btn.id = 'kite-adblock-toggle';
    btn.type = 'button';
    btn.className = 'kite-ext-tray-btn';
    btn.addEventListener('click', function () {
      disabled = !disabled;
      setDisabledHere(disabled);
      applyState(disabled);
    });
    kiteGetToggleTray().appendChild(btn);
    applyState(disabled);
  }

  if (document.body) {
    addToggleButton();
  } else {
    document.addEventListener('DOMContentLoaded', addToggleButton, { once: true });
  }
})();
