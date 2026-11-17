//! Purpose:
//! Refines element stores that cannot reach reference markers, PHP coercions or destructors.
//!
//! Called from:
//! - Whole-module instruction refinement and callable effect summarization.
//!
//! Key details:
//! - Unknown or Mixed-element storage retains the opcode's complete callback effects.
//! - Proven typed writes still read, allocate, mutate and retire container storage.

use crate::ir::{Effects, Function, Instruction};
use crate::types::PhpType;

/// Narrows a typed store only when its key and replacement require no user-code conversion.
pub(super) fn effects(function: &Function, instruction: &Instruction) -> Option<Effects> {
    let container = function.value(*instruction.operands.first()?)?.php_type.codegen_repr();
    let element = match &container {
        PhpType::Array(element) => element.as_ref(),
        PhpType::AssocArray { value, .. } => value.as_ref(),
        _ => return None,
    };
    if !destructor_free(element) { return None; }
    let key = function.value(*instruction.operands.get(1)?)?.php_type.codegen_repr();
    if !matches!(key, PhpType::Int | PhpType::Str) { return None; }
    let replacement = function.value(*instruction.operands.get(2)?)?.php_type.codegen_repr();
    if replacement != element.codegen_repr() { return None; }
    Some(Effects::READS_HEAP | Effects::WRITES_HEAP | Effects::ALLOC_HEAP
        | Effects::MAY_FATAL | Effects::REFCOUNT_OP)
}

/// Proves that retiring any value allowed by this storage type cannot invoke PHP destruction.
fn destructor_free(ty: &PhpType) -> bool {
    match ty.codegen_repr() {
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str
        | PhpType::Void | PhpType::Never | PhpType::Pointer(_) => true,
        PhpType::Array(element) => destructor_free(&element),
        PhpType::AssocArray { value, .. } => destructor_free(&value),
        _ => false,
    }
}
