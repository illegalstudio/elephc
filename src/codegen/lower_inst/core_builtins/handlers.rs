//! Purpose:
//! Lowers PHP Core error and exception handler state for the AOT backend.
//!
//! Called from:
//! - `super::lower_core_builtin()` for handler and user-error operations.
//!
//! Key details:
//! - Handler registrations own retained callback cells and callable descriptors.
//! - Previous registrations live in heap-backed linked nodes, without a fixed nesting limit.
//! - A handler return value suppresses PHP's default diagnostic unless it is exactly `false`.

use crate::codegen::platform::Arch;
use crate::codegen::{abi, callable_descriptor};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::expect_operand;
use super::{emit_bool_result, emit_null_mixed_result};
use crate::codegen::Result;

const ERROR_HANDLER_NODE_BYTES: i64 = 48;
const EXCEPTION_HANDLER_NODE_BYTES: i64 = 40;
const E_USER_ERROR: i64 = 256;
const E_USER_WARNING: i64 = 512;
const E_USER_NOTICE: i64 = 1_024;
const E_USER_DEPRECATED: i64 = 16_384;
const INVALID_TRIGGER_LEVEL_MESSAGE: &str =
    "trigger_error(): Argument #2 ($error_level) must be one of E_USER_ERROR, E_USER_WARNING, E_USER_NOTICE, or E_USER_DEPRECATED";

/// Gets or replaces the process-local PHP error reporting mask.
pub(super) fn lower_error_reporting(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_php_error_reporting",
        0,
    );
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let source_type = ctx.raw_value_php_type(value)?.codegen_repr();
    if !matches!(source_type, PhpType::Void | PhpType::Never) {
        load_integer_operand(ctx, value)?;
        abi::emit_store_reg_to_symbol(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            "_php_error_reporting",
            0,
        );
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Installs an AOT user error handler and returns the previously active callback.
pub(super) fn lower_set_error_handler(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let callback = expect_operand(inst, 0)?;
    let descriptor = expect_operand(inst, 1)?;
    let mask = expect_operand(inst, 2)?;
    allocate_previous_handler_node(
        ctx,
        ERROR_HANDLER_NODE_BYTES,
        "_php_error_handler_stack",
        "_php_error_handler_value",
        "_php_error_handler_callable",
        Some("_php_error_handler_mask"),
        &[
            "_php_error_handler_context",
            "_php_error_handler_context_release",
        ],
    );
    preserve_previous_callback_result(ctx, "_php_error_handler_value");
    retain_and_store_mixed(ctx, callback, "_php_error_handler_value")?;
    retain_and_store_descriptor(ctx, descriptor, "_php_error_handler_callable")?;
    load_integer_operand(ctx, mask)?;
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_php_error_handler_mask",
        0,
    );
    abi::emit_store_zero_to_symbol(ctx.emitter, "_php_error_handler_context", 0);
    abi::emit_store_zero_to_symbol(
        ctx.emitter,
        "_php_error_handler_context_release",
        0,
    );
    restore_previous_callback_result(ctx);
    Ok(())
}

/// Installs an AOT uncaught-exception handler and returns the previous callback.
pub(super) fn lower_set_exception_handler(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let callback = expect_operand(inst, 0)?;
    let descriptor = expect_operand(inst, 1)?;
    allocate_previous_handler_node(
        ctx,
        EXCEPTION_HANDLER_NODE_BYTES,
        "_php_exception_handler_stack",
        "_php_exception_handler_value",
        "_php_exception_handler_callable",
        None,
        &[
            "_php_exception_handler_context",
            "_php_exception_handler_context_release",
        ],
    );
    preserve_previous_callback_result(ctx, "_php_exception_handler_value");
    retain_and_store_mixed(ctx, callback, "_php_exception_handler_value")?;
    retain_and_store_descriptor(ctx, descriptor, "_php_exception_handler_callable")?;
    abi::emit_store_zero_to_symbol(ctx.emitter, "_php_exception_handler_context", 0);
    abi::emit_store_zero_to_symbol(
        ctx.emitter,
        "_php_exception_handler_context_release",
        0,
    );
    restore_previous_callback_result(ctx);
    Ok(())
}

