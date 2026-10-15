//! Purpose:
//! Emitter-level regressions for the `__rt_incref` input register used by the array
//! helpers that retain heap-backed elements.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `__rt_incref` reads its pointer from `rax` on linux-x86_64 (see
//!   `arrays/incref.rs`) and from `x0` on aarch64, never from the SysV argument
//!   register `rdi`. #1707 fixed the first helpers; these pin the rest listed by #1738.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Target;

use super::{
    emit_amr_box_value, emit_array_merge_recursive, emit_array_replace,
    emit_array_replace_recursive, emit_array_to_hash_reverse, emit_array_to_hash_unique,
    emit_assoc_diff_intersect,
};

/// The helpers whose retain sites #1738 covers, as `(name, emitter)` pairs.
fn retain_helpers() -> [(&'static str, fn(&mut Emitter)); 7] {
    [
        ("array_replace", emit_array_replace),
        ("array_replace_recursive", emit_array_replace_recursive),
        ("array_to_hash_reverse", emit_array_to_hash_reverse),
        ("array_to_hash_unique", emit_array_to_hash_unique),
        ("array_merge_recursive", emit_array_merge_recursive),
        ("amr_box_value", emit_amr_box_value),
        ("assoc_diff_intersect", emit_assoc_diff_intersect),
    ]
}

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

/// linux-x86_64 retains pass the element to `__rt_incref` in `rax`, never in the SysV
/// argument register `rdi`.
#[test]
fn test_array_helpers_retain_through_incref_input_register_on_x86_64() {
    let call = "call __rt_incref";
    for (name, emit) in retain_helpers() {
        let assembly = instructions("linux-x86_64", emit);
        let calls = assembly.iter().filter(|line| *line == call).count();
        assert!(calls >= 1, "{name}: expected at least one {call}");
        for pair in assembly.windows(2) {
            if pair[1] == call {
                assert!(
                    pair[0].starts_with("mov rax,"),
                    "{name}: {call} must take its pointer from rax, found: {}",
                    pair[0]
                );
            }
        }
    }
}

/// aarch64 retains pass the element in `x0`, matching `__rt_incref`'s AArch64 input
/// register.
#[test]
fn test_array_helpers_retain_through_x0_on_aarch64() {
    let call = "bl __rt_incref";
    for target in ["macos-aarch64", "linux-aarch64"] {
        for (name, emit) in retain_helpers() {
            let assembly = instructions(target, emit);
            let calls = assembly.iter().filter(|line| *line == call).count();
            assert!(calls >= 1, "{target} {name}: expected at least one {call}");
            for pair in assembly.windows(2) {
                if pair[1] == call {
                    assert!(
                        pair[0].starts_with("mov x0,") || pair[0].starts_with("ldr x0,"),
                        "{target} {name}: {call} must take its pointer from x0, found: {}",
                        pair[0]
                    );
                }
            }
        }
    }
}
