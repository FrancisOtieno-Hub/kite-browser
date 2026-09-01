// Kite YouTube Video Ad Blocking
//
// This does NOT block ad requests at the network layer - YouTube's video
// ads are increasingly served through the same CDN infrastructure as real
// video content specifically so host-based blocklists can't tell them
// apart without breaking playback. Instead this watches YouTube's own
// player state (the same "ad-showing" class SponsorBlock already checks)
// and reacts: auto-click Skip the instant it's clickable, close overlay
// banner ads, and for unskippable ads, run the video at a very high
// playbackRate so it blows through in a couple seconds instead of playing
// at normal speed. Original speed is restored the moment the ad ends.
//
// Known gap: some YouTube ads are server-side stitched directly into the
// video stream with no separate ad-player element at all - no class
// change on #movie_player, nothing distinguishable in the DOM. There is no
// client-side fix for that variant; this only handles client-inserted ads
// (the ones with visible Skip buttons / a distinct ad-playing UI state).
//
// The toggle button docks into the shared tray (#kite-ext-tray) at the
// viewport's right-middle edge - same pattern as dark-mode.js,
// cosmetic-adblock.js, and sponsorblock.js.

(function () {
  var DISABLED_KEY = 'kite-video-adblock-disabled';
  var FAST_RATE = 16;
  var POLL_MS = 400;

  var disabled = isDisabled();
  var adActive = false;
  var originalRate = 1;

  var SKIP_BUTTON_SELECTORS = [
    '.ytp-skip-ad-button',
    '.ytp-ad-skip-button-modern',
    '.ytp-ad-skip-button',
    'button.ytp-ad-skip-button-slot',
  ];
  var OVERLAY_CLOSE_SELECTOR = '.ytp-ad-overlay-close-button';

  function isDisabled() {
    try {
      return window.localStorage.getItem(DISABLED_KEY) === '1';
    } catch (e) {
      return false;
    }
  }

  function setDisabled(value) {
    try {
      if (value) {
        window.localStorage.setItem(DISABLED_KEY, '1');
      } else {
        window.localStorage.removeItem(DISABLED_KEY);
      }
    } catch (e) {
      // Toggle just won't survive a reload if this throws - not fatal.
    }
  }

  function clickSkipButtonIfPresent() {
    for (var i = 0; i < SKIP_BUTTON_SELECTORS.length; i++) {
      var btn = document.querySelector(SKIP_BUTTON_SELECTORS[i]);
      if (btn) {
        btn.click();
        return true;
      }
    }
    return false;
  }

  function closeOverlayIfPresent() {
    var btn = document.querySelector(OVERLAY_CLOSE_SELECTOR);
    if (btn) btn.click();
  }

  function handleAdTick(player) {
    if (disabled) return;
    var showing = player.classList.contains('ad-showing');
    var video = document.querySelector('video');

    if (showing) {
      if (!adActive) {
        adActive = true;
        if (video) originalRate = video.playbackRate || 1;
      }
      clickSkipButtonIfPresent();
      closeOverlayIfPresent();
      if (video) video.playbackRate = FAST_RATE;
    } else if (adActive) {
      adActive = false;
      if (video) video.playbackRate = originalRate;
    }
  }

  function attachToPlayer(player) {
    if (player.dataset.kiteAdblockAttached === '1') return;
    player.dataset.kiteAdblockAttached = '1';

    handleAdTick(player);

    var observer = new MutationObserver(function () {
      handleAdTick(player);
    });
    observer.observe(player, { attributes: true, attributeFilter: ['class'] });

    // The ad-showing class flips promptly, but the Skip button itself
    // becomes clickable a few seconds into the ad - polling while an ad is
    // active catches that moment without needing a MutationObserver on
    // the button's own (frequently-changing) container.
    setInterval(function () {
      if (!disabled && player.classList.contains('ad-showing')) {
        clickSkipButtonIfPresent();
        closeOverlayIfPresent();
      }
    }, POLL_MS);
  }

  function waitForPlayer(attemptsLeft) {
    var player = document.getElementById('movie_player');
    if (player) {
      attachToPlayer(player);
      return;
    }
    if (attemptsLeft <= 0) return;
    setTimeout(function () {
      waitForPlayer(attemptsLeft - 1);
    }, 250);
  }

  // Shared docking point for every Kite extension's toggle button.
  // Whichever extension's script runs first creates the tray; the rest
  // just append their own button into it.
  function kiteGetToggleTray() {
    var tray = document.getElementById('kite-ext-tray');
    if (tray) return tray;
    tray = document.createElement('div');
    tray.id = 'kite-ext-tray';
    document.body.appendChild(tray);
    return tray;
  }

  function addToggleButton() {
    if (document.getElementById('kite-video-adblock-toggle')) return;
    var style = document.createElement('style');
    style.textContent = [
      // Shared tray styling - see kiteGetToggleTray. Defined identically
      // in every extension that uses the tray; idempotent if duplicated.
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

    var btn = document.createElement('button');
    btn.id = 'kite-video-adblock-toggle';
    btn.type = 'button';
    btn.className = 'kite-ext-tray-btn';
    btn.textContent = disabled ? '\u{1F6AB}' : '\u{23E9}'; // no-entry : fast-forward
    btn.title = disabled
      ? 'Video ad skipping is off - click to turn back on'
      : 'Video ad skipping is on - click to turn off';
    btn.addEventListener('click', function () {
      disabled = !disabled;
      setDisabled(disabled);
      btn.textContent = disabled ? '\u{1F6AB}' : '\u{23E9}';
      btn.title = disabled
        ? 'Video ad skipping is off - click to turn back on'
        : 'Video ad skipping is on - click to turn off';
      if (disabled && adActive) {
        // Restore normal speed immediately rather than leaving a
        // mid-ad video stuck at 16x if toggled off during a fast-forward.
        var video = document.querySelector('video');
        if (video) video.playbackRate = originalRate;
        adActive = false;
      }
    });
    kiteGetToggleTray().appendChild(btn);
  }

  if (document.body) {
    addToggleButton();
  } else {
    document.addEventListener('DOMContentLoaded', addToggleButton, { once: true });
  }

  // Initial page load.
  waitForPlayer(40); // ~10s of retries

  // YouTube's SPA router event - #movie_player is normally reused across
  // client-side video navigations rather than recreated, so the existing
  // observer usually keeps working untouched. This is a safety net for
  // the case where it does get replaced.
  document.addEventListener('yt-navigate-finish', function () {
    waitForPlayer(20);
  });
})();
