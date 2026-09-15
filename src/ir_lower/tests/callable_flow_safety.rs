//! Purpose:
//! Structural regressions for callable target joins, descriptor spreads, and descriptor-only
//! Traversable argument specialization.
//!
//! Called from:
//! - `cargo test` through the AST-to-EIR unit test module.
//!
//! Key details:
//! - A checker-approved `Callable` parameter operand must already be a descriptor in EIR.
//! - Descriptor invocations may walk Traversable spreads without direct-call specialization.
//! - Container cleanup reaches user code only when its stored values can own objects or closures.

/// Typed scalar containers cannot run PHP cleanup hooks, while nested object and callable
/// containers can release user-observable owners.
#[test]
fn callable_fact_cleanup_classification_inspects_container_values() {
    use crate::types::PhpType;

    let scalar_indexed = PhpType::Array(Box::new(PhpType::Int));
    let scalar_hash = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Int),
    };
    let nested_object = PhpType::Array(Box::new(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Object("CleanupProbe".to_string())),
    }));
    let nested_callable = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Array(Box::new(PhpType::Callable))),
    };

    assert!(!super::super::context::php_type_cleanup_may_invoke_user_code(
        &scalar_indexed,
    ));
    assert!(!super::super::context::php_type_cleanup_may_invoke_user_code(
        &scalar_hash,
    ));
    assert!(super::super::context::php_type_cleanup_may_invoke_user_code(
        &nested_object,
    ));
    assert!(super::super::context::php_type_cleanup_may_invoke_user_code(
        &nested_callable,
    ));
}

/// Identical static callable-array targets survive an `if` join as descriptor operands.
#[test]
fn matching_branch_callable_arrays_lower_to_descriptors_on_every_target() {
    use crate::ir::Op;
    use crate::types::PhpType;

    let source = r#"<?php
class JoinedCallableTarget { public static function hit(): int { return 7; } }
function consumeJoinedCallable(callable $callback): int { return $callback(); }
function invokeJoinedCallable(int $choice): int {
    if ($choice > 0) {
        $callback = [JoinedCallableTarget::class, 'hit'];
    } else {
        $callback = [JoinedCallableTarget::class, 'hit'];
    }
    return consumeJoinedCallable($callback);
}
echo invokeJoinedCallable($argc);
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "invokeJoinedCallable")
            .unwrap();
        let call = function
            .instructions
            .iter()
            .find(|instruction| instruction.op == Op::Call)
            .unwrap();
        assert_eq!(
            function.value(call.operands[0]).unwrap().php_type.codegen_repr(),
            PhpType::Callable,
            "{target}: the joined callable array must cross the parameter boundary as a descriptor",
        );
        assert!(
            function
                .instructions
                .iter()
                .any(|instruction| instruction.op == Op::FirstClassCallableNew),
            "{target}: the static callable-array target must materialize a descriptor",
        );
    }
}

/// An untouched instance callable keeps its one pre-split captured receiver after the join.
#[test]
fn preexisting_instance_callable_survives_an_untouched_join_on_every_target() {
    use crate::ir::Op;
    use crate::types::PhpType;

    let source = r#"<?php
class PrejoinedCallableTarget { public function hit(): int { return 7; } }
function consumePrejoinedCallable(callable $callback): int { return $callback(); }
function invokePrejoinedCallable(int $choice): int {
    $callback = [new PrejoinedCallableTarget(), 'hit'];
    if ($choice > 0) { $marker = 1; } else { $marker = 2; }
    return consumePrejoinedCallable($callback) + $marker;
}
echo invokePrejoinedCallable($argc);
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "invokePrejoinedCallable")
            .unwrap();
        let call = function
            .instructions
            .iter()
            .find(|instruction| instruction.op == Op::Call)
            .unwrap();
        assert_eq!(
            function.value(call.operands[0]).unwrap().php_type.codegen_repr(),
            PhpType::Callable,
            "{target}: the pre-split captured receiver must remain a descriptor source",
        );
    }
}

