//! Purpose:
//! Regression tests for AST-to-EIR lowering of indexed array expressions.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - Array access result metadata must come from the lowered array value, not
//!   from syntactic fallback inference that lacks local type facts.

use crate::ir::print_module;

/// Literal storage follows the boxed nullsafe-chain result instead of guessing scalar metadata.
#[test]
fn nullsafe_literal_results_use_mixed_slots_on_every_target() {
    use crate::ir::Op;
    use crate::types::PhpType;
    let source = r#"<?php
class ArrayProbeLeaf { public function __construct(public string $name) {} }
class ArrayProbeFactory { public function leaf(): ArrayProbeLeaf { return new ArrayProbeLeaf("leaf"); } }
class ArrayProbeHolder { public function __construct(public ?ArrayProbeFactory $factory) {} }
function nullsafeListProbe(?ArrayProbeHolder $holder): array { return [$holder?->factory?->leaf()]; }
function nullsafeMapProbe(?ArrayProbeHolder $holder): array { return ["leaf" => $holder?->factory?->leaf()]; }
echo count(nullsafeListProbe(null)), count(nullsafeMapProbe(null));
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["nullsafeListProbe", "nullsafeMapProbe"] {
            let function = module.functions.iter().find(|function| function.name.as_str() == name).unwrap();
            let literal = function.instructions.iter().find(|inst| matches!(inst.op, Op::ArrayNew | Op::HashNew)).unwrap();
            let element = match literal.result_php_type.codegen_repr() {
                PhpType::Array(element) => element,
                PhpType::AssocArray { value, .. } => value,
                other => panic!("{target}: {name}: {other:?}"),
            };
            assert_eq!(element.codegen_repr(), PhpType::Mixed, "{target}: {name}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Boxed slices retain before conversion and retire their private payload without source writeback.
#[test]
fn boxed_array_slice_owns_normalization_without_mutating_sources_on_every_target() {
    let source = r#"<?php
function sliceBoxedSnapshot(mixed $values): array { return array_slice($values, 1, 2); }
echo count(sliceBoxedSnapshot([1, 2, 3, 4]));
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let converted = asm.find("__rt_array_to_mixed").unwrap();
        let unboxed = asm[..converted].rfind("__rt_mixed_unbox").unwrap();
        assert!(asm[unboxed..converted].contains("__rt_incref"), "{target}");
        let sliced = converted + asm[converted..].find("__rt_array_slice_refcounted").unwrap();
        let retired = sliced + asm[sliced..].find("__rt_decref_array").unwrap();
        assert!(converted < sliced && sliced < retired, "{target}");
        let old_writeback = if target == "linux-x86_64" {
            "mov QWORD PTR [r10 + 8], rax"
        } else {
            "str x0, [x10, #8]"
        };
        assert!(!asm[converted..sliced].contains(old_writeback), "{target}");
    }
}

/// Splice separates the outer cell before consuming and mutating its packed payload owner.
#[test]
fn boxed_array_splice_separates_receiver_before_payload_on_every_target() {
    let source = r#"<?php
function spliceBoxed(array &$values): array { return array_splice($values, 1, 2, ["x"]); }
$values = [1, 2, 3, 4];
echo count(spliceBoxed($values));
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let cell = asm.find("__rt_array_cell_ensure_unique").unwrap();
        let payload = asm[cell..].find("__rt_array_to_mixed").unwrap();
        let splice = asm[cell..].find("__rt_array_splice_refcounted").unwrap();
        assert!(payload < splice, "{target}");
    }
}

/// Declared PHP array sorts normalize both layouts and retain scalar guards on every target.
#[test]
fn boxed_array_sorts_emit_normalization_and_comparison_on_every_target() {
    let source = r#"<?php
function sortBoxed(array &$values): void { sort($values); rsort($values); }
$values = ["last" => 3, 7 => 1, "middle" => 2];
sortBoxed($values);
echo implode(",", $values);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        for helper in ["__rt_array_cell_ensure_unique", "__rt_array_to_mixed", "__rt_array_ensure_unique",
            "__rt_mixed_sort_require_scalars", "__rt_php_compare_slots", "__rt_php_compare_slots_desc"]
        {
            assert!(asm.contains(helper), "{target}: {helper}");
        }
    }
}

