//! Purpose:
//! Proves when an EIR callable already transfers a stable owned string result.
//!
//! Called from:
//! - Descriptor construction for native functions and closures.
//!
//! Key details:
//! - Borrowed parameters, literals and unknown producers retain the copying invoker path.
//! - Every return must establish ownership; one owned branch cannot justify another branch.

use crate::ir::{Function, Op, Terminator, ValueDef, ValueId};
use crate::types::PhpType;

/// Proves ownership for the exact EIR method implementation selected by a descriptor wrapper.
pub(in crate::codegen) fn method_returns_owned_string(
    module: &crate::ir::Module,
    impl_class: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    module.class_methods.iter().find(|function| {
        function.flags.is_static == is_static
            && function.name.rsplit_once("::").is_some_and(|(class, method)| {
                class == impl_class && crate::names::php_symbol_key(method) == method_key
            })
    }).is_some_and(function_returns_owned_string)
}

/// Recognizes functions whose returning paths all explicitly persist or acquire their string.
pub(crate) fn function_returns_owned_string(function: &Function) -> bool {
    if function.return_php_type.codegen_repr() != PhpType::Str || function.flags.by_ref_return {
        return false;
    }
    let mut has_return = false;
    for block in &function.blocks {
        if let Some(Terminator::Return { value }) = block.terminator.as_ref() {
            has_return = true;
            if !value.is_some_and(|value| {
                owns_string_value(function, value)
                    || crate::codegen::frame::return_transfers_local_string_owner(function, value)
            }) {
                return false;
            }
        }
    }
    has_return
}

/// Follows identity operations without mistaking a borrowed local or parameter for an owner.
fn owns_string_value(function: &Function, mut value: ValueId) -> bool {
    for _ in 0..function.values.len() {
        let Some(metadata) = function.value(value) else { return false; };
        if metadata.php_type.codegen_repr() != PhpType::Str { return false; }
        let ValueDef::Instruction { inst, .. } = metadata.def else { return false; };
        let Some(inst) = function.instruction(inst) else { return false; };
        match inst.op {
            Op::StrPersist | Op::Acquire => return true,
            Op::Move | Op::Borrow => {
                let Some(source) = inst.operands.first() else { return false; };
                value = *source;
            }
            _ => return false,
        }
    }
    false
}
