//! Purpose:
//! Verifies which local array writes keep packed storage and which promote to a hash because
//! their integer key may skip a slot of a non-empty packed array.
//!
//! Called from:
//! - AST-to-EIR unit tests.
//!
//! Key details:
//! - A candidate `array<int>` local written under a key nothing bounds keeps its typed storage:
//!   its `ArraySet` carries `Immediate::PromotableArray`, and codegen promotes it to a hash only
//!   when a key skips a slot. A local some construct can reach otherwise promotes eagerly with
//!   `ArrayToHash` + `HashSet`; a widening value keeps the runtime-promoting `ArraySetMixedKey`
//!   writer. No path is a plain `ArraySet`, which would zero-fill the gap.

use super::*;
use crate::ir::{Immediate, Op};
use crate::types::PhpType;

/// Returns whether the named function lowers any instruction with `op`.
fn function_uses(module: &crate::ir::Module, name: &str, op: Op) -> bool {
    module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing function {name}"))
        .instructions
        .iter()
        .any(|inst| inst.op == op)
}

/// Returns whether the named function lowers an `ArraySet` with the given promotable marking.
fn function_uses_array_set(module: &crate::ir::Module, name: &str, promotable: bool) -> bool {
    module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing function {name}"))
        .instructions
        .iter()
        .any(|inst| {
            inst.op == Op::ArraySet
                && (inst.immediate == Some(Immediate::PromotableArray)) == promotable
        })
}

