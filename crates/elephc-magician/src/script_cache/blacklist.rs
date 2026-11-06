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

    /// Returns whether the directive blocks nothing, so callers can skip the match entirely.
    ///
    /// True both for a binary compiled without `opcache.blacklist_filename` and for one whose
    /// listed files existed but held no usable line — a blacklist of only comments and blanks
    /// is indistinguishable from no blacklist, which is also php-src's answer.
    pub(crate) fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Adds every usable line of one blacklist file's contents.
    ///
    /// Each line is handled in `zend_accel_blacklist_loadone`'s order, and every step is
    /// MEASURED on reference PHP 8.5.10:
    ///
    /// 1. ONE trailing `\n` is removed, then ONE `\r` before it — and nothing else. Trailing
    ///    spaces and tabs STAY in the entry (`"app.php  "` is reported as such, and blocks
    ///    nothing); `app.php\r\r\n` keeps a `\r`; a last line with no `\n` keeps its `\r`.
    /// 2. Leading `\r`s are stripped. Leading spaces are not: `  app.php` is a RELATIVE entry.
    /// 3. One surrounding pair of double quotes is stripped; a lone `"` is dropped.
    /// 4. An empty line is skipped, and so is one whose FIRST byte is now `;` — so a quoted
    ///   `";…"` is a comment, while ` ; x` (a space first) is an entry. A spaces-only line is
    ///    an entry too: `<dir>/   `.
    ///
    /// EVERY SURVIVING LINE IS EXPANDED, not stored verbatim. php-src strips a surrounding
    /// pair of double quotes, then resolves the entry against `base_dir` — the directory of
    /// the blacklist file itself, NOT the process cwd — and normalises `.` and `..`. This is
    /// what makes a list of bare filenames beside the list work at all, and it is also what
    /// `opcache_get_configuration()['blacklist']` reports. VERIFIED against reference PHP
    /// 8.5.10: a list containing only `flat.php` refuses `<dir>/flat.php` and reports the
    /// expanded path; a quoted entry behaves as the unquoted one.
    pub(crate) fn extend_from_file_contents(&mut self, contents: &str, base_dir: &Path) {
        for piece in contents.split_inclusive('\n') {
            let line = match piece.strip_suffix('\n') {
                Some(line) => line.strip_suffix('\r').unwrap_or(line),
                None => piece,
            };
            let line = line.trim_start_matches('\r');
            // php-src strips the quotes FIRST and then drops the line if nothing is left
            // (`path_length -= 2; if (path_length <= 0) continue;`). Without this, a line of
            // two quotes expanded to the blacklist file's own directory — a prefix that
            // refuses everything beneath it. VERIFIED: reference blocks nothing and reports
            // an empty list for such a line, or for a lone `"`.
            let entry = if line.starts_with('"') && line.ends_with('"') {
                match line.len() {
                    0..=2 => continue,
                    len => &line[1..len - 1],
                }
            } else {
                line
            };
            if entry.is_empty() || entry.starts_with(';') {
                continue;
            }
            let Some(pattern) = expand_entry(entry, base_dir) else {
                continue;
            };
            self.patterns.push(pattern);
        }
    }

    /// Returns whether `path` is blacklisted, and must therefore run without being cached.
    pub(crate) fn blocks(&self, path: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| prefix_matches(pattern, path))
    }
}

/// One compiled piece of a blacklist entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    /// A literal byte, matched case-sensitively.
    Byte(u8),
    /// `?` — exactly one byte, never a separator.
    Single,
    /// A run of stars. `crosses` is true when the run was `**` or longer, which php-src
    /// compiles to `.*`; a lone `*` becomes `[^/]*` and stops at a separator.
    Star { crosses: bool },
}