/// Boxed prepends retain a prefix before COW and publish a merged payload on every target.
#[test]
fn boxed_array_unshift_emits_owned_prefix_and_merge_on_every_target() {
    let source = r#"<?php
function prependBoxed(array &$values): int { return array_unshift($values, $values, "prefix", 1.25); }
$values = [1, 2];
echo prependBoxed($values), count($values);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let prefix = asm.find("__rt_array_push_refcounted").expect("retained prefix");
        let split = asm.find("__rt_array_cell_ensure_unique").expect("boxed receiver COW");
        let merge = asm.find("__rt_array_merge_boxed").expect("PHP key renumbering");
        assert!(prefix < split && split < merge, "{target}");
        assert!(asm[merge..].contains("__rt_decref_any"), "{target}");
    }
}

/// Concrete string pop/shift transfer the removed slot instead of retaining it through borrowed boxing.
#[test]
fn concrete_array_take_transfers_removed_strings_on_every_target() {
    let source = r#"<?php
function popConcreteString(int $length): mixed {
    $items = [str_repeat('x', $length)];
    return array_pop($items);
}
function shiftConcreteString(int $length): mixed {
    $items = [str_repeat('y', $length)];
    return array_shift($items);
}
echo popConcreteString($argc), shiftConcreteString($argc);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        for empty_branch in ["array_pop_empty", "array_shift_empty"] {
            let removed_slot = asm.split(empty_branch).nth(1).unwrap_or_else(|| panic!("{target}: {asm}"));
            assert!(removed_slot.contains("__rt_heap_alloc"), "{target}: {removed_slot}");
            assert!(!removed_slot.contains("__rt_mixed_from_value"), "{target}: {removed_slot}");
        }
    }
}

/// Nullable integer keys use their inline tag for hash reads and probes on every ABI.
#[test]
fn nullable_integer_hash_keys_lower_on_every_target() {
    let source = r#"<?php
function nullableKeyProbe(array $items, ?int $key): bool { return isset($items[$key]); }
function nullableKeyRead(array $items, ?int $key): mixed { return $items[$key]; }
$items = ['' => 9, 0 => 10];
echo nullableKeyProbe($items, $argc > 1 ? null : 0), nullableKeyRead($items, null);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("tagged_hash_key_null"), "{target}");
    }
}

/// Scalar and descriptor writes stamp the post-COW array returned by the shared word helpers.
#[test]
fn indexed_array_word_writes_preserve_semantic_tags_on_every_target() {
    use crate::codegen::platform::Target;
    use std::path::Path;

    let source = r#"<?php
function appendFloatMetadata(float $value): array { $items = []; $items[] = $value; return $items; }
function appendBoolMetadata(bool $value): array { $items = []; $items[] = $value; return $items; }
function appendCallableMetadata(callable $value): array { $items = []; $items[] = $value; return $items; }
function setFloatMetadata(float $value): array { $items = []; $items[0] = $value; return $items; }
function metadataIdentity(int $value): int { return $value; }
echo count(appendFloatMetadata($argc / 2.0));
echo count(appendBoolMetadata($argc > 0));
echo count(appendCallableMetadata(metadataIdentity(...)));
echo count(setFloatMetadata($argc / 4.0));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let lines = assembly.lines().collect::<Vec<_>>();
        for (helper, tag) in [
            ("__rt_array_push_int", 2), ("__rt_array_push_int", 3),
            ("__rt_array_push_int", 10), ("__rt_array_set_int", 2),
        ] {
            let stamp = if name == "linux-x86_64" {
                format!("mov r12, {tag}")
            } else {
                format!("mov x11, #{tag}")
            };
            assert!(lines.iter().enumerate().any(|(index, line)| {
                line.contains(helper) && lines.iter().skip(index + 1).take(12)
                    .any(|line| line.contains(&stamp))
            }), "{name}: missing tag {tag} after {helper}");
        }
    }
}

/// Generic array spreads use an owned hash boundary instead of raw array operations on boxed cells.
#[test]
fn php_array_literal_spreads_use_typed_hash_boundary_on_every_target() {
    use crate::codegen::platform::Target;
    use std::path::Path;

    let source = r#"<?php
function spreadPhpArray(array $items): array { return [0, ...$items, "tail"]; }
function spreadMixedArray(mixed $items): array { return [...$items]; }
class SpreadPhpArraySource {
    public function none(): array { return []; }
    public function combined(): array { $local = []; return [...$local, ...$this->none()]; }
}
$spread = function(array $items, mixed $tail) { return [...$items, $tail]; };
echo count(spreadPhpArray([$argc]));
echo count(spreadMixedArray(["key" => $argc]));
echo count($spread(["key" => $argc], "last"));
echo count((new SpreadPhpArraySource())->combined());
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let ir = print_module(&module);
        assert!(ir.contains("array.unpack_to_hash"), "{name}: {ir}");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains("__rt_mixed_clone"), "{name}");
        assert!(assembly.contains("__rt_mixed_cell_promote_to_hash"), "{name}");
        assert!(assembly.contains("__rt_hash_spread"), "{name}");
        assert!(assembly.contains("__rt_decref_mixed"), "{name}");
    }
}

