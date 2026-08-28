# Kite Browser

A native desktop browser built directly on your OS's own engine — no bundled Chromium riding along for the weight. Fast to open, light to run, quiet about what it sends home.

Built with [Tauri v2](https://tauri.app) (Rust backend + HTML/CSS/JS chrome), rendered by your operating system's own WebView instead of a second browser engine shipped inside the app.

**[kite.scotech.co.ke](https://kite.scotech.co.ke)** · **[Latest release](https://github.com/FrancisOtieno-Hub/kite-browser/releases/latest)** · **[Report an issue](https://github.com/FrancisOtieno-Hub/kite-browser/issues)**

---

## Features

- **Tabs** — create, switch, close, back/forward/reload, session restore
- **History & bookmarks** — JSON-persisted, with a library panel UI
- **Homepage** (`kite://home`) — search bar, photo background, persistent bookmarks bar
- **Content blocking** — a 97,000+ domain blocklist, refreshable, with per-site allow toggle
- **Password vault** — Argon2id + AES-GCM encrypted, one master password, save & autofill, no third-party extension needed
- **Genuine private tabs** — real cookie/cache isolation via the OS's own incognito profile, not just a UI label
- **Find-in-page & zoom** — Ctrl+F, Ctrl+/-/0, tracked per tab
- **Context menu** — links, images, selection, with native image actions (Open in New Tab, Save As, Copy, Copy Address)
- **Downloads** — intercepted into your OS Downloads folder, collision-avoided naming, library panel UI
- **Settings** (`kite://settings`) — search engine, homepage/startup behavior, downloads location, clear browsing data
- **Crash recovery** — a tab whose content crashes shows a recovery page instead of going dead (Windows, via WebView2's `ProcessFailed` event)
- **Favicons** — detected, fetched, cached, and persisted so they survive restarts
- **Keyboard shortcuts** — Ctrl+T/W/Shift+T (reopen closed tab), Ctrl+L/D/H/F, Ctrl+1–9, Ctrl+Tab cycling, zoom shortcuts

## Platform support

Windows 10/11 today, via `.msi` and `.exe` installers. macOS and Linux are scoped but not yet implemented — see [Roadmap](#roadmap).

## Installing

Grab the latest `.msi` or `.exe` from the [Releases page](https://github.com/FrancisOtieno-Hub/kite-browser/releases/latest).

> **Seeing "Windows protected your PC"?** This is Microsoft's SmartScreen warning, and it shows up on *any* app that isn't code-signed — it isn't specific to Kite, and it isn't a sign of a bad or tampered build. Code signing costs money and Kite doesn't have it yet. To proceed: click **"More info"**, then click the **"Run anyway"** button that appears. This is the standard, safe way past this warning on Windows.

## Building from source

Requires [Node.js](https://nodejs.org), [Rust](https://rustup.rs), and the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
git clone https://github.com/FrancisOtieno-Hub/kite-browser.git
cd kite-browser
npm install
npm run tauri dev
```

To produce a release build (installers land in `src-tauri/target/release/bundle/`):

```bash
npm run tauri build
```

## Roadmap

- Cross-platform support (macOS/Linux) — scoping is done; implementation is blocked on access to non-Windows hardware
- Browser extensions

## Contributing

Issues and pull requests are welcome. The full source is here to read, build, and modify.

## Support

Kite doesn't sell what you browse. Development is funded by the people who use it — you can support it on [Patreon](https://www.patreon.com/cw/KiteBrowser).

## License

Kite Browser is licensed under the [GNU General Public License v3.0](LICENSE).

---

© ScoTech. Built in Kenya.
