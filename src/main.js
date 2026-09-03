const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

console.log("[kite] main.js loaded, __TAURI__ present:", !!window.__TAURI__);

const chromeEl = document.querySelector(".chrome");
const tabBar = document.getElementById("tab-bar");
const newTabBtn = document.getElementById("new-tab-btn");
const addressForm = document.getElementById("address-form");
const addressInput = document.getElementById("address-input");
const backBtn = document.getElementById("back-btn");
const forwardBtn = document.getElementById("forward-btn");
const reloadBtn = document.getElementById("reload-btn");
const homeBtn = document.getElementById("home-btn");
const starBtn = document.getElementById("star-btn");
const libraryBtn = document.getElementById("library-btn");
const zoomOutBtn = document.getElementById("zoom-out-btn");
const zoomInBtn = document.getElementById("zoom-in-btn");
const zoomLevelEl = document.getElementById("zoom-level");
const shieldBadgeEl = document.getElementById("shield-badge");
const shieldCountEl = document.getElementById("shield-count");

const bookmarksBar = document.getElementById("bookmarks-bar");
const bookmarksBarList = document.getElementById("bookmarks-bar-list");
const bookmarksBarEmpty = document.getElementById("bookmarks-bar-empty");
const loginSavePrompt = document.getElementById("login-save-prompt");
const loginSavePromptText = document.getElementById("login-save-prompt-text");
const loginSavePromptError = document.getElementById("login-save-prompt-error");
const loginSavePromptSaveBtn = document.getElementById("login-save-prompt-save-btn");
const loginSavePromptDismissBtn = document.getElementById("login-save-prompt-dismiss-btn");
const loginSavePromptCloseBtn = document.getElementById("login-save-prompt-close-btn");
const loginSavePromptActions = document.getElementById("login-save-prompt-actions");
const loginSavePromptUnlockForm = document.getElementById("login-save-prompt-unlock");
const loginSavePromptPasswordInput = document.getElementById("login-save-prompt-password");
const loginSavePromptUnlockCancelBtn = document.getElementById("login-save-prompt-unlock-cancel-btn");
const autofillPromptText = document.getElementById("autofill-prompt-text");
const autofillPromptError = document.getElementById("autofill-prompt-error");
const autofillPromptSelect = document.getElementById("autofill-prompt-select");
const autofillPromptFillBtn = document.getElementById("autofill-prompt-fill-btn");
const autofillPromptDismissBtn = document.getElementById("autofill-prompt-dismiss-btn");
const autofillPromptCloseBtn = document.getElementById("autofill-prompt-close-btn");

const libraryPanel = document.getElementById("library");
const libraryBackBtn = document.getElementById("library-back-btn");
const showHistoryBtn = document.getElementById("show-history-btn");
const showBookmarksBtn = document.getElementById("show-bookmarks-btn");
const showDownloadsBtn = document.getElementById("show-downloads-btn");
const showPasswordsBtn = document.getElementById("show-passwords-btn");
const showSettingsBtn = document.getElementById("show-settings-btn");
const showExtensionsBtn = document.getElementById("show-extensions-btn");
const clearHistoryBtn = document.getElementById("clear-history-btn");
const historyView = document.getElementById("history-view");
const bookmarksView = document.getElementById("bookmarks-view");
const downloadsView = document.getElementById("downloads-view");
const passwordsView = document.getElementById("passwords-view");
const settingsView = document.getElementById("settings-view");
const extensionsView = document.getElementById("extensions-view");
const extensionsList = document.getElementById("extensions-list");
const extensionsEmpty = document.getElementById("extensions-empty");
const extensionsReloadBtn = document.getElementById("extensions-reload-btn");
const extensionsOpenFolderBtn = document.getElementById("extensions-open-folder-btn");
const vaultSetupState = document.getElementById("vault-setup-state");
const vaultUnlockState = document.getElementById("vault-unlock-state");
const vaultUnlockedState = document.getElementById("vault-unlocked-state");
const vaultLoginsList = document.getElementById("vault-logins-list");
const vaultLoginsEmpty = document.getElementById("vault-logins-empty");
const vaultCreateForm = document.getElementById("vault-create-form");
const vaultCreatePassword = document.getElementById("vault-create-password");
const vaultCreateConfirm = document.getElementById("vault-create-confirm");
const vaultCreateError = document.getElementById("vault-create-error");
const vaultUnlockForm = document.getElementById("vault-unlock-form");
const vaultUnlockPassword = document.getElementById("vault-unlock-password");
const vaultUnlockError = document.getElementById("vault-unlock-error");
const vaultLockBtn = document.getElementById("vault-lock-btn");
const vaultAddCardBtn = document.getElementById("vault-add-card-btn");
const vaultCardForm = document.getElementById("vault-card-form");
const vaultCardOriginalLabel = document.getElementById("vault-card-original-label");
const vaultCardLabel = document.getElementById("vault-card-label");
const vaultCardName = document.getElementById("vault-card-name");
const vaultCardNumber = document.getElementById("vault-card-number");
const vaultCardMonth = document.getElementById("vault-card-month");
const vaultCardYear = document.getElementById("vault-card-year");
const vaultCardCvv = document.getElementById("vault-card-cvv");
const vaultCardError = document.getElementById("vault-card-error");
const vaultCardCancelBtn = document.getElementById("vault-card-cancel-btn");
const vaultCardsList = document.getElementById("vault-cards-list");
const vaultCardsEmpty = document.getElementById("vault-cards-empty");
const vaultAddAddressBtn = document.getElementById("vault-add-address-btn");
const vaultAddressForm = document.getElementById("vault-address-form");
const vaultAddressOriginalLabel = document.getElementById("vault-address-original-label");
const vaultAddressLabel = document.getElementById("vault-address-label");
const vaultAddressName = document.getElementById("vault-address-name");
const vaultAddressLine1 = document.getElementById("vault-address-line1");
const vaultAddressLine2 = document.getElementById("vault-address-line2");
const vaultAddressCity = document.getElementById("vault-address-city");
const vaultAddressState = document.getElementById("vault-address-state");
const vaultAddressPostal = document.getElementById("vault-address-postal");
const vaultAddressCountry = document.getElementById("vault-address-country");
const vaultAddressPhone = document.getElementById("vault-address-phone");
const vaultAddressError = document.getElementById("vault-address-error");
const vaultAddressCancelBtn = document.getElementById("vault-address-cancel-btn");
const vaultAddressesList = document.getElementById("vault-addresses-list");
const vaultAddressesEmpty = document.getElementById("vault-addresses-empty");
const settingsSearchEngine = document.getElementById("settings-search-engine");
const settingsHomepageRadios = document.querySelectorAll('input[name="settings-homepage-mode"]');
const settingsHomepageUrl = document.getElementById("settings-homepage-url");
const settingsDownloadsPath = document.getElementById("settings-downloads-path");
const settingsChooseDownloadsBtn = document.getElementById("settings-choose-downloads-btn");
const settingsContentBlocking = document.getElementById("settings-content-blocking");
const settingsClearDataBtn = document.getElementById("settings-clear-data-btn");
const settingsBlocklistStatus = document.getElementById("settings-blocklist-status");
const settingsRefreshBlocklistBtn = document.getElementById("settings-refresh-blocklist-btn");
const historyList = document.getElementById("history-list");
const bookmarksList = document.getElementById("bookmarks-list");
const downloadsList = document.getElementById("downloads-list");
const historyEmpty = document.getElementById("history-empty");
const bookmarksEmpty = document.getElementById("bookmarks-empty");
const downloadsEmpty = document.getElementById("downloads-empty");

let activeLabel = null;
let activeTitle = "";
// tab_label -> { host, username } - populated by the "login-capture-
// available" listener, drained by Save/Not now/close. Deliberately never
// holds a password (see LoginCapturePayload's own comment in main.rs for
// why) - this is only ever enough to render the prompt's text.
let pendingLoginPrompts = {};
// tab_label -> { host, usernames } - populated by the "autofill-available"
// listener, drained by Fill/Not now/close. Passwords never touch this -
// vault_autofill decrypts and fills directly from Rust; this only ever
// holds enough to render the prompt and the account picker.
let pendingAutofillPrompts = {};
// The active tab's actual URL, kept separate from addressInput.value -
// the address bar blanks itself out on the Kite home page (see the
// url-changed listener below), but star/bookmark logic still needs the
// real URL regardless of what's displayed.
let activeUrl = "";
let bookmarkedUrls = new Set();

const DEFAULT_FAVICON =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%236b7690' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Ccircle cx='12' cy='12' r='9'/%3E%3Cline x1='3' y1='12' x2='21' y2='12'/%3E%3Cpath d='M12 3a14.5 14.5 0 0 1 0 18a14.5 14.5 0 0 1 0-18'/%3E%3C/svg%3E";

// Shared by the tab bar, bookmarks bar, and library panel's history/
// bookmarks lists - falls back to the generic globe placeholder when
// there's no favicon yet (or the fetch failed), and again if the real
// favicon URL itself fails to load as an <img>.
function makeFaviconImg(faviconDataUrl, className) {
  const img = document.createElement("img");
  img.className = className;
  img.src = faviconDataUrl || DEFAULT_FAVICON;
  img.alt = "";
  img.addEventListener("error", () => {
    if (img.src !== DEFAULT_FAVICON) {
      img.src = DEFAULT_FAVICON;
    }
  });
  return img;
}

