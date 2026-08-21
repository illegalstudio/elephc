//! Purpose:
//! Context storage, static arrays, literal paths, and scalar results.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Verifies that a builtin call has a lowered operand count within an inclusive range.
pub(super) fn ensure_arg_count_between(inst: &Instruction, name: &str, min: usize, max: usize) -> Result<()> {
    let actual = inst.operands.len();
    if (min..=max).contains(&actual) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {}..={} args, got {}",
        name, min, max, actual
    )))
}

/// Loads the four-argument `stream_context_set_option` form into the runtime helper ABI.
pub(super) fn lower_stream_context_set_option_4(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let wrapper = expect_operand(inst, 1)?;
    let option = expect_operand(inst, 2)?;
    let value = expect_operand(inst, 3)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, wrapper, "stream_context_set_option wrapper")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, option, "stream_context_set_option option")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            materialize_owned_stream_context_option_mixed(ctx, value)?;
            ctx.emitter.instruction("mov x4, x0");                              // transfer the owned boxed Mixed option value
            ctx.emitter.instruction("mov x5, xzr");                             // boxed Mixed values use no high payload word
            abi::emit_pop_reg_pair(ctx.emitter, "x2", "x3");
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, wrapper, "stream_context_set_option wrapper")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, option, "stream_context_set_option option")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            materialize_owned_stream_context_option_mixed(ctx, value)?;
            ctx.emitter.instruction("mov r8, rax");                             // transfer the owned boxed Mixed option value
            ctx.emitter.instruction("xor r9, r9");                              // boxed Mixed values use no high payload word
            abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_context_set_option_4");
    Ok(())
}

/// Materializes one stream option as an owned boxed Mixed cell.
///
/// Concrete values are boxed with child retention. Values already represented
/// by a Mixed cell are unboxed and reboxed so the context owns a distinct PHP
/// value cell while retaining the same underlying COW payload. The runtime
/// option setter consumes the newly created owner.
fn materialize_owned_stream_context_option_mixed(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let raw_value_ty = ctx.raw_value_php_type(value)?;
    let value_ty = match raw_value_ty {
        PhpType::Resource(kind) => PhpType::Resource(kind),
        other => other.codegen_repr(),
    };
    ctx.load_value_to_result(value)?;
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        if ctx.emitter.target.arch == Arch::X86_64 {
            ctx.emitter.instruction("mov rsi, rdx");                            // move the unboxed high word into mixed_from_value's SysV input register
        }
        abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    Ok(())
}

/// Merges one EIR options hash into the owned construction scratch.
pub(super) fn merge_stream_context_options_into_scratch(
    ctx: &mut FunctionContext<'_>,
    options: ValueId,
) -> Result<()> {
    ctx.load_value_to_result(options)?;
    emit_merge_loaded_stream_context_options_into_scratch(ctx);
    Ok(())
}

