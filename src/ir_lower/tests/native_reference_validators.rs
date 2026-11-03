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
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let callbacks = module.functions.iter().filter(|function| function.instructions.iter().any(|inst|
        matches!(inst.immediate, Some(Immediate::RuntimeCall(
            RuntimeCallTarget::ReferencePropertyCoerce { .. }))))).collect::<Vec<_>>();
    assert_eq!(callbacks.len(), 8, "{target}: seven instance declarations and one static");
    for callback in callbacks {
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
