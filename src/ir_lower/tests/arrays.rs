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

/// Every target boxes nested array slots before passing their real addresses to array ref parameters.
#[test]
fn php_array_reference_elements_use_boxed_parent_slots_on_every_target() {
    use crate::codegen::platform::Target;
    use crate::ir::Op;
    use crate::types::PhpType;
    use std::path::Path;

    let source = r#"<?php
function mutateNestedPhpArray(array &$items): void { $items["key"] = 7; }
$items = [[$argc]];
mutateNestedPhpArray($items[0]);
echo count($items[0]);
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
                    assert_eq!(instruction.result_php_type, PhpType::Mixed, "{name}");
                }
            }
        }
        assert!(addresses > 0, "{name}");
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
        let instructions = module.functions.iter().flat_map(|function| &function.instructions)
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