// Small mask/glasses glyph shown on a private tab, right after its
// favicon - the tab's own background/text color shift (see .tab.private
// in styles.css) is the main signal, this is the "why is this tab dark"
// explanation at a glance.
const PRIVATE_BADGE_SVG =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6-10-6-10-6z'/%3E%3Ccircle cx='12' cy='12' r='2.5'/%3E%3C/svg%3E";

function renderTabs(tabs, active) {
  activeLabel = active;
  const activeTab = tabs.find((t) => t.label === active);
  activeTitle = activeTab ? activeTab.title : "";

  // A pending capture/offer belongs to a specific tab (and, once the tab
  // navigates elsewhere, a specific *page* within that tab) - drop
  // entries for tabs that have since closed, and separately drop ones
  // whose stored host no longer matches where the tab actually is now,
  // so a stale "Log in to X?" banner doesn't linger after navigating
  // away from X in the same tab.
  const liveLabels = new Set(tabs.map((t) => t.label));
  const tabHost = (label) => {
    const tab = tabs.find((t) => t.label === label);
    try {
      return tab ? new URL(tab.url).hostname : null;
    } catch {
      return null;
    }
  };
  [pendingLoginPrompts, pendingAutofillPrompts].forEach((map) => {
    Object.keys(map).forEach((label) => {
      if (!liveLabels.has(label) || tabHost(label) !== map[label].host) delete map[label];
    });
  });

  // Tints the toolbar/address bar (see .private-active in styles.css)
  // whenever the tab currently on screen is private - the tab strip
  // marker alone is easy to lose track of once you're focused on the
  // page itself, this keeps the signal visible the whole time you're
  // actually using the tab, not just when glancing at the strip.
  document.body.classList.toggle("private-active", !!(activeTab && activeTab.is_private));

  // Shield badge reflects the active tab's own blocked_count/
  // site_allowlisted - both pushed by the same tabs-changed event this
  // function already runs on for every other per-tab bit of chrome UI,
  // so no separate listener or invoke() is needed just to keep the badge
  // current.
  const isRealSite = !!(activeTab && activeTab.url && !activeTab.url.startsWith("kite://"));
  shieldBadgeEl.disabled = !isRealSite;

  if (!isRealSite) {
    shieldCountEl.textContent = "";
    shieldBadgeEl.classList.remove("active", "site-off");
    shieldBadgeEl.title = "No site to toggle blocking for";
  } else if (activeTab.site_allowlisted) {
    shieldCountEl.textContent = "Off";
    shieldBadgeEl.classList.remove("active");
    shieldBadgeEl.classList.add("site-off");
    shieldBadgeEl.title = "Tracker blocking is off for this site - click to turn it back on";
  } else {
    const blockedCount = activeTab.blocked_count || 0;
    shieldCountEl.textContent = blockedCount;
    shieldBadgeEl.classList.toggle("active", blockedCount > 0);
    shieldBadgeEl.classList.remove("site-off");
    const countPhrase =
      blockedCount === 1 ? "1 tracker blocked on this page" : `${blockedCount} trackers blocked on this page`;
    shieldBadgeEl.title = `${countPhrase} - click to turn off blocking for this site`;
  }

  [...tabBar.querySelectorAll(".tab")].forEach((el) => el.remove());

  tabs.forEach((tab) => {
    const el = document.createElement("div");
    el.className =
      "tab" + (tab.is_private ? " private" : "") + (tab.label === active ? " active" : "");
    el.dataset.label = tab.label;

    el.appendChild(makeFaviconImg(tab.favicon, "tab-favicon"));

    if (tab.is_private) {
      const badge = document.createElement("img");
      badge.className = "tab-private-badge";
      badge.src = PRIVATE_BADGE_SVG;
      badge.alt = "Private";
      badge.title = "Private tab";
      el.appendChild(badge);
    }

    const title = document.createElement("span");
    title.className = "tab-title";
    title.textContent = tab.title || "New Tab";
    el.appendChild(title);

    const close = document.createElement("button");
    close.className = "tab-close";
    close.textContent = "\u00D7";
    // Ctrl+W only ever closes the *active* tab, so only hint it there -
    // showing it on every tab's own close button would wrongly imply
    // Ctrl+W closes whichever one you're hovering.
    close.title = tab.label === active ? "Close tab (Ctrl+W)" : "Close tab";
    close.addEventListener("click", (e) => {
      e.stopPropagation();
      invoke("close_tab", { label: tab.label }).catch((err) =>
        console.error("close_tab failed:", err)
      );
    });
    el.appendChild(close);

    el.addEventListener("click", () => {
      if (tab.label !== activeLabel) {
        invoke("switch_tab", { label: tab.label }).catch((err) =>
          console.error("switch_tab failed:", err)
        );
      }
    });

    tabBar.insertBefore(el, newTabBtn);
  });

  updateTabPromptVisibility();
}

newTabBtn.addEventListener("click", () => {
  console.log("[kite] + clicked");
  invoke("new_tab", { url: null })
    .then((label) => console.log("[kite] new_tab succeeded:", label))
    .catch((err) => console.error("[kite] new_tab failed:", err));
});

// Right-click on the + button: a small custom popup rather than a native
// OS menu - this is the chrome webview's own plain HTML/CSS, same as
// every other piece of chrome UI, so there's no need for the native-menu
// plumbing report_context_menu/build_context_menu use for content tabs'
// right-click (that pipeline is specifically for content-* webviews and
// their link/image targets, not chrome's own controls).
let newTabMenuEl = null;

function closeNewTabMenu() {
  if (newTabMenuEl) {
    newTabMenuEl.remove();
    newTabMenuEl = null;
    document.removeEventListener("click", closeNewTabMenu, true);
    document.removeEventListener("keydown", onNewTabMenuKeydown, true);
  }
}

function onNewTabMenuKeydown(e) {
  if (e.key === "Escape") closeNewTabMenu();
}

function openNewTab(private_) {
  invoke("new_tab", { url: null, private: private_ })
    .then((label) => console.log("[kite] new_tab succeeded:", label))
    .catch((err) => console.error("[kite] new_tab failed:", err));
}

newTabBtn.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  closeNewTabMenu();

  const menu = document.createElement("div");
  menu.className = "new-tab-menu";

  const regularItem = document.createElement("button");
  regularItem.className = "new-tab-menu-item";
  regularItem.textContent = "New Tab";
  regularItem.addEventListener("click", () => {
    closeNewTabMenu();
    openNewTab(false);
  });
  menu.appendChild(regularItem);

  const privateItem = document.createElement("button");
  privateItem.className = "new-tab-menu-item private";
  privateItem.textContent = "New Private Tab";
  privateItem.addEventListener("click", () => {
    closeNewTabMenu();
    openNewTab(true);
  });
  menu.appendChild(privateItem);

  document.body.appendChild(menu);

  // Clamp so the menu can't render partly off-screen - the + button sits
  // near the left edge of the tab bar, so only the right/bottom edges
  // realistically need checking here.
  const menuRect = menu.getBoundingClientRect();
  const x = Math.min(e.clientX, window.innerWidth - menuRect.width - 8);
  const y = Math.min(e.clientY, window.innerHeight - menuRect.height - 8);
  menu.style.left = `${Math.max(8, x)}px`;
  menu.style.top = `${Math.max(8, y)}px`;

  newTabMenuEl = menu;
  // Capture phase + next tick: the contextmenu event itself would
  // otherwise immediately bubble into this same click-away listener if
  // attached synchronously on some platforms.
  setTimeout(() => {
    document.addEventListener("click", closeNewTabMenu, true);
    document.addEventListener("keydown", onNewTabMenuKeydown, true);
  }, 0);
});

// Chrome's own click-away listener above only ever sees clicks inside
// chrome's own webview - clicking into the actual page (a separate native
// child webview, see content_size/create_tab_webview in main.rs) never
// bubbles anything here. report_content_click (invoked from
// context_menu.js on every content-tab mousedown) closes that gap.
listen("content-clicked", () => {
  closeNewTabMenu();
}).then(() => console.log("[kite] listening for content-clicked"))
  .catch((err) => console.error("[kite] failed to listen content-clicked:", err));

addressForm.addEventListener("submit", (e) => {
  e.preventDefault();
  console.log("[kite] address form submitted");
  const value = addressInput.value.trim();
  if (!value) return;
  if (value === "kite://home" || value === "kite://newtab") {
    invoke("new_tab", { url: null }).catch((err) =>
      console.error("[kite] new_tab (home) failed:", err)
    );
  } else {
    invoke("navigate", { url: value })
      .then(() => console.log("[kite] navigate succeeded"))
      .catch((err) => console.error("[kite] navigate failed:", err));
  }
  addressInput.blur();
});

backBtn.addEventListener("click", () => {
  console.log("[kite] back clicked");
  invoke("go_back")
    .then(() => console.log("[kite] go_back succeeded"))
    .catch((err) => console.error("[kite] go_back failed:", err));
});

forwardBtn.addEventListener("click", () => {
  console.log("[kite] forward clicked");
  invoke("go_forward")
    .then(() => console.log("[kite] go_forward succeeded"))
    .catch((err) => console.error("[kite] go_forward failed:", err));
});

reloadBtn.addEventListener("click", () => {
  console.log("[kite] reload clicked");
  invoke("reload")
    .then(() => console.log("[kite] reload succeeded"))
    .catch((err) => console.error("[kite] reload failed:", err));
});

// Navigates the current tab to Home, in place - see go_home's comment in
// main.rs for why this isn't just a plain navigate() call.
homeBtn.addEventListener("click", () => {
  console.log("[kite] home clicked");
  invoke("go_home")
    .then(() => console.log("[kite] go_home succeeded"))
    .catch((err) => console.error("[kite] go_home failed:", err));
});

// --- Bookmark star ---

function updateStarState() {
  const isHome = activeUrl === "kite://home";
  starBtn.disabled = isHome;
  starBtn.classList.toggle("bookmarked", !isHome && bookmarkedUrls.has(activeUrl));
}

function refreshBookmarkedUrls() {
  return invoke("get_bookmarks")
    .then((bookmarks) => {
      bookmarkedUrls = new Set(bookmarks.map((b) => b.url));
      updateStarState();
      renderBookmarksBar(bookmarks);
      return bookmarks;
    })
    .catch((err) => {
      console.error("[kite] get_bookmarks failed:", err);
      return [];
    });
}

// Persistent bar just below the toolbar, shown on every page (unlike the
// old home-page quick-links grid this replaces) - see renderBookmarksBar
// for what fills it in, and refreshBookmarkedUrls above for the one place
// that keeps it (and the star icon) in sync with the star button, the
// library panel's remove button, and startup.
function renderBookmarksBar(bookmarks) {
  bookmarksBarList.innerHTML = "";
  bookmarksBarEmpty.hidden = bookmarks.length > 0;
  bookmarks.forEach((bookmark) => {
    const item = document.createElement("div");
    item.className = "bookmarks-bar-item";
    item.title = bookmark.url;

    item.appendChild(makeFaviconImg(bookmark.favicon, "bookmarks-bar-item-favicon"));

    const label = document.createElement("span");
    label.className = "bookmarks-bar-item-label";
    label.textContent = bookmark.title || bookmark.url;
    item.appendChild(label);

    item.addEventListener("click", () => goToUrlFromLibrary(bookmark.url));
    bookmarksBarList.appendChild(item);
  });
}

// Shows/hides whichever of the save-password prompt or the autofill
// prompt applies to the currently active tab - called on every
// renderTabs (so switching tabs updates things immediately), after
// either "*-available" event, and after any action that clears one of
// the two pending maps. The two share the bookmarks-bar row (see the
// markup comment in index.html) and are mutually exclusive: a save
// offer always wins if both are somehow pending for the same tab, since
// it's the more time-sensitive of the two (a submit just happened).
function updateTabPromptVisibility() {
  const saveInfo = pendingLoginPrompts[activeLabel];
  const autofillInfo = saveInfo ? null : pendingAutofillPrompts[activeLabel];

  bookmarksBar.classList.toggle("login-prompt-active", !!saveInfo);
  bookmarksBar.classList.toggle("autofill-prompt-active", !!autofillInfo);
  hideLoginPromptUnlockForm();

  if (saveInfo) {
    loginSavePromptError.textContent = "";
    loginSavePromptText.textContent = saveInfo.username
      ? `Save password for ${saveInfo.host}? (${saveInfo.username})`
      : `Save password for ${saveInfo.host}?`;
    return;
  }

  if (autofillInfo) {
    autofillPromptError.textContent = "";
    autofillPromptText.textContent = `Log in to ${autofillInfo.host}?`;
    autofillPromptSelect.innerHTML = "";
    const multiple = autofillInfo.usernames.length > 1;
    autofillPromptSelect.hidden = !multiple;
    if (multiple) {
      autofillInfo.usernames.forEach((username) => {
        const option = document.createElement("option");
        option.value = username;
        option.textContent = username || "(no username)";
        autofillPromptSelect.appendChild(option);
      });
    }
  }
}

listen("login-capture-available", (event) => {
  const { tab_label, host, username } = event.payload;
  pendingLoginPrompts[tab_label] = { host, username };
  updateTabPromptVisibility();
}).catch((err) => console.error("[kite] failed to listen login-capture-available:", err));

listen("autofill-available", (event) => {
  const { tab_label, host, usernames } = event.payload;
  pendingAutofillPrompts[tab_label] = { host, usernames };
  updateTabPromptVisibility();
}).catch((err) => console.error("[kite] failed to listen autofill-available:", err));

// Swaps the prompt's normal Save/Not now/x row for a small inline
// "enter master password" form - used when Save is clicked but the
// vault turns out to be locked, so saving doesn't require a separate
// trip to the Passwords view first. See vault_unlock_and_save_login's
// own comment in main.rs.
function showLoginPromptUnlockForm() {
  loginSavePromptError.textContent = "";
  loginSavePromptActions.hidden = true;
  loginSavePromptUnlockForm.hidden = false;
  loginSavePromptPasswordInput.value = "";
  loginSavePromptPasswordInput.focus();
}

function hideLoginPromptUnlockForm() {
  loginSavePromptUnlockForm.hidden = true;
  loginSavePromptActions.hidden = false;
}

function saveLoginForTab(label) {
  invoke("vault_save_login", { tabLabel: label })
    .then(() => {
      delete pendingLoginPrompts[label];
      updateTabPromptVisibility();
    })
    .catch((err) => {
      loginSavePromptError.textContent = String(err);
    });
}

loginSavePromptSaveBtn.addEventListener("click", () => {
  const label = activeLabel;
  // Only ask vault_status first (rather than just calling vault_save_login
  // and reacting to its error) so a locked vault can go straight to the
  // inline unlock form instead of flashing an error message first.
  invoke("vault_status")
    .then((status) => {
      if (status.unlocked) {
        saveLoginForTab(label);
      } else if (status.exists) {
        showLoginPromptUnlockForm();
      } else {
        loginSavePromptError.textContent = "No vault yet - create one in Passwords first.";
      }
    })
    .catch((err) => console.error("[kite] vault_status failed:", err));
});

loginSavePromptUnlockForm.addEventListener("submit", (e) => {
  e.preventDefault();
  const label = activeLabel;
  const password = loginSavePromptPasswordInput.value;
  loginSavePromptError.textContent = "";
  invoke("vault_unlock_and_save_login", { masterPassword: password, tabLabel: label })
    .then(() => {
      loginSavePromptPasswordInput.value = "";
      delete pendingLoginPrompts[label];
      updateTabPromptVisibility();
    })
    .catch((err) => {
      loginSavePromptError.textContent = String(err);
    });
});

loginSavePromptUnlockCancelBtn.addEventListener("click", hideLoginPromptUnlockForm);

function dismissActiveLoginPrompt() {
  const label = activeLabel;
  invoke("vault_dismiss_login", { tabLabel: label })
    .then(() => {
      delete pendingLoginPrompts[label];
      updateTabPromptVisibility();
    })
    .catch((err) => console.error("[kite] vault_dismiss_login failed:", err));
}

loginSavePromptDismissBtn.addEventListener("click", dismissActiveLoginPrompt);
loginSavePromptCloseBtn.addEventListener("click", dismissActiveLoginPrompt);

autofillPromptFillBtn.addEventListener("click", () => {
  const label = activeLabel;
  const info = pendingAutofillPrompts[label];
  if (!info) return;
  const username = info.usernames.length > 1 ? autofillPromptSelect.value : info.usernames[0];
  autofillPromptError.textContent = "";
  invoke("vault_autofill", { tabLabel: label, host: info.host, username })
    .then(() => {
      delete pendingAutofillPrompts[label];
      updateTabPromptVisibility();
    })
    .catch((err) => {
      autofillPromptError.textContent = String(err);
    });
});

function dismissActiveAutofillPrompt() {
  delete pendingAutofillPrompts[activeLabel];
  updateTabPromptVisibility();
}

autofillPromptDismissBtn.addEventListener("click", dismissActiveAutofillPrompt);
autofillPromptCloseBtn.addEventListener("click", dismissActiveAutofillPrompt);

starBtn.addEventListener("click", () => {
  const url = activeUrl;
  if (!url) return;
  if (bookmarkedUrls.has(url)) {
    invoke("remove_bookmark", { url })
      .then(() => refreshBookmarkedUrls())
      .catch((err) => console.error("[kite] remove_bookmark failed:", err));
  } else {
    invoke("add_bookmark", { url, title: activeTitle || url })
      .then(() => refreshBookmarkedUrls())
      .catch((err) => console.error("[kite] add_bookmark failed:", err));
  }
});

// --- Shield badge (per-site content blocking toggle) ---

// The backend decides what "the active site" means (the active tab's own
// current URL) and reloads the tab itself once the toggle lands - see
// toggle_site_allowlist's own comment for why this doesn't take a host
// parameter from here. The resulting tabs-changed push (via
// refresh_site_allowlisted_for_all_tabs) is what actually updates the
// badge's visual state; this handler is just the invoke() call.
shieldBadgeEl.addEventListener("click", () => {
  invoke("toggle_site_allowlist").catch((err) =>
    console.error("[kite] toggle_site_allowlist failed:", err)
  );
});

// --- Zoom ---

zoomOutBtn.addEventListener("click", () => {
  invoke("zoom_out").catch((err) => console.error("[kite] zoom_out failed:", err));
});

zoomInBtn.addEventListener("click", () => {
  invoke("zoom_in").catch((err) => console.error("[kite] zoom_in failed:", err));
});

zoomLevelEl.addEventListener("click", () => {
  invoke("zoom_reset").catch((err) => console.error("[kite] zoom_reset failed:", err));
});

listen("zoom-changed", (event) => {
  const percent = Math.round(event.payload * 100);
  zoomLevelEl.textContent = `${percent}%`;
}).catch((err) => console.error("[kite] failed to listen zoom-changed:", err));

// --- Library panel (history & bookmarks & downloads) ---

function formatVisitedAt(ms) {
  const d = new Date(ms);
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

// Opening the library now goes through navigate("kite://<page>") on the
// Rust side rather than calling show_library directly - that's what lets
// typing "kite://history" in the address bar do the same thing as the
// toolbar button, and keeps the address bar in sync either way.
function goToInternalPage(page) {
  invoke("navigate", { url: `kite://${page}` }).catch((err) =>
    console.error(`[kite] navigate to kite://${page} failed:`, err)
  );
}

function closeLibrary() {
  invoke("hide_library")
    .then(applyLibraryClosedUI)
    .catch((err) => console.error("[kite] hide_library failed:", err));
}

// Just the CSS/UI side of closing the library, with no invoke("hide_library")
// call - used by closeLibrary() above (after that invoke succeeds) and by
// the "library-closed" listener below (the Rust side already closed it
// there, e.g. as a side effect of switching tabs - see activate_tab's own
// comment - so invoking hide_library again would be redundant).
function applyLibraryClosedUI() {
  libraryPanel.classList.remove("open");
  chromeEl.classList.remove("library-mode");
}

let currentLibraryView = "history";

function switchLibraryView(view) {
  currentLibraryView = view;
  showHistoryBtn.classList.toggle("active", view === "history");
  showBookmarksBtn.classList.toggle("active", view === "bookmarks");
  showDownloadsBtn.classList.toggle("active", view === "downloads");
  showPasswordsBtn.classList.toggle("active", view === "passwords");
  showSettingsBtn.classList.toggle("active", view === "settings");
  showExtensionsBtn.classList.toggle("active", view === "extensions");
  historyView.classList.toggle("active", view === "history");
  bookmarksView.classList.toggle("active", view === "bookmarks");
  downloadsView.classList.toggle("active", view === "downloads");
  passwordsView.classList.toggle("active", view === "passwords");
  settingsView.classList.toggle("active", view === "settings");
  extensionsView.classList.toggle("active", view === "extensions");
  // Bookmarks, Passwords, Settings, and Extensions have no "clear all" of
  // their own.
  clearHistoryBtn.style.visibility = ["bookmarks", "passwords", "settings", "extensions"].includes(view)
    ? "hidden"
    : "visible";
}

function goToUrlFromLibrary(url) {
  invoke("navigate", { url })
    .then(() => closeLibrary())
    .catch((err) => console.error("[kite] navigate from library failed:", err));
}

function loadHistory() {
  invoke("get_history")
    .then((entries) => {
      historyList.innerHTML = "";
      historyEmpty.classList.toggle("visible", entries.length === 0);
      entries.forEach((entry) => {
        const li = document.createElement("li");
        li.className = "library-item";

        li.appendChild(makeFaviconImg(entry.favicon, "library-item-favicon"));

        const text = document.createElement("div");
        text.className = "library-item-text";

        const title = document.createElement("div");
        title.className = "library-item-title";
        title.textContent = entry.title || entry.url;
        text.appendChild(title);

        const url = document.createElement("div");
        url.className = "library-item-url";
        url.textContent = entry.url;
        text.appendChild(url);

        li.appendChild(text);

        const meta = document.createElement("div");
        meta.className = "library-item-meta";
        meta.textContent = formatVisitedAt(entry.visited_at);
        li.appendChild(meta);

        const remove = document.createElement("button");
        remove.className = "library-item-remove";
        remove.textContent = "\u00D7";
        remove.title = "Remove from history";
        remove.addEventListener("click", (e) => {
          e.stopPropagation();
          invoke("remove_history_entry", { visitedAt: entry.visited_at, url: entry.url })
            .then(loadHistory)
            .catch((err) => console.error("[kite] remove_history_entry failed:", err));
        });
        li.appendChild(remove);

        li.addEventListener("click", () => goToUrlFromLibrary(entry.url));
        historyList.appendChild(li);
      });
    })
    .catch((err) => console.error("[kite] get_history failed:", err));
}

function loadBookmarks() {
  invoke("get_bookmarks")
    .then((bookmarks) => {
      bookmarksList.innerHTML = "";
      bookmarksEmpty.classList.toggle("visible", bookmarks.length === 0);
      bookmarks.forEach((bookmark) => {
        const li = document.createElement("li");
        li.className = "library-item";

        li.appendChild(makeFaviconImg(bookmark.favicon, "library-item-favicon"));

        const text = document.createElement("div");
        text.className = "library-item-text";

        const title = document.createElement("div");
        title.className = "library-item-title";
        title.textContent = bookmark.title || bookmark.url;
        text.appendChild(title);

        const url = document.createElement("div");
        url.className = "library-item-url";
        url.textContent = bookmark.url;
        text.appendChild(url);

        li.appendChild(text);

        const remove = document.createElement("button");
        remove.className = "library-item-remove";
        remove.textContent = "\u00D7";
        remove.title = "Remove bookmark";
        remove.addEventListener("click", (e) => {
          e.stopPropagation();
          invoke("remove_bookmark", { url: bookmark.url })
            .then(() => refreshBookmarkedUrls())
            .then(loadBookmarks)
            .catch((err) => console.error("[kite] remove_bookmark failed:", err));
        });
        li.appendChild(remove);

        li.addEventListener("click", () => goToUrlFromLibrary(bookmark.url));
        bookmarksList.appendChild(li);
      });
    })
    .catch((err) => console.error("[kite] get_bookmarks failed:", err));
}

function loadDownloads() {
  invoke("get_downloads")
    .then((downloads) => {
      downloadsList.innerHTML = "";
      downloadsEmpty.classList.toggle("visible", downloads.length === 0);
      downloads.forEach((entry) => {
        const li = document.createElement("li");
        li.className = "library-item";

        const text = document.createElement("div");
        text.className = "library-item-text";

        const title = document.createElement("div");
        title.className = "library-item-title";
        title.textContent = entry.file_name;
        text.appendChild(title);

        const path = document.createElement("div");
        path.className = "library-item-url";
        path.textContent = entry.success ? entry.path : "Failed";
        text.appendChild(path);

        li.appendChild(text);

        const meta = document.createElement("div");
        meta.className = "library-item-meta";
        meta.textContent = formatVisitedAt(entry.completed_at);
        li.appendChild(meta);

        // Failed downloads (or the rare case WebView2 didn't report a
        // final path) have nothing on disk to open or reveal.
        if (entry.success && entry.path) {
          const openBtn = document.createElement("button");
          openBtn.className = "library-item-action";
          openBtn.textContent = "Open";
          openBtn.title = "Open file";
          openBtn.addEventListener("click", (e) => {
            e.stopPropagation();
            invoke("open_download", { path: entry.path }).catch((err) =>
              console.error("[kite] open_download failed:", err)
            );
          });
          li.appendChild(openBtn);

          const revealBtn = document.createElement("button");
          revealBtn.className = "library-item-action";
          revealBtn.textContent = "Show in Folder";
          revealBtn.title = "Show in folder";
          revealBtn.addEventListener("click", (e) => {
            e.stopPropagation();
            invoke("show_download_in_folder", { path: entry.path }).catch((err) =>
              console.error("[kite] show_download_in_folder failed:", err)
            );
          });
          li.appendChild(revealBtn);
        }

        const remove = document.createElement("button");
        remove.className = "library-item-remove";
        remove.textContent = "\u00D7";
        remove.title = "Remove from list";
        remove.addEventListener("click", (e) => {
          e.stopPropagation();
          invoke("remove_download_entry", {
            completedAt: entry.completed_at,
            url: entry.url,
          })
            .then(loadDownloads)
            .catch((err) => console.error("[kite] remove_download_entry failed:", err));
        });
        li.appendChild(remove);

        downloadsList.appendChild(li);
      });
    })
    .catch((err) => console.error("[kite] get_downloads failed:", err));
}

// --- Settings ---

// Loads current values from the Rust side each time the Settings view is
// opened, same as loadHistory/loadBookmarks/loadDownloads do for their own
// views.
function loadSettings() {
  invoke("get_settings")
    .then((settings) => {
      settingsSearchEngine.value = settings.search_engine;
      settingsHomepageRadios.forEach((radio) => {
        radio.checked = radio.value === settings.homepage_mode;
      });
      settingsHomepageUrl.value = settings.homepage_url;
      settingsHomepageUrl.disabled = settings.homepage_mode !== "custom";
      settingsDownloadsPath.value = settings.downloads_dir || "";
      settingsContentBlocking.checked = settings.content_blocking_enabled;
    })
    .catch((err) => console.error("[kite] get_settings failed:", err));

  invoke("get_blocklist_status")
    .then((status) => {
      settingsBlocklistStatus.textContent = formatBlocklistStatus(status);
    })
    .catch((err) => console.error("[kite] get_blocklist_status failed:", err));
}

// last_refresh/entry_count come from get_blocklist_status on load, or
// the "blocklist-refreshed" event payload after a manual check - same
// shape either way (see BlocklistStatus/BlocklistRefreshResult on the
// Rust side).
function formatBlocklistStatus(status) {
  if (!status.last_refresh) {
    return "Using the built-in blocklist (never checked for updates).";
  }
  const count = status.entry_count != null ? status.entry_count.toLocaleString() : "?";
  return `Updated ${formatVisitedAt(status.last_refresh)} \u00b7 ${count} entries`;
}

settingsSearchEngine.addEventListener("change", () => {
  invoke("set_search_engine", { engine: settingsSearchEngine.value }).catch((err) => {
    console.error("[kite] set_search_engine failed:", err);
    // Revert the dropdown to whatever's actually persisted, since the
    // change didn't take.
    loadSettings();
  });
});

settingsContentBlocking.addEventListener("change", () => {
  invoke("set_content_blocking", { enabled: settingsContentBlocking.checked }).catch((err) => {
    console.error("[kite] set_content_blocking failed:", err);
    loadSettings();
  });
});

// refresh_blocklist itself returns almost immediately (the actual fetch
// runs on a background thread on the Rust side - see its own comment),
// so the button's real feedback comes from the "blocklist-refreshed"
// event listener below, not this .then().
settingsRefreshBlocklistBtn.addEventListener("click", () => {
  settingsRefreshBlocklistBtn.disabled = true;
  settingsBlocklistStatus.textContent = "Checking for updates\u2026";
  invoke("refresh_blocklist").catch((err) => {
    console.error("[kite] refresh_blocklist failed:", err);
    settingsRefreshBlocklistBtn.disabled = false;
    settingsBlocklistStatus.textContent = "Couldn't start the update check.";
  });
});

listen("blocklist-refreshed", (event) => {
  const result = event.payload;
  settingsRefreshBlocklistBtn.disabled = false;
  if (result.success) {
    settingsBlocklistStatus.textContent = formatBlocklistStatus({
      last_refresh: result.refreshed_at,
      entry_count: result.entry_count,
    });
  } else {
    console.error("[kite] blocklist refresh failed:", result.error);
    settingsBlocklistStatus.textContent = "Update failed \u2014 still using the previous list.";
  }
}).catch((err) => console.error("[kite] failed to listen blocklist-refreshed:", err));

function commitHomepage(mode, url) {
  invoke("set_homepage", { mode, url }).catch((err) => {
    console.error("[kite] set_homepage failed:", err);
    // Revert to whatever's actually persisted, since the change didn't
    // take (e.g. an invalid custom URL).
    loadSettings();
  });
}

settingsHomepageRadios.forEach((radio) => {
  radio.addEventListener("change", () => {
    const mode = radio.value;
    settingsHomepageUrl.disabled = mode !== "custom";
    if (mode === "custom") {
      settingsHomepageUrl.focus();
      // Nothing to save yet until an actual URL is entered - see the
      // blur/Enter handling on settingsHomepageUrl below. Switching here
      // with an empty field would just fail set_homepage's validation.
      return;
    }
    commitHomepage(mode, "");
  });
});

function commitHomepageUrlIfCustom() {
  const customRadio = Array.from(settingsHomepageRadios).find((r) => r.value === "custom");
  if (!customRadio || !customRadio.checked) return;
  const url = settingsHomepageUrl.value.trim();
  if (!url) return;
  commitHomepage("custom", url);
}

settingsHomepageUrl.addEventListener("blur", commitHomepageUrlIfCustom);
settingsHomepageUrl.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    commitHomepageUrlIfCustom();
    settingsHomepageUrl.blur();
  }
});

settingsChooseDownloadsBtn.addEventListener("click", () => {
  invoke("choose_downloads_dir")
    .then((path) => {
      // null means the user cancelled the picker - leave the field as is.
      if (path) {
        settingsDownloadsPath.value = path;
      }
    })
    .catch((err) => console.error("[kite] choose_downloads_dir failed:", err));
});

// Reuses the same clear_history/clear_downloads commands the library
// panel's own "Clear all" button already calls - bookmarks aren't touched
// by either. There's no list here to visibly empty out (unlike History/
// Downloads' own Clear all), so the button gives its own brief "Cleared"
// feedback instead of just silently succeeding.
settingsClearDataBtn.addEventListener("click", () => {
  Promise.all([invoke("clear_history"), invoke("clear_downloads")])
    .then(() => {
      const original = settingsClearDataBtn.textContent;
      settingsClearDataBtn.textContent = "Cleared";
      settingsClearDataBtn.disabled = true;
      setTimeout(() => {
        settingsClearDataBtn.textContent = original;
        settingsClearDataBtn.disabled = false;
      }, 1500);
    })
    .catch((err) => console.error("[kite] clear browsing data failed:", err));
});

// --- Extensions ---

// Shared by loadExtensions (initial open / after a toggle) and the
// Reload button (which already gets the fresh list back from
// reload_extensions, so no second round trip is needed there).
function renderExtensionList(extensions) {
  extensionsList.innerHTML = "";
  extensionsEmpty.classList.toggle("visible", extensions.length === 0);
  extensions.forEach((ext) => {
    const li = document.createElement("li");
    li.className = "library-item";

    const text = document.createElement("div");
    text.className = "library-item-text";

    const title = document.createElement("div");
    title.className = "library-item-title";
    title.textContent = `${ext.name} (${ext.version})`;
    text.appendChild(title);

    const matches = document.createElement("div");
    matches.className = "library-item-url";
    matches.textContent = ext.matches.length ? ext.matches.join(", ") : "No sites matched";
    text.appendChild(matches);

    li.appendChild(text);

    // Same plain-checkbox convention as settings-content-blocking - no
    // new toggle-switch component needed for this.
    const label = document.createElement("label");
    label.className = "settings-radio";
    const toggle = document.createElement("input");
    toggle.type = "checkbox";
    toggle.checked = ext.enabled;
    const statusText = document.createTextNode(ext.enabled ? "Enabled" : "Disabled");
    toggle.addEventListener("change", () => {
      const nextEnabled = toggle.checked;
      invoke("set_extension_enabled", { id: ext.id, enabled: nextEnabled })
        .then(() => {
          statusText.textContent = nextEnabled ? "Enabled" : "Disabled";
        })
        .catch((err) => {
          console.error("[kite] set_extension_enabled failed:", err);
          toggle.checked = !nextEnabled; // revert on failure - label already matched the pre-click state, so no text change needed
        });
    });
    label.appendChild(toggle);
    label.appendChild(statusText);
    li.appendChild(label);

    extensionsList.appendChild(li);
  });
}

function loadExtensions() {
  invoke("list_extensions")
    .then(renderExtensionList)
    .catch((err) => console.error("[kite] list_extensions failed:", err));
}

extensionsOpenFolderBtn.addEventListener("click", () => {
  invoke("open_extensions_folder").catch((err) =>
    console.error("[kite] open_extensions_folder failed:", err)
  );
});

extensionsReloadBtn.addEventListener("click", () => {
  extensionsReloadBtn.disabled = true;
  extensionsReloadBtn.textContent = "Reloading\u2026";
  invoke("reload_extensions")
    .then(renderExtensionList)
    .catch((err) => console.error("[kite] reload_extensions failed:", err))
    .finally(() => {
      extensionsReloadBtn.disabled = false;
      extensionsReloadBtn.textContent = "Reload extensions";
    });
});

// Shows exactly one of the three vault-state sections based on
// vault_status, and clears any stale error text/inputs left over from a
// previous visit - called whenever the Passwords view opens, and again
// right after create/unlock/lock so the UI reflects the new state
// immediately rather than waiting for the next navigation.
function refreshVaultUI() {
  invoke("vault_status")
    .then((status) => {
      const state = status.unlocked ? "unlocked" : status.exists ? "locked" : "none";
      vaultSetupState.classList.toggle("active", state === "none");
      vaultUnlockState.classList.toggle("active", state === "locked");
      vaultUnlockedState.classList.toggle("active", state === "unlocked");
      if (state === "unlocked") {
        loadVaultLogins();
        loadVaultCards();
        loadVaultAddresses();
      }
    })
    .catch((err) => console.error("[kite] vault_status failed:", err));
}

// Renders the saved-logins list inside vault-unlocked-state. Passwords
// are never fetched here - only host/username (see VaultLoginSummary's
// own comment in main.rs) - a password is only ever requested from Rust
// the moment "Show" or "Copy" is actually clicked on its row.
function loadVaultLogins() {
  invoke("vault_list_logins")
    .then((logins) => {
      vaultLoginsList.innerHTML = "";
      vaultLoginsEmpty.classList.toggle("visible", logins.length === 0);
      logins.forEach((login) => {
        const li = document.createElement("li");
        li.className = "library-item vault-login-item";

        const text = document.createElement("div");
        text.className = "library-item-text";

        const host = document.createElement("div");
        host.className = "library-item-title";
        host.textContent = login.host;
        text.appendChild(host);

        const username = document.createElement("div");
        username.className = "library-item-url";
        username.textContent = login.username || "(no username detected)";
        text.appendChild(username);

        li.appendChild(text);

        const passwordField = document.createElement("span");
        passwordField.className = "vault-login-password";
        passwordField.textContent = "\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022";
        li.appendChild(passwordField);

        const actions = document.createElement("div");
        actions.className = "vault-login-actions";

        const showBtn = document.createElement("button");
        showBtn.type = "button";
        showBtn.className = "library-item-action";
        showBtn.textContent = "Show";
        let revealed = false;
        showBtn.addEventListener("click", () => {
          if (revealed) {
            passwordField.textContent = "\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022";
            showBtn.textContent = "Show";
            revealed = false;
            return;
          }
          invoke("vault_reveal_login", { host: login.host, username: login.username })
            .then((password) => {
              passwordField.textContent = password;
              showBtn.textContent = "Hide";
              revealed = true;
            })
            .catch((err) => console.error("[kite] vault_reveal_login failed:", err));
        });
        actions.appendChild(showBtn);

        const copyBtn = document.createElement("button");
        copyBtn.type = "button";
        copyBtn.className = "library-item-action";
        copyBtn.textContent = "Copy";
        copyBtn.addEventListener("click", () => {
          invoke("vault_copy_login_password", { host: login.host, username: login.username })
            .then(() => {
              copyBtn.textContent = "Copied";
              setTimeout(() => {
                copyBtn.textContent = "Copy";
              }, 1200);
            })
            .catch((err) => console.error("[kite] vault_copy_login_password failed:", err));
        });
        actions.appendChild(copyBtn);

        const deleteBtn = document.createElement("button");
        deleteBtn.type = "button";
        deleteBtn.className = "library-item-remove";
        deleteBtn.textContent = "\u00D7";
        deleteBtn.title = "Delete saved login";
        deleteBtn.addEventListener("click", () => {
          invoke("vault_delete_login", { host: login.host, username: login.username })
            .then(loadVaultLogins)
            .catch((err) => console.error("[kite] vault_delete_login failed:", err));
        });
        actions.appendChild(deleteBtn);

        li.appendChild(actions);
        vaultLoginsList.appendChild(li);
      });
    })
    .catch((err) => console.error("[kite] vault_list_logins failed:", err));
}

// --- Payment methods ---
//
// Same list-then-explicit-reveal shape as loadVaultLogins: the list never
// carries the full card number or CVV (see VaultCardSummary in main.rs),
// only enough to tell cards apart at a glance. Unlike logins there's no
// separate "Show" toggle on the row - clicking Edit is what fetches the
// full card (vault_reveal_card) and drops it into the form, since editing
// and revealing need the same data anyway and a card is edited far more
// often than a login's password is "shown".
function resetVaultCardForm() {
  vaultCardForm.reset();
  vaultCardOriginalLabel.value = "";
  vaultCardError.textContent = "";
}

function openVaultCardForm() {
  vaultCardForm.hidden = false;
  vaultCardLabel.focus();
}

function closeVaultCardForm() {
  vaultCardForm.hidden = true;
  resetVaultCardForm();
}

function loadVaultCards() {
  invoke("vault_list_cards")
    .then((cards) => {
      vaultCardsList.innerHTML = "";
      vaultCardsEmpty.classList.toggle("visible", cards.length === 0);
      cards.forEach((card) => {
        const li = document.createElement("li");
        li.className = "library-item vault-card-item";

        const text = document.createElement("div");
        text.className = "library-item-text";

        const label = document.createElement("div");
        label.className = "library-item-title";
        label.textContent = card.label;
        text.appendChild(label);

        const detail = document.createElement("div");
        detail.className = "library-item-url";
        detail.textContent = card.cardholder_name
          ? `${card.cardholder_name} \u00B7 exp ${card.expiry_month}/${card.expiry_year}`
          : `exp ${card.expiry_month}/${card.expiry_year}`;
        text.appendChild(detail);

        li.appendChild(text);

        const number = document.createElement("span");
        number.className = "vault-card-number";
        number.textContent = `\u2022\u2022\u2022\u2022 ${card.last4}`;
        li.appendChild(number);

        const actions = document.createElement("div");
        actions.className = "vault-login-actions";

        const editBtn = document.createElement("button");
        editBtn.type = "button";
        editBtn.className = "library-item-action";
        editBtn.textContent = "Edit";
        editBtn.addEventListener("click", () => {
          invoke("vault_reveal_card", { label: card.label })
            .then((full) => {
              resetVaultCardForm();
              vaultCardOriginalLabel.value = card.label;
              vaultCardLabel.value = full.label;
              vaultCardName.value = full.cardholder_name;
              vaultCardNumber.value = full.card_number;
              vaultCardMonth.value = full.expiry_month;
              vaultCardYear.value = full.expiry_year;
              vaultCardCvv.value = full.cvv;
              openVaultCardForm();
            })
            .catch((err) => console.error("[kite] vault_reveal_card failed:", err));
        });
        actions.appendChild(editBtn);

        const deleteBtn = document.createElement("button");
        deleteBtn.type = "button";
        deleteBtn.className = "library-item-remove";
        deleteBtn.textContent = "\u00D7";
        deleteBtn.title = "Delete saved card";
        deleteBtn.addEventListener("click", () => {
          invoke("vault_delete_card", { label: card.label })
            .then(loadVaultCards)
            .catch((err) => console.error("[kite] vault_delete_card failed:", err));
        });
        actions.appendChild(deleteBtn);

        li.appendChild(actions);
        vaultCardsList.appendChild(li);
      });
    })
    .catch((err) => console.error("[kite] vault_list_cards failed:", err));
}

vaultAddCardBtn.addEventListener("click", () => {
  resetVaultCardForm();
  openVaultCardForm();
});

vaultCardCancelBtn.addEventListener("click", closeVaultCardForm);

vaultCardForm.addEventListener("submit", (e) => {
  e.preventDefault();
  vaultCardError.textContent = "";
  const label = vaultCardLabel.value.trim();
  if (!label) {
    vaultCardError.textContent = "Label is required.";
    return;
  }
  if (!vaultCardNumber.value.trim()) {
    vaultCardError.textContent = "Card number is required.";
    return;
  }
  // Saving under a different label than the one this edit started from
  // (originalLabel) creates a new entry rather than renaming the old one
  // - label is the upsert key on the Rust side (see vault_save_card), so
  // an actual rename needs the old entry deleted separately.
  const originalLabel = vaultCardOriginalLabel.value;
  invoke("vault_save_card", {
    label,
    cardholderName: vaultCardName.value.trim(),
    cardNumber: vaultCardNumber.value.trim(),
    expiryMonth: vaultCardMonth.value.trim(),
    expiryYear: vaultCardYear.value.trim(),
    cvv: vaultCardCvv.value.trim(),
  })
    .then(() => {
      if (originalLabel && originalLabel !== label) {
        return invoke("vault_delete_card", { label: originalLabel });
      }
    })
    .then(() => {
      closeVaultCardForm();
      loadVaultCards();
    })
    .catch((err) => {
      vaultCardError.textContent = String(err);
    });
});

// --- Addresses ---
//
// Same edit-prefills-from-reveal pattern as cards above. Addresses aren't
// as sensitive as a card/password, but the list still only shows a
// summary (see VaultAddressSummary in main.rs) for the same "glance, not
// a dump" reasoning as vault_list_logins.
function resetVaultAddressForm() {
  vaultAddressForm.reset();
  vaultAddressOriginalLabel.value = "";
  vaultAddressError.textContent = "";
}

function openVaultAddressForm() {
  vaultAddressForm.hidden = false;
  vaultAddressLabel.focus();
}

function closeVaultAddressForm() {
  vaultAddressForm.hidden = true;
  resetVaultAddressForm();
}

function loadVaultAddresses() {
  invoke("vault_list_addresses")
    .then((addresses) => {
      vaultAddressesList.innerHTML = "";
      vaultAddressesEmpty.classList.toggle("visible", addresses.length === 0);
      addresses.forEach((address) => {
        const li = document.createElement("li");
        li.className = "library-item vault-address-item";

        const text = document.createElement("div");
        text.className = "library-item-text";

        const label = document.createElement("div");
        label.className = "library-item-title";
        label.textContent = address.label;
        text.appendChild(label);

        const detail = document.createElement("div");
        detail.className = "library-item-url";
        const cityCountry = [address.city, address.country].filter(Boolean).join(", ");
        detail.textContent = [address.full_name, cityCountry].filter(Boolean).join(" \u00B7 ");
        text.appendChild(detail);

        li.appendChild(text);

        const actions = document.createElement("div");
        actions.className = "vault-login-actions";

        const editBtn = document.createElement("button");
        editBtn.type = "button";
        editBtn.className = "library-item-action";
        editBtn.textContent = "Edit";
        editBtn.addEventListener("click", () => {
          invoke("vault_reveal_address", { label: address.label })
            .then((full) => {
              resetVaultAddressForm();
              vaultAddressOriginalLabel.value = address.label;
              vaultAddressLabel.value = full.label;
              vaultAddressName.value = full.full_name;
              vaultAddressLine1.value = full.address_line1;
              vaultAddressLine2.value = full.address_line2;
              vaultAddressCity.value = full.city;
              vaultAddressState.value = full.state;
              vaultAddressPostal.value = full.postal_code;
              vaultAddressCountry.value = full.country;
              vaultAddressPhone.value = full.phone;
              openVaultAddressForm();
            })
            .catch((err) => console.error("[kite] vault_reveal_address failed:", err));
        });
        actions.appendChild(editBtn);

        const deleteBtn = document.createElement("button");
        deleteBtn.type = "button";
        deleteBtn.className = "library-item-remove";
        deleteBtn.textContent = "\u00D7";
        deleteBtn.title = "Delete saved address";
        deleteBtn.addEventListener("click", () => {
          invoke("vault_delete_address", { label: address.label })
            .then(loadVaultAddresses)
            .catch((err) => console.error("[kite] vault_delete_address failed:", err));
        });
        actions.appendChild(deleteBtn);

        li.appendChild(actions);
        vaultAddressesList.appendChild(li);
      });
    })
    .catch((err) => console.error("[kite] vault_list_addresses failed:", err));
}

vaultAddAddressBtn.addEventListener("click", () => {
  resetVaultAddressForm();
  openVaultAddressForm();
});

vaultAddressCancelBtn.addEventListener("click", closeVaultAddressForm);

vaultAddressForm.addEventListener("submit", (e) => {
  e.preventDefault();
  vaultAddressError.textContent = "";
  const label = vaultAddressLabel.value.trim();
  if (!label) {
    vaultAddressError.textContent = "Label is required.";
    return;
  }
  const originalLabel = vaultAddressOriginalLabel.value;
  invoke("vault_save_address", {
    label,
    fullName: vaultAddressName.value.trim(),
    addressLine1: vaultAddressLine1.value.trim(),
    addressLine2: vaultAddressLine2.value.trim(),
    city: vaultAddressCity.value.trim(),
    state: vaultAddressState.value.trim(),
    postalCode: vaultAddressPostal.value.trim(),
    country: vaultAddressCountry.value.trim(),
    phone: vaultAddressPhone.value.trim(),
  })
    .then(() => {
      if (originalLabel && originalLabel !== label) {
        return invoke("vault_delete_address", { label: originalLabel });
      }
    })
    .then(() => {
      closeVaultAddressForm();
      loadVaultAddresses();
    })
    .catch((err) => {
      vaultAddressError.textContent = String(err);
    });
});

vaultCreateForm.addEventListener("submit", (e) => {
  e.preventDefault();
  vaultCreateError.textContent = "";
  const password = vaultCreatePassword.value;
  const confirm = vaultCreateConfirm.value;
  if (password.length < 8) {
    vaultCreateError.textContent = "Master password must be at least 8 characters.";
    return;
  }
  if (password !== confirm) {
    vaultCreateError.textContent = "Passwords don't match.";
    return;
  }
  invoke("vault_create", { masterPassword: password })
    .then(() => {
      vaultCreatePassword.value = "";
      vaultCreateConfirm.value = "";
      refreshVaultUI();
    })
    .catch((err) => {
      vaultCreateError.textContent = String(err);
    });
});

vaultUnlockForm.addEventListener("submit", (e) => {
  e.preventDefault();
  vaultUnlockError.textContent = "";
  const password = vaultUnlockPassword.value;
  invoke("vault_unlock", { masterPassword: password })
    .then(() => {
      vaultUnlockPassword.value = "";
      refreshVaultUI();
    })
    .catch((err) => {
      vaultUnlockError.textContent = String(err);
    });
});

vaultLockBtn.addEventListener("click", () => {
  invoke("vault_lock")
    .then(() => {
      closeVaultCardForm();
      closeVaultAddressForm();
      refreshVaultUI();
    })
    .catch((err) => console.error("[kite] vault_lock failed:", err));
});

libraryBtn.addEventListener("click", () => {
  console.log("[kite] library clicked");
  goToInternalPage("history");
});

libraryBackBtn.addEventListener("click", closeLibrary);
showHistoryBtn.addEventListener("click", () => goToInternalPage("history"));
showBookmarksBtn.addEventListener("click", () => goToInternalPage("bookmarks"));
showDownloadsBtn.addEventListener("click", () => goToInternalPage("downloads"));
showPasswordsBtn.addEventListener("click", () => goToInternalPage("passwords"));
showSettingsBtn.addEventListener("click", () => goToInternalPage("settings"));
showExtensionsBtn.addEventListener("click", () => goToInternalPage("extensions"));

clearHistoryBtn.addEventListener("click", () => {
  if (currentLibraryView === "downloads") {
    invoke("clear_downloads")
      .then(loadDownloads)
      .catch((err) => console.error("[kite] clear_downloads failed:", err));
  } else {
    invoke("clear_history")
      .then(loadHistory)
      .catch((err) => console.error("[kite] clear_history failed:", err));
  }
});

listen("open-library-view", (event) => {
  const view = event.payload; // "history" | "bookmarks" | "downloads" | "passwords" | "settings" | "extensions"
  console.log("[kite] open-library-view event received:", view);
  libraryPanel.classList.add("open");
  chromeEl.classList.add("library-mode");
  switchLibraryView(view);
  if (view === "history") {
    loadHistory();
  } else if (view === "bookmarks") {
    loadBookmarks();
  } else if (view === "downloads") {
    loadDownloads();
  } else if (view === "passwords") {
    refreshVaultUI();
  } else if (view === "settings") {
    loadSettings();
  } else if (view === "extensions") {
    loadExtensions();
  }
}).then(() => console.log("[kite] listening for open-library-view"))
  .catch((err) => console.error("[kite] failed to listen open-library-view:", err));

// Emitted by activate_tab on the Rust side when switching tabs (or
// opening a new one) closes an open Library Panel out from under the
// user - see activate_tab's own comment. The native side has already
// been closed by the time this fires; this just brings the chrome UI's
// own "is it open" state back in sync.
listen("library-closed", () => {
  applyLibraryClosedUI();
}).then(() => console.log("[kite] listening for library-closed"))
  .catch((err) => console.error("[kite] failed to listen library-closed:", err));

// Tab bar reflects whatever Rust says the current tab list/active tab is.
listen("tabs-changed", (event) => {
  console.log("[kite] tabs-changed event received:", event.payload);
  renderTabs(event.payload.tabs, event.payload.active);
}).then(() => console.log("[kite] listening for tabs-changed"))
  .catch((err) => console.error("[kite] failed to listen tabs-changed:", err));

// Address bar shows the active tab's URL - skip updating while the user
// is actively editing it, so we don't yank their in-progress edit away.
// The Kite home page is the exception: like a normal browser's new-tab
// page, its address bar stays blank (placeholder only) rather than
// showing the internal "kite://home" address.
listen("url-changed", (event) => {
  console.log("[kite] url-changed event received:", event.payload);
  activeUrl = event.payload;
  if (document.activeElement !== addressInput) {
    addressInput.value = activeUrl === "kite://home" ? "" : activeUrl;
  }
  updateStarState();
}).then(() => console.log("[kite] listening for url-changed"))
  .catch((err) => console.error("[kite] failed to listen url-changed:", err));

// --- Keyboard shortcuts ---
//
// Ctrl/Cmd+T, +Shift+T, +Shift+N, +W, +L, +D, +H, +F, +1-9, +Tab,
// +Shift+Tab are
// registered as OS-level global hotkeys on the Rust side
// (tauri-plugin-global-shortcut), so they work no matter which of Kite's
// webviews - chrome or a page's own content - currently has keyboard
// focus. The Rust handler emits a "shortcut" event naming the action;
// this just dispatches it to the same code paths the on-screen buttons
// (or, for tab switching/reopening, plain invoke calls with no on-screen
// equivalent yet) use.
//
// Escape (close the library panel) is handled locally instead, since it
// isn't registered as a global hotkey - it only makes sense while the
// panel already has focus in the chrome webview.

function runShortcutAction(action) {
  switch (action) {
    case "new_tab":
      invoke("new_tab", { url: null, private: false }).catch((err) =>
        console.error("[kite] new_tab (shortcut) failed:", err)
      );
      break;
    case "new_private_tab":
      invoke("new_tab", { url: null, private: true }).catch((err) =>
        console.error("[kite] new_tab (private, shortcut) failed:", err)
      );
      break;
    case "close_tab":
      if (activeLabel) {
        invoke("close_tab", { label: activeLabel }).catch((err) =>
          console.error("[kite] close_tab (shortcut) failed:", err)
        );
      }
      break;
    case "reopen_closed_tab":
      invoke("reopen_closed_tab").catch((err) =>
        console.error("[kite] reopen_closed_tab failed:", err)
      );
      break;
    case "focus_address":
      addressInput.focus();
      addressInput.select();
      break;
    case "toggle_bookmark":
      starBtn.click();
      break;
    case "open_history":
      goToInternalPage("history");
      break;
    case "toggle_tab_search":
      if (tabSearchOverlay.classList.contains("open")) {
        closeTabSearch();
      } else {
        openTabSearch();
      }
      break;
    case "go_back":
      backBtn.click();
      break;
    case "go_forward":
      forwardBtn.click();
      break;
    case "reload":
      reloadBtn.click();
      break;
    case "go_home":
      homeBtn.click();
      break;
    case "find_in_page":
      invoke("toggle_find_in_page").catch((err) =>
        console.error("[kite] toggle_find_in_page failed:", err)
      );
      break;
    case "zoom_in":
      invoke("zoom_in").catch((err) => console.error("[kite] zoom_in (shortcut) failed:", err));
      break;
    case "zoom_out":
      invoke("zoom_out").catch((err) => console.error("[kite] zoom_out (shortcut) failed:", err));
      break;
    case "zoom_reset":
      invoke("zoom_reset").catch((err) =>
        console.error("[kite] zoom_reset (shortcut) failed:", err)
      );
      break;
    case "switch_tab_last":
      invoke("activate_tab_at", { index: null }).catch((err) =>
        console.error("[kite] activate_tab_at (last) failed:", err)
      );
      break;
    case "switch_tab_next":
      invoke("cycle_active_tab", { forward: true }).catch((err) =>
        console.error("[kite] cycle_active_tab (next) failed:", err)
      );
      break;
    case "switch_tab_prev":
      invoke("cycle_active_tab", { forward: false }).catch((err) =>
        console.error("[kite] cycle_active_tab (prev) failed:", err)
      );
      break;
    default:
      // Ctrl+1-8: "switch_tab_1".."switch_tab_8" - a literal (1-based in
      // the shortcut, 0-based once it reaches activate_tab_at) tab
      // position, unlike switch_tab_last above.
      if (action.startsWith("switch_tab_")) {
        const n = parseInt(action.slice("switch_tab_".length), 10);
        if (Number.isInteger(n) && n >= 1) {
          invoke("activate_tab_at", { index: n - 1 }).catch((err) =>
            console.error("[kite] activate_tab_at failed:", err)
          );
          break;
        }
      }
      console.warn("[kite] unknown shortcut action:", action);
  }
}