/// Merges a present runtime `params['options']` map into options scratch.
///
/// The lookup accepts direct associative hashes and hashes boxed behind Mixed
/// storage. An absent or non-array entry leaves scratch unchanged.
pub(super) fn merge_stream_context_params_options_into_scratch(
    ctx: &mut FunctionContext<'_>,
    params: ValueId,
) -> Result<()> {
    let direct = ctx.next_label("sctx_params_options_direct");
    let merge = ctx.next_label("sctx_params_options_merge");
    let done = ctx.next_label("sctx_params_options_done");
    let (key, key_len) = ctx.data.add_string(b"options");
    ctx.load_value_to_result(params)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &key);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction(&format!("cbz x0, {}", done));              // absent params options leave the current context options unchanged
            ctx.emitter.instruction("cmp x3, #5");                              // is the params entry a direct associative wrapper map?
            ctx.emitter.instruction(&format!("b.eq {}", direct));               // direct maps expose their pointer in x1
            ctx.emitter.instruction("cmp x3, #7");                              // is the params entry boxed behind Mixed storage?
            ctx.emitter.instruction(&format!("b.ne {}", done));                 // ignore malformed non-array params options
            ctx.emitter.instruction("mov x0, x1");                              // pass the boxed params entry to Mixed unboxing
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("cmp x0, #5");                              // did the Mixed cell contain an associative wrapper map?
            ctx.emitter.instruction(&format!("b.ne {}", done));                 // ignore malformed boxed params options
            ctx.emitter.instruction("mov x0, x1");                              // move the unboxed map into the canonical result register
            ctx.emitter.instruction(&format!("b {}", merge));                   // join direct and boxed map paths
            ctx.emitter.label(&direct);
            ctx.emitter.instruction("mov x0, x1");                              // move the direct wrapper map into the canonical result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the runtime params hash to lookup
            abi::emit_symbol_address(ctx.emitter, "rsi", &key);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction("test rax, rax");                           // was a params options key present?
            ctx.emitter.instruction(&format!("jz {}", done));                   // absent params options preserve the current context options
            ctx.emitter.instruction("cmp rcx, 5");                              // is the entry a direct associative wrapper map?
            ctx.emitter.instruction(&format!("je {}", direct));                 // direct maps expose their pointer in rdi
            ctx.emitter.instruction("cmp rcx, 7");                              // is the entry boxed behind Mixed storage?
            ctx.emitter.instruction(&format!("jne {}", done));                  // ignore malformed non-array params options
            ctx.emitter.instruction("mov rax, rdi");                            // pass the boxed params entry to Mixed unboxing
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("cmp rax, 5");                              // did the Mixed cell contain an associative wrapper map?
            ctx.emitter.instruction(&format!("jne {}", done));                  // ignore malformed boxed params options
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed map into the canonical result register
            ctx.emitter.instruction(&format!("jmp {}", merge));                 // join direct and boxed map paths
            ctx.emitter.label(&direct);
            ctx.emitter.instruction("mov rax, rdi");                            // move the direct wrapper map into the canonical result register
        }
    }
    ctx.emitter.label(&merge);
    emit_merge_loaded_stream_context_options_into_scratch(ctx);
    ctx.emitter.label(&done);
    Ok(())
}

/// Merges the loaded incoming wrapper map into the owned options scratch.
///
/// The dedicated runtime helper returns a fresh COW root. This emitter releases
/// the previous scratch owner only after the replacement is complete, then
/// publishes the returned owner for later transfer into `ContextState`.
fn emit_merge_loaded_stream_context_options_into_scratch(
    ctx: &mut FunctionContext<'_>,
) {
    let old_released = ctx.next_label("sctx_merge_old_released");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x0, [x9]");                            // pass the current owned wrapper map as the left merge input
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_call_label(ctx.emitter, "__rt_stream_context_merge_options");
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x0, [x9]");                            // load the previous scratch owner for release
            ctx.emitter.instruction(&format!("cbz x0, {}", old_released));      // an empty scratch has no owner to release
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.label(&old_released);
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("str x0, [x9]");                            // publish the newly owned merged wrapper map
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov rdi, QWORD PTR [r9]");                 // pass the current owned wrapper map as the left merge input
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_call_label(ctx.emitter, "__rt_stream_context_merge_options");
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9]");                 // load the previous scratch owner for release
            ctx.emitter.instruction("test rax, rax");                           // does the previous scratch own a wrapper map?
            ctx.emitter.instruction(&format!("jz {}", old_released));           // an empty scratch has no owner to release
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.label(&old_released);
            abi::emit_pop_reg(ctx.emitter, "rax");
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov QWORD PTR [r9], rax");                 // publish the newly owned merged wrapper map
        }
    }
}

