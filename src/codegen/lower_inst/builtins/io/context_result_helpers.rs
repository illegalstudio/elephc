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
            load_string_to_result(ctx, value, "stream_context_set_option value")?;
            ctx.emitter.instruction("mov x4, x1");                              // pass the option value pointer as the fifth runtime argument
            ctx.emitter.instruction("mov x5, x2");                              // pass the option value length as the sixth runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "x2", "x3");
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, wrapper, "stream_context_set_option wrapper")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, option, "stream_context_set_option option")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, value, "stream_context_set_option value")?;
            ctx.emitter.instruction("mov r8, rax");                             // pass the option value pointer as the fifth runtime argument
            ctx.emitter.instruction("mov r9, rdx");                             // pass the option value length as the sixth runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    let target = if ctx.module.web_application {
        crate::names::function_symbol(crate::web_prelude::application::CONTEXT_OPTION)
    } else {
        "__rt_stream_context_set_option_4".to_string()
    };
    abi::emit_call_label(ctx.emitter, &target);
    Ok(())
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
    if ctx.module.web_application { normalize_application_options(ctx); }
    match ctx.emitter.target.arch {
        Arch::AArch64 => store_stream_context_options_aarch64(ctx, clear_on_null),
        Arch::X86_64 => store_stream_context_options_x86_64(ctx, clear_on_null),
    }
    Ok(())
}

/// Opens a physically boxed array before publishing its borrowed raw-container pointer.
fn normalize_application_options(ctx: &mut FunctionContext<'_>) {
    let boxed = ctx.next_label("application_options_boxed");
    let array = ctx.next_label("application_options_array");
    let null = ctx.next_label("application_options_null");
    let done = ctx.next_label("application_options_normalized");
    let value = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, value);
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #5");                              // heap kind five denotes Mixed storage, independently of its PHP value tag
            ctx.emitter.instruction(&format!("b.eq {boxed}"));                  // unwrap managed Mixed cells before storing options
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 5");                              // distinguish Mixed heap cells from concrete containers
            ctx.emitter.instruction(&format!("je {boxed}"));                    // leave raw packed and associative storage unchanged
        }
    }
    abi::emit_pop_reg(ctx.emitter, value);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&boxed);
    abi::emit_pop_reg(ctx.emitter, value);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    for tag in [4, 5, 8] {
        let label = if tag == 8 { &null } else { &array };
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp x0, #{tag}"));            // accept arrays and the optional null default
                ctx.emitter.instruction(&format!("b.eq {label}"));              // select the payload representation
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp rax, {tag}"));            // inspect the unboxed PHP value tag
                ctx.emitter.instruction(&format!("je {label}"));                // distinguish array payloads from null
            }
        }
    }
    crate::codegen::lower_inst::exceptions::emit_type_error(ctx, "Stream context options must be of type array");
    ctx.emitter.label(&array);
    let payload = if ctx.emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" };
    abi::emit_reg_move(ctx.emitter, value, payload);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&null);
    abi::emit_load_int_immediate(ctx.emitter, value, 0);
    ctx.emitter.label(&done);
}

/// Stores the loaded AArch64 options pointer into `_stream_context_options`.
pub(super) fn store_stream_context_options_aarch64(ctx: &mut FunctionContext<'_>, clear_on_null: bool) {
    if ctx.module.web_application {
        store_application_context_options(ctx, clear_on_null);
        return;
    }
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
    if ctx.module.web_application {
        store_application_context_options(ctx, clear_on_null);
        return;
    }
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

/// Clears the runtime's single stream-context options slot.
pub(super) fn clear_stream_context_options(ctx: &mut FunctionContext<'_>) {
    if ctx.module.web_application {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::emit_call_label(ctx.emitter, "__rt_web_context_options_store");
        return;
    }
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

/// Publishes application-mode context options while balancing the previous ambient owner.
fn store_application_context_options(ctx: &mut FunctionContext<'_>, clear_on_null: bool) {
    let done = ctx.next_label("application_context_options_stored");
    if !clear_on_null { abi::emit_branch_if_int_result_zero(ctx.emitter, &done); }
    abi::emit_call_label(ctx.emitter, "__rt_web_context_options_store");
    ctx.emitter.label(&done);
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
    let suffix = path.strip_prefix("php://fd/")?;
    suffix.parse::<i64>().ok()
}

/// Recognizes in-memory `php://` stream URLs backed by the temp-file helper.
pub(super) fn is_php_memory_stream(path: &str) -> bool {
    path == "php://memory" || path == "php://temp" || path.starts_with("php://temp/")
}

