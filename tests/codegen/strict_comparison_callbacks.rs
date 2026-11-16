//! Purpose:
//! Executes original-kind checks for static, descriptor and string comparator callbacks.
//!
//! Called from:
//! - Focused strict comparison codegen tests before public admission.
//!
//! Key details:
//! - Sorts and comparator-based set operations share checked return behavior.
//! - Rejected heap results retire through the ordinary catchable exception boundary.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Instance and static comparator failures retire boxed scalar and string method arguments.
#[test]
fn test_strict_comparison_method_argument_owners_unwind() {
    let source = include_str!("../fixtures/strict/comparator_method_owners.php");
    for strict in [false, true] {
        for mode in ["--ir-opt=off", "--ir-opt=on"] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-comparisons"); }
            let output = StrictFixture::new(source).capture(&flags);
            assert_eq!(String::from_utf8_lossy(&output.stdout), if strict { "MSABD" } else { "PPPPD" },
                "strict={strict} {mode}: {}", String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "strict={strict} {mode}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Raw sort payloads become owned mixed callback arguments before normal or throwing invocation.
#[test]
fn test_strict_comparison_scalar_callback_parameters_remain_boxed() {
    let source = r#"<?php
function boxedArgumentOrder(mixed $left, mixed $right): mixed { return $left <=> $right; }
function boxedArgumentInvalid(mixed $left, mixed $right): mixed { return true; }
function boxedArgumentThrow(mixed $left, mixed $right): int { throw new TypeError('stop'); }
$items = [2, 1]; usort($items, boxedArgumentOrder(...)); echo implode(',', $items), '|';
$sort = usort(...); $items = [2, 1]; $sort($items, boxedArgumentOrder(...));
echo implode(',', $items), '|';
$words = ['b', 'a']; $sort($words, boxedArgumentOrder(...)); echo implode(',', $words), '|';
try { $items = [2, 1]; $sort($items, boxedArgumentInvalid(...)); echo 'P'; }
catch (TypeError $invalid) { echo 'T'; unset($invalid); }
echo '|', implode(',', $items), '|';
$throwWords = [str_repeat('b', 2), str_repeat('a', 2)];
try { $sort($throwWords, boxedArgumentThrow(...)); }
catch (TypeError $thrown) { echo 'E'; unset($thrown); }
echo '|', implode(',', $throwWords); gc_collect_cycles();
"#;
    for strict in [false, true] {
        for mode in ["--ir-opt=off", "--ir-opt=on"] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-comparisons"); }
            let output = StrictFixture::new(source).capture(&flags);
            let expected = if strict { "1,2|1,2|a,b|T|2,1|E|bb,aa" }
                else { "1,2|1,2|a,b|P|1,2|E|bb,aa" };
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected,
                "strict={strict} {mode}: {}", String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "strict={strict} {mode}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Source and EIR optimization independently preserve the checked return's exception route.
#[test]
fn test_strict_comparison_static_comparator_guard_survives_optimizations() {
    let source = "<?php function nonIntegerComparator(int $left, int $right): bool { return true; } \
        $items = [2, 1]; try { usort($items, nonIntegerComparator(...)); echo 'X'; } \
        catch (TypeError $error) { echo 'T'; unset($error); } echo 'done'; gc_collect_cycles();";
    for (ast, ir) in [(false, false), (true, false), (false, true), (true, true)] {
        let output = StrictFixture::new(source).capture_staged_optimizers(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true), ast, ir);
        assert!(output.success, "ast={ast} ir={ir}: {} {}", output.stdout, output.stderr);
        assert_eq!(output.stdout, "Tdone", "ast={ast} ir={ir}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "ast={ast} ir={ir}: {}", output.stderr);
    }
}

/// Every native callback representation rejects hidden integer conversions and balances owners.
#[test]
fn test_strict_comparison_native_comparator_result_guards_bundle() {
    let source = include_str!("../fixtures/strict/comparator_native_results.php");
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true), optimize);
        assert!(output.success, "opt={optimize}: {} {}", output.stdout, output.stderr);
        assert_eq!(output.stdout, "I1,2;M1,2;C1,2;N1,2;A11;BFSDOCDRNUDV", "opt={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "opt={optimize}: {}", output.stderr);
    }
}