/// Restores the preceding AOT user error handler, if one was registered.
pub(super) fn lower_restore_error_handler(ctx: &mut FunctionContext<'_>) -> Result<()> {
    restore_previous_handler_node(
        ctx,
        "_php_error_handler_stack",
        "_php_error_handler_value",
        "_php_error_handler_callable",
        Some("_php_error_handler_mask"),
        &[
            "_php_error_handler_context",
            "_php_error_handler_context_release",
        ],
    );
    emit_bool_result(ctx, true);
    Ok(())
}

/// Restores the preceding AOT uncaught-exception handler, if present.
pub(super) fn lower_restore_exception_handler(ctx: &mut FunctionContext<'_>) -> Result<()> {
    restore_previous_handler_node(
        ctx,
        "_php_exception_handler_stack",
        "_php_exception_handler_value",
        "_php_exception_handler_callable",
        None,
        &[
            "_php_exception_handler_context",
            "_php_exception_handler_context_release",
        ],
    );
    emit_bool_result(ctx, true);
    Ok(())
}

/// Dispatches a user-level diagnostic through the active handler or PHP's default path.
pub(super) fn lower_trigger_error(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let message = expect_operand(inst, 0)?;
    let level = expect_operand(inst, 1)?;
    let file = expect_operand(inst, 2)?;
    let line = expect_operand(inst, 3)?;
    let default_label = ctx.next_label("trigger_error_default");
    let done_label = ctx.next_label("trigger_error_done");
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    emit_validate_trigger_error_level(ctx, level)?;
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        descriptor_reg,
        "_php_error_handler_callable",
        0,
    );
    emit_branch_if_zero(ctx, descriptor_reg, &default_label);
    load_integer_operand(ctx, level)?;
    let mask_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, mask_reg, "_php_error_handler_mask", 0);
    emit_branch_if_no_mask_overlap(
        ctx,
        mask_reg,
        abi::int_result_reg(ctx.emitter),
        &default_label,
    );
    super::super::callables::emit_descriptor_reg_invoker_mixed_result_with_args(
        ctx,
        descriptor_reg,
        &[level, message, file, line],
        "trigger_error_handler",
        false,
    )?;
    emit_release_handler_result_and_branch_on_false(ctx, &default_label);
    emit_bool_result(ctx, true);
    ctx.emitter.instruction(&branch_instruction(ctx, &done_label));             // skip PHP's default diagnostic after handler suppression
    ctx.emitter.label(&default_label);
    emit_default_user_error(ctx, message, level, file, line)?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Rejects every error level outside PHP's four user-generated categories.
fn emit_validate_trigger_error_level(
    ctx: &mut FunctionContext<'_>,
    level: ValueId,
) -> Result<()> {
    load_integer_operand(ctx, level)?;
    let level_reg = abi::int_result_reg(ctx.emitter);
    let valid = ctx.next_label("trigger_error_level_valid");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            for accepted in [
                E_USER_ERROR,
                E_USER_WARNING,
                E_USER_NOTICE,
                E_USER_DEPRECATED,
            ] {
                ctx.emitter
                    .instruction(&format!("cmp {level_reg}, #{accepted}"));     // compare against one PHP user-error category
                ctx.emitter.instruction(&format!("b.eq {valid}"));             // any recognized user category is valid
            }
        }
        Arch::X86_64 => {
            for accepted in [
                E_USER_ERROR,
                E_USER_WARNING,
                E_USER_NOTICE,
                E_USER_DEPRECATED,
            ] {
                ctx.emitter
                    .instruction(&format!("cmp {level_reg}, {accepted}"));      // compare against one PHP user-error category
                ctx.emitter.instruction(&format!("je {valid}"));               // any recognized user category is valid
            }
        }
    }
    super::super::exceptions::emit_value_error(ctx, INVALID_TRIGGER_LEVEL_MESSAGE);
    ctx.emitter.label(&valid);
    Ok(())
}

