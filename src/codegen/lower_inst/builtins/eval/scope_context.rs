//! Purpose:
//! Manages persistent eval scope handles and active class scope.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Late-static class lookup keeps target-specific lowering isolated here.

use super::*;

/// Where the eval context handle lives while the native inventory is registered.
///
/// The handle is needed by every registration call, and each of those calls
/// clobbers the register the handle arrived in. It is parked in a module-level
/// symbol: the module-level inventory helper has the fixed 32-byte frame the
/// shared-helper prologue builds, with no room for a local slot, and a local
/// addressed relative to the frame pointer would land in the caller's stack.
#[derive(Clone, Copy)]
pub(super) enum EvalContextSource {
    /// The handle is in this module-level symbol.
    Symbol(&'static str),
}

/// Loads the persistent eval context handle from the caller's frame slot.
pub(super) fn load_eval_context_local_to_arg(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    arg_index: usize,
) {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, arg_index);
    abi::load_at_offset(ctx.emitter, arg_reg, context_offset);
}

/// Loads the eval context handle from the given source into the first argument register.
pub(super) fn load_eval_context_source_to_arg(
    ctx: &mut FunctionContext<'_>,
    source: EvalContextSource,
    arg_index: usize,
) {
    match source {
        EvalContextSource::Symbol(symbol) => {
            let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, arg_index);
            abi::emit_load_symbol_to_reg(ctx.emitter, arg_reg, symbol, 0);
        }
    }
}

/// Loads the current eval context handle into the selected integer argument register.
pub(super) fn load_eval_context_to_arg(ctx: &mut FunctionContext<'_>, arg_index: usize) {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, arg_index);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_reg, EVAL_CONTEXT_HANDLE_OFFSET);
}

/// Reloads the saved eval source string into the bridge code pointer/length arguments.
pub(super) fn move_saved_eval_code_to_eval_args(ctx: &mut FunctionContext<'_>) {
    let code_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    let code_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_temporary_stack_slot(ctx.emitter, code_ptr_arg, EVAL_CODE_PTR_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, code_len_arg, EVAL_CODE_LEN_OFFSET);
}

/// Parses or looks up the fragment and saves a type-safe conservative scope-access mask.
pub(super) fn prepare_dynamic_eval_scope_access_mask(
    ctx: &mut FunctionContext<'_>,
    locals: &[EvalSyncLocal],
    globals: &[EvalSyncGlobal],
) -> Result<()> {
    // -- prepare the dynamic eval scope access mask --
    ctx.emitter
        .comment("-- prepare the dynamic eval scope access mask --");
    load_eval_context_to_arg(ctx, 0);
    let code_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let code_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, code_ptr_arg, EVAL_CODE_PTR_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, code_len_arg, EVAL_CODE_LEN_OFFSET);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_scope_access_mask");
    abi::emit_call_label(ctx.emitter, &symbol);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_SCOPE_ACCESS_MASK_OFFSET);
    promote_unsafe_eval_scope_access_to_full(ctx, locals, globals)
}

/// Promotes selected heap, reference-capable, or union bindings to full synchronization.
fn promote_unsafe_eval_scope_access_to_full(
    ctx: &mut FunctionContext<'_>,
    locals: &[EvalSyncLocal],
    globals: &[EvalSyncGlobal],
) -> Result<()> {
    let full = ctx.next_label("eval_scope_access_full");
    let done = ctx.next_label("eval_scope_access_types_done");
    for local in locals {
        if matches!(local.ty.codegen_repr(), PhpType::Int | PhpType::Bool | PhpType::Float) {
            continue;
        }
        let next = ctx.next_label("eval_scope_access_local_type_safe");
        emit_branch_if_eval_name_unaccessed(
            ctx.emitter,
            &local.name,
            EVAL_SCOPE_ACCESS_MASK_OFFSET,
            &next,
        );
        if matches!(local.ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            ctx.load_local_to_result(local.slot)?;
            emit_branch_if_mixed_numeric_scalar(ctx.emitter, &next);
        }
        emit_branch_to_eval_scope_label(ctx, &full);
        ctx.emitter.label(&next);
    }
    for global in globals {
        if matches!(global.ty.codegen_repr(), PhpType::Int | PhpType::Bool | PhpType::Float) {
            continue;
        }
        let next = ctx.next_label("eval_scope_access_global_type_safe");
        emit_branch_if_eval_name_unaccessed(
            ctx.emitter,
            &global.name,
            EVAL_SCOPE_ACCESS_MASK_OFFSET,
            &next,
        );
        if matches!(global.ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            load_global_to_result(ctx, global);
            emit_branch_if_mixed_numeric_scalar(ctx.emitter, &next);
        }
        emit_branch_to_eval_scope_label(ctx, &full);
        ctx.emitter.label(&next);
    }
    emit_branch_to_eval_scope_label(ctx, &done);
    ctx.emitter.label(&full);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, -1);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_SCOPE_ACCESS_MASK_OFFSET);
    ctx.emitter.label(&done);
    Ok(())
}

