// Injected as an initialization script into every content-tab webview,
// alongside context_menu.js / favicon.js (see create_tab_webview in main.rs):
// runs before the page's own scripts and again on every navigation.
//
// WHY THIS EXISTS
//
// Plenty of web apps open links with the "blank window first" trick:
//
//     var w = window.open('', '_blank');   // open an empty window now,
//     w.location = url;                    // set where it goes afterwards
//
// (done so the window opens inside the click, and popup blockers allow it).
// Kite turns new-window requests into tabs on the Rust side
// (handle_new_window_request), but that can only act on a window that has a
// URL when it is requested - WebView2 can't hand Kite a half-made window to
// fill in later. So the blank request is refused, window.open returns null,
// the page's next line throws, and the link appears to do nothing.
//
// This script covers exactly that case. For window.open('') /
// window.open('about:blank') it returns a stand-in window object; when the
// page then assigns an address to it, the address is opened as a normal Kite
// tab via the same narrow command the keyboard-nav extension uses
// (open_link_in_new_tab - content.json-allowed, require_content()-gated,
// http/https only, rate-limited per tab). Every other window.open call goes
// straight to the browser's own implementation untouched.
//
// What it deliberately doesn't do: it can't give a page a real second
// window, so a blank popup the page writes into (document.write for a print
// preview, an OAuth popup that is posted to by form target) stays unsupported
// - those never worked in Kite and still don't, they just no longer throw.
(function () {
  var nativeOpen = window.open;
  if (typeof nativeOpen !== 'function') return;

  function ipcAvailable() {
    return !!(window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);
  }

  // Resolve against the page and accept only http(s): the Rust side checks
  // again, this just avoids pointless IPC for javascript:/data:/blob: values.
  function resolveWebUrl(raw) {
    try {
      var u = new URL(String(raw), document.baseURI);
      return u.protocol === 'http:' || u.protocol === 'https:' ? u.href : null;
    } catch (e) {
      return null;
    }
  }

  function makeStandIn() {
    var closed = false;
    var opened = false; // only the first address becomes a tab
    var currentHref = 'about:blank';

    function go(raw) {
      if (closed || opened) return;
      var href = resolveWebUrl(raw);
      if (!href) return;
      try {
        window.__TAURI__.core.invoke('open_link_in_new_tab', { href: href }).catch(function (err) {
          console.warn('[kite] open_link_in_new_tab failed', err);
        });
        opened = true;
        currentHref = href;
      } catch (e) {
        console.warn('[kite] popup stand-in could not open tab', e);
      }
    }

    var loc = {
      assign: go,
      replace: go,
      reload: function () {},
      toString: function () {
        return currentHref;
      },
    };
    Object.defineProperty(loc, 'href', {
      get: function () {
        return currentHref;
      },
      set: go,
    });

    // A document stub so "loading..." placeholders written into the blank
    // window don't throw.
    var doc = {
      write: function () {},
      writeln: function () {},
      open: function () {
        return doc;
      },
      close: function () {},
      title: '',
      body: null,
      readyState: 'complete',
    };
    Object.defineProperty(doc, 'location', { get: function () { return loc; }, set: go });

    var fake = {
      document: doc,
      opener: null,
      name: '',
      close: function () {
        closed = true;
      },
      focus: function () {},
      blur: function () {},
      postMessage: function () {},
      print: function () {},
      addEventListener: function () {},
      removeEventListener: function () {},
    };
    Object.defineProperty(fake, 'location', { get: function () { return loc; }, set: go });
    Object.defineProperty(fake, 'closed', { get: function () { return closed; } });
    return fake;
  }

  window.open = function (url, target) {
    var u = url === undefined || url === null ? '' : String(url);
    var isBlank = u === '' || u === 'about:blank';
    // Only a *new* window is emulated. A named/self target ("_self", an
    // iframe's name, ...) means something else - leave that to the browser.
    var wantsNewWindow = target === undefined || target === null || target === '' || target === '_blank';
    if (isBlank && wantsNewWindow && ipcAvailable()) {
      return makeStandIn();
    }
    return nativeOpen.apply(this, arguments);
  };
})();
