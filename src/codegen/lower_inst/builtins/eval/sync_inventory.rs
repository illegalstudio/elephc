//! Purpose:
//! Discovers locals and globals eligible for eval scope synchronization.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Only storage types that safely round-trip through Mixed are selected.

use super::*;

/// Collects PHP-visible locals that the current conservative scope sync can round-trip.
pub(super) fn eval_sync_locals(ctx: &FunctionContext<'_>) -> Vec<EvalSyncLocal> {
    ctx.function
        .locals
        .iter()
        .filter(|local| local.kind == LocalKind::PhpLocal)
        .filter(|local| !local_uses_eval_global_sync(ctx, local.name.as_deref()))
        .filter_map(|local| {
            let name = local.name.clone()?;
            let ty = local.php_type.codegen_repr();
            eval_sync_type_supported(&ty).then_some(EvalSyncLocal {
                name,
                slot: local.id,
                ty,
            })
        })
        .collect()
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
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
        emit_eval_scope_set(ctx, local, scope_set_flags_for_type(&ty));
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