/// Allocates a linked node and transfers the current handler state into it.
fn allocate_previous_handler_node(
    ctx: &mut FunctionContext<'_>,
    node_bytes: i64,
    stack_symbol: &str,
    value_symbol: &str,
    callable_symbol: &str,
    mask_symbol: Option<&str>,
    extra_symbols: &[&str],
) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        node_bytes,
    );
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    let node_reg = abi::nested_call_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, node_reg, abi::int_result_reg(ctx.emitter));
    for (offset, symbol) in [
        (0, stack_symbol),
        (8, value_symbol),
        (16, callable_symbol),
    ] {
        let scratch = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_load_symbol_to_reg(ctx.emitter, scratch, symbol, 0);
        abi::emit_store_to_address(ctx.emitter, scratch, node_reg, offset);
    }
    if let Some(mask_symbol) = mask_symbol {
        let scratch = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_load_symbol_to_reg(ctx.emitter, scratch, mask_symbol, 0);
        abi::emit_store_to_address(ctx.emitter, scratch, node_reg, 24);
    }
    let extra_base = if mask_symbol.is_some() { 32 } else { 24 };
    for (index, symbol) in extra_symbols.iter().enumerate() {
        let scratch = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_load_symbol_to_reg(ctx.emitter, scratch, symbol, 0);
        abi::emit_store_to_address(ctx.emitter, scratch, node_reg, extra_base + index * 8);
    }
    abi::emit_store_reg_to_symbol(ctx.emitter, node_reg, stack_symbol, 0);
}

/// Retains the old callback for the return value while its original owner moves to the stack.
fn preserve_previous_callback_result(ctx: &mut FunctionContext<'_>, value_symbol: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, value_symbol, 0);
    let no_retain = ctx.next_label("handler_previous_null");
    emit_branch_if_zero(ctx, result_reg, &no_retain);
    callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    ctx.emitter.label(&no_retain);
    abi::emit_push_reg(ctx.emitter, result_reg);
}

/// Restores the staged previous callback, boxing PHP null for an empty initial state.
fn restore_previous_callback_result(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    let done = ctx.next_label("handler_previous_result_done");
    emit_branch_if_nonzero(ctx, result_reg, &done);
    emit_null_mixed_result(ctx);
    ctx.emitter.label(&done);
}

/// Retains one boxed Mixed callback and installs it in the selected global slot.
fn retain_and_store_mixed(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    symbol: &str,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        symbol,
        0,
    );
    Ok(())
}

/// Retains one normalized callable descriptor and installs it in a global slot.
fn retain_and_store_descriptor(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    symbol: &str,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        symbol,
        0,
    );
    Ok(())
}

