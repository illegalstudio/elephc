//! Purpose:
//! Discovers locals and globals eligible for eval scope synchronization.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Only storage types that safely round-trip through Mixed are selected.
//! - Compiler-generated frame and parser temporaries are excluded here, which is the single
//!   choke point both the pre-eval FLUSH and the post-eval RELOAD read. Leaking one of them
//!   would publish `__elephc_func_args#gen` into the eval scope, let `get_defined_vars()`
//!   inside the fragment report it as a PHP variable, and let an eval assignment to that name
//!   be written back over the frame's hidden argument collector or its actual-argument count.

use super::*;

/// Collects PHP-visible locals that the current conservative scope sync can round-trip.
pub(super) fn eval_sync_locals(ctx: &FunctionContext<'_>) -> Vec<EvalSyncLocal> {
    eval_sync_function_locals(ctx.function)
        .into_iter()
        .filter(|local| !local_uses_eval_global_sync(ctx, Some(&local.name)))
        .collect()
}

/// Maps PHP parameter names to their active COW shadows, never exposing incoming ABI duplicates.
fn eval_sync_function_locals(function: &Function) -> Vec<EvalSyncLocal> {
    let shadows = function.locals.iter()
        .filter(|local| local.kind == LocalKind::PhpLocal)
        .filter_map(|local| local.name.as_deref()?.strip_suffix("#cow"))
        .collect::<BTreeSet<_>>();
    function.locals.iter()
        .filter(|local| local.kind == LocalKind::PhpLocal)
        .filter_map(|local| {
            let stored_name = local.name.as_deref()?;
            // `privatize_container_param` leaves the ABI slot in place and redirects PHP reads
            // and writes to `name#cow`. Eval must use that same binding under the PHP name.
            let name = if let Some(name) = stored_name.strip_suffix("#cow") {
                name
            } else if shadows.contains(stored_name) {
                return None;
            } else {
                stored_name
            };
            // The marker survives the `#cow` strip above, so a privatized hidden local is
            // caught here too.
            if crate::names::is_generated_local_name(name) {
                return None;
            }
            let ty = local.php_type.codegen_repr();
            eval_sync_type_supported(&ty).then_some(EvalSyncLocal {
                name: name.to_string(), slot: local.id, ty,
            })
        }).collect()
}

/// Keeps only eval-sync locals whose PHP name appears in `names`.
pub(super) fn filter_eval_sync_locals_by_name(
    locals: Vec<EvalSyncLocal>,
    names: &BTreeSet<String>,
) -> Vec<EvalSyncLocal> {
    locals
        .into_iter()
        .filter(|local| names.contains(&local.name))
        .collect()
}

/// Returns true when a local name is backed by program-global storage during eval.
pub(super) fn local_uses_eval_global_sync(ctx: &FunctionContext<'_>, name: Option<&str>) -> bool {
    name.is_some_and(|name| main_name_uses_eval_global_scope(ctx, name))
}

/// Returns true when a main-scope name has actual EIR global storage to synchronize.
pub(super) fn main_name_uses_eval_global_scope(ctx: &FunctionContext<'_>, name: &str) -> bool {
    ctx.is_main && eval_sync_global_type(ctx, name).is_some()
}

/// Collects caller-scope `global` aliases that eval fragments inherit by name.
pub(super) fn eval_global_aliases(ctx: &FunctionContext<'_>) -> Vec<EvalGlobalAlias> {
    ctx.function
        .locals
        .iter()
        .filter(|local| local.kind == LocalKind::GlobalAlias)
        .filter_map(|local| {
            let name = local.name.clone()?;
            Some(EvalGlobalAlias {
                global_name: name.clone(),
                name,
            })
        })
        .collect()
}

/// Collects program globals that can be boxed into the eval global scope.
pub(super) fn eval_sync_globals(ctx: &FunctionContext<'_>) -> Vec<EvalSyncGlobal> {
    let mut globals = ctx
        .module
        .data
        .global_names
        .iter()
        .filter_map(|name| {
            let ty = eval_sync_global_type(ctx, name)?;
            eval_sync_global_type_supported(&ty).then_some(EvalSyncGlobal {
                name: name.clone(),
                ty,
            })
        })
        .collect::<Vec<_>>();
    // Process globals share ordinary global Mixed storage, including their entry-point initializers.
    push_eval_process_superglobal(&mut globals, "argc", PhpType::Mixed);
    push_eval_process_superglobal(&mut globals, "argv", PhpType::Mixed);
    globals
}

