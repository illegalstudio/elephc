//! Purpose:
//! `opcache.blacklist_filename` — the list of paths php-src runs but refuses to CACHE.
//!
//! Called from:
//! - `crate::ffi::context::__elephc_eval_opcache_load_blacklist()` to load it at
//!   eval-context setup, which is elephc's analogue of php-src's `MINIT`.
//! - `crate::script_cache::store::fill_entry` for the admission decision itself.
//!
//! Key details:
//! - A blacklisted script is EXECUTED NORMALLY and merely not stored. It disappears from
//!   `opcache_get_status()['scripts']`, `opcache_is_script_cached()` answers `false` for
//!   it, and each refusal bumps `opcache_statistics.blacklist_misses` by one. VERIFIED
//!   against reference PHP 8.5.10.
//! - The DIRECTIVE VALUE is a `glob()` naming the blacklist FILES, and php-src loads
//!   EVERY matching file and unions their entries — VERIFIED: two files matching
//!   `bl_*.list` blacklisted one script each.
//! - Inside those files, `*` and `?` are wildcards that DO NOT CROSS `/`, and the entry
//!   matches as a PREFIX — anchored at the start, open at the end — so the bare
//!   `…/p_pref` blocks `…/p_prefix.php` and a bare directory blocks everything under it.
//!   Matching is case-sensitive.
//! - THE WILDCARDS STOP AT `/`, and it is worth stating because the opposite is the
//!   natural guess for a matcher php-src builds a regexp from. VERIFIED on reference PHP
//!   8.5.10: `<dir>/*deep.php` blocks `<dir>/xdeep.php` but NOT `<dir>/sub/deep.php`, and
//!   `<dir>/sub?deep.php` blocks neither. A probe that seems to show otherwise is usually
//!   confounded by `opcache.file_update_protection` refusing a just-written file for its
//!   AGE — set it to 0 before drawing any conclusion about the blacklist.
//! - The entry matcher and the directive's own `glob()` therefore differ in ONE respect:
//!   the glob must consume the whole filename, the entry need only match a prefix.
//! - `;` starts a comment and blank lines are skipped.
//! - A directive value matching NO file is not an error: php-src logs
//!   `Warning No blacklist file found matching: <value>` — which needs
//!   `opcache.log_verbosity_level >= 2` to be seen — and blacklists nothing.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use super::accel_log::{accel_log, AccelLogLevel};

/// The byte neither wildcard may consume.
const SEPARATOR: u8 = b'/';

/// The compiled `opcache.blacklist_filename` entries for this process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Blacklist {
    /// One entry per surviving line, in the order php-src would have compiled them into
    /// its alternation. Order is not observable — any match refuses — but keeping it
    /// makes a failing test readable.
    patterns: Vec<String>,
}

impl Blacklist {
    /// The blacklist a binary without the directive observes: empty, blocking nothing.
    pub(crate) const fn empty() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Adds every usable line of one blacklist file's contents.
    ///
    /// Skips blank lines and `;` comments. php-src tests the FIRST character of the line
    /// for `;`, so a comment marker that follows anything else — even whitespace — is not
    /// a comment; the line is kept and simply will not match a real path.
    ///
    /// EVERY SURVIVING LINE IS EXPANDED, not stored verbatim. php-src strips a surrounding
    /// pair of double quotes, then resolves the entry against `base_dir` — the directory of
    /// the blacklist file itself, NOT the process cwd — and normalises `.` and `..`. This is
    /// what makes a list of bare filenames beside the list work at all, and it is also what
    /// `opcache_get_configuration()['blacklist']` reports. VERIFIED against reference PHP
    /// 8.5.10: a list containing only `flat.php` refuses `<dir>/flat.php` and reports the
    /// expanded path; a quoted entry behaves as the unquoted one.
    pub(crate) fn extend_from_file_contents(&mut self, contents: &str, base_dir: &Path) {
        for line in contents.lines() {
            let trimmed = line.trim_end_matches(['\r', ' ', '\t']);
            if trimmed.is_empty() || trimmed.starts_with(';') {
                continue;
            }
            self.patterns.push(expand_entry(trimmed, base_dir));
        }
    }

