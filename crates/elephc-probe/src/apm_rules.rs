//! Purpose:
//! The `[apm] include` and `exclude` patterns of a project's `elephc.toml`: which
//! transactions — routes, script runs, messages, named boundaries, library calls — are
//! counted at all. A health check or a framework's debug toolbar polled every second is
//! noise in a transaction table; excluded, it takes no row, no new-route budget, no in-flight
//! row and no capture, as if it had never been served.
//!
//! Called from:
//! - `elephc_probe_set_route` (a web request's label), `elephc_probe_note_transaction` and
//!   `elephc_probe_request_begin` (an isolated web worker recording by name),
//!   `elephc_probe_txn_begin` (messages, named boundaries, library calls) and
//!   `apm_runtime::close_at_exit` (a script's run).
//!
//! Key details:
//! - The rules are baked into the binary at build time: codegen stores the address of a
//!   NUL-terminated text, one rule per line, `+pattern` to include and `-pattern` to exclude,
//!   into `elephc_probe_apm_rules` in main's prologue (or `elephc_init`), before the first
//!   transaction. Without rules everything is counted, as before.
//! - A pattern matches the whole label (`GET /health`, `CLI console cache:clear`,
//!   `MSG App\Message\Ping`), `*` standing for any run of characters, case included.
//! - With any `include`, only what an include matches is counted; an `exclude` then removes
//!   exceptions from that, and from everything when there is no include.
//! - A label is matched when its transaction starts; a route already in the table was
//!   matched before it got there, so a counted request costs no matching at all.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Address of the build's rule text, 0 when the build has none. Written by the generated
/// prologue before any transaction, read once.
#[no_mangle]
pub static mut elephc_probe_apm_rules: usize = 0;

/// Whether the current web request is excluded: set by `elephc_probe_set_route`, read by the
/// calls that would count the request or its work. Process-local: one request at a time.
pub(crate) static REQUEST_IGNORED: AtomicBool = AtomicBool::new(false);

/// A build's include and exclude patterns.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Rules {
    include: Vec<String>,
    exclude: Vec<String>,
    /// The build's identity, `<unix seconds> <label>`, from the `@` line: what snapshots
    /// carry so a collector can mark the deploys of a new build.
    build: Option<String>,
}

impl Rules {
    /// Reads the baked text: `+pattern` and `-pattern` lines and the `@<seconds> <label>`
    /// build line; anything else is ignored.
    pub(crate) fn parse(text: &str) -> Self {
        let mut rules = Self::default();
        for line in text.lines() {
            if let Some(pattern) = line.strip_prefix('+') {
                rules.include.push(pattern.to_string());
            } else if let Some(pattern) = line.strip_prefix('-') {
                rules.exclude.push(pattern.to_string());
            } else if let Some(build) = line.strip_prefix('@') {
                rules.build = Some(build.trim().to_string()).filter(|build| !build.is_empty());
            }
        }
        rules
    }

    /// Whether a transaction named `label` is counted.
    pub(crate) fn counts(&self, label: &str) -> bool {
        (self.include.is_empty() || self.include.iter().any(|pattern| matches(pattern, label)))
            && !self.exclude.iter().any(|pattern| matches(pattern, label))
    }

    /// Whether there is anything to match.
    fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }
}

/// The build's rules, read from `elephc_probe_apm_rules` on first use.
fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| {
        let address = unsafe { std::ptr::addr_of!(elephc_probe_apm_rules).read_volatile() };
        if address == 0 {
            return Rules::default();
        }
        let text = unsafe { std::ffi::CStr::from_ptr(address as *const libc::c_char) };
        Rules::parse(&text.to_string_lossy())
    })
}

/// The build's identity, `<unix seconds> <label>`, when the build carries one.
pub(crate) fn build() -> Option<&'static str> {
    rules().build.as_deref()
}

/// Whether a transaction named `label` is counted under the build's rules.
pub(crate) fn counts(label: &str) -> bool {
    #[cfg(test)]
    if let Some(rules) = TEST_RULES.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return rules.counts(label);
    }
    let rules = rules();
    rules.is_empty() || rules.counts(label)
}

/// Rules a test sets in place of the build's, which are read once per process.
#[cfg(test)]
static TEST_RULES: std::sync::Mutex<Option<Rules>> = std::sync::Mutex::new(None);

/// Sets the rules the probe's tests count under, or clears them.
#[cfg(test)]
pub(crate) fn set_test_rules(text: Option<&str>) {
    *TEST_RULES.lock().unwrap_or_else(|e| e.into_inner()) = text.map(Rules::parse);
}

/// Whether `pattern`, where `*` stands for any run of characters, matches all of `text`.
fn matches(pattern: &str, text: &str) -> bool {
    let (pattern, text) = (pattern.as_bytes(), text.as_bytes());
    // Iterative glob with one backtrack point: the last `*` seen and where it resumed.
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && pattern[p] == b'*' {
            star = Some((p, t));
            p += 1;
        } else if p < pattern.len() && pattern[p] == text[t] {
            p += 1;
            t += 1;
        } else if let Some((star_p, star_t)) = star {
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|byte| *byte == b'*')
}

/// Marks the current request excluded, or not.
pub(crate) fn set_request_ignored(ignored: bool) {
    REQUEST_IGNORED.store(ignored, Ordering::Relaxed);
}

/// Whether the current request is excluded.
pub(crate) fn request_ignored() -> bool {
    REQUEST_IGNORED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `*` stands for any run of characters, anywhere, and the whole label must match.
    #[test]
    fn patterns_match_whole_labels() {
        assert!(matches("GET /health", "GET /health"));
        assert!(!matches("GET /health", "GET /healthz"));
        assert!(matches("GET /_profiler/*", "GET /_profiler/abc/search"));
        assert!(matches("*/health", "HEAD /health"));
        assert!(matches("CLI console cache:*", "CLI console cache:clear"));
        assert!(!matches("CLI console cache:*", "CLI console app:import"));
        assert!(matches("GET /*/edit", "GET /posts/{id}/edit"));
        assert!(matches("*", ""));
        assert!(!matches("GET", "get"), "case is kept");
    }

    /// No rules count everything; an include narrows to what it names, and an exclude
    /// removes exceptions inside it.
    #[test]
    fn include_narrows_and_exclude_removes() {
        assert!(Rules::default().counts("GET /anything"));
        let excluding = Rules::parse("-GET /health\n-CLI console cache:*\n");
        assert!(!excluding.counts("GET /health"));
        assert!(!excluding.counts("CLI console cache:clear"));
        assert!(excluding.counts("GET /products"));
        let narrowed = Rules::parse("+GET /api/*\n-GET /api/ping\n");
        assert!(narrowed.counts("GET /api/orders"));
        assert!(!narrowed.counts("GET /api/ping"), "an exception inside the include");
        assert!(!narrowed.counts("GET /admin"), "outside the include");
    }

    /// The `@` line carries the build's identity, and alone it is no rule: everything counts.
    #[test]
    fn the_build_line_is_no_rule() {
        let rules = Rules::parse("@1763540000 v1.4.2\n");
        assert_eq!(rules.build.as_deref(), Some("1763540000 v1.4.2"));
        assert!(rules.is_empty());
        assert_eq!(Rules::parse("@ \n").build, None);
    }
}