/// Counter-indexed loops (sorting, scaling, fills) never promote eagerly or zero-fill.
#[test]
fn counter_loop_writes_never_promote_to_hash() {
    let module = lower_source(r#"<?php
function bubble(int $extra) {
    $arr = [5, 3, 8, 1];
    $arr[] = $extra;
    $n = count($arr);
    for ($i = 0; $i < $n - 1; $i++) {
        for ($j = 0; $j < $n - $i - 1; $j++) {
            if ($arr[$j] > $arr[$j + 1]) {
                $tmp = $arr[$j];
                $arr[$j] = $arr[$j + 1];
                $arr[$j + 1] = $tmp;
            }
        }
    }
    return $arr;
}
function scale(int $extra) {
    $values = [1, 2, $extra];
    for ($i = 0; $i < count($values); $i++) { $values[$i] = $values[$i] * 2; }
    return $values;
}
function select(int $extra) {
    $a = [4, 2, 9, $extra];
    $n = count($a);
    for ($i = 0; $i < $n; $i++) {
        for ($j = $i + 1; $j < $n; $j++) {
            if ($a[$j] < $a[$i]) { $t = $a[$i]; $a[$i] = $a[$j]; $a[$j] = $t; }
        }
    }
    return $a;
}
function fill(int $k) {
    $a = [5, 6];
    for ($i = 0; $i < $k; $i++) { $a[$i] = $i * 3; }
    return $a;
}
echo implode(",", bubble(2)), implode(",", scale(3)), implode(",", select(1)), implode(",", fill(4));
"#);
    for name in ["bubble", "scale", "select", "fill"] {
        assert!(!function_uses(&module, name, Op::ArrayToHash), "{name} promoted to a hash");
        assert!(!function_uses_array_set(&module, name, false), "{name} kept a zero-filling write");
    }
    // The sorts move integers between slots, so their swaps keep raw `array<int>` storage.
    for name in ["bubble", "select"] {
        assert!(function_uses_array_set(&module, name, true), "{name} lost its typed write");
        assert!(!function_uses(&module, name, Op::ArraySetMixedKey), "{name} boxed its slots");
    }
}

/// Unbounded keys, a shrinking body, a rebound alias and an escaping local never zero-fill.
#[test]
fn unbounded_loop_writes_never_zero_fill() {
    let module = lower_source(r#"<?php
function past_end(int $k) { $a = [1, 2]; $a[$k] = 1; return $a; }
function off_by_one(int $extra) {
    $a = [1, $extra];
    for ($i = 0; $i < count($a); $i++) { $a[$i + 2] = 0; }
    return $a;
}
function shrinking(int $extra) {
    $a = [1, 2, $extra];
    for ($i = 0; $i < count($a); $i++) { array_pop($a); $a[$i] = 0; }
    return $a;
}
function rebound_alias(int $extra) {
    $a = [1, $extra];
    $n = count($a);
    $n = $n + 5;
    for ($i = 0; $i < $n; $i++) { if ($i > 2) { $a[$i] = 0; } }
    return $a;
}
function consume($x) { return count($x); }
function consume_ref(&$x) { return count($x); }
function passed_by_value(int $k) { $a = [1, 2]; $a[$k] = 1; consume($a); return $a; }
function passed_out(int $k) { $a = [1, 2]; $a[$k] = 1; consume_ref($a); return $a; }
echo count(past_end(4)), count(off_by_one(1)), count(shrinking(2)), count(rebound_alias(1)),
    count(passed_by_value(5)), count(passed_out(5));
"#);
    for name in ["past_end", "off_by_one", "shrinking", "rebound_alias", "passed_by_value", "passed_out"] {
        assert!(!function_uses_array_set(&module, name, false), "{name} kept a zero-filling write");
        assert!(function_uses_array_set(&module, name, true)
            || function_uses(&module, name, Op::ArrayToHash)
            || function_uses(&module, name, Op::ArraySetMixedKey), "{name} has no sparse-safe write");
    }
    // A by-value user parameter receives the `array<mixed>` view, so the local stays promotable;
    // a by-reference one could mutate it behind the dispatch, so it promotes at the write.
    assert!(function_uses_array_set(&module, "passed_by_value", true), "passed_by_value lost promotion");
    assert!(function_uses(&module, "passed_out", Op::ArrayToHash), "passed_out stayed promotable");
}

/// A widening loop write keeps the runtime writer; an integer scatter keeps typed storage.
#[test]
fn widening_and_scatter_loop_writes() {
    let module = lower_source(r#"<?php
function hist(int $n) {
    $counts = [0, 0, 0, 0];
    $seed = 7;
    for ($i = 0; $i < $n; $i++) {
        $seed = ($seed * 31 + 5) % 1000;
        $bucket = $seed % 4;
        $counts[$bucket] = $counts[$bucket] + 1;
    }
    return $counts;
}
function scatter(int $n) {
    $perm = [];
    for ($i = 0; $i < $n; $i++) { $perm[] = ($i * 7) % $n; }
    $out = array_fill(0, $n, 0);
    for ($i = 0; $i < $n; $i++) { $out[$perm[$i]] = 7; }
    return $out;
}
echo implode(",", hist(10)), implode(",", scatter(5));
"#);
    // `$counts[$bucket] + 1` is `int|float`, which widens the payload to the runtime writer.
    assert!(!function_uses(&module, "hist", Op::ArrayToHash), "hist promoted to a hash");
    assert!(function_uses(&module, "hist", Op::ArraySetMixedKey), "hist lost the runtime writer");
    // A scatter stores integers under keys read from another array: typed and promotable.
    assert!(!function_uses(&module, "scatter", Op::ArrayToHash), "scatter promoted to a hash");
    assert!(!function_uses(&module, "scatter", Op::ArraySetMixedKey), "scatter boxed its slots");
    assert!(function_uses_array_set(&module, "scatter", true), "scatter lost its typed write");
}

/// A declared `array<int>` parameter turns promotable only when a caller passes a promotable
/// array: that caller hands it on raw, while a plain caller's callee keeps ordinary packed writes.
#[test]
fn declared_array_int_params_activate_only_for_promotable_callers() {
    let module = lower_source(r#"<?php
function plain(array<int> $p): array<int> { $p[0] = 1; return $p; }
function plain_caller(int $n): int { $a = [1, 2, $n]; return count(plain($a)); }
function shifted(array<int> $p): array<int> {
    $n = count($p);
    for ($i = 0; $i < $n; $i++) { $j = $i + 1; $p[$j] = $p[$i]; }
    return $p;
}
function promotable_caller(int $n): int {
    $a = [1, 2, $n];
    for ($i = 0; $i < 2; $i++) { $j = $i + 1; $a[$j] = $a[$i]; }
    $b = shifted($a);
    return count($b);
}
echo plain_caller(3), promotable_caller(3);
"#);
    // Nobody passes `plain` a promotable array: its write stays an ordinary packed `ArraySet`
    // and its return needs no `array<mixed>` view.
    assert!(function_uses_array_set(&module, "plain", false), "plain lost its packed write");
    assert!(!function_uses_array_set(&module, "plain", true), "plain was activated");
    assert!(!function_uses(&module, "plain", Op::ArrayToMixed), "plain viewed its return");
    // The promotable caller hands its local on raw and receives the result raw.
    assert!(function_uses_array_set(&module, "shifted", true), "shifted lost its typed write");
    assert!(!function_uses(&module, "promotable_caller", Op::ArrayToMixed), "the caller boxed a raw hand-off");
}

/// Returns the user functions the named function calls directly.
fn direct_callees(module: &crate::ir::Module, name: &str) -> Vec<String> {
    module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing function {name}"))
        .instructions
        .iter()
        .filter_map(|inst| match (inst.op, &inst.immediate) {
            (Op::Call, Some(Immediate::Data(target))) => {
                module.data.function_names.get(target.as_raw() as usize).cloned()
            }
            _ => None,
        })
        .collect()
}

/// A promotable local passed to an undeclared parameter calls a typed clone that takes the array
/// raw and keeps typed promotable writes; a plain caller keeps the boxed original.
#[test]
fn undeclared_params_receive_promotable_arrays_through_typed_clones() {
    let module = lower_source(r#"<?php
function shifted($p) {
    $n = count($p);
    for ($i = 0; $i < $n; $i++) { $j = $i + 1; $p[$j] = $p[$i]; }
    return $p;
}
function promotable_caller(int $n): int {
    $a = [1, 2, $n];
    for ($i = 0; $i < 2; $i++) { $j = $i + 1; $a[$j] = $a[$i]; }
    return count(shifted($a));
}
function plain_caller(int $n): int { return count(shifted([1, $n])); }
echo promotable_caller(3), plain_caller(3);
"#);
    let clone = "shifted#__promotable_0";
    let function = module
        .functions
        .iter()
        .find(|function| function.name == clone)
        .unwrap_or_else(|| panic!("missing clone {clone}"));
    assert_eq!(function.source_name.as_deref(), Some("shifted"));
    assert_eq!(function.params[0].php_type, PhpType::Array(Box::new(PhpType::Int)));
    assert!(function_uses_array_set(&module, clone, true), "the clone lost its typed write");
    assert!(direct_callees(&module, "promotable_caller").iter().any(|callee| callee == clone));
    assert!(direct_callees(&module, "plain_caller").iter().any(|callee| callee == "shifted"));
    assert!(!function_uses(&module, "promotable_caller", Op::ArrayToMixed), "the caller boxed a raw hand-off");
    let original = module.functions.iter().find(|function| function.name == "shifted").expect("original");
    assert_eq!(original.params[0].php_type, PhpType::Mixed);
}