    /// Returns whether `path` is blacklisted, and must therefore run without being cached.
    pub(crate) fn blocks(&self, path: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| prefix_matches(pattern, path))
    }
}

/// Matches one blacklist entry against a path, the way php-src's matcher does.
///
/// Anchored at the START and NOT at the end, so an entry is a PREFIX: `/srv/vendor/` blocks
/// everything beneath it. Every byte other than a wildcard is literal and case-sensitive.
///
/// THE THREE WILDCARDS, and the difference between the first two is the whole subtlety:
/// php-src compiles `*` to `[^/]*` but `**` to `.*`. So a SINGLE star stops at a separator
/// while a DOUBLED star crosses it, and `?` (compiled to `[^/]`) stops. VERIFIED against
/// reference PHP 8.5.10: `<dir>/**.php` refuses `<dir>/sub/nested.php` while `<dir>/*.php`
/// caches it. An earlier revision treated `**` as one `*`; the differential corpus missed it
/// because a TRAILING `**` behaves identically under both readings.
///
/// Implemented as a backtracking walk rather than by building a regexp: the alternative is
/// a regex engine this crate does not have, for patterns that are a handful of literals and
/// wildcards.
fn prefix_matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.as_bytes();
    let path = path.as_bytes();
    // `star` remembers where to resume after the most recent `*` when a later literal
    // fails, which is what makes this match the regexp's greedy-with-backtracking answer
    // without recursing.
    let (mut p, mut s) = (0usize, 0usize);
    // The third element records whether this star may cross `/` — i.e. whether it was
    // written `**`. Without it a doubled star would merely re-arm the single-star rule.
    let mut star: Option<(usize, usize, bool)> = None;
    while s < path.len() {
        if p < pattern.len() && pattern[p] == b'*' {
            let doubled = pattern.get(p + 1) == Some(&b'*');
            // A run of three or more stars is still just "crosses `/`"; consume it whole so
            // the resume point below lands after the entire run.
            let mut end = p;
            while end < pattern.len() && pattern[end] == b'*' {
                end += 1;
            }
            star = Some((end - 1, s, doubled));
            p = end;
            continue;
        }
        if p < pattern.len()
            && ((pattern[p] == b'?' && path[s] != SEPARATOR) || pattern[p] == path[s])
        {
            p += 1;
            s += 1;
            continue;
        }
        // The pattern is exhausted with path left over: the entry has matched a PREFIX,
        // which is a match. This must be checked BEFORE the backtracking below — a `*`
        // earlier in the pattern is irrelevant once there is nothing left to match, and
        // resuming from it here would loop over the rest of the path for no reason.
        if p >= pattern.len() {
            return true;
        }
        match star {
            // Extending the star consumes the character it currently sits on. A single star
            // has to stop at `/` — that is what keeps `/srv/*.php` out of subdirectories —
            // while a doubled one may swallow it.
            Some((star_p, star_s, crosses)) if crosses || path[star_s] != SEPARATOR => {
                p = star_p + 1;
                s = star_s + 1;
                star = Some((star_p, s, crosses));
            }
            _ => return false,
        }
    }
    // The path ran out. Any trailing `*` still matches the empty rest.
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p >= pattern.len()
}

