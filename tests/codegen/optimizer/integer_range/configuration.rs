//! Purpose:
//! Verifies integer specialization under alternate null representations and runtime settings.
//!
//! Called from:
//! - The integer-range optimizer integration-test module.
//!
//! Key details:
//! - A boxed integer may contain the legacy null sentinel as an ordinary payload.
//! - Representation flags must preserve optimizer-on/off behavior independently.

use super::*;

/// Scalar sink folding preserves the selected representation's existing null-predicate behavior.
#[test]
fn test_integer_range_scalar_sentinel_null_predicate_matches_both_modes() {
    let source = "<?php int $value = ($argc & 0) + (PHP_INT_MAX - 1); var_dump(is_null($value));";
    for null_repr in ["--null-repr=sentinel", "--null-repr=tagged"] {
        let plain = run_variant_with_options(source, false, &[null_repr]);
        assert_eq!(plain.0, if null_repr == "--null-repr=sentinel" {
            "bool(true)\n"
        } else { "bool(false)\n" });
        assert_eq!(plain, run_variant_with_options(source, true, &[null_repr]));
    }
}

/// Legacy sentinel storage must not reinterpret an ordinary boxed integer after narrowing.
#[test]
fn test_integer_range_null_representation_preserves_boxed_sentinel() {
    for expression in [
        "(($argc & 1) + (PHP_INT_MAX - 2))",
        "(($argc & 0) + (PHP_INT_MAX - 1))",
    ] {
        let source = format!("<?php var_dump(is_null({expression})); echo {expression}, \"|\";");
        for null_repr in ["--null-repr=sentinel", "--null-repr=tagged"] {
            let plain = run_variant_with_options(&source, false, &[null_repr]);
            assert_eq!(plain.0, "bool(false)\n9223372036854775806|");
            assert!(plain.1.is_empty(), "{}", plain.1);
            assert_eq!(plain, run_variant_with_options(&source, true, &[null_repr]),
                "{null_repr}: {expression}");
        }
    }
}

/// All supported emitters retain the boxed collision path only in legacy sentinel mode.
#[test]
fn test_integer_range_all_targets_preserve_sentinel_collision() {
    let source = r#"<?php
#[Export]
function sentinel_probe(int $input): void {
    echo ($input & 1) + (PHP_INT_MAX - 2);
    echo ($input & 0) + (PHP_INT_MAX - 1);
}
#[Export]
function scalar_sentinel_probe(int $input): bool {
    int $value = ($input & 0) + (PHP_INT_MAX - 1);
    return is_null($value);
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for null_repr in ["--null-repr=sentinel", "--null-repr=tagged"] {
            for optimized in [false, true] {
                let assembly = target_assembly_with_options(source, target, optimized, &[null_repr]);
                assert_eq!(assembly.contains("op=ichecked_add"),
                    !optimized || null_repr == "--null-repr=sentinel",
                    "{target}, {null_repr}, optimized={optimized}");
                assert_eq!(assembly.contains("op=is_null"),
                    !optimized || null_repr == "--null-repr=sentinel",
                    "{target}, {null_repr}, optimized={optimized}");
            }
        }
    }
}

/// Reboxed arithmetic survives reference aliases and array ownership with heap validation enabled.
#[test]
fn test_integer_range_reference_ownership_is_heap_clean_in_both_modes() {
    let source = r#"<?php
function collect_values(int $seed): void {
    $value = 0;
    $alias =& $value;
    $items = [];
    for ($i = 0; $i < 32; $i++) {
        $value = ($seed & 255) + $i;
        $items[] = $alias;
    }
    echo count($items), "|", $items[0], "|", $items[31];
}
collect_values($argc);
"#;
    for null_repr in ["--null-repr=sentinel", "--null-repr=tagged"] {
        for optimized in [false, true] {
            let output = run_variant_with_options(&source, optimized, &[null_repr, "--heap-debug"]);
            assert_eq!(output.0, "32|1|32");
            assert!(output.1.contains("HEAP DEBUG: leak summary: clean"), "{}", output.1);
        }
    }
}
