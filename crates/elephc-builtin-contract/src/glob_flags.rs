//! Purpose:
//! php's own `GLOB_*` flag values, shared by the compiler and the eval interpreter.
//!
//! Called from:
//! - `elephc::types::stream_constants`, which declares them as php constants.
//! - `elephc::codegen_support::runtime::io::glob`, which translates them to libc bits.
//! - `elephc_magician::interpreter::builtins::filesystem::glob`, which applies them directly.
//!
//! Key details:
//! - These are NOT the bits of any libc. php 8.5 ships its own glob, so the values are identical
//!   on every target — measured across the three per-target oracle manifests, `GLOB_ONLYDIR`
//!   included, where glibc's own is `1 << 13` and php's is `1 << 30`. macOS's glob.h even defines
//!   php's `GLOB_NOESCAPE` value, 4096, as `GLOB_LIMIT`.
//! - Anything reaching the system `glob()` therefore has to be translated first; only the eval
//!   interpreter, which matches paths itself, can act on these numbers as they stand.

/// php's `GLOB_ERR`.
pub const GLOB_ERR: i64 = 4;
/// php's `GLOB_MARK`: append a slash to every directory in the result.
pub const GLOB_MARK: i64 = 8;
/// php's `GLOB_NOCHECK`: answer the pattern itself when it matched nothing.
pub const GLOB_NOCHECK: i64 = 16;
/// php's `GLOB_NOSORT`: leave the matches in the order the filesystem gave them.
pub const GLOB_NOSORT: i64 = 32;
/// php's `GLOB_BRACE`: expand `{a,b}` alternatives, csh style.
pub const GLOB_BRACE: i64 = 128;
/// php's `GLOB_NOESCAPE`: a backslash quotes nothing.
pub const GLOB_NOESCAPE: i64 = 4096;
/// php's `GLOB_ONLYDIR`: keep directories only. php's private bit, which no libc implements.
pub const GLOB_ONLYDIR: i64 = 1 << 30;
/// The OR of the seven above, which is what php validates `$flags` against.
pub const GLOB_AVAILABLE_FLAGS: i64 =
    GLOB_ERR | GLOB_MARK | GLOB_NOCHECK | GLOB_NOSORT | GLOB_BRACE | GLOB_NOESCAPE | GLOB_ONLYDIR;

/// Every php glob flag, paired with the name php publishes it under.
pub const GLOB_FLAGS: &[(&str, i64)] = &[
    ("GLOB_ERR", GLOB_ERR),
    ("GLOB_MARK", GLOB_MARK),
    ("GLOB_NOCHECK", GLOB_NOCHECK),
    ("GLOB_NOSORT", GLOB_NOSORT),
    ("GLOB_BRACE", GLOB_BRACE),
    ("GLOB_NOESCAPE", GLOB_NOESCAPE),
    ("GLOB_ONLYDIR", GLOB_ONLYDIR),
    ("GLOB_AVAILABLE_FLAGS", GLOB_AVAILABLE_FLAGS),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// The published set is exactly the OR the validator uses.
    ///
    /// php refuses `$flags` by testing against `GLOB_AVAILABLE_FLAGS`, so a flag added here
    /// without being added to that OR would be declared and then rejected.
    fn available_flags_covers_every_declared_flag() {
        let ored = GLOB_FLAGS
            .iter()
            .filter(|(name, _)| *name != "GLOB_AVAILABLE_FLAGS")
            .fold(0, |acc, (_, value)| acc | value);
        assert_eq!(ored, GLOB_AVAILABLE_FLAGS);
    }
}