/// Expands the directive value as a `glob()` over blacklist FILES and loads each match.
///
/// php-src calls `glob()` here, whose wildcards stay INSIDE one path component. Only the
/// final component is expanded: a wildcard in a DIRECTORY component is the one part of
/// `glob()` not reproduced, because it would mean walking the tree for a shape no real
/// configuration uses. Such a value finds no file and takes the warning path below, which
/// is the same outcome php-src reaches whenever a pattern matches nothing.
///
/// Files are read as BYTES and converted lossily, never through `read_to_string`: a single
/// non-UTF-8 byte used to drop the whole file — silently losing every valid line in it — and
/// then take the "no blacklist file found" path, which was a false statement because the
/// glob HAD matched. php-src reads bytes and keeps every line.
///
/// Each match is returned with its own directory, because a relative entry inside a blacklist
/// file resolves against THAT file's location rather than the process cwd.
fn expand_and_read(value: &str) -> Vec<(PathBuf, String)> {
    let mut contents = Vec::new();
    let path = Path::new(value);
    let Some(file_pattern) = path.file_name().and_then(|name| name.to_str()) else {
        return contents;
    };
    let read_lossy = |file: &Path| -> Option<(PathBuf, String)> {
        let bytes = std::fs::read(file).ok()?;
        let dir = file.parent().unwrap_or(Path::new(".")).to_path_buf();
        Some((dir, String::from_utf8_lossy(&bytes).into_owned()))
    };
    if !is_glob(file_pattern) {
        // A literal path: read it directly rather than listing its parent.
        if let Some(entry) = read_lossy(path) {
            contents.push(entry);
        }
        return contents;
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let dir = parent.unwrap_or_else(|| Path::new("."));
    let Ok(entries) = std::fs::read_dir(dir) else {
        return contents;
    };
    // Sorted so that a configuration whose files disagree loads them in a stable order,
    // and so the tests do not depend on directory iteration order.
    let mut matched: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            component_matches(file_pattern, &name).then(|| entry.path())
        })
        .collect();
    matched.sort();
    for file in matched {
        if let Some(entry) = read_lossy(&file) {
            contents.push(entry);
        }
    }
    contents
}

/// Whether a filename component carries any `glob()` metacharacter.
///
/// `[` counts: `glob()` honours character classes, and treating `bl_[ab].list` as a literal
/// filename made it match nothing at all — reference loads both `bl_a.list` and `bl_b.list`
/// (VERIFIED). A `[` with no closing `]` is not a class, and `glob()` then treats it
/// literally, which is why the scan below looks for the pair.
fn is_glob(component: &str) -> bool {
    if component.contains(['*', '?']) {
        return true;
    }
    match component.find('[') {
        Some(open) => component[open + 1..].contains(']'),
        None => false,
    }
}

/// Matches one `glob()` character class against `byte`, returning the index just past the
/// closing `]`. `[!abc]` and `[^abc]` negate; `a-z` is a range; `]` first is literal.
fn class_matches(pattern: &[u8], open: usize, byte: u8) -> Option<(bool, usize)> {
    let mut i = open + 1;
    let negated = matches!(pattern.get(i), Some(b'!') | Some(b'^'));
    if negated {
        i += 1;
    }
    let mut hit = false;
    let mut first = true;
    while i < pattern.len() {
        if pattern[i] == b']' && !first {
            return Some((hit != negated, i + 1));
        }
        first = false;
        // A range needs a `-` with a member on each side; a trailing `-` is literal.
        if i + 2 < pattern.len() && pattern[i + 1] == b'-' && pattern[i + 2] != b']' {
            if pattern[i] <= byte && byte <= pattern[i + 2] {
                hit = true;
            }
            i += 3;
            continue;
        }
        if pattern[i] == byte {
            hit = true;
        }
        i += 1;
    }
    None // unterminated: not a class after all
}

