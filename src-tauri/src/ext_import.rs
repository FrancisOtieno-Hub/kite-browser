// Import of Chrome/Chromium-family and Firefox extensions into Kite's own
// extension format (see the "Extensions" module in main.rs).
//
// WHAT THIS IS - AND ISN'T
//
// Kite's extension system is a content-script runner: a manifest with
// match patterns plus JS/CSS that gets injected into matching pages. That
// is the part of a Chrome/Firefox extension this importer carries over -
// the manifest's `content_scripts`. Everything else a real browser
// extension can have (background pages / service workers, toolbar popups,
// options pages, the chrome.* / browser.* APIs, request blocking) has no
// equivalent in Kite and is NOT imported. Each import reports what was
// dropped as warnings instead of pretending the extension will work in
// full, and extensions with no content scripts at all are reported as
// "nothing to import" rather than creating an empty shell.
//
// SAFETY
//
//   - Imported extensions are written to disk but never enabled; Kite's
//     existing opt-in (enabled_ids) still gates them. Re-importing an
//     extension that was enabled disables it again (the code changed).
//   - Every path taken from a manifest or archive is normalised and
//     rejected if absolute or containing "..", so a hostile package can't
//     write or read outside its own folder.
//   - Per-file, per-archive and total size caps bound what a package can
//     make Kite read into memory (including zip-bomb style archives).
//   - Nothing is executed during import; it's file copying plus JSON.
//   - Match patterns are converted to Kite globs by the same rules
//     browsers use (http/https only, "*.host" covers the bare domain).

use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SCRIPT_GROUPS: usize = 16;

// ---------------------------------------------------------------------
// Source abstraction: an unpacked folder, or an in-memory zip (.xpi/.zip,
// and .crx once its header is stripped).
// ---------------------------------------------------------------------

pub enum Source {
    Dir(PathBuf),
    Zip(Vec<u8>),
}

// Normalises a manifest-supplied relative path. Returns None for anything
// that isn't a plain, forward-only relative path.
pub fn normalize_rel_path(raw: &str) -> Option<String> {
    let s = raw.trim().replace('\\', "/");
    let s = s.trim_start_matches("./").trim_start_matches('/');
    if s.is_empty() || s.contains('\0') || s.contains(':') {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for comp in Path::new(s).components() {
        match comp {
            Component::Normal(c) => parts.push(c.to_str()?),
            Component::CurDir => {}
            // "..", roots and drive prefixes are all escape attempts.
            _ => return None,
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

impl Source {
    pub fn open(path: &Path) -> Result<Source, String> {
        if path.is_dir() {
            return Ok(Source::Dir(path.to_path_buf()));
        }
        let meta = fs::metadata(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
        if meta.len() > MAX_ARCHIVE_BYTES {
            return Err("file is too large to be a browser extension".to_string());
        }
        let bytes = fs::read(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
        Source::from_bytes(bytes)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Source, String> {
        let zip_bytes = if bytes.starts_with(b"Cr24") {
            strip_crx_header(&bytes)?.to_vec()
        } else {
            bytes
        };
        // Validate it really is a zip before accepting it.
        zip::ZipArchive::new(Cursor::new(&zip_bytes))
            .map_err(|_| "not a valid extension package (expected a folder, .crx, .xpi or .zip)".to_string())?;
        Ok(Source::Zip(zip_bytes))
    }

    // `rel` must already be normalised (see normalize_rel_path).
    pub fn read(&self, rel: &str) -> Option<Vec<u8>> {
        match self {
            Source::Dir(root) => {
                let full = root.join(rel);
                // Resolve symlinks and make sure we're still inside root.
                let canon = fs::canonicalize(&full).ok()?;
                let canon_root = fs::canonicalize(root).ok()?;
                if !canon.starts_with(&canon_root) {
                    return None;
                }
                let meta = fs::metadata(&canon).ok()?;
                if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
                    return None;
                }
                fs::read(canon).ok()
            }
            Source::Zip(bytes) => {
                let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
                let entry = archive.by_name(rel).ok()?;
                if entry.size() > MAX_FILE_BYTES {
                    return None;
                }
                let mut out = Vec::new();
                // `take` as well as the size check above: the declared
                // size in a hostile archive can lie.
                entry.take(MAX_FILE_BYTES + 1).read_to_end(&mut out).ok()?;
                if out.len() as u64 > MAX_FILE_BYTES {
                    return None;
                }
                Some(out)
            }
        }
    }
}

// CRX2: "Cr24" | ver=2 | pubkey_len | sig_len | pubkey | sig | zip
// CRX3: "Cr24" | ver=3 | header_len | header (protobuf) | zip
fn strip_crx_header(b: &[u8]) -> Result<&[u8], String> {
    let u32_at = |i: usize| -> Option<usize> {
        b.get(i..i + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
    };
    let version = u32_at(4).ok_or("truncated .crx file")?;
    let start = match version {
        3 => 12usize
            .checked_add(u32_at(8).ok_or("truncated .crx file")?)
            .ok_or("corrupt .crx header")?,
        2 => {
            let pk = u32_at(8).ok_or("truncated .crx file")?;
            let sig = u32_at(12).ok_or("truncated .crx file")?;
            16usize
                .checked_add(pk)
                .and_then(|v| v.checked_add(sig))
                .ok_or("corrupt .crx header")?
        }
        v => return Err(format!("unsupported .crx version {v}")),
    };
    b.get(start..).ok_or_else(|| "corrupt .crx header".to_string())
}

// ---------------------------------------------------------------------
// Manifest conversion
// ---------------------------------------------------------------------

// One Kite extension produced from one `content_scripts` entry.
pub struct ConvertedScript {
    pub id: String,
    pub manifest: Value, // Kite-format manifest.json
    pub files: Vec<(String, Vec<u8>)>, // (normalised relative path, bytes)
}

pub struct Converted {
    pub name: String,
    pub scripts: Vec<ConvertedScript>,
    pub warnings: Vec<String>, // extension-level (not tied to one script)
}

pub fn sanitize_id(raw: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in raw.trim().chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches(|c| c == '-' || c == '.').to_string();
    out.chars().take(48).collect()
}

// Resolves "__MSG_key__" names via _locales/<default_locale>/messages.json.
fn localized(src: &Source, manifest: &Value, raw: &str) -> String {
    let Some(key) = raw.strip_prefix("__MSG_").and_then(|r| r.strip_suffix("__")) else {
        return raw.to_string();
    };
    let mut locales: Vec<String> = Vec::new();
    if let Some(d) = manifest.get("default_locale").and_then(|v| v.as_str()) {
        locales.push(d.to_string());
    }
    locales.push("en".to_string());
    locales.push("en_US".to_string());
    for loc in locales {
        let Some(rel) = normalize_rel_path(&format!("_locales/{loc}/messages.json")) else {
            continue;
        };
        let Some(bytes) = src.read(&rel) else { continue };
        // messages.json is sometimes written with a UTF-8 BOM.
        let text = String::from_utf8_lossy(&bytes);
        let text = text.trim_start_matches('\u{feff}');
        let Ok(v) = serde_json::from_str::<Value>(text) else { continue };
        if let Some(obj) = v.as_object() {
            for (k, entry) in obj {
                if k.eq_ignore_ascii_case(key) {
                    if let Some(msg) = entry.get("message").and_then(|m| m.as_str()) {
                        return msg.to_string();
                    }
                }
            }
        }
    }
    key.to_string()
}

// Browser match pattern -> Kite glob(s). http/https only; "*.host" also
// covers the bare domain. Err(reason) for patterns that can't be honoured.
pub fn match_pattern_to_globs(pattern: &str) -> Result<Vec<String>, String> {
    let p = pattern.trim();
    if p == "<all_urls>" {
        return Ok(vec!["http://*/*".into(), "https://*/*".into()]);
    }
    let (scheme, rest) = p
        .split_once("://")
        .ok_or_else(|| format!("unsupported match pattern \"{p}\""))?;
    let schemes: Vec<&str> = match scheme {
        "*" => vec!["http", "https"],
        "http" => vec!["http"],
        "https" => vec!["https"],
        other => return Err(format!("\"{other}://\" patterns aren't supported (\"{p}\")")),
    };
    let (host, path) = rest
        .split_once('/')
        .map(|(h, pa)| (h, format!("/{pa}")))
        .ok_or_else(|| format!("match pattern \"{p}\" has no path"))?;
    if host.is_empty() {
        return Err(format!("match pattern \"{p}\" has no host"));
    }
    let mut hosts = vec![host.to_string()];
    if let Some(bare) = host.strip_prefix("*.") {
        hosts.push(bare.to_string()); // "*.example.com" also matches example.com
    }
    let mut out = Vec::new();
    for s in &schemes {
        for h in &hosts {
            out.push(format!("{s}://{h}{path}"));
        }
    }
    Ok(out)
}

fn convert_patterns(list: Option<&Value>, warnings: &mut Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some(arr) = list.and_then(|v| v.as_array()) else {
        return out;
    };
    for item in arr {
        let Some(s) = item.as_str() else { continue };
        match match_pattern_to_globs(s) {
            Ok(globs) => {
                for g in globs {
                    if !out.contains(&g) {
                        out.push(g);
                    }
                }
            }
            Err(reason) => {
                let w = format!("Skipped a page pattern: {reason}");
                if !warnings.contains(&w) {
                    warnings.push(w);
                }
            }
        }
    }
    out
}

fn string_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

// Which chrome.* / browser.* namespaces a script visibly calls. Purely a
// heads-up for the warning text - this is a substring scan, not a parser.
fn find_api_usage(js: &str) -> Vec<&'static str> {
    const APIS: &[&str] = &[
        "storage", "runtime", "tabs", "i18n", "extension", "webRequest", "windows", "cookies",
        "alarms", "notifications", "scripting", "declarativeNetRequest", "action",
        "browserAction", "contextMenus", "menus", "commands", "permissions", "webNavigation",
        "downloads", "history", "bookmarks", "identity", "management",
    ];
    let mut hits = Vec::new();
    for api in APIS {
        for ns in ["chrome", "browser"] {
            let needle = format!("{ns}.{api}");
            // Require a non-identifier char after, so "chrome.tabsFoo" or
            // "chrome.runtimeX" doesn't count.
            let mut from = 0;
            while let Some(i) = js[from..].find(&needle) {
                let end = from + i + needle.len();
                let next = js[end..].chars().next();
                let boundary = !matches!(next, Some(c) if c.is_ascii_alphanumeric() || c == '_');
                // And a non-identifier char before (so "notchrome.tabs" doesn't count).
                let prev = js[..from + i].chars().next_back();
                let start_ok = !matches!(prev, Some(c) if c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.');
                if boundary && start_ok {
                    hits.push(*api);
                    break;
                }
                from = end;
            }
            if hits.last() == Some(api) {
                break;
            }
        }
    }
    hits
}

pub fn convert(
    src: &Source,
    id_tag: &str,
    id_hint: Option<&str>,
    source_label: &str,
) -> Result<Converted, String> {
    let manifest_bytes = src
        .read("manifest.json")
        .ok_or("no manifest.json found - this doesn't look like an extension")?;
    let text = String::from_utf8_lossy(&manifest_bytes);
    let text = text.trim_start_matches('\u{feff}');
    let manifest: Value =
        serde_json::from_str(text).map_err(|e| format!("manifest.json isn't valid JSON: {e}"))?;

    let name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .map(|n| localized(src, &manifest, n))
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "Imported extension".to_string());
    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();

    // ---- extension-level warnings: what Kite can't run ----
    let mut ext_warnings: Vec<String> = Vec::new();
    let bg = manifest.get("background");
    if bg.is_some() {
        ext_warnings.push(
            "Has a background script/service worker, which Kite doesn't run - anything that depends on it won't work.".into(),
        );
    }
    let has_popup = ["action", "browser_action", "page_action"].iter().any(|k| {
        manifest
            .get(*k)
            .map(|v| v.get("default_popup").is_some())
            .unwrap_or(false)
    });
    if has_popup {
        ext_warnings.push("Its toolbar popup isn't available in Kite.".into());
    }
    if manifest.get("options_ui").is_some() || manifest.get("options_page").is_some() {
        ext_warnings.push("Its settings page isn't available in Kite.".into());
    }
    let perms: Vec<String> = string_list(manifest.get("permissions"));
    if perms.iter().any(|p| {
        matches!(
            p.as_str(),
            "webRequest" | "webRequestBlocking" | "declarativeNetRequest" | "declarativeNetRequestWithHostAccess"
        )
    }) {
        ext_warnings.push(
            "It blocks or rewrites network requests - Kite doesn't support that, only page scripts/styles are imported.".into(),
        );
    }
    if manifest.get("web_accessible_resources").is_some() {
        ext_warnings.push(
            "Bundled images/fonts it exposes to pages aren't imported; anything relying on them may look broken.".into(),
        );
    }

    let groups = manifest
        .get("content_scripts")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if groups.is_empty() {
        return Err(
            "no content scripts - this extension works through background scripts or browser features Kite can't run, so there's nothing to import".to_string(),
        );
    }

    // ---- stable base id ----
    let gecko_id = manifest
        .pointer("/browser_specific_settings/gecko/id")
        .or_else(|| manifest.pointer("/applications/gecko/id"))
        .and_then(|v| v.as_str());
    let base_hint = id_hint
        .map(str::to_string)
        .or_else(|| gecko_id.map(str::to_string))
        .unwrap_or_else(|| name.clone());
    let mut base = sanitize_id(&base_hint);
    if base.is_empty() {
        base = "extension".to_string();
    }
    let base_id = format!("imp-{}-{}", sanitize_id(id_tag), base);

    let mut scripts: Vec<ConvertedScript> = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut skipped_groups = 0usize;

    for group in groups.iter().take(MAX_SCRIPT_GROUPS) {
        let mut gw: Vec<String> = Vec::new();

        let matches = convert_patterns(group.get("matches"), &mut gw);
        if matches.is_empty() {
            skipped_groups += 1;
            continue;
        }
        let exclude = convert_patterns(group.get("exclude_matches"), &mut gw);

        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        let mut read_list = |key: &str, gw: &mut Vec<String>| -> Vec<String> {
            let mut kept = Vec::new();
            for raw in string_list(group.get(key)) {
                let Some(rel) = normalize_rel_path(&raw) else {
                    gw.push(format!("Skipped an unsafe file path: {raw}"));
                    continue;
                };
                if files.iter().any(|(p, _)| *p == rel) {
                    kept.push(rel);
                    continue;
                }
                match src.read(&rel) {
                    Some(bytes) => {
                        total_bytes += bytes.len() as u64;
                        files.push((rel.clone(), bytes));
                        kept.push(rel);
                    }
                    None => gw.push(format!("Couldn't read {rel} (missing or too large).")),
                }
            }
            kept
        };
        let js = read_list("js", &mut gw);
        let css = read_list("css", &mut gw);
        if total_bytes > MAX_TOTAL_BYTES {
            return Err("extension is too large to import".to_string());
        }
        if js.is_empty() && css.is_empty() {
            skipped_groups += 1;
            continue;
        }

        // API usage heads-up
        let mut apis: Vec<&'static str> = Vec::new();
        for (p, bytes) in &files {
            if p.ends_with(".js") {
                for a in find_api_usage(&String::from_utf8_lossy(bytes)) {
                    if !apis.contains(&a) {
                        apis.push(a);
                    }
                }
            }
        }
        if !apis.is_empty() {
            gw.push(format!(
                "Calls browser-extension APIs Kite doesn't provide ({}) - those parts will fail.",
                apis.iter().map(|a| format!("chrome.{a}")).collect::<Vec<_>>().join(", ")
            ));
        }
        if group.get("world").and_then(|v| v.as_str()) == Some("MAIN") {
            // informational only - Kite always runs in the page's world
        } else if !js.is_empty() {
            gw.push("Runs in the page's own context rather than an isolated one, so it can see/clash with the page's variables.".into());
        }
        if group.get("include_globs").is_some() || group.get("exclude_globs").is_some() {
            gw.push("Extra include/exclude globs were ignored; the script may run on more pages than intended.".into());
        }

        // "document_end"/"document_idle"/absent all map to Kite's idle run.
        let run_at = match group.get("run_at").and_then(|v| v.as_str()) {
            Some("document_start") => "document_start",
            _ => "document_idle",
        };

        let n = scripts.len() + 1;
        let id = if n == 1 { base_id.clone() } else { format!("{base_id}-{n}") };
        let display_name = if n == 1 { name.clone() } else { format!("{name} (script {n})") };

        let mut all_warnings = ext_warnings.clone();
        all_warnings.extend(gw);

        scripts.push(ConvertedScript {
            id: id.clone(),
            manifest: json!({
                "id": id,
                "name": display_name,
                "version": version,
                "matches": matches,
                "exclude_matches": exclude,
                "run_at": run_at,
                "js": js,
                "css": css,
                "permissions": [],
                "imported_from": source_label,
                "import_warnings": all_warnings,
            }),
            files,
        });
    }

    if scripts.is_empty() {
        return Err(
            "none of its content scripts could be imported (no supported page patterns or readable files)".to_string(),
        );
    }

    let mut warnings = Vec::new();
    if skipped_groups > 0 {
        warnings.push(format!(
            "{skipped_groups} content-script group(s) were skipped (unsupported patterns or unreadable files)."
        ));
    }
    if groups.len() > MAX_SCRIPT_GROUPS {
        warnings.push(format!("Only the first {MAX_SCRIPT_GROUPS} content-script groups were imported."));
    }
    if scripts.len() > 1 {
        warnings.push(format!(
            "It has {} separate script groups, imported as {} entries - enable the ones you want.",
            scripts.len(),
            scripts.len()
        ));
    }

    Ok(Converted { name, scripts, warnings })
}

// ---------------------------------------------------------------------
// Writing into Kite's extensions folder
// ---------------------------------------------------------------------

// Writes one converted script as <ext_root>/<id>/ (manifest last), via a
// temp folder + rename so a half-written import can never be picked up by
// load_extensions. Replaces any previous import with the same id.
pub fn write_script(ext_root: &Path, script: &ConvertedScript) -> Result<(), String> {
    let id = &script.id;
    // id came from sanitize_id, but this is the last gate before the disk.
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err("invalid extension id".to_string());
    }
    fs::create_dir_all(ext_root).map_err(|e| format!("can't create extensions folder: {e}"))?;
    let tmp = ext_root.join(format!(".import-{id}.tmp"));
    let dest = ext_root.join(id);
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    let result = (|| -> Result<(), String> {
        for (rel, bytes) in &script.files {
            let rel = normalize_rel_path(rel).ok_or("unsafe file path")?;
            let target = tmp.join(&rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&target, bytes).map_err(|e| format!("can't write {rel}: {e}"))?;
        }
        let manifest = serde_json::to_string_pretty(&script.manifest).map_err(|e| e.to_string())?;
        fs::write(tmp.join("manifest.json"), manifest).map_err(|e| e.to_string())
    })();
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }

    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| format!("can't replace the existing import: {e}"))?;
    }
    fs::rename(&tmp, &dest).map_err(|e| {
        let _ = fs::remove_dir_all(&tmp);
        format!("can't finish import: {e}")
    })
}

#[derive(Serialize, Clone)]
pub struct ImportedItem {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Clone)]
pub struct ImportOutcome {
    pub source: String, // what the person picked (path), for the result list
    pub label: String,  // display name (extension name when known)
    pub ok: bool,
    pub imported: Vec<ImportedItem>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

fn tag_for_path(path: &Path) -> (&'static str, &'static str) {
    // (id tag, display label) for a manually picked file/folder.
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("xpi") => ("firefox", "Firefox add-on file"),
        Some("crx") => ("chrome", "Chrome extension file"),
        _ => ("file", "Extension file"),
    }
}

// Full pipeline for one source. `tag`/`label` None -> derived from the
// path (manual import); scan-based imports pass their browser.
pub fn import_from_path(
    ext_root: &Path,
    path: &Path,
    tag: Option<&str>,
    label: Option<&str>,
    id_hint: Option<&str>,
) -> ImportOutcome {
    let (auto_tag, auto_label) = tag_for_path(path);
    let tag = tag.unwrap_or(auto_tag);
    let label = label.unwrap_or(auto_label);
    let source = path.to_string_lossy().to_string();
    let fail = |msg: String| ImportOutcome {
        source: source.clone(),
        label: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| source.clone()),
        ok: false,
        imported: vec![],
        warnings: vec![],
        error: Some(msg),
    };

    let src = match Source::open(path) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let converted = match convert(&src, tag, id_hint, label) {
        Ok(c) => c,
        Err(e) => return fail(e),
    };

    let mut imported = Vec::new();
    for script in &converted.scripts {
        if let Err(e) = write_script(ext_root, script) {
            return ImportOutcome {
                source,
                label: converted.name,
                ok: false,
                imported,
                warnings: converted.warnings,
                error: Some(e),
            };
        }
        imported.push(ImportedItem {
            id: script.id.clone(),
            name: script.manifest["name"].as_str().unwrap_or("").to_string(),
        });
    }

    let mut warnings = converted.warnings;
    if let Some(first) = converted.scripts.first() {
        if let Some(w) = first.manifest["import_warnings"].as_array() {
            for item in w.iter().filter_map(|x| x.as_str()) {
                if !warnings.iter().any(|e| e == item) {
                    warnings.push(item.to_string());
                }
            }
        }
    }
    ImportOutcome {
        source,
        label: converted.name,
        ok: true,
        imported,
        warnings,
        error: None,
    }
}

// ---------------------------------------------------------------------
// Discovering extensions already installed in other browsers
// ---------------------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
pub struct Candidate {
    pub browser: String,  // "Chrome", "Edge", "Brave", "Vivaldi", "Chromium", "Firefox"
    pub tag: String,      // id prefix tag, lowercase
    pub profile: String,  // profile folder name
    pub ext_id: String,   // browser's own id (Chrome: 32 letters; Firefox: gecko id)
    pub name: String,
    pub version: String,
    pub path: String,     // folder (Chromium) or .xpi/folder (Firefox)
    pub importable: bool, // has at least one content script
    pub reason: Option<String>,
}

pub struct ScanRoots {
    pub chromium: Vec<(String, PathBuf)>, // (label, "User Data" dir)
    pub firefox_profiles: Option<PathBuf>,
}

impl ScanRoots {
    pub fn from_env() -> ScanRoots {
        let mut chromium = Vec::new();
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            for (label, rel) in [
                ("Chrome", "Google/Chrome/User Data"),
                ("Edge", "Microsoft/Edge/User Data"),
                ("Brave", "BraveSoftware/Brave-Browser/User Data"),
                ("Vivaldi", "Vivaldi/User Data"),
                ("Chromium", "Chromium/User Data"),
            ] {
                chromium.push((label.to_string(), local.join(rel)));
            }
        }
        let firefox_profiles = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|a| a.join("Mozilla").join("Firefox").join("Profiles"));
        ScanRoots { chromium, firefox_profiles }
    }
}

// "1.10.2_0" > "1.9_0": compare dotted numeric components.
fn version_key(dir_name: &str) -> Vec<u64> {
    dir_name
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u64>().unwrap_or(0))
        .collect()
}

fn describe(src: &Source, fallback_name: &str) -> (String, String, bool, Option<String>) {
    let Some(bytes) = src.read("manifest.json") else {
        return (fallback_name.to_string(), String::new(), false, Some("no manifest".into()));
    };
    let text = String::from_utf8_lossy(&bytes);
    let Ok(m) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else {
        return (fallback_name.to_string(), String::new(), false, Some("unreadable manifest".into()));
    };
    let name = m
        .get("name")
        .and_then(|v| v.as_str())
        .map(|n| localized(src, &m, n))
        .unwrap_or_else(|| fallback_name.to_string());
    let version = m.get("version").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let has_scripts = m
        .get("content_scripts")
        .and_then(|v| v.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    let reason = if has_scripts {
        None
    } else {
        Some("no content scripts (works through background/browser features Kite can't run)".to_string())
    };
    (name, version, has_scripts, reason)
}

pub fn scan_installed(roots: &ScanRoots) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();

    for (label, user_data) in &roots.chromium {
        let Ok(profiles) = fs::read_dir(user_data) else { continue };
        let mut profile_dirs: Vec<PathBuf> = profiles
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n == "Default" || n.starts_with("Profile "))
                        .unwrap_or(false)
            })
            .collect();
        profile_dirs.sort();
        for profile in profile_dirs {
            let profile_name = profile.file_name().unwrap().to_string_lossy().to_string();
            let Ok(exts) = fs::read_dir(profile.join("Extensions")) else { continue };
            let mut ext_dirs: Vec<PathBuf> = exts.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
            ext_dirs.sort();
            for ext_dir in ext_dirs {
                let ext_id = ext_dir.file_name().unwrap().to_string_lossy().to_string();
                if out.iter().any(|c| c.browser == *label && c.ext_id == ext_id) {
                    continue; // same extension in another profile
                }
                // Newest version folder that actually has a manifest.
                let Ok(vers) = fs::read_dir(&ext_dir) else { continue };
                let mut ver_dirs: Vec<PathBuf> = vers
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir() && p.join("manifest.json").is_file())
                    .collect();
                ver_dirs.sort_by_key(|p| version_key(&p.file_name().unwrap().to_string_lossy()));
                let Some(best) = ver_dirs.last() else { continue };
                let src = Source::Dir(best.clone());
                let (name, version, importable, reason) = describe(&src, &ext_id);
                out.push(Candidate {
                    browser: label.clone(),
                    tag: label.to_ascii_lowercase(),
                    profile: profile_name.clone(),
                    ext_id,
                    name,
                    version,
                    path: best.to_string_lossy().to_string(),
                    importable,
                    reason,
                });
            }
        }
    }

    if let Some(ff) = &roots.firefox_profiles {
        if let Ok(profiles) = fs::read_dir(ff) {
            let mut dirs: Vec<PathBuf> = profiles.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
            dirs.sort();
            for profile in dirs {
                let profile_name = profile.file_name().unwrap().to_string_lossy().to_string();
                let Ok(text) = fs::read_to_string(profile.join("extensions.json")) else { continue };
                let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
                let Some(addons) = v.get("addons").and_then(|a| a.as_array()) else { continue };
                for a in addons {
                    if a.get("type").and_then(|t| t.as_str()) != Some("extension") {
                        continue;
                    }
                    // Only add-ons the person installed themselves, not
                    // Firefox's bundled/system ones.
                    if a.get("location").and_then(|l| l.as_str()) != Some("app-profile") {
                        continue;
                    }
                    let Some(ext_id) = a.get("id").and_then(|i| i.as_str()) else { continue };
                    if out.iter().any(|c| c.browser == "Firefox" && c.ext_id == ext_id) {
                        continue;
                    }
                    let path = a
                        .get("path")
                        .and_then(|p| p.as_str())
                        .map(PathBuf::from)
                        .unwrap_or_else(|| profile.join("extensions").join(format!("{ext_id}.xpi")));
                    if !path.exists() {
                        continue;
                    }
                    let Ok(src) = Source::open(&path) else { continue };
                    let listed_name = a
                        .pointer("/defaultLocale/name")
                        .and_then(|n| n.as_str())
                        .unwrap_or(ext_id)
                        .to_string();
                    let (name, version, importable, reason) = describe(&src, &listed_name);
                    out.push(Candidate {
                        browser: "Firefox".into(),
                        tag: "firefox".into(),
                        profile: profile_name.clone(),
                        ext_id: ext_id.to_string(),
                        name,
                        version,
                        path: path.to_string_lossy().to_string(),
                        importable,
                        reason,
                    });
                }
            }
        }
    }

    out.sort_by(|a, b| {
        (!a.importable, a.browser.as_str(), a.name.to_lowercase())
            .cmp(&(!b.importable, b.browser.as_str(), b.name.to_lowercase()))
    });
    out
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmpdir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("kite-import-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn make_zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            for (name, body) in files {
                w.start_file(*name, opts).unwrap();
                w.write_all(body.as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        buf.into_inner()
    }

    fn write_dir_ext(dir: &Path, files: &[(&str, &str)]) {
        for (name, body) in files {
            let p = dir.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        }
    }

    // ---- paths ----
    #[test]
    fn path_normalisation_accepts_plain_and_rejects_escapes() {
        assert_eq!(normalize_rel_path("js/a.js").as_deref(), Some("js/a.js"));
        assert_eq!(normalize_rel_path("./js/a.js").as_deref(), Some("js/a.js"));
        assert_eq!(normalize_rel_path("/js/a.js").as_deref(), Some("js/a.js"));
        assert_eq!(normalize_rel_path("js\\a.js").as_deref(), Some("js/a.js"));
        assert_eq!(normalize_rel_path("a/./b.js").as_deref(), Some("a/b.js"));
        assert!(normalize_rel_path("../x.js").is_none());
        assert!(normalize_rel_path("a/../../x.js").is_none());
        assert!(normalize_rel_path("C:/Windows/x.js").is_none());
        assert!(normalize_rel_path("C:\\Windows\\x.js").is_none());
        assert!(normalize_rel_path("").is_none());
        assert!(normalize_rel_path("   ").is_none());
        assert!(normalize_rel_path("a\0b").is_none());
        assert!(normalize_rel_path("//server/share/x.js").is_some()); // leading slashes stripped -> relative
    }

    // ---- match patterns ----
    #[test]
    fn match_patterns() {
        assert_eq!(
            match_pattern_to_globs("<all_urls>").unwrap(),
            vec!["http://*/*", "https://*/*"]
        );
        assert_eq!(
            match_pattern_to_globs("*://*.example.com/*").unwrap(),
            vec![
                "http://*.example.com/*",
                "http://example.com/*",
                "https://*.example.com/*",
                "https://example.com/*"
            ]
        );
        assert_eq!(match_pattern_to_globs("https://a.com/x*").unwrap(), vec!["https://a.com/x*"]);
        assert_eq!(match_pattern_to_globs("*://*/*").unwrap(), vec!["http://*/*", "https://*/*"]);
        assert!(match_pattern_to_globs("file:///*").is_err());
        assert!(match_pattern_to_globs("ftp://a.com/*").is_err());
        assert!(match_pattern_to_globs("https://a.com").is_err()); // no path
        assert!(match_pattern_to_globs("nonsense").is_err());
        assert!(match_pattern_to_globs("https:///x").is_err()); // no host
    }

    #[test]
    fn sanitize_ids() {
        assert_eq!(sanitize_id("Dark Reader!"), "dark-reader");
        assert_eq!(sanitize_id("{abc-123}@x.org"), "abc-123-x.org");
        assert_eq!(sanitize_id("../../evil"), "evil");
        assert_eq!(sanitize_id("///"), "");
        assert!(sanitize_id(&"a".repeat(200)).len() <= 48);
    }

    // ---- conversion ----
    const CHROME_MV3: &str = r#"{
        "manifest_version": 3,
        "name": "__MSG_appName__",
        "default_locale": "en",
        "version": "2.1.0",
        "background": {"service_worker": "bg.js"},
        "action": {"default_popup": "popup.html"},
        "permissions": ["storage", "declarativeNetRequest"],
        "content_scripts": [{
            "matches": ["*://*.example.com/*", "file:///*"],
            "exclude_matches": ["*://mail.example.com/*"],
            "js": ["content/a.js", "../../steal.js"],
            "css": ["content/a.css"],
            "run_at": "document_start"
        }]
    }"#;

    fn chrome_ext_files() -> Vec<(&'static str, &'static str)> {
        vec![
            ("manifest.json", CHROME_MV3),
            ("_locales/en/messages.json", "\u{feff}{\"appName\": {\"message\": \"Cool Tweaks\"}}"),
            ("content/a.js", "chrome.storage.local.get('x'); console.log('hi')"),
            ("content/a.css", "body{color:red}"),
            ("bg.js", "// background"),
        ]
    }

    #[test]
    fn converts_chrome_mv3_manifest() {
        let dir = tmpdir("conv1");
        write_dir_ext(&dir, &chrome_ext_files());
        let c = convert(&Source::Dir(dir.clone()), "chrome", Some("abcdefabcdef"), "Chrome").unwrap();
        assert_eq!(c.name, "Cool Tweaks"); // __MSG_ resolved, BOM tolerated
        assert_eq!(c.scripts.len(), 1);
        let m = &c.scripts[0].manifest;
        assert_eq!(m["id"], "imp-chrome-abcdefabcdef");
        assert_eq!(m["run_at"], "document_start");
        assert_eq!(m["js"], json!(["content/a.js"])); // traversal path dropped
        assert_eq!(m["css"], json!(["content/a.css"]));
        assert_eq!(m["permissions"], json!([]));
        assert_eq!(m["imported_from"], "Chrome");
        // file:// pattern dropped, others converted, bare domain added
        let matches: Vec<String> = m["matches"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert!(matches.contains(&"https://example.com/*".to_string()));
        assert!(matches.contains(&"http://*.example.com/*".to_string()));
        assert!(!matches.iter().any(|g| g.starts_with("file")));
        assert_eq!(m["exclude_matches"][0], "http://mail.example.com/*");
        // warnings mention the important losses
        let w = m["import_warnings"].to_string();
        assert!(w.contains("background"), "{w}");
        assert!(w.contains("popup"), "{w}");
        assert!(w.contains("network requests"), "{w}");
        assert!(w.contains("chrome.storage"), "{w}");
        assert!(w.contains("unsafe file path"), "{w}");
        assert!(w.contains("file://"), "{w}");
        // only the readable, safe files were carried over (no bg.js)
        let paths: Vec<&str> = c.scripts[0].files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"content/a.js") && paths.contains(&"content/a.css"));
        assert!(!paths.contains(&"bg.js"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn firefox_xpi_uses_gecko_id_and_run_at_mapping() {
        let manifest = r#"{
            "manifest_version": 2, "name": "FF Thing", "version": "1.0",
            "browser_specific_settings": {"gecko": {"id": "thing@example.org"}},
            "content_scripts": [
              {"matches": ["https://a.com/*"], "js": ["a.js"], "run_at": "document_end"},
              {"matches": ["https://b.com/*"], "js": ["b.js"]}
            ]}"#;
        let zipb = make_zip(&[("manifest.json", manifest), ("a.js", "browser.tabs.query()"), ("b.js", "1")]);
        let c = convert(&Source::from_bytes(zipb).unwrap(), "firefox", None, "Firefox").unwrap();
        assert_eq!(c.scripts.len(), 2);
        assert_eq!(c.scripts[0].id, "imp-firefox-thing-example.org");
        assert_eq!(c.scripts[1].id, "imp-firefox-thing-example.org-2");
        assert_eq!(c.scripts[0].manifest["run_at"], "document_idle"); // document_end -> idle
        assert_eq!(c.scripts[1].manifest["name"], "FF Thing (script 2)");
        assert!(c.scripts[0].manifest["import_warnings"].to_string().contains("chrome.tabs"));
        assert!(c.warnings.iter().any(|w| w.contains("separate script groups")));
    }

    #[test]
    fn no_content_scripts_is_a_clear_error_not_an_empty_import() {
        let m = r#"{"manifest_version":3,"name":"BG only","version":"1","background":{"service_worker":"b.js"}}"#;
        let z = make_zip(&[("manifest.json", m), ("b.js", "")]);
        let err = convert(&Source::from_bytes(z).unwrap(), "chrome", None, "Chrome").err().unwrap();
        assert!(err.contains("no content scripts"), "{err}");
    }

    #[test]
    fn missing_manifest_and_bad_json_are_errors() {
        let z = make_zip(&[("readme.txt", "x")]);
        assert!(convert(&Source::from_bytes(z).unwrap(), "x", None, "X").is_err());
        let z = make_zip(&[("manifest.json", "{ nope")]);
        assert!(convert(&Source::from_bytes(z).unwrap(), "x", None, "X").is_err());
    }

    #[test]
    fn group_with_only_unsupported_patterns_or_missing_files_is_skipped() {
        let m = r#"{"name":"X","version":"1","content_scripts":[
            {"matches":["file:///*"],"js":["a.js"]},
            {"matches":["https://ok.com/*"],"js":["missing.js"]},
            {"matches":["https://ok.com/*"],"js":["good.js"]}]}"#;
        let z = make_zip(&[("manifest.json", m), ("a.js", "1"), ("good.js", "2")]);
        let c = convert(&Source::from_bytes(z).unwrap(), "chrome", Some("id1"), "Chrome").unwrap();
        assert_eq!(c.scripts.len(), 1);
        assert!(c.warnings.iter().any(|w| w.contains("skipped")));
    }

    // ---- archives ----
    #[test]
    fn crx3_and_crx2_headers_are_stripped() {
        let z = make_zip(&[("manifest.json", r#"{"name":"C","version":"1","content_scripts":[{"matches":["<all_urls>"],"js":["a.js"]}]}"#), ("a.js", "x")]);
        // CRX3: magic, version 3, header_len, header bytes
        let mut crx3 = b"Cr24".to_vec();
        crx3.extend_from_slice(&3u32.to_le_bytes());
        crx3.extend_from_slice(&5u32.to_le_bytes());
        crx3.extend_from_slice(&[9, 9, 9, 9, 9]);
        crx3.extend_from_slice(&z);
        assert!(convert(&Source::from_bytes(crx3).unwrap(), "chrome", Some("i"), "Chrome").is_ok());
        // CRX2: magic, version 2, pubkey_len, sig_len, pubkey, sig
        let mut crx2 = b"Cr24".to_vec();
        crx2.extend_from_slice(&2u32.to_le_bytes());
        crx2.extend_from_slice(&3u32.to_le_bytes());
        crx2.extend_from_slice(&2u32.to_le_bytes());
        crx2.extend_from_slice(&[1, 1, 1, 2, 2]);
        crx2.extend_from_slice(&z);
        assert!(convert(&Source::from_bytes(crx2).unwrap(), "chrome", Some("i"), "Chrome").is_ok());
    }

    #[test]
    fn corrupt_crx_does_not_panic() {
        assert!(Source::from_bytes(b"Cr24".to_vec()).is_err());
        let mut huge = b"Cr24".to_vec();
        huge.extend_from_slice(&3u32.to_le_bytes());
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(Source::from_bytes(huge).is_err());
        let mut v9 = b"Cr24".to_vec();
        v9.extend_from_slice(&9u32.to_le_bytes());
        v9.extend_from_slice(&[0; 16]);
        assert!(Source::from_bytes(v9).is_err());
        assert!(Source::from_bytes(b"not a zip at all".to_vec()).is_err());
        assert!(Source::from_bytes(vec![]).is_err());
    }

    #[test]
    fn oversized_file_in_archive_is_refused() {
        let big = "a".repeat((MAX_FILE_BYTES + 10) as usize);
        let m = r#"{"name":"X","version":"1","content_scripts":[{"matches":["<all_urls>"],"js":["big.js"]}]}"#;
        let z = make_zip(&[("manifest.json", m), ("big.js", &big)]);
        let err = convert(&Source::from_bytes(z).unwrap(), "x", Some("i"), "X").err().unwrap();
        assert!(err.contains("none of its content scripts"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escaping_the_folder_is_not_followed() {
        let outside = tmpdir("outside");
        fs::write(outside.join("secret.js"), "SECRET").unwrap();
        let dir = tmpdir("symlink");
        write_dir_ext(&dir, &[("manifest.json", r#"{"name":"S","version":"1","content_scripts":[{"matches":["<all_urls>"],"js":["link.js"]}]}"#)]);
        std::os::unix::fs::symlink(outside.join("secret.js"), dir.join("link.js")).unwrap();
        let r = convert(&Source::Dir(dir.clone()), "x", Some("i"), "X");
        assert!(r.is_err(), "symlinked file outside the extension must not be read");
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_dir_all(outside);
    }

    // ---- writing ----
    #[test]
    fn end_to_end_import_writes_a_loadable_disabled_extension_folder() {
        let src_dir = tmpdir("e2e-src");
        write_dir_ext(&src_dir, &chrome_ext_files());
        let root = tmpdir("e2e-root");
        let out = import_from_path(&root, &src_dir, Some("chrome"), Some("Chrome"), Some("abcdefabcdef"));
        assert!(out.ok, "{:?}", out.error);
        assert_eq!(out.imported.len(), 1);
        let folder = root.join("imp-chrome-abcdefabcdef");
        let manifest: Value = serde_json::from_str(&fs::read_to_string(folder.join("manifest.json")).unwrap()).unwrap();
        // Same shape Kite's own ExtensionManifest loader expects.
        for key in ["id", "name", "version", "matches", "exclude_matches", "run_at", "js", "css", "permissions"] {
            assert!(manifest.get(key).is_some(), "missing {key}");
        }
        assert_eq!(fs::read_to_string(folder.join("content/a.css")).unwrap(), "body{color:red}");
        assert!(!folder.join("bg.js").exists());
        // no leftovers
        assert!(fs::read_dir(&root).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(".import-")));

        // Re-import replaces cleanly instead of failing or duplicating.
        let again = import_from_path(&root, &src_dir, Some("chrome"), Some("Chrome"), Some("abcdefabcdef"));
        assert!(again.ok);
        assert_eq!(fs::read_dir(&root).unwrap().flatten().count(), 1);
        let _ = fs::remove_dir_all(src_dir);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn manual_import_of_xpi_file_derives_tag_and_failure_is_reported() {
        let root = tmpdir("manual-root");
        let files = tmpdir("manual-files");
        let m = r#"{"name":"Manual","version":"1","browser_specific_settings":{"gecko":{"id":"m@x"}},"content_scripts":[{"matches":["https://a.com/*"],"js":["a.js"]}]}"#;
        let xpi = files.join("thing.xpi");
        fs::write(&xpi, make_zip(&[("manifest.json", m), ("a.js", "1")])).unwrap();
        let ok = import_from_path(&root, &xpi, None, None, None);
        assert!(ok.ok, "{:?}", ok.error);
        assert_eq!(ok.imported[0].id, "imp-firefox-m-x");

        let bad = files.join("bad.zip");
        fs::write(&bad, b"garbage").unwrap();
        let r = import_from_path(&root, &bad, None, None, None);
        assert!(!r.ok && r.error.is_some());
        let r = import_from_path(&root, &files.join("nope.crx"), None, None, None);
        assert!(!r.ok);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(files);
    }

    // ---- scanning ----
    #[test]
    fn scans_chromium_profiles_picks_newest_version_and_dedupes_profiles() {
        let user_data = tmpdir("scan-chrome");
        let ext = r#"{"name":"__MSG_n__","default_locale":"en","version":"1","content_scripts":[{"matches":["<all_urls>"],"js":["a.js"]}]}"#;
        let loc = r#"{"n":{"message":"Scanned"}}"#;
        let old = r#"{"name":"OLD","version":"1","content_scripts":[{"matches":["<all_urls>"],"js":["a.js"]}]}"#;
        write_dir_ext(&user_data.join("Default/Extensions/aaaa/1.9_0"), &[("manifest.json", old)]);
        write_dir_ext(&user_data.join("Default/Extensions/aaaa/1.10_0"), &[("manifest.json", ext), ("_locales/en/messages.json", loc)]);
        // same extension in a second profile -> not listed twice
        write_dir_ext(&user_data.join("Profile 1/Extensions/aaaa/1.10_0"), &[("manifest.json", ext)]);
        // background-only extension -> listed, but not importable
        write_dir_ext(&user_data.join("Default/Extensions/bbbb/1.0_0"), &[("manifest.json", r#"{"name":"BgOnly","version":"1"}"#)]);
        // junk dirs ignored
        fs::create_dir_all(user_data.join("Crashpad")).unwrap();
        let roots = ScanRoots { chromium: vec![("Chrome".into(), user_data.clone())], firefox_profiles: None };
        let found = scan_installed(&roots);
        assert_eq!(found.len(), 2, "{found:#?}");
        assert!(found[0].importable && found[0].name == "Scanned" && found[0].ext_id == "aaaa");
        assert!(found[0].path.ends_with("1.10_0"));
        assert!(!found[1].importable && found[1].reason.is_some());
        let _ = fs::remove_dir_all(user_data);
    }

    #[test]
    fn scans_firefox_profile_using_extensions_json() {
        let profiles = tmpdir("scan-ff");
        let prof = profiles.join("abcd.default-release");
        fs::create_dir_all(prof.join("extensions")).unwrap();
        let m = r#"{"name":"FF Scanned","version":"3","content_scripts":[{"matches":["https://a.com/*"],"js":["a.js"]}]}"#;
        let xpi = prof.join("extensions").join("one@x.xpi");
        fs::write(&xpi, make_zip(&[("manifest.json", m), ("a.js", "1")])).unwrap();
        let ej = json!({"addons": [
            {"id": "one@x", "type": "extension", "location": "app-profile", "path": xpi.to_string_lossy(), "defaultLocale": {"name": "One"}},
            {"id": "sys@x", "type": "extension", "location": "app-builtin", "path": xpi.to_string_lossy()},
            {"id": "theme@x", "type": "theme", "location": "app-profile", "path": xpi.to_string_lossy()},
            {"id": "gone@x", "type": "extension", "location": "app-profile", "path": "/definitely/not/here.xpi"}
        ]});
        fs::write(prof.join("extensions.json"), ej.to_string()).unwrap();
        let roots = ScanRoots { chromium: vec![], firefox_profiles: Some(profiles.clone()) };
        let found = scan_installed(&roots);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].name, "FF Scanned");
        assert!(found[0].importable);
        assert_eq!(found[0].browser, "Firefox");
        let _ = fs::remove_dir_all(profiles);
    }

    #[test]
    fn scanning_with_nothing_installed_is_just_empty() {
        let roots = ScanRoots {
            chromium: vec![("Chrome".into(), PathBuf::from("/no/such/dir"))],
            firefox_profiles: Some(PathBuf::from("/no/such/ff")),
        };
        assert!(scan_installed(&roots).is_empty());
    }

    // ---- api usage detector ----
    #[test]
    fn api_detector_has_word_boundaries() {
        assert_eq!(find_api_usage("chrome.storage.local.get()"), vec!["storage"]);
        assert_eq!(find_api_usage("browser.runtime.sendMessage()"), vec!["runtime"]);
        assert!(find_api_usage("mychrome.storage.x").is_empty());
        assert!(find_api_usage("chrome.storageArea").is_empty());
        assert!(find_api_usage("window.chrome.tabs").is_empty()); // preceded by '.'
        assert!(find_api_usage("const s = 'nothing here'").is_empty());
    }
}