/// Stores an options heap pointer in the runtime's single stream-context slot.
pub(super) fn store_stream_context_options(
    ctx: &mut FunctionContext<'_>,
    options: ValueId,
    clear_on_null: bool,
) -> Result<()> {
    if matches!(
        ctx.raw_value_php_type(options)?.codegen_repr(),
        PhpType::Void | PhpType::Never
    ) {
        if clear_on_null {
            clear_stream_context_options(ctx);
        }
        return Ok(());
    }
    ctx.load_value_to_result(options)?;
    // php keeps only entries whose key is a STRING and whose value is an ARRAY, and raises a
    // catchable `ValueError` for anything else — `['ssl' => "abc"]`, `['ssl' => 1]`,
    // `['ssl' => null]` and `[0 => [...]]` all measured on `php -n` 8.5.6. elephc stored the
    // malformed map in silence, so a typo in a context array produced a context carrying nothing.
    // The guard runs on the LOADED pointer, before it is published, so a refused map never
    // reaches the runtime slot.
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_stream_context_options_shape_ok");
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedAtLeast(result_reg, 1),
        STREAM_CONTEXT_OPTIONS_SHAPE_MESSAGE,
    );
    abi::emit_pop_reg(ctx.emitter, result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => store_stream_context_options_aarch64(ctx, clear_on_null),
        Arch::X86_64 => store_stream_context_options_x86_64(ctx, clear_on_null),
    }
    Ok(())
}

/// php-src's verbatim `ValueError` for a stream-context options array of the wrong shape.
pub(super) const STREAM_CONTEXT_OPTIONS_SHAPE_MESSAGE: &str =
    "Options should have the form [\"wrappername\"][\"optionname\"] = $value";

/// Stores the loaded AArch64 options pointer into `_stream_context_options`.
pub(super) fn store_stream_context_options_aarch64(ctx: &mut FunctionContext<'_>, clear_on_null: bool) {
    let skip_label = ctx.next_label("sctx_store_done");
    if clear_on_null {
        let zero_label = ctx.next_label("sctx_store_zero");
        ctx.emitter.instruction(&format!("cbz x0, {}", zero_label));            // clear the context slot when a null options value is passed
        abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
        ctx.emitter.instruction("str x0, [x9]");                                // persist the options heap pointer globally
        abi::emit_call_label(ctx.emitter, "__rt_incref");
        ctx.emitter.instruction(&format!("b {}", skip_label));                  // skip the null-clearing fallback after retaining options
        ctx.emitter.label(&zero_label);
        clear_stream_context_options(ctx);
        ctx.emitter.label(&skip_label);
        return;
    }
    ctx.emitter.instruction(&format!("cbz x0, {}", skip_label));                // leave the context slot unchanged for null options
    abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
    ctx.emitter.instruction("str x0, [x9]");                                    // persist the options heap pointer globally
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&skip_label);
}

/// Stores the loaded x86_64 options pointer into `_stream_context_options`.
pub(super) fn store_stream_context_options_x86_64(ctx: &mut FunctionContext<'_>, clear_on_null: bool) {
    let skip_label = ctx.next_label("sctx_store_done_x86");
    if clear_on_null {
        let zero_label = ctx.next_label("sctx_store_zero_x86");
        ctx.emitter.instruction("test rax, rax");                               // check whether the options pointer is null
        ctx.emitter.instruction(&format!("jz {}", zero_label));                 // clear the context slot when a null options value is passed
        abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
        ctx.emitter.instruction("mov QWORD PTR [r9], rax");                     // persist the options heap pointer globally
        ctx.emitter.instruction("mov rdi, rax");                                // pass the options pointer to incref
        abi::emit_call_label(ctx.emitter, "__rt_incref");
        ctx.emitter.instruction(&format!("jmp {}", skip_label));                // skip the null-clearing fallback after retaining options
        ctx.emitter.label(&zero_label);
        clear_stream_context_options(ctx);
        ctx.emitter.label(&skip_label);
        return;
    }
    ctx.emitter.instruction("test rax, rax");                                   // check whether the options pointer is null
    ctx.emitter.instruction(&format!("jz {}", skip_label));                     // leave the context slot unchanged for null options
    abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
    ctx.emitter.instruction("mov QWORD PTR [r9], rax");                         // persist the options heap pointer globally
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the options pointer to incref
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&skip_label);
}

/// Transfers the retained construction-slot options into one `ContextState`.
///
/// The global slot is only scratch storage while lowering the options value.
/// Each context owns its own hash reference, and replacement releases the
/// previous state child after the new pointer has been installed.
pub(super) fn update_stream_context_state_from_handle(
    ctx: &mut FunctionContext<'_>,
    context: ValueId,
) -> Result<()> {
    load_context_state_to_result(ctx, context, "stream_context_set_option")?;
    emit_transfer_stream_context_options_to_loaded_state(ctx);
    Ok(())
}

/// Transfers retained options scratch into the `ContextState` in the result register.
pub(super) fn emit_transfer_stream_context_options_to_loaded_state(ctx: &mut FunctionContext<'_>) {
    let skip_label = ctx.next_label("sctx_update_skip");
    let done_label = ctx.next_label("sctx_update_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", skip_label));        // ignore an invalid or stale context handle
            ctx.emitter.instruction("mov x10, x0");                             // preserve ContextState while loading construction scratch
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x11, [x9]");                           // take the retained replacement options pointer
            ctx.emitter.instruction("str xzr, [x9]");                           // transfer scratch ownership into ContextState
            ctx.emitter.instruction("ldr x0, [x10]");                           // load the previously owned options hash
            ctx.emitter.instruction("str x11, [x10]");                          // publish the replacement before releasing the old hash
            ctx.emitter.instruction(&format!("cbz x0, {}", done_label));        // skip release when the context had no options
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction(&format!("b {}", done_label));              // join the state-update epilogue
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the context handle resolve?
            ctx.emitter.instruction(&format!("jz {}", skip_label));
            ctx.emitter.instruction("mov r10, rax");                            // preserve ContextState while loading construction scratch
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov r11, QWORD PTR [r9]");                 // take the retained replacement options pointer
            ctx.emitter.instruction("mov QWORD PTR [r9], 0");                   // transfer scratch ownership into ContextState
            ctx.emitter.instruction("mov rax, QWORD PTR [r10]");                // load the previously owned options hash
            ctx.emitter.instruction("mov QWORD PTR [r10], r11");                // publish the replacement before releasing the old hash
            ctx.emitter.instruction("test rax, rax");                           // did the context own an earlier hash?
            ctx.emitter.instruction(&format!("jz {}", done_label));             // skip release for an empty state
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // join the state-update epilogue
        }
    }
    ctx.emitter.label(&skip_label);
    ctx.emitter.label(&done_label);
}

/// Restores stream-context options from the handle's stable `ContextState`.
///
/// The global slot remains transient scratch for legacy open helpers; it never
/// owns this borrowed pointer. Context identity and ownership stay in the
/// registry state.
pub(super) fn restore_stream_context_from_handle(
    ctx: &mut FunctionContext<'_>,
    context: ValueId,
) -> Result<()> {
    let skip_label = ctx.next_label("sctx_restore_skip");
    load_context_state_to_result(ctx, context, "fopen")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", skip_label));        // invalid or stale context handles carry no options
            ctx.emitter.instruction("ldr x1, [x0]");                            // load the borrowed ContextState.options pointer
            ctx.emitter.instruction(&format!("cbz x1, {}", skip_label));        // an empty context leaves construction scratch unchanged
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("str x1, [x9]");                            // expose the borrowed hash to the legacy open helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test rax, rax"));
            ctx.emitter.instruction(&format!("jz {}", skip_label));             // invalid or stale context handles carry no options
            ctx.emitter.instruction("mov rcx, QWORD PTR [rax]");                // load the borrowed ContextState.options pointer
            ctx.emitter.instruction(&format!("test rcx, rcx"));
            ctx.emitter.instruction(&format!("jz {}", skip_label));             // an empty context leaves construction scratch unchanged
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov QWORD PTR [r9], rcx");                 // expose the borrowed hash to the legacy open helper
        }
    }
    ctx.emitter.label(&skip_label);
    Ok(())
}