/// `glob()` matching for ONE path component: like `prefix_matches` it refuses to cross `/`
/// (there is none inside a component), but it must consume the WHOLE name rather than a
/// prefix, and it additionally honours `[...]` classes, which `glob()` has and the blacklist
/// entry matcher does not.
fn component_matches(pattern: &str, name: &str) -> bool {
    let (pat, nam) = (pattern.as_bytes(), name.as_bytes());
    let (mut p, mut n) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while n < nam.len() {
        if p < pat.len() {
            if pat[p] == b'*' {
                star = Some((p, n));
                p += 1;
                continue;
            }
            if pat[p] == b'[' {
                if let Some((hit, next)) = class_matches(pat, p, nam[n]) {
                    if hit {
                        p = next;
                        n += 1;
                        continue;
                    }
                    // A class that does not match is a hard miss for this position; fall
                    // through to the star backtracking below.
                    match star {
                        Some((star_p, star_n)) => {
                            p = star_p + 1;
                            n = star_n + 1;
                            star = Some((star_p, n));
                            continue;
                        }
                        None => return false,
                    }
                }
            }
            if pat[p] == b'?' || pat[p] == nam[n] {
                p += 1;
                n += 1;
                continue;
            }
        }
        match star {
            Some((star_p, star_n)) => {
                p = star_p + 1;
                n = star_n + 1;
                star = Some((star_p, n));
            }
            None => return false,
        }
    }
    while p < pat.len() && pat[p] == b'*' {
        p += 1;
    }
    p >= pat.len()
}

/// Resolves one blacklist entry the way php-src's `zend_accel_blacklist_loadone` does.
///
/// A surrounding pair of double quotes is stripped, then a relative entry is joined to
/// `base_dir` — the blacklist FILE's directory — and `.` / `..` are folded out. An absolute
/// entry is normalised but not relocated. Wildcards survive untouched: this is a textual
/// expansion, never a filesystem resolution, so an entry naming files that do not exist yet
/// still works.
fn expand_entry(entry: &str, base_dir: &Path) -> String {
    let unquoted = entry
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(entry);
    let joined = if unquoted.starts_with('/') {
        unquoted.to_string()
    } else {
        format!("{}/{}", base_dir.display(), unquoted)
    };
    let mut out: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    // A trailing separator is meaningful — `/srv/vendor/` blocks only what is beneath it —
    // so it is preserved rather than normalised away.
    let tail = if joined.ends_with('/') { "/" } else { "" };
    format!("/{}{}", out.join("/"), tail)
}