/// Keeps only eval-sync globals whose PHP name appears in `names`.
pub(super) fn filter_eval_sync_globals_by_name(
    globals: Vec<EvalSyncGlobal>,
    names: &BTreeSet<String>,
) -> Vec<EvalSyncGlobal> {
    globals
        .into_iter()
        .filter(|global| names.contains(&global.name))
        .collect()
}

/// Adds a process superglobal to eval global sync unless normal globals already include it.
pub(super) fn push_eval_process_superglobal(globals: &mut Vec<EvalSyncGlobal>, name: &str, ty: PhpType) {
    if globals.iter().any(|global| global.name == name) {
        return;
    }
    globals.push(EvalSyncGlobal {
        name: name.to_string(),
        ty,
    });
}

/// Returns one unambiguous codegen type used for a program global, if available.
pub(super) fn eval_sync_global_type(ctx: &FunctionContext<'_>, name: &str) -> Option<PhpType> {
    let is_typed_superglobal = ctx.module.web && crate::superglobals::is_superglobal(name);
    let mut inferred = None;
    for function in ctx
        .module
        .functions
        .iter()
        .chain(ctx.module.closures.iter())
    {
        for inst in &function.instructions {
            if global_instruction_name(ctx, inst) != Some(name) {
                continue;
            }
            // Only real global storage instructions make a name a program
            // global; eval scope ops reference names through the same data
            // pool without any global storage behind them.
            if !matches!(inst.op, Op::LoadGlobal | Op::StoreGlobal) {
                continue;
            }
            if !is_typed_superglobal {
                // Regular globals always hold one boxed Mixed word (see
                // `lower_store_global`); store operands carry narrower source
                // types, so per-instruction inference would reject globals
                // written as scalars and read back as Mixed after a barrier.
                return Some(PhpType::Mixed);
            }
            let candidate = global_instruction_value_type(function, inst)?;
            let candidate = candidate.codegen_repr();
            if !eval_sync_global_type_supported(&candidate) {
                return None;
            }
            match &inferred {
                Some(existing) if existing != &candidate => return None,
                Some(_) => {}
                None => inferred = Some(candidate),
            }
        }
    }
    inferred
}

/// Returns the global name referenced by a load/store-global instruction.
pub(super) fn global_instruction_name<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Option<&'a str> {
    let Some(Immediate::GlobalName(data)) = inst.immediate else {
        return None;
    };
    ctx.module
        .data
        .global_names
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Returns the value type carried by a global load or store instruction.
pub(super) fn global_instruction_value_type(function: &Function, inst: &Instruction) -> Option<PhpType> {
    match inst.op {
        Op::LoadGlobal => {
            let result = inst.result?;
            function.value(result).map(|value| value.php_type.clone())
        }
        Op::StoreGlobal => {
            let value = *inst.operands.first()?;
            function.value(value).map(|value| value.php_type.clone())
        }
        _ => None,
    }
}

/// Returns true when a global type can round-trip through eval global scope sync.
pub(super) fn eval_sync_global_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Returns true when a local type can be boxed to Mixed and restored from Mixed after eval.
pub(super) fn eval_sync_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Flushes visible native locals into the materialized eval scope before executing eval.
pub(super) fn flush_eval_scope_locals(ctx: &mut FunctionContext<'_>, locals: &[EvalSyncLocal]) -> Result<()> {
    for local in locals {
        let ty = ctx.load_local_to_result(local.slot)?.codegen_repr();
        if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &ty);
        } else {
            // Keep an independent snapshot while eval or another alias replaces the native slot.
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
        emit_eval_scope_set(ctx, local, EVAL_SCOPE_FLAG_OWNED);
    }
    Ok(())
}

/// Flushes supported program globals into the eval global scope before eval.
pub(super) fn flush_eval_global_scope(
    ctx: &mut FunctionContext<'_>,
    globals: &[EvalSyncGlobal],
) -> Result<()> {
    for global in globals {
        if main_eval_local_supplies_global(ctx, &global.name) {
            continue;
        }
        load_global_to_result(ctx, global);
        if !matches!(global.ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &global.ty);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
        emit_eval_global_scope_set(ctx, global, scope_set_flags_for_type(&global.ty));
    }
    Ok(())
}

