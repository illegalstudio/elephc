//! Purpose:
//! Verifies destination view leases across throwing array assignment operands.
//!
//! Called from:
//! - All-target EIR regressions for late frame widening by opaque eval.
//!
//! Key details:
//! - The destination guard must precede the RHS call and stay outside nested key ownership.

/// Final Mixed frame storage gives an earlier destination capture its own unwind lease.
fn verify_array_write_pending_receiver_owners(target: &str) {
    use crate::ir::{Immediate, Op, RuntimeCallTarget};
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/strict/array_write_pending_owner.php"));
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    let main = module.functions.iter().find(|function| function.name == "main").unwrap();
    assert!(main.instructions.iter().any(|inst| matches!(inst.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned)))
        && inst.operands.first().is_some_and(|value| main.values[value.as_raw() as usize].php_type
            .codegen_repr() == crate::types::PhpType::Array(Box::new(crate::types::PhpType::Int)))),
        "{target}: the captured destination must be protected before a throwing RHS");
    assert!(main.instructions.iter().any(|inst| inst.op == Op::Call), "{target}: retain the throwing source call");
    crate::ir::validate_module(&module).unwrap();
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

supported_target_cases!(array_write_pending_receiver_owners, verify_array_write_pending_receiver_owners);
