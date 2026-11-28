//! Purpose:
//! Separates reached global declarations from the activation bindings they replace.
//!
//! Called from:
//! - The shared global-statement dispatcher used by interpreted bodies.
//!
//! Key details:
//! - Missing program entries are created before retiring the old local.
//! - A throwing retirement leaves a local null before alias publication.
//! - Original producer contracts are copied independently of current value snapshots.

use super::*;

/// Prepares each program entry, retires the displaced local and publishes the named alias.
pub(super) fn bind(names: &[String], context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let global = context.global_scope_ptr().ok_or(EvalStatus::RuntimeFatal)?;
    let same_scope = global == scope as *mut ElephcEvalScope;
    for name in names {
        let origin = if same_scope { prepare(name, context, scope, values)? }
            else {
                let global = unsafe { global.as_mut() }.ok_or(EvalStatus::RuntimeFatal)?;
                prepare(name, context, global, values)?
            };
        if !same_scope {
            let old = unset_scope_cell(context, scope, name.clone())?;
            let null = values.null()?;
            let replaced = scope.set(name.clone(), null, ScopeCellOwnership::Owned);
            for old in old.into_iter().chain(replaced) { eval_release_value(context, values, old)?; }
        }
        scope.mark_global_alias(name.clone());
        if let Some(origin) = origin { scope.infer_local_type(name.clone(), origin); }
    }
    Ok(())
}

/// Creates an untyped missing global while preserving an existing source's original contract.
fn prepare(name: &str, context: &mut ElephcEvalContext, global: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalParameterType>, EvalStatus> {
    if global.visible_cell(name).is_none() {
        let null = values.null()?;
        if let Some(old) = global.set(name.to_string(), null, ScopeCellOwnership::Owned) {
            eval_release_value(context, values, old)?;
        }
    }
    Ok(local_producer_contract(&EvalExpr::LoadVar(name.to_string()), context, global))
}
