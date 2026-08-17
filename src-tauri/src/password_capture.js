// Injected as an initialization script into every content-tab webview,
// alongside context_menu.js/favicon.js (see create_tab_webview in
// main.rs) - runs before the page's own scripts and re-runs automatically
// on every navigation. Reports candidate login credentials back to Rust
// via the narrowly-scoped report_login_submit command
// (capabilities/content.json), following the same require_content()
// pattern report_context_menu/report_favicon use. Rust decides what (if
// anything) happens with a report - nothing is saved to the vault yet at
// this phase; see report_login_submit's own comment.
//
// Coverage note: this only catches a real <form> "submit" event with a
// password field inside it - the large majority of login forms, but not
// sites whose "Sign in" button is wired to a JS click handler with no
// underlying <form>/submit at all. That's a known gap, not an oversight;
// covering it would mean guessing at arbitrary click handlers, which is a
// much noisier signal to detect reliably.
(function () {
  // Same reasoning as favicon.js: WebView2 runs initialization scripts in
  // every frame, and a login form in a cross-origin iframe (e.g. an
  // embedded SSO widget) isn't this page's own login, so only the
  // top-level frame is worth watching.
  if (window.top !== window.self) {
    return;
  }

  // Guards against reporting the exact same submission twice - some
  // frameworks re-dispatch or double-bind a submit handler, which would
  // otherwise fire this twice for one real login attempt. Deliberately
  // not persisted anywhere (module-scoped var, reset on navigation since
  // the whole script re-runs), so a genuine second login attempt with the
  // same credentials later in the same page still reports normally.
  let lastReported = null;

  function findUsernameField(passwordField, form) {
    const scope = form || document;
    const candidates = Array.from(
      scope.querySelectorAll(
        "input:not([type=hidden]):not([type=checkbox]):not([type=radio]):not([type=submit]):not([type=button])"
      )
    ).filter((el) => el !== passwordField && el.type !== "password");

    const byAutocomplete = candidates.find((el) => el.autocomplete === "username");
    if (byAutocomplete) return byAutocomplete;

    // Otherwise prefer the field immediately preceding the password field
    // in document order - the common "username above password" layout.
    const before = candidates.filter(
      (el) => el.compareDocumentPosition(passwordField) & Node.DOCUMENT_POSITION_FOLLOWING
    );
    if (before.length) return before[before.length - 1];

    return candidates[0] || null;
  }

  function handleSubmit(form) {
    const passwordField = form.querySelector('input[type="password"]');
    if (!passwordField || !passwordField.value) return;

    const usernameField = findUsernameField(passwordField, form);
    const username = usernameField ? usernameField.value : "";
    const password = passwordField.value;

    const dedupeKey = username + "\u0000" + password.length;
    if (dedupeKey === lastReported) return;
    lastReported = dedupeKey;

    if (window.__TAURI__ && window.__TAURI__.core) {
      window.__TAURI__.core
        .invoke("report_login_submit", {
          host: window.location.hostname,
          username,
          password,
        })
        .catch((err) => console.error("[kite] report_login_submit failed:", err));
    }
  }

  // Capture phase, not bubble - runs before the page's own submit handler
  // gets a chance to call preventDefault()/stopPropagation(), and
  // certainly before the resulting navigation, so this always sees the
  // real submitted values regardless of what the page's own JS does next.
  document.addEventListener(
    "submit",
    (e) => {
      if (e.target && e.target.tagName === "FORM") {
        handleSubmit(e.target);
      }
    },
    true
  );

  // Autofill side: on page load, just report whether a login form is
  // present at all - Rust decides whether the (already-unlocked) vault
  // actually has anything saved for this host, and only then tells
  // chrome to offer it. Fires once per real navigation (this whole
  // script re-runs then, same as favicon.js/context_menu.js) - a form
  // added later by client-side JS on the same page load isn't caught,
  // same coverage gap as the submit-detection above.
  function reportLoginFormIfPresent() {
    if (!document.querySelector('input[type="password"]')) return;
    if (window.__TAURI__ && window.__TAURI__.core) {
      window.__TAURI__.core
        .invoke("report_login_form_present", { host: window.location.hostname })
        .catch((err) => console.error("[kite] report_login_form_present failed:", err));
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", reportLoginFormIfPresent, { once: true });
  } else {
    reportLoginFormIfPresent();
  }
})();