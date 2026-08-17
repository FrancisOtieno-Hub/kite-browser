#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Kite Browser - Tabs, History, and Bookmarks
//
// Each browser tab is a separate content webview. Only the active tab's
// webview is positioned where it's visible; the rest are parked off-screen
// (we don't rely on a show/hide API here, since that's less certain to
// exist across all platform backends - moving a webview off-canvas is a
// simpler, more portable way to "hide" it for now).
//
// Tab state (list of tabs + which one is active) lives in a Mutex managed
// by Tauri, and the tab bar UI in index.html/main.js is just a reflection
// of whatever this side tells it via the "tabs-changed" and "url-changed"
// events.
//
// History and bookmarks live in a second Mutex-managed struct (AppData),
// persisted as a single JSON file in the OS app-data directory. They're
// written straight from Rust with std::fs - no Tauri fs-plugin permission
// is needed for that, since permissions only gate JS-invokable plugin
// commands, not what our own command handlers do internally.
//
// Note on new_tab/switch_tab/close_tab being `async fn`: creating a
// webview (window.add_child) needs to happen off the thread that's also
// pumping the window's own message loop, or WebView2's async controller
// creation can't complete (a Windows-specific COM/STA quirk). Plain sync
// #[tauri::command] functions run directly on the main thread in Tauri
// v2 - exactly the thread add_child needs free. Declaring these commands
// `async fn` moves their execution onto Tokio's worker pool instead,
// letting Tauri's own internal main-thread dispatch (built into
// add_child) do its job without our code occupying that thread already.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use base64::Engine;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::Argon2;
use rand::RngCore;
use zeroize::Zeroize;
use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuItem};
use tauri::webview::{DownloadEvent, WebviewBuilder};
use tauri::window::WindowBuilder;
use tauri::{Emitter, LogicalPosition, LogicalSize, Manager, Position, WebviewUrl};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use url::Url;

const CHROME_HEIGHT: f64 = 124.0; // tab bar row + toolbar row + bookmarks bar row combined
const MAIN_WEBVIEW_LABEL: &str = "chrome";
// The friendly "address" for Kite's own new-tab/home page. It isn't a real
// navigable URL - there's no http(s) resource behind it - so it's never
// passed to Url::parse/WebviewUrl::External. create_tab_webview special-
// cases it to load the bundled home.html asset instead, and on_navigation
// rewrites the resulting internal asset URL back to this for display/
// history purposes.
const HOME_URL: &str = "kite://home";
const BLANK_URL: &str = "about:blank";
// Passed to create_tab_webview to load the bundled crashed.html asset,
// exactly like HOME_URL does for home.html - see show_crashed_page. Not a
// real address a user ever sees or types; distinct from HOME_URL purely
// so create_tab_webview knows which bundled asset to load.
const CRASHED_ASSET_MARKER: &str = "kite-internal://crashed";
const OFFSCREEN_X: f64 = -10000.0;
const HISTORY_LIMIT: usize = 2000;
const DOWNLOAD_LIMIT: usize = 500;
const CLOSED_TABS_LIMIT: usize = 15;
const DATA_FILE_NAME: &str = "kite_data.json";
// Kept in its own file, separate from kite_data.json - the rest of
// PersistedData is written back to disk on essentially every action
// (navigation, history, favicons...), and there's no reason for that
// unrelated churn to ever touch the file holding encrypted vault
// material. See Vault* items below (search "password vault").
const PASSWORDS_FILE_NAME: &str = "kite_passwords.json";
const VAULT_SALT_LEN: usize = 16;
const VAULT_KEY_LEN: usize = 32; // AES-256
const VAULT_NONCE_LEN: usize = 12; // AES-GCM standard nonce size
// Encrypted under the derived key and stored alongside the (empty, for
// now) entry list. vault_unlock re-derives a key from whatever password
// it's given and tries to decrypt this - AES-GCM's authentication tag
// means that only succeeds, and only produces exactly this byte string
// back out, if the password was correct. This never touches the actual
// password or key on disk, just proof that a candidate key can open it.
const VAULT_VERIFIER_PLAINTEXT: &[u8] = b"kite-vault-v1";
// Self-contained find-in-page overlay, injected via eval() into the active
// content webview - see find_in_page.js for why (content tabs have zero
// Tauri IPC, so this can't call back into Rust and is built to not need to).
const FIND_IN_PAGE_SCRIPT: &str = include_str!("find_in_page.js");
// Right-click target detection, injected as an initialization script into
// every content-tab webview (see create_tab_webview) - runs before the
// page's own scripts and re-runs automatically on every navigation, unlike
// FIND_IN_PAGE_SCRIPT which is only eval'd on demand via Ctrl+F. Reports
// what was clicked back to Rust via the narrowly-scoped report_context_menu
// command (see require_content / capabilities/content.json).
const CONTEXT_MENU_SCRIPT: &str = include_str!("context_menu.js");
// Favicon detection, injected as an initialization script into every
// content-tab webview just like CONTEXT_MENU_SCRIPT - runs before the
// page's own scripts and re-runs on every navigation. Reports the
// detected icon URL back via the narrowly-scoped report_favicon command
// (see require_content / capabilities/content.json).
const FAVICON_SCRIPT: &str = include_str!("favicon.js");
// Login-form detection, injected as an initialization script alongside
// CONTEXT_MENU_SCRIPT/FAVICON_SCRIPT. Reports candidate credentials back
// via the narrowly-scoped report_login_submit command - see that
// command's own comment for what happens (and doesn't yet) with a report.
const PASSWORD_CAPTURE_SCRIPT: &str = include_str!("password_capture.js");

#[derive(Clone, Serialize)]
struct TabInfo {
    label: String,
    title: String,
    url: String,
    zoom: f64,
    // Set when this tab is showing the bundled crash-recovery page after
    // its content webview's render process died (see watch_for_crash /
    // show_crashed_page) - url deliberately keeps the real site's address
    // the whole time (see on_navigation's is_crash_page handling), so this
    // flag is the only way to tell "crashed" apart from a normal tab.
    #[serde(default)]
    crashed: bool,
    // Data URL (e.g. "data:image/png;base64,...") for the tab's favicon,
    // once fetched - see report_favicon/SharedFaviconCache. None until
    // then (or if fetching it ever fails), in which case the UI falls
    // back to a generic icon.
    #[serde(default)]
    favicon: Option<String>,
    // True for a private/incognito tab - see create_tab_webview's
    // .incognito() call for the actual cookie/cache isolation this backs
    // (a real non-persistent WebView2/WKWebView data store, not just a
    // UI label). Drives: skipping record_history/update_history_title,
    // skipping favicon persistence to disk, skipping downloads-list
    // persistence, exclusion from SessionState on quit, and the tab's
    // visual marker in the frontend.
    #[serde(default)]
    is_private: bool,
    // How many resource requests watch_for_requests has cancelled for
    // this tab's current page - reset to 0 on_navigation's own url update
    // below, so it always reflects "this page", not a running total
    // across every page the tab has ever visited.
    #[serde(default)]
    blocked_count: u32,
    // Whether the tab's current page host is in
    // PersistedData.content_blocking_allowlist - recomputed on every real
    // navigation (see on_navigation) and after every toggle_site_allowlist
    // call (see refresh_site_allowlisted_for_all_tabs), rather than
    // re-checked on every single resource request the way blocked-host
    // matching is - watch_for_requests just reads this cached flag.
    #[serde(default)]
    site_allowlisted: bool,
}

struct TabState {
    tabs: Vec<TabInfo>,
    active: String,
    next_id: u32,
    window_size: (f64, f64),
    // Which tab, if any, the Library Panel (History/Bookmarks/Downloads/
    // Settings) is currently associated with - None means it's fully
    // closed. The panel itself is a full-window takeover of the chrome
    // webview (see show_library_impl), not something that belongs to any
    // one tab's own content, so it needs its own tracking separate from
    // `active`. Whether it's actually *visible* right now is simply
    // `library_tab == Some(active)` - switching to a different tab parks
    // it (chrome shrinks back so the other tab can be seen normally)
    // without forgetting it's there; switching back to this exact tab reopens it
    // automatically. See activate_tab for where that's implemented.
    library_tab: Option<String>,
    // Which sub-view (history/bookmarks/downloads/settings) was last
    // shown - every switch between them already round-trips through
    // open_internal_page (see goToInternalPage in main.js, which always
    // calls navigate rather than switching views purely client-side), so
    // this stays accurate for free. Used by emit_active_url to restore
    // the right kite://<view> address-bar text when returning to a
    // parked library tab - default matches main.js's own default.
    library_view: String,
    // URLs of recently closed tabs, most-recent-last, for
    // reopen_closed_tab (Ctrl+Shift+T) - in-memory only, same as the
    // favicon fetch-dedup cache, since a "recently closed" list that
    // survives a full app restart isn't what that shortcut is for in any
    // browser. The home page is deliberately never pushed here (see
    // close_tab), same reasoning as it being kept out of history.
    closed_tabs: Vec<String>,
}

type SharedTabState = Mutex<TabState>;

// A Mutex is "poisoned" once some thread panics while holding its lock -
// after that, every .lock().unwrap() anywhere else in the app that
// touches the SAME mutex panics too, forever, even in commands totally
// unrelated to whatever originally panicked. One bad command taking down
// every other command that happens to share this state isn't acceptable
// for a browser people are actively using - a stale-but-present tab list
// is a far better failure mode than the whole app becoming unusable.
// Since our shared state here is just plain owned data with no
// invariants a panic mid-update could leave subtly broken in a way that
// matters (nothing here is, say, a partially-written file on disk), it's
// safe to just recover the guard and carry on with whatever's in it
// rather than propagating the poison.
trait LockExt<T> {
    fn lock_recover(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_recover(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

// Same poison-resilience reasoning as LockExt above, for the one RwLock
// in the app (BLOCKLIST, see below) - a refresh panicking mid-swap
// shouldn't take every future request-check down with it.
trait RwLockExt<T> {
    fn read_recover(&self) -> std::sync::RwLockReadGuard<'_, T>;
    fn write_recover(&self) -> std::sync::RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for std::sync::RwLock<T> {
    fn read_recover(&self) -> std::sync::RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    fn write_recover(&self) -> std::sync::RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

// Holds whatever report_context_menu last recorded, so the on_menu_event
// handler (fired later, once the user actually picks an item) knows what
// the click was on. There's only ever one context menu open at a time, so
// a single slot is enough - no need to key this by tab/label.
#[derive(Clone)]
struct ContextMenuTarget {
    href: Option<String>,
    src: Option<String>,
    selection_text: Option<String>,
    // The content-* webview this right-click happened in - used only so
    // "Open Link/Image in New Tab" can inherit that tab's is_private
    // status (a private tab's "open in new tab" should stay private,
    // same as any real browser).
    source_label: String,
}

type SharedContextMenu = Mutex<Option<ContextMenuTarget>>;

// Deliberately NOT #[derive(Serialize)] - unlike TabInfo (which get_tabs
// sends to the chrome frontend wholesale), a captured plaintext password
// must never leave Rust. vault_save_login reads host/username/password
// straight out of this struct on the backend to build the encrypted
// vault entry; the frontend only ever sees LoginCapturePayload below,
// which deliberately omits the password.
#[derive(Clone)]
struct PendingLoginCapture {
    host: String,
    username: String,
    password: String,
}

// Keyed by content-tab label (not by host) - report_login_submit's most
// recent capture for a given tab simply overwrites whatever was pending
// for that same tab before. Never persisted to disk; this is purely an
// in-memory handoff to whatever eventually shows a save-password prompt.
struct PendingLogins(std::collections::HashMap<String, PendingLoginCapture>);
type SharedPendingLogins = Mutex<PendingLogins>;

// Pushed to the chrome webview so it can offer to save a just-submitted
// login - see report_login_submit (which emits this) and main.js's
// "login-capture-available" listener. Only ever carries display-safe
// fields; the password itself stays in SharedPendingLogins, backend-only,
// until vault_save_login is actually called.
#[derive(Clone, Serialize)]
struct LoginCapturePayload {
    tab_label: String,
    host: String,
    username: String,
}

// Pushed to the chrome webview by report_login_form_present when a page
// loads with a login form and the (already-unlocked) vault has at least
// one saved entry for its host - see main.js's "autofill-available"
// listener. Multiple usernames means multiple saved accounts for the
// same site; the frontend offers a picker rather than guessing which one.
#[derive(Clone, Serialize)]
struct AutofillAvailablePayload {
    tab_label: String,
    host: String,
    usernames: Vec<String>,
}

// The plaintext shape encrypted into an EncryptedVaultEntry's ciphertext
// (see vault_save_login) - never written to disk except inside that
// ciphertext, and never sent to any webview.
#[derive(Serialize, Deserialize)]
struct VaultEntryPlaintext {
    host: String,
    username: String,
    password: String,
}

// Caches fetched favicons keyed by the icon's own URL (not the page URL,
// since e.g. every Wikipedia page shares the same apple-touch icon) so
// switching tabs or revisiting a site doesn't refetch bytes we already
// have. Values are ready-to-use "data:image/...;base64,..." strings - see
// build_favicon_data_url - so the frontend can drop them straight into an
// <img src>. Never evicted; a session's worth of distinct favicons is
// small enough that this isn't worth the complexity yet.
// Tauri's .manage()/.state() are keyed by concrete type, not by type
// alias name - two `Mutex<HashMap<String, String>>` aliases collide at
// runtime ("state ... is already being managed"), so each cache below
// gets its own newtype wrapper even though the inner map shape is
// identical.
struct FaviconCache(std::collections::HashMap<String, String>);
type SharedFaviconCache = Mutex<FaviconCache>;

// --- History & bookmarks ---

#[derive(Clone, Serialize, Deserialize)]
struct HistoryEntry {
    url: String,
    title: String,
    visited_at: i64, // unix ms
}

// What get_history actually returns - HistoryEntry itself is also the
// on-disk persisted shape (see PersistedData), and favicons are
// deliberately kept out of kite_data.json (in-memory only, see
// SharedPageFaviconIndex) - so this is a separate, IPC-only view rather
// than a field added straight onto HistoryEntry.
#[derive(Clone, Serialize)]
struct HistoryEntryView {
    url: String,
    title: String,
    visited_at: i64,
    favicon: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Bookmark {
    url: String,
    title: String,
}

// Same reasoning as HistoryEntryView - Bookmark is the on-disk shape,
// this is what get_bookmarks returns.
#[derive(Clone, Serialize)]
struct BookmarkView {
    url: String,
    title: String,
    favicon: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct DownloadEntry {
    url: String,
    file_name: String,
    path: String,
    completed_at: i64, // unix ms
    success: bool,
}

// One open tab's worth of "Continue where you left off" state - just the
// URL, since title/zoom/scroll position aren't things a fresh webview can
// be handed back anyway (see initial_session_tabs/create_tab_webview).
#[derive(Clone, Serialize, Deserialize)]
struct SessionTab {
    url: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct SessionState {
    #[serde(default)]
    tabs: Vec<SessionTab>,
    #[serde(default)]
    active_index: usize,
}

// Persisted user preferences, edited from the kite://settings page.
// #[serde(default...)] on every field (plus the struct-level Default impl
// below, used by PersistedData's #[serde(default)]) means older
// kite_data.json files saved before Settings existed still load fine -
// missing fields just fall back to these defaults instead of failing to
// parse.
#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    #[serde(default = "default_search_engine")]
    search_engine: String, // "google" | "bing" | "duckduckgo"
    #[serde(default = "default_homepage_mode")]
    homepage_mode: String, // "home" | "custom" | "blank" | "restore"
    #[serde(default)]
    homepage_url: String,
    #[serde(default)]
    downloads_dir: Option<String>,
    #[serde(default = "default_content_blocking_enabled")]
    content_blocking_enabled: bool,
}

fn default_search_engine() -> String {
    "google".to_string()
}

fn default_homepage_mode() -> String {
    "home".to_string()
}

fn default_content_blocking_enabled() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            search_engine: default_search_engine(),
            homepage_mode: default_homepage_mode(),
            homepage_url: String::new(),
            downloads_dir: None,
            content_blocking_enabled: default_content_blocking_enabled(),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct PersistedData {
    #[serde(default)]
    history: Vec<HistoryEntry>,
    #[serde(default)]
    bookmarks: Vec<Bookmark>,
    #[serde(default)]
    downloads: Vec<DownloadEntry>,
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    session: SessionState,
    // Page URL -> favicon data URL, so bookmarks/history keep their icons
    // across restarts. Keyed by page URL (not icon URL - see the
    // now-removed SharedPageFaviconIndex this replaced), and populated in
    // apply_favicon_to_tab. Deliberately not the icon-URL SharedFaviconCache
    // (that one's a session-only fetch-dedup cache, keyed differently and
    // not worth persisting - refetching a handful of icon bytes on next
    // launch is cheap).
    #[serde(default)]
    favicons: std::collections::HashMap<String, String>,
    // Hosts where content blocking is turned off for that specific site -
    // separate from Settings.content_blocking_enabled (the global on/off
    // switch), this is the per-site exception list toggled via the
    // toolbar's shield badge (see toggle_site_allowlist). Stored as plain
    // hostnames exactly as they appear in a tab's own URL (no public-
    // suffix-list-aware "registrable domain" normalization) - see
    // host_matches_allowlist's own comment for what that does and
    // doesn't cover.
    #[serde(default)]
    content_blocking_allowlist: Vec<String>,
    // Unix timestamp (ms, matching now_ms()) of the last time the
    // blocklist was successfully refreshed from the network - None until the first
    // refresh_blocklist call ever succeeds (fresh installs, or anyone
    // still on the compiled-in bundled list only). Set alongside
    // blocklist_entry_count by refresh_blocklist; both are display-only
    // (Settings UI) - they don't drive any logic themselves, since
    // load_initial_blocklist decides what to load purely from whether
    // blocklist_update.txt exists and parses to a sane size.
    #[serde(default)]
    last_blocklist_refresh: Option<i64>,
    // Entry count of the blocklist as of the last successful refresh -
    // shown next to last_blocklist_refresh in Settings. Not touched by
    // load_initial_blocklist (that only affects the live in-memory
    // BLOCKLIST), so on first launch after upgrading to this feature
    // it'll read None/0 even though the bundled list is already active -
    // that's expected, and resolves itself after the first refresh.
    #[serde(default)]
    blocklist_entry_count: Option<usize>,
}


struct AppData {
    data: PersistedData,
    file_path: PathBuf,
}

type SharedAppData = Mutex<AppData>;

// --- Password vault ---
//
// Deliberately separate from PersistedData/AppData above (own file, own
// Mutex, own encryption) - saved logins are more sensitive than history/
// bookmarks and get their own security model: everything at rest is
// encrypted under a key derived from a master password the user chooses,
// via Argon2id (VaultFile::kdf_salt is the only thing about that password
// ever written to disk). The derived key itself only ever lives in
// VaultRuntime, in memory, for as long as the vault is unlocked - see
// vault_create/vault_unlock/vault_lock.
//
// Phase 1 (this file, right now) only covers creating/unlocking/locking
// an empty vault - `entries` is wired up but unused until the save/
// autofill phases land on top of this.

#[derive(Clone, Serialize, Deserialize)]
struct EncryptedVaultEntry {
    // Not read/written anywhere yet - see the module comment above.
    // Each entry will get its own random nonce (never reused across
    // entries, unlike the single verifier nonce) once save-password
    // lands, so the shape is established now rather than migrated later.
    nonce: String,
    ciphertext: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct VaultFile {
    // Base64 - random per vault, fixed for its lifetime. Combined with
    // the user's master password (never itself stored) via Argon2id to
    // re-derive the same 256-bit key on every unlock.
    kdf_salt: String,
    // Base64 nonce + ciphertext for VAULT_VERIFIER_PLAINTEXT under the
    // derived key - see that constant's own comment for what this proves.
    verifier_nonce: String,
    verifier_ciphertext: String,
    #[serde(default)]
    entries: Vec<EncryptedVaultEntry>,
}

// The in-memory-only counterpart to VaultFile - never serialized, and in
// particular `key` must never end up anywhere near serde_json::to_string.
// `key` is None whenever the vault is locked (including "no vault has
// ever been created yet"); Some(...) only while unlocked, cleared (and
// zeroized) by vault_lock and on process exit via VaultRuntime's Drop.
struct VaultRuntime {
    file_path: PathBuf,
    key: Option<[u8; VAULT_KEY_LEN]>,
}

impl Drop for VaultRuntime {
    fn drop(&mut self) {
        if let Some(key) = &mut self.key {
            key.zeroize();
        }
    }
}

type SharedVaultState = Mutex<VaultRuntime>;

#[derive(Serialize)]
struct VaultStatusPayload {
    // Whether kite_passwords.json exists on disk at all - false means
    // "never set up a vault", which the UI uses to show the creation
    // form instead of the unlock form.
    exists: bool,
    unlocked: bool,
}

fn vault_file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(PASSWORDS_FILE_NAME))
}

fn load_vault_file(path: &PathBuf) -> Option<VaultFile> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn save_vault_file(path: &PathBuf, file: &VaultFile) -> Result<(), String> {
    let json = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

// Argon2id with this crate's default parameters (19 MiB memory, 2
// iterations, 1 lane - OWASP's current baseline recommendation for
// interactive login) turns the master password + salt into a 256-bit
// AES key. Deliberately slow (tens of ms) - that cost is the whole point
// against offline guessing if kite_passwords.json is ever stolen.
fn derive_vault_key(password: &str, salt: &[u8]) -> Result<[u8; VAULT_KEY_LEN], String> {
    let mut key = [0u8; VAULT_KEY_LEN];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| format!("key derivation failed: {e}"))?;
    Ok(key)
}

fn vault_encrypt(key: &[u8; VAULT_KEY_LEN], plaintext: &[u8]) -> Result<(String, String), String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; VAULT_NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("encryption failed: {e}"))?;
    Ok((
        base64::engine::general_purpose::STANDARD.encode(nonce_bytes),
        base64::engine::general_purpose::STANDARD.encode(ciphertext),
    ))
}

fn vault_decrypt(
    key: &[u8; VAULT_KEY_LEN],
    nonce_b64: &str,
    ciphertext_b64: &str,
) -> Result<Vec<u8>, String> {
    let nonce_bytes = base64::engine::general_purpose::STANDARD
        .decode(nonce_b64)
        .map_err(|e| e.to_string())?;
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(ciphertext_b64)
        .map_err(|e| e.to_string())?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext.as_ref())
        // AES-GCM's auth tag fails this on *any* mismatch - wrong key
        // (i.e. wrong master password) or tampered ciphertext alike -
        // which is exactly the "prove the password was correct" check
        // vault_unlock relies on. Deliberately vague message: don't leak
        // which of "wrong password" vs "corrupt file" happened.
        .map_err(|_| "decryption failed".to_string())
}

#[tauri::command]
fn vault_status(webview: tauri::Webview, app: tauri::AppHandle) -> Result<VaultStatusPayload, String> {
    require_chrome(&webview)?;
    let state = app.state::<SharedVaultState>();
    let st = state.lock_recover();
    Ok(VaultStatusPayload {
        exists: st.file_path.exists(),
        unlocked: st.key.is_some(),
    })
}

#[tauri::command]
fn vault_create(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    master_password: String,
) -> Result<(), String> {
    require_chrome(&webview)?;
    if master_password.len() < 8 {
        return Err("Master password must be at least 8 characters.".to_string());
    }

    let state = app.state::<SharedVaultState>();
    let mut st = state.lock_recover();
    if st.file_path.exists() {
        return Err("A vault already exists.".to_string());
    }

    let mut salt = [0u8; VAULT_SALT_LEN];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    let key = derive_vault_key(&master_password, &salt)?;
    let (verifier_nonce, verifier_ciphertext) = vault_encrypt(&key, VAULT_VERIFIER_PLAINTEXT)?;

    let file = VaultFile {
        kdf_salt: base64::engine::general_purpose::STANDARD.encode(salt),
        verifier_nonce,
        verifier_ciphertext,
        entries: Vec::new(),
    };
    save_vault_file(&st.file_path, &file)?;
    st.key = Some(key);
    Ok(())
}

#[tauri::command]
// Shared by vault_unlock and vault_unlock_and_save_login - derives a key
// from a candidate master password against the vault's stored salt and
// confirms it's correct via the verifier (see VAULT_VERIFIER_PLAINTEXT's
// own comment for how). Doesn't touch VaultRuntime.key itself; callers
// decide what to do with a correct key.
fn verify_master_password(file: &VaultFile, master_password: &str) -> Result<[u8; VAULT_KEY_LEN], String> {
    let salt = base64::engine::general_purpose::STANDARD
        .decode(&file.kdf_salt)
        .map_err(|e| e.to_string())?;
    let key = derive_vault_key(master_password, &salt)?;
    vault_decrypt(&key, &file.verifier_nonce, &file.verifier_ciphertext)
        .map_err(|_| "Incorrect master password.".to_string())?;
    Ok(key)
}

#[tauri::command]
fn vault_unlock(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    master_password: String,
) -> Result<(), String> {
    require_chrome(&webview)?;
    let state = app.state::<SharedVaultState>();
    let mut st = state.lock_recover();

    let file = load_vault_file(&st.file_path).ok_or_else(|| "No vault found.".to_string())?;
    let key = verify_master_password(&file, &master_password)?;

    st.key = Some(key);
    Ok(())
}

#[tauri::command]
fn vault_lock(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    let state = app.state::<SharedVaultState>();
    let mut st = state.lock_recover();
    if let Some(mut key) = st.key.take() {
        key.zeroize();
    }
    Ok(())
}

// Called when the person clicks "Save" on the login-save prompt
// (main.js's login-save-prompt-save-btn) - encrypts the pending capture
// for this tab and writes it into kite_passwords.json. If an entry for
// the same host+username already exists, this replaces its ciphertext
// in place rather than appending a duplicate, so logging into the same
// site again (password change, or just a second visit) updates the
// existing entry instead of accumulating copies.
#[tauri::command]
fn vault_save_login(webview: tauri::Webview, app: tauri::AppHandle, tab_label: String) -> Result<(), String> {
    require_chrome(&webview)?;

    let vault_state = app.state::<SharedVaultState>();
    let vst = vault_state.lock_recover();
    let key = vst
        .key
        .ok_or_else(|| "Vault is locked. Unlock it in Passwords to save.".to_string())?;

    encrypt_and_store_login(&app, &vst.file_path, &key, &tab_label)
}

// Backs the login-save prompt's inline "Enter master password to save"
// form (main.js shows this instead of a plain error when Save is clicked
// while the vault happens to be locked) - verifies the password and
// saves the pending capture in one round trip, rather than making the
// person separately unlock in the Passwords view first and then come
// back and click Save again. Doesn't leave the vault unlocked afterward
// on a wrong password (verify_master_password errors out before
// touching st.key), and *does* leave it unlocked on success, same as a
// normal vault_unlock would - there's no reason to make someone unlock
// twice just because the first unlock happened to come from this form.
#[tauri::command]
fn vault_unlock_and_save_login(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    master_password: String,
    tab_label: String,
) -> Result<(), String> {
    require_chrome(&webview)?;

    let vault_state = app.state::<SharedVaultState>();
    let mut vst = vault_state.lock_recover();
    let file = load_vault_file(&vst.file_path).ok_or_else(|| "No vault found.".to_string())?;
    let key = verify_master_password(&file, &master_password)?;
    vst.key = Some(key);

    encrypt_and_store_login(&app, &vst.file_path, &key, &tab_label)
}

// Shared by vault_save_login and vault_unlock_and_save_login - both have
// already resolved a valid key by the time they call this; this is just
// "encrypt the pending capture for tab_label and write it into the vault
// file at file_path", identical either way.
fn encrypt_and_store_login(
    app: &tauri::AppHandle,
    file_path: &PathBuf,
    key: &[u8; VAULT_KEY_LEN],
    tab_label: &str,
) -> Result<(), String> {
    let pending_state = app.state::<SharedPendingLogins>();
    let mut pending = pending_state.lock_recover();
    let capture = pending
        .0
        .remove(tab_label)
        .ok_or_else(|| "Nothing to save.".to_string())?;
    drop(pending);

    let mut file = load_vault_file(file_path).ok_or_else(|| "Vault file missing.".to_string())?;

    let plaintext = VaultEntryPlaintext {
        host: capture.host,
        username: capture.username,
        password: capture.password,
    };
    let plaintext_bytes = serde_json::to_vec(&plaintext).map_err(|e| e.to_string())?;
    let (nonce, ciphertext) = vault_encrypt(key, &plaintext_bytes)?;

    // Decrypting every existing entry just to find a host+username match
    // is fine at the scale a personal password vault actually reaches
    // (tens to low hundreds of entries, not millions) - simplicity here
    // matters more than an index that would need to be kept in sync with
    // the encrypted store on every write.
    let existing_index = file.entries.iter().position(|e| {
        vault_decrypt(key, &e.nonce, &e.ciphertext)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<VaultEntryPlaintext>(&bytes).ok())
            .is_some_and(|existing| {
                existing.host.eq_ignore_ascii_case(&plaintext.host) && existing.username == plaintext.username
            })
    });

    let new_entry = EncryptedVaultEntry { nonce, ciphertext };
    let was_update = existing_index.is_some();
    match existing_index {
        Some(i) => file.entries[i] = new_entry,
        None => file.entries.push(new_entry),
    }

    save_vault_file(file_path, &file)?;
    // No management UI to confirm this in yet (that's Phase 5) - this is
    // the only way to verify a save actually landed on disk until then.
    // Never logs the password, only the fact and shape of the write.
    eprintln!(
        "[kite] vault {} entry for {} (username={:?}) - {} entries total",
        if was_update { "updated" } else { "saved new" },
        plaintext.host,
        plaintext.username,
        file.entries.len()
    );
    Ok(())
}

// Called for "Not now" and the prompt's close (x) button alike - neither
// saves anything, both just clear the pending capture for this tab so
// the prompt won't keep reappearing for the same login attempt. Doesn't
// persist any "don't ask for this site again" preference; a future
// submit on the same site will prompt again, same as most browsers'
// lightweight dismiss (as opposed to their separate, explicit "never
// save for this site" list, which isn't in scope here).
#[tauri::command]
fn vault_dismiss_login(webview: tauri::Webview, app: tauri::AppHandle, tab_label: String) -> Result<(), String> {
    require_chrome(&webview)?;
    let state = app.state::<SharedPendingLogins>();
    let mut pending = state.lock_recover();
    pending.0.remove(&tab_label);
    Ok(())
}

// What the Passwords library view actually lists - see vault_list_logins.
// Deliberately no password field, same reasoning as LoginCapturePayload:
// the list itself should never carry plaintext passwords over IPC, only
// vault_reveal_login (an explicit "Show" click) or vault_copy_login_password
// (an explicit "Copy" click, which never even returns to JS - see below)
// should ever cause a password to leave Rust.
#[derive(Clone, Serialize)]
struct VaultLoginSummary {
    host: String,
    username: String,
}

// Shared by vault_reveal_login/vault_copy_login_password/vault_delete_login
// - locates the one entry (by index, so callers can both read and mutate)
// matching a host+username pair the frontend already learned about from
// vault_list_logins. Requires the vault to already be unlocked; callers
// are responsible for that check (they need the key for other reasons
// anyway - see each command below).
fn find_vault_entry_index(
    file: &VaultFile,
    key: &[u8; VAULT_KEY_LEN],
    host: &str,
    username: &str,
) -> Option<usize> {
    file.entries.iter().position(|e| {
        vault_decrypt(key, &e.nonce, &e.ciphertext)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<VaultEntryPlaintext>(&bytes).ok())
            .is_some_and(|existing| existing.host.eq_ignore_ascii_case(host) && existing.username == username)
    })
}

// Populates the Passwords library view's list - see VaultLoginSummary's
// own comment for why this never includes a password.
#[tauri::command]
fn vault_list_logins(webview: tauri::Webview, app: tauri::AppHandle) -> Result<Vec<VaultLoginSummary>, String> {
    require_chrome(&webview)?;
    let vault_state = app.state::<SharedVaultState>();
    let vst = vault_state.lock_recover();
    let key = vst.key.ok_or_else(|| "Vault is locked.".to_string())?;
    let file = load_vault_file(&vst.file_path).ok_or_else(|| "Vault file missing.".to_string())?;

    let mut summaries: Vec<VaultLoginSummary> = file
        .entries
        .iter()
        .filter_map(|e| {
            vault_decrypt(&key, &e.nonce, &e.ciphertext)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<VaultEntryPlaintext>(&bytes).ok())
                .map(|p| VaultLoginSummary {
                    host: p.host,
                    username: p.username,
                })
        })
        .collect();
    summaries.sort_by(|a, b| a.host.cmp(&b.host).then(a.username.cmp(&b.username)));
    Ok(summaries)
}

// Backs the "Show" toggle on a saved login row - the one command in this
// group that hands a plaintext password to the frontend at all, and only
// because displaying it inline is the whole point of a "Show" button.
#[tauri::command]
fn vault_reveal_login(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    host: String,
    username: String,
) -> Result<String, String> {
    require_chrome(&webview)?;
    let vault_state = app.state::<SharedVaultState>();
    let vst = vault_state.lock_recover();
    let key = vst.key.ok_or_else(|| "Vault is locked.".to_string())?;
    let file = load_vault_file(&vst.file_path).ok_or_else(|| "Vault file missing.".to_string())?;
    let idx = find_vault_entry_index(&file, &key, &host, &username).ok_or_else(|| "Login not found.".to_string())?;
    let bytes = vault_decrypt(&key, &file.entries[idx].nonce, &file.entries[idx].ciphertext)?;
    let plaintext: VaultEntryPlaintext = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(plaintext.password)
}

// Backs the "Copy" button - deliberately never returns the password to
// JS at all (Result<(), String>, not Result<String, String>) and writes
// straight to the OS clipboard instead, the same way report_context_menu's
// "Copy Image Address" does via app.clipboard(). A password that only
// ever needs to be pasted somewhere else has no reason to pass through
// the webview/DOM even transiently the way vault_reveal_login's does.
#[tauri::command]
fn vault_copy_login_password(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    host: String,
    username: String,
) -> Result<(), String> {
    require_chrome(&webview)?;
    let vault_state = app.state::<SharedVaultState>();
    let vst = vault_state.lock_recover();
    let key = vst.key.ok_or_else(|| "Vault is locked.".to_string())?;
    let file = load_vault_file(&vst.file_path).ok_or_else(|| "Vault file missing.".to_string())?;
    let idx = find_vault_entry_index(&file, &key, &host, &username).ok_or_else(|| "Login not found.".to_string())?;
    let bytes = vault_decrypt(&key, &file.entries[idx].nonce, &file.entries[idx].ciphertext)?;
    let plaintext: VaultEntryPlaintext = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    app.clipboard().write_text(plaintext.password).map_err(|e| e.to_string())
}

// Backs the "Delete" button on a saved login row.
#[tauri::command]
fn vault_delete_login(webview: tauri::Webview, app: tauri::AppHandle, host: String, username: String) -> Result<(), String> {
    require_chrome(&webview)?;
    let vault_state = app.state::<SharedVaultState>();
    let vst = vault_state.lock_recover();
    let key = vst.key.ok_or_else(|| "Vault is locked.".to_string())?;
    let mut file = load_vault_file(&vst.file_path).ok_or_else(|| "Vault file missing.".to_string())?;
    let idx = find_vault_entry_index(&file, &key, &host, &username).ok_or_else(|| "Login not found.".to_string())?;
    file.entries.remove(idx);
    save_vault_file(&vst.file_path, &file)
}

// Tauri's ACL/capabilities system only auto-restricts *core* and *plugin*
// commands by default - commands we register ourselves via invoke_handler
// are callable by any webview regardless of the capabilities file, unless
// explicitly opted in via a build-time mechanism. Rather than depend on
// that (and its permission-identifier naming rules, which reject
// underscores - a real snag we hit), every command below takes the
// injected Webview that actually called it and checks its label directly.
// Only "chrome" (the tab bar/toolbar/library panel) is allowed through;
// content tabs - which can load arbitrary external sites - get rejected
// outright, no matter what a capability file grants.
fn require_chrome(webview: &tauri::Webview) -> Result<(), String> {
    if webview.label() != MAIN_WEBVIEW_LABEL {
        return Err("not allowed from this webview".to_string());
    }
    Ok(())
}

// Mirrors require_chrome, but inverted: this command must only ever be
// called by a content tab reporting its own context-menu target, never by
// chrome. Checks the "content-" label prefix create_tab_webview assigns,
// rather than an exact match.
fn require_content(webview: &tauri::Webview) -> Result<(), String> {
    if !webview.label().starts_with("content-") {
        return Err("not allowed from this webview".to_string());
    }
    Ok(())
}

// tao/Tauri's window-level Focused event and Window::is_focused() both
// track top-level window activation - but Kite's chrome and every tab are
// separate child webviews (see the "unstable" multiwebview note in
// Cargo.toml), and empirically clicking into any of them never fires a
// WindowEvent::Focused at all here, so that signal can't tell us whether
// Kite is the foreground app. Instead we ask Windows directly: whichever
// window is currently in the foreground, does it belong to *our* process?
// That's true no matter which of our own child webviews has the actual
// input focus, and only false once a genuinely different application is
// in front - which is all the global-shortcut guard actually needs.
#[cfg(target_os = "windows")]
fn app_is_foreground() -> bool {
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return false;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        pid == GetCurrentProcessId()
    }
}

#[cfg(not(target_os = "windows"))]
fn app_is_foreground() -> bool {
    // Kite is Windows-only for now (see build.rs/Cargo.toml comments) -
    // this fallback just avoids a compile error if that ever changes
    // before an equivalent check is written for that platform.
    true
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn app_data_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(DATA_FILE_NAME))
}

// A panic anywhere in the app currently just vanishes - a bundled
// release-mode Windows build has no visible console for eprintln! (or
// the default panic hook) to reach, so there's no way to tell one
// happened, let alone where. This doesn't change whether the app
// survives a panic (that's Cargo's default unwind behavior, plus
// lock_recover() above for shared state specifically) - it only makes
// panics observable, by also writing them to a log file next to
// kite_data.json. Installed as early as possible in setup() below.
fn install_panic_hook(app: &tauri::AppHandle) {
    let log_path = match app.path().app_data_dir() {
        Ok(dir) => {
            let _ = fs::create_dir_all(&dir);
            dir.join("kite_panic.log")
        }
        Err(_) => return, // nowhere sensible to write - keep the default hook
    };
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        let line = format!("[{}] {info}\n", now_ms());
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
            let _ = file.write_all(line.as_bytes());
        }
        eprintln!("[kite] panic: {info}");
    }));
}

fn load_persisted_data(path: &PathBuf) -> PersistedData {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_persisted_data(app: &tauri::AppHandle) {
    let state = app.state::<SharedAppData>();
    let st = state.lock_recover();
    if let Ok(json) = serde_json::to_string_pretty(&st.data) {
        let _ = fs::write(&st.file_path, json);
    }
}

fn record_history(app: &tauri::AppHandle, url: &str) {
    if url.is_empty() {
        return;
    }
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        st.data.history.push(HistoryEntry {
            url: url.to_string(),
            title: String::new(),
            visited_at: now_ms(),
        });
        let len = st.data.history.len();
        if len > HISTORY_LIMIT {
            let excess = len - HISTORY_LIMIT;
            st.data.history.drain(0..excess);
        }
    }
    save_persisted_data(app);
}

fn update_history_title(app: &tauri::AppHandle, url: &str, title: &str) {
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        if let Some(entry) = st.data.history.iter_mut().rev().find(|e| e.url == url) {
            entry.title = title.to_string();
        }
    }
    save_persisted_data(app);
}

// path is None in the rare case WebView2 couldn't report a final path (see
// DownloadEvent::Finished's doc comment: that does NOT necessarily mean
// the download failed - success is the only field to trust for that), so
// this still records an entry either way, just without a usable path.
fn record_download(app: &tauri::AppHandle, url: &str, path: Option<&std::path::Path>, success: bool) {
    let (file_name, path_str) = match path {
        Some(p) => (
            p.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| url.to_string()),
            p.to_string_lossy().to_string(),
        ),
        None => (url.to_string(), String::new()),
    };
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        st.data.downloads.push(DownloadEntry {
            url: url.to_string(),
            file_name,
            path: path_str,
            completed_at: now_ms(),
            success,
        });
        let len = st.data.downloads.len();
        if len > DOWNLOAD_LIMIT {
            let excess = len - DOWNLOAD_LIMIT;
            st.data.downloads.drain(0..excess);
        }
    }
    save_persisted_data(app);
}

#[tauri::command]
fn get_history(webview: tauri::Webview, app: tauri::AppHandle) -> Result<Vec<HistoryEntryView>, String> {
    require_chrome(&webview)?;
    let (mut items, favicons): (Vec<HistoryEntry>, std::collections::HashMap<String, String>) = {
        let state = app.state::<SharedAppData>();
        let st = state.lock_recover();
        (st.data.history.clone(), st.data.favicons.clone())
    };
    items.reverse(); // most recent first
    Ok(items
        .into_iter()
        .map(|e| HistoryEntryView {
            favicon: favicons.get(&e.url).cloned(),
            url: e.url,
            title: e.title,
            visited_at: e.visited_at,
        })
        .collect())
}

#[tauri::command]
fn clear_history(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        st.data.history.clear();
    }
    save_persisted_data(&app);
    Ok(())
}

#[tauri::command]
fn remove_history_entry(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    visited_at: i64,
    url: String,
) -> Result<(), String> {
    require_chrome(&webview)?;
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        st.data
            .history
            .retain(|e| !(e.visited_at == visited_at && e.url == url));
    }
    save_persisted_data(&app);
    Ok(())
}

#[tauri::command]
fn get_bookmarks(webview: tauri::Webview, app: tauri::AppHandle) -> Result<Vec<BookmarkView>, String> {
    require_chrome(&webview)?;
    let (bookmarks, favicons): (Vec<Bookmark>, std::collections::HashMap<String, String>) = {
        let state = app.state::<SharedAppData>();
        let st = state.lock_recover();
        (st.data.bookmarks.clone(), st.data.favicons.clone())
    };
    Ok(bookmarks
        .into_iter()
        .map(|b| BookmarkView {
            favicon: favicons.get(&b.url).cloned(),
            url: b.url,
            title: b.title,
        })
        .collect())
}

#[tauri::command]
fn get_downloads(webview: tauri::Webview, app: tauri::AppHandle) -> Result<Vec<DownloadEntry>, String> {
    require_chrome(&webview)?;
    let state = app.state::<SharedAppData>();
    let st = state.lock_recover();
    let mut items = st.data.downloads.clone();
    items.reverse(); // most recent first
    Ok(items)
}

#[tauri::command]
fn open_download(webview: tauri::Webview, app: tauri::AppHandle, path: String) -> Result<(), String> {
    require_chrome(&webview)?;
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn show_download_in_folder(webview: tauri::Webview, app: tauri::AppHandle, path: String) -> Result<(), String> {
    require_chrome(&webview)?;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_downloads(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        st.data.downloads.clear();
    }
    save_persisted_data(&app);
    Ok(())
}

#[tauri::command]
fn remove_download_entry(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    completed_at: i64,
    url: String,
) -> Result<(), String> {
    require_chrome(&webview)?;
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        st.data
            .downloads
            .retain(|e| !(e.completed_at == completed_at && e.url == url));
    }
    save_persisted_data(&app);
    Ok(())
}

#[tauri::command]
fn add_bookmark(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    url: String,
    title: String,
) -> Result<(), String> {
    require_chrome(&webview)?;
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        if !st.data.bookmarks.iter().any(|b| b.url == url) {
            st.data.bookmarks.push(Bookmark { url, title });
        }
    }
    save_persisted_data(&app);
    Ok(())
}

#[tauri::command]
fn remove_bookmark(webview: tauri::Webview, app: tauri::AppHandle, url: String) -> Result<(), String> {
    require_chrome(&webview)?;
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        st.data.bookmarks.retain(|b| b.url != url);
    }
    save_persisted_data(&app);
    Ok(())
}

#[tauri::command]
fn get_settings(webview: tauri::Webview, app: tauri::AppHandle) -> Result<Settings, String> {
    require_chrome(&webview)?;
    let state = app.state::<SharedAppData>();
    let st = state.lock_recover();
    Ok(st.data.settings.clone())
}

// Separate from get_settings/Settings on purpose - last_blocklist_refresh
// and blocklist_entry_count are display-only status (Settings UI), not
// user-editable preferences, and they live on PersistedData directly
// rather than nested in Settings for the same reason (see their own
// comments on PersistedData).
#[derive(Clone, Serialize)]
struct BlocklistStatus {
    last_refresh: Option<i64>, // unix ms
    entry_count: Option<usize>,
}

#[tauri::command]
fn get_blocklist_status(webview: tauri::Webview, app: tauri::AppHandle) -> Result<BlocklistStatus, String> {
    require_chrome(&webview)?;
    let state = app.state::<SharedAppData>();
    let st = state.lock_recover();
    Ok(BlocklistStatus {
        last_refresh: st.data.last_blocklist_refresh,
        entry_count: st.data.blocklist_entry_count,
    })
}

#[tauri::command]
fn set_search_engine(webview: tauri::Webview, app: tauri::AppHandle, engine: String) -> Result<(), String> {
    require_chrome(&webview)?;
    const VALID_ENGINES: [&str; 3] = ["google", "bing", "duckduckgo"];
    if !VALID_ENGINES.contains(&engine.as_str()) {
        return Err(format!("unknown search engine: {engine}"));
    }
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        st.data.settings.search_engine = engine;
    }
    save_persisted_data(&app);
    // A home tab that's already loaded (e.g. the tab parked behind the
    // Settings panel right now) won't pick up the new setting until it
    // next navigates - push it live so it doesn't look stale if the user
    // switches back to it without reloading.
    let home_labels: Vec<String> = {
        let state = app.state::<SharedTabState>();
        let st = state.lock_recover();
        st.tabs
            .iter()
            .filter(|t| t.url == HOME_URL)
            .map(|t| t.label.clone())
            .collect()
    };
    for label in home_labels {
        push_search_engine_to_home(&app, &label);
    }
    Ok(())
}

#[tauri::command]
fn set_homepage(webview: tauri::Webview, app: tauri::AppHandle, mode: String, url: String) -> Result<(), String> {
    require_chrome(&webview)?;
    const VALID_MODES: [&str; 4] = ["home", "custom", "blank", "restore"];
    if !VALID_MODES.contains(&mode.as_str()) {
        return Err(format!("unknown homepage mode: {mode}"));
    }
    // Only "custom" needs a URL at all - validate it up front so a bad
    // value never reaches disk, rather than surfacing as a broken new tab
    // later. "home"/"blank" don't carry a URL, so clear out anything left
    // over from a previous "custom" selection.
    let stored_url = if mode == "custom" {
        normalize_homepage_url(&url)?
    } else {
        String::new()
    };
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        st.data.settings.homepage_mode = mode;
        st.data.settings.homepage_url = stored_url;
    }
    save_persisted_data(&app);
    Ok(())
}

// Toggles content blocking on/off, checked live by watch_for_requests on
// every intercepted request - so unlike set_search_engine, there's no
// need to push anything to already-open tabs, the very next request in
// any tab picks up the new value straight from SharedAppData.
#[tauri::command]
fn set_content_blocking(webview: tauri::Webview, app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    require_chrome(&webview)?;
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        st.data.settings.content_blocking_enabled = enabled;
    }
    save_persisted_data(&app);
    Ok(())
}

// Toggles content blocking for whichever host the active tab is
// currently on - the shield badge's click handler. Unlike
// set_content_blocking (global on/off), this needs to know which site,
// so it reads the active tab's own URL rather than taking a host
// parameter from the frontend - the backend is the single source of
// truth for "what site is the active tab actually on" (the frontend's
// own address-bar value can lag slightly, e.g. mid-navigation), same
// reasoning active_webview() elsewhere in this file already follows for
// "act on whatever's really active, not whatever the caller thinks is
// active". Reloads the active tab afterward so the toggle has a visible
// effect immediately, rather than only changing behavior for requests
// made from here on - matches how per-site shield toggles behave in
// real browsers. Returns the new allowlisted state so the frontend can
// use it without waiting on the following tabs-changed push, though in
// practice that push (via refresh_site_allowlisted_for_all_tabs) arrives
// almost immediately anyway.
#[tauri::command]
fn toggle_site_allowlist(webview: tauri::Webview, app: tauri::AppHandle) -> Result<bool, String> {
    require_chrome(&webview)?;

    let host = {
        let state = app.state::<SharedTabState>();
        let st = state.lock_recover();
        st.tabs
            .iter()
            .find(|t| t.label == st.active)
            .and_then(|t| url::Url::parse(&t.url).ok())
            .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
    };
    let Some(host) = host else {
        return Err("active tab has no site to toggle".to_string());
    };

    let now_allowlisted = {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        if let Some(pos) = st.data.content_blocking_allowlist.iter().position(|h| h == &host) {
            st.data.content_blocking_allowlist.remove(pos);
            false
        } else {
            st.data.content_blocking_allowlist.push(host);
            true
        }
    };
    save_persisted_data(&app);
    refresh_site_allowlisted_for_all_tabs(&app);
    emit_tabs_changed(&app);

    if let Ok(w) = active_webview(&app) {
        let _ = w.eval("location.reload()");
    }

    Ok(now_allowlisted)
}

// Hand-picked additions merged into every refresh, on top of whatever
// comes back from StevenBlack/hosts - see blocklist.txt's own header for
// why these specific ones are called out (common trackers not reliably
// present in that upstream list). This is now the source of truth for
// them (refreshes overwrite blocklist_update.txt entirely, so anything
// only ever hand-added to the compiled-in blocklist.txt wouldn't survive
// a refresh otherwise).
const HAND_PICKED_BLOCKLIST_HOSTS: &[&str] = &[
    "connect.facebook.net",
    "adservice.google.com",
    "criteo.com",
    "taboola.com",
    "outbrain.com",
];

const BLOCKLIST_SOURCE_URL: &str = "https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts";

// Parses the StevenBlack/hosts file format: "0.0.0.0 <domain>" lines,
// one host per line, mixed in with comments and a handful of
// housekeeping entries (0.0.0.0 itself, localhost, broadcasthost, etc.)
// at the top that map real machine names rather than ad/tracking
// domains. Anything not matching that exact "0.0.0.0 <host>" shape is
// skipped rather than erroring - a hosts file has enough incidental
// format noise (comments, blank lines) that failing hard on the first
// unexpected line would be too fragile for something fetched over the
// network on every refresh.
fn parse_stevenblack_hosts(text: &str) -> std::collections::HashSet<String> {
    const SKIP_HOSTS: &[&str] = &[
        "0.0.0.0",
        "localhost",
        "localhost.localdomain",
        "local",
        "broadcasthost",
        "ip6-localhost",
        "ip6-loopback",
    ];
    text.lines()
        .filter_map(|line| {
            let line = line.split('#').next().unwrap_or("").trim();
            let mut parts = line.split_whitespace();
            let ip = parts.next()?;
            let host = parts.next()?;
            if ip != "0.0.0.0" || SKIP_HOSTS.contains(&host) {
                return None;
            }
            Some(host.to_lowercase())
        })
        .collect()
}

// Overwrites blocklist_update.txt with the given host set - called only
// after a fetch has already passed BLOCKLIST_MIN_SANE_ENTRIES, so this
// never has a chance to persist a bad/truncated refresh over a
// previously-good one.
fn write_blocklist_to_disk(app: &tauri::AppHandle, hosts: &std::collections::HashSet<String>) -> Result<(), String> {
    let path = blocklist_update_path(app)?;
    let mut sorted: Vec<&String> = hosts.iter().collect();
    sorted.sort();
    let mut out = String::with_capacity(hosts.len() * 16 + 256);
    out.push_str("# Kite content-blocking blocklist - runtime refresh, do not hand-edit\n");
    out.push_str(&format!("# Source: {BLOCKLIST_SOURCE_URL}\n"));
    out.push_str(&format!("# Refreshed at (unix ms): {}\n", now_ms()));
    out.push_str(&format!("# {} entries (includes hand-picked additions)\n#\n", sorted.len()));
    for host in sorted {
        out.push_str(host);
        out.push('\n');
    }
    fs::write(&path, out).map_err(|e| e.to_string())
}

// Payload for the "blocklist-refreshed" event - refresh_blocklist runs
// on a background thread and returns immediately (see its own comment
// for why), so the frontend finds out how it went via this event rather
// than the command's own return value. Also used for the silent
// startup auto-refresh (see maybe_auto_refresh_blocklist_on_startup) -
// if Settings happens to be open when that lands, it updates live the
// same way a manual check would.
#[derive(Clone, Serialize)]
struct BlocklistRefreshResult {
    success: bool,
    entry_count: usize,
    refreshed_at: i64, // unix ms
    error: Option<String>,
}

// The actual fetch/parse/validate/swap/persist work, shared by both
// refresh_blocklist (manual, from the Settings button) and
// maybe_auto_refresh_blocklist_on_startup (automatic, if the list is
// stale) - identical outcome either way, just triggered differently.
// Blocking - always call this from a background thread, never directly
// from a command or from setup() itself.
fn perform_blocklist_refresh(app: &tauri::AppHandle) {
    let outcome = (|| -> Result<usize, String> {
        let resp = reqwest::blocking::get(BLOCKLIST_SOURCE_URL).map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let text = resp.text().map_err(|e| e.to_string())?;
        let mut hosts = parse_stevenblack_hosts(&text);
        if hosts.len() < BLOCKLIST_MIN_SANE_ENTRIES {
            return Err(format!(
                "parsed only {} entries (expected at least {BLOCKLIST_MIN_SANE_ENTRIES}) - looks like a bad or truncated fetch, keeping the current list",
                hosts.len()
            ));
        }
        for host in HAND_PICKED_BLOCKLIST_HOSTS {
            hosts.insert(host.to_string());
        }
        let count = hosts.len();
        write_blocklist_to_disk(app, &hosts)?;
        *blocklist().write_recover() = hosts;
        Ok(count)
    })();

    let payload = match outcome {
        Ok(count) => {
            let refreshed_at = now_ms();
            {
                let state = app.state::<SharedAppData>();
                let mut st = state.lock_recover();
                st.data.last_blocklist_refresh = Some(refreshed_at);
                st.data.blocklist_entry_count = Some(count);
            }
            save_persisted_data(app);
            BlocklistRefreshResult { success: true, entry_count: count, refreshed_at, error: None }
        }
        Err(e) => {
            eprintln!("[kite] blocklist refresh failed: {e}");
            BlocklistRefreshResult { success: false, entry_count: 0, refreshed_at: now_ms(), error: Some(e) }
        }
    };
    let _ = app.emit_to(MAIN_WEBVIEW_LABEL, "blocklist-refreshed", payload);
}

// Fetches a fresh copy of the blocklist from upstream and, if it all
// checks out, swaps it into the live BLOCKLIST and writes it to disk so
// it survives restarts (see perform_blocklist_refresh for the actual
// work). Runs entirely on a background thread (same reasoning as
// report_favicon's fetch: this dispatches on the invoke_handler thread,
// and a blocking network call there would stall every other command
// until it finished), so this returns immediately and reports the real
// outcome via a "blocklist-refreshed" event instead of its own Result.
#[tauri::command]
fn refresh_blocklist(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    std::thread::spawn(move || perform_blocklist_refresh(&app));
    Ok(())
}

// How old last_blocklist_refresh has to be before startup triggers an
// automatic background refresh - arbitrary starting point, trivial to
// change (or make user-configurable in Settings) later.
const BLOCKLIST_AUTO_REFRESH_INTERVAL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

// Called once from setup(), after the window and initial tabs already
// exist - never delays first paint, since it only ever spawns a
// background thread and returns immediately either way. Silent by
// design: this is a background maintenance check, not a user action, so
// unlike refresh_blocklist there's no button to show "Checking..." on -
// if Settings happens to be open when it lands, the existing
// "blocklist-refreshed" listener picks it up anyway.
fn maybe_auto_refresh_blocklist_on_startup(app: &tauri::AppHandle) {
    let is_stale = {
        let state = app.state::<SharedAppData>();
        let st = state.lock_recover();
        match st.data.last_blocklist_refresh {
            None => true,
            Some(last) => now_ms().saturating_sub(last) >= BLOCKLIST_AUTO_REFRESH_INTERVAL_MS,
        }
    };
    if !is_stale {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || perform_blocklist_refresh(&app));
}

// Opens a native folder picker (tauri-plugin-dialog) for the user to pick
// where downloads should be saved, then validates and persists the choice.
// Returns the chosen path, or None if the user cancelled the dialog.
//
// NOTE: FileDialogBuilder's pick_folder() is callback-based rather than
// returning a Future, so this blocks on a channel to turn it back into a
// plain return value - fine since (unlike new_tab/switch_tab elsewhere in
// this file) this command is NOT declared `async fn`, so Tauri dispatches
// it on its blocking-command thread pool rather than the async runtime.
// The exact callback signature (Option<FilePath> vs Option<PathBuf>) is
// the one thing here I couldn't fully pin down without the crate docs open
// - if this doesn't compile, check tauri-plugin-dialog's FileResponse/
// FilePath type for this version and adjust the .into_path() call below.
#[tauri::command]
fn choose_downloads_dir(webview: tauri::Webview, app: tauri::AppHandle) -> Result<Option<String>, String> {
    require_chrome(&webview)?;
    use tauri_plugin_dialog::DialogExt;

    let starting_dir = configured_downloads_dir(&app);
    let (tx, rx) = std::sync::mpsc::channel();

    let mut builder = app.dialog().file();
    if let Some(dir) = &starting_dir {
        builder = builder.set_directory(dir);
    }
    builder.pick_folder(move |picked| {
        let _ = tx.send(picked);
    });

    let picked = rx.recv().map_err(|e| e.to_string())?;
    let Some(file_path) = picked else {
        return Ok(None); // user cancelled
    };
    let path = file_path.into_path().map_err(|e| e.to_string())?;
    if !path.is_dir() {
        return Err("selected item isn't a folder".to_string());
    }
    let path_str = path.to_string_lossy().to_string();

    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        st.data.settings.downloads_dir = Some(path_str.clone());
    }
    save_persisted_data(&app);
    Ok(Some(path_str))
}

// --- Tabs (unchanged behaviour, plus history hooks below) ---

fn visible_position() -> LogicalPosition<f64> {
    LogicalPosition::new(0.0, CHROME_HEIGHT)
}

fn hidden_position() -> LogicalPosition<f64> {
    LogicalPosition::new(OFFSCREEN_X, CHROME_HEIGHT)
}

fn content_size(win_w: f64, win_h: f64) -> LogicalSize<f64> {
    LogicalSize::new(win_w, (win_h - CHROME_HEIGHT).max(0.0))
}

fn search_url_for(engine: &str, query: &str) -> String {
    let q = urlencode(query);
    match engine {
        "bing" => format!("https://www.bing.com/search?q={q}"),
        "duckduckgo" => format!("https://duckduckgo.com/?q={q}"),
        // "google" and any unrecognized value both fall back to Google -
        // set_search_engine already rejects unknown values before they're
        // ever persisted, so this arm is just a safe default, not a real
        // "unknown engine" case.
        _ => format!("https://www.google.com/search?q={q}"),
    }
}

fn normalize_url(input: &str, search_engine: &str) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else if trimmed.contains('.') && !trimmed.contains(' ') {
        format!("https://{trimmed}")
    } else {
        search_url_for(search_engine, trimmed)
    }
}

fn current_search_engine(app: &tauri::AppHandle) -> String {
    let state = app.state::<SharedAppData>();
    let st = state.lock_recover();
    st.data.settings.search_engine.clone()
}

// What a brand-new tab (New Tab, or navigating the current tab Home)
// should load, per the Startup section of kite://settings. This is NOT
// used to decide what reopens at app *launch* when "restore" is picked -
// see initial_session_tabs below for that - a tab opened mid-session has
// no "last session" to reopen, so "restore" falls back to Home here, the
// same as plain "home" mode.
fn startup_target(app: &tauri::AppHandle) -> String {
    let state = app.state::<SharedAppData>();
    let st = state.lock_recover();
    match st.data.settings.homepage_mode.as_str() {
        "custom" if !st.data.settings.homepage_url.is_empty() => {
            st.data.settings.homepage_url.clone()
        }
        "blank" => BLANK_URL.to_string(),
        // "home", "restore" (see comment above), plus "custom" with no URL
        // saved yet (shouldn't normally happen - set_homepage validates -
        // but falls back safely).
        _ => HOME_URL.to_string(),
    }
}

// What to (re)open at app launch: normally just startup_target's single
// URL as a one-tab list, but "Continue where you left off" instead
// reopens every tab from the last saved session (see save_session_snapshot),
// falling back to a single Home tab if there's no saved session yet (first
// run, or the user just switched into this mode and hasn't restarted
// since). Returns the URLs to open, in order, plus which index among them
// should end up active. Only ever called once, from setup().
fn initial_session_tabs(app: &tauri::AppHandle) -> (Vec<String>, usize) {
    {
        let state = app.state::<SharedAppData>();
        let st = state.lock_recover();
        if st.data.settings.homepage_mode == "restore" && !st.data.session.tabs.is_empty() {
            let urls: Vec<String> = st.data.session.tabs.iter().map(|t| t.url.clone()).collect();
            let active_index = st.data.session.active_index.min(urls.len() - 1);
            return (urls, active_index);
        }
    }
    (vec![startup_target(app)], 0)
}

// Snapshots the current tab list (URLs + which one is active) into
// PersistedData so "Continue where you left off" can rebuild it after a
// restart. Piggybacks on emit_tabs_changed, which already fires on every
// navigation, title change, and tab open/close/switch (see
// create_tab_webview's on_navigation/on_document_title_changed and every
// *_tab command below) - so this stays current without needing its own
// set of call sites. Writes to disk on every call rather than debouncing,
// same tradeoff record_history/update_history_title already make
// elsewhere in this file: simpler, and a session snapshot changes far
// less often than, say, every keystroke.
fn save_session_snapshot(app: &tauri::AppHandle) {
    let (tabs, active_index) = {
        let state = app.state::<SharedTabState>();
        let st = state.lock_recover();
        // Private tabs are deliberately never part of what gets restored
        // on next launch - same reasoning as everywhere else is_private
        // gates persistence. active_index is computed against this
        // filtered sequence (not st.tabs' original positions), since a
        // private tab earlier in the strip would otherwise throw off
        // every later index; if the active tab itself is private,
        // position() finds nothing and this falls back to index 0.
        let tabs: Vec<SessionTab> = st
            .tabs
            .iter()
            .filter(|t| !t.is_private)
            .map(|t| SessionTab { url: t.url.clone() })
            .collect();
        let active_index = st
            .tabs
            .iter()
            .filter(|t| !t.is_private)
            .position(|t| t.label == st.active)
            .unwrap_or(0);
        (tabs, active_index)
    };
    {
        let state = app.state::<SharedAppData>();
        let mut st = state.lock_recover();
        st.data.session.tabs = tabs;
        st.data.session.active_index = active_index;
    }
    save_persisted_data(app);
}


// Custom homepage URLs are typed by hand in Settings, so unlike
// normalize_url() (used for the address bar / home search box, where an
// ambiguous input is assumed to be a search query) an ambiguous input here
// is just rejected - a homepage is expected to be an actual URL, not a
// search term.
fn normalize_homepage_url(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("enter a URL for the custom homepage".to_string());
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Ok(trimmed.to_string());
    }
    if trimmed.contains('.') && !trimmed.contains(' ') {
        return Ok(format!("https://{trimmed}"));
    }
    Err(format!("'{trimmed}' doesn't look like a valid URL"))
}

fn urlencode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => "+".to_string(),
            c if c.is_alphanumeric() => c.to_string(),
            c => format!("%{:02X}", c as u32),
        })
        .collect()
}

// Resolves the folder downloads should land in: the user's chosen folder
// from kite://settings if one is set and still exists, otherwise the OS
// Downloads folder. Falls back the same way resolve_download_path always
// has if even that can't be resolved (unusual, but e.g. some Linux setups
// have no XDG Downloads dir configured).
fn configured_downloads_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let custom = {
        let state = app.state::<SharedAppData>();
        let st = state.lock_recover();
        st.data.settings.downloads_dir.clone()
    };
    if let Some(dir) = custom {
        let path = PathBuf::from(&dir);
        if path.is_dir() {
            return Some(path);
        }
        eprintln!(
            "[kite] configured downloads folder no longer exists, falling back to the OS default: {dir}"
        );
    }
    app.path().download_dir().ok()
}

// Redirects a download into the configured Downloads folder, keeping the
// name WebView2/WKWebView/WebKitGTK suggested (carried in `suggested`'s file
// name) but avoiding silently overwriting an existing file by appending
// " (1)", " (2)", etc. Returns None if no Downloads folder can be
// resolved, in which case the caller falls back to whatever default
// destination the platform already picked.
fn resolve_download_path(app: &tauri::AppHandle, suggested: &std::path::Path) -> Option<PathBuf> {
    let downloads_dir = configured_downloads_dir(app)?;
    let file_name = suggested.file_name()?.to_string_lossy().to_string();
    let (stem, ext) = match file_name.rfind('.') {
        Some(i) if i > 0 => (file_name[..i].to_string(), file_name[i..].to_string()),
        _ => (file_name.clone(), String::new()),
    };

    let mut candidate = downloads_dir.join(&file_name);
    let mut n = 1;
    while candidate.exists() {
        candidate = downloads_dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    Some(candidate)
}

fn emit_tabs_changed(app: &tauri::AppHandle) {
    let state = app.state::<SharedTabState>();
    let st = state.lock_recover();
    let payload = json!({ "tabs": st.tabs.clone(), "active": st.active.clone() });
    drop(st);
    let _ = app.emit_to(MAIN_WEBVIEW_LABEL, "tabs-changed", payload);
    save_session_snapshot(app);
}

fn emit_active_url(app: &tauri::AppHandle) {
    let state = app.state::<SharedTabState>();
    let st = state.lock_recover();
    let url = if st.library_tab.as_deref() == Some(st.active.as_str()) {
        // The Library Panel is currently showing for this tab (either
        // just opened, or we've switched back to a tab it was parked on
        // - see activate_tab) - the address bar should reflect that
        // (kite://<view>), not the tab's own real URL, which is hidden
        // underneath and not what's actually on screen right now.
        Some(format!("kite://{}", st.library_view))
    } else {
        st.tabs.iter().find(|t| t.label == st.active).map(|t| t.url.clone())
    };
    if let Some(url) = url {
        drop(st);
        let _ = app.emit_to(MAIN_WEBVIEW_LABEL, "url-changed", url);
    }
}

// Lets the toolbar's zoom-level indicator stay in sync with whichever tab
// is active - each content webview remembers its own zoom natively, but
// chrome (a separate webview) has no way to read that back, so we mirror
// it ourselves in TabInfo.zoom and push it over on every switch.
fn emit_zoom_changed(app: &tauri::AppHandle) {
    let state = app.state::<SharedTabState>();
    let st = state.lock_recover();
    if let Some(tab) = st.tabs.iter().find(|t| t.label == st.active) {
        let zoom = tab.zoom;
        drop(st);
        let _ = app.emit_to(MAIN_WEBVIEW_LABEL, "zoom-changed", zoom);
    }
}

fn is_home_asset_url(url: &str) -> bool {
    // The bundled home.html resolves to a platform-specific internal
    // scheme (tauri://localhost/home.html on macOS/Linux,
    // https://tauri.localhost/home.html on Windows) - match on the
    // filename rather than hardcoding either host.
    url.ends_with("home.html") || url.ends_with("home.html/")
}

fn is_crashed_asset_url(url: &str) -> bool {
    url.ends_with("crashed.html") || url.ends_with("crashed.html/")
}

// The home page loads inside a content-tab webview, which (see
// capabilities/chrome.json) has no Tauri IPC access at all - any tab can
// browse to an arbitrary site, so tabs get zero command access by design.
// That means home.js can't invoke("get_settings") itself for the search
// engine its own search box needs. Instead we push the value in directly
// via webview.eval() whenever the home page loads. Called from both
// on_navigation and on_document_title_changed since eval timing relative
// to the page's own script executing isn't guaranteed - home.js checks
// for the value on load and also listens for the event.
fn push_search_engine_to_home(app: &tauri::AppHandle, label: &str) {
    let engine = current_search_engine(app);
    let json = match serde_json::to_string(&engine) {
        Ok(j) => j,
        Err(_) => return,
    };
    if let Some(webview) = app.get_webview(label) {
        let script = format!(
            "window.__KITE_SEARCH_ENGINE__ = {json}; window.dispatchEvent(new CustomEvent('kite-search-engine'));"
        );
        let _ = webview.eval(&script);
    }
}

// Same mechanism as push_search_engine_to_home -
// crashed.html runs in a content-tab webview with zero Tauri IPC (see
// capabilities/content.json), so its own Reload button can't invoke() to
// find out what to reload. This pushes the tab's real URL (kept in
// TabInfo.url the whole time a tab is "crashed" - see on_navigation) in
// directly via webview.eval(). Called from both on_navigation and
// on_document_title_changed for the same reason those two call the
// bookmarks/search-engine pushes from both places: eval timing relative
// to the page's own script executing isn't guaranteed.
fn push_crashed_url_to_page(app: &tauri::AppHandle, label: &str) {
    let real_url = {
        let state = app.state::<SharedTabState>();
        let st = state.lock_recover();
        st.tabs.iter().find(|t| t.label == label).map(|t| t.url.clone())
    };
    let Some(real_url) = real_url else { return };
    let json = match serde_json::to_string(&real_url) {
        Ok(j) => j,
        Err(_) => return,
    };
    if let Some(webview) = app.get_webview(label) {
        let script = format!("window.__KITE_CRASHED_URL__ = {json};");
        let _ = webview.eval(&script);
    }
}

fn create_tab_webview(app: &tauri::AppHandle, url: &str, private: bool) -> Result<String, String> {
    let window = app.get_window("main").ok_or("main window missing")?;

    let label = {
        let state = app.state::<SharedTabState>();
        let mut st = state.lock_recover();
        let id = st.next_id;
        st.next_id += 1;
        format!("content-{id}")
    };

    let webview_url = if url == HOME_URL {
        WebviewUrl::App("home.html".into())
    } else if url == CRASHED_ASSET_MARKER {
        WebviewUrl::App("crashed.html".into())
    } else {
        let parsed = Url::parse(url).map_err(|e| e.to_string())?;
        WebviewUrl::External(parsed)
    };

    let (win_w, win_h) = {
        let state = app.state::<SharedTabState>();
        let st = state.lock_recover();
        (st.window_size.0, st.window_size.1)
    };

    let app_for_nav = app.clone();
    let label_for_nav = label.clone();
    let app_for_title = app.clone();
    let label_for_title = label.clone();
    let url_for_error = url.to_string();

    let content_webview = window
        .add_child(
            WebviewBuilder::new(label.clone(), webview_url)
                .initialization_script(CONTEXT_MENU_SCRIPT)
                .initialization_script(FAVICON_SCRIPT)
                .initialization_script(PASSWORD_CAPTURE_SCRIPT)
                // The actual privacy mechanism, not just a UI label - a
                // real non-persistent cookie/cache store per Tauri's docs
                // (WebView2 101.0.1210.39+ on Windows, WKWebView's
                // nonPersistent DataStore on macOS/iOS). Unsupported on
                // Android, which Kite doesn't target anyway.
                .incognito(private)
                .on_navigation(move |nav_url| {
                    eprintln!("[kite] on_navigation ({label_for_nav}): {nav_url}");
                    let raw = nav_url.to_string();
                    let is_crash_page = is_crashed_asset_url(&raw);
                    let display_url = if is_home_asset_url(&raw) {
                        HOME_URL.to_string()
                    } else {
                        raw
                    };
                    let state = app_for_nav.state::<SharedTabState>();
                    let (is_active, is_private) = {
                        let mut st = state.lock_recover();
                        if let Some(tab) = st.tabs.iter_mut().find(|t| t.label == label_for_nav) {
                            tab.crashed = is_crash_page;
                            // A navigation to the bundled crash-recovery
                            // page isn't a real site visit - keep showing
                            // the tab's last real URL (this is what makes
                            // "crashed" a state a tab is in, rather than
                            // just another tab pointed at an internal
                            // page like Home) instead of overwriting it
                            // with the recovery page's own resolved URL.
                            if !is_crash_page {
                                tab.url = display_url.clone();
                                // New page, new count - see blocked_count's
                                // own doc comment on TabInfo.
                                tab.blocked_count = 0;
                                let host = url::Url::parse(&display_url)
                                    .ok()
                                    .and_then(|u| u.host_str().map(|h| h.to_string()));
                                tab.site_allowlisted = host
                                    .map(|h| is_host_allowlisted(&app_for_nav, &h))
                                    .unwrap_or(false);
                            }
                        }
                        let is_private = st
                            .tabs
                            .iter()
                            .find(|t| t.label == label_for_nav)
                            .map(|t| t.is_private)
                            .unwrap_or(false);
                        (st.active == label_for_nav, is_private)
                    };
                    // The home page isn't a real visited site - keep it out
                    // of history the same way a browser wouldn't log its
                    // own new-tab page. Push it its search engine setting
                    // instead, since it has no IPC access to fetch that
                    // itself (bookmarks no longer go to the home page - see
                    // the chrome-side bookmarks bar instead). The crash-
                    // recovery page isn't a real visit either, and needs
                    // the same kind of push (its real URL, so its Reload
                    // button knows where to go back to). A private tab's
                    // navigations are real visits, just deliberately never
                    // written to disk - see is_private's own doc comment.
                    if is_crash_page {
                        push_crashed_url_to_page(&app_for_nav, &label_for_nav);
                    } else if display_url == HOME_URL {
                        push_search_engine_to_home(&app_for_nav, &label_for_nav);
                    } else if !is_private {
                        record_history(&app_for_nav, &display_url);
                    }
                    emit_tabs_changed(&app_for_nav);
                    if is_active {
                        emit_active_url(&app_for_nav);
                    }
                    true
                })
                .on_document_title_changed(move |_webview, title| {
                    let state = app_for_title.state::<SharedTabState>();
                    let (tab_url, is_crashed, is_private) = {
                        let mut st = state.lock_recover();
                        if let Some(tab) = st.tabs.iter_mut().find(|t| t.label == label_for_title) {
                            // Neither the home page's static <title>Kite</title>
                            // nor the crash-recovery page's own static title
                            // should overwrite the tab's displayed title -
                            // mirrors the HOME_URL check, just keyed on the
                            // crashed flag too since a crashed tab keeps its
                            // real (non-HOME_URL) url the whole time.
                            if tab.url != HOME_URL && !tab.crashed {
                                tab.title = title.clone();
                            }
                            (Some(tab.url.clone()), tab.crashed, tab.is_private)
                        } else {
                            (None, false, false)
                        }
                    };
                    emit_tabs_changed(&app_for_title);
                    if let Some(url) = tab_url {
                        if is_crashed {
                            push_crashed_url_to_page(&app_for_title, &label_for_title);
                        } else if url == HOME_URL {
                            push_search_engine_to_home(&app_for_title, &label_for_title);
                        } else if !is_private {
                            // Guards against more than just "don't record
                            // this" - without this check, a private tab
                            // that happens to visit a URL already present
                            // in real history (a page you'd visited before
                            // going private) would overwrite that real
                            // entry's title via the .find() below, since
                            // update_history_title matches on URL alone
                            // with no notion of which tab it came from.
                            update_history_title(&app_for_title, &url, &title);
                        }
                    }
                })
                // Step 1: redirect into the Downloads folder and log
                // request/completion so we can confirm files land in the
                // right place before building the Downloads list UI.
                .on_download(|webview, event| {
                    match event {
                        DownloadEvent::Requested { url, destination } => {
                            let app = webview.app_handle();
                            match resolve_download_path(app, destination) {
                                Some(path) => {
                                    eprintln!(
                                        "[kite] download requested: {url} -> {}",
                                        path.display()
                                    );
                                    *destination = path;
                                }
                                None => {
                                    eprintln!(
                                        "[kite] download requested: {url} (Downloads folder not resolved, using default: {})",
                                        destination.display()
                                    );
                                }
                            }
                        }
                        DownloadEvent::Finished { url, path, success } => {
                            eprintln!(
                                "[kite] download finished: {url} -> {path:?}, success={success}"
                            );
                            let app = webview.app_handle();
                            let is_private = {
                                let state = app.state::<SharedTabState>();
                                let st = state.lock_recover();
                                st.tabs
                                    .iter()
                                    .find(|t| t.label == webview.label())
                                    .map(|t| t.is_private)
                                    .unwrap_or(false)
                            };
                            if is_private {
                                eprintln!("[kite] download from private tab, not logging: {url}");
                            } else {
                                record_download(app, url.as_str(), path.as_deref(), success);
                            }
                        }
                        _ => {}
                    }
                    true
                }),
            hidden_position(),
            content_size(win_w, win_h),
        )
        .map_err(|e| format!("add_child failed for {url_for_error}: {e}"))?;

    // Windows-only, and the least-confirmed code in this project - see
    // watch_for_crash's own doc comment for why and what to do if it
    // doesn't compile as-is.
    #[cfg(windows)]
    watch_for_crash(&content_webview, app.clone(), label.clone());

    // Content blocking (see watch_for_requests) - installed the same way
    // as watch_for_crash above, right after the webview exists. Phase 1
    // (log-only) confirmed the hook fires; this now actually cancels
    // matched requests.
    #[cfg(windows)]
    watch_for_requests(&content_webview, app.clone(), label.clone());

    {
        let state = app.state::<SharedTabState>();
        let mut st = state.lock_recover();
        st.tabs.push(TabInfo {
            label: label.clone(),
            title: "New Tab".into(),
            url: url.to_string(),
            zoom: 1.0,
            crashed: false,
            favicon: None,
            is_private: private,
            blocked_count: 0,
            site_allowlisted: false,
        });
    }

    Ok(label)
}

fn activate_tab(app: &tauri::AppHandle, label: &str) -> Result<(), String> {
    let (prev_active, win_w, win_h, library_tab) = {
        let state = app.state::<SharedTabState>();
        let mut st = state.lock_recover();
        let prev = st.active.clone();
        st.active = label.to_string();
        (prev, st.window_size.0, st.window_size.1, st.library_tab.clone())
    };

    // The Library Panel (History/Bookmarks/Downloads/Settings) is a
    // full-window takeover of the chrome webview pinned to whichever tab
    // it was opened on (see library_tab's own comment on TabState), not
    // something that belongs to any tab's own content. Every tab-switch
    // path routes through this one function, so handling it here - rather
    // than separately in new_tab/switch_tab/close_tab/etc. - is the one
    // place that can't be missed.
    let is_library_tab = library_tab.as_deref() == Some(label);
    let was_library_tab = library_tab.as_deref() == Some(prev_active.as_str());

    if is_library_tab {
        // Switching back to the tab the panel is pinned to - re-expand
        // chrome and let it reappear. The frontend's own "open"/
        // "library-mode" CSS was never removed while parked (only a real
        // close via hide_library does that), so nothing needs to be
        // re-sent to it here - it becomes visible again the instant
        // chrome's native size covers the window again. This tab's own
        // content webview is deliberately left alone below (not
        // repositioned/resized/focused) - it stays hidden underneath,
        // exactly like show_library_impl leaves it.
        if let Some(chrome) = app.get_webview(MAIN_WEBVIEW_LABEL) {
            let _ = chrome.set_size(LogicalSize::new(win_w, win_h));
        }
    } else if was_library_tab {
        // Switching away from the tab the panel is pinned to - shrink
        // chrome back down so the newly-active tab isn't hidden
        // underneath it (the original bug), but deliberately don't touch
        // library_tab or tell the frontend anything - it's parked, not
        // closed, and reopens automatically if the user comes back to
        // that tab. Chrome being shrunk means none of the frontend's
        // still-"open" state is even visible in the meantime regardless.
        if let Some(chrome) = app.get_webview(MAIN_WEBVIEW_LABEL) {
            let _ = chrome.set_size(LogicalSize::new(win_w, CHROME_HEIGHT));
        }
    }

    if !prev_active.is_empty() && prev_active != label {
        if let Some(prev_webview) = app.get_webview(&prev_active) {
            let _ = prev_webview.set_position(hidden_position());
        }
    }

    if !is_library_tab {
        if let Some(webview) = app.get_webview(label) {
            webview
                .set_position(visible_position())
                .map_err(|e| e.to_string())?;
            webview
                .set_size(content_size(win_w, win_h))
                .map_err(|e| e.to_string())?;
            // Positioning/resizing only makes it visible - it doesn't transfer
            // OS keyboard focus, which otherwise stays wherever it was before
            // the switch (a previous tab, or the chrome webview if switched
            // via a keyboard shortcut rather than a click into the page
            // itself). Without this, F12/typing/etc. keep targeting whatever
            // had focus before, not the tab now on screen - only a manual
            // click into the new tab's content was fixing that, which is
            // itself just an ordinary focus-follows-click, not activate_tab
            // doing anything right. Non-fatal if it fails (e.g. webview
            // briefly not ready) - the tab still displays correctly either
            // way, it just wouldn't have focus yet.
            let _ = webview.set_focus();
        }
    }

    emit_zoom_changed(app);

    Ok(())
}

// Swaps a tab's content over to the bundled crash-recovery page, in place -
// called once we've decided (see watch_for_crash) that its content
// webview's render process has died. Mirrors go_home's approach rather
// than calling webview.navigate() directly on the existing (crashed)
// webview: that would need the crashed page's exact platform-resolved
// asset URL (see is_home_asset_url's comment on why that's awkward), so -
// same as go_home - this builds a fresh webview at CRASHED_ASSET_MARKER
// and splices it into the old tab's spot, closing the old one after.
fn show_crashed_page(app: &tauri::AppHandle, old_label: &str) {
    let (real_url, real_title, real_is_private) = {
        let state = app.state::<SharedTabState>();
        let st = state.lock_recover();
        match st.tabs.iter().find(|t| t.label == old_label) {
            Some(t) => (t.url.clone(), t.title.clone(), t.is_private),
            None => return, // tab was already closed
        }
    };

    let new_label = match create_tab_webview(app, CRASHED_ASSET_MARKER, real_is_private) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[kite] failed to show crashed page for {old_label}: {e}");
            return;
        }
    };

    let was_active = {
        let state = app.state::<SharedTabState>();
        let mut st = state.lock_recover();
        let old_idx = st.tabs.iter().position(|t| t.label == old_label);
        // create_tab_webview always appends the new tab at the end of the
        // list, so new_idx is always > old_idx here - removing it first
        // doesn't shift old_idx before we insert at it.
        let new_idx = st.tabs.iter().position(|t| t.label == new_label);
        if let (Some(old_i), Some(new_i)) = (old_idx, new_idx) {
            let mut new_tab_info = st.tabs.remove(new_i);
            new_tab_info.url = real_url;
            new_tab_info.title = real_title;
            new_tab_info.crashed = true;
            st.tabs.insert(old_i, new_tab_info);
            st.tabs.remove(old_i + 1);
        }
        st.active == old_label
    };

    if let Some(old_webview) = app.get_webview(old_label) {
        let _ = old_webview.close();
    }

    if was_active {
        let _ = activate_tab(app, &new_label);
    }

    push_crashed_url_to_page(app, &new_label);
    emit_tabs_changed(app);
    emit_active_url(app);
}

// WebView2's own render-process-crash signal isn't exposed through
// Tauri/wry's cross-platform Webview API - the only documented way to
// reach it is with_webview()'s escape hatch down to the raw WebView2 COM
// object (see tauri::webview::Webview::with_webview's own doc example,
// which uses this same ICoreWebView2Controller/webview2-com pattern for
// SetZoomFactor).
//
// Round 5: CoreWebView2() itself needs an unsafe block too, same as
// add_ProcessFailed below - both are raw COM calls.
#[cfg(windows)]
fn watch_for_crash(webview: &tauri::Webview, app: tauri::AppHandle, label: String) {
    let _ = webview.with_webview(move |platform_webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED;
        use webview2_com::ProcessFailedEventHandler;

        // ICoreWebView2 isn't handed to this closure directly - only the
        // controller is, matching Tauri's own with_webview() doc example
        // (which calls .controller().SetZoomFactor(...) the same way).
        // ICoreWebView2 comes from the controller's own CoreWebView2()
        // getter.
        let controller = platform_webview.controller();
        let core = match unsafe { controller.CoreWebView2() } {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[kite] watch_for_crash: couldn't get CoreWebView2: {e:?}");
                return;
            }
        };
        let app = app.clone();
        let label = label.clone();
        // The interface type ICoreWebView2ProcessFailedEventHandler has
        // no ::create - that's webview2-com's ergonomic wrapper struct
        // (ProcessFailedEventHandler, from its callback.rs, generated by
        // its #[event_callback] macro) implementing that interface, with
        // the closure taking (sender, args) - not just args.
        let handler = ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
            if let Some(args) = args {
                let mut kind = Default::default();
                unsafe {
                    let _ = args.ProcessFailedKind(&mut kind);
                }
                if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED {
                    show_crashed_page(&app, &label);
                }
            }
            Ok(())
        }));
        // Round 4: add_ProcessFailed's actual signature in this resolved
        // version (per the compiler's own bindings.rs excerpt) takes the
        // token as a bare `*mut i64`, not an EventRegistrationToken
        // struct - the previous round's import was for a version of this
        // crate we're no longer pulling in at all (see Cargo.toml).
        // &mut i64 auto-coerces to *mut i64 at the call site.
        let mut token: i64 = 0;
        unsafe {
            let _ = core.add_ProcessFailed(&handler, &mut token);
        }
    });
}

// Embedded at compile time - see blocklist.txt's own header for source,
// license, and how to refresh it. This is only the seed/fallback list -
// see BLOCKLIST below for the live, swappable copy that refresh_blocklist
// replaces at runtime once it exists (this const never changes after
// compilation, same as before this feature).
const BLOCKLIST_SRC: &str = include_str!("blocklist.txt");

// File name for the on-disk override written once a runtime refresh
// succeeds - see blocklist_update_path/load_initial_blocklist. Lives
// next to kite_data.json in the app-data dir, but is deliberately a
// separate plain-text file rather than a field on PersistedData: at ~99k
// lines it has no business being serialized as a JSON string inside
// kite_data.json on every save.
const BLOCKLIST_UPDATE_FILE_NAME: &str = "blocklist_update.txt";

// A fetched/parsed list smaller than this is treated as a bad or
// truncated response rather than a real update - StevenBlack/hosts is
// ~99k entries, so anything drastically below that almost certainly
// means the fetch got an error/login page or was cut off mid-transfer.
// Also used to validate the on-disk override at startup, so a corrupted
// or hand-edited-into-garbage override file can't silently gut content
// blocking either.
const BLOCKLIST_MIN_SANE_ENTRIES: usize = 50_000;

// Swappable at runtime (see refresh_blocklist, added on top of this) -
// an RwLock rather than a plain HashSet since is_blocked_host sits on
// the hot path for every intercepted request across every tab (many
// concurrent readers; a refresh replacing the whole set is rare). The
// OnceLock only defers allocating the RwLock itself to first use;
// load_initial_blocklist (called once from setup(), before any tab
// exists to possibly trigger a request) overwrites its *contents*
// directly, so the lazy default built here is only ever a fallback in
// case that somehow didn't run first.
static BLOCKLIST: std::sync::OnceLock<std::sync::RwLock<std::collections::HashSet<String>>> =
    std::sync::OnceLock::new();

fn parse_blocklist_text(text: &str) -> std::collections::HashSet<String> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_lowercase())
        .collect()
}

fn blocklist() -> &'static std::sync::RwLock<std::collections::HashSet<String>> {
    BLOCKLIST.get_or_init(|| std::sync::RwLock::new(parse_blocklist_text(BLOCKLIST_SRC)))
}

fn blocklist_update_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(BLOCKLIST_UPDATE_FILE_NAME))
}

// Called once from setup(), before the window/any tab webview exists -
// checks for a previously-refreshed list on disk and seeds BLOCKLIST
// from it instead of the compiled-in BLOCKLIST_SRC, if it's present and
// parses to a sane size. Silently falls back to the compiled-in list on
// any failure (no file yet, unreadable, too small/corrupted) - a refresh
// that's never succeeded, or a bad override file, should never leave
// content blocking worse off than a fresh install would be.
fn load_initial_blocklist(app: &tauri::AppHandle) {
    let Ok(path) = blocklist_update_path(app) else { return };
    let Ok(text) = fs::read_to_string(&path) else { return };
    let parsed = parse_blocklist_text(&text);
    let count = parsed.len();
    if count < BLOCKLIST_MIN_SANE_ENTRIES {
        eprintln!(
            "[kite] ignoring {BLOCKLIST_UPDATE_FILE_NAME} ({count} entries, expected at least {BLOCKLIST_MIN_SANE_ENTRIES}) - using bundled blocklist instead"
        );
        return;
    }
    *blocklist().write_recover() = parsed;
    eprintln!("[kite] loaded refreshed blocklist from disk: {count} entries");
}

// Matches a request host against the blocklist: either the host itself is
// listed, or one of its parent domains is (so "googlesyndication.com"
// being listed also blocks "pagead2.googlesyndication.com" without every
// subdomain needing its own entry). Walks up the label chain doing O(1)
// HashSet lookups rather than the old approach's per-entry string
// scan/format! - fine at 12 entries, not at 99k.
fn is_blocked_host(host: &str) -> bool {
    let host = host.to_lowercase();
    let list = blocklist().read_recover();
    if list.contains(&host) {
        return true;
    }
    let mut rest = host.as_str();
    while let Some(dot) = rest.find('.') {
        rest = &rest[dot + 1..];
        if rest.is_empty() {
            break;
        }
        if list.contains(rest) {
            return true;
        }
    }
    false
}

// Checks a host against the per-site allowlist (PersistedData.
// content_blocking_allowlist). Matches in both directions - host is (or
// is a subdomain of) an allowlisted entry, OR the allowlisted entry is a
// subdomain of host - rather than exact-string-only. That's a deliberate
// simplification: this is plain hostname comparison, not real
// public-suffix-list-aware "registrable domain" matching (no dependency
// on a PSL crate/data file for that yet), so allowlisting "www.
// nytimes.com" won't automatically cover "cooking.nytimes.com" - only
// exact matches and this host's own subdomains. Good enough for "turn
// blocking off for the site I'm actually on" in the common case; a real
// eTLD+1-aware allowlist is a possible future improvement, not this pass.
fn host_matches_allowlist(host: &str, allowlist: &[String]) -> bool {
    let host = host.to_lowercase();
    allowlist.iter().any(|entry| {
        let entry = entry.to_lowercase();
        host == entry || host.ends_with(&format!(".{entry}")) || entry.ends_with(&format!(".{host}"))
    })
}

// Single-host check against the live persisted allowlist - used from
// on_navigation, where recomputing per navigation (not per request) is
// cheap enough to just fetch the current list fresh each time.
fn is_host_allowlisted(app: &tauri::AppHandle, host: &str) -> bool {
    let allowlist = {
        let state = app.state::<SharedAppData>();
        let st = state.lock_recover();
        st.data.content_blocking_allowlist.clone()
    };
    host_matches_allowlist(host, &allowlist)
}

// Recomputes site_allowlisted for every open tab against the current
// allowlist - called after toggle_site_allowlist changes it, since the
// same host could be open in more than one tab (a toggle in one tab
// should be reflected everywhere that host is showing, not just the tab
// that triggered it). Fetches the allowlist once rather than per-tab, to
// avoid re-locking SharedAppData in a loop.
fn refresh_site_allowlisted_for_all_tabs(app: &tauri::AppHandle) {
    let allowlist = {
        let state = app.state::<SharedAppData>();
        let st = state.lock_recover();
        st.data.content_blocking_allowlist.clone()
    };
    let state = app.state::<SharedTabState>();
    let mut st = state.lock_recover();
    for tab in st.tabs.iter_mut() {
        let host = url::Url::parse(&tab.url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_lowercase()));
        tab.site_allowlisted = host
            .map(|h| host_matches_allowlist(&h, &allowlist))
            .unwrap_or(false);
    }
}

// Intercepts every outgoing resource request (main document, scripts,
// images, XHR, everything) for a content tab's webview and cancels the
// ones matching the blocklist by handing back an empty 200 response
// instead of letting them reach the network - Phase 2 of content
// blocking. Phase 1 (log every request, no blocking) confirmed this hook
// fires correctly, including for Kite's own internal traffic
// (ipc.localhost, 127.0.0.1) - so is_kite_internal below is deliberate
// defense in depth, not a hypothetical: without it, a bad future
// blocklist entry could break Kite's own IPC/asset loading, not just
// third-party ad requests.
//
// Same COM escape-hatch pattern as watch_for_crash: with_webview() ->
// controller -> CoreWebView2 -> raw ICoreWebView2 event registration.
// AddWebResourceRequestedFilter lives on ICoreWebView2_2 (added after the
// base ICoreWebView2 interface), so this needs a .cast() up first -
// confirmed working as of the Phase 1 build. get_Environment() is also on
// ICoreWebView2_2 (confirmed against Microsoft's own WebView2 sample
// code), so core2 covers both needs. CreateWebResourceResponse and
// args.SetResponse() are UNCONFIRMED against this exact webview2-com
// version - most likely spot for the next compile error if there is one.
#[cfg(windows)]
fn watch_for_requests(webview: &tauri::Webview, app: tauri::AppHandle, label: String) {
    let _ = webview.with_webview(move |platform_webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_2, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
        };
        use webview2_com::WebResourceRequestedEventHandler;
        // .cast() (QueryInterface) is a trait method, not an inherent one -
        // needs this import or it's invisible, same as any other windows-rs
        // interface cast.
        use windows::core::Interface;

        let controller = platform_webview.controller();
        let core = match unsafe { controller.CoreWebView2() } {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[kite] watch_for_requests: couldn't get CoreWebView2: {e:?}");
                return;
            }
        };

        let core2: ICoreWebView2_2 = match core.cast() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[kite] watch_for_requests: couldn't get ICoreWebView2_2: {e:?}");
                return;
            }
        };

        unsafe {
            let _ = core2.AddWebResourceRequestedFilter(
                windows::core::w!("*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
            );
        }

        // Cloned so the closure below can call .Environment() on its own
        // copy while `core` itself is still needed afterward, for
        // add_WebResourceRequested - windows-rs COM interface wrappers are
        // just refcounted pointers, so .clone() here is a cheap AddRef,
        // not a deep copy. Environment() lives on ICoreWebView2_2 (core2),
        // not the base ICoreWebView2 - confirmed against Microsoft's own
        // WebView2 sample code (QueryInterface to ICoreWebView2_2, then
        // get_Environment).
        let core2_for_handler = core2.clone();
        let app_for_handler = app.clone();
        let label_for_log = label.clone();

        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_sender, args| {
            let content_blocking_enabled = {
                let state = app_for_handler.state::<SharedAppData>();
                let st = state.lock_recover();
                st.data.settings.content_blocking_enabled
            };
            if !content_blocking_enabled {
                return Ok(());
            }

            // Per-site exception (see toggle_site_allowlist / the shield
            // badge) - reads the cached flag on TabInfo rather than
            // re-parsing tab.url and re-checking the allowlist on every
            // single resource request, since this runs a lot more often
            // than a navigation does.
            let site_allowlisted = {
                let state = app_for_handler.state::<SharedTabState>();
                let st = state.lock_recover();
                st.tabs
                    .iter()
                    .find(|t| t.label == label_for_log)
                    .map(|t| t.site_allowlisted)
                    .unwrap_or(false)
            };
            if site_allowlisted {
                return Ok(());
            }

            let Some(args) = args else { return Ok(()) };
            let Ok(request) = (unsafe { args.Request() }) else { return Ok(()) };

            let mut uri_ptr = windows::core::PWSTR::null();
            let uri = unsafe {
                if request.Uri(&mut uri_ptr).is_ok() && !uri_ptr.is_null() {
                    uri_ptr.to_string().unwrap_or_default()
                } else {
                    return Ok(());
                }
            };

            let Some(host) = url::Url::parse(&uri).ok().and_then(|u| u.host_str().map(|h| h.to_string())) else {
                return Ok(());
            };

            // Never let the blocklist matter for Kite's own traffic, no
            // matter what ends up in that list later - see the function
            // doc comment above.
            let is_kite_internal = matches!(
                host.as_str(),
                "ipc.localhost" | "127.0.0.1" | "tauri.localhost" | "localhost"
            );

            if !is_kite_internal && is_blocked_host(&host) {
                eprintln!("[kite] ({label_for_log}) blocked: {uri}");
                if let Ok(environment) = unsafe { core2_for_handler.Environment() } {
                    if let Ok(response) = unsafe {
                        environment.CreateWebResourceResponse(
                            None,
                            200,
                            windows::core::w!("OK"),
                            windows::core::w!(""),
                        )
                    } {
                        let _ = unsafe { args.SetResponse(&response) };
                        increment_blocked_count(&app_for_handler, &label_for_log);
                    }
                }
            }

            Ok(())
        }));

        // Same *mut i64 token quirk watch_for_crash already worked out
        // for add_ProcessFailed - confirmed matching here too as of the
        // Phase 1 build.
        let mut token: i64 = 0;
        unsafe {
            let _ = core.add_WebResourceRequested(&handler, &mut token);
        }
    });
}

// Builds the native popup menu shown for a given right-click target.
// target_type is already validated against a fixed set by the caller
// (report_context_menu) before reaching here.
fn build_context_menu(
    app: &tauri::AppHandle,
    target_type: &str,
    has_href: bool,
    has_src: bool,
    has_selection: bool,
) -> Result<Menu<tauri::Wry>, String> {
    let mut builder = MenuBuilder::new(app);
    builder = match target_type {
        "link" if has_href => builder
            .item(
                &MenuItem::with_id(app, "ctx-open-link", "Open Link in New Tab", true, None::<&str>)
                    .map_err(|e| e.to_string())?,
            )
            .item(
                &MenuItem::with_id(app, "ctx-copy-link", "Copy Link", true, None::<&str>)
                    .map_err(|e| e.to_string())?,
            ),
        "image" if has_src => builder
            .item(
                &MenuItem::with_id(app, "ctx-open-image", "Open Image in New Tab", true, None::<&str>)
                    .map_err(|e| e.to_string())?,
            )
            .separator()
            .item(
                &MenuItem::with_id(app, "ctx-save-image", "Save Image As...", true, None::<&str>)
                    .map_err(|e| e.to_string())?,
            )
            .item(
                &MenuItem::with_id(app, "ctx-copy-image", "Copy Image", true, None::<&str>)
                    .map_err(|e| e.to_string())?,
            )
            .item(
                &MenuItem::with_id(app, "ctx-copy-image-src", "Copy Image Address", true, None::<&str>)
                    .map_err(|e| e.to_string())?,
            ),
        "editable" if has_selection => builder
            .item(&MenuItem::with_id(app, "ctx-cut-selection", "Cut", true, None::<&str>).map_err(|e| e.to_string())?)
            .item(&MenuItem::with_id(app, "ctx-copy-selection", "Copy", true, None::<&str>).map_err(|e| e.to_string())?)
            .item(&MenuItem::with_id(app, "ctx-paste", "Paste", true, None::<&str>).map_err(|e| e.to_string())?)
            .separator()
            .item(&MenuItem::with_id(app, "ctx-select-all", "Select All", true, None::<&str>).map_err(|e| e.to_string())?),
        "editable" => builder
            .item(&MenuItem::with_id(app, "ctx-paste", "Paste", true, None::<&str>).map_err(|e| e.to_string())?)
            .item(&MenuItem::with_id(app, "ctx-select-all", "Select All", true, None::<&str>).map_err(|e| e.to_string())?),
        "selection" if has_selection => builder
            .item(
                &MenuItem::with_id(app, "ctx-copy-selection", "Copy", true, None::<&str>)
                    .map_err(|e| e.to_string())?,
            )
            .item(
                &MenuItem::with_id(app, "ctx-select-all", "Select All", true, None::<&str>)
                    .map_err(|e| e.to_string())?,
            ),
        _ => builder
            .item(&MenuItem::with_id(app, "ctx-back", "Back", true, None::<&str>).map_err(|e| e.to_string())?)
            .item(
                &MenuItem::with_id(app, "ctx-forward", "Forward", true, None::<&str>)
                    .map_err(|e| e.to_string())?,
            )
            .item(
                &MenuItem::with_id(app, "ctx-reload", "Reload", true, None::<&str>)
                    .map_err(|e| e.to_string())?,
            ),
    };
    builder.build().map_err(|e| e.to_string())
}

// Runs whichever action the user picked from the popup menu built above.
// Reads the target recorded by report_context_menu, since menu item clicks
// only carry an id, not the original click data. Save Image / Copy Image
// need to fetch the image's actual bytes (not just its URL) - see
// save_image_as/copy_image_to_clipboard below - and this handler fires
// on the main/menu-event thread, so those two are dispatched onto their
// own thread rather than blocking it on a network fetch + (for Save) a
// modal dialog.
fn handle_context_menu_event(app: &tauri::AppHandle, id: &str) {
    let target = {
        let state = app.state::<SharedContextMenu>();
        let st = state.lock_recover();
        st.clone()
    };
    let Some(target) = target else { return };
    let source_is_private = {
        let state = app.state::<SharedTabState>();
        let st = state.lock_recover();
        st.tabs
            .iter()
            .find(|t| t.label == target.source_label)
            .map(|t| t.is_private)
            .unwrap_or(false)
    };

    match id {
        "ctx-open-link" => {
            if let Some(href) = target.href {
                if let Ok(label) = create_tab_webview(app, &href, source_is_private) {
                    let _ = activate_tab(app, &label);
                    emit_tabs_changed(app);
                    emit_active_url(app);
                }
            }
        }
        "ctx-copy-link" => {
            if let Some(href) = target.href {
                let _ = app.clipboard().write_text(href);
            }
        }
        "ctx-copy-image-src" => {
            if let Some(src) = target.src {
                let _ = app.clipboard().write_text(src);
            }
        }
        "ctx-open-image" => {
            if let Some(src) = target.src {
                if let Ok(label) = create_tab_webview(app, &src, source_is_private) {
                    let _ = activate_tab(app, &label);
                    emit_tabs_changed(app);
                    emit_active_url(app);
                }
            }
        }
        "ctx-save-image" => {
            if let Some(src) = target.src {
                let app = app.clone();
                std::thread::spawn(move || save_image_as(&app, &src));
            }
        }
        "ctx-copy-image" => {
            if let Some(src) = target.src {
                let app = app.clone();
                std::thread::spawn(move || copy_image_to_clipboard(&app, &src));
            }
        }
        "ctx-copy-selection" => {
            if let Some(text) = target.selection_text {
                let _ = app.clipboard().write_text(text);
            }
        }
        "ctx-cut-selection" => {
            // Same as Copy, then remove the selected text from the page.
            // execCommand('insertText', false, '') replaces whatever's
            // currently selected with nothing, which is exactly a cut's
            // second half - and unlike execCommand('delete'), 'insertText'
            // is the same primitive ctx-paste below already relies on
            // working (see that comment for why it's the reliable one to
            // build on), so this stays consistent with it rather than
            // leaning on a second, less predictable execCommand variant.
            if let Some(text) = target.selection_text {
                let _ = app.clipboard().write_text(text);
            }
            if let Ok(w) = active_webview(app) {
                let _ = w.eval("document.execCommand('insertText', false, '')");
            }
        }
        "ctx-paste" => {
            // Reads the OS clipboard from Rust (which has real OS-level
            // access, no permission prompt involved) and inserts it via
            // execCommand('insertText', ...) rather than trying to make
            // the page's own JS call navigator.clipboard.readText() or
            // document.execCommand('paste') - both are blocked for
            // ordinary web content in Chromium-based engines (WebView2
            // included) without an explicit clipboard-read permission
            // grant, which content tabs never request. insertText simply
            // simulates typing at the current cursor position instead,
            // which needs no such permission and respects the page's own
            // undo stack the way a real paste would.
            if let Ok(text) = app.clipboard().read_text() {
                if let Ok(w) = active_webview(app) {
                    let script = format!(
                        "document.execCommand('insertText', false, {});",
                        serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".to_string())
                    );
                    let _ = w.eval(&script);
                }
            }
        }
        "ctx-select-all" => {
            if let Ok(w) = active_webview(app) {
                let _ = w.eval("document.execCommand('selectAll')");
            }
        }
        "ctx-back" => {
            if let Ok(w) = active_webview(app) {
                let _ = w.eval("history.back()");
            }
        }
        "ctx-forward" => {
            if let Ok(w) = active_webview(app) {
                let _ = w.eval("history.forward()");
            }
        }
        "ctx-reload" => {
            if let Ok(w) = active_webview(app) {
                let _ = w.eval("location.reload()");
            }
        }
        _ => {}
    }
}

fn active_webview(app: &tauri::AppHandle) -> Result<tauri::webview::Webview, String> {
    let label = {
        let state = app.state::<SharedTabState>();
        let st = state.lock_recover();
        st.active.clone()
    };
    app.get_webview(&label)
        .ok_or_else(|| "active webview not found".to_string())
}

// Fetches the bytes behind an <img> src for Save Image As / Copy Image.
// Handles both real http(s) URLs and inline `data:` URIs (common for
// small/lazy-loaded thumbnails) - the latter never touches the network.
// Returns the content-type too, when known, so the caller can guess a
// sensible file extension. Only base64-encoded data URIs are decoded;
// the rare unencoded form is passed through as raw bytes, which is
// technically wrong (it should be percent-decoded first) but harmless
// here since it's an edge case real sites almost never use.
fn fetch_image_bytes(src: &str) -> Result<(Vec<u8>, Option<String>), String> {
    if let Some(rest) = src.strip_prefix("data:") {
        let comma = rest.find(',').ok_or_else(|| "malformed data URI".to_string())?;
        let meta = &rest[..comma];
        let payload = &rest[comma + 1..];
        let content_type = meta.split(';').next().filter(|s| !s.is_empty()).map(|s| s.to_string());
        let bytes = if meta.contains("base64") {
            base64::engine::general_purpose::STANDARD
                .decode(payload)
                .map_err(|e| e.to_string())?
        } else {
            payload.as_bytes().to_vec()
        };
        Ok((bytes, content_type))
    } else {
        let parsed = Url::parse(src).map_err(|e| e.to_string())?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err("unsupported image URL scheme".to_string());
        }
        let resp = reqwest::blocking::get(src).map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("failed to fetch image: HTTP {}", resp.status()));
        }
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let bytes = resp.bytes().map_err(|e| e.to_string())?.to_vec();
        Ok((bytes, content_type))
    }
}

fn extension_for_content_type(ct: &str) -> Option<&'static str> {
    match ct.split(';').next().unwrap_or("").trim() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

// Suggested file name for the Save Image As dialog: the URL's own last
// path segment when it looks like a real file name, otherwise a generic
// name using the extension implied by the fetched content-type.
fn suggested_image_file_name(src: &str, content_type: Option<&str>) -> String {
    if !src.starts_with("data:") {
        if let Ok(parsed) = Url::parse(src) {
            if let Some(last) = parsed.path_segments().and_then(|mut s| s.next_back()) {
                if !last.is_empty() && last.contains('.') {
                    return last.to_string();
                }
            }
        }
    }
    let ext = content_type.and_then(extension_for_content_type).unwrap_or("png");
    format!("image.{ext}")
}

// Fetches the right-clicked image and lets the user pick where to save it
// via a native Save dialog, then writes the bytes there directly. This
// isn't a real WebView2-initiated download (see the on_download hook in
// create_tab_webview), so it doesn't go through resolve_download_path,
// but it's still recorded into the same Downloads list afterwards for a
// consistent history. Always called on its own thread (see
// handle_context_menu_event) since both the fetch and the modal dialog
// would otherwise block the main/menu-event thread.
fn save_image_as(app: &tauri::AppHandle, src: &str) {
    use tauri_plugin_dialog::DialogExt;

    let (bytes, content_type) = match fetch_image_bytes(src) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[kite] save image failed to fetch {src}: {e}");
            return;
        }
    };
    let file_name = suggested_image_file_name(src, content_type.as_deref());
    let starting_dir = configured_downloads_dir(app);

    let (tx, rx) = std::sync::mpsc::channel();
    let mut builder = app.dialog().file().set_file_name(&file_name);
    if let Some(dir) = &starting_dir {
        builder = builder.set_directory(dir);
    }
    builder.save_file(move |picked| {
        let _ = tx.send(picked);
    });

    let Ok(picked) = rx.recv() else { return };
    let Some(file_path) = picked else { return }; // user cancelled
    let Ok(path) = file_path.into_path() else { return };

    match fs::write(&path, &bytes) {
        Ok(()) => {
            eprintln!("[kite] image saved: {src} -> {}", path.display());
            record_download(app, src, Some(path.as_path()), true);
        }
        Err(e) => {
            eprintln!("[kite] failed to write saved image to {}: {e}", path.display());
            record_download(app, src, None, false);
        }
    }
}

// Fetches the right-clicked image and decodes it to raw RGBA for the
// system clipboard, so it can be pasted as an actual image somewhere else
// (ctx-copy-image-src already covers copying just the URL as text).
// NOTE: write_image's exact signature is the one thing here I couldn't
// fully confirm without this plugin version's docs open - if this doesn't
// compile, check tauri-plugin-clipboard-manager's ClipboardExt for the
// installed version's method name/signature and adjust this call.
fn copy_image_to_clipboard(app: &tauri::AppHandle, src: &str) {
    let (bytes, _content_type) = match fetch_image_bytes(src) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[kite] copy image failed to fetch {src}: {e}");
            return;
        }
    };
    let decoded = match image::load_from_memory(&bytes) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            eprintln!("[kite] copy image failed to decode {src}: {e}");
            return;
        }
    };
    let (width, height) = decoded.dimensions();
    let rgba = decoded.into_raw();
    if let Err(e) = app.clipboard().write_image(&Image::new(&rgba, width, height)) {
        eprintln!("[kite] copy image failed to write to clipboard: {e}");
    }
}

#[derive(Serialize)]
struct TabsSnapshot {
    tabs: Vec<TabInfo>,
    active: String,
}

// Lets main.js pull the current tab list once its own listeners are ready,
// rather than relying solely on the "tabs-changed" push event. Startup
// calls emit_tabs_changed() for the very first tab before the chrome
// webview's own JS has necessarily finished registering its listener -
// Tauri doesn't replay events emitted before a listener exists, so that
// first push can be missed and the tab bar would show nothing until the
// next tabs-changed event (e.g. opening a second tab, which is what made
// this look like "the home tab only appears once you open a new one").
#[tauri::command]
fn get_tabs(webview: tauri::Webview, app: tauri::AppHandle) -> Result<TabsSnapshot, String> {
    require_chrome(&webview)?;
    let state = app.state::<SharedTabState>();
    let st = state.lock_recover();
    Ok(TabsSnapshot {
        tabs: st.tabs.clone(),
        active: st.active.clone(),
    })
}

#[tauri::command]
async fn new_tab(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    url: Option<String>,
    private: Option<bool>,
) -> Result<String, String> {
    require_chrome(&webview)?;
    let private = private.unwrap_or(false);
    let engine = current_search_engine(&app);
    let target = url
        .map(|u| normalize_url(&u, &engine))
        .unwrap_or_else(|| startup_target(&app));
    let label = create_tab_webview(&app, &target, private)?;
    activate_tab(&app, &label)?;
    emit_tabs_changed(&app);
    emit_active_url(&app);
    Ok(label)
}

// Navigates the *current* tab to Home, in place, rather than opening a new
// one - unlike new_tab, this can't just point the existing webview at
// home.html directly (that'd need its exact resolved URL, which is
// platform-specific - see is_home_asset_url's comment). Instead it builds
// a fresh webview the same tested way new_tab does, then splices its
// TabInfo into the old tab's spot in the list (create_tab_webview always
// appends at the end, which would otherwise jump this tab to the end of
// the tab bar) and closes the old webview - so from the tab bar's
// perspective it looks and behaves like an in-place navigation.
#[tauri::command]
async fn go_home(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;

    let (old_label, is_private) = {
        let state = app.state::<SharedTabState>();
        let st = state.lock_recover();
        let is_private = st
            .tabs
            .iter()
            .find(|t| t.label == st.active)
            .map(|t| t.is_private)
            .unwrap_or(false);
        (st.active.clone(), is_private)
    };
    if old_label.is_empty() {
        return Err("no active tab".to_string());
    }

    let target = startup_target(&app);
    let new_label = create_tab_webview(&app, &target, is_private)?;

    {
        let state = app.state::<SharedTabState>();
        let mut st = state.lock_recover();
        let old_idx = st.tabs.iter().position(|t| t.label == old_label);
        let new_idx = st.tabs.iter().position(|t| t.label == new_label);
        // new_idx is always the last index (create_tab_webview just
        // pushed it) and old_idx is always earlier, so removing new_idx
        // first never invalidates old_idx.
        if let (Some(old_i), Some(new_i)) = (old_idx, new_idx) {
            let new_tab_info = st.tabs.remove(new_i);
            st.tabs.insert(old_i, new_tab_info);
            st.tabs.remove(old_i + 1);
        }
    }

    if let Some(old_webview) = app.get_webview(&old_label) {
        let _ = old_webview.close();
    }

    activate_tab(&app, &new_label)?;
    emit_tabs_changed(&app);
    emit_active_url(&app);
    Ok(())
}

#[tauri::command]
async fn switch_tab(webview: tauri::Webview, app: tauri::AppHandle, label: String) -> Result<(), String> {
    require_chrome(&webview)?;
    activate_tab(&app, &label)?;
    emit_tabs_changed(&app);
    emit_active_url(&app);
    Ok(())
}

#[tauri::command]
async fn close_tab(webview: tauri::Webview, app: tauri::AppHandle, label: String) -> Result<(), String> {
    require_chrome(&webview)?;

    // A pending capture for this tab (see report_login_submit) is only
    // useful while the tab it came from still exists to show a prompt
    // for - drop it now rather than leaving a plaintext password sitting
    // in memory indefinitely for a tab that's gone.
    {
        let pending_state = app.state::<SharedPendingLogins>();
        pending_state.lock_recover().0.remove(&label);
    }

    let (was_active, remaining_empty, next_active, closed_the_library_tab) = {
        let state = app.state::<SharedTabState>();
        let mut st = state.lock_recover();
        let idx = st.tabs.iter().position(|t| t.label == label);
        if let Some(i) = idx {
            // Same reasoning as history keeping the home page out - it's
            // not a real site, so "reopen closed tab" reopening it isn't
            // useful. Crashed tabs still get pushed: their stored url is
            // whatever real page crashed, which is exactly what reopening
            // should restore.
            let closed_url = st.tabs[i].url.clone();
            let closed_was_private = st.tabs[i].is_private;
            // Same reasoning as the home-page exclusion, plus: a private
            // tab's whole point is that closing it forgets it - "reopen
            // closed tab" bringing it back (as a private OR a regular
            // tab) would defeat that, so it's simply never pushed here.
            if closed_url != HOME_URL && !closed_was_private {
                st.closed_tabs.push(closed_url);
                let len = st.closed_tabs.len();
                if len > CLOSED_TABS_LIMIT {
                    let excess = len - CLOSED_TABS_LIMIT;
                    st.closed_tabs.drain(0..excess);
                }
            }
            st.tabs.remove(i);
        }
        // The tab being closed might be the one the Library Panel is
        // currently parked on (see library_tab on TabState) - if so,
        // there's nothing left to return to, so this needs to be a real
        // close, not just leaving it parked on a tab that no longer
        // exists.
        let closed_the_library_tab = st.library_tab.as_deref() == Some(label.as_str());
        if closed_the_library_tab {
            st.library_tab = None;
        }
        let was_active = st.active == label;
        let remaining_empty = st.tabs.is_empty();
        let next = if was_active && !remaining_empty {
            let new_idx = idx.unwrap_or(0).min(st.tabs.len() - 1);
            Some(st.tabs[new_idx].label.clone())
        } else if !was_active {
            Some(st.active.clone())
        } else {
            None
        };
        (was_active, remaining_empty, next, closed_the_library_tab)
    };

    // Mirrors hide_library's own event - the tab's gone, so if the
    // frontend's "open"/"library-mode" CSS was still sitting there from
    // being parked, it needs to be told to drop it. Harmless/no-op if the
    // library was never opened this session.
    if closed_the_library_tab {
        let _ = app.emit_to(MAIN_WEBVIEW_LABEL, "library-closed", ());
    }

    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.close();
    }

    if remaining_empty {
        let new_label = create_tab_webview(&app, HOME_URL, false)?;
        activate_tab(&app, &new_label)?;
        emit_tabs_changed(&app);
        emit_active_url(&app);
    } else if let Some(next_label) = next_active {
        if was_active {
            activate_tab(&app, &next_label)?;
        }
        emit_tabs_changed(&app);
        emit_active_url(&app);
    }

    Ok(())
}

// Ctrl+Shift+T - reopens the most recently closed tab (see close_tab's
// closed_tabs push). Only the URL is restored, not scroll position, zoom,
// or anything else about the page's prior state - same scope as any other
// browser's "reopen closed tab" for a single level of undo (no history of
// several reopens deep beyond what's still in the stack).
#[tauri::command]
async fn reopen_closed_tab(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    let url = {
        let state = app.state::<SharedTabState>();
        let mut st = state.lock_recover();
        st.closed_tabs.pop()
    };
    let Some(url) = url else {
        return Ok(());
    };
    let label = create_tab_webview(&app, &url, false)?;
    activate_tab(&app, &label)?;
    emit_tabs_changed(&app);
    emit_active_url(&app);
    Ok(())
}

// Backs Ctrl+1-8 (literal tab position, 0-based here) and Ctrl+9 (always
// the last tab, per the Chrome/Firefox convention - see the shortcut
// handler in main() for why that one's `index: None`) - a position past
// the end of the tab strip (e.g. Ctrl+8 with only 3 tabs open) is simply a
// no-op rather than an error, same as those browsers.
#[tauri::command]
async fn activate_tab_at(webview: tauri::Webview, app: tauri::AppHandle, index: Option<usize>) -> Result<(), String> {
    require_chrome(&webview)?;
    let label = {
        let state = app.state::<SharedTabState>();
        let st = state.lock_recover();
        match index {
            Some(i) => st.tabs.get(i).map(|t| t.label.clone()),
            None => st.tabs.last().map(|t| t.label.clone()),
        }
    };
    if let Some(label) = label {
        activate_tab(&app, &label)?;
        emit_tabs_changed(&app);
        emit_active_url(&app);
    }
    Ok(())
}

// Backs Ctrl+Tab / Ctrl+Shift+Tab - cycles to the next/previous tab in
// strip order, wrapping around at either end.
#[tauri::command]
async fn cycle_active_tab(webview: tauri::Webview, app: tauri::AppHandle, forward: bool) -> Result<(), String> {
    require_chrome(&webview)?;
    let label = {
        let state = app.state::<SharedTabState>();
        let st = state.lock_recover();
        if st.tabs.is_empty() {
            None
        } else {
            let current = st.tabs.iter().position(|t| t.label == st.active).unwrap_or(0);
            let len = st.tabs.len();
            let next = if forward {
                (current + 1) % len
            } else {
                (current + len - 1) % len
            };
            Some(st.tabs[next].label.clone())
        }
    };
    if let Some(label) = label {
        activate_tab(&app, &label)?;
        emit_tabs_changed(&app);
        emit_active_url(&app);
    }
    Ok(())
}

fn show_library_impl(app: tauri::AppHandle) -> Result<(), String> {
    let (win_w, win_h, active) = {
        let state = app.state::<SharedTabState>();
        let mut st = state.lock_recover();
        // Pins the library to whichever tab is active right now - if it
        // was previously pinned to some other tab, that tab silently goes
        // back to being a normal tab (there's only one library instance).
        st.library_tab = Some(st.active.clone());
        (st.window_size.0, st.window_size.1, st.active.clone())
    };
    if let Some(chrome) = app.get_webview(MAIN_WEBVIEW_LABEL) {
        chrome
            .set_size(LogicalSize::new(win_w, win_h))
            .map_err(|e| e.to_string())?;
    }
    if let Some(active_webview) = app.get_webview(&active) {
        let _ = active_webview.set_position(hidden_position());
    }
    Ok(())
}

#[tauri::command]
fn show_library(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    show_library_impl(app)
}

#[tauri::command]
fn hide_library(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    let (win_w, win_h, active) = {
        let state = app.state::<SharedTabState>();
        let mut st = state.lock_recover();
        // A real close (the "Back to browsing" button, or Esc) - unlike
        // just switching tabs away from it, this actually forgets the
        // pin, so returning to this tab later shows its own page again,
        // not the library.
        st.library_tab = None;
        (st.window_size.0, st.window_size.1, st.active.clone())
    };
    if let Some(chrome) = app.get_webview(MAIN_WEBVIEW_LABEL) {
        chrome
            .set_size(LogicalSize::new(win_w, CHROME_HEIGHT))
            .map_err(|e| e.to_string())?;
    }
    if let Some(active_webview) = app.get_webview(&active) {
        active_webview
            .set_position(visible_position())
            .map_err(|e| e.to_string())?;
        active_webview
            .set_size(content_size(win_w, win_h))
            .map_err(|e| e.to_string())?;
    }
    // The address bar was showing a kite:// page while the library was
    // open; put the active tab's real URL back now that it's visible again.
    emit_active_url(&app);
    Ok(())
}

// kite://history and kite://bookmarks aren't real navigable URLs - there's
// nothing for a content webview to load - so navigate() intercepts them
// before they'd otherwise get treated as a search query, and routes to
// the same library panel the toolbar button opens.
fn open_internal_page(app: &tauri::AppHandle, page: &str) -> Result<(), String> {
    let view = match page.trim_end_matches('/') {
        "history" => "history",
        "bookmarks" => "bookmarks",
        "downloads" => "downloads",
        "settings" => "settings",
        "passwords" => "passwords",
        other => return Err(format!("unknown internal page: kite://{other}")),
    };
    show_library_impl(app.clone())?;
    {
        let state = app.state::<SharedTabState>();
        state.lock_recover().library_view = view.to_string();
    }
    let _ = app.emit_to(MAIN_WEBVIEW_LABEL, "open-library-view", view);
    let _ = app.emit_to(MAIN_WEBVIEW_LABEL, "url-changed", format!("kite://{view}"));
    Ok(())
}

#[tauri::command]
fn navigate(webview: tauri::Webview, app: tauri::AppHandle, url: String) -> Result<(), String> {
    require_chrome(&webview)?;
    let trimmed = url.trim();
    if let Some(page) = trimmed.strip_prefix("kite://") {
        return open_internal_page(&app, page);
    }
    let target = normalize_url(&url, &current_search_engine(&app));
    let active = active_webview(&app)?;
    active
        .navigate(Url::parse(&target).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn go_back(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    active_webview(&app)?
        .eval("history.back()")
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn go_forward(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    active_webview(&app)?
        .eval("history.forward()")
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn reload(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    active_webview(&app)?
        .eval("location.reload()")
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_find_in_page(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    active_webview(&app)?
        .eval(FIND_IN_PAGE_SCRIPT)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn report_context_menu(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    target_type: String,
    href: Option<String>,
    src: Option<String>,
    selection_text: Option<String>,
    x: f64,
    y: f64,
) -> Result<(), String> {
    require_content(&webview)?;

    // Untrusted input: capabilities/content.json grants this command to
    // any http(s) origin (not just context_menu.js), since content tabs
    // load arbitrary remote sites by design. A malicious page's own JS
    // could call this directly with fabricated values, so target_type is
    // checked against a fixed set rather than trusted as-is, and href/src
    // are only ever used as opaque navigation targets / clipboard text -
    // never executed or path-concatenated.
    let target_type = match target_type.as_str() {
        "link" | "image" | "selection" | "editable" => target_type,
        _ => "page".to_string(),
    };

    {
        let state = app.state::<SharedContextMenu>();
        let mut st = state.lock_recover();
        *st = Some(ContextMenuTarget {
            href: href.clone(),
            src: src.clone(),
            selection_text: selection_text.clone(),
            source_label: webview.label().to_string(),
        });
    }

    let menu = build_context_menu(
        &app,
        &target_type,
        href.is_some(),
        src.is_some(),
        selection_text.is_some(),
    )?;

    let window = app.get_window("main").ok_or("main window missing")?;
    // x/y are relative to the content webview's own viewport; popup_menu_at
    // wants window-relative coordinates, so add the content webview's
    // fixed offset (it always sits at visible_position() while active).
    let position = Position::Logical(LogicalPosition::new(x, y + CHROME_HEIGHT));
    window
        .popup_menu_at(&menu, position)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn report_content_click(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_content(&webview)?;
    // Lets chrome-side floating UI (currently just the new-tab right-click
    // menu, see newTabMenuEl in main.js) close itself on a click anywhere
    // in the actual page - chrome's own click-away listener only ever sees
    // clicks inside its own webview (toolbar, tab bar, library panel),
    // since content-* tabs are entirely separate native child webviews
    // that never bubble anything into chrome's DOM. No payload needed:
    // this is just a "something was clicked in content" signal, not a
    // description of what.
    let _ = app.emit_to(MAIN_WEBVIEW_LABEL, "content-clicked", ());
    Ok(())
}

// Guesses a MIME type from the icon URL's extension, for the (common)
// case where the fetch response has no usable Content-Type header - most
// often plain .ico files served by static hosts with no content-type
// configured at all. "image/x-icon" is the safe fallback since a bare
// favicon.ico is the single most common case.
fn guess_favicon_mime(icon_url: &str) -> &'static str {
    let lower = icon_url.to_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "image/x-icon"
    }
}

fn build_favicon_data_url(bytes: &[u8], content_type: Option<&str>, icon_url: &str) -> String {
    let mime = content_type
        .map(|ct| ct.split(';').next().unwrap_or(ct).trim().to_string())
        .filter(|ct| ct.starts_with("image/"))
        .unwrap_or_else(|| guess_favicon_mime(icon_url).to_string());
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{mime};base64,{encoded}")
}

// Bumps a tab's blocked_count and pushes the change to the UI, same
// "tab may already be gone" guard as apply_favicon_to_tab - a blocked
// request's callback can arrive after the tab's been closed or navigated
// away. Nothing here is persisted to disk (unlike favicons/history) -
// blocked_count is a live per-page counter, not something that should
// survive a restart.
fn increment_blocked_count(app: &tauri::AppHandle, label: &str) {
    let state = app.state::<SharedTabState>();
    let found = {
        let mut st = state.lock_recover();
        match st.tabs.iter_mut().find(|t| t.label == label) {
            Some(tab) => {
                tab.blocked_count += 1;
                true
            }
            None => false,
        }
    };
    if found {
        emit_tabs_changed(app);
    }
}

// Sets a tab's favicon and pushes the change to the UI, but only if the
// tab is still around - it may have been closed, or navigated somewhere
// else entirely, by the time a background fetch (see report_favicon)
// completes. Also records it in PersistedData.favicons (page URL -> data
// URL) so bookmarks/history keep their icons across restarts - same
// save_persisted_data pattern as add_bookmark/record_history: mutate
// inside a scoped lock, then save once that lock's dropped (calling it
// while still held would deadlock on SharedAppData's own mutex).
fn apply_favicon_to_tab(app: &tauri::AppHandle, label: &str, data_url: &str) {
    let state = app.state::<SharedTabState>();
    let (found, persist_url) = {
        let mut st = state.lock_recover();
        match st.tabs.iter_mut().find(|t| t.label == label) {
            Some(tab) => {
                tab.favicon = Some(data_url.to_string());
                // Still shown live in the tab either way - just never
                // written to disk for a private tab.
                let persist_url = if tab.is_private { None } else { Some(tab.url.clone()) };
                (true, persist_url)
            }
            None => (false, None),
        }
    };
    if !found {
        return;
    }
    if let Some(page_url) = persist_url {
        let app_state = app.state::<SharedAppData>();
        let mut st = app_state.lock_recover();
        st.data.favicons.insert(page_url, data_url.to_string());
        drop(st);
        save_persisted_data(app);
    }
    emit_tabs_changed(app);
}

// Caps how large a "favicon" we'll accept - real favicons are a few KB;
// this just guards against a misbehaving or hostile site handing back
// something large under that URL.
const MAX_FAVICON_BYTES: usize = 512 * 1024;

#[tauri::command]
fn report_favicon(webview: tauri::Webview, app: tauri::AppHandle, href: String) -> Result<(), String> {
    require_content(&webview)?;

    // Untrusted input, same caveat as report_context_menu - favicon.js
    // reports this, but a malicious remote page's own JS could call this
    // command directly too, so it's parsed and scheme-checked rather than
    // trusted outright.
    let Ok(parsed) = Url::parse(&href) else {
        return Ok(());
    };
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Ok(());
    }
    let icon_url = parsed.to_string();
    let label = webview.label().to_string();

    // Skip Kite's own internal pages (home/crashed) - favicon.js's
    // /favicon.ico fallback fires for them too (there's a dev server
    // behind kite://home locally), but they're not real sites and the UI
    // has no use for an icon on them.
    let should_process = {
        let state = app.state::<SharedTabState>();
        let st = state.lock_recover();
        st.tabs
            .iter()
            .find(|t| t.label == label)
            .map(|t| t.url != HOME_URL && !t.crashed)
            .unwrap_or(false)
    };
    if !should_process {
        return Ok(());
    }

    // Already fetched this exact icon URL before (e.g. every page on the
    // same site sharing one favicon) - apply straight from cache, no
    // network round-trip needed.
    let cached = {
        let state = app.state::<SharedFaviconCache>();
        let cache = state.lock_recover();
        cache.0.get(&icon_url).cloned()
    };
    if let Some(data_url) = cached {
        apply_favicon_to_tab(&app, &label, &data_url);
        return Ok(());
    }

    // Fetch on its own thread, same reasoning as save_image_as: this runs
    // on report_content's invoke_handler thread, and a blocking network
    // call there would stall other commands until it finishes.
    let app_for_fetch = app.clone();
    std::thread::spawn(move || {
        let (bytes, content_type) = match fetch_image_bytes(&icon_url) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[kite] favicon fetch failed for {icon_url}: {e}");
                return;
            }
        };
        if bytes.is_empty() || bytes.len() > MAX_FAVICON_BYTES {
            eprintln!(
                "[kite] favicon fetch skipped for {icon_url}: unexpected size ({} bytes)",
                bytes.len()
            );
            return;
        }
        let data_url = build_favicon_data_url(&bytes, content_type.as_deref(), &icon_url);
        {
            let state = app_for_fetch.state::<SharedFaviconCache>();
            let mut cache = state.lock_recover();
            cache.0.insert(icon_url.clone(), data_url.clone());
        }
        apply_favicon_to_tab(&app_for_fetch, &label, &data_url);
    });

    Ok(())
}

// Shared by report_login_submit and report_login_form_present - both
// receive a `host` from content-tab JS that has to be treated as
// untrusted (capabilities/content.json grants their commands to the
// page's own origin too, not just password_capture.js - a malicious
// page could call either directly with a fabricated host). Cross-checks
// against the tab's own current URL (set by a real on_navigation event,
// never from anything the page handed us) and skips private/crashed
// tabs, mirroring history/downloads/favicons - see TabInfo::is_private's
// own comment. Returns the tab's real host only when everything lines
// up; None means "silently ignore this report".
fn eligible_tab_host(app: &tauri::AppHandle, label: &str) -> Option<String> {
    let state = app.state::<SharedTabState>();
    let st = state.lock_recover();
    let tab = st.tabs.iter().find(|t| t.label == label)?;
    if tab.is_private || tab.crashed {
        return None;
    }
    Url::parse(&tab.url).ok().and_then(|u| u.host_str().map(|h| h.to_lowercase()))
}

// Called by password_capture.js.new(){...} - see that script's DOMContentLoaded
// listener - whenever a page loads with what looks like a login form.
// Rust decides whether there's anything worth offering: only if the
// vault's unlocked (autofill never prompts to unlock the way saving
// does - filling in a password is a more sensitive action than saving
// one, so this stays a no-op rather than a prompt on a locked vault) and
// only if a saved entry actually exists for this host.
#[tauri::command]
fn report_login_form_present(webview: tauri::Webview, app: tauri::AppHandle, host: String) -> Result<(), String> {
    require_content(&webview)?;

    let host = host.trim().to_lowercase();
    if host.is_empty() {
        return Ok(());
    }
    let label = webview.label().to_string();
    let Some(tab_host) = eligible_tab_host(&app, &label) else {
        return Ok(());
    };
    if tab_host != host {
        return Ok(());
    }

    let usernames = {
        let vault_state = app.state::<SharedVaultState>();
        let vst = vault_state.lock_recover();
        let Some(key) = vst.key else {
            return Ok(());
        };
        let Some(file) = load_vault_file(&vst.file_path) else {
            return Ok(());
        };
        let mut usernames: Vec<String> = file
            .entries
            .iter()
            .filter_map(|e| {
                vault_decrypt(&key, &e.nonce, &e.ciphertext)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<VaultEntryPlaintext>(&bytes).ok())
                    .filter(|p| p.host.eq_ignore_ascii_case(&host))
                    .map(|p| p.username)
            })
            .collect();
        usernames.sort();
        usernames.dedup();
        usernames
    };
    if usernames.is_empty() {
        return Ok(());
    }

    let _ = app.emit_to(
        MAIN_WEBVIEW_LABEL,
        "autofill-available",
        AutofillAvailablePayload {
            tab_label: label,
            host,
            usernames,
        },
    );
    Ok(())
}

// Backs the autofill prompt's "Fill" button - decrypts the chosen saved
// entry and writes both fields into the page via a one-shot eval'd
// script, targeting tab_label directly (not just "whichever tab is
// active" the way ctx-back/ctx-forward do) since the person could in
// principle switch tabs between the prompt appearing and clicking Fill.
// Doesn't submit the form - filling is already an explicit action (the
// Fill click); submitting on top of that would be a second one the
// person didn't ask for.
#[tauri::command]
fn vault_autofill(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    tab_label: String,
    host: String,
    username: String,
) -> Result<(), String> {
    require_chrome(&webview)?;

    let vault_state = app.state::<SharedVaultState>();
    let vst = vault_state.lock_recover();
    let key = vst.key.ok_or_else(|| "Vault is locked.".to_string())?;
    let file = load_vault_file(&vst.file_path).ok_or_else(|| "Vault file missing.".to_string())?;
    let idx = find_vault_entry_index(&file, &key, &host, &username).ok_or_else(|| "Login not found.".to_string())?;
    let bytes = vault_decrypt(&key, &file.entries[idx].nonce, &file.entries[idx].ciphertext)?;
    let plaintext: VaultEntryPlaintext = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    drop(vst);

    let target = app.get_webview(&tab_label).ok_or_else(|| "Tab not found.".to_string())?;
    let script = build_autofill_script(&plaintext.username, &plaintext.password);
    target.eval(&script).map_err(|e| e.to_string())
}

// Sets a value the same way a real keystroke would, rather than just
// `el.value = ...` - plain assignment doesn't fire the events a
// framework's controlled-input state relies on, so a React/Vue-backed
// login form would visually show the filled value but the framework's
// own state (and therefore what actually gets submitted) would still be
// empty. Going through the native property setter and then dispatching
// a real 'input' event is the standard workaround: it makes the
// assignment indistinguishable from typing as far as the framework's
// change-detection is concerned.
fn build_autofill_script(username: &str, password: &str) -> String {
    let username_json = serde_json::to_string(username).unwrap_or_else(|_| "\"\"".to_string());
    let password_json = serde_json::to_string(password).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"(function() {{
  function setNativeValue(el, value) {{
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
    setter.call(el, value);
    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }}

  const passwordField = document.querySelector('input[type="password"]');
  if (!passwordField) return;

  const form = passwordField.closest('form') || document;
  const candidates = Array.from(
    form.querySelectorAll(
      'input:not([type=hidden]):not([type=checkbox]):not([type=radio]):not([type=submit]):not([type=button])'
    )
  ).filter((el) => el !== passwordField && el.type !== 'password');
  let usernameField = candidates.find((el) => el.autocomplete === 'username');
  if (!usernameField) {{
    const before = candidates.filter(
      (el) => el.compareDocumentPosition(passwordField) & Node.DOCUMENT_POSITION_FOLLOWING
    );
    usernameField = before.length ? before[before.length - 1] : candidates[0];
  }}

  const username = {username_json};
  const password = {password_json};
  if (usernameField && username) setNativeValue(usernameField, username);
  setNativeValue(passwordField, password);
}})();"#
    )
}


// Called by password_capture.js on every detected login-form submit -
// see that script's own comment for what it does and doesn't catch.
// Nothing is saved to the vault here; this just records the candidate
// credentials in memory (SharedPendingLogins) for a later phase's
// save-password prompt to read and decide on. The password never gets
// logged, returned to any webview, or written to disk by this command.
#[tauri::command]
fn report_login_submit(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    host: String,
    username: String,
    password: String,
) -> Result<(), String> {
    require_content(&webview)?;

    let host = host.trim().to_lowercase();
    if host.is_empty() || password.is_empty() {
        return Ok(());
    }
    let label = webview.label().to_string();

    let Some(tab_host) = eligible_tab_host(&app, &label) else {
        return Ok(());
    };
    if tab_host != host {
        return Ok(());
    }

    // If the vault's unlocked and these exact credentials are already
    // saved for this host, there's nothing for a save prompt to offer -
    // this is what stops the same login from re-prompting on every
    // single visit once it's been saved once. If the vault's locked,
    // this can't be checked (nothing to decrypt against), so it falls
    // through to prompting as normal - better an occasional redundant
    // prompt than silently missing a real password change.
    if login_already_saved(&app, &host, &username, &password) {
        return Ok(());
    }

    eprintln!(
        "[kite] login capture on tab {label} ({host}): username={:?}, password=<{} chars>",
        username,
        password.len()
    );

    let state = app.state::<SharedPendingLogins>();
    let mut pending = state.lock_recover();
    pending.0.insert(
        label.clone(),
        PendingLoginCapture {
            host: host.clone(),
            username: username.clone(),
            password,
        },
    );
    drop(pending);

    let _ = app.emit_to(
        MAIN_WEBVIEW_LABEL,
        "login-capture-available",
        LoginCapturePayload {
            tab_label: label,
            host,
            username,
        },
    );

    Ok(())
}

// Shared by report_login_submit's re-prompt suppression above. Returns
// false (rather than erroring) whenever it can't actually check - locked
// vault, no vault yet, or a decrypt failure on some entry - since "can't
// tell" should fall back to prompting, not to silently skipping a save.
fn login_already_saved(app: &tauri::AppHandle, host: &str, username: &str, password: &str) -> bool {
    let vault_state = app.state::<SharedVaultState>();
    let vst = vault_state.lock_recover();
    let Some(key) = vst.key else {
        return false;
    };
    let Some(file) = load_vault_file(&vst.file_path) else {
        return false;
    };
    file.entries.iter().any(|e| {
        vault_decrypt(&key, &e.nonce, &e.ciphertext)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<VaultEntryPlaintext>(&bytes).ok())
            .is_some_and(|existing| {
                existing.host.eq_ignore_ascii_case(host)
                    && existing.username == username
                    && existing.password == password
            })
    })
}


// rather than a fixed +/-10% multiply, so repeated zooming lands on the
// same "nice" percentages a person would expect (100%, 125%, 150%...).
const ZOOM_LEVELS: [f64; 17] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0,
];

fn closest_zoom_index(zoom: f64) -> usize {
    ZOOM_LEVELS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - zoom).abs().partial_cmp(&(*b - zoom).abs()).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(7) // index of 1.0, if the list is ever somehow empty
}

fn active_tab_zoom(app: &tauri::AppHandle) -> Result<f64, String> {
    let state = app.state::<SharedTabState>();
    let st = state.lock_recover();
    Ok(st
        .tabs
        .iter()
        .find(|t| t.label == st.active)
        .map(|t| t.zoom)
        .unwrap_or(1.0))
}

fn set_active_zoom(app: &tauri::AppHandle, zoom: f64) -> Result<(), String> {
    {
        let state = app.state::<SharedTabState>();
        let mut st = state.lock_recover();
        let active = st.active.clone();
        if let Some(tab) = st.tabs.iter_mut().find(|t| t.label == active) {
            tab.zoom = zoom;
        }
    }
    active_webview(app)?
        .set_zoom(zoom)
        .map_err(|e| e.to_string())?;
    emit_zoom_changed(app);
    Ok(())
}

#[tauri::command]
fn zoom_in(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    let idx = closest_zoom_index(active_tab_zoom(&app)?);
    let next = ZOOM_LEVELS[(idx + 1).min(ZOOM_LEVELS.len() - 1)];
    set_active_zoom(&app, next)
}

#[tauri::command]
fn zoom_out(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    let idx = closest_zoom_index(active_tab_zoom(&app)?);
    let next = ZOOM_LEVELS[idx.saturating_sub(1)];
    set_active_zoom(&app, next)
}

#[tauri::command]
fn zoom_reset(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    require_chrome(&webview)?;
    set_active_zoom(&app, 1.0)
}

fn main() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }

                    // These are OS-level hotkeys, registered so they still
                    // reach us even when a page's own content webview (not
                    // our chrome) has keyboard focus. Without this focus
                    // check they'd also fire while some other application
                    // is in the foreground, which isn't what a browser
                    // shortcut should do.
                    let focused = app_is_foreground();
                    if !focused {
                        return;
                    }

                    let action = if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::KeyT) {
                        Some("new_tab")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::KeyW) {
                        Some("close_tab")
                    } else if *shortcut
                        == Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyT)
                    {
                        Some("reopen_closed_tab")
                    } else if *shortcut
                        == Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyN)
                    {
                        Some("new_private_tab")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::KeyL) {
                        Some("focus_address")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::KeyD) {
                        Some("toggle_bookmark")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::KeyH) {
                        Some("open_history")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::KeyF) {
                        Some("find_in_page")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Equal) {
                        Some("zoom_in")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Minus) {
                        Some("zoom_out")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Digit0) {
                        Some("zoom_reset")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Digit1) {
                        Some("switch_tab_1")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Digit2) {
                        Some("switch_tab_2")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Digit3) {
                        Some("switch_tab_3")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Digit4) {
                        Some("switch_tab_4")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Digit5) {
                        Some("switch_tab_5")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Digit6) {
                        Some("switch_tab_6")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Digit7) {
                        Some("switch_tab_7")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Digit8) {
                        Some("switch_tab_8")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Digit9) {
                        // Matches Chrome/Firefox convention: Ctrl+9 always
                        // jumps to the LAST tab, not literally "tab 9" -
                        // Ctrl+1-8 are the only ones that mean a literal
                        // position.
                        Some("switch_tab_last")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::Tab) {
                        Some("switch_tab_next")
                    } else if *shortcut
                        == Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Tab)
                    {
                        Some("switch_tab_prev")
                    // Browser-convention navigation shortcuts - Alt+Left/
                    // Right for back/forward and Alt+Home for home page
                    // match Firefox's defaults (Chrome has no default
                    // keybinding for these at all, so there's no existing
                    // convention to clash with there); Ctrl+R for reload
                    // matches both.
                    } else if *shortcut == Shortcut::new(Some(Modifiers::ALT), Code::ArrowLeft) {
                        Some("go_back")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::ALT), Code::ArrowRight) {
                        Some("go_forward")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::CONTROL), Code::KeyR) {
                        Some("reload")
                    } else if *shortcut == Shortcut::new(Some(Modifiers::ALT), Code::Home) {
                        Some("go_home")
                    } else {
                        None
                    };

                    if let Some(action) = action {
                        let _ = app.emit_to(MAIN_WEBVIEW_LABEL, "shortcut", action);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(TabState {
            tabs: Vec::new(),
            active: String::new(),
            next_id: 0,
            window_size: (1200.0, 800.0),
            library_tab: None,
            library_view: "history".to_string(),
            closed_tabs: Vec::new(),
        }))
        .manage(Mutex::new(None::<ContextMenuTarget>) as SharedContextMenu)
        .manage(Mutex::new(FaviconCache(std::collections::HashMap::new())) as SharedFaviconCache)
        .manage(Mutex::new(PendingLogins(std::collections::HashMap::new())) as SharedPendingLogins)
        .invoke_handler(tauri::generate_handler![
            navigate,
            go_back,
            go_forward,
            reload,
            new_tab,
            switch_tab,
            close_tab,
            reopen_closed_tab,
            activate_tab_at,
            cycle_active_tab,
            get_history,
            clear_history,
            remove_history_entry,
            get_bookmarks,
            get_downloads,
            open_download,
            show_download_in_folder,
            clear_downloads,
            remove_download_entry,
            add_bookmark,
            remove_bookmark,
            get_settings,
            get_blocklist_status,
            set_search_engine,
            set_homepage,
            set_content_blocking,
            toggle_site_allowlist,
            refresh_blocklist,
            choose_downloads_dir,
            get_tabs,
            go_home,
            show_library,
            hide_library,
            toggle_find_in_page,
            report_context_menu,
            report_content_click,
            report_favicon,
            report_login_submit,
            report_login_form_present,
            zoom_in,
            zoom_out,
            zoom_reset,
            vault_status,
            vault_create,
            vault_unlock,
            vault_lock,
            vault_save_login,
            vault_unlock_and_save_login,
            vault_autofill,
            vault_dismiss_login,
            vault_list_logins,
            vault_reveal_login,
            vault_copy_login_password,
            vault_delete_login
        ])
        .setup(|app| {
            install_panic_hook(&app.handle());

            let width = 1200.0;
            let height = 800.0;

            // Load (or initialize) persisted history/bookmarks before anything
            // else touches SharedAppData.
            let data_path = app_data_path(&app.handle())?;
            let persisted = load_persisted_data(&data_path);
            app.manage(Mutex::new(AppData {
                data: persisted,
                file_path: data_path,
            }) as SharedAppData);

            // Password vault - separate file, separate lock (see the
            // "password vault" module comment above SharedAppData for
            // why). Starts locked (key: None) every launch, regardless
            // of whether kite_passwords.json exists yet - vault_status
            // tells the UI which of "no vault" / "locked" / "unlocked" to
            // show, and vault_create/vault_unlock are what ever populate
            // `key`.
            let passwords_path = vault_file_path(&app.handle())?;
            app.manage(Mutex::new(VaultRuntime {
                file_path: passwords_path,
                key: None,
            }) as SharedVaultState);

            // Before any tab exists to possibly trigger a blocked-host
            // check, seed BLOCKLIST from a previously-refreshed on-disk
            // copy if one exists - see load_initial_blocklist.
            load_initial_blocklist(&app.handle());

            let window = WindowBuilder::new(app, "main")
                .title("Kite")
                .inner_size(width, height)
                .min_inner_size(480.0, 360.0)
                .build()?;

            // NOTE: CONTROL here targets Windows/Linux. If Kite ever ships
            // on macOS, these should switch to Modifiers::SUPER (Cmd) for
            // that platform - CmdOrCtrl-style cross-platform shortcut
            // helpers aren't part of this plugin's API, so it'd need a
            // #[cfg(target_os = "macos")] split.
            for shortcut in [
                Shortcut::new(Some(Modifiers::CONTROL), Code::KeyT),
                Shortcut::new(Some(Modifiers::CONTROL), Code::KeyW),
                Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyT),
                Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyN),
                Shortcut::new(Some(Modifiers::CONTROL), Code::KeyL),
                Shortcut::new(Some(Modifiers::CONTROL), Code::KeyD),
                Shortcut::new(Some(Modifiers::CONTROL), Code::KeyH),
                Shortcut::new(Some(Modifiers::CONTROL), Code::KeyF),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Equal),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Minus),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Digit0),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Digit1),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Digit2),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Digit3),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Digit4),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Digit5),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Digit6),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Digit7),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Digit8),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Digit9),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Tab),
                Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Tab),
                Shortcut::new(Some(Modifiers::ALT), Code::ArrowLeft),
                Shortcut::new(Some(Modifiers::ALT), Code::ArrowRight),
                Shortcut::new(Some(Modifiers::CONTROL), Code::KeyR),
                Shortcut::new(Some(Modifiers::ALT), Code::Home),
            ] {
                match app.global_shortcut().register(shortcut) {
                    Ok(()) => {}
                    Err(e) => eprintln!("[kite] failed to register shortcut {:?}: {e}", shortcut),
                }
            }

            window.add_child(
                WebviewBuilder::new(MAIN_WEBVIEW_LABEL, WebviewUrl::App("index.html".into())),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(width, CHROME_HEIGHT),
            )?;

            // Dispatches whichever item the user picked from the context
            // menu built in report_context_menu. NOTE: event.id's exact
            // form (field vs. method, i.e. `event.id.as_ref()` vs.
            // `event.id().as_ref()`) is the one thing here I couldn't
            // fully pin down against this Tauri version - if this line
            // is the compile error, try the other form.
            let app_handle_menu = app.handle().clone();
            window.on_menu_event(move |_window, event| {
                handle_context_menu_event(&app_handle_menu, event.id.as_ref());
            });

            let app_handle = app.handle().clone();
            let (initial_urls, initial_active_index) = initial_session_tabs(&app_handle);
            let mut restored_labels = Vec::with_capacity(initial_urls.len());
            for url in &initial_urls {
                match create_tab_webview(&app_handle, url, false) {
                    Ok(label) => restored_labels.push(label),
                    Err(e) => eprintln!("[kite] failed to restore tab for {url}: {e}"),
                }
            }
            // If every restored tab failed (e.g. a saved URL no longer
            // parses), fall back to a single Home tab rather than leaving
            // the browser with zero tabs and nothing to activate below.
            if restored_labels.is_empty() {
                restored_labels.push(create_tab_webview(&app_handle, HOME_URL, false)?);
            }
            let active_label = restored_labels
                .get(initial_active_index)
                .cloned()
                .unwrap_or_else(|| restored_labels[0].clone());
            activate_tab(&app_handle, &active_label)?;
            emit_tabs_changed(&app_handle);
            emit_active_url(&app_handle);

            // Window and tabs are already up at this point, so this can't
            // delay first paint - it only ever spawns a background thread
            // (if the list is actually stale) and returns immediately.
            maybe_auto_refresh_blocklist_on_startup(&app_handle);

            let window_clone = window.clone();
            let app_handle_resize = app.handle().clone();
            // Shared by both WindowEvent::Resized and WindowEvent::Focused
            // below - see the Focused arm's own comment for why a second
            // trigger is needed at all, on top of Resized.
            let sync_layout_to_window = {
                let app_handle_resize = app_handle_resize.clone();
                move |logical: LogicalSize<f64>| {
                    let state = app_handle_resize.state::<SharedTabState>();
                    let (active_label, lib_open) = {
                        let mut st = state.lock_recover();
                        st.window_size = (logical.width, logical.height);
                        let lib_open = st.library_tab.as_deref() == Some(st.active.as_str());
                        (st.active.clone(), lib_open)
                    };

                    if let Some(chrome) = app_handle_resize.get_webview(MAIN_WEBVIEW_LABEL) {
                        let chrome_height = if lib_open { logical.height } else { CHROME_HEIGHT };
                        let _ = chrome.set_size(LogicalSize::new(logical.width, chrome_height));
                    }

                    if !lib_open {
                        if let Some(active) = app_handle_resize.get_webview(&active_label) {
                            let _ = active.set_size(content_size(logical.width, logical.height));
                        }
                    }
                }
            };

            window.on_window_event(move |event| match event {
                tauri::WindowEvent::Resized(phys_size) => {
                    if let Ok(factor) = window_clone.scale_factor() {
                        sync_layout_to_window(phys_size.to_logical::<f64>(factor));
                    }
                }
                // Resized doesn't reliably fire on Windows for every
                // minimize -> maximize/restore transition (a known class
                // of Tauri/tao issue on Windows - e.g. tauri-apps/tauri#7664,
                // "minimize triggers onResized while maximize does not"),
                // and that gap seems to be what's behind the home-page
                // content getting stuck at a stale size after the window's
                // sat idle and is then maximized or restored (see Francis's
                // report). Focused(true), on the other hand, is a genuine
                // top-level-window activation and fires reliably for
                // exactly those transitions (unlike the child-webview-click
                // case noted elsewhere in this file, which is a different,
                // narrower gap) - re-querying and reapplying the actual
                // current window size here is a cheap, idempotent safety
                // net that self-heals regardless of whether Resized fired
                // correctly for a given transition.
                tauri::WindowEvent::Focused(true) => {
                    if let (Ok(phys_size), Ok(factor)) =
                        (window_clone.inner_size(), window_clone.scale_factor())
                    {
                        sync_layout_to_window(phys_size.to_logical::<f64>(factor));
                    }
                }
                _ => {}
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Kite");
}