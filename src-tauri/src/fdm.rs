// Free Download Manager (FDM) handoff.
//
// Kite hands a download to FDM by launching fdm.exe with the file's URL;
// FDM then shows its own "Add download" flow and does the transfer. This
// module is only the mechanics (find the exe, build the command line,
// launch it) - the decision of *when* to hand off lives next to the
// on_download hook in main.rs.
//
// WHAT IS AND ISN'T VERIFIED
//
// FDM's official documentation doesn't spell out its command-line syntax
// (its changelog confirms command-line support and a `qurl` argument, but
// not the exact form). The default here - the URL as a plain positional
// argument - is the conventional shape, but it is exactly the part that
// needs confirming against a real FDM install. So the argument template is
// user-editable ("{url}" is replaced by the link), and Settings has a
// "send test link" button to verify the handoff in seconds.
//
// SAFETY
//   - fdm.exe is launched directly with an argument vector, never through
//     a shell, so a hostile URL can't inject commands.
//   - Only http/https/ftp URLs are ever forwarded; blob:/data:/file: and
//     anything else stays in Kite.
//   - A user-chosen executable must literally be named fdm.exe.
//   - Launch failures are returned as errors; the caller falls back to
//     Kite's normal download so a failed handoff never loses a download.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use url::Url;

pub const DEFAULT_ARGS: &str = "{url}";
const MAX_URL_LEN: usize = 8192;

pub fn is_fdm_exe_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("fdm.exe"))
        .unwrap_or(false)
}

// Where FDM 6 normally installs (per-machine, per-user, and the older
// vendor-folder layout). Env-var based so it's inert off Windows.
pub fn default_install_candidates() -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Some(v) = std::env::var_os(var) {
            let p = PathBuf::from(v);
            if !bases.contains(&p) {
                bases.push(p);
            }
        }
    }
    if let Some(v) = std::env::var_os("LOCALAPPDATA") {
        bases.push(PathBuf::from(v).join("Programs"));
    }
    let mut out = Vec::new();
    for base in bases {
        for rel in [
            "FreeDownloadManager.ORG/Free Download Manager/fdm.exe",
            "Softdeluxe/Free Download Manager/fdm.exe",
            "Free Download Manager/fdm.exe",
        ] {
            out.push(base.join(rel));
        }
    }
    out
}

// A configured path wins if it still exists; otherwise fall back to the
// usual install locations.
pub fn find(custom: Option<&str>) -> Option<PathBuf> {
    if let Some(c) = custom.map(str::trim).filter(|c| !c.is_empty()) {
        let p = PathBuf::from(c);
        if is_fdm_exe_name(&p) && p.is_file() {
            return Some(p);
        }
    }
    default_install_candidates().into_iter().find(|p| p.is_file())
}

// Only plain downloadable web URLs are handed off.
pub fn is_handoff_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https" | "ftp") && url.as_str().len() <= MAX_URL_LEN
}

// Template -> argument vector. Whitespace-separated tokens; any token
// containing "{url}" has it replaced. If no token mentions {url} the URL is
// appended, so a mangled template still passes the link.
pub fn build_args(template: &str, url: &str) -> Vec<String> {
    let template = if template.trim().is_empty() { DEFAULT_ARGS } else { template };
    let mut args: Vec<String> = Vec::new();
    let mut used = false;
    for tok in template.split_whitespace() {
        if tok.contains("{url}") {
            used = true;
            args.push(tok.replace("{url}", url));
        } else {
            args.push(tok.to_string());
        }
    }
    if !used {
        args.push(url.to_string());
    }
    args
}