/// Releases the current handler and transfers the top linked-node state back to globals.
fn restore_previous_handler_node(
    ctx: &mut FunctionContext<'_>,
    stack_symbol: &str,
    value_symbol: &str,
    callable_symbol: &str,
    mask_symbol: Option<&str>,
    extra_symbols: &[&str],
) {
    let node_reg = abi::nested_call_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, node_reg, stack_symbol, 0);
    let done = ctx.next_label("restore_handler_done");
    emit_branch_if_zero(ctx, node_reg, &done);

    abi::emit_push_reg(ctx.emitter, node_reg);
    release_global_mixed(ctx, value_symbol);
    release_global_descriptor(ctx, callable_symbol);
    if let [context_symbol, release_symbol, ..] = extra_symbols {
        release_handler_context(ctx, context_symbol, release_symbol);
    }
    abi::emit_pop_reg(ctx.emitter, node_reg);

    for (offset, symbol) in [
        (0, stack_symbol),
        (8, value_symbol),
        (16, callable_symbol),
    ] {
        let scratch = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_load_from_address(ctx.emitter, scratch, node_reg, offset);
        abi::emit_store_reg_to_symbol(ctx.emitter, scratch, symbol, 0);
    }
    if let Some(mask_symbol) = mask_symbol {
        let scratch = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_load_from_address(ctx.emitter, scratch, node_reg, 24);
        abi::emit_store_reg_to_symbol(ctx.emitter, scratch, mask_symbol, 0);
    }
    let extra_base = if mask_symbol.is_some() { 32 } else { 24 };
    for (index, symbol) in extra_symbols.iter().enumerate() {
        let scratch = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_load_from_address(ctx.emitter, scratch, node_reg, extra_base + index * 8);
        abi::emit_store_reg_to_symbol(ctx.emitter, scratch, symbol, 0);
    }
    abi::emit_reg_move(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        node_reg,
    );
    abi::emit_call_label(ctx.emitter, "__rt_heap_free");
    ctx.emitter.label(&done);
}

/// Releases the eval context owner carried by the selected active handler.
fn release_handler_context(
    ctx: &mut FunctionContext<'_>,
    context_symbol: &str,
    release_symbol: &str,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let release_reg = abi::nested_call_reg(ctx.emitter);
    let done = ctx.next_label("release_exception_handler_context_done");
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        result_reg,
        release_symbol,
        0,
    );
    emit_branch_if_zero(ctx, result_reg, &done);
    abi::emit_reg_move(ctx.emitter, release_reg, result_reg);
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        context_symbol,
        0,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("blr {release_reg}")), // release the retained magician context owner
        Arch::X86_64 => ctx.emitter.instruction(&format!("call {release_reg}")), // release the retained magician context owner
    }
    ctx.emitter.label(&done);
    abi::emit_store_zero_to_symbol(ctx.emitter, context_symbol, 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, release_symbol, 0);
}

/// Releases the boxed Mixed value owned by one runtime global and clears the slot.
fn release_global_mixed(ctx: &mut FunctionContext<'_>, symbol: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, symbol, 0);
    let done = ctx.next_label("release_handler_mixed_done");
    emit_branch_if_zero(ctx, result_reg, &done);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    ctx.emitter.label(&done);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, symbol, 0);
}

/// Releases the callable descriptor owned by one runtime global and clears the slot.
fn release_global_descriptor(ctx: &mut FunctionContext<'_>, symbol: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, symbol, 0);
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, symbol, 0);
}

/// Loads an integer operand, casting boxed Mixed values with PHP semantics.
fn load_integer_operand(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let ty = ctx.raw_value_php_type(value)?.codegen_repr();
    ctx.load_value_to_result(value)?;
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
    }
    Ok(())
}

