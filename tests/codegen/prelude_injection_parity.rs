//! Purpose:
//! Verifies the codegen test harness injects the same preludes `pipeline::compile` does.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The harness in `tests/codegen/support/compiler.rs` mirrors the pipeline's prelude sequence BY
//!   HAND, so the two are separate lists that must agree. When the `gz*` prelude was wired into
//!   the pipeline only, every test using it failed with "Undefined function: gzopen" — a failure
//!   that names the symptom and not the cause, repeated once per test.
//! - Both files are read as TEXT and their `<name>_prelude::inject...` calls compared as sets. A
//!   textual gate is the right shape here precisely because the duplication is textual: there is
//!   no shared value to assert on, only two sequences that happen to be written twice.
//! - ORDER is not compared, only membership. The pipeline interleaves other phases between
//!   injections and the harness does not, so requiring the same order would fail on a difference
//!   that does not matter.
//! - Two exclusions, both decisions rather than drift. `web_prelude` is `inject_if_web` and the
//!   harness compiles no web programs. `opcache_prelude` takes an opcache manifest, preload
//!   symbols and preload statistics that the harness has no source for — which does mean the
//!   functions it supplies are NOT covered by this suite, and that is a stated limitation rather
//!   than a hidden one.

use std::collections::BTreeSet;

/// Returns every `<name>_prelude` a source file injects.
fn injected_preludes(source: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for line in source.lines() {
        let Some(call) = line.find("_prelude::inject") else {
            continue;
        };
        let head = &line[..call];
        let name_start = head
            .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .map_or(0, |index| index + 1);
        let name = &head[name_start..];
        if name.is_empty() || name == "web" || name == "opcache" {
            continue;
        }
        found.insert(name.to_string());
    }
    found
}

/// Every file that hand-copies the pipeline's prelude sequence.
///
/// Both are test harnesses that compile PHP without going through `pipeline::compile`, so each is
/// a separate list that can fall behind on its own. They are checked separately so a failure names
/// which one.
const HARNESSES: &[&str] = &[
    "tests/codegen/support/compiler.rs",
    "src/ir_lower/tests/mod.rs",
];

/// Verifies every harness injects the same set of preludes the pipeline does.
#[test]
fn test_the_test_harnesses_inject_every_prelude_the_pipeline_does() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let pipeline = std::fs::read_to_string(repo.join("src/pipeline.rs"))
        .expect("pipeline source must be readable");
    let expected = injected_preludes(&pipeline);
    assert!(
        !expected.is_empty(),
        "the pipeline scan found no preludes at all — the gate is reading the wrong thing"
    );

    for harness in HARNESSES {
        let source = std::fs::read_to_string(repo.join(harness))
            .unwrap_or_else(|_| panic!("{harness} must be readable"));
        let actual = injected_preludes(&source);
        let missing: Vec<_> = expected.difference(&actual).collect();
        assert!(
            missing.is_empty(),
            "{harness} does not inject {missing:?}, which `pipeline::compile` does — programs \
             using them fail with \"Undefined function\" only inside that harness"
        );
    }
}