thread_local! {
    static BLACKLIST: RefCell<Blacklist> = RefCell::new(Blacklist::empty());

    /// The directive value the blacklist above was built from, so a repeated call with the
    /// same value is a no-op instead of re-reading every file. See `load`.
    static LOADED_FROM: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Loads `opcache.blacklist_filename` for the current thread.
///
/// An empty value is "unset" and loads nothing. A value matching no file logs php-src's
/// own warning — gated, like every accelerator diagnostic, by
/// `opcache.log_verbosity_level`, which must be at least 2 for a `Warning` to appear.
pub(crate) fn load(value: &str) {
    if value.is_empty() {
        return;
    }
    // LOAD ONCE. Generated code calls this from `ensure_eval_context`, whose guard is a
    // FUNCTION-LOCAL stack slot zeroed in every prologue — so five calls of a function
    // containing an `eval()` re-globbed and re-read every blacklist file five times, and
    // repeated the no-match warning five times (measured). Reference PHP reads the files
    // once at startup and never again. The bridge cannot assume anything about how often
    // generated code calls it, so "once" is enforced here rather than in codegen.
    let already = LOADED_FROM.with(|cell| cell.borrow().clone());
    if already.as_deref() == Some(value) {
        return;
    }
    LOADED_FROM.with(|cell| *cell.borrow_mut() = Some(value.to_string()));

    let files = expand_and_read(value);
    if files.is_empty() {
        accel_log(
            AccelLogLevel::Warning,
            &format!("No blacklist file found matching: {value}"),
        );
        return;
    }
    let mut blacklist = Blacklist::empty();
    for (dir, contents) in &files {
        blacklist.extend_from_file_contents(contents, dir);
    }
    BLACKLIST.with(|cell| *cell.borrow_mut() = blacklist);
}

/// Returns the loaded patterns, in the order php-src would list them.
///
/// This is what `opcache_get_configuration()['blacklist']` reports. Reference PHP lists the
/// RESOLVED entries — the lines of every file the directive's glob matched, unioned — not
/// the directive's own value, so the order across files is the sorted file order and the
/// order within a file is the file's.
pub(crate) fn patterns() -> Vec<String> {
    BLACKLIST.with(|cell| cell.borrow().patterns.clone())
}

/// Returns whether `path` must run without being cached.
///
/// Cheap on the overwhelmingly common empty blacklist: one `is_empty` on a thread-local.
pub(crate) fn blocks(path: &Path) -> bool {
    BLACKLIST.with(|cell| {
        let blacklist = cell.borrow();
        if blacklist.is_empty() {
            return false;
        }
        path.to_str().is_some_and(|path| blacklist.blocks(path))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a blacklist from file contents, as if the list lived in `/srv/lists`.
    fn of(lines: &str) -> Blacklist {
        let mut blacklist = Blacklist::empty();
        blacklist.extend_from_file_contents(lines, std::path::Path::new("/srv/lists"));
        blacklist
    }

    /// The plain case: a full path blocks exactly itself.
    #[test]
    fn an_exact_path_blocks_only_that_file() {
        let blacklist = of("/srv/app/blocked.php\n");
        assert!(blacklist.blocks("/srv/app/blocked.php"));
        assert!(!blacklist.blocks("/srv/app/allowed.php"));
    }

    /// php-src anchors its regexp at the start ONLY, so an entry is a PREFIX and a bare
    /// directory blocks everything under it. VERIFIED on reference PHP 8.5.10, where the
    /// entry `…/p_pref` refused `…/p_prefix.php`.
    #[test]
    fn an_entry_matches_as_a_prefix() {
        let blacklist = of("/srv/app/p_pref\n");
        assert!(blacklist.blocks("/srv/app/p_prefix.php"));
        assert!(blacklist.blocks("/srv/app/p_pref"));
        assert!(!blacklist.blocks("/srv/app/other.php"));

        let directory = of("/srv/vendor/\n");
        assert!(directory.blocks("/srv/vendor/anything/at/all.php"));
        assert!(!directory.blocks("/srv/app/main.php"));
    }

    /// NEITHER wildcard crosses `/`. VERIFIED on reference PHP 8.5.10 with
    /// `opcache.file_update_protection=0` and a main script that cannot match the pattern:
    /// `<dir>/*deep.php` refused `<dir>/xdeep.php` and CACHED `<dir>/sub/deep.php`, and
    /// `<dir>/sub?deep.php` refused neither.
    ///
    /// This is the assertion an earlier revision had backwards, on a probe confounded by
    /// `file_update_protection` — the subdirectory file was refused for its AGE, and the
    /// one blacklist miss belonged to the main script.
    #[test]
    fn no_wildcard_crosses_a_directory_separator() {
        let star = of("/srv/app/*deep.php\n");
        assert!(star.blocks("/srv/app/xdeep.php"));
        assert!(star.blocks("/srv/app/deep.php"));
        assert!(!star.blocks("/srv/app/sub/deep.php"));
        assert!(!star.blocks("/srv/app/a/b/c/deep.php"));

        let question = of("/srv/app/sub?deep.php\n");
        assert!(!question.blocks("/srv/app/sub/deep.php"));
        assert!(question.blocks("/srv/app/subXdeep.php"));
    }

    /// `?` is exactly one character, and does not stretch.
    #[test]
    fn a_question_mark_matches_exactly_one_character() {
        let blacklist = of("/srv/p_st?r.php\n");
        assert!(blacklist.blocks("/srv/p_star.php"));
        assert!(!blacklist.blocks("/srv/p_stiar.php"));
        assert!(!blacklist.blocks("/srv/p_str.php"));
    }

    /// Reference PHP refused `p_case.php` against the entry `P_CASE.PHP`, so matching is
    /// case-sensitive even where the filesystem is not.
    #[test]
    fn matching_is_case_sensitive() {
        let blacklist = of("/srv/P_CASE.PHP\n");
        assert!(!blacklist.blocks("/srv/p_case.php"));
        assert!(blacklist.blocks("/srv/P_CASE.PHP"));
    }

    /// `;` comments and blank lines never become patterns — and a `;` that is not the
    /// first character does NOT start one.
    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let blacklist = of("; /srv/commented.php\n\n   \n/srv/real.php\n");
        assert_eq!(blacklist.patterns, vec!["/srv/real.php".to_string()]);
        assert!(!blacklist.blocks("/srv/commented.php"));
        assert!(blacklist.blocks("/srv/real.php"));
    }

    /// Trailing whitespace and a CRLF line ending must not become part of the pattern,
    /// or a blacklist authored on Windows would silently match nothing.
    #[test]
    fn trailing_whitespace_and_crlf_are_trimmed() {
        let blacklist = of("/srv/app.php  \r\n");
        assert_eq!(blacklist.patterns, vec!["/srv/app.php".to_string()]);
        assert!(blacklist.blocks("/srv/app.php"));
    }

    /// An empty blacklist blocks nothing — the state almost every process is in.
    #[test]
    fn an_empty_blacklist_blocks_nothing() {
        assert!(!Blacklist::empty().blocks("/srv/anything.php"));
        assert!(Blacklist::empty().is_empty());
    }

    /// Backtracking: a `*` that consumed too much must give characters back so a later
    /// literal can still match — but it may only give back within ONE component, so
    /// `/srv/*/vendor/` reaches exactly one level down and no further.
    #[test]
    fn a_star_backtracks_within_one_component() {
        let blacklist = of("/srv/*/vendor/\n");
        assert!(blacklist.blocks("/srv/a/vendor/x.php"));
        assert!(blacklist.blocks("/srv/long-name/vendor/x.php"));
        assert!(!blacklist.blocks("/srv/a/b/vendor/x.php"));
        assert!(!blacklist.blocks("/srv/a/vendorish/x.php"));
    }

    /// Several `*` in one entry, the shape a real "exclude every cache directory" line
    /// has. Each star stays inside its own component, so the entry names exactly one
    /// directory depth.
    #[test]
    fn several_stars_in_one_entry() {
        let blacklist = of("/srv/*/cache/*.php\n");
        assert!(blacklist.blocks("/srv/app/cache/twig.php"));
        assert!(!blacklist.blocks("/srv/app/cache/twig.txt"));
        assert!(!blacklist.blocks("/srv/app/cache/deep/twig.php"));
    }

    /// A DOUBLED star crosses `/` where a single one does not: php-src compiles `*` to
    /// `[^/]*` but `**` to `.*`. VERIFIED on reference PHP 8.5.10 — `<dir>/**.php` refuses
    /// `<dir>/sub/nested.php` while `<dir>/*.php` caches it.
    ///
    /// Found by review. The differential corpus missed it because the only doubled star it
    /// carried was TRAILING, and a trailing `**` behaves the same under both readings.
    #[test]
    fn a_doubled_star_crosses_a_directory_separator() {
        let single = of("/srv/app/*.php\n");
        assert!(single.blocks("/srv/app/flat.php"));
        assert!(!single.blocks("/srv/app/sub/nested.php"));

        let doubled = of("/srv/app/**.php\n");
        assert!(doubled.blocks("/srv/app/flat.php"));
        assert!(doubled.blocks("/srv/app/sub/nested.php"));
        assert!(doubled.blocks("/srv/app/a/b/c/deep.php"));

        // A doubled star mid-pattern still has to honour the literals after it.
        let middle = of("/srv/**/vendor/\n");
        assert!(middle.blocks("/srv/a/b/c/vendor/x.php"));
        assert!(!middle.blocks("/srv/a/b/c/vendorish/x.php"));
    }

    /// An entry is EXPANDED, not stored verbatim: quotes stripped, a relative entry resolved
    /// against the blacklist FILE's directory, `.` and `..` folded out. VERIFIED on reference
    /// PHP 8.5.10, where a list containing only `flat.php` refuses `<dir>/flat.php` and the
    /// configuration reports the expanded path.
    #[test]
    fn entries_are_expanded_against_the_lists_own_directory() {
        let relative = of("flat.php\n");
        assert_eq!(relative.patterns, vec!["/srv/lists/flat.php".to_string()]);
        assert!(relative.blocks("/srv/lists/flat.php"));

        let quoted = of("\"/srv/app/blocked.php\"\n");
        assert_eq!(quoted.patterns, vec!["/srv/app/blocked.php".to_string()]);
        assert!(quoted.blocks("/srv/app/blocked.php"));

        let dotted = of("../app/./blocked.php\n");
        assert_eq!(dotted.patterns, vec!["/srv/app/blocked.php".to_string()]);
        assert!(dotted.blocks("/srv/app/blocked.php"));

        // A trailing separator survives: it is what makes a directory entry a prefix.
        let dir = of("vendor/\n");
        assert_eq!(dir.patterns, vec!["/srv/lists/vendor/".to_string()]);
        assert!(dir.blocks("/srv/lists/vendor/lib.php"));

        // An absolute entry keeps its own location, and wildcards survive expansion.
        let absolute = of("/srv/other/*.php\n");
        assert_eq!(absolute.patterns, vec!["/srv/other/*.php".to_string()]);
    }

    /// The directive value goes through `glob()`, which honours `[...]` classes.
    #[test]
    fn the_directive_glob_honours_character_classes() {
        assert!(component_matches("bl_[ab].list", "bl_a.list"));
        assert!(component_matches("bl_[ab].list", "bl_b.list"));
        assert!(!component_matches("bl_[ab].list", "bl_c.list"));
        assert!(component_matches("bl_[a-c].list", "bl_b.list"));
        assert!(!component_matches("bl_[a-c].list", "bl_d.list"));
        assert!(component_matches("bl_[!a].list", "bl_z.list"));
        assert!(!component_matches("bl_[!a].list", "bl_a.list"));
        assert!(component_matches("x[*]y", "x*y"));
        // An unterminated `[` is literal, and is therefore not a glob at all.
        assert!(!is_glob("bl_[ab.list"));
        assert!(is_glob("bl_[ab].list"));
        assert!(!is_glob("plain.list"));
    }

    /// Creates a fresh directory for one test's blacklist files.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "elephc-blacklist-{}-{}-{:?}",
            name,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// The directive value is a `glob()` over FILES, and php-src loads EVERY match and
    /// unions their entries. VERIFIED on reference PHP 8.5.10: with `bl_*.list` matching
    /// two files that named one script each, BOTH scripts were refused.
    #[test]
    fn the_directive_value_globs_and_unions_every_matching_file() {
        let dir = scratch("union");
        std::fs::write(dir.join("bl_a.list"), "/srv/one.php\n").unwrap();
        std::fs::write(dir.join("bl_b.list"), "/srv/two.php\n").unwrap();
        // Reset the once-only guard: these tests load several different values in a row.
        reset_for_tests();
        // Must NOT be picked up: the glob anchors the whole component.
        std::fs::write(dir.join("bl_c.list.bak"), "/srv/three.php\n").unwrap();

        load(&format!("{}/bl_*.list", dir.display()));

        assert!(blocks(std::path::Path::new("/srv/one.php")));
        assert!(blocks(std::path::Path::new("/srv/two.php")));
        assert!(!blocks(std::path::Path::new("/srv/three.php")));
    }

    /// A value with no wildcard is a literal path, read directly.
    #[test]
    fn a_literal_value_reads_that_one_file() {
        let dir = scratch("literal");
        let file = dir.join("blacklist.txt");
        std::fs::write(&file, "; a comment\n/srv/literal.php\n").unwrap();
        reset_for_tests();

        load(&file.to_string_lossy());

        assert!(blocks(std::path::Path::new("/srv/literal.php")));
        assert!(!blocks(std::path::Path::new("/srv/other.php")));
    }

    /// An empty directive is "unset": it loads nothing and leaves any already-loaded
    /// blacklist alone, rather than clearing it.
    #[test]
    fn an_empty_value_loads_nothing() {
        let dir = scratch("empty");
        std::fs::write(dir.join("b.list"), "/srv/kept.php\n").unwrap();
        reset_for_tests();
        load(&dir.join("b.list").to_string_lossy());
        assert!(blocks(std::path::Path::new("/srv/kept.php")));

        load("");

        assert!(
            blocks(std::path::Path::new("/srv/kept.php")),
            "an empty value must leave the loaded blacklist alone"
        );
    }

    /// Clears the once-only guard so one test thread can load several values in turn.
    fn reset_for_tests() {
        LOADED_FROM.with(|cell| *cell.borrow_mut() = None);
        BLACKLIST.with(|cell| *cell.borrow_mut() = Blacklist::empty());
    }

    /// `load` reads the files ONCE per value. Generated code calls it from
    /// `ensure_eval_context`, whose guard is a function-local stack slot zeroed in every
    /// prologue, so it arrives once per call of any function containing an `eval()` —
    /// measured at five reads for five calls before this guard existed.
    #[test]
    fn loading_the_same_value_twice_reads_the_files_once() {
        let dir = scratch("once");
        let file = dir.join("deny.list");
        std::fs::write(&file, "/srv/first.php\n").unwrap();
        reset_for_tests();
        load(&file.to_string_lossy());
        assert!(blocks(std::path::Path::new("/srv/first.php")));

        // Rewrite the file behind the loader's back, then ask for the SAME value again.
        std::fs::write(&file, "/srv/second.php\n").unwrap();
        load(&file.to_string_lossy());

        assert!(
            blocks(std::path::Path::new("/srv/first.php")),
            "a repeated load of the same value must not re-read the file"
        );
        assert!(!blocks(std::path::Path::new("/srv/second.php")));
    }

    /// A file that is not valid UTF-8 keeps its usable lines instead of being dropped whole,
    /// and does NOT take the "no blacklist file found" path — the glob did match a file, so
    /// that message would be false.
    #[test]
    fn a_non_utf8_file_keeps_its_usable_lines() {
        let dir = scratch("nonutf8");
        let file = dir.join("deny.list");
        // A stray 0xFF byte on its own line, with valid entries on both sides.
        let mut bytes = b"/srv/before.php\n".to_vec();
        bytes.extend_from_slice(&[0xFF, b'\n']);
        bytes.extend_from_slice(b"/srv/after.php\n");
        std::fs::write(&file, bytes).unwrap();
        reset_for_tests();

        load(&file.to_string_lossy());

        assert!(blocks(std::path::Path::new("/srv/before.php")));
        assert!(blocks(std::path::Path::new("/srv/after.php")));
    }

    /// A value matching no file blacklists nothing and is NOT fatal — reference PHP only
    /// logs `No blacklist file found matching: <value>`, and only at verbosity >= 2.
    #[test]
    fn a_value_matching_no_file_blacklists_nothing() {
        reset_for_tests();
        load("/nonexistent-elephc-dir/nope-*.list");
        assert!(!blocks(std::path::Path::new("/srv/anything.php")));
    }

    /// The directive value's own glob is a DIFFERENT matcher: it must consume the whole
    /// component, so `bl_*.list` names files and not merely prefixes of them.
    #[test]
    fn the_directive_glob_anchors_both_ends() {
        assert!(component_matches("bl_*.list", "bl_a.list"));
        assert!(component_matches("bl_*.list", "bl_.list"));
        assert!(!component_matches("bl_*.list", "bl_a.list.bak"));
        assert!(!component_matches("bl_*.list", "xbl_a.list"));
        assert!(component_matches("exact.list", "exact.list"));
        assert!(!component_matches("exact.list", "exact.list2"));
    }
}
