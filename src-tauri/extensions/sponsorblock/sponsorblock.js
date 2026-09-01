// Kite SponsorBlock
//
// Two things make this different from dark-mode.js / cosmetic-adblock.js:
//
// 1. YouTube is a single-page app. Kite's script injection re-runs on real
//    navigations only (build_extension_script's guard fires per WebView2
//    navigation event), but clicking to another video on youtube.com is a
//    client-side history.pushState, not a real navigation - our script
//    would otherwise only ever see the first video of the session. YouTube
//    dispatches a "yt-navigate-finish" event on `document` when its own
//    router finishes a client-side transition, so we listen for that
//    ourselves instead of relying on being re-injected.
//
// 2. The SponsorBlock API call is a same-world fetch() (this extension
//    model runs in the page's own JS context, not an isolated content-
//    script world like a real browser extension), so it's subject to
//    YouTube's own CSP connect-src - confirmed working in testing (a 404
//    for a video with no submitted segments still reaches the API cleanly,
//    no CORS/blocked-resource error).
//
// The toggle button docks into a shared tray (#kite-ext-tray) at the
// viewport's right-middle edge rather than its own fixed corner - see
// kiteGetToggleTray, identical pattern to dark-mode.js and
// cosmetic-adblock.js. Sharing one tray is what fixed a real collision:
// SponsorBlock's old top-right pill was covering YouTube's own sign-in
// button.

(function () {
  var API_BASE = 'https://sponsor.ajay.app/api/skipSegments';
  var CATEGORIES = ['sponsor', 'selfpromo', 'interaction'];
  var DISABLED_KEY = 'kite-sponsorblock-disabled';
  var SKIP_EPSILON = 0.15; // seconds; avoids re-triggering right at a segment's own end

  var currentVideoId = null;
  var currentSegments = [];
  var currentVideoEl = null;
  var timeupdateHandler = null;
  var disabled = isDisabled();

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

  function getVideoIdFromUrl() {
    var path = location.pathname;
    if (path.indexOf('/shorts/') === 0) {
      return path.split('/')[2] || null;
    }
    var params = new URLSearchParams(location.search);
    return params.get('v');
  }

  function fetchSegments(videoId) {
    var url =
      API_BASE +
      '?videoID=' +
      encodeURIComponent(videoId) +
      '&categories=' +
      encodeURIComponent(JSON.stringify(CATEGORIES));

    fetch(url)
      .then(function (res) {
        // 404 from this API just means "no submitted segments for this
        // video" - not an error, just an empty result.
        if (res.status === 404) return [];
        if (!res.ok) throw new Error('SponsorBlock API status ' + res.status);
        return res.json();
      })
      .then(function (data) {
        if (videoId !== currentVideoId) return; // user already navigated away
        currentSegments = (data || []).map(function (entry) {
          return { start: entry.segment[0], end: entry.segment[1], category: entry.category };
        });
      })
      .catch(function (err) {
        // Swallow failures rather than break playback.
        console.warn('[kite-extension:sponsorblock] segment fetch failed', err);
        currentSegments = [];
      });
  }

  function isAdShowing() {
    var player = document.getElementById('movie_player');
    return !!(player && player.classList.contains('ad-showing'));
  }

  function showToast(category) {
    var toast = document.getElementById('kite-sponsorblock-toast');
    if (!toast) {
      toast = document.createElement('div');
      toast.id = 'kite-sponsorblock-toast';
      document.body.appendChild(toast);
    }
    toast.textContent = 'Skipped ' + category;
    toast.classList.add('kite-sponsorblock-toast-visible');
    clearTimeout(toast._hideTimer);
    toast._hideTimer = setTimeout(function () {
      toast.classList.remove('kite-sponsorblock-toast-visible');
    }, 1800);
  }

  function onTimeUpdate() {
    if (disabled || !currentVideoEl || isAdShowing()) return;
    var t = currentVideoEl.currentTime;
    for (var i = 0; i < currentSegments.length; i++) {
      var seg = currentSegments[i];
      if (t >= seg.start && t < seg.end - SKIP_EPSILON) {
        currentVideoEl.currentTime = seg.end;
        showToast(seg.category);
        break;
      }
    }
  }

  function attachToVideo(videoEl) {
    if (currentVideoEl && timeupdateHandler) {
      currentVideoEl.removeEventListener('timeupdate', timeupdateHandler);
    }
    currentVideoEl = videoEl;
    timeupdateHandler = onTimeUpdate;
    currentVideoEl.addEventListener('timeupdate', timeupdateHandler);
  }

  function waitForVideoElement(attemptsLeft, cb) {
    var el = document.querySelector('video');
    if (el) {
      cb(el);
      return;
    }
    if (attemptsLeft <= 0) return;
    setTimeout(function () {
      waitForVideoElement(attemptsLeft - 1, cb);
    }, 250);
  }

  function onPossibleVideoChange() {
    var id = getVideoIdFromUrl();
    if (!id || id === currentVideoId) return;
    currentVideoId = id;
    currentSegments = [];
    fetchSegments(id);
    waitForVideoElement(20, attachToVideo); // ~5s of retries; YouTube can take a moment to swap the element
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
    if (document.getElementById('kite-sponsorblock-toggle')) return;
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
      // SponsorBlock's button carries text ("SB: On"/"SB: Off") rather
      // than just an icon, since "SB" alone as a glyph isn't legible -
      // widen it slightly from the shared 32px icon-only circle.
      '#kite-sponsorblock-toggle {',
      '  width: auto;',
      '  min-width: 32px;',
      '  height: 28px;',
      '  padding: 0 10px;',
      '  border-radius: 999px;',
      '  font-size: 11px;',
      '}',
      // Toast moves to bottom-center: a transient status message is a
      // natural fit there, and it stays clear of the tray and of any
      // page chrome living in the corners.
      '#kite-sponsorblock-toast {',
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
      '#kite-sponsorblock-toast.kite-sponsorblock-toast-visible {',
      '  opacity: 0.92;',
      '  transform: translateX(-50%) translateY(0);',
      '}',
    ].join('\n');
    document.head.appendChild(style);

    var btn = document.createElement('button');
    btn.id = 'kite-sponsorblock-toggle';
    btn.type = 'button';
    btn.className = 'kite-ext-tray-btn';
    btn.textContent = disabled ? 'SB: Off' : 'SB: On';
    btn.title = 'Click to toggle automatic sponsor-segment skipping';
    btn.addEventListener('click', function () {
      disabled = !disabled;
      setDisabled(disabled);
      btn.textContent = disabled ? 'SB: Off' : 'SB: On';
    });
    kiteGetToggleTray().appendChild(btn);
  }

  if (document.body) {
    addToggleButton();
  } else {
    document.addEventListener('DOMContentLoaded', addToggleButton, { once: true });
  }

  // YouTube's own SPA router event - fires after every client-side
  // navigation (video changes, search-result clicks, etc).
  document.addEventListener('yt-navigate-finish', onPossibleVideoChange);

  // Defensive fallback in case yt-navigate-finish ever doesn't fire for a
  // given transition (YouTube has changed this event's behavior across
  // versions before) - cheap enough to poll every couple seconds.
  setInterval(onPossibleVideoChange, 2000);

  // Handle the very first load of this document (yt-navigate-finish only
  // fires for *subsequent* client-side transitions, not the initial page).
  onPossibleVideoChange();
})();