// Starts FDM and returns immediately; FDM owns the transfer from here.
// Ok means the process started - not that FDM accepted the link.
pub fn launch(exe: &Path, template: &str, url: &Url) -> Result<(), String> {
    if !is_fdm_exe_name(exe) {
        return Err("not a Free Download Manager executable".to_string());
    }
    if !is_handoff_url(url) {
        return Err("only http, https and ftp links can be sent to Free Download Manager".to_string());
    }
    let args = build_args(template, url.as_str());
    Command::new(exe)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_child| ()) // not waited on: FDM keeps running on its own
        .map_err(|e| format!("couldn't start Free Download Manager: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_name_check_is_strict() {
        assert!(is_fdm_exe_name(Path::new("C:/x/fdm.exe")));
        assert!(is_fdm_exe_name(Path::new("C:/x/FDM.EXE")));
        assert!(!is_fdm_exe_name(Path::new("C:/x/cmd.exe")));
        assert!(!is_fdm_exe_name(Path::new("C:/x/fdm.exe.bat")));
        assert!(!is_fdm_exe_name(Path::new("C:/x/notfdm.exe")));
        assert!(!is_fdm_exe_name(Path::new("")));
    }

    #[test]
    fn only_plain_web_urls_are_handed_off() {
        for ok in ["http://a.com/f.zip", "https://a.com/f.zip?x=1", "ftp://a.com/f"] {
            assert!(is_handoff_url(&Url::parse(ok).unwrap()), "{ok}");
        }
        for no in ["blob:https://a.com/uuid", "data:text/plain,hi", "file:///C:/x", "javascript:alert(1)", "about:blank", "kite://home"] {
            assert!(!is_handoff_url(&Url::parse(no).unwrap()), "{no}");
        }
        let long = format!("https://a.com/{}", "a".repeat(9000));
        assert!(!is_handoff_url(&Url::parse(&long).unwrap()));
    }

    #[test]
    fn default_template_passes_the_url_as_one_argument() {
        let u = "https://a.com/my file.zip?x=1&y=2";
        let parsed = Url::parse(u).unwrap();
        assert_eq!(build_args("{url}", parsed.as_str()), vec![parsed.as_str().to_string()]);
        assert_eq!(build_args("", "https://a.com/x"), vec!["https://a.com/x"]);
        assert_eq!(build_args("   ", "https://a.com/x"), vec!["https://a.com/x"]);
    }

    #[test]
    fn custom_templates() {
        assert_eq!(build_args("--add {url}", "https://a.com/x"), vec!["--add", "https://a.com/x"]);
        assert_eq!(build_args("--url={url}", "https://a.com/x"), vec!["--url=https://a.com/x"]);
        // template without {url}: link still gets appended
        assert_eq!(build_args("--silent", "https://a.com/x"), vec!["--silent", "https://a.com/x"]);
    }

    #[test]
    fn hostile_url_stays_a_single_inert_argument() {
        // Spaces/quotes/ampersands in a URL must not split into extra args
        // or be interpreted - there is no shell involved.
        let u = Url::parse("https://a.com/x?a=1&b=%22%20--evil%20%26calc.exe").unwrap();
        let args = build_args("{url}", u.as_str());
        assert_eq!(args.len(), 1);
        assert!(args[0].starts_with("https://a.com/"));
    }

    #[test]
    fn find_prefers_valid_custom_path_and_ignores_bad_ones() {
        let dir = std::env::temp_dir().join(format!("kite-fdm-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("fdm.exe");
        std::fs::write(&good, b"").unwrap();
        let wrong_name = dir.join("other.exe");
        std::fs::write(&wrong_name, b"").unwrap();
        assert_eq!(find(Some(good.to_str().unwrap())), Some(good.clone()));
        // wrong file name is rejected even though it exists
        let r = find(Some(wrong_name.to_str().unwrap()));
        assert_ne!(r, Some(wrong_name));
        // nonexistent custom path doesn't panic
        let _ = find(Some("/no/such/fdm.exe"));
        let _ = find(None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn launch_refuses_bad_inputs_without_spawning() {
        let u = Url::parse("https://a.com/x").unwrap();
        assert!(launch(Path::new("/bin/sh"), "{url}", &u).is_err()); // wrong exe name
        let blob = Url::parse("blob:https://a.com/1").unwrap();
        assert!(launch(Path::new("fdm.exe"), "{url}", &blob).is_err()); // bad scheme
        // right name but doesn't exist -> a clean Err, not a panic
        assert!(launch(Path::new("/no/such/fdm.exe"), "{url}", &u).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn launch_actually_passes_the_url_to_the_program() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("kite-fdm-spawn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("received.txt");
        let exe = dir.join("fdm.exe");
        // A fake "fdm.exe": writes each argument on its own line.
        std::fs::write(&exe, format!("#!/bin/sh\nfor a in \"$@\"; do echo \"$a\" >> '{}'; done\n", out.display())).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let u = Url::parse("https://example.com/a%20b/file.zip?x=1&y=2").unwrap();
        launch(&exe, "--add {url}", &u).unwrap();
        for _ in 0..50 {
            if out.exists() { break; }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        let got = std::fs::read_to_string(&out).unwrap();
        assert_eq!(got.lines().collect::<Vec<_>>(), vec!["--add", u.as_str()]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