/// A dynamic spread of descriptor values projects a `Callable` operand, never a raw array.
#[test]
fn callable_descriptor_spreads_lower_callable_operands_on_every_target() {
    use crate::ir::Op;
    use crate::types::PhpType;

    let source = r#"<?php
class SpreadDescriptorTarget { public static function hit(): int { return 9; } }
function consumeSpreadDescriptor(callable $callback): int { return $callback(); }
function invokeSpreadDescriptor(array $callbacks): int {
    return consumeSpreadDescriptor(...$callbacks);
}
echo invokeSpreadDescriptor([SpreadDescriptorTarget::hit(...)]);
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "invokeSpreadDescriptor")
            .unwrap();
        let call = function
            .instructions
            .iter()
            .find(|instruction| {
                if instruction.op != Op::Call {
                    return false;
                }
                let Some(crate::ir::Immediate::Data(callee)) = instruction.immediate else {
                    return false;
                };
                crate::names::php_symbol_key(
                    module.data.function_names[callee.as_raw() as usize]
                        .trim_start_matches('\\'),
                ) == crate::names::php_symbol_key("consumeSpreadDescriptor")
            })
            .expect("the spread guard's exception constructor must not hide the target call");
        assert_eq!(
            function.value(call.operands[0]).unwrap().php_type.codegen_repr(),
            PhpType::Callable,
            "{target}: unpacking array<Callable> must keep descriptor storage",
        );
        assert!(
            function.instructions.iter().any(|instruction| {
                instruction.op == Op::MixedUnbox
                    && instruction.result_php_type.codegen_repr() == PhpType::Callable
            }),
            "{target}: a boxed spread projection must be validated as a descriptor",
        );
    }
}

/// A tracked instance callable array remains admissible at a resolved Callable boundary.
#[test]
fn tracked_instance_callable_array_crosses_resolved_function_boundary_on_every_target() {
    use crate::ir::Op;
    use crate::types::PhpType;

    let source = r#"<?php
class ResolvedArrayTarget { public function hit(): int { return 11; } }
function consumeResolvedArray(callable $callback): int { return $callback(); }
function invokeResolvedArray(): int {
    $callback = [new ResolvedArrayTarget(), 'hit'];
    return consumeResolvedArray($callback);
}
function invokeResolvedLiteral(): int {
    return consumeResolvedArray([new ResolvedArrayTarget(), 'hit']);
}
echo invokeResolvedArray(), invokeResolvedLiteral();
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["invokeResolvedArray", "invokeResolvedLiteral"] {
            let function = module
                .functions
                .iter()
                .find(|function| function.name == name)
                .unwrap();
            let call = function
                .instructions
                .iter()
                .find(|instruction| instruction.op == Op::Call)
                .unwrap();
            assert_eq!(
                function.value(call.operands[0]).unwrap().php_type.codegen_repr(),
                PhpType::Callable,
                "{target}/{name}: the proven callable array must materialize before the call",
            );
            assert!(
                function
                    .instructions
                    .iter()
                    .any(|instruction| instruction.op == Op::FirstClassCallableNew),
                "{target}/{name}: the instance target must become a descriptor",
            );
        }
    }
}

/// Untyped function and method FCC targets reach descriptor invocation with Traversable spreads.
#[test]
fn untyped_descriptor_targets_accept_traversable_spreads_on_every_target() {
    use crate::ir::Op;

    let source = r#"<?php
class DescriptorValues implements IteratorAggregate {
    public function getIterator(): Traversable { yield 5; }
}
function descriptorFunction($value): int { return $value; }
class DescriptorMethods {
    public function instanceValue($value): int { return $value; }
    public static function staticValue($value): int { return $value; }
}
function invokeDescriptorTargets(Traversable $values, DescriptorMethods $methods): int {
    return call_user_func(descriptorFunction(...), ...$values)
        + call_user_func($methods->instanceValue(...), ...$values)
        + call_user_func(DescriptorMethods::staticValue(...), ...$values);
}
echo invokeDescriptorTargets(new DescriptorValues(), new DescriptorMethods());
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "invokeDescriptorTargets")
            .unwrap();
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|instruction| instruction.op == Op::CallableDescriptorInvoke)
                .count(),
            3,
            "{target}: every untyped FCC target must use descriptor-aware Traversable unpacking",
        );
    }
}

/// Descriptor planning defers runtime spread keys and their projected value types to the binder.
#[test]
fn descriptor_spread_keys_and_object_values_remain_runtime_bound_on_every_target() {
    use crate::ir::Op;

    let source = r#"<?php
class DescriptorMarker {}
class NamedDescriptorValues implements IteratorAggregate {
    public function getIterator(): Traversable { yield 'left' => new DescriptorMarker(); }
}
function joinDescriptorMarkers(DescriptorMarker $left, DescriptorMarker $right): string {
    return 'joined';
}
function invokeNamedDescriptor(
    callable $callback,
    Traversable $values,
    DescriptorMarker $right,
): string {
    return call_user_func($callback, ...$values, right: $right);
}
echo invokeNamedDescriptor(
    joinDescriptorMarkers(...),
    new NamedDescriptorValues(),
    new DescriptorMarker(),
);
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "invokeNamedDescriptor")
            .unwrap();
        assert!(
            function
                .instructions
                .iter()
                .any(|instruction| instruction.op == Op::CallableDescriptorInvoke),
            "{target}: the runtime-keyed spread must reach the descriptor binder",
        );
        assert!(
            function
                .instructions
                .iter()
                .any(|instruction| instruction.op == Op::IterStart),
            "{target}: the Traversable source must retain its runtime key walk",
        );
    }
}
