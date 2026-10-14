//! Purpose:
//! Regressions for false parents in nested assignments, appends and static updates.
//!
//! Called from:
//! - The objects codegen integration suite.
//!
//! Key details:
//! - PHP deprecates conversion only after the RHS and before the leaf write.
//! - Replaced promoted parents skip the missing-key read and keep owner cleanup balanced.

use crate::support::*;

/// A literal null dimension selects the empty-string key without touching false at key zero.
#[test]
fn test_static_false_tenth_review_null_parent_key() {
    assert_false_write(r#"<?php
$items = [false]; $copy = $items;
set_error_handler(function(int $level, string $message): bool { return true; });
$items[null][0] = 9;
echo json_encode($items), ':', json_encode($copy);
restore_error_handler();
unset($items, $copy);
"#, r#"{"0":false,"":[9]}:[false]"#);
}

/// Literal null parent dimensions keep their key shape without EIR optimization.
#[test]
fn test_static_false_tenth_review_null_parent_key_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_null_parent_key);
}

/// One-level one level local append uses the same attached parent protocol.
#[test]
fn test_static_false_tenth_review_one_level_local_append() {
    assert_false_write(r#"<?php
$items = [false]; $copy = $items;
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
$items[0][] = 3;
echo json_encode($items), ':', json_encode($copy);
restore_error_handler(); unset($items, $copy);
"#, r#"8192|[[3]]:[false]"#);
}

/// One-level false-parent append is unchanged without EIR optimization.
#[test]
fn test_static_false_tenth_review_one_level_local_append_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_one_level_local_append);
}

/// One-level one level static append uses the same attached parent protocol.
#[test]
fn test_static_false_tenth_review_one_level_static_append() {
    assert_false_write(r#"<?php
class C { public static array $items = [false]; }
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
C::$items[0][] = 3;
echo json_encode(C::$items);
restore_error_handler(); C::$items = [];
"#, r#"8192|[[3]]"#);
}

/// One-level false-parent append is unchanged without EIR optimization.
#[test]
fn test_static_false_tenth_review_one_level_static_append_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_one_level_static_append);
}

/// Pending conversion errors preserve key-sensitive completion and TypeError chaining.
#[test]
fn test_static_false_tenth_review_pending_key_kinds() {
    let source = r#"<?php
class C { public static array $items = [[false]]; }
$key = match ((int)$argv[1]) { 0 => 1, 1 => 'leaf', 2 => true, 3 => null, 4 => 1.5, 5 => [], 6 => new stdClass(), default => fopen('php://memory', 'w+') };
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; throw new Error('handler'); });
try { C::$items[0][0][$key] = 9; }
catch (Throwable $e) {
    $previous = $e->getPrevious();
    echo get_class($e), ':', $e->getMessage(), ':', $previous === null ? 'none' : $previous->getMessage(), ':';
}
restore_error_handler(); if (is_resource($key)) { fclose($key); } echo json_encode(C::$items); C::$items = []; unset($key, $e, $previous);
"#;
    let expected = [
        r#"8192|Error:handler:none:[[{"1":9}]]"#,
        r#"8192|Error:handler:none:[[{"leaf":9}]]"#,
        r#"8192|Error:handler:none:[[{"1":9}]]"#,
        r#"8192|Error:handler:none:[[[]]]"#,
        r#"8192|Error:handler:none:[[[]]]"#,
        r#"8192|TypeError:Cannot access offset of type array on array:handler:[[[]]]"#,
        r#"8192|TypeError:Cannot access offset of type stdClass on array:handler:[[[]]]"#,
        r#"8192|Error:handler:none:[[[]]]"#,
    ];
    let args = ["0", "1", "2", "3", "4", "5", "6", "7"];
    let vectors = args.iter().map(|arg| vec![*arg]).collect::<Vec<_>>();
    let argvs = vectors.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let outputs = compile_and_run_with_heap_debug_per_argv(source, &argvs);
    for (out, expected) in outputs.into_iter().zip(expected) {
        assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
        assert_eq!(out.stdout, expected, "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// Pending-key completion and chaining do not depend on EIR optimization.
#[test]
fn test_static_false_tenth_review_pending_key_kinds_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_pending_key_kinds);
}

