//! Purpose:
//! Regression tests for associative-array slot indexing: the power-of-two mask
//! fast path and the arbitrary-capacity division fallback shared by
//! `__rt_hash_get` and `__rt_hash_set`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness via `tests/codegen/arrays/mod.rs`.
//!
//! Key details:
//! - The assembly assertions generate the runtime text for both supported ABIs,
//!   so the x86_64 bodies are checked even on an aarch64 host, where the runtime
//!   cache cannot assemble a foreign-target object.
//! - The PHP fixture grows a literal-built hash (non-power-of-two capacities) and
//!   an empty-built hash (power-of-two capacities) through the same probe code.

use crate::support::*;

/// Returns the runtime assembly slice between two labels, including the start
/// label and excluding the end label.
fn asm_between_labels<'a>(asm: &'a str, start_label: &str, end_label: &str) -> &'a str {
    let start = asm
        .find(start_label)
        .unwrap_or_else(|| panic!("missing start label {start_label}"));
    let tail = &asm[start..];
    let end = tail
        .find(end_label)
        .unwrap_or_else(|| panic!("missing end label {end_label} after {start_label}"));
    &tail[..end]
}

/// Verifies both targets derive the initial `__rt_hash_get` probe slot with a
/// mask when the capacity is a power of two, and fall back to division
/// otherwise.
#[test]
fn test_hash_get_slot_index_masks_power_of_two_capacities() {
    let aarch64 = elephc::codegen::generate_runtime_with_features(
        8_388_608,
        elephc::codegen::platform::Target::parse("linux-aarch64").expect("valid target"),
        elephc::codegen::RuntimeFeatures::none(),
    );
    let arm_initial = asm_between_labels(&aarch64, "__rt_hash_get:", "__rt_hash_get_slot_div:");
    for needle in ["sub x9, x6, #1", "tst x6, x9", "and x8, x0, x9"] {
        assert!(
            arm_initial.contains(needle),
            "aarch64 hash_get initial slot must mask powers of two, missing `{needle}`:\n{arm_initial}"
        );
    }
    let arm_div = asm_between_labels(
        &aarch64,
        "__rt_hash_get_slot_div:",
        "__rt_hash_get_slot_ready:",
    );
    assert!(
        arm_div.contains("udiv x7, x0, x6") && arm_div.contains("msub x8, x7, x6, x0"),
        "aarch64 hash_get must keep the division fallback:\n{arm_div}"
    );
    let arm_wrap = asm_between_labels(
        &aarch64,
        "__rt_hash_get_wrap_div:",
        "__rt_hash_get_wrap_ready:",
    );
    assert!(
        arm_wrap.contains("udiv x7, x9, x6") && arm_wrap.contains("msub x9, x7, x6, x9"),
        "aarch64 hash_get must keep the probe-wrap division fallback:\n{arm_wrap}"
    );

    let x86_64 = elephc::codegen::generate_runtime_with_features(
        8_388_608,
        elephc::codegen::platform::Target::parse("linux-x86_64").expect("valid target"),
        elephc::codegen::RuntimeFeatures::none(),
    );
    let x86_initial = asm_between_labels(&x86_64, "__rt_hash_get:", "__rt_hash_get_slot_div_x:");
    for needle in ["lea rdx, [r11 - 1]", "test r11, rdx", "and rax, rdx"] {
        assert!(
            x86_initial.contains(needle),
            "x86_64 hash_get initial slot must mask powers of two, missing `{needle}`:\n{x86_initial}"
        );
    }
    let x86_div = asm_between_labels(
        &x86_64,
        "__rt_hash_get_slot_div_x:",
        "__rt_hash_get_slot_ready_x:",
    );
    assert!(
        x86_div.contains("div r11"),
        "x86_64 hash_get must keep the division fallback:\n{x86_div}"
    );
}

/// Verifies both targets mask the `__rt_hash_set` probe slot for power-of-two
/// capacities, including the aarch64 probe wraparound, and keep the division
/// fallback for arbitrary capacities.
#[test]
fn test_hash_set_slot_index_masks_power_of_two_capacities() {
    let aarch64 = elephc::codegen::generate_runtime_with_features(
        8_388_608,
        elephc::codegen::platform::Target::parse("linux-aarch64").expect("valid target"),
        elephc::codegen::RuntimeFeatures::none(),
    );
    let arm_mask = asm_between_labels(&aarch64, "__rt_hash_set_no_grow:", "__rt_hash_set_slot_div:");
    for needle in ["sub x9, x6, #1", "tst x6, x9", "and x8, x0, x9"] {
        assert!(
            arm_mask.contains(needle),
            "aarch64 hash_set initial slot must mask powers of two, missing `{needle}`:\n{arm_mask}"
        );
    }
    let arm_initial = asm_between_labels(&aarch64, "__rt_hash_set_slot_div:", "__rt_hash_set_slot_ready:");
    assert!(
        arm_initial.contains("udiv x7, x0, x6") && arm_initial.contains("msub x8, x7, x6, x0"),
        "aarch64 hash_set must keep the division fallback:\n{arm_initial}"
    );
    let arm_wrap = asm_between_labels(
        &aarch64,
        "__rt_hash_set_wrap_div:",
        "__rt_hash_set_wrap_ready:",
    );
    assert!(
        arm_wrap.contains("udiv x7, x9, x6") && arm_wrap.contains("msub x9, x7, x6, x9"),
        "aarch64 hash_set must keep the probe-wrap division fallback:\n{arm_wrap}"
    );

    let x86_64 = elephc::codegen::generate_runtime_with_features(
        8_388_608,
        elephc::codegen::platform::Target::parse("linux-x86_64").expect("valid target"),
        elephc::codegen::RuntimeFeatures::none(),
    );
    let x86_initial = asm_between_labels(
        &x86_64,
        "__rt_hash_set_no_grow_x:",
        "__rt_hash_set_slot_div_x:",
    );
    for needle in ["lea rdx, [r11 - 1]", "test r11, rdx", "and rax, rdx"] {
        assert!(
            x86_initial.contains(needle),
            "x86_64 hash_set initial slot must mask powers of two, missing `{needle}`:\n{x86_initial}"
        );
    }
    let x86_div = asm_between_labels(
        &x86_64,
        "__rt_hash_set_slot_div_x:",
        "__rt_hash_set_slot_ready_x:",
    );
    assert!(
        x86_div.contains("div r11"),
        "x86_64 hash_set must keep the division fallback:\n{x86_div}"
    );
}

/// Exercises updates through a literal-built hash whose capacity starts at a
/// non-power-of-two and is doubled on growth, and through an empty-built hash
/// whose capacities stay powers of two.
#[test]
fn test_assoc_hash_slot_index_paths_keep_php_values() {
    let source = r#"<?php
$literal = ['alpha' => 0, 'beta' => 0, 'gamma' => 0];
$empty = [];
for ($i = 0; $i < 40; $i++) {
    $key = 'k' . $i;
    $empty[$key] = $i;
}
$keys = ['alpha', 'beta', 'gamma'];
for ($i = 0; $i < 30; $i++) {
    $key = $keys[$i % 3];
    $literal[$key] += 1;
}
$literal['delta'] = 4;
$literal['epsilon'] = 5;
$literal['zeta'] = 6;
$sum = 0;
foreach ($literal as $value) {
    $sum += $value;
}
foreach ($empty as $value) {
    $sum += $value;
}
echo $sum;
"#;
    assert_eq!(compile_and_run(source), "825");
}
