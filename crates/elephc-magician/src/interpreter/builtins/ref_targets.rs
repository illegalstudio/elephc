//! Purpose:
//! Shared caller-lvalue writeback helpers for direct by-reference eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins` modules that implement PHP builtins with
//!   direct by-reference output parameters.
//!
//! Key details:
//! - Variable writes can request a specific scope-cell ownership, while object,
//!   static-property, and array-element targets reuse method by-reference
//!   writeback semantics.

use super::super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// Retires a nonescaping builtin target's owned keys and receivers after mutation or failure.
pub(in crate::interpreter) fn with_direct_ref_target<V: RuntimeValueOps>(
    mut target: EvalReferenceTarget, context: &mut ElephcEvalContext, values: &mut V,
    consume: impl FnOnce(&EvalReferenceTarget, &mut ElephcEvalContext, &mut V)
        -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut owners = Vec::new();
    capture_abandoned_reference_owners(Some(&mut target), &mut owners);
    let owners = owners.into_iter().map(RuntimeOwnerSlot::new).collect::<Vec<_>>();
    with_runtime_owners(&owners, || {
        let result = consume(&target, context, values);
        finish_synthesized_method_arguments(result, owners.iter().map(RuntimeOwnerSlot::take), context, values)
    })
}

/// Writes a direct by-reference builtin result back to the captured caller lvalue.
pub(in crate::interpreter) fn eval_write_direct_ref_target(
    target: &EvalReferenceTarget,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    variable_ownership: Option<ScopeCellOwnership>,
) -> Result<(), EvalStatus> {
    match target {
        EvalReferenceTarget::Variable { scope, name } => {
            let Some(scope) = (unsafe { scope.as_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let ownership = variable_ownership.unwrap_or_else(|| {
                scope_entry(context, scope, name)
                    .filter(|entry| entry.flags().is_visible())
                    .map(|entry| entry.flags().ownership)
                    .unwrap_or(ScopeCellOwnership::Owned)
            });
            let replaced = if variable_ownership == Some(ScopeCellOwnership::Owned) {
                set_owned_scope_cell(context, scope, name.clone(), value, values)?
            } else { set_scope_cell(context, scope, name.clone(), value, ownership, values)? };
            for replaced in replaced { values.release(replaced)?; }
            Ok(())
        }
        _ => write_back_method_ref_target(target, value, context, values),
    }
}
