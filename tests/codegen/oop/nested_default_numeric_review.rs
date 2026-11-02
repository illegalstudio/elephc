//! Purpose:
//! Preserves exact integer types when native default expressions cross into dynamic eval.
//!
//! Called from:
//! - The object-oriented codegen integration suite.
//!
//! Key details:
//! - A retained lazy default branch forces expression-source registration rather than scalar metadata.
//! - The smallest signed integer must not be parsed as a negated overflowing float literal.

use crate::support::*;

/// A selected native default keeps i64::MIN as an integer inside runtime eval too.
#[test]
fn test_nested_default_numeric_review_minimum_integer() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Defaults {
    const ENABLED = true;
    public static function kind(array $items = [(-9223372036854775807 - 1), self::ENABLED ? 'ok' : parent::A]): string {
        echo is_int($items[0]) ? 'I' : 'F';
        return $items[0] === (-9223372036854775807 - 1) ? 'same' : 'different';
    }
}
function run(string $source): mixed { return eval($source); }
echo Defaults::kind(), '|', run('return Defaults::kind();');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Isame|Isame", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Integer type preservation is the same without EIR optimization.
#[test]
fn test_nested_default_numeric_review_minimum_integer_no_ir_opt() {
    without_ir_opt(test_nested_default_numeric_review_minimum_integer);
}