/// Releases a callback result and falls through to the default path only for exact false.
fn emit_release_handler_result_and_branch_on_false(
    ctx: &mut FunctionContext<'_>,
    default_label: &str,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let exact_false = ctx.next_label("trigger_handler_exact_false");
    let classified = ctx.next_label("trigger_handler_result_classified");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #3");                              // only the bool runtime tag can request default handling
            ctx.emitter.instruction(&format!("b.ne {classified}"));             // every non-bool return suppresses the default handler
            ctx.emitter
                .instruction(&format!("cbz x1, {exact_false}"));               // boolean false requests the built-in diagnostic path
            ctx.emitter.instruction(&format!("b {classified}"));                // boolean true suppresses the built-in diagnostic
            ctx.emitter.label(&exact_false);
            ctx.emitter.instruction("mov x0, #1");                              // stage the exact-false classification
            let staged = ctx.next_label("trigger_handler_flag_staged");
            ctx.emitter.instruction(&format!("b {staged}"));                    // preserve the exact-false classification
            ctx.emitter.label(&classified);
            ctx.emitter.instruction("mov x0, #0");                              // stage the suppress-default classification
            ctx.emitter.label(&staged);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 3");                              // only the bool runtime tag can request default handling
            ctx.emitter.instruction(&format!("jne {classified}"));              // every non-bool return suppresses the default handler
            ctx.emitter.instruction("test rdi, rdi");                           // inspect the unboxed boolean payload
            ctx.emitter.instruction(&format!("jz {exact_false}"));              // boolean false requests the built-in diagnostic path
            ctx.emitter.instruction(&format!("jmp {classified}"));              // boolean true suppresses the built-in diagnostic
            ctx.emitter.label(&exact_false);
            ctx.emitter.instruction("mov rax, 1");                              // stage the exact-false classification
            let staged = ctx.next_label("trigger_handler_flag_staged");
            ctx.emitter.instruction(&format!("jmp {staged}"));                  // preserve the exact-false classification
            ctx.emitter.label(&classified);
            ctx.emitter.instruction("xor eax, eax");                            // stage the suppress-default classification
            ctx.emitter.label(&staged);
        }
    }
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    let flag_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, flag_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    emit_branch_if_nonzero(ctx, flag_reg, default_label);
}

/// Emits PHP's default user-error path and terminates for an unhandled E_USER_ERROR.
fn emit_default_user_error(
    ctx: &mut FunctionContext<'_>,
    message: ValueId,
    level: ValueId,
    file: ValueId,
    line: ValueId,
) -> Result<()> {
    let skip_output = ctx.next_label("trigger_error_skip_output");
    load_integer_operand(ctx, level)?;
    let level_reg = abi::nested_call_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, level_reg, abi::int_result_reg(ctx.emitter));
    let mask_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, mask_reg, "_php_error_reporting", 0);
    emit_branch_if_no_mask_overlap(ctx, mask_reg, level_reg, &skip_output);
    emit_user_error_category(ctx, level_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.load_string_value_to_regs(message, "x1", "x2")?,
        Arch::X86_64 => ctx.load_string_value_to_regs(message, "rdi", "rsi")?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    emit_user_error_fragment(ctx, b" in ");
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.load_string_value_to_regs(file, "x1", "x2")?,
        Arch::X86_64 => ctx.load_string_value_to_regs(file, "rdi", "rsi")?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    emit_user_error_fragment(ctx, b" on line ");
    load_integer_operand(ctx, line)?;
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {}
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the formatted line-number pointer to the diagnostic helper
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the formatted line-number length to the diagnostic helper
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    emit_user_error_fragment(ctx, b"\n");
    ctx.emitter.label(&skip_output);
    load_integer_operand(ctx, level)?;
    let level_reg = abi::int_result_reg(ctx.emitter);
    emit_exit_if_user_error(ctx, level_reg);
    emit_bool_result(ctx, true);
    Ok(())
}