/// Boxed pop and shift publish separated receivers before removal on every supported target.
#[test]
fn php_array_pop_shift_use_boxed_receiver_helpers_on_every_target() {
    use crate::codegen::platform::Target;
    use std::path::Path;

    let source = r#"<?php
function takeArrayEdges(array &$items): void {
    $pop = array_pop(...);
    echo $pop($items), ':', array_shift(array: $items);
}
$items = [1, 2, 3];
takeArrayEdges($items);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.matches("__rt_array_cell_ensure_unique").count() >= 2, "{name}");
        assert!(assembly.matches("__rt_array_take_boxed").count() >= 2, "{name}");
    }
}

/// Boxed callback resolution, null identity and descriptor cleanup are emitted for all supported ABIs.
#[test]
fn php_array_map_boxed_callbacks_use_owned_descriptor_envs_on_every_target() {
    use crate::codegen::platform::Target;
    use std::path::Path;

    let source = r#"<?php
function callbacksForMap(string $prefix): array {
    return [function(string $name) use ($prefix): string { return $prefix . $name; }];
}
function mapDynamicCallback(mixed $callback, array $items): array {
    return array_map($callback, $items);
}
$callbacks = callbacksForMap("old");
$callback = $callbacks[0];
echo array_map($callback, ["Ada"])[0];
echo mapDynamicCallback(null, ["key" => "kept"])["key"];
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains("__rt_cleanup_call_operand_descriptor"), "{name}");
        assert!(assembly.contains("__rt_array_map_boxed"), "{name}");
        assert!(assembly.contains("array_map_null_callback"), "{name}");
        assert!(assembly.contains("array_map(): Argument #1 ($callback) must be a valid callback or null"), "{name}");
        let env = if name == "linux-x86_64" { "lea rdx, [rsp + 48]" } else { "add x2, x2, #48" };
        assert!(assembly.contains(env), "{name}: descriptor environment excludes its cleanup record");
    }
}

/// Rebinding declared arrays keeps boxed storage for later native reads and reference writeback.
#[test]
fn php_array_reassignment_keeps_the_declared_storage_contract() {
    use crate::codegen::platform::Target;
    use crate::ir::Op;
    use crate::types::PhpType;
    use std::path::Path;

    let source = r#"<?php
function replaceArrayValue(array $items): array {
    $items = ["A", "B"];
    return $items;
}
function replaceArrayReference(array &$items): void { $items = ["C", "D"]; }
$items = replaceArrayValue([$argc]);
replaceArrayReference($items);
echo implode(",", $items);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let function = module.functions.iter().find(|function| {
            function.name.as_str() == "replaceArrayReference"
        }).expect("reference function");
        let store = function.instructions.iter().find(|inst| inst.op == Op::StoreRefCell)
            .expect("boxed reference store");
        assert_eq!(store.result_php_type.codegen_repr(), PhpType::Mixed, "{name}");
        assert!(function.instructions.iter().any(|inst| inst.op == Op::MixedBox), "{name}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Native declared-array removal reaches the rooted boxed storage path on all supported ABIs.
#[test]
fn php_array_unset_uses_installed_sparse_storage_on_every_target() {
    use crate::codegen::platform::Target;
    use crate::ir::{Effects, Op};
    use std::path::Path;

    let source = r#"<?php
function removeDeclaredArrayOffset(array &$items): void { unset($items[1]); }
$items = [10, 20, 30];
removeDeclaredArrayOffset($items);
echo count($items);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let operations = module.functions.iter().flat_map(|function| &function.instructions)
            .filter(|instruction| instruction.op == Op::OffsetUnset).collect::<Vec<_>>();
        assert_eq!(operations.len(), 1, "{name}");
        assert!(operations[0].effects.contains(Effects::MAY_THROW | Effects::ALLOC_HEAP));
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let promote = assembly.find("__rt_mixed_cell_promote_to_hash").unwrap();
        let remove = assembly.find("__rt_hash_unset").unwrap();
        assert!(promote < remove, "publish sparse storage before removal on {name}");
    }
}

/// Declared PHP array sources reach boxed map traversal for direct and descriptor callbacks on all ABIs.
#[test]
fn php_array_map_uses_boxed_traversal_on_every_target() {
    use crate::codegen::platform::Target;
    use std::path::Path;

    let source = r#"<?php
function mapPhpArray(array $items, callable $callback): array { return array_map($callback, $items); }
function directMapPhpArray(array $items): array { return array_map(fn(mixed $value): mixed => $value, $items); }
function labelPhpArrayValue(mixed $value): string { return "v:" . $value; }
echo count(mapPhpArray([$argc], labelPhpArrayValue(...)));
echo count(directMapPhpArray(["key" => $argc]));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.matches("__rt_array_map_boxed").count() >= 2, "{name}");
    }
}