/// Continues selective synchronization when a boxed binding currently contains a numeric scalar.
fn emit_branch_if_mixed_numeric_scalar(
    emitter: &mut crate::codegen_support::emit::Emitter,
    label: &str,
) {
    // -- unbox a selected Mixed binding for its scalar tag --
    emitter.comment(
        "-- accept selected Mixed bindings only when they hold numeric scalars --",
    );
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    let tag = abi::int_result_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- accept numeric scalar tags on AArch64 --
            for value in [0, 2, 3] {
                emitter.instruction(&format!("cmp {tag}, #{value}"));           // accept an integer, float, or boolean payload
                emitter.instruction(&format!("b.eq {label}"));                  // retain the selective mask for this scalar
            }
        }
        Arch::X86_64 => {
            // -- accept numeric scalar tags on x86_64 --
            for value in [0, 2, 3] {
                emitter.instruction(&format!("cmp {tag}, {value}"));            // accept an integer, float, or boolean payload
                emitter.instruction(&format!("je {label}"));                    // retain the selective mask for this scalar
            }
        }
    }
}

/// Branches to one scope-access label without changing the saved runtime mask.
fn emit_branch_to_eval_scope_label(ctx: &mut FunctionContext<'_>, label: &str) {
    // -- continue at the shared eval scope access path --
    ctx.emitter
        .comment("-- continue at the shared eval scope access path --");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- continue at the AArch64 scope-mask path --
            ctx.emitter.instruction(&format!("b {label}"));                     // continue at the shared scope-mask path
        }
        Arch::X86_64 => {
            // -- continue at the x86_64 scope-mask path --
            ctx.emitter.instruction(&format!("jmp {label}"));                   // continue at the shared scope-mask path
        }
    }
}

/// Ensures a persistent eval scope exists and stores its handle in the scratch frame.
pub(super) fn ensure_eval_scope(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let slot = eval_scope_slot(ctx)?;
    let offset = ctx.local_offset(slot)?;
    let ready = ctx.next_label("eval_scope_ready");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &ready);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_new");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::store_at_offset(ctx.emitter, result_reg, offset);
    ctx.emitter.label(&ready);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_SCOPE_HANDLE_OFFSET);
    Ok(())
}

/// Uses the main eval scope as global storage, or allocates a distinct function-global scope.
pub(super) fn ensure_eval_global_scope(ctx: &mut FunctionContext<'_>) -> Result<()> {
    if ctx.is_main {
        // Top-level eval declarations and `global` inside eval functions share storage.
        // Only EvalScope owns this handle; leave EvalGlobalScope null to avoid double-free.
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_SCOPE_HANDLE_OFFSET);
        abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_GLOBAL_SCOPE_HANDLE_OFFSET);
        return Ok(());
    }
    let slot = eval_global_scope_slot(ctx)?;
    let offset = ctx.local_offset(slot)?;
    let ready = ctx.next_label("eval_global_scope_ready");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &ready);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_new");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::store_at_offset(ctx.emitter, result_reg, offset);
    ctx.emitter.label(&ready);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_GLOBAL_SCOPE_HANDLE_OFFSET);
    Ok(())
}

/// Returns the hidden frame slot that owns this function's persistent eval scope.
pub(super) fn eval_scope_slot(ctx: &FunctionContext<'_>) -> Result<LocalSlotId> {
    // Main executes in the global symbol table, including names first created by
    // opaque eval. Keep one owning slot so frame cleanup frees the table once.
    if ctx.is_main {
        return eval_global_scope_slot(ctx);
    }
    ctx.function
        .locals
        .iter()
        .find(|local| local.kind == LocalKind::EvalScope)
        .map(|local| local.id)
        .ok_or_else(|| CodegenIrError::invalid_module("eval call missing eval scope local"))
}

