//! Purpose:
//! Preserves assignment-site strictness and the original boxed RHS at native reference stores.
//!
//! Called from:
//! - The supported-target AST-to-EIR suite in CI.
//!
//! Key details:
//! - A parameter declaration describes the raw ABI, not the source assignment mode.
//! - The canonical publisher must see the original RHS before legacy scalar narrowing.

use crate::ir::{Immediate, Op, ValueDef};

/// Weak and strict source files carry explicit modes without unboxing the original assignment.
fn verify_native_reference_store_modes(target: &str) {
    for strict in [false, true] {
        let source = format!("<?php {} \
            function nativeModeStore(int &$value, mixed $original): void {{ $value = $original; }}",
            if strict { "declare(strict_types=1);" } else { "" });
        let module = super::lower_source_at_for_target(
            &source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "nativeModeStore").unwrap();
        let store = function.instructions.iter().find(|inst| inst.op == Op::StoreRefCell).unwrap();
        assert_eq!(store.operands.len(), 2, "{target}: original RHS and source mode");
        assert_eq!(function.value(store.operands[0]).unwrap().php_type, crate::types::PhpType::Mixed,
            "{target}: preserve the original RHS before raw integer coercion");
        let ValueDef::Instruction { inst, .. } = function.value(store.operands[1]).unwrap().def else {
            panic!("{target}: source mode must be an explicit constant");
        };
        let mode = function.instruction(inst).unwrap();
        assert_eq!(mode.op, Op::ConstBool);
        assert_eq!(mode.immediate, Some(Immediate::Bool(strict)), "{target}: assignment-site mode");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

supported_target_cases!(native_reference_store_modes, verify_native_reference_store_modes);
