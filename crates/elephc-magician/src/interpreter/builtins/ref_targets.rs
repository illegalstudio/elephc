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
            let Some(scope) = (unsafe { scope.live_mut() }) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            // A by-reference PARAMETER names caller storage this scope only borrows: the entry's
            // cell is the caller's own, so swapping it for the replacement (and releasing the
            // old one) frees storage the caller still holds -- `function f(array &$a) {
            // sort($a); }` left the caller's variable pointing at a freed cell. The write goes
            // through the recorded reference target first, exactly as a plain assignment to the
            // variable does, and the local entry then only borrows the new value.
            let through_reference = match scope.reference_target(name).cloned() {
                Some(reference) => {
                    write_back_method_ref_target(&reference, value, context, values)?;
                    true
                }
                None => false,
            };
            let ownership = if through_reference {
                ScopeCellOwnership::Borrowed
            } else {
                variable_ownership.unwrap_or_else(|| {
                    scope_entry(context, scope, name)
                        .filter(|entry| entry.flags().is_visible())
                        .map(|entry| entry.flags().ownership)
                        .unwrap_or(ScopeCellOwnership::Owned)
                })
            };
            for replaced in set_scope_cell(context, scope, name.clone(), value, ownership, values)? {
                values.release(replaced)?;
            }
            Ok(())
        }
        _ => write_back_method_ref_target(target, value, context, values),
    }
}
