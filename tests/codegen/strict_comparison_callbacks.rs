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