/// Returns the hidden frame slot that owns this function's eval global scope.
pub(super) fn eval_global_scope_slot(ctx: &FunctionContext<'_>) -> Result<LocalSlotId> {
    ctx.function
        .locals
        .iter()
        .find(|local| local.kind == LocalKind::EvalGlobalScope)
        .map(|local| local.id)
        .ok_or_else(|| CodegenIrError::invalid_module("eval call missing eval global scope local"))
}

/// Loads the current eval scope handle into the selected integer argument register.
pub(super) fn load_eval_scope_to_arg(ctx: &mut FunctionContext<'_>, arg_index: usize) {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, arg_index);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_reg, EVAL_SCOPE_HANDLE_OFFSET);
}

/// Loads the current eval global-scope handle into the selected integer argument register.
pub(super) fn load_eval_global_scope_to_arg(ctx: &mut FunctionContext<'_>, arg_index: usize) {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, arg_index);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_reg, EVAL_GLOBAL_SCOPE_HANDLE_OFFSET);
}

/// Installs the current eval global-scope handle into the eval context.
pub(super) fn set_eval_context_global_scope(ctx: &mut FunctionContext<'_>) {
    load_eval_context_to_arg(ctx, 0);
    load_eval_global_scope_to_arg(ctx, 1);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_context_set_global_scope");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Enters the current AOT method's class scope in the eval context, if any.
pub(super) fn push_eval_context_class_scope(ctx: &mut FunctionContext<'_>) -> Result<bool> {
    let Some(class_name) = current_eval_method_class(ctx).map(str::to_string) else {
        return Ok(false);
    };
    emit_eval_called_class_name_result(ctx, &class_name)?;
    let (called_ptr_reg, called_len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, called_ptr_reg, EVAL_CALLED_CLASS_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, called_len_reg, EVAL_CALLED_CLASS_LEN_OFFSET);
    load_eval_context_to_arg(ctx, 0);
    let (class_label, class_len) = ctx.data.add_string(class_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &class_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_len as i64,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        EVAL_CALLED_CLASS_PTR_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        EVAL_CALLED_CLASS_LEN_OFFSET,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_context_push_class_scope");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    Ok(true)
}

/// Leaves a pushed eval class scope while preserving the original eval status.
pub(super) fn pop_eval_context_class_scope(ctx: &mut FunctionContext<'_>, pushed: bool) {
    if !pushed {
        return;
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    load_eval_context_to_arg(ctx, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_context_pop_class_scope");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
}

/// Returns the lexical class encoded in the current EIR method name.
pub(super) fn current_eval_method_class<'a>(ctx: &'a FunctionContext<'_>) -> Option<&'a str> {
    ctx.function
        .flags
        .is_method
        .then(|| {
            ctx.function
                .name
                .rsplit_once("::")
                .map(|(class_name, _)| class_name)
        })
        .flatten()
}

/// Materializes the runtime called-class name for eval `static::` resolution.
pub(super) fn emit_eval_called_class_name_result(
    ctx: &mut FunctionContext<'_>,
    fallback_class: &str,
) -> Result<()> {
    if eval_late_static_class_id_available(ctx) {
        match ctx.emitter.target.arch {
            Arch::AArch64 => emit_eval_called_class_name_result_aarch64(ctx),
            Arch::X86_64 => emit_eval_called_class_name_result_x86_64(ctx),
        }
    } else {
        emit_eval_static_string_result(ctx, fallback_class.as_bytes());
        Ok(())
    }
}

/// Emits the AArch64 class-id table lookup for eval's called class.
pub(super) fn emit_eval_called_class_name_result_aarch64(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let missing = ctx.next_label("eval_called_class_missing");
    let done = ctx.next_label("eval_called_class_done");
    emit_eval_late_static_class_id_to_reg(ctx, "x12")?;
    abi::emit_load_symbol_to_reg(ctx.emitter, "x10", "_class_name_count", 0);
    ctx.emitter.instruction("cmp x12, x10");                                    // reject called-class ids outside the class-name table
    ctx.emitter.instruction(&format!("b.hs {}", missing));                      // fall back to the lexical eval class when metadata is missing
    abi::emit_symbol_address(ctx.emitter, "x11", "_class_name_entries");
    ctx.emitter.instruction("lsl x12, x12, #4");                                // convert class id to a 16-byte class-name table offset
    ctx.emitter.instruction("add x11, x11, x12");                               // select the called-class metadata row
    ctx.emitter.instruction("ldr x1, [x11]");                                   // load the called-class name pointer
    ctx.emitter.instruction("ldr x2, [x11, #8]");                               // load the called-class name length
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the missing-metadata fallback
    ctx.emitter.label(&missing);
    abi::emit_symbol_address(ctx.emitter, "x1", "_class_name_missing");
    ctx.emitter.instruction("mov x2, #0");                                      // empty called-class name triggers lexical fallback in eval
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits the x86_64 class-id table lookup for eval's called class.
pub(super) fn emit_eval_called_class_name_result_x86_64(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let missing = ctx.next_label("eval_called_class_missing");
    let done = ctx.next_label("eval_called_class_done");
    emit_eval_late_static_class_id_to_reg(ctx, "r8")?;
    abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_class_name_count", 0);
    ctx.emitter.instruction("cmp r8, r9");                                      // reject called-class ids outside the class-name table
    ctx.emitter.instruction(&format!("jae {}", missing));                       // fall back to the lexical eval class when metadata is missing
    abi::emit_symbol_address(ctx.emitter, "r10", "_class_name_entries");
    ctx.emitter.instruction("shl r8, 4");                                       // convert class id to a 16-byte class-name table offset
    ctx.emitter.instruction("add r10, r8");                                     // select the called-class metadata row
    ctx.emitter.instruction("mov rax, QWORD PTR [r10]");                        // load the called-class name pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                    // load the called-class name length
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the missing-metadata fallback
    ctx.emitter.label(&missing);
    abi::emit_symbol_address(ctx.emitter, "rax", "_class_name_missing");
    ctx.emitter.instruction("mov rdx, 0");                                      // empty called-class name triggers lexical fallback in eval
    ctx.emitter.label(&done);
    Ok(())
}

/// Returns true when the current method frame can provide a late-static class id.
pub(super) fn eval_late_static_class_id_available(ctx: &FunctionContext<'_>) -> bool {
    ctx.local_slot_by_name(crate::names::CALLED_CLASS_ID_LOCAL).is_some()
        || ctx.local_slot_by_name("this").is_some()
}

/// Loads the late-static class id from the hidden static slot or `$this`.
pub(super) fn emit_eval_late_static_class_id_to_reg(ctx: &mut FunctionContext<'_>, reg: &str) -> Result<()> {
    if let Some(slot) = ctx.local_slot_by_name(crate::names::CALLED_CLASS_ID_LOCAL) {
        let offset = ctx.local_offset(slot)?;
        abi::load_at_offset(ctx.emitter, reg, offset);
        return Ok(());
    }
    if let Some(slot) = ctx.local_slot_by_name("this") {
        match ctx.local_php_type(slot)? {
            PhpType::Mixed | PhpType::Union(_) => {
                ctx.load_local_to_result(slot)?;
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                let object_reg = eval_mixed_unbox_low_payload_reg(ctx);
                abi::emit_load_from_address(ctx.emitter, reg, object_reg, 0);
            }
            PhpType::Object(_) => {
                let offset = ctx.local_offset(slot)?;
                abi::load_at_offset(ctx.emitter, reg, offset);
                abi::emit_load_from_address(ctx.emitter, reg, reg, 0);
            }
            other => {
                return Err(CodegenIrError::invalid_module(format!(
                    "eval class scope this local has PHP type {:?}",
                    other
                )))
            }
        }
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "eval class scope without called-class source in {}",
        ctx.function.name
    )))
}

/// Emits a static string result for eval class-scope setup fallback paths.
pub(super) fn emit_eval_static_string_result(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

#[cfg(test)]
mod tests {
    use super::emit_branch_if_mixed_numeric_scalar;
    use crate::codegen_support::{emit::Emitter, platform::Target};

    /// Every supported target accepts only integer, float, and boolean Mixed tags.
    #[test]
    fn eval_scope_scalar_proof_covers_all_supported_targets() {
        for target in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_branch_if_mixed_numeric_scalar(&mut emitter, "scalar_ok");
            let asm = emitter.output();
            assert!(asm.contains("__rt_mixed_unbox"), "{target}: {asm}");
            for tag in [0, 2, 3] {
                let compare = if target == "linux-x86_64" {
                    format!("cmp rax, {tag}")
                } else {
                    format!("cmp x0, #{tag}")
                };
                assert!(asm.contains(&compare), "{target}: {asm}");
            }
            assert_eq!(asm.matches("scalar_ok").count(), 3, "{target}: {asm}");
        }
    }
}