/// Mixed/concrete operand pairs in either order and two boxed sources share the merge ABI on all targets.
#[test]
fn php_array_merge_uses_boxed_result_storage_on_every_target() {
    use crate::codegen::platform::Target;
    use crate::ir::{Immediate, RuntimeCallTarget, RuntimeFnId};
    use crate::types::PhpType;
    use std::path::Path;

    let source = r#"<?php
function appendPhpArray(array $items): array { return array_merge($items, ["tail"]); }
function prependPhpArray(array $items): array { return array_merge(["head" => 1], $items); }
function mergePhpArrays(array $left, array $right): array { return array_merge($left, $right); }
echo count(appendPhpArray([$argc]));
echo count(prependPhpArray(["key" => $argc]));
echo count(mergePhpArrays([$argc], ["key" => $argc]));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let mut calls = 0;
        for function in &module.functions {
            for instruction in &function.instructions {
                if matches!(instruction.immediate,
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::ArrayMerge)))
                    | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
                        target: RuntimeFnId::ArrayMerge, ..
                    })))
                {
                    calls += 1;
                    assert_eq!(instruction.result_php_type.codegen_repr(), PhpType::Mixed, "{name}");
                }
            }
        }
        assert_eq!(calls, 3, "{name}");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.matches("__rt_array_merge_boxed").count() >= 3, "{name}");
    }
}

/// Variadic merge descriptors validate their pack and emit exactly two boxed backend operands.
#[test]
fn php_array_merge_callable_unpacks_backend_operands_on_every_target() {
    use crate::codegen::platform::Target;
    use crate::ir::{Immediate, RuntimeCallTarget, RuntimeFnId};
    use std::path::Path;

    let source = r#"<?php
function mergeDescriptor(callable $callback, array $left, array $right): mixed {
    return $callback($left, $right);
}
$callback = array_merge(...);
echo count(mergeDescriptor($callback, [$argc], ["key" => $argc]));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let signature = crate::types::first_class_callable_builtin_sig("array_merge").unwrap();
        let wrapper = crate::ir_lower::lower_array_merge_callable(
            &mut module, "test_merge_wrapper", &signature, false,
        );
        crate::ir::validate_function(&wrapper).unwrap();
        let merges = wrapper.instructions.iter().filter(|instruction| matches!(
            instruction.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::ArrayMerge)))
            | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
                target: RuntimeFnId::ArrayMerge, ..
            }))
        )).collect::<Vec<_>>();
        assert_eq!(merges.len(), 1, "{name}");
        assert_eq!(merges[0].operands.len(), 2, "{name}");
        assert_eq!(merges[0].result_php_type.codegen_repr(), crate::types::PhpType::Mixed, "{name}");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__rt_array_merge_boxed"), "{name}");
        assert!(assembly.contains("array_merge() takes exactly 2 arguments"), "{name}");
    }
}

