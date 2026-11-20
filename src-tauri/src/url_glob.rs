// Tiny Tampermonkey-style glob -> regex translator used by Kite's
// extension loader. The regex strings it produces are evaluated in the
// *page* (see build_extension_script in main.rs), so they target
// JavaScript's RegExp - kept to constructs that mean the same thing there
// and in Rust's `regex` crate (which the tests below use to check them).
//
// Two shapes:
//
//   scheme://host/path   -> "host-aware" translation. `*` in the host stops
//                           at '/', so `*://*.example.com/*` can't be
//                           satisfied by `https://evil.com/x.example.com/`
//                           (the plain ".*" translation could). A port is
//                           tolerated when the pattern doesn't name one,
//                           matching how browser match patterns behave.
//   anything else        -> the original behaviour: `*` becomes `.*`.
//
// Why this exists as its own file: extension *import* feeds third-party
// match patterns (Chrome/Firefox manifests) into this matcher, and for
// those the looser ".*" host handling was no longer good enough.

const REGEX_META: &str = "\\.+?^$()[]{}|";

fn push_escaped(out: &mut String, ch: char) {
    if REGEX_META.contains(ch) {
        out.push('\\');
    }
    out.push(ch);
}

fn legacy(pattern: &str) -> String {
    let mut out = String::from("^");
    for ch in pattern.chars() {
        if ch == '*' {
            out.push_str(".*");
        } else {
            push_escaped(&mut out, ch);
        }
    }
    out.push('$');
    out
}

pub fn glob_to_regex(pattern: &str) -> String {
    let Some(idx) = pattern.find("://") else {
        return legacy(pattern);
    };
    let scheme = &pattern[..idx];
    let rest = &pattern[idx + 3..];

    let scheme_ok = !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c == '*' || c.is_ascii_alphanumeric() || "+.-".contains(c));
    // No '/' after the host (e.g. "*://*") means there's no host/path
    // boundary to be strict about - keep the old permissive behaviour.
    let Some(slash) = rest.find('/') else {
        return legacy(pattern);
    };
    if !scheme_ok {
        return legacy(pattern);
    }
    let host = &rest[..slash];
    let path = &rest[slash..];

    let mut out = String::from("^");
    for ch in scheme.chars() {
        if ch == '*' {
            // Any scheme, same as the old ".*" did for "*://..." patterns
            // (built-in extensions rely on matching file:// pages too).
            out.push_str("[A-Za-z][A-Za-z0-9+.-]*");
        } else {
            push_escaped(&mut out, ch);
        }
    }
    out.push_str("://");
    for ch in host.chars() {
        if ch == '*' {
            out.push_str("[^/]*");
        } else {
            push_escaped(&mut out, ch);
        }
    }
    if !host.contains(':') && !host.ends_with('*') && !host.is_empty() {
        out.push_str("(?::[0-9]+)?");
    }
    for ch in path.chars() {
        if ch == '*' {
            out.push_str(".*");
        } else {
            push_escaped(&mut out, ch);
        }
    }
    out.push('$');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    fn m(pattern: &str, url: &str) -> bool {
        Regex::new(&glob_to_regex(pattern)).unwrap().is_match(url)
    }

    // --- The patterns Kite's own built-in extensions ship with must keep
    // --- matching exactly what they matched before.
    #[test]
    fn builtin_all_urls_pattern_unchanged() {
        assert!(m("*://*/*", "https://example.com/"));
        assert!(m("*://*/*", "http://example.com/a/b?c=d#e"));
        assert!(m("*://*/*", "http://localhost:3000/x"));
        assert!(m("*://*/*", "file:///C:/pages/index.html"));
    }

    #[test]
    fn builtin_youtube_patterns_unchanged() {
        assert!(m("*://www.youtube.com/*", "https://www.youtube.com/watch?v=abc"));
        assert!(m("*://youtube.com/*", "https://youtube.com/"));
        assert!(m("*://m.youtube.com/*", "http://m.youtube.com/shorts/x"));
        assert!(!m("*://www.youtube.com/*", "https://www.notyoutube.com/"));
    }

    // --- The reason this module exists.
    #[test]
    fn host_wildcard_cannot_cross_into_the_path() {
        assert!(m("https://*.example.com/*", "https://a.example.com/x"));
        assert!(m("https://*.example.com/*", "https://a.b.example.com/"));
        assert!(!m("https://*.example.com/*", "https://evil.com/x.example.com/"));
        assert!(!m("https://*.example.com/*", "https://evil.com/?u=a.example.com/"));
        assert!(!m("https://*.example.com/*", "https://a.example.com.evil.com/"));
        assert!(!m("*://*.example.com/*", "https://evil.com/a.example.com/b"));
    }

    #[test]
    fn exact_host_is_exact() {
        assert!(m("https://example.com/*", "https://example.com/a"));
        assert!(!m("https://example.com/*", "https://example.com.evil.com/a"));
        assert!(!m("https://example.com/*", "https://xexample.com/a"));
        assert!(!m("https://example.com/*", "http://example.com/a"));
    }

    #[test]
    fn port_is_tolerated_unless_pattern_names_one() {
        assert!(m("http://localhost/*", "http://localhost:8080/a"));
        assert!(m("http://localhost/*", "http://localhost/a"));
        assert!(m("http://localhost:8080/*", "http://localhost:8080/a"));
        assert!(!m("http://localhost:8080/*", "http://localhost:9090/a"));
    }

    #[test]
    fn path_wildcards_and_literals() {
        assert!(m("https://example.com/a/*/c", "https://example.com/a/b/c"));
        assert!(!m("https://example.com/a/*/c", "https://example.com/a/b/d"));
        assert!(m("https://example.com/search?q=*", "https://example.com/search?q=rust"));
    }

    #[test]
    fn regex_metacharacters_in_host_and_path_are_escaped() {
        assert!(!m("https://exa.ple.com/*", "https://exaXple.com/"));
        assert!(m("https://example.com/a+b", "https://example.com/a+b"));
        assert!(!m("https://example.com/a+b", "https://example.com/aab"));
    }

    #[test]
    fn non_url_patterns_keep_legacy_behaviour() {
        assert!(m("*", "anything at all"));
        assert!(m("about:*", "about:blank"));
        // no path boundary -> permissive, as before
        assert!(m("*://*", "https://example.com/deep/path"));
    }
}