/// Preserves PHP opcode completion for ancestor conversion throw.
#[test]
fn test_static_false_tenth_review_ancestor_conversion_throw() {
    assert_false_write(r#"<?php
class C { public static array $items = [false]; }
set_error_handler(function(int $level, string $message): bool {
    echo $level, ':', json_encode(C::$items), '|'; throw new Error('handler');
});
try { C::$items[0][0][1] = 9; } catch (Error $e) { echo $e->getMessage(); }
restore_error_handler(); echo ':', json_encode(C::$items); C::$items = []; unset($e);
"#, r#"8192:[[]]|handler:[[null]]"#);
}

/// Preserves opcode completion without EIR optimization.
#[test]
fn test_static_false_tenth_review_ancestor_conversion_throw_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_ancestor_conversion_throw);
}

/// Preserves PHP opcode completion for append conversion throw.
#[test]
fn test_static_false_tenth_review_append_conversion_throw() {
    assert_false_write(r#"<?php
class C { public static array $items = [[false]]; }
set_error_handler(function(int $level, string $message): bool {
    echo $level, ':', json_encode(C::$items), '|'; throw new Error('handler');
});
try { C::$items[0][0][] = 3; } catch (Error $e) { echo $e->getMessage(); }
restore_error_handler(); echo ':', json_encode(C::$items); C::$items = []; unset($e);
"#, r#"8192:[[[]]]|handler:[[[3]]]"#);
}

/// Preserves opcode completion without EIR optimization.
#[test]
fn test_static_false_tenth_review_append_conversion_throw_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_append_conversion_throw);
}

/// Preserves PHP ordering and cleanup for ancestor assignment.
#[test]
fn test_static_false_tenth_review_ancestor_assignment() {
    assert_false_write(r#"<?php
class C { public static array $items = [false]; }
function rhs(): int { echo 'R'; return 9; }
set_error_handler(function(int $level, string $message): bool {
    echo $level, ':', json_encode(C::$items), '|'; return true;
});
C::$items[0][0][1] = rhs();
echo json_encode(C::$items);
restore_error_handler(); C::$items = [];
"#, r#"R8192:[[]]|[[{"1":9}]]"#);
}

/// Preserves the same behavior without EIR optimization.
#[test]
fn test_static_false_tenth_review_ancestor_assignment_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_ancestor_assignment);
}

/// Preserves PHP ordering and cleanup for computed assignment.
#[test]
fn test_static_false_tenth_review_computed_assignment() {
    assert_false_write(r#"<?php
class C { public static array $items = [[false]]; }
function outer(): int { echo 'P'; return 0; }
function middle(): int { echo 'M'; return 0; }
function leaf(int $n): string { echo 'L'; return 'k'.strval($n); }
function rhs(): int { echo 'R'; return 9; }
set_error_handler(function(int $level, string $message): bool {
    echo $level, ':', json_encode(C::$items), '|'; return true;
});
C::$items[outer()][middle()][leaf($argc)] = rhs();
echo json_encode(C::$items);
restore_error_handler(); C::$items = [];
"#, r#"PMLR8192:[[[]]]|[[{"k1":9}]]"#);
}

/// Preserves the same behavior without EIR optimization.
#[test]
fn test_static_false_tenth_review_computed_assignment_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_computed_assignment);
}

/// Preserves PHP ordering and cleanup for conversion throw.
#[test]
fn test_static_false_tenth_review_conversion_throw() {
    assert_false_write(r#"<?php
class C { public static array $items = [[false]]; }
function rhs(): int { echo 'R'; return 9; }
set_error_handler(function(int $level, string $message): bool {
    echo $level, ':', json_encode(C::$items), '|'; throw new Error('handler');
});
try { C::$items[0][0][1] = rhs(); } catch (Error $e) { echo $e->getMessage(); }
restore_error_handler(); echo ':', json_encode(C::$items); C::$items = []; unset($e);
"#, r#"R8192:[[[]]]|handler:[[{"1":9}]]"#);
}

/// Preserves the same behavior without EIR optimization.
#[test]
fn test_static_false_tenth_review_conversion_throw_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_conversion_throw);
}

/// Preserves PHP ordering and cleanup for true parent.
#[test]
fn test_static_false_tenth_review_true_parent() {
    assert_false_write(r#"<?php
function write(bool $flag): void {
    $items = [[$flag]];
    try { $items[0][0][1] = 9; } catch (Error $e) { echo $e->getMessage(); }
}
write(true);
"#, r#"Cannot use a scalar value as an array"#);
}

/// Preserves the same behavior without EIR optimization.
#[test]
fn test_static_false_tenth_review_true_parent_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_true_parent);
}

