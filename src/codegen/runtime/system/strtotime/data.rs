//! Purpose:
//! Emits the pre-lowercased lookup tables consumed by the `__rt_strtotime` strategy parsers.
//! All entries share a fixed 12-byte stride (10 chars zero-padded + length byte + kind byte) plus a length=0 sentinel terminator.
//!
//! Called from:
//! - `crate::codegen::runtime::data::fixed::emit_fixed_data()` via `system::emit_strtotime_data`.
//!
//! Key details:
//! - Tables are read-only ASCII and are addressed by absolute symbol names (`_strtotime_keyword_tab`, ...).
//! - Names stored pre-lowercased so the matcher only lowercases the input once into the dispatcher's lc16 buffer.

/// Keyword table entries indexed by the strtotime lexer.
///
/// Each entry maps a lowercased keyword string to a kind code.
/// Kinds 0-5: bare keywords. Kinds 6-8: modifiers consumed by the weekday strategy.
/// Kind 9: `ago` (consumed by the offsets strategy as a trailing suffix).
/// Kinds 10-16: weekday names (10=Sun..16=Sat) — full and abbreviated forms share the same kind.
/// Kinds 17-18: "a"/"an" relative magnitudes consumed by the offsets strategy.
const KEYWORDS: &[(&str, u8)] = &[
    ("now", 0),
    ("today", 1),
    ("tomorrow", 2),
    ("yesterday", 3),
    ("midnight", 4),
    ("noon", 5),
    ("next", 6),
    ("last", 7),
    ("this", 8),
    ("ago", 9),
    ("a", 17),
    ("an", 18),
    ("sunday", 10),
    ("monday", 11),
    ("tuesday", 12),
    ("wednesday", 13),
    ("thursday", 14),
    ("friday", 15),
    ("saturday", 16),
    ("sun", 10),
    ("mon", 11),
    ("tue", 12),
    ("wed", 13),
    ("thu", 14),
    ("fri", 15),
    ("sat", 16),
];

/// Unit table entries indexed by the strtotime lexer.
///
/// Maps lowercased unit strings to accumulator indices: 0=sec, 1=min, 2=hour, 3=day, 4=week, 5=month, 6=year.
/// Plural forms share the same kind as their singular counterparts.
const UNITS: &[(&str, u8)] = &[
    ("seconds", 0),
    ("second", 0),
    ("secs", 0),
    ("sec", 0),
    ("minutes", 1),
    ("minute", 1),
    ("mins", 1),
    ("min", 1),
    ("hours", 2),
    ("hour", 2),
    ("days", 3),
    ("day", 3),
    ("weeks", 4),
    ("week", 4),
    ("months", 5),
    ("month", 5),
    ("years", 6),
    ("year", 6),
];

/// Emits NASM assembly directives for the strtotime keyword and unit lookup tables.
///
/// Writes two global symbols (`_strtotime_keyword_tab` and `_strtotime_unit_tab`) each followed
/// by their entries, then a 12-byte sentinel terminator (12 zero bytes). Entries are emitted by `entry()`.
/// Returns the complete assembly string.
pub(crate) fn emit_strtotime_data() -> String {
    let mut out = String::new();
    out.push_str(".globl _strtotime_keyword_tab\n_strtotime_keyword_tab:\n");
    for (name, kind) in KEYWORDS {
        out.push_str(&entry(name, *kind));
    }
    out.push_str("    .byte 0,0,0,0,0,0,0,0,0,0,0,0\n");

    out.push_str(".globl _strtotime_unit_tab\n_strtotime_unit_tab:\n");
    for (name, kind) in UNITS {
        out.push_str(&entry(name, *kind));
    }
    out.push_str("    .byte 0,0,0,0,0,0,0,0,0,0,0,0\n");
    out
}

/// Formats one fixed-stride table entry for the strtotime data section.
///
/// Each entry occupies exactly 12 bytes: up to 10 bytes for the name (zero-padded on the right),
/// followed by a length byte and a kind byte. The returned string contains three assembly directives
/// (`.ascii` for the name, `.byte` for the length, `.byte` for the kind) with a trailing newline.
///
/// # Panics
/// Panics if `name` exceeds 10 characters.
fn entry(name: &str, kind: u8) -> String {
    debug_assert!(name.len() <= 10, "strtotime table entry too long: {}", name);
    let mut padded = name.to_string();
    while padded.len() < 10 {
        padded.push('\0');
    }
    format!(
        "    .ascii \"{}\"\n    .byte {}\n    .byte {}\n",
        padded.replace('\0', "\\0"),
        name.len(),
        kind,
    )
}