/// Runtime key-preservation flags on PHP array declarations use the boxed reversal ABI on all targets.
#[test]
fn php_array_reverse_uses_boxed_result_storage_on_every_target() {
    use crate::codegen::platform::Target;
    use crate::ir::{Immediate, RuntimeCallTarget, RuntimeFnId};
    use crate::types::PhpType;
    use std::path::Path;

    let source = r#"<?php
function reversePhpArray(array $items, bool $preserve): array {
    return array_reverse($items, preserve_keys: $preserve);
}
echo count(reversePhpArray([$argc], $argc > 1));
echo count(reversePhpArray(["key" => $argc], $argc > 1));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let mut calls = 0;
        for function in &module.functions {
            for instruction in &function.instructions {
                if matches!(instruction.immediate,
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::ArrayReverse)))
                    | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
                        target: RuntimeFnId::ArrayReverse, ..
                    })))
                {
                    calls += 1;
                    assert_eq!(instruction.result_php_type.codegen_repr(), PhpType::Mixed, "{name}");
                }
            }
        }
        assert!(calls > 0, "{name}");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains("__rt_array_reverse_boxed"), "{name}");
    }
}

/// Boxed key sorts publish unique hash payloads before relinking on every supported target.
#[test]
fn boxed_array_key_sorts_keep_cell_types_on_every_target() {
    use crate::codegen::platform::Target;
    use std::path::Path;

    let source = r#"<?php
function orderBoxedKeys(array &$items): void { krsort($items); ksort($items); }
$items = [$argc, 2];
orderBoxedKeys($items);
echo implode(',', array_keys($items));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains("__rt_hash_ksort"), "{name}");
        assert!(assembly.contains("__rt_hash_krsort"), "{name}");
        assert!(assembly.matches("__rt_array_cell_ensure_unique").count() >= 2, "{name}");
        assert!(assembly.matches("__rt_mixed_cell_promote_to_hash").count() >= 2, "{name}");
    }
}

/// Boxed user sorts use a private working array and an exception finalizer on every target.
#[test]
fn boxed_usort_publishes_private_arrays_on_every_target() {
    use crate::codegen::platform::Target;
    use crate::ir::{Immediate, LocalKind, Op, RuntimeCallTarget, RuntimeFnId, ValueDef};
    use crate::types::PhpType;
    use std::path::Path;

    let source = r#"<?php
class TargetSortBag { public array $items = ['b' => 2, 'a' => 1]; }
function targetBoxedSort(array &$items): void { usort($items, fn(int $a, int $b): int => $a <=> $b); }
function dynamicBoxedSort(callable $sort, array &$items): void {
    $sort($items, fn(int $a, int $b): int => $a <=> $b);
}
$bag = new TargetSortBag();
$items = [2, 1];
targetBoxedSort($items);
$sort = usort(...);
dynamicBoxedSort($sort, $items);
usort($bag->items, fn(int $a, int $b): int => $b <=> $a);
usort(callback: fn(int $a, int $b): int => $a <=> $b, array: $bag->items);
echo implode(',', $bag->items);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let mut sorts = 0;
        let mut rooted_references = 0;
        for function in &module.functions {
            for inst in &function.instructions {
                if inst.op == Op::LoadPropRefCell {
                    let receiver = inst.operands[0];
                    let ValueDef::Instruction { inst: producer, .. } = function.value(receiver).unwrap().def else {
                        continue;
                    };
                    let producer = &function.instructions[producer.as_raw() as usize];
                    if producer.op == Op::LoadLocal {
                        if let Some(Immediate::LocalSlot(slot)) = producer.immediate {
                            if function.locals[slot.as_raw() as usize].kind == LocalKind::HiddenTemp {
                                rooted_references += 1;
                                assert!(!function.instructions.iter().any(|candidate| {
                                    candidate.op == Op::Release && candidate.operands == [receiver]
                                }), "{name}: the reference capture borrows its rooted object receiver");
                            }
                        }
                    }
                }
                if matches!(inst.immediate,
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::Usort)
                        | RuntimeCallTarget::ProfiledFunction { target: RuntimeFnId::Usort, .. })))
                {
                    sorts += 1;
                    let array = function.value(inst.operands[0]).unwrap();
                    assert_eq!(array.php_type, PhpType::Array(Box::new(PhpType::Mixed)),
                        "{name}: positional and named sorts must consume a dense working array");
                }
            }
        }
        assert!(sorts >= 3, "{name}: both planner forms reach the private-array lowering");
        assert!(rooted_references >= 2, "{name}: both property sorts capture a rooted reference");
        let signature = crate::builtins::registry::first_class_callable_sig("usort").unwrap();
        let wrapper = crate::ir_lower::lower_boxed_usort_callable(
            &mut module.clone(), "boxed_usort_probe", &signature, false,
        );
        crate::ir::validate_function(&wrapper).unwrap();
        assert!(wrapper.instructions.iter().any(|inst| inst.op == Op::TryPushHandler), "{name}");
        assert!(wrapper.instructions.iter().any(|inst| inst.op == Op::CatchBind), "{name}");
        let wrapper_sorts = wrapper.instructions.iter().filter(|inst| matches!(inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::Usort)
                | RuntimeCallTarget::ProfiledFunction { target: RuntimeFnId::Usort, .. }))
        )).collect::<Vec<_>>();
        assert_eq!(wrapper_sorts.len(), 1, "{name}");
        assert_eq!(wrapper.value(wrapper_sorts[0].operands[0]).unwrap().php_type,
            PhpType::Array(Box::new(PhpType::Mixed)), "{name}");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains("__rt_usort"), "{name}");
        assert!(assembly.contains("__rt_array_ensure_unique"), "{name}");
        assert!(assembly.contains("usort(): Argument #1 ($array) must be of type array"), "{name}");
    }
}

