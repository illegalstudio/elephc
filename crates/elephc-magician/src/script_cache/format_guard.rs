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
//! - The fingerprint is over the eval IR's SOURCE, so it also trips on a comment or a
//!   rename. That is the safe direction: an unnecessary bump costs one cold parse per
//!   cached script, while a missed one costs correctness.
//! - Deliberately not a build script. The check belongs where a reviewer sees it fail, with
//!   a message that says what to do, rather than in a silent codegen step.

#[cfg(test)]
mod tests {
    use super::super::file_store::FORMAT_VERSION;
    use std::hash::{Hash, Hasher};

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
    ];

    /// The fingerprint recorded for `FORMAT_VERSION`. Update BOTH together, never one.
    const RECORDED_FINGERPRINT: u64 = 15678573635801945442;

    /// Returns a stable fingerprint of every source the stored format depends on.
    fn fingerprint() -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for (name, source) in IR_SOURCES {
            name.hash(&mut hasher);
            source.hash(&mut hasher);
        }
        hasher.finish()
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
