//! Purpose:
//! Preserves static compound RHS effects before deferred class-initialization errors.
//!
//! Called from:
//! - The object-oriented codegen integration suite.
//!
//! Key details:
//! - Literal, constant and computed dimensions share PHP's eager compound RHS order.
//! - Coalescing remains lazy, and temporary operands are checked with heap diagnostics.

use crate::support::*;

/// Verifies output and owner retirement for one deferred-default compound fixture.
fn assert_compound(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eager compound operands run before the class Error, unlike a lazy coalescing RHS.
#[test]
fn test_nested_default_compound_review_rhs_order() {
    assert_compound(r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer { use Values; public static array $items = [1]; }
const KEY = 0;
function value(): int { echo 'v'; return 2; }
function text(): string { echo 's'; return str_repeat('x', 2); }
function index(): int { echo 'k'; return 0; }
try { Consumer::$items[0] += value(); } catch (Error $error) { echo 'E|'; }
try { Consumer::$items[KEY] .= text(); } catch (Error $error) { echo 'E|'; }
try { Consumer::$items[index()] -= value(); } catch (Error $error) { echo 'E|'; }
try { Consumer::$items[0] ??= value(); } catch (Error $error) { echo 'E|'; }
$key = 0;
try { Consumer::$items[$key] += value(); } catch (Error $error) { echo 'E'; }
unset($error);
"#, "vE|sE|kvE|E|vE");
}

/// Deferred compound ordering is identical without EIR optimization.
#[test]
fn test_nested_default_compound_review_rhs_order_no_ir_opt() {
    without_ir_opt(test_nested_default_compound_review_rhs_order);
}

/// A throwing eager RHS wins over the deferred class access Error.
#[test]
fn test_nested_default_compound_review_rhs_throw() {
    assert_compound(r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer { use Values; public static array $items = [1]; }
function value(): int { echo 'v'; throw new RuntimeException('rhs'); }
try { Consumer::$items[0] += value(); }
catch (RuntimeException $error) { echo $error->getMessage(); }
catch (Error $error) { echo 'bad'; }
unset($error);
"#, "vrhs");
}

/// RHS exception precedence remains the same without EIR optimization.
#[test]
fn test_nested_default_compound_review_rhs_throw_no_ir_opt() {
    without_ir_opt(test_nested_default_compound_review_rhs_throw);
}

/// A fresh string index stays owned when the eager compound RHS throws before the class Error.
#[test]
fn test_nested_default_compound_review_string_index_throw() {
    assert_compound(r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer { use Values; public static array $items = [1]; }
function makeKey(): string { echo 'k'; return str_repeat('x', 2); }
function fail(): string { echo 'v'; throw new RuntimeException('rhs'); }
try { Consumer::$items[makeKey()] .= fail(); }
catch (RuntimeException $error) { echo $error->getMessage(); }
catch (Error $error) { echo 'bad'; }
unset($error);
"#, "kvrhs");
}

/// Computed string index cleanup is unchanged without EIR optimization.
#[test]
fn test_nested_default_compound_review_string_index_throw_no_ir_opt() {
    without_ir_opt(test_nested_default_compound_review_string_index_throw);
}
