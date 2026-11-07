//! Purpose:
//! Checks declaration callback EIR, borrowed inputs and native entry shape on every target.
//!
//! Called from:
//! - Supported-target AST-to-EIR tests in CI.
//!
//! Key details:
//! - Instance references and static declarations use the same boxed-candidate operation.
//! - PHP defaults and backtrace collectors cannot change the callback's two-argument C ABI.

use crate::ir::{Immediate, Op, Ownership, RuntimeCallTarget};

/// Generates scalar, nullable, literal-false and union validators without retaining either input.
fn verify_native_reference_validator_contract(target: &str) {
    let source = r#"<?php
class ReferenceValidatorShapes {
    public int $integer = 1;
    public float $floating = 1.0;
    public bool $boolean = false;
    public string $text = 'before';
    public ?int $nullable = null;
    public ?false $optional = null;
    public int|float|bool $numeric = 0;
    public static string $label = 'label';
}
$object = new ReferenceValidatorShapes();
$a = &$object->integer; $b = &$object->floating; $c = &$object->boolean;
$d = &$object->text; $e = &$object->nullable; $f = &$object->optional;
$g = &$object->numeric;
echo $a, $b, $c, $d, $e, $f, $g, ReferenceValidatorShapes::$label;
debug_backtrace();
"#;
    let mut module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    assert!(!module.required_runtime_features.native_reference_validation);
    assert!(!module.functions.iter().any(|function|
        function.name.starts_with("__eir_reference_property_")),
        "{target}: ordinary lowering must not generate unused validators");
    crate::ir_lower::enable_native_reference_validation(&mut module).unwrap();
    assert!(module.required_runtime_features.native_reference_validation);
    let function_count = module.functions.len();
    crate::ir_lower::enable_native_reference_validation(&mut module).unwrap();
    assert_eq!(module.functions.len(), function_count, "{target}: capability selection is idempotent");
    let callbacks = module.functions.iter().filter(|function| function.instructions.iter().any(|inst|
        matches!(inst.immediate, Some(Immediate::RuntimeCall(
            RuntimeCallTarget::ReferencePropertyCoerce { .. }))))).collect::<Vec<_>>();
    assert_eq!(callbacks.len(), 8, "{target}: seven instance declarations and one static");
    for callback in callbacks {
        assert!(callback.flags.is_native_reference_validator, "{target}: typed native entry role");
        assert_eq!(callback.params.len(), 2, "{target}: native ABI, without hidden PHP parameters");
        assert!(callback.signature.is_none(), "{target}: no PHP argument normalization");
        assert_eq!(callback.no_epilogue_cleanup_slots.len(), 2, "{target}: borrowed inputs");
        for load in callback.instructions.iter().filter(|inst| inst.op == Op::LoadLocal) {
            let value = load.result.expect("parameter load has a result");
            let expected = if load.result_php_type == crate::types::PhpType::Int {
                Ownership::NonHeap
            } else { Ownership::Borrowed };
            assert_eq!(callback.values[value.as_raw() as usize].ownership, expected);
        }
        let coercion = callback.instructions.iter().find(|inst| inst.op == Op::RuntimeCall).unwrap();
        let value = coercion.result.unwrap();
        assert_eq!(callback.values[value.as_raw() as usize].ownership, Ownership::Owned);
    }
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    assert!(assembly.contains("reference_validator_weak"), "{target}: assignment-site mode dispatch");
    assert!(assembly.contains("__rt_mixed_cast_float"), "{target}: boxed float candidates");
    let printed = crate::ir::print_module(&module);
    assert!(printed.contains("runtime.reference.property_coerce class="));
}

supported_target_cases!(native_reference_validator_contract,
    verify_native_reference_validator_contract);

/// Native declarations also retain array, iterable, object and nullable union constraints.
fn verify_native_reference_container_validator_contract(target: &str) {
    let source = r#"<?php
class NativeContainerBase {}
class NativeContainerChild extends NativeContainerBase {}
class NativeContainerDeclarations {
    public array $values = [];
    public iterable $items = [];
    public NativeContainerBase $item;
    public object $any;
    public NativeContainerBase|array|null $choice = null;
    public static ?NativeContainerBase $shared = null;
}
$owner = new NativeContainerDeclarations();
$a =& $owner->values; $b =& $owner->items; $c =& $owner->item;
$d =& $owner->any; $e =& $owner->choice;
echo NativeContainerDeclarations::$shared;
"#;
    let mut module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    crate::ir_lower::enable_native_reference_validation(&mut module).unwrap();
    let owner = module.class_infos["NativeContainerDeclarations"].class_id;
    let declarations = module.functions.iter().flat_map(|function| function.instructions.iter())
        .filter_map(|inst| match inst.immediate {
            Some(Immediate::RuntimeCall(RuntimeCallTarget::ReferencePropertyCoerce {
                class_id, slot, is_static,
            })) if class_id == owner => Some((slot, is_static)),
            _ => None,
        }).collect::<std::collections::BTreeSet<_>>();
    assert_eq!(declarations, [(0, false), (1, false), (2, false), (3, false),
        (4, false), (0, true)].into_iter().collect(),
        "{target}: every physical PHP constraint has its own native declaration callback");
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    assert!(assembly.contains("reference_validator_weak"), "{target}: assignment-site strictness remains explicit");
    assert!(assembly.contains("NativeContainerBase"), "{target}: diagnostics retain the declared class");
}

supported_target_cases!(native_reference_container_validator_contract,
    verify_native_reference_container_validator_contract);


/// Explicit mixed declarations preserve original candidates; untyped properties add no constraint.
fn verify_native_reference_mixed_declaration_contract(target: &str) {
    let source = r#"<?php
class NativeExplicitMixedSource {
    public mixed $declared = null;
    public $untyped;
    public static mixed $shared = null;
}
$owner = new NativeExplicitMixedSource();
$typed =& $owner->declared; $untyped =& $owner->untyped;
"#;
    let mut module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    crate::ir_lower::enable_native_reference_validation(&mut module).unwrap();
    let id = module.class_infos["NativeExplicitMixedSource"].class_id;
    let declarations = module.functions.iter().flat_map(|function| function.instructions.iter())
        .filter_map(|inst| match inst.immediate {
            Some(Immediate::RuntimeCall(RuntimeCallTarget::ReferencePropertyCoerce {
                class_id, slot, is_static,
            })) if class_id == id => Some((slot, is_static)),
            _ => None,
        }).collect::<std::collections::BTreeSet<_>>();
    assert_eq!(declarations, [(0, false), (0, true)].into_iter().collect(),
        "{target}: mixed is declared, but an untyped slot is unconstrained");
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    for is_static in [false, true] {
        let name = crate::ir_lower::reference_property_validators::callback_name(id, 0, is_static);
        let body = assembly.split_once(&format!("@fn name={name} symbol=")).unwrap().1
            .split_once(&format!("@endfn name={name}")).unwrap().0;
        assert!(body.contains("__rt_incref"), "{target}: retain the unchanged borrowed candidate");
        assert!(!body.contains("__rt_mixed_cast_"), "{target}: mixed never coerces another source's RHS");
    }
}

supported_target_cases!(native_reference_mixed_declaration_contract, verify_native_reference_mixed_declaration_contract);
