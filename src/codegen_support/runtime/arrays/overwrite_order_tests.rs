//! Purpose:
//! Emitter-level regressions for the order in which an element overwrite stores the
//! replacement and releases the value it replaces.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Releasing a refcounted element can run a destructor, and that destructor can read, grow
//!   or replace the container being written. PHP stores the new element first, so the helpers
//!   must publish the replacement before the release call and leave the container alone after.
//! - Every supported target is checked, because the helpers are emitted per architecture.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Target;

use super::{emit_array_set_mixed, emit_array_set_refcounted, emit_hash_set};

/// Every target the supported matrix lists.
const TARGETS: [&str; 5] =
    ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"];

/// Emits `helper` for `target` and returns its instructions with comments stripped.
fn instructions(target: &str, emit: fn(&mut Emitter)) -> Vec<String> {
    let target = Target::parse(target).unwrap();
    let mut emitter = Emitter::new(target);
    emit(&mut emitter);
    emitter
        .output()
        .lines()
        .map(|line| line.split("//").next().unwrap().trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Returns the instructions from `label` up to the next jump or return that leaves the path.
fn path_from<'a>(assembly: &'a [String], label: &str) -> &'a [String] {
    let start = assembly
        .iter()
        .position(|line| line.ends_with(&format!("{label}:")))
        .unwrap_or_else(|| panic!("missing label {label}"));
    let length = assembly[start..]
        .iter()
        .position(|line| line.starts_with("b ") || line.starts_with("jmp ") || line == "ret")
        .unwrap_or_else(|| panic!("{label}: path never leaves"));
    &assembly[start..=start + length]
}

/// Asserts that `store` appears before `release` on `path`, and that nothing after `release`
/// writes memory.
fn assert_store_precedes_release(context: &str, path: &[String], store: &str, release: &str) {
    let stored = path
        .iter()
        .position(|line| line == store)
        .unwrap_or_else(|| panic!("{context}: missing store `{store}` in {path:?}"));
    let released = path
        .iter()
        .position(|line| line.ends_with(release))
        .unwrap_or_else(|| panic!("{context}: missing release `{release}` in {path:?}"));
    assert!(stored < released, "{context}: the replacement must be stored before the release");
    for line in &path[released + 1..] {
        assert!(
            !line.starts_with("str ") && !line.starts_with("mov QWORD PTR"),
            "{context}: the container is written after the release: {line}"
        );
    }
}

/// `__rt_hash_set` publishes an overwritten entry's replacement before releasing the old value.
#[test]
fn test_hash_set_overwrite_stores_before_release_on_all_targets() {
    for target in TARGETS {
        let assembly = instructions(target, emit_hash_set);
        let x86 = target == "linux-x86_64";
        let store = if x86 { "mov QWORD PTR [r12 + 40], r13" } else { "str x13, [x12, #40]" };
        for (label, release) in [
            ("__rt_hash_set_release_any", "__rt_decref_any"),
            ("__rt_hash_set_release_callable", "__rt_callable_descriptor_release"),
        ] {
            let label = if x86 { format!("{label}_x") } else { label.to_string() };
            let path = path_from(&assembly, &label);
            assert_store_precedes_release(&format!("{target} {label}"), path, store, release);
        }
    }
}

/// The indexed helpers publish an in-bounds overwrite before releasing the old element.
#[test]
fn test_array_set_overwrite_stores_before_release_on_all_targets() {
    for target in TARGETS {
        let x86 = target == "linux-x86_64";
        let refcounted = instructions(target, emit_array_set_refcounted);
        let path = path_from(&refcounted, "__rt_array_set_refcounted_store");
        let store = if x86 { "mov QWORD PTR [rax + 24 + rsi * 8], rdx" } else { "str x2, [x10, x1, lsl #3]" };
        assert_store_precedes_release(&format!("{target} refcounted"), path, store, "__rt_decref_any");

        let mixed = instructions(target, emit_array_set_mixed);
        let path = path_from(&mixed, "__rt_array_set_mixed_grow_ready");
        let store = if x86 { "mov QWORD PTR [r10 + 24 + r9 * 8], r8" } else { "str x13, [x12, x9, lsl #3]" };
        assert_store_precedes_release(&format!("{target} mixed"), path, store, "__rt_decref_mixed");
    }
}