/// Boxed property push validates and separates its receiver before appending on every target.
#[test]
fn boxed_array_push_properties_publish_separated_cells_on_every_target() {
    use crate::codegen::platform::Target;
    use std::path::Path;

    let source = r#"<?php
class BoxedPushEmitter {
    public array $items = [1];
    public static array $shared = [2];
}
$owner = new BoxedPushEmitter();
array_push($owner->items, $argc);
array_push(BoxedPushEmitter::$shared, $argc);
echo count($owner->items), count(BoxedPushEmitter::$shared);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(assembly.matches("__rt_array_cell_ensure_unique").count(), 2, "{name}");
        assert_eq!(assembly.matches("__rt_mixed_array_append").count(), 2, "{name}");
    }
}

/// Every target boxes scalar and nested array slots before exposing addresses to boxed ref parameters.
#[test]
fn php_array_reference_elements_use_boxed_parent_slots_on_every_target() {
    use crate::codegen::platform::Target;
    use crate::ir::{IrType, Op, Ownership};
    use crate::types::PhpType;
    use std::path::Path;

    let source = r#"<?php
function mutateNestedPhpArray(array &$items): void { $items["key"] = 7; }
function replaceScalarArraySlot(mixed &$value): void { $value = "updated"; }
$items = [[$argc]];
mutateNestedPhpArray($items[0]);
echo count($items[0]);
$scalars = [$argc];
replaceScalarArraySlot($scalars[0]);
echo $scalars[0];
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let mut addresses = 0;
        for function in &module.functions {
            for instruction in &function.instructions {
                if instruction.op == Op::ArrayElemAddr {
                    addresses += 1;
                    assert_eq!(instruction.result_php_type, PhpType::Pointer(None), "{name}");
                    assert_eq!(instruction.result_type, IrType::I64, "{name}");
                    assert_eq!(instruction.result_ownership, Ownership::NonHeap, "{name}");
                    let receiver = function.value(instruction.operands[0]).unwrap();
                    assert_eq!(receiver.php_type.codegen_repr(), PhpType::Array(Box::new(PhpType::Mixed)), "{name}");
                }
            }
        }
        assert!(addresses >= 2, "{name}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Boxed PHP array value extraction reaches both physical layouts on all supported targets.
#[test]
fn php_array_values_emit_packed_and_hash_paths_for_every_target() {
    use crate::codegen::platform::Target;
    use std::path::Path;

    let source = r#"<?php
function projectPhpArray(array $items): array { return array_values($items); }
echo count(projectPhpArray([$argc])), count(projectPhpArray(["key" => $argc]));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains("__rt_mixed_unbox"), "{name}");
        assert!(assembly.contains("__rt_array_to_mixed"), "{name}");
        assert!(assembly.contains("__rt_hash_iter_next"), "{name}");
    }
}

/// Boxed array property writes separate storage and keep the property's cell borrowed on every ABI.
#[test]
fn php_array_property_mutations_use_borrowed_separated_cells() {
    use crate::codegen::platform::Target;
    use crate::ir::{Op, Ownership};
    use std::path::Path;

    let source = r#"<?php
class PhpArrayWrites {
    public array $items = [1];
    public static array $shared = [2];
    public function write(int $value): void {
        $this->items[] = $value;
        $this->items["key"] = $value;
        self::$shared[] = $value;
        self::$shared["key"] = $value;
    }
}
$owner = new PhpArrayWrites();
$owner->write($argc);
echo count($owner->items), count(PhpArrayWrites::$shared);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let instructions = module.functions.iter().chain(&module.class_methods)
            .flat_map(|function| &function.instructions)
            .collect::<Vec<_>>();
        let fetches = instructions.iter().filter(|inst| inst.op == Op::PropGetForWrite)
            .collect::<Vec<_>>();
        assert_eq!(fetches.len(), 2, "{name}");
        assert!(fetches.iter().all(|inst| inst.result_ownership == Ownership::Borrowed), "{name}");
        assert_eq!(instructions.iter().filter(|inst| inst.op == Op::MixedArrayAppend).count(), 2, "{name}");
        assert_eq!(instructions.iter().filter(|inst| inst.op == Op::MixedClone).count(), 2, "{name}");
        // Typed receivers must also reach a supported emitter, including on both iOS targets.
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// PHP array declarations retain packed-or-hash boxed storage across defaults and call sites.
#[test]
fn php_array_declarations_share_boxed_parameter_property_and_return_storage() {
    use crate::codegen::platform::Target;
    use crate::types::PhpType;
    use std::path::Path;

    let source = r#"<?php
class PhpArrayShape {
    public array $items = [1, 2];
    public static array $shared = [3, 4];
    public function __construct(array $items) { $this->items = $items; }
    public function replace(array $items): array { $this->items = $items; return $this->items; }
}
function keepPhpArray(array $items): array { return $items; }
$owner = new PhpArrayShape([$argc]);
echo count($owner->replace(keepPhpArray(["key" => $argc])));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let class = &module.class_infos["PhpArrayShape"];
        for (_, ty) in class.properties.iter().chain(&class.static_properties) {
            assert!(ty.is_php_array(), "{name}: {ty:?}");
            assert_eq!(ty.codegen_repr(), PhpType::Mixed, "{name}");
        }
        for method in ["__construct", "replace"] {
            assert!(class.methods[method].params[0].1.is_php_array(), "{name}:{method}");
        }
        assert!(class.methods["replace"].return_type.is_php_array(), "{name}");
        let function = module.functions.iter().find(|function| function.name.eq_ignore_ascii_case("keepPhpArray")).unwrap();
        assert!(function.params[0].php_type.is_php_array(), "{name}");
    }
}

/// Class-method inventories use constant-size result construction EIR on all supported targets.
#[test]
fn class_method_inventory_does_not_expand_one_push_per_name() {
    use crate::codegen::platform::Target;
    use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};
    use std::path::Path;

    let methods = (0..40).map(|index| format!("public function method{index}(): void {{}}"))
        .collect::<Vec<_>>().join("\n");
    let source = format!("<?php class CompactMethods {{ {methods} }} echo count(get_class_methods(CompactMethods::class));");
    let expected = (0..40).map(|index| format!("method{index}"))
        .collect::<Vec<_>>().join("\0");
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            &source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let main = module.functions.iter().find(|function| function.name == "main").unwrap();
        assert!(!main.instructions.iter().any(|inst| inst.op == Op::ArrayPush), "{name}");
        assert_eq!(main.instructions.iter().filter(|inst| matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::Explode)))
        )).count(), 1, "{name}");
        assert!(module.data.strings.contains(&expected), "{name}");
    }
}

/// Verifies indexed array access preserves string and float element metadata.
/// The indices are runtime-unknown (`$argc`) so the accesses survive AST-level
/// array-fact propagation, which folds constant-index reads of literal-backed
/// locals before lowering.
#[test]
fn indexed_array_access_uses_array_element_type() {
    let module = super::lower_source(
        r#"<?php
$strings = ["a", "b"];
echo $strings[$argc];
$floats = [1.5, 2.5];
echo $floats[$argc];
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains(": Str php=string own=maybe_owned = array_get"),
        "missing string array_get metadata in {text}"
    );
    assert!(
        text.contains(": F64 php=float = array_get"),
        "missing float array_get metadata in {text}"
    );
}