/// Clears the runtime's single stream-context options slot.
pub(super) fn clear_stream_context_options(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("str xzr, [x9]");                           // clear the persisted stream-context options pointer
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov QWORD PTR [r9], 0");                   // clear the persisted stream-context options pointer
        }
    }
}

/// Retains the transient options scratch before a helper may replace it through COW.
pub(super) fn retain_stream_context_options_scratch(ctx: &mut FunctionContext<'_>) {
    let done_label = ctx.next_label("sctx_scratch_retain_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x0, [x9]");                            // load the borrowed ContextState.options pointer
            ctx.emitter.instruction(&format!("cbz x0, {}", done_label));        // empty scratch has no ownership to acquire
            abi::emit_call_label(ctx.emitter, "__rt_incref");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9]");                 // load the borrowed ContextState.options pointer
            ctx.emitter.instruction("test rax, rax");                           // is there an options hash to retain?
            ctx.emitter.instruction(&format!("jz {}", done_label));             // empty scratch has no ownership to acquire
            ctx.emitter.instruction("mov rdi, rax");                            // pass the options hash to heap incref
            abi::emit_call_label(ctx.emitter, "__rt_incref");
        }
    }
    ctx.emitter.label(&done_label);
}

/// Emits an empty associative hash with Mixed values as the current result.
pub(super) fn emit_empty_mixed_hash(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #1");                              // pass the empty hash's initial capacity
            ctx.emitter.instruction("mov x1, #7");                              // select Mixed values for the empty hash
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 1");                              // pass the empty hash's initial capacity
            ctx.emitter.instruction("mov esi, 7");                              // select Mixed values for the empty hash
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
}

/// Emits an indexed string array from static names as the current result.
pub(super) fn emit_static_string_array(ctx: &mut FunctionContext<'_>, names: &[&str]) {
    let capacity = names.len().max(1);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 16);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 16);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_static_string_array_fill_aarch64(ctx, names),
        Arch::X86_64 => emit_static_string_array_fill_x86_64(ctx, names),
    }
}

/// Appends static strings to the current result array on AArch64.
pub(super) fn emit_static_string_array_fill_aarch64(ctx: &mut FunctionContext<'_>, names: &[&str]) {
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the string array while appending entries
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the string array for this append
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown string array
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final string array as the result
}

/// Appends static strings to the current result array on x86_64.
pub(super) fn emit_static_string_array_fill_x86_64(ctx: &mut FunctionContext<'_>, names: &[&str]) {
    ctx.emitter.instruction("push rax");                                        // park the string array while appending entries
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across append helper calls
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the string array for this append
        abi::emit_symbol_address(ctx.emitter, "rsi", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown string array
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final string array as the result
}

/// Emits a stream descriptor as the current integer/resource result.
pub(super) fn emit_fd_result(ctx: &mut FunctionContext<'_>, fd: i64) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), fd);
}

/// Emits `dup(fd)` and leaves the FRESH descriptor as the current integer result.
///
/// A `php://stdout` handle must not be the process's own descriptor. php-src's
/// `php://` wrapper duplicates it (`ext/standard/php_fopen_wrapper.c`), so closing the
/// stream closes only the copy. Handing out descriptor 1 directly made
/// `fclose(fopen('php://stdout', 'w'))` close the program's real standard output: every
/// later `echo` was written to a closed descriptor and silently discarded, while the
/// program still exited 0.
///
/// `dup` returning `-1` flows into the caller's `false` boxing like any other failed
/// open. The call is safe here because the descriptor is boxed through
/// `__rt_mixed_from_value` immediately afterwards, so this path already contains a call
/// and the enclosing frame already preserves the link register.
pub(super) fn emit_dup_fd_result(ctx: &mut FunctionContext<'_>, fd: i64) {
    let arg_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x0",
        Arch::X86_64 => "rdi",
    };
    abi::emit_load_int_immediate(ctx.emitter, arg_reg, fd);
    ctx.emitter.bl_c("dup");                                                    // hand out a copy, never the process's own descriptor
}

