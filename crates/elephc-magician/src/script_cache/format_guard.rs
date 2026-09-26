//! Purpose:
//! Fails the test suite when the eval IR changes without
//! [`super::file_store::FORMAT_VERSION`] moving with it.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - WHY THIS IS NOT BUREAUCRACY. bincode is not self-describing. A payload written by an
//!   older eval IR can decode into a DIFFERENT but structurally plausible tree rather than
//!   failing — and that tree would then be executed as the user's program. The stored shape
//!   and the version that guards it must therefore move together, and "remember to bump it"
//!   is not a mechanism.
//! - The fingerprint is over the eval IR's SOURCE, so it also trips on a rename or a
//!   logic change that leaves the shape alone. That is the safe direction: an unnecessary
//!   bump costs one cold parse per cached script, while a missed one costs correctness.
//! - COMMENT AND BLANK LINES ARE NOT HASHED. An unnecessary bump is cheap only while the
//!   cache can refill: under `opcache.file_cache_read_only=1` the new version's directory
//!   stays empty, so every recycled `--web` worker re-parses every script. Version 7 was
//!   such a bump, forced by one doc comment. A trailing comment after code on the same
//!   line is still hashed; none of these files has a line comment inside a string literal,
//!   which is the one case where dropping a `//` line would drop code.
//! - Deliberately not a build script. The check belongs where a reviewer sees it fail, with
//!   a message that says what to do, rather than in a silent codegen step.

#[cfg(test)]
mod tests {
    use super::super::file_store::FORMAT_VERSION;

    /// The eval IR files whose shape the stored format mirrors.
    const IR_SOURCES: &[(&str, &str)] = &[
        ("attributes", include_str!("../eval_ir/attributes.rs")),
        ("callable", include_str!("../eval_ir/callable.rs")),
        ("classes", include_str!("../eval_ir/classes.rs")),
        ("enums", include_str!("../eval_ir/enums.rs")),
        ("expressions", include_str!("../eval_ir/expressions.rs")),
        ("interfaces", include_str!("../eval_ir/interfaces.rs")),
        ("methods", include_str!("../eval_ir/methods.rs")),
        ("program", include_str!("../eval_ir/program.rs")),
        ("properties", include_str!("../eval_ir/properties.rs")),
        ("statements", include_str!("../eval_ir/statements.rs")),
        ("traits", include_str!("../eval_ir/traits.rs")),
        ("segments", include_str!("segments.rs")),
        // `EvalParseError` is a `ScriptSegment` variant's payload, so it is part of the
        // stored shape whether or not an entry currently carries one. It was missing here,
        // which meant a change to the error type moved the format without moving the guard.
        ("errors", include_str!("../errors.rs")),
    ];

    /// The fingerprint recorded for `FORMAT_VERSION`. Update BOTH together, never one.
    ///
    /// RE-RECORDED WITHOUT A VERSION BUMP, once, and the reason is worth writing down so the
    /// next person does not read it as licence. The serialised shape did not move: no
    /// `eval_ir` file and no `segments.rs` changed. What moved is this guard's own inputs —
    /// `errors.rs` was added to `IR_SOURCES`, and the hash became a specified one. A bump
    /// would have invalidated every cache entry to record a change to the measuring
    /// instrument. Any fingerprint change that comes from an IR file still needs the bump.
    /// BUMPED TO 2 WITH THIS VALUE, and this one IS an IR change rather than a change to
    /// the measuring instrument. Rebasing this branch onto main replayed it over 884 commits,
    /// several of which moved `eval_ir`; the serialised shape those files describe is not the
    /// shape the branch recorded before the rebase. The guard caught it, which is what it is
    /// for, and the bump is what keeps a file written by a pre-rebase build from decoding
    /// into a plausible but wrong tree.
    /// BUMPED TO 4 WITH THIS VALUE: round 12 rewrote `segments.rs`'s close-tag search. The
    /// stored shape did not move, but this guard does not judge that, by design.
    /// BUMPED TO 5 WITH THIS VALUE: round 13 taught the same search to skip `{$...}`
    /// interpolation. Again no shape change — again the guard's rule.
    /// BUMPED TO 6 WITH THIS VALUE: that skip learned to treat a comment inside `{$...}` as inert.
    /// RE-RECORDED AT 6, the version 7 was reverted to: 7 existed only because a doc comment in
    /// `segments.rs` moved the fingerprint, and comment lines are no longer hashed. Every
    /// build that wrote a version-6 entry ran the same segmenter, so those entries are valid
    /// again rather than orphaned.
    const RECORDED_FINGERPRINT: u64 = 13163168241487621815;

    /// Returns a stable fingerprint of every source the stored format depends on.
    ///
    /// "Stable" has to mean across TOOLCHAINS, not just across runs. `DefaultHasher`'s
    /// algorithm is explicitly unspecified and may change between Rust releases, so the
    /// recorded constant below would have started failing on a toolchain bump — a guard that
    /// cries wolf is a guard people learn to re-record without reading.
    fn fingerprint() -> u64 {
        let mut bytes = Vec::new();
        for (name, source) in IR_SOURCES {
            bytes.extend_from_slice(name.as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(code_lines(source).as_bytes());
            bytes.push(0);
        }
        super::super::file_store::stable_hash(&bytes)
    }

    /// Returns `source` without its comment lines and blank lines, trailing whitespace trimmed.
    ///
    /// A line is dropped when it starts with `//` after its indentation, which covers `//`,
    /// `///` and `//!`. Block comments are kept: none of the fingerprinted files uses them.
    fn code_lines(source: &str) -> String {
        source
            .lines()
            .map(str::trim_end)
            .filter(|line| {
                let code = line.trim_start();
                !code.is_empty() && !code.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Verifies a comment-only edit leaves the hashed text alone while a code edit moves it.
    #[test]
    fn only_code_lines_reach_the_fingerprint() {
        let original = "pub enum A {\n    B,\n}\n";
        let documented = "//! Module.\n\n/// The enum.\npub enum A {\n    // why\n    B,   \n}\n";
        let renamed = "pub enum A {\n    C,\n}\n";

        assert_eq!(code_lines(original), code_lines(documented));
        assert_ne!(code_lines(original), code_lines(renamed));
        assert_eq!(
            code_lines("let s = b\"<?php // note\";\n"),
            "let s = b\"<?php // note\";",
            "a `//` after code on the same line is code, not a comment line",
        );
    }

    /// Verifies the stored format's version still matches the IR it describes.
    #[test]
    fn the_format_version_tracks_the_eval_ir() {
        let current = fingerprint();
        assert_eq!(
            current, RECORDED_FINGERPRINT,
            "\nThe eval IR or the segment shape changed, so scripts already written to an \
             `opcache.file_cache` directory no longer describe this build's types.\n\
             bincode is not self-describing: such a file can decode into a plausible but \
             WRONG tree, which would then be executed.\n\n\
             Do both, in `script_cache/file_store.rs` and here:\n  \
             1. FORMAT_VERSION {} -> {}\n  \
             2. RECORDED_FINGERPRINT {} -> {}\n",
            FORMAT_VERSION,
            FORMAT_VERSION + 1,
            RECORDED_FINGERPRINT,
            current,
        );
    }
}