/// Compiles an entry into tokens, collapsing each run of stars into one.
fn compile_pattern(pattern: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::with_capacity(pattern.len());
    let mut i = 0;
    while i < pattern.len() {
        match pattern[i] {
            b'*' => {
                let start = i;
                while i < pattern.len() && pattern[i] == b'*' {
                    i += 1;
                }
                tokens.push(Token::Star {
                    crosses: i - start > 1,
                });
            }
            b'?' => {
                tokens.push(Token::Single);
                i += 1;
            }
            byte => {
                tokens.push(Token::Byte(byte));
                i += 1;
            }
        }
    }
    tokens
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
/// caches it.
///
/// WHY A DYNAMIC PROGRAM AND NOT A BACKTRACKING WALK. The usual glob trick remembers the
/// most recent star and resumes there, which is correct only while every star is
/// interchangeable. Once `*` and `**` differ, a single star blocked at a separator has to
/// fall back to an EARLIER doubled star, and a one-star memory cannot: `/srv/**/*.php` then
/// missed `/srv/a/b/c.php`, which reference refuses. Evaluating each (token, position) pair
/// once explores every split by construction, and keeps the cost linear where a naive
/// recursive fix would be exponential in the number of stars.
///
/// `row[s]` answers "do the tokens from here on match the path from `s` on", filled from the
/// end of the pattern backwards, with `s` descending so a star can read its own row at
/// `s + 1`.
fn prefix_matches(pattern: &str, path: &str) -> bool {
    let tokens = compile_pattern(pattern.as_bytes());
    let path = path.as_bytes();

    // The pattern is exhausted: a prefix has matched, whatever is left of the path.
    let mut row = vec![true; path.len() + 1];

    for token in tokens.iter().rev() {
        let next = row;
        let mut current = vec![false; path.len() + 1];
        for s in (0..=path.len()).rev() {
            current[s] = match *token {
                Token::Byte(byte) => s < path.len() && path[s] == byte && next[s + 1],
                Token::Single => {
                    s < path.len() && path[s] != SEPARATOR && next[s + 1]
                }
                // Match nothing here, or consume one more byte and stay on this token.
                Token::Star { crosses } => {
                    next[s]
                        || (s < path.len()
                            && (crosses || path[s] != SEPARATOR)
                            && current[s + 1])
                }
            };
        }
        row = current;
    }
    row[0]
}

/// Expands the directive value as a `glob()` over blacklist FILES and loads each match.
///
/// php-src calls `glob()` here, whose wildcards stay INSIDE one path component — but may sit in
/// ANY component. Only the final one used to be expanded, on the reasoning that a wildcard in a
/// directory "is a shape no real configuration uses"; one list per team or per app directory is
/// exactly that shape. Such a value loaded nothing, and the scripts it listed stayed cacheable.
/// MEASURED with `lists/*/bl.txt`: reference loads the entry and leaves the script uncached;
/// elephc loaded nothing and cached it.
///
/// Files are read as BYTES and converted lossily, never through `read_to_string`: a single
/// non-UTF-8 byte used to drop the whole file — silently losing every valid line in it — and
/// then take the "no blacklist file found" path, which was a false statement because the
/// glob HAD matched. php-src reads bytes and keeps every line.
///
/// Each match is returned with its own directory, because a relative entry inside a blacklist
/// file resolves against THAT file's location rather than the process cwd.
fn expand_and_read(value: &str) -> Vec<(PathBuf, String)> {
    let read_lossy = |file: &Path| -> Option<(PathBuf, String)> {
        let bytes = std::fs::read(file).ok()?;
        // The file's REAL directory. A relative entry resolves against it, and the paths it
        // will be matched against are canonical, so a symlinked list location would otherwise
        // produce entries that can never match.
        let dir = std::fs::canonicalize(file)
            .ok()
            .and_then(|real| real.parent().map(Path::to_path_buf))
            .or_else(|| file.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        Some((dir, String::from_utf8_lossy(&bytes).into_owned()))
    };
    glob_paths(Path::new(value))
        .iter()
        .filter_map(|file| read_lossy(file))
        .collect()
}

/// `glob()` over `pattern`, expanding a wildcard in ANY component, and returning the matches
/// sorted as `glob()` sorts them.
///
/// Walks one component at a time: a literal component is appended to every candidate, and a
/// wildcard one lists each candidate directory and keeps the entries `component_matches`
/// accepts. Every intermediate match must be a directory; the final ones may be anything, and
/// reading them is what filters out whatever is not a file. A pattern with no wildcard at all
/// yields its one literal path without listing any directory.
///
/// A backslash quotes the next byte, in EVERY component: php-src globs the directive with
/// `php_glob(..., 0, ...)`, and without `GLOB_NOESCAPE` that is `glob()`'s default. So a
/// literal component is appended UNESCAPED, which is why there is no "no wildcard, return the
/// value as is" shortcut any more: `deny\.list` named a file that does not exist, the read
/// failed, and every listed script stayed cacheable. MEASURED on reference PHP 8.5.10 —
/// `deny\.list` and `de\ny.list` both load `deny.list`, and `d\*.list` names the literal
/// `d*.list`, which loads nothing.
fn glob_paths(pattern: &Path) -> Vec<PathBuf> {
    let components: Vec<_> = pattern.components().collect();
    let mut candidates = vec![PathBuf::new()];
    let last = components.len().saturating_sub(1);
    for (index, component) in components.iter().enumerate() {
        let std::path::Component::Normal(name) = component else {
            for candidate in &mut candidates {
                candidate.push(component.as_os_str());
            }
            continue;
        };
        let Some(name) = name.to_str() else {
            for candidate in &mut candidates {
                candidate.push(name);
            }
            continue;
        };
        if !is_glob(name) {
            let literal = unescaped(name);
            for candidate in &mut candidates {
                candidate.push(&literal);
            }
            continue;
        }
        let mut next = Vec::new();
        for candidate in &candidates {
            let dir = if candidate.as_os_str().is_empty() {
                Path::new(".")
            } else {
                candidate.as_path()
            };
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Some(entry_name) = entry.file_name().to_str().map(str::to_string) else {
                    continue;
                };
                if !component_matches(name, &entry_name) {
                    continue;
                }
                if index != last && !entry.path().is_dir() {
                    continue;
                }
                next.push(candidate.join(&entry_name));
            }
        }
        candidates = next;
    }
    // Sorted so that a configuration whose files disagree loads them in a stable order,
    // and so the tests do not depend on directory iteration order.
    candidates.sort();
    candidates
}

/// One byte of a `glob()` pattern component, and whether a backslash quoted it.
///
/// A quoted byte is always literal: `\*` and `\?` match themselves, `\[` opens no class,
/// `\]` closes none, and a quoted `!` or `-` inside a class is a plain member. MEASURED on
/// reference PHP 8.5.10: `k\[ab].list` and `k[ab\].list` both load the file literally named
/// `k[ab].list`, and `x[\!]y.list` loads `x!y.list`. A trailing backslash quotes nothing and
/// stays a backslash, as `glob()` leaves it.
#[derive(Clone, Copy)]
struct PatternByte {
    byte: u8,
    quoted: bool,
}

impl PatternByte {
    /// Whether this byte is the UNQUOTED metacharacter `meta`.
    fn is(self, meta: u8) -> bool {
        !self.quoted && self.byte == meta
    }
}

/// Splits `component` into pattern bytes, consuming each quoting backslash.
fn pattern_bytes(component: &str) -> Vec<PatternByte> {
    let bytes = component.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let quoted = bytes[i] == b'\\' && i + 1 < bytes.len();
        if quoted {
            i += 1;
        }
        out.push(PatternByte {
            byte: bytes[i],
            quoted,
        });
        i += 1;
    }
    out
}

