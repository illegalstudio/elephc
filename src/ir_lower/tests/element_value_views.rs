//! Purpose:
//! Verifies that a boxed element-forming operand is projected to its checked type at the
//! EIR boundary instead of widening the container it is written into.
//!
//! Called from:
//! - The AST-to-EIR unit test harness.
//!
//! Key details:
//! - Every supported target lowers the same projection and emits the function successfully.
//! - A value whose checked type does not fit the container keeps the widening path.
//! - An `if` arm that leaves a builder empty adopts the other arm's layout at the merge.

use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::{Function, Op};
use crate::types::PhpType;

/// Every supported target, so a projection cannot land on one backend only.
const TARGETS: [&str; 5] =
    ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"];

/// Lowers `source` for `target_name`, emits its assembly and hands `function_name` to `check`.
fn with_function(source: &str, target_name: &str, function_name: &str, check: impl Fn(&Function)) {
    let module = super::lower_source_at_for_target(
        source, Path::new("main.php"), Path::new("."), Target::parse(target_name).unwrap(),
    );
    let function = module.functions.iter().find(|f| f.name == function_name)
        .unwrap_or_else(|| panic!("{target_name}: missing function {function_name}"));
    check(function);
    crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|e| panic!("{target_name}: {e}"));
}

/// Returns whether the function converts any container to boxed `mixed` payloads.
fn widens_a_container(function: &Function) -> bool {
    function.instructions.iter().any(|i| matches!(i.op, Op::ArrayToMixed | Op::HashToMixed))
}