/// Checks complete native output and balanced temporary owners for one write context.
fn assert_false_write(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Keeps false-parent static assignment compatible with PHP.
#[test]
fn test_static_false_tenth_review_static_assignment() {
    assert_false_write(r#"<?php
class C { public static array $items = [[false], [null], [0]]; }
$copy = C::$items;
set_error_handler(function(int $level, string $message): bool {
    echo $level, ':', json_encode(C::$items), '|'; return true;
});
C::$items[0][0][1] = 9;
echo json_encode(C::$items), ':', json_encode($copy);
restore_error_handler(); C::$items = []; unset($copy);
"#, r#"8192:[[[]],[null],[0]]|[[{"1":9}],[null],[0]]:[[false],[null],[0]]"#);
}

/// Checks the same write context without EIR optimization.
#[test]
fn test_static_false_tenth_review_static_assignment_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_static_assignment);
}

/// Keeps false-parent local assignment compatible with PHP.
#[test]
fn test_static_false_tenth_review_local_assignment() {
    assert_false_write(r#"<?php
$items = [[false], [null], [0]]; $copy = $items;
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
$items[0][0][1] = 9;
echo json_encode($items), ':', json_encode($copy);
restore_error_handler(); unset($items, $copy);
"#, r#"8192|[[{"1":9}],[null],[0]]:[[false],[null],[0]]"#);
}

/// Checks the same write context without EIR optimization.
#[test]
fn test_static_false_tenth_review_local_assignment_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_local_assignment);
}

/// Keeps false-parent static append compatible with PHP.
#[test]
fn test_static_false_tenth_review_static_append() {
    assert_false_write(r#"<?php
class C { public static array $items = [[false]]; }
function rhs(): int { echo 'R'; return 3; }
set_error_handler(function(int $level, string $message): bool {
    echo $level, ':', json_encode(C::$items), '|'; return true;
});
C::$items[0][0][] = rhs();
echo json_encode(C::$items);
restore_error_handler(); C::$items = [];
"#, r#"R8192:[[[]]]|[[[3]]]"#);
}

/// Checks the same write context without EIR optimization.
#[test]
fn test_static_false_tenth_review_static_append_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_static_append);
}

/// Keeps false-parent local append compatible with PHP.
#[test]
fn test_static_false_tenth_review_local_append() {
    assert_false_write(r#"<?php
$items = [[false]];
function rhs(): int { echo 'R'; return 3; }
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
$items[0][0][] = rhs();
echo json_encode($items);
restore_error_handler(); unset($items);
"#, r#"R8192|[[[3]]]"#);
}

/// Checks the same write context without EIR optimization.
#[test]
fn test_static_false_tenth_review_local_append_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_local_append);
}

/// Keeps false-parent assignment rhs throw compatible with PHP.
#[test]
fn test_static_false_tenth_review_assignment_rhs_throw() {
    assert_false_write(r#"<?php
class C { public static array $items = [[false]]; }
function rhs(): int { echo 'R'; throw new Error('stop'); }
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
try { C::$items[0][0][1] = rhs(); } catch (Error $e) { echo $e->getMessage(), '|'; }
echo json_encode(C::$items);
restore_error_handler(); C::$items = []; unset($e);
"#, r#"Rstop|[[false]]"#);
}

/// Checks the same write context without EIR optimization.
#[test]
fn test_static_false_tenth_review_assignment_rhs_throw_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_assignment_rhs_throw);
}

/// Root, intermediate and leaf replacement leave only the selected temporary owner.
#[test]
fn test_static_false_tenth_review_detached_parent() {
    let source = r#"<?php
class C { public static array $items = [[false]]; }
$mode = (int)$argv[1];
set_error_handler(function(int $level, string $message) use ($mode): bool {
    echo $level, '|';
    if ($level !== E_DEPRECATED) { throw new Error('unexpected:' . $message); }
    if ($mode === 0) { C::$items = [['replaced']]; }
    elseif ($mode === 1) { C::$items[0] = ['replaced']; }
    else { C::$items[0][0] = 'replaced'; }
    return true;
});
try {
    $result = ++C::$items[0][0][1];
    echo 'OK:', $result, ':', json_encode(C::$items);
} catch (Error $e) { echo 'EX:', $e->getMessage(); }
restore_error_handler(); C::$items = []; unset($e);
"#;
    let outputs = compile_and_run_with_heap_debug_per_argv(source, &[&["0"], &["1"], &["2"]]);
    for out in outputs {
        assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
        assert_eq!(out.stdout, r#"8192|OK:1:[["replaced"]]"#, "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// Handler detachment keeps the same semantics without EIR optimization.
#[test]
fn test_static_false_tenth_review_detached_parent_no_ir_opt() {
    without_ir_opt(test_static_false_tenth_review_detached_parent);
}