/// The file name a wildcard-free `component` names: its bytes with the quoting removed.
/// Only ASCII backslashes are dropped, so what remains is still UTF-8.
fn unescaped(component: &str) -> String {
    let bytes: Vec<u8> = pattern_bytes(component).iter().map(|b| b.byte).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Whether a filename component carries any `glob()` metacharacter.
///
/// `[` counts: `glob()` honours character classes, and treating `bl_[ab].list` as a literal
/// filename made it match nothing at all — reference loads both `bl_a.list` and `bl_b.list`
/// (VERIFIED). A `[` with no closing `]` is not a class, and `glob()` then treats it
/// literally, which is why the scan below looks for the pair.
fn is_glob(component: &str) -> bool {
    let pattern = pattern_bytes(component);
    if pattern.iter().any(|b| b.is(b'*') || b.is(b'?')) {
        return true;
    }
    // The FIRST opener decides: any later `[` that has a closer after it, the first one has
    // too.
    match pattern.iter().position(|b| b.is(b'[')) {
        Some(open) => pattern[open + 1..].iter().any(|b| b.is(b']')),
        None => false,
    }
}

/// One member of a `glob()` bracket class.
#[derive(Clone, Copy)]
enum ClassMember {
    Byte(u8),
    Range(u8, u8),
    Named(fn(u8) -> bool),
}

/// What an unquoted `[` opens, as `php_glob`'s `glob0` parses it.
enum Bracket {
    /// No closing `]`: the `[` is an ordinary byte.
    Literal,
    /// A `[:name:]` naming no known class. php_glob then answers `GLOB_NOMATCH` for the WHOLE
    /// pattern: `bl_[[:bogus:]].list` loads nothing, even beside `bl_[.list` (MEASURED).
    UnknownClass,
    /// A class; `next` is the index just past its closing `]`.
    Class {
        negated: bool,
        members: Vec<ClassMember>,
        next: usize,
    },
}

/// Matches the `[:blank:]` character class: C's `isblank` in the "C" locale.
fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

/// Matches the `[:print:]` character class: C's `isprint` in the "C" locale.
fn is_print(byte: u8) -> bool {
    byte.is_ascii_graphic() || byte == b' '
}

/// Matches the `[:space:]` character class: C's `isspace` in the "C" locale.
fn is_space(byte: u8) -> bool {
    // C's `isspace`, which unlike `u8::is_ascii_whitespace` includes the vertical tab.
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// php_glob's `cclasses` table, tested with the C locale's ctype — ASCII only.
const NAMED_CLASSES: [(&str, fn(u8) -> bool); 12] = [
    ("alnum", |b| b.is_ascii_alphanumeric()),
    ("alpha", |b| b.is_ascii_alphabetic()),
    ("blank", is_blank),
    ("cntrl", |b| b.is_ascii_control()),
    ("digit", |b| b.is_ascii_digit()),
    ("graph", |b| b.is_ascii_graphic()),
    ("lower", |b| b.is_ascii_lowercase()),
    ("print", is_print),
    ("punct", |b| b.is_ascii_punctuation()),
    ("space", is_space),
    ("upper", |b| b.is_ascii_uppercase()),
    ("xdigit", |b| b.is_ascii_hexdigit()),
];

/// `g_charclass`: `pattern[colon]` is the `:` right after a class's inner `[`.
///
/// `None` when there is no `:]` to end a name — the inner `[` is then an ordinary member
/// (`[[:alpha]` is the class `{[, :, a, l, p, h}`, MEASURED). `Some(Err(()))` for a name the
/// table lacks, `Some(Ok(..))` for a known one with the index past its `:]`.
fn named_class(pattern: &[PatternByte], colon: usize) -> Option<Result<(fn(u8) -> bool, usize), ()>> {
    let start = colon + 1;
    let end = (start..pattern.len()).find(|&i| pattern[i].is(b':'))?;
    if !pattern.get(end + 1).is_some_and(|b| b.is(b']')) {
        return None;
    }
    let name = &pattern[start..end];
    Some(
        NAMED_CLASSES
            .iter()
            .find(|(known, _)| {
                name.iter().all(|b| !b.quoted)
                    && known.as_bytes().iter().copied().eq(name.iter().map(|b| b.byte))
            })
            .map(|(_, test)| (*test, end + 2))
            .ok_or(()),
    )
}

/// Parses the bracket expression an unquoted `pattern[open] == '['` starts.
///
/// Mirrors `glob0`'s `LBRACKET` case. Negation is `!` ONLY: php bundles its own `php_glob`
/// (system glob is off by default), whose `LBRACKET` case tests `c == NOT` with
/// `#define NOT '!'` — a leading `^` is an ordinary member. MEASURED: `bl_[^a].list` loads
/// `bl_a.list` and `bl_[^b].list` loads nothing, so `[^i]nclude.list` matches `include.list`.
/// The first member is taken whatever it is, so `]` first is literal; `a-z` is a range unless
/// `]` follows the `-`; and `[:alpha:]`-style named classes (MEASURED: `[[:alpha:]]` matches a
/// letter, `[[:digit:]]` does not, `[![:digit:]]` matches `a` and `[`) are members too.
fn parse_bracket(pattern: &[PatternByte], open: usize) -> Bracket {
    let mut i = open + 1;
    let negated = pattern.get(i).is_some_and(|b| b.is(b'!'));
    if negated {
        i += 1;
    }
    if i >= pattern.len() || !pattern[i + 1..].iter().any(|b| b.is(b']')) {
        return Bracket::Literal;
    }
    let starts_named = |i: usize, c: PatternByte| {
        c.is(b'[') && pattern.get(i).is_some_and(|b| b.is(b':'))
    };
    let mut members = Vec::new();
    let mut c = pattern[i];
    i += 1;
    loop {
        if starts_named(i, c) {
            loop {
                match named_class(pattern, i) {
                    Some(Err(())) => return Bracket::UnknownClass,
                    // Not a name after all: this `[` is an ordinary member, below.
                    None => break,
                    Some(Ok((test, next))) => {
                        members.push(ClassMember::Named(test));
                        i = next;
                    }
                }
                // A named class can consume the `]` the opener's check found; php_glob then
                // reads past the pattern's end. Treat that as no class at all.
                let Some(&following) = pattern.get(i) else {
                    return Bracket::Literal;
                };
                c = following;
                i += 1;
                if !starts_named(i, c) {
                    break;
                }
            }
            if c.is(b']') {
                return Bracket::Class {
                    negated,
                    members,
                    next: i,
                };
            }
        }
        if pattern.get(i).is_some_and(|b| b.is(b'-'))
            && pattern.get(i + 1).is_some_and(|b| !b.is(b']'))
        {
            members.push(ClassMember::Range(c.byte, pattern[i + 1].byte));
            i += 2;
        } else {
            members.push(ClassMember::Byte(c.byte));
        }
        let Some(&following) = pattern.get(i) else {
            return Bracket::Literal;
        };
        c = following;
        i += 1;
        if c.is(b']') {
            return Bracket::Class {
                negated,
                members,
                next: i,
            };
        }
    }
}

/// Matches one `glob()` bracket class against `byte`, returning whether it matched and the
/// index just past the closing `]`; `None` when the `[` opens no class and is literal.
///
/// A class naming an unknown `[:name:]` never matches, and every component of the pattern
/// must match for a file to be loaded — so the pattern loads nothing, which is php_glob's
/// `GLOB_NOMATCH` for it.
fn class_matches(pattern: &[PatternByte], open: usize, byte: u8) -> Option<(bool, usize)> {
    match parse_bracket(pattern, open) {
        Bracket::Literal => None,
        Bracket::UnknownClass => Some((false, pattern.len())),
        Bracket::Class {
            negated,
            members,
            next,
        } => {
            let hit = members.iter().any(|member| match *member {
                ClassMember::Byte(member) => member == byte,
                ClassMember::Range(low, high) => low <= byte && byte <= high,
                ClassMember::Named(test) => test(byte),
            });
            Some((hit != negated, next))
        }
    }
}

/// `glob()` matching for ONE path component: like `prefix_matches` it refuses to cross `/`
/// (there is none inside a component), but it must consume the WHOLE name rather than a
/// prefix, and it additionally honours `[...]` classes, which `glob()` has and the blacklist
/// entry matcher does not.
fn component_matches(pattern: &str, name: &str) -> bool {
    let (pat, nam) = (pattern_bytes(pattern), name.as_bytes());
    // POSIX `glob()` hides dotfiles: a leading `.` is matched only by a LITERAL `.` in the
    // pattern, never by `*`, `?` or a class. VERIFIED on reference PHP 8.5.10 —
    // `opcache.blacklist_filename=*.list` loaded `deny.list` and ignored `.secret.list`.
    // Without this an editor's backup or a hidden file beside the real list would be read as
    // a blacklist and silently change what the cache stores. A QUOTED leading `.` is literal
    // too: `\.s*.list` loads `.secret.list` (MEASURED).
    if nam.first() == Some(&b'.') && pat.first().map(|b| b.byte) != Some(b'.') {
        return false;
    }
    let (mut p, mut n) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while n < nam.len() {
        if p < pat.len() {
            if pat[p].is(b'*') {
                star = Some((p, n));
                p += 1;
                continue;
            }
            if pat[p].is(b'[') {
                if let Some((hit, next)) = class_matches(&pat, p, nam[n]) {
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
            if pat[p].is(b'?') || pat[p].byte == nam[n] {
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
    while p < pat.len() && pat[p].is(b'*') {
        p += 1;
    }
    p >= pat.len()
}

/// Resolves one blacklist entry the way php-src's `zend_accel_blacklist_loadone` does.
///
/// The caller has already stripped the line ending and the quotes. A relative entry is joined
/// to `base_dir` — the blacklist FILE's directory — and `.` / `..` are folded out. An absolute
/// entry is normalised but not relocated. Wildcards survive untouched: this is a textual
/// expansion, never a filesystem resolution, so an entry naming files that do not exist yet
/// still works.
fn expand_entry(unquoted: &str, base_dir: &Path) -> Option<String> {
    if unquoted.is_empty() {
        return None;
    }
    let joined = if unquoted.starts_with('/') {
        unquoted.to_string()
    } else if base_dir.is_absolute() {
        format!("{}/{}", base_dir.display(), unquoted)
    } else {
        // The blacklist file was named by a RELATIVE directive value, so its own directory is
        // relative too — `""` for a bare filename. php-src's `expand_filepath` resolves that
        // against the process cwd; without this the entry came out anchored at `/` instead,
        // and a relative entry then matched nothing at all.
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let base = cwd.join(base_dir);
        format!("{}/{}", base.display(), unquoted)
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
    // so it is preserved rather than normalised away. The one exception is an entry that
    // resolves to the ROOT itself: `/` has no components left, and pasting the tail back
    // would yield `//`, which matches nothing — so the most sweeping entry a list can carry
    // would have silently blocked nothing at all.
    if out.is_empty() {
        return Some("/".to_string());
    }
    let tail = if joined.ends_with('/') { "/" } else { "" };
    Some(canonicalize_existing_prefix(&format!(
        "/{}{}",
        out.join("/"),
        tail
    )))
}

/// Resolves symlinks in the longest leading part of `path` that exists on disk.
///
/// php-src runs each blacklist entry through `expand_filepath_ex` in `CWD_FILEPATH` mode,
/// which realpaths it, and then matches the result against the script's `opened_path` — also
/// realpathed. elephc matches against the canonical cache key, so an entry that keeps its
/// symlinks can never match one.
///
/// This is not an edge case: `current -> releases/42` is how nearly every PHP deployment is
/// laid out, and a blacklist written against `/srv/app/current/...` was SILENTLY INERT there
/// — it blocked nothing, with no diagnostic, which for a directive whose whole job is to keep
/// files out is the worst possible failure.
///
/// Two limits are deliberate. Resolution stops at the first wildcard, because `/srv/*/tmp`
/// names no single directory to resolve; and it backs off one component at a time until a
/// prefix exists, because an entry may legitimately name a file that is not there yet. What
/// does not exist cannot be a symlink, so leaving that part textual loses nothing.
fn canonicalize_existing_prefix(path: &str) -> String {
    // Start at the wildcard (or the end), so the LAST component is offered to `canonicalize`
    // too. Seeding at the final separator instead resolved every directory above the leaf and
    // left the leaf's own symlink in place — which for an entry naming a file is the whole
    // entry. The cache key it is matched against is fully resolved, so the two could never
    // agree: blacklisting `lib/alias.php` (a symlink to `real.php`) blocked nothing, and
    // reference reports `…/real.php` back from `opcache_get_configuration()['blacklist']`.
    //
    // The bug hid behind the case that was tested. A directory prefix WITH a trailing slash
    // resolves correctly either way, because the trailing `/` already pushes the seed past
    // the component — so the `current -> releases/42` deployment probe passed while the
    // narrower and more common "the listed file is itself a symlink" case did not.
    let wildcard_at = path.find(['*', '?']).unwrap_or(path.len());
    let mut boundary = wildcard_at;
    loop {
        if boundary > 1 {
            let head = &path[..boundary];
            if let Ok(real) = std::fs::canonicalize(head) {
                let mut out = real.to_string_lossy().into_owned();
                if out.len() > 1 {
                    while out.ends_with('/') {
                        out.pop();
                    }
                }
                let rest = &path[boundary..];
                // Put back the separator canonicalization ate. `/srv/app/` resolves to
                // `/srv/app`, so splitting at the wildcard in `/srv/app/*.php` would rejoin
                // as `/srv/app*.php` — an entry that matches a sibling directory and not the
                // files it was written for.
                if head.ends_with('/') && !rest.starts_with('/') && !out.ends_with('/') {
                    out.push('/');
                }
                return format!("{out}{rest}");
            }
        }
        boundary = match path[..boundary].rfind('/') {
            Some(0) | None => return path.to_string(),
            Some(index) => index,
        };
    }
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

/// Returns one loaded pattern by index, cloning only that pattern.
///
/// The `opcache_get_configuration()['blacklist']` loop asks once per entry, so handing back
/// the whole list each time was quadratic in the number of entries.
pub(crate) fn pattern_at(index: usize) -> Option<String> {
    BLACKLIST.with(|cell| cell.borrow().patterns.get(index).cloned())
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

    /// `;` comments and EMPTY lines never become patterns — and a `;` that is not the first
    /// byte does NOT start one. A spaces-only line is not empty: it is the relative entry
    /// `<dir>/   `. MEASURED on reference PHP 8.5.10, which reports exactly these entries.
    #[test]
    fn comments_and_empty_lines_are_skipped() {
        let blacklist = of("; /srv/commented.php\n\n   \n ; x\n/srv/real.php\n");
        assert_eq!(
            blacklist.patterns,
            vec![
                "/srv/lists/   ".to_string(),
                "/srv/lists/ ; x".to_string(),
                "/srv/real.php".to_string(),
            ]
        );
        assert!(!blacklist.blocks("/srv/commented.php"));
        assert!(blacklist.blocks("/srv/real.php"));
    }

    /// Only the line ending is removed — ONE `\n`, then ONE `\r` — and trailing spaces and
    /// tabs stay in the entry, which then blocks nothing. A CRLF file still works.
    ///
    /// This used to trim every trailing `\r`, space and tab. MEASURED on reference PHP 8.5.10:
    /// `app.php  \r\n` reports `app.php  `, `app.php\t` keeps the tab, `app.php\r\r\n` keeps
    /// one `\r`, and a final line with no `\n` keeps its `\r`.
    #[test]
    fn only_the_line_ending_is_removed() {
        let blacklist = of("/srv/app.php  \r\n/srv/tab.php\t\n/srv/crlf.php\r\n/srv/two.php\r\r\n/srv/last.php\r");
        assert_eq!(
            blacklist.patterns,
            vec![
                "/srv/app.php  ".to_string(),
                "/srv/tab.php\t".to_string(),
                "/srv/crlf.php".to_string(),
                "/srv/two.php\r".to_string(),
                "/srv/last.php\r".to_string(),
            ]
        );
        assert!(!blacklist.blocks("/srv/app.php"));
        assert!(blacklist.blocks("/srv/crlf.php"));
    }

    /// Leading `\r`s are stripped before the comment test, quotes are stripped BEFORE it too,
    /// and a lone `"` is dropped. MEASURED: `\r;x` and `";<path>"` are comments; `"` alone
    /// reports nothing; a `\r` inside the line stays.
    #[test]
    fn quotes_and_carriage_returns_follow_php_srcs_order() {
        let blacklist = of("\r\r/srv/lead.php\n\r;x\n\";/srv/quoted_comment.php\"\n\"\n\"\r\n/srv/t.p\rhp\n");
        assert_eq!(
            blacklist.patterns,
            vec!["/srv/lead.php".to_string(), "/srv/t.p\rhp".to_string()]
        );
        assert!(!blacklist.blocks("/srv/quoted_comment.php"));
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

    /// A single `*` blocked at a separator must fall BACK to an earlier `**`, not give up.
    ///
    /// VERIFIED on reference PHP 8.5.10: `<dir>/**/*.php` refuses `<dir>/a/b/c.php`. php-src
    /// compiles it to `<dir>/.*/[^/]*\.php`, and the greedy `.*` backtracks so the `[^/]*`
    /// lands on the last component.
    #[test]
    fn a_blocked_single_star_falls_back_to_an_earlier_double_star() {
        let mixed = of("/srv/**/*.php\n");
        assert!(mixed.blocks("/srv/a/b/c.php"));
        assert!(mixed.blocks("/srv/a/c.php"));
        assert!(mixed.blocks("/srv/a/b/c/d/e.php"));
        assert!(!mixed.blocks("/srv/a/b/c.txt"));
    }

    /// A line that is empty once its quotes come off is DISCARDED, not expanded.
    ///
    /// php-src strips the quotes and then drops the line. Expanding it instead produced the
    /// blacklist file's own directory — a prefix refusing everything beneath it, which for a
    /// list living beside the code it names is the whole application. VERIFIED: reference
    /// blocks nothing and reports an empty list for a line of two quotes.
    #[test]
    fn an_entry_that_is_empty_once_unquoted_is_dropped() {
        let empty = of("\"\"\n");
        assert!(empty.patterns.is_empty(), "{:?}", empty.patterns);
        assert!(!empty.blocks("/srv/lists/anything.php"));

        // A real entry on another line still survives.
        let mixed = of("\"\"\n/srv/real.php\n");
        assert_eq!(mixed.patterns, vec!["/srv/real.php".to_string()]);
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

        // An entry that resolves to the ROOT stays `/` rather than becoming `//`, which would
        // match nothing — and `/` is the most sweeping entry a list can carry.
        let root = of("/\n");
        assert_eq!(root.patterns, vec!["/".to_string()]);
        assert!(root.blocks("/srv/anything.php"));
        let climbing = of("../..\n");
        assert_eq!(climbing.patterns, vec!["/".to_string()]);

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
        // `^` is an ordinary MEMBER in php's bundled glob, never a negation. VERIFIED:
        // `bl_[^a].list` loads `bl_a.list`, which POSIX semantics would have excluded.
        assert!(component_matches("bl_[^a].list", "bl_a.list"));
        assert!(component_matches("bl_[^a].list", "bl_^.list"));
        assert!(!component_matches("bl_[^a].list", "bl_z.list"));
        assert!(component_matches("x[*]y", "x*y"));
        // An unterminated `[` is literal, and is therefore not a glob at all.
        assert!(!is_glob("bl_[ab.list"));
        assert!(is_glob("bl_[ab].list"));
        assert!(!is_glob("plain.list"));
    }

    /// Verifies `[:name:]` classes inside a bracket, as php_glob parses them.
    ///
    /// Each row is a MEASURED reference PHP 8.5.10 outcome: `[[:alpha:]]` matches a letter and
    /// `[[:digit:]]` does not; `[![:digit:]]` matches `a` and `[`; a name with no `:]`
    /// (`[[:alpha]`) leaves its `[` an ordinary member; and an UNKNOWN name makes the whole
    /// pattern match nothing.
    #[test]
    fn the_directive_glob_knows_named_classes() {
        assert!(component_matches("[[:alpha:]]nclude.list", "include.list"));
        assert!(component_matches("bl_[[:alpha:]].list", "bl_a.list"));
        assert!(!component_matches("bl_[[:digit:]].list", "bl_a.list"));
        assert!(component_matches("bl_[[:digit:]].list", "bl_7.list"));
        assert!(component_matches("bl_[![:digit:]].list", "bl_a.list"));
        assert!(component_matches("bl_[![:digit:]].list", "bl_[.list"));
        assert!(component_matches("bl_[[:digit:]a].list", "bl_a.list"));
        assert!(component_matches("bl_[[:lower:]].list", "bl_a.list"));
        assert!(!component_matches("bl_[[:upper:]].list", "bl_a.list"));
        assert!(component_matches("bl_[[:alpha]].list", "bl_a].list"));
        assert!(component_matches("bl_[[:alpha].list", "bl_[.list"));
        assert!(!component_matches("bl_[[:bogus:]].list", "bl_a.list"));
        assert!(!component_matches("bl_[[:bogus:]].list", "bl_[.list"));
        // Not even the name a literal `[` would have spelled.
        assert!(!component_matches("bl_[[:bogus:]].list", "bl_[b].list"));
        // A quoted `[` opens nothing, so `[:bogus:]` is an ordinary class of its bytes.
        assert!(component_matches("bl_\\[[:bogus:]].list", "bl_[b].list"));
        // A QUOTED byte in or around the name is never a class name's: MEASURED, reference
        // loads nothing for any of these beside `bl_a.list`.
        assert!(!component_matches("bl_[[:al\\pha:]].list", "bl_a.list"));
        assert!(!component_matches("bl_[[:alpha\\:]].list", "bl_a.list"));
        assert!(!component_matches("bl_[[\\:alpha:]].list", "bl_a.list"));
        // `^` stays an ordinary member, named classes or not.
        assert!(component_matches("[^i]nclude.list", "include.list"));
        assert!(!component_matches("bl_[^b].list", "bl_a.list"));
    }

    /// Verifies a backslash quotes the next byte, so a quoted metacharacter is literal.
    ///
    /// Each row is a MEASURED reference PHP 8.5.10 outcome: `k\[ab].list` and `k[ab\].list`
    /// name `k[ab].list` literally, `x[\!]y.list` matches `x!y.list`, and a quoted leading `.`
    /// still reaches a dotfile.
    #[test]
    fn a_backslash_makes_the_next_glob_byte_literal() {
        assert!(!is_glob("d\\*.list"));
        assert!(!is_glob("k\\[ab].list"));
        assert!(!is_glob("k[ab\\].list"));
        assert!(is_glob("d*\\.list"));
        assert_eq!(unescaped("deny\\.list"), "deny.list");
        assert_eq!(unescaped("de\\ny.list"), "deny.list");
        assert_eq!(unescaped("k\\[ab].list"), "k[ab].list");
        // A trailing backslash quotes nothing and stays.
        assert_eq!(unescaped("deny.list\\"), "deny.list\\");
        assert!(component_matches("d*\\.list", "deny.list"));
        assert!(component_matches("x[\\!]y.list", "x!y.list"));
        assert!(!component_matches("x[\\!]y.list", "xzy.list"));
        assert!(component_matches("x\\*y*", "x*yz"));
        assert!(!component_matches("x\\*y*", "xay"));
        assert!(component_matches("x\\?*", "x?z"));
        assert!(!component_matches("x\\?*", "xaz"));
        assert!(component_matches("\\.s*.list", ".secret.list"));
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

    /// Verifies an unknown class name anywhere in the directive makes it load nothing, even
    /// where another reading would have matched a file. MEASURED: with `bl_a.list`,
    /// `bl_[.list` and `bl_[b].list` all present, `bl_[[:bogus:]].list` loads none of them,
    /// while the quoted `bl_\[[:bogus:]].list` loads `bl_[b].list`.
    #[test]
    fn an_unknown_class_name_loads_nothing() {
        let dir = scratch("bogus_class");
        std::fs::write(dir.join("bl_a.list"), "/srv/bogus_a.php\n").unwrap();
        std::fs::write(dir.join("bl_[.list"), "/srv/bogus_bracket.php\n").unwrap();
        std::fs::write(dir.join("bl_[b].list"), "/srv/bogus_literal.php\n").unwrap();
        reset_for_tests();
        load(&format!("{}/bl_[[:bogus:]].list", dir.display()));
        assert!(!blocks(std::path::Path::new("/srv/bogus_a.php")));
        assert!(!blocks(std::path::Path::new("/srv/bogus_bracket.php")));
        assert!(!blocks(std::path::Path::new("/srv/bogus_literal.php")));

        reset_for_tests();
        load(&format!("{}/bl_\\[[:bogus:]].list", dir.display()));
        assert!(blocks(std::path::Path::new("/srv/bogus_literal.php")));

        reset_for_tests();
        load(&format!("{}/bl_[![:digit:]].list", dir.display()));
        assert!(blocks(std::path::Path::new("/srv/bogus_a.php")));
        assert!(blocks(std::path::Path::new("/srv/bogus_bracket.php")));
    }

    /// Verifies an escaped directive value with NO wildcard is unescaped before it is read.
    ///
    /// It used to be read verbatim, the read failed, and the scripts it listed stayed
    /// cacheable. MEASURED: reference loads `deny.list` for `deny\.list`, and nothing for
    /// `d\*.list`, which names a file that does not exist.
    #[test]
    fn an_escaped_directive_value_names_the_unescaped_file() {
        let dir = scratch("escaped");
        std::fs::write(dir.join("deny.list"), "/srv/escaped.php\n").unwrap();
        reset_for_tests();
        load(&format!("{}/deny\\.list", dir.display()));
        assert!(blocks(std::path::Path::new("/srv/escaped.php")));

        reset_for_tests();
        load(&format!("{}/d\\*.list", dir.display()));
        assert!(!blocks(std::path::Path::new("/srv/escaped.php")));
    }

    /// A wildcard in a DIRECTORY component is expanded too, as `glob()` does.
    ///
    /// Only the final component used to be expanded, so `lists/*/bl.txt` loaded nothing and the
    /// scripts it listed stayed cacheable. MEASURED: reference loads the entry and leaves the
    /// script uncached. The dotfile directory must stay hidden, as `glob()` hides it.
    #[test]
    fn a_wildcard_directory_component_is_expanded() {
        let dir = scratch("dir_wildcard");
        for (team, script) in [("team-a", "/srv/a.php"), ("team-b", "/srv/b.php"), (".hidden", "/srv/h.php")] {
            std::fs::create_dir_all(dir.join(team)).unwrap();
            std::fs::write(dir.join(team).join("bl.txt"), format!("{script}\n")).unwrap();
        }
        reset_for_tests();

        load(&format!("{}/*/bl.txt", dir.display()));

        assert!(blocks(std::path::Path::new("/srv/a.php")), "team-a's list is loaded");
        assert!(blocks(std::path::Path::new("/srv/b.php")), "and team-b's");
        assert!(
            !blocks(std::path::Path::new("/srv/h.php")),
            "a `*` does not match a leading dot, in a directory any more than in a file name"
        );
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
        // POSIX glob hides dotfiles from every wildcard, and only a literal `.` reveals them.
        assert!(!component_matches("*.list", ".secret.list"));
        assert!(!component_matches("?secret.list", ".secret.list"));
        assert!(!component_matches("[.]secret.list", ".secret.list"));
        assert!(component_matches(".secret.list", ".secret.list"));
        assert!(component_matches(".*.list", ".secret.list"));
        assert!(!component_matches("exact.list", "exact.list2"));
    }
}