listen("shortcut", (event) => {
  console.log("[kite] shortcut event received:", event.payload);
  runShortcutAction(event.payload);
}).then(() => console.log("[kite] listening for shortcut"))
  .catch((err) => console.error("[kite] failed to listen shortcut:", err));

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && libraryPanel.classList.contains("open")) {
    closeLibrary();
  }
});

// ---------------------------------------------------------------------
// Tab search / command palette (Ctrl+K)
//
// Filters the live tab list and switches/closes tabs via the same
// switch_tab/close_tab commands the tab bar itself uses. Opening/closing
// the overlay itself goes through show_tab_search/hide_tab_search - the
// overlay is plain HTML/CSS living inside the small chrome webview, which
// normally only covers the toolbar strip (the rest of the window is a
// separate native content webview on top of it), so without those two
// commands temporarily resizing chrome to fill the window - exactly what
// show_library/hide_library already do for the library panel - the
// overlay would only ever be visible within that thin strip. Ctrl+K
// itself is wired as a real global shortcut (see "shortcut" listener /
// runShortcutAction's "toggle_tab_search" case below), not a local
// document keydown listener, since a content tab (a different native
// webview with its own focus) would otherwise swallow the keystroke
// before it ever reached this document.
// ---------------------------------------------------------------------

document.getElementById("tab-search-btn").addEventListener("click", () => openTabSearch());

const tabSearchOverlay = document.getElementById("tab-search-overlay");
const tabSearchInput = document.getElementById("tab-search-input");
const tabSearchList = document.getElementById("tab-search-list");
const tabSearchEmpty = document.getElementById("tab-search-empty");

let tabSearchResults = []; // currently rendered/filtered tabs, in display order
let tabSearchSelectedIndex = 0;

function openTabSearch() {
  // Close the library panel first if it's open - only one overlay should
  // be usable at a time, and the tab search box floats above everything
  // including the library, which would look broken layered together.
  // closeLibrary() already invokes hide_library, so chrome is back to its
  // normal size before show_tab_search below takes it full-window again.
  if (libraryPanel.classList.contains("open")) closeLibrary();

  invoke("show_tab_search")
    .then(() => invoke("get_tabs"))
    .then(({ tabs }) => {
      tabSearchOverlay.classList.add("open");
      tabSearchInput.value = "";
      renderTabSearchResults(tabs, "");
      // Focus after the overlay is actually visible, not before - focusing
      // a display:none input silently no-ops in some WebView2 builds.
      requestAnimationFrame(() => tabSearchInput.focus());
    })
    .catch((err) => console.error("[kite] show_tab_search failed:", err));
}

function closeTabSearch() {
  tabSearchOverlay.classList.remove("open");
  invoke("hide_tab_search").catch((err) =>
    console.error("[kite] hide_tab_search failed:", err)
  );
}

function renderTabSearchResults(allTabs, query) {
  const q = query.trim().toLowerCase();
  tabSearchResults = q
    ? allTabs.filter(
        (t) =>
          (t.title || "").toLowerCase().includes(q) ||
          (t.url || "").toLowerCase().includes(q)
      )
    : allTabs;

  tabSearchSelectedIndex = 0;
  tabSearchList.innerHTML = "";
  tabSearchEmpty.classList.toggle("visible", tabSearchResults.length === 0);

  tabSearchResults.forEach((tab, i) => {
    const li = document.createElement("li");
    li.className = "tab-search-item" + (i === 0 ? " selected" : "");

    li.appendChild(makeFaviconImg(tab.favicon, "tab-search-item-favicon"));

    const text = document.createElement("div");
    text.className = "tab-search-item-text";
    const title = document.createElement("div");
    title.className = "tab-search-item-title";
    title.textContent = tab.title || tab.url || "New Tab";
    const url = document.createElement("div");
    url.className = "tab-search-item-url";
    url.textContent = tab.url === "kite://home" ? "" : tab.url;
    text.appendChild(title);
    text.appendChild(url);
    li.appendChild(text);

    const closeBtn = document.createElement("button");
    closeBtn.className = "tab-search-item-close";
    closeBtn.title = "Close tab";
    closeBtn.textContent = "\u2715";
    closeBtn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      // Chained, not fired in parallel: close_tab is async (it's Rust's
      // "async fn close_tab", awaiting webview teardown), so firing
      // get_tabs alongside it - rather than after it resolves - could
      // read the tab list before the close actually landed, showing the
      // just-closed tab still sitting there until some later action
      // happened to re-render. .catch() before .then() here means a
      // failed close_tab still refreshes the list rather than leaving it
      // stuck silently.
      invoke("close_tab", { label: tab.label })
        .catch((err) => console.error("[kite] close_tab (tab search) failed:", err))
        .then(() => invoke("get_tabs"))
        .then(({ tabs }) => renderTabSearchResults(tabs, tabSearchInput.value))
        .catch((err) => console.error("[kite] get_tabs (tab search) failed:", err));
    });
    li.appendChild(closeBtn);

    li.addEventListener("click", () => selectTabSearchResult(i));
    tabSearchList.appendChild(li);
  });
}

function selectTabSearchResult(index) {
  const tab = tabSearchResults[index];
  if (!tab) return;
  invoke("switch_tab", { label: tab.label }).catch((err) =>
    console.error("[kite] switch_tab (tab search) failed:", err)
  );
  closeTabSearch();
}

function moveTabSearchSelection(delta) {
  if (tabSearchResults.length === 0) return;
  const items = tabSearchList.querySelectorAll(".tab-search-item");
  items[tabSearchSelectedIndex]?.classList.remove("selected");
  tabSearchSelectedIndex =
    (tabSearchSelectedIndex + delta + tabSearchResults.length) % tabSearchResults.length;
  items[tabSearchSelectedIndex]?.classList.add("selected");
  items[tabSearchSelectedIndex]?.scrollIntoView({ block: "nearest" });
}

tabSearchInput.addEventListener("input", () => {
  invoke("get_tabs")
    .then(({ tabs }) => renderTabSearchResults(tabs, tabSearchInput.value))
    .catch((err) => console.error("[kite] get_tabs (tab search) failed:", err));
});

tabSearchInput.addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    moveTabSearchSelection(1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    moveTabSearchSelection(-1);
  } else if (e.key === "Enter") {
    e.preventDefault();
    selectTabSearchResult(tabSearchSelectedIndex);
  } else if (e.key === "Escape") {
    e.preventDefault();
    closeTabSearch();
  }
});

// Clicking the dimmed backdrop (anywhere outside the box) closes it, same
// convention as the new-tab-menu overlay elsewhere in this file.
tabSearchOverlay.addEventListener("click", (e) => {
  if (e.target === tabSearchOverlay) closeTabSearch();
});

// Note: Ctrl+K itself is NOT bound here. It's registered as a real OS-level
// global shortcut in Rust (so it fires no matter which webview - chrome or
// a content tab - currently has focus) and arrives via the "shortcut"
// event's "toggle_tab_search" case in runShortcutAction above.

// Prime the tab bar and address bar on load too - relying solely on the
// tabs-changed/url-changed events pushed from Rust risks losing the very
// first push, since app setup emits it before this script is guaranteed to
// have finished registering its listeners (Tauri doesn't replay events
// emitted before a listener exists). That's what made the home tab appear
// to only show up once a second tab was opened - the backend had already
// created it, the UI just never heard about it.
invoke("get_tabs")
  .then(({ tabs, active }) => {
    renderTabs(tabs, active);
    const activeTab = tabs.find((t) => t.label === active);
    activeUrl = activeTab ? activeTab.url : "";
    addressInput.value = activeUrl === "kite://home" ? "" : activeUrl;
    updateStarState();
  })
  .catch((err) => console.error("[kite] get_tabs failed:", err));

// Prime the star button state on load.
refreshBookmarkedUrls();