/// Emits the `php://fd/N` open, which is a `dup()` that SAYS why it failed.
///
/// The three standard streams reach [`emit_dup_fd_result`] instead: their descriptors always
/// exist, so php has no refusal to word for them. A descriptor the URL names is different —
/// php checks it against `getdtablesize()` and reports a failed `dup()` with the errno NUMBER
/// as well as its text — and neither the bound nor the errno is known at compile time, so the
/// whole open goes through the runtime helper and the URL travels with it for the message.
pub(super) fn emit_php_fd_open_result(ctx: &mut FunctionContext<'_>, fd: i64, path: &str) {
    let (path_symbol, path_len) = ctx.data.add_string(path.as_bytes());
    let (fd_reg, url_reg, len_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi", "rdx"),
    };
    abi::emit_load_int_immediate(ctx.emitter, fd_reg, fd);
    abi::emit_symbol_address(ctx.emitter, url_reg, &path_symbol);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, path_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_php_fd_open");
}

/// Emits a boolean scalar as the current integer result.
pub(super) fn emit_bool_result(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
}

/// Returns a literal string operand when the value was produced by `ConstStr`.
pub(super) fn optional_const_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<String>> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "string literal operand has no data id",
        ));
    };
    Ok(Some(
        ctx.module
            .data
            .strings
            .get(data.as_raw() as usize)
            .cloned()
            .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?,
    ))
}

/// Maps statically-known `php://` standard-stream URLs to native descriptors.
pub(super) fn php_standard_stream_fd(path: &str) -> Option<i64> {
    match path {
        "php://stdin" | "php://input" => Some(0),
        "php://stdout" | "php://output" => Some(1),
        "php://stderr" => Some(2),
        _ => None,
    }
}

/// Recognizes `php://fd/N` URLs and returns the descriptor embedded in the URL.
pub(super) fn php_fd_stream(path: &str) -> Option<i64> {
    php_fd_number(path.strip_prefix("php://fd/")?)
}

/// Reads the descriptor php-src's `php_stream_url_wrap_php` reads out of a `php://fd/` URL.
///
/// php-src runs `ZEND_STRTOL(start, &end, 10)` and refuses the URL with its own FORM sentence
/// when `end == start` or `*end != '\0'`; anything that DOES parse — including a negative number
/// and a leading-zero spelling — goes on to the range check, which words its refusal differently.
/// Measured on `php -n` 8.5.6: `php://fd/abc` and `php://fd/12abc` get the form sentence,
/// `php://fd/-1` gets the range sentence, and `php://fd/099` is descriptor 99.
///
/// `strtol` also skips leading whitespace, so php opens `php://fd/ 1`; this reader does not, and
/// answers the FORM sentence for it. The run-time dispatch in `__rt_php_fd_open`'s caller reads
/// the same shape, so the two agree with each other — which is the property that matters more
/// here than a space inside a URL.
///
/// The accumulation wraps rather than saturating, for the same reason: the assembly parser
/// multiplies and adds without an overflow check, and a URL spelling thirty digits should not
/// mean two different things depending on which dispatch saw it.
pub(super) fn php_fd_number(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    let (negative, digits) = match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut value: i64 = 0;
    for byte in digits {
        value = value
            .wrapping_mul(10)
            .wrapping_add(i64::from(byte - b'0'));
    }
    Some(if negative { value.wrapping_neg() } else { value })
}

/// Recognizes in-memory `php://` stream URLs backed by the temp-file helper.
pub(super) fn is_php_memory_stream(path: &str) -> bool {
    path == "php://memory" || path == "php://temp" || path.starts_with("php://temp/")
}

