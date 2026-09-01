// Kite Volume Booster
//
// Native <video>/<audio> volume caps at 100% - there's no way past that
// without routing the audio through the Web Audio API's GainNode, which
// can amplify beyond unity. That's the whole mechanism here.
//
// Two things make this different from the other extensions:
//
// 1. createMediaElementSource(el) is ONE-WAY and PERMANENT for that
//    element's lifetime: once an element's audio is routed into a Web
//    Audio graph, it can never go back to plain browser-handled output,
//    and calling createMediaElementSource on the same element a second
//    time throws. So "off" here means gain = 1.0 (unity, i.e. normal
//    volume), never disconnecting - and every element gets wrapped
//    exactly once, tracked via WRAPPED (a WeakSet).
//
// 2. AudioContext starts suspended until a real user gesture occurs on
//    the page (browser autoplay policy) - boosted audio would otherwise
//    silently do nothing until the user clicks or presses a key, so we
//    resume the context on the first such interaction.
//
// This is a percentage slider, not an on/off toggle like the other
// extensions, so it gets a small popup panel instead of a single-click
// tray icon - the shared tray icon just opens/closes that panel.

(function () {
  var STORAGE_KEY = 'kite-volume-boost-percent';
  var MIN_PERCENT = 100;
  var MAX_PERCENT = 500;
  var DEFAULT_PERCENT = 100;

  var audioCtx = null;
  var gainNodes = []; // every GainNode currently in the graph, kept in sync with the slider
  var WRAPPED = new WeakSet();
  var currentPercent = loadPercent();

  function loadPercent() {
    try {
      var v = parseInt(window.localStorage.getItem(STORAGE_KEY), 10);
      if (!isNaN(v) && v >= MIN_PERCENT && v <= MAX_PERCENT) return v;
    } catch (e) {
      // fall through to default
    }
    return DEFAULT_PERCENT;
  }

  function savePercent(v) {
    try {
      window.localStorage.setItem(STORAGE_KEY, String(v));
    } catch (e) {
      // Not fatal - just won't survive a reload.
    }
  }

  function getAudioContext() {
    if (!audioCtx) {
      audioCtx = new (window.AudioContext || window.webkitAudioContext)();
    }
    return audioCtx;
  }

  function ensureResumedOnUserGesture() {
    function resume() {
      if (audioCtx && audioCtx.state === 'suspended') {
        audioCtx.resume().catch(function () {
          // Some browsers still refuse without a "real" click on this
          // exact event tick - a later gesture will retry naturally
          // since we don't remove these listeners.
        });
      }
    }
    document.addEventListener('click', resume, { capture: true });
    document.addEventListener('keydown', resume, { capture: true });
  }

  function wrapMediaElement(el) {
    if (WRAPPED.has(el)) return;
    WRAPPED.add(el);
    try {
      var ctx = getAudioContext();
      var source = ctx.createMediaElementSource(el);
      var gain = ctx.createGain();
      gain.gain.value = currentPercent / 100;
      source.connect(gain);
      gain.connect(ctx.destination);
      gainNodes.push(gain);
    } catch (e) {
      // Some elements (e.g. ones already routed elsewhere, or certain
      // cross-origin edge cases) can throw here - skip rather than break
      // the rest of the page's playback.
      console.warn('[kite-extension:volume-booster] could not wrap media element', e);
    }
  }

  function applyPercentToAllNodes(percent) {
    currentPercent = percent;
    savePercent(percent);
    for (var i = 0; i < gainNodes.length; i++) {
      gainNodes[i].gain.value = percent / 100;
    }
  }

  function wrapExistingMediaElements() {
    var els = document.querySelectorAll('video, audio');
    for (var i = 0; i < els.length; i++) wrapMediaElement(els[i]);
  }

  function watchForNewMediaElements() {
    var observer = new MutationObserver(function (mutations) {
      for (var i = 0; i < mutations.length; i++) {
        var added = mutations[i].addedNodes;
        for (var j = 0; j < added.length; j++) {
          var node = added[j];
          if (node.nodeType !== 1) continue;
          if (node.tagName === 'VIDEO' || node.tagName === 'AUDIO') {
            wrapMediaElement(node);
          }
          if (node.querySelectorAll) {
            var nested = node.querySelectorAll('video, audio');
            for (var k = 0; k < nested.length; k++) wrapMediaElement(nested[k]);
          }
        }
      }
    });
    observer.observe(document.documentElement, { childList: true, subtree: true });
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

  function buildPanel() {
    var panel = document.createElement('div');
    panel.id = 'kite-volume-booster-panel';

    var label = document.createElement('div');
    label.id = 'kite-volume-booster-label';
    label.textContent = currentPercent + '%';

    var slider = document.createElement('input');
    slider.type = 'range';
    slider.min = String(MIN_PERCENT);
    slider.max = String(MAX_PERCENT);
    slider.step = '10';
    slider.value = String(currentPercent);
    slider.id = 'kite-volume-booster-slider';
    slider.addEventListener('input', function () {
      var v = parseInt(slider.value, 10);
      label.textContent = v + '%';
      applyPercentToAllNodes(v);
    });

    var resetBtn = document.createElement('button');
    resetBtn.type = 'button';
    resetBtn.id = 'kite-volume-booster-reset';
    resetBtn.textContent = 'Reset';
    resetBtn.addEventListener('click', function () {
      slider.value = String(DEFAULT_PERCENT);
      label.textContent = DEFAULT_PERCENT + '%';
      applyPercentToAllNodes(DEFAULT_PERCENT);
    });

    panel.appendChild(label);
    panel.appendChild(slider);
    panel.appendChild(resetBtn);
    document.body.appendChild(panel);
    return panel;
  }

  function addToggleButton() {
    if (document.getElementById('kite-volume-booster-toggle')) return;

    var style = document.createElement('style');
    style.textContent = [
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
      // Popup panel - anchored just to the left of the tray so it never
      // overlaps the tray icons themselves.
      '#kite-volume-booster-panel {',
      '  all: initial;',
      '  position: fixed;',
      '  top: 50%;',
      '  right: 52px;',
      '  transform: translateY(-50%);',
      '  z-index: 2147483647;',
      '  display: none;',
      '  flex-direction: column;',
      '  align-items: center;',
      '  gap: 6px;',
      '  padding: 10px 12px;',
      '  border-radius: 10px;',
      '  background: #1e1e1e;',
      '  color: #f2f2f2;',
      '  font-family: system-ui, sans-serif;',
      '  box-shadow: 0 2px 8px rgba(0,0,0,0.35);',
      '}',
      '#kite-volume-booster-panel.kite-volume-booster-open { display: flex; }',
      '#kite-volume-booster-label { font-size: 12px; }',
      '#kite-volume-booster-slider {',
      '  writing-mode: vertical-lr;',
      '  direction: rtl;',
      '  width: 20px;',
      '  height: 100px;',
      '  cursor: pointer;',
      '}',
      '#kite-volume-booster-reset {',
      '  all: initial;',
      '  font-size: 10px;',
      '  color: #ccc;',
      '  cursor: pointer;',
      '  padding: 2px 6px;',
      '  border-radius: 4px;',
      '}',
      '#kite-volume-booster-reset:hover { background: rgba(255,255,255,0.1); }',
    ].join('\n');
    document.head.appendChild(style);

    var panel = buildPanel();

    var btn = document.createElement('button');
    btn.id = 'kite-volume-booster-toggle';
    btn.type = 'button';
    btn.className = 'kite-ext-tray-btn';
    btn.textContent = '\u{1F50A}'; // speaker
    btn.title = 'Volume booster (currently ' + currentPercent + '%)';
    btn.addEventListener('click', function () {
      panel.classList.toggle('kite-volume-booster-open');
    });
    kiteGetToggleTray().appendChild(btn);

    // Close the panel when clicking elsewhere on the page.
    document.addEventListener('click', function (e) {
      if (e.target !== btn && !panel.contains(e.target)) {
        panel.classList.remove('kite-volume-booster-open');
      }
    });
  }

  ensureResumedOnUserGesture();
  wrapExistingMediaElements();
  watchForNewMediaElements();

  if (document.body) {
    addToggleButton();
  } else {
    document.addEventListener('DOMContentLoaded', addToggleButton, { once: true });
  }
})();