/// Writes the PHP diagnostic category selected by one validated user-error level.
fn emit_user_error_category(ctx: &mut FunctionContext<'_>, level_reg: &str) {
    let warning = ctx.next_label("trigger_error_category_warning");
    let deprecated = ctx.next_label("trigger_error_category_deprecated");
    let notice = ctx.next_label("trigger_error_category_notice");
    let done = ctx.next_label("trigger_error_category_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {level_reg}, #{E_USER_ERROR}"));     // select Fatal error for E_USER_ERROR
            ctx.emitter.instruction(&format!("b.ne {warning}"));               // inspect the remaining nonfatal categories
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {level_reg}, {E_USER_ERROR}"));      // select Fatal error for E_USER_ERROR
            ctx.emitter.instruction(&format!("jne {warning}"));                // inspect the remaining nonfatal categories
        }
    }
    emit_user_error_fragment(ctx, b"Fatal error: ");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&warning);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {level_reg}, #{E_USER_WARNING}"));   // select Warning for E_USER_WARNING
            ctx.emitter.instruction(&format!("b.ne {deprecated}"));            // inspect deprecation and notice categories next
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {level_reg}, {E_USER_WARNING}"));    // select Warning for E_USER_WARNING
            ctx.emitter.instruction(&format!("jne {deprecated}"));             // inspect deprecation and notice categories next
        }
    }
    emit_user_error_fragment(ctx, b"Warning: ");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&deprecated);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {level_reg}, #{E_USER_DEPRECATED}")); // select Deprecated for E_USER_DEPRECATED
            ctx.emitter.instruction(&format!("b.ne {notice}"));                // the only remaining validated level is E_USER_NOTICE
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {level_reg}, {E_USER_DEPRECATED}")); // select Deprecated for E_USER_DEPRECATED
            ctx.emitter.instruction(&format!("jne {notice}"));                 // the only remaining validated level is E_USER_NOTICE
        }
    }
    emit_user_error_fragment(ctx, b"Deprecated: ");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&notice);
    emit_user_error_fragment(ctx, b"Notice: ");
    ctx.emitter.label(&done);
}

/// Writes one static fragment through the suppressible PHP diagnostic channel.
fn emit_user_error_fragment(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}

/// Exits with PHP's fatal status when the unhandled level is E_USER_ERROR.
fn emit_exit_if_user_error(ctx: &mut FunctionContext<'_>, level_reg: &str) {
    let done = ctx.next_label("trigger_error_nonfatal");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {level_reg}, #{E_USER_ERROR}"));    // distinguish the fatal user-error category
            ctx.emitter.instruction(&format!("b.ne {done}"));                   // notices, warnings, and deprecations remain nonfatal
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {level_reg}, {E_USER_ERROR}"));     // distinguish the fatal user-error category
            ctx.emitter.instruction(&format!("jne {done}"));                    // notices, warnings, and deprecations remain nonfatal
        }
    }
    abi::emit_exit(ctx.emitter, 255);
    ctx.emitter.label(&done);
}

/// Branches when a register is zero on the active target.
fn emit_branch_if_zero(ctx: &mut FunctionContext<'_>, reg: &str, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbz {reg}, {label}")), // branch when the selected handler pointer is null
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {reg}, {reg}"));             // set flags from the selected handler pointer
            ctx.emitter.instruction(&format!("jz {label}"));                    // branch when the selected handler pointer is null
        }
    }
}

/// Branches when a register is nonzero on the active target.
fn emit_branch_if_nonzero(ctx: &mut FunctionContext<'_>, reg: &str, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbnz {reg}, {label}")), // branch when the selected handler pointer is present
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {reg}, {reg}"));             // set flags from the selected handler pointer
            ctx.emitter.instruction(&format!("jnz {label}"));                   // branch when the selected handler pointer is present
        }
    }
}

/// Branches when two integer masks have no common enabled bit.
fn emit_branch_if_no_mask_overlap(
    ctx: &mut FunctionContext<'_>,
    lhs_reg: &str,
    rhs_reg: &str,
    label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx
            .emitter
            .instruction(&format!("tst {lhs_reg}, {rhs_reg}")),
        Arch::X86_64 => ctx
            .emitter
            .instruction(&format!("test {lhs_reg}, {rhs_reg}")),
    }
    ctx.emitter.instruction(&format!(                                           // branch when the two error masks do not overlap
        "{} {label}", match ctx.emitter.target.arch {
        Arch::AArch64 => "b.eq",
        Arch::X86_64 => "jz",
    }));
}

/// Returns the unconditional branch mnemonic for the active target.
fn branch_instruction(ctx: &FunctionContext<'_>, label: &str) -> String {
    match ctx.emitter.target.arch {
        Arch::AArch64 => format!("b {label}"),
        Arch::X86_64 => format!("jmp {label}"),
    }
}
