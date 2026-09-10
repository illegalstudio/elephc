//! Purpose:
//! Covers membership through boxed PHP array declarations and callable adapters.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Heap checks ensure scanning borrows values without leaking boxes or consuming aliases.
//! - Runtime strictness and mixed needles keep comparisons out of constant folding.

use crate::support::*;

/// Packed and sparse arrays preserve PHP strict/loose comparison and their existing cursor.
#[test]
fn test_core_boxed_array_membership_values_and_cursor_are_heap_clean() {
    let source = r#"<?php
class MembershipItem { public int $id = 1; }
function member(mixed $needle, array $items, bool $strict): string {
    return \IN_ARRAY($needle, $items, $strict) ? '1' : '0';
}
function wordMember(string $needle, array $items): bool { return in_array($needle, $items, true); }
echo member(2, ['2'], false), member(2, ['2'], true), '|';
echo member('2', [2], false), member('2', [2], true), '|';
echo member(false, [null], false), member(false, [null], true), '|';
echo member(null, [], false), member(null, [null], true), '|';
echo member(1.5, ['1.5'], false), member(1.5, ['1.5'], true), '|';
echo member([1, 2], [[1, 2]], true), member(['a' => 1], [8 => ['a' => 1]], true), '|';
$item = new MembershipItem();
$items = ['first' => str_repeat('x', 3), 'item' => $item, 8 => ['nested' => 7]];
$copy = $items;
next($items);
echo member(new MembershipItem(), $items, false), member(new MembershipItem(), $items, true),
    member($item, $items, true), '|';
echo wordMember(str_repeat('x', 3), $items) ? 'W' : 'bad';
for ($i = 0; $i < 5; $i++) {
    echo member(str_repeat('x', 3), $items, true), member(['nested' => 7], $items, true);
}
echo '|', key($items), ':', $copy['first'], ':', $copy[8]['nested'];
unset($item, $items, $copy);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "10|10|10|01|10|11|101|W1111111111|item:xxx:7");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Dynamic callable wrappers accept declared arrays and throw for non-array Mixed haystacks.
#[test]
fn test_core_boxed_array_membership_callable_and_invalid_inputs() {
    let source = r#"<?php
function invokeMember(callable $probe, mixed $needle, array $haystack, bool $strict): bool {
    return $probe($needle, $haystack, $strict);
}
function dynamicMember(mixed $haystack): bool { return in_array(1, $haystack, true); }
$probe = in_array(...);
echo invokeMember($probe, 2, ['v' => '2'], false) ? '1' : '0';
echo invokeMember($probe, 2, ['v' => '2'], true) ? '1' : '0';
echo call_user_func('in_array', 2, ['v' => 2], true) ? '1' : '0';
echo '|';
foreach ([null, 42, 'not an array', new stdClass()] as $bad) {
    try { dynamicMember($bad); echo 'bad'; }
    catch (TypeError $error) { echo 'T'; unset($error); }
}
echo '|', dynamicMember([1]) ? 'yes' : 'no';
unset($probe, $bad);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "101|TTTT|yes");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Strict Mixed comparisons use IEEE equality for signed zero, NaN and infinities.
#[test]
fn test_core_boxed_array_membership_strict_float_edges() {
    let source = r#"<?php
function sameFloat(mixed $left, mixed $right): bool { return $left === $right; }
function floatMember(float $needle, array $items): bool { return in_array($needle, $items, true); }
echo sameFloat(0.0, -0.0) ? '1' : '0';
echo sameFloat(NAN, NAN) ? '1' : '0';
echo sameFloat(INF, INF) ? '1' : '0';
echo '|';
echo floatMember(-0.0, [0.0]) ? '1' : '0';
echo floatMember(NAN, [NAN]) ? '1' : '0';
echo floatMember(INF, [INF]) ? '1' : '0';
echo floatMember(1.0, [1]) ? '1' : '0';
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "101|1010");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
