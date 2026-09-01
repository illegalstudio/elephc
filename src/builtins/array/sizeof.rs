//! Purpose:
//! Registers PHP's `sizeof` alias with the same checker and EIR contract as `count`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Lowering uses the existing typed count runtime target and its one-visible-argument rule.

use crate::builtins::semantics::{
    runtime_fn_semantics, with_argument_lowering, BuiltinArgumentLowering, BuiltinEffects,
    BuiltinSemanticInput, BuiltinSemantics,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::checker::builtins::arrays::union_member_is_countable_array;
use crate::types::PhpType;

builtin! {
    contract: "sizeof",
    check: check,
    semantics: sizeof_semantics(),
}

/// Builds the count runtime semantics while preserving the unary AOT contract.
const fn sizeof_semantics() -> BuiltinSemantics {
    let mut semantics = with_argument_lowering(
        runtime_fn_semantics(crate::ir::RuntimeFnId::Count),
        BuiltinArgumentLowering::Count,
    );
    semantics.effects = BuiltinEffects::Shared(effects);
    semantics
}

/// Resolves the alias's heap-read and catchable-error effects from the receiver representation.
fn effects(input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    match input.arg_types.first().map(PhpType::codegen_repr) {
        Some(PhpType::Array(_) | PhpType::AssocArray { .. }) => {
            crate::ir::Effects::READS_HEAP | crate::ir::Effects::MAY_THROW
        }
        _ => crate::ir::RuntimeFnId::Count.effects(),
    }
}

/// Accepts arrays, mixed values, countable unions, and objects implementing `Countable`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match &ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed => Ok(PhpType::Int),
        PhpType::Union(members) if members.iter().all(union_member_is_countable_array) => {
            Ok(PhpType::Int)
        }
        PhpType::Object(class_name) => {
            if cx.checker.class_implements_interface(class_name, "Countable") {
                Ok(PhpType::Int)
            } else {
                Err(CompileError::new(
                    cx.span,
                    "sizeof() object argument must implement Countable",
                ))
            }
        }
        _ => Err(CompileError::new(
            cx.span,
            "sizeof() argument must be array or Countable object",
        )),
    }
}