/// Flushes global-backed variables into the local eval scope for scope-read EIR AOT.
pub(super) fn flush_eval_globals_to_local_scope(ctx: &mut FunctionContext<'_>, globals: &[EvalSyncGlobal]) {
    for global in globals {
        if main_eval_local_supplies_global(ctx, &global.name) {
            continue;
        }
        load_global_to_result(ctx, global);
        if !matches!(global.ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &global.ty);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
        emit_eval_scope_set_name(ctx, &global.name, scope_set_flags_for_type(&global.ty));
    }
}

/// Keeps top-level local process values authoritative when local and global eval scopes are shared.
fn main_eval_local_supplies_global(ctx: &FunctionContext<'_>, name: &str) -> bool {
    ctx.is_main
        && !main_name_uses_eval_global_scope(ctx, name)
        && ctx.function.locals.iter().any(|local| {
            local.kind == LocalKind::PhpLocal
                && local.name.as_deref() == Some(name)
                && eval_sync_type_supported(&local.php_type.codegen_repr())
        })
}

/// Loads a program-global symbol into result registers using its inferred type.
pub(super) fn load_global_to_result(ctx: &mut FunctionContext<'_>, global: &EvalSyncGlobal) {
    let symbol = ir_global_symbol(&global.name);
    let ty = global.ty.codegen_repr();
    ctx.data.add_comm(symbol.clone(), ty.stack_size().max(8));
    abi::emit_load_symbol_to_result(ctx.emitter, &symbol, &ty);
}

/// Returns ABI flags for a scope value produced from the given native type.
pub(super) fn scope_set_flags_for_type(ty: &PhpType) -> i64 {
    if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        0
    } else {
        EVAL_SCOPE_FLAG_OWNED
    }
}

#[cfg(test)]
mod tests {
    use super::eval_sync_function_locals;
    use crate::ir::{Function, IrType, LocalKind};
    use crate::types::PhpType;

    /// The eval inventory exposes each PHP name once and selects its active COW slot.
    #[test]
    fn eval_inventory_uses_parameter_shadows_under_php_names() {
        let mut function = Function::new("shadow_scope".to_string(), IrType::Void, PhpType::Void);
        let mut expected = Vec::new();
        for (name, ty) in [
            ("value", PhpType::Mixed),
            ("items", PhpType::Array(Box::new(PhpType::Mixed))),
        ] {
            let ir_type = IrType::from_php(&ty);
            function.add_local(Some(name.to_string()), ir_type, ty.clone(), LocalKind::PhpLocal);
            let shadow = function.add_local(Some(format!("{name}#cow")), ir_type, ty, LocalKind::PhpLocal);
            expected.push((name.to_string(), shadow));
        }
        let text = function.add_local(Some("text".to_string()), IrType::Str, PhpType::Str, LocalKind::PhpLocal);
        expected.push(("text".to_string(), text));
        let actual = eval_sync_function_locals(&function).into_iter()
            .map(|local| (local.name, local.slot)).collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    /// Compiler-generated frame locals never enter the inventory, privatized ones included.
    ///
    /// This is the single list both the pre-eval flush and the post-eval reload read, so a name
    /// appearing here would be publishable to `get_defined_vars()` inside the fragment AND
    /// writable back over the frame's hidden argument state. The user variable spelled like a
    /// hidden local's readable stem is in the same fixture to pin that the filter keys on the
    /// unforgeable marker rather than on the `__elephc_` prefix.
    #[test]
    fn eval_inventory_excludes_generated_frame_locals() {
        let mut function = Function::new("hidden_scope".to_string(), IrType::Void, PhpType::Void);
        let visible = function.add_local(
            Some("__elephc_func_arg_value".to_string()),
            IrType::Str,
            PhpType::Str,
            LocalKind::PhpLocal,
        );
        for hidden in [
            crate::func_args::HIDDEN_ARGS_PARAM.to_string(),
            crate::func_args::HIDDEN_ARGC_PARAM.to_string(),
            crate::names::generated_local_name("__elephc_foreach_3_9"),
            format!("{}#cow", crate::func_args::HIDDEN_ARGS_PARAM),
        ] {
            function.add_local(Some(hidden), IrType::Str, PhpType::Str, LocalKind::PhpLocal);
        }
        let actual = eval_sync_function_locals(&function).into_iter()
            .map(|local| (local.name, local.slot)).collect::<Vec<_>>();
        assert_eq!(actual, vec![("__elephc_func_arg_value".to_string(), visible)]);
    }
}