/// A hash `foreach` value written under a key keeps an inferred scalar map on its payload.
#[test]
fn element_views_keep_scalar_hash_writes_typed_on_all_targets() {
    for (name, ty) in [
        ("int", PhpType::Int), ("float", PhpType::Float),
        ("bool", PhpType::Bool), ("string", PhpType::Str),
    ] {
        let source = format!(r#"<?php
function overlay(array<string, {name}> $base, array<string, {name}> $overrides): array<string, {name}> {{
    $merged = $base;
    foreach ($overrides as $key => $value) {{ $merged[$key] = $value; }}
    return $merged;
}}
"#);
        for target_name in TARGETS {
            with_function(&source, target_name, "overlay", |function| {
                let set = function.instructions.iter().find(|i| i.op == Op::HashSet)
                    .expect("fixture must write the foreach value under its key");
                assert_eq!(function.value(set.operands[2]).unwrap().php_type, ty,
                    "{name} on {target_name}");
                assert!(!widens_a_container(function),
                    "{name} on {target_name}: the scalar map must not widen");
            });
        }
    }
}

/// A guard-narrowed `mixed` value fills a declared map inside a loop without a loop contract.
#[test]
fn element_views_keep_declared_maps_typed_through_guarded_loops_on_all_targets() {
    let source = r#"<?php
function validated(mixed $data): array<string, bool> {
    array<string, bool> $values = [];
    foreach ($data as $name => $value) {
        if (!is_string($name) || !is_bool($value)) { throw new UnexpectedValueException('bad'); }
        $values[$name] = $value;
    }
    return $values;
}
"#;
    for target_name in TARGETS {
        with_function(source, target_name, "validated", |function| {
            let set = function.instructions.iter().find(|i| i.op == Op::HashSet)
                .expect("fixture must write the guarded value");
            assert_eq!(function.value(set.operands[2]).unwrap().php_type, PhpType::Bool,
                "{target_name}");
            assert!(!widens_a_container(function),
                "{target_name}: the declared map must keep its bool payload");
        });
    }
}

/// Appends, plain copies, branch merges, guards and literals all keep `array<int>`.
#[test]
fn element_views_keep_indexed_builders_typed_on_all_targets() {
    let source = r#"<?php
function collect(bool $flag, mixed $extra): array {
    $source = ['a' => 5, 'b' => 6];
    $out = [];
    foreach ($source as $value) {
        $out[] = $value;
        $copy = $value;
        $out[] = $copy;
        $out[] = $flag ? $value : 0;
    }
    if (is_int($extra)) { $out[] = $extra; }
    $pair = [1, 2];
    foreach ($source as $value) { $pair = [$value, $value]; }
    return $flag ? $out : $pair;
}
"#;
    for target_name in TARGETS {
        with_function(source, target_name, "collect", |function| {
            for push in function.instructions.iter().filter(|i| i.op == Op::ArrayPush) {
                assert_eq!(function.value(push.operands[1]).unwrap().php_type, PhpType::Int,
                    "{target_name}: every appended operand must be the checked int");
            }
            assert!(!widens_a_container(function), "{target_name}: the builders must not widen");
        });
    }
}

/// A boxed row read from a `foreach` binding is extracted as the array it is.
#[test]
fn element_views_keep_nested_rows_typed_on_all_targets() {
    let source = r#"<?php
function rows(): array {
    $source = ['a' => [1, 2], 'b' => [3]];
    $out = [[9]];
    foreach ($source as $row) { $out[] = $row; }
    return $out;
}
"#;
    let row = PhpType::Array(Box::new(PhpType::Int));
    for target_name in TARGETS {
        with_function(source, target_name, "rows", |function| {
            assert!(function.instructions.iter().any(|i| i.op == Op::MixedUnbox
                && i.result_php_type == row), "{target_name}: the boxed row must be extracted");
            assert!(!widens_a_container(function), "{target_name}: the row list must not widen");
        });
    }
}

/// A value whose checked type the container cannot hold still widens the container.
#[test]
fn element_views_leave_genuine_widening_alone_on_all_targets() {
    let source = r#"<?php
function mixedUp(mixed $unknown): array {
    $source = ['a' => 5, 'b' => 'six'];
    $out = [1];
    foreach ($source as $value) { $out[] = $value; }
    $names = ['x' => 'y'];
    $names['k'] = $unknown;
    return [$out, $names];
}
"#;
    for target_name in TARGETS {
        with_function(source, target_name, "mixedUp", |function| {
            assert!(function.instructions.iter().any(|i| i.op == Op::ArrayToMixed),
                "{target_name}: a heterogeneous append must still box the list");
            assert!(function.instructions.iter().any(|i| i.op == Op::HashToMixed),
                "{target_name}: an unknown value must still box the map");
            assert!(!function.instructions.iter().any(|i| i.op == Op::Cast),
                "{target_name}: nothing here is proven, so nothing is projected");
        });
    }
}

/// A branch that leaves a builder empty adopts the other branch's layout instead of boxing.
#[test]
fn element_views_join_empty_arms_to_the_typed_layout_on_all_targets() {
    let source = r#"<?php
function evens(): array {
    $out = [];
    foreach ([1, 2, 3, 4] as $value) { if ($value % 2 === 0) { $out[] = $value; } }
    return $out;
}
function keyed(): array {
    $out = [];
    foreach ([1, 2, 3] as $value) { if ($value > 1) { $out['k' . $value] = $value; } }
    return $out;
}
"#;
    for target_name in TARGETS {
        with_function(source, target_name, "evens", |function| {
            assert!(!widens_a_container(function),
                "{target_name}: the empty arm must adopt array<int>");
        });
        with_function(source, target_name, "keyed", |function| {
            assert!(!widens_a_container(function),
                "{target_name}: the empty arm must adopt the typed hash");
            for convert in function.instructions.iter().filter(|i| i.op == Op::ArrayToHash) {
                assert!(matches!(convert.result_php_type.codegen_repr(),
                    PhpType::AssocArray { value, .. } if value.codegen_repr() == PhpType::Int),
                    "{target_name}: an empty arm becomes the int-valued hash on its edge");
            }
        });
    }
}

/// A local assigned `null` and then a string in one branch reads `string` in the checker but
/// can still hold the `null`, so its boxed value is appended as it is, never cast.
#[test]
fn element_views_do_not_project_ordinary_assigned_locals_on_all_targets() {
    let source = r#"<?php
function maybe(int $n): array {
    $text = null;
    if ($n > 0) { $text = 'in' . $n; }
    $out = [];
    $out[] = $text;
    return [$text, $out];
}
"#;
    for target_name in TARGETS {
        with_function(source, target_name, "maybe", |function| {
            assert!(!function.instructions.iter().any(|i| i.op == Op::Cast
                && i.result_php_type == PhpType::Str),
                "{target_name}: an unproven local must keep its boxed value");
        });
    }
}

/// A branch result used as a literal value is concrete on every arm, so a keyed literal
/// stores raw integers instead of the boxed merge temp it used to hold.
#[test]
fn element_views_keep_branch_results_typed_in_literals_on_all_targets() {
    let source = r#"<?php
function keyed(bool $flag): array {
    $source = ['a' => 5, 'b' => 6];
    $out = [];
    foreach ($source as $value) { $out = ['v' => $flag ? $value : 0, 'w' => $value ?: 1]; }
    return $out;
}
"#;
    for target_name in TARGETS {
        with_function(source, target_name, "keyed", |function| {
            let sets: Vec<_> = function.instructions.iter().filter(|i| i.op == Op::HashSet).collect();
            assert!(!sets.is_empty(), "{target_name}: fixture must build the keyed literal");
            for set in sets {
                assert_eq!(function.value(set.operands[2]).unwrap().php_type, PhpType::Int,
                    "{target_name}: every literal value must be the checked int");
            }
            assert!(!widens_a_container(function), "{target_name}: the literal must not widen");
        });
    }
}

/// `??`, `match` and a ternary beside a call all read the loop binding as the checked int, so
/// an unseeded literal assigned to a fresh local is built as `array<int>`.
#[test]
fn element_views_project_every_merge_arm_kind_on_all_targets() {
    let source = r#"<?php
function merged(bool $flag): array {
    $source = ['a' => 5, 'b' => 6];
    foreach ($source as $value) {
        $coalesced = [$value ?? 0];
        $matched = [match ($flag) { true => $value, default => 0 }];
        $beside = [$value, $flag ? $value : strlen('ab')];
    }
    return [$coalesced, $matched, $beside];
}
"#;
    for target_name in TARGETS {
        with_function(source, target_name, "merged", |function| {
            for push in function.instructions.iter().filter(|i| i.op == Op::ArrayPush) {
                let operand = &function.value(push.operands[1]).unwrap().php_type;
                assert!(matches!(operand, PhpType::Int | PhpType::Array(_)),
                    "{target_name}: a merged literal item must be the checked int, got {operand:?}");
            }
            for literal in function.instructions.iter().filter(|i| i.op == Op::ArrayNew) {
                assert!(!matches!(&literal.result_php_type,
                    PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed),
                    "{target_name}: no literal may be built with boxed elements");
            }
        });
    }
}