/// An array local RE-BOUND to an incompatible type inside an array-representation fixed-point
/// region keeps its old slot at the old representation, and the new value gets a fresh one.
///
/// The ternary makes this statement a conversion-HIDING region, so `lower_region_at_type_fixpoint`
/// runs its speculative discovery pass over it while `$a` is still a convertible `array<int>`
/// candidate. Two things have to come out of that:
///
/// - nothing is canonicalized at the region entry. Canonicalization emits `Op::ArrayToMixed` /
///   `Op::ArrayToHash` against the binding live at region ENTRY, and this region ENDS that
///   binding; a conversion belonging to the fresh slot must not be hoisted onto the old one.
/// - the old slot is not widened. The rebind releases and nulls it through the ordinary overwrite
///   path, and that path widens the slot to whatever type it stores — a whole-frame property, so
///   nulling at `Void` would re-type every load of that slot the body already lowered.
#[test]
fn retype_in_a_conversion_region_rebinds_without_widening_the_old_slot() {
    let module = super::lower_source(
        r#"<?php
$a = [1, $argc];
$a = $argc > 0 ? "s" . $argc : "t";
echo $a;
"#,
    );
    let text = print_module(&module);
    assert!(
        !text.contains("array_to_mixed") && !text.contains("array_to_hash"),
        "the fixed point canonicalized a binding the region re-binds: {text}"
    );
    let main = module.functions.iter().find(|function| function.flags.is_main).unwrap();
    let retired = main.instructions.iter().find(|inst| inst.op == crate::ir::Op::ReleaseLocalSlot)
        .expect("rebinding must retire the old array owner atomically");
    let Some(crate::ir::Immediate::LocalSlot(slot)) = retired.immediate else {
        panic!("the retirement must identify the abandoned local slot");
    };
    let local = main.locals.iter().find(|local| local.id == slot).unwrap();
    assert_eq!(local.php_type, crate::types::PhpType::Array(Box::new(crate::types::PhpType::Int)),
        "the abandoned slot lost its concrete array<int> storage type: {text}");
    assert!(!main.instructions.iter().any(|inst| {
        inst.op == crate::ir::Op::StoreLocal
            && inst.immediate == Some(crate::ir::Immediate::LocalSlot(slot))
            && main.value(inst.operands[0]).unwrap().php_type == crate::types::PhpType::Str
    }), "the re-bound string was stored through the old slot instead of a fresh one: {text}");
}

/// A local the checker marked as branch-divergently assigned gets a boxed `mixed` slot before its
/// FIRST store and keeps it through the array-representation fixed point.
///
/// The `if` here is a conversion-hiding region (`$b[0] = "x"` promotes `$b` to `array<mixed>`), so
/// `lower_region_at_type_fixpoint` lowers the whole statement SPECULATIVELY, reads the discovered
/// conversions, rolls everything back and lowers it again. `$a`'s first store — and therefore the
/// `declare_local(.., Mixed)` that precedes it — happens inside that region, so this is the shape
/// that pins the marked slot's stability under the pass:
///
/// - the marked name is never a canonicalization CANDIDATE: `convertible_array_locals` only picks
///   names whose `local_types` entry is an `Array(_)`, and a marked name's is `Mixed` from its
///   first store onwards, so no `Op::ArrayToMixed`/`Op::ArrayToHash` is ever hoisted onto it;
/// - the rollback (`LoweringContext::restore`) puts back `local_slots`, `local_types` and the whole
///   function, so the discarded pass leaves no slot behind and the real pass re-declares the same
///   `Mixed` one from the same recorded span;
/// - and no later store can narrow it: `widened_local_storage_type` answers `Mixed` for every
///   `(Mixed, other)` pair, so the `int` arm's store widens nothing.
///
/// Both arms therefore read slot 2 as `mixed`, which is what makes the two dynamic outcomes share
/// one binding.
#[test]
fn mixed_storage_local_keeps_its_boxed_slot_through_the_fixed_point() {
    let module = super::lower_source(
        r#"<?php
$b = [1, $argc];
if ($argc > 1) { $a = 0; $b[0] = "x"; } else { $a = "ciao"; }
echo $a, "|", $b[0];
"#,
    );
    let text = print_module(&module);
    assert_eq!(
        text.matches("php=mixed own=maybe_owned = load_local slot[2]")
            .count(),
        2,
        "both arms must read the marked local out of one boxed slot: {text}"
    );
    assert!(
        !text.contains("php=int = load_local slot[2]")
            && !text.contains("php=string own=maybe_owned = load_local slot[2]"),
        "the marked local's slot was narrowed to a concrete representation: {text}"
    );
    assert_eq!(
        text.matches("array_to_mixed").count(),
        1,
        "exactly one conversion belongs here — `$b`'s, hoisted to the region entry; a second one \
         would mean the marked local was canonicalized as if it were a convertible array: {text}"
    );
}
