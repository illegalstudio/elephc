//! Purpose:
//! Lowers the PHP output-buffering (`ob_*`) builtins for the EIR backend by
//! marshalling into the `__rt_ob_*` runtime helpers and boxing their results.
//!
//! Called from:
//! - The per-builtin `lower` hooks in `crate::builtins::io::ob_*`, via
//!   `crate::codegen::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - `ob_start` ignores its already-evaluated operands: the checker rejects a
//!   non-null handler callback at compile time, and `chunk_size`/`flags` have no
//!   effect because elephc buffers are unchunked with the standard flags.
//! - String-or-false results (`ob_get_contents`/`ob_get_clean`/`ob_get_flush`)
//!   reuse `io::box_owned_string_or_false_result` on the null-pointer failure
//!   convention; `ob_get_length` boxes its -1 sentinel to PHP `false`.
//! - `ob_get_status` boxes the raw hash pointer as a Mixed associative array
//!   (runtime tag 5), mirroring `getdate`/`localtime`; `ob_list_handlers`
//!   returns the raw string-array handle like `hash_algos`.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::codegen_support::callable_descriptor;
use crate::codegen_support::callable_dispatch;
use crate::codegen_support::runtime::{
    OB_CLOSURE_INVOKE_NAME, OB_DEFAULT_HANDLER_NAME, OB_NTC_CREATE_FAIL,
    OB_WARN_BAD_CALLBACK_GENERIC, OB_WARN_BAD_CALLBACK_PREFIX, OB_WARN_BAD_CALLBACK_SUFFIX,
};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::callables::runtime_string_descriptor_cases;
use super::super::super::context::FunctionContext;
use super::{load_value_to_first_int_arg, store_if_result};

/// Lowers `ob_start([$callback[, $chunk_size[, $flags]]])` to `__rt_ob_start_ex`.
///
/// Resolves the handler triple (invocation stub, env word, display name) from
/// the callback operand: `null` selects the default handler; a `Callable`
/// descriptor is retained and invoked through `__rt_ob_invoke_descriptor`; a
/// runtime string dispatches through the shared callable descriptor cases (a
/// miss raises PHP's invalid-callback warning and returns `false`); a boxed
/// `Mixed` value unboxes to one of those shapes at run time.
pub(crate) fn lower_ob_start(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "ob_start", 0, 3)?;
    // Stage 1: push the handler triple as two pairs: [name_ptr, name_len]
    // first, then [stub, env] on top. A stub of -1 marks "callback rejected"
    // (warnings already written); the call is skipped and false returned.
    match inst.operands.first().copied() {
        None => emit_push_default_handler_triple(ctx),
        Some(callback) => match ctx.value_php_type(callback)?.codegen_repr() {
            PhpType::Void => emit_push_default_handler_triple(ctx),
            PhpType::Callable => {
                ctx.load_value_to_result(callback)?;
                emit_push_descriptor_handler_triple(ctx);
            }
            PhpType::Str => {
                super::io::load_string_to_result(ctx, callback, "ob_start callback name")?;
                emit_push_string_handler_triple(ctx)?;
            }
            PhpType::Mixed | PhpType::Union(_) => {
                load_value_to_first_int_arg(ctx, callback)?;
                emit_push_mixed_handler_triple(ctx)?;
            }
            ty => {
                return Err(CodegenIrError::unsupported(format!(
                    "ob_start callback for PHP type {:?}",
                    ty
                )));
            }
        },
    }
    // Stage 2: stage the chunk size and flags (defaults 0 / STDFLAGS).
    match inst.operands.get(1).copied() {
        Some(chunk) => resolve_integer_arg_to_result(ctx, chunk, "ob_start chunk_size")?,
        None => abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0),
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match inst.operands.get(2).copied() {
        Some(flags) => resolve_integer_arg_to_result(ctx, flags, "ob_start flags")?,
        None => abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 112),
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    // Stage 3: assemble the __rt_ob_start_ex arguments and call (or fail).
    let fail_label = ctx.next_label("ob_start_rejected");
    let done_label = ctx.next_label("ob_start_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_pop_reg(ctx.emitter, "x2");
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
            abi::emit_pop_reg_pair(ctx.emitter, "x4", "x5");
            ctx.emitter.instruction("cmn x0, #1");                              // was the callback rejected (stub sentinel -1)?
            ctx.emitter.instruction(&format!("b.eq {}", fail_label));           // skip the buffer creation after a rejected callback
            abi::emit_call_label(ctx.emitter, "__rt_ob_start_ex");
            ctx.emitter.instruction(&format!("b {}", done_label));              // return the runtime success flag
            ctx.emitter.label(&fail_label);
            ctx.emitter.instruction("mov x0, #0");                              // a rejected callback makes ob_start() return false
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rcx");
            abi::emit_pop_reg(ctx.emitter, "rdx");
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
            abi::emit_pop_reg_pair(ctx.emitter, "r8", "r9");
            ctx.emitter.instruction("cmp rdi, -1");                             // was the callback rejected (stub sentinel -1)?
            ctx.emitter.instruction(&format!("je {}", fail_label));             // skip the buffer creation after a rejected callback
            abi::emit_call_label(ctx.emitter, "__rt_ob_start_ex");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // return the runtime success flag
            ctx.emitter.label(&fail_label);
            ctx.emitter.instruction("xor eax, eax");                            // a rejected callback makes ob_start() return false
            ctx.emitter.label(&done_label);
        }
    }
    store_if_result(ctx, inst)
}

/// Pushes the default-handler triple: the "default output handler" name pair,
/// then a zero stub/env pair.
fn emit_push_default_handler_triple(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x4", "_ob_handler_name");
            abi::emit_load_int_immediate(ctx.emitter, "x5", OB_DEFAULT_HANDLER_NAME.len() as i64);
            abi::emit_push_reg_pair(ctx.emitter, "x4", "x5");
            ctx.emitter.instruction("mov x4, #0");                              // no handler stub (default handler)
            ctx.emitter.instruction("mov x5, #0");                              // no handler env word
            abi::emit_push_reg_pair(ctx.emitter, "x4", "x5");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r8", "_ob_handler_name");
            abi::emit_load_int_immediate(ctx.emitter, "r9", OB_DEFAULT_HANDLER_NAME.len() as i64);
            abi::emit_push_reg_pair(ctx.emitter, "r8", "r9");
            ctx.emitter.instruction("xor r8d, r8d");                            // no handler stub (default handler)
            ctx.emitter.instruction("xor r9d, r9d");                            // no handler env word
            abi::emit_push_reg_pair(ctx.emitter, "r8", "r9");
        }
    }
}

/// Pushes the handler triple for a callable descriptor held in the integer
/// result register: name from the descriptor's php_name (or
/// "Closure::__invoke"), a retain, and the descriptor-invoker stub.
fn emit_push_descriptor_handler_triple(ctx: &mut FunctionContext<'_>) {
    let closure_name = ctx.next_label("ob_start_closure_name");
    let name_ready = ctx.next_label("ob_start_name_ready");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x4, [x0]");                            // load the descriptor kind
            ctx.emitter.instruction("cmp x4, #4");                              // closure/first-class/adapter kinds (1..3)?
            ctx.emitter.instruction(&format!("b.lo {}", closure_name));         // closure-shaped handlers report Closure::__invoke like PHP
            ctx.emitter.instruction("ldr x4, [x0, #16]");                       // load the descriptor's PHP name pointer
            ctx.emitter.instruction("ldr x5, [x0, #24]");                       // load the descriptor's PHP name length
            ctx.emitter.instruction(&format!("cbnz x4, {}", name_ready));       // a named callable keeps its PHP name
            ctx.emitter.label(&closure_name);
            abi::emit_symbol_address(ctx.emitter, "x4", "_ob_closure_invoke_name");
            abi::emit_load_int_immediate(ctx.emitter, "x5", OB_CLOSURE_INVOKE_NAME.len() as i64);
            ctx.emitter.label(&name_ready);
            abi::emit_push_reg_pair(ctx.emitter, "x4", "x5");
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
            abi::emit_symbol_address(ctx.emitter, "x4", "__rt_ob_invoke_descriptor");
            abi::emit_push_reg_pair(ctx.emitter, "x4", "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r8, QWORD PTR [rax]");                 // load the descriptor kind
            ctx.emitter.instruction("cmp r8, 4");                               // closure/first-class/adapter kinds (1..3)?
            ctx.emitter.instruction(&format!("jb {}", closure_name));           // closure-shaped handlers report Closure::__invoke like PHP
            ctx.emitter.instruction("mov r8, QWORD PTR [rax + 16]");            // load the descriptor's PHP name pointer
            ctx.emitter.instruction("mov r9, QWORD PTR [rax + 24]");            // load the descriptor's PHP name length
            ctx.emitter.instruction("test r8, r8");                             // does the descriptor carry a PHP name?
            ctx.emitter.instruction(&format!("jnz {}", name_ready));            // a named callable keeps its PHP name
            ctx.emitter.label(&closure_name);
            abi::emit_symbol_address(ctx.emitter, "r8", "_ob_closure_invoke_name");
            abi::emit_load_int_immediate(ctx.emitter, "r9", OB_CLOSURE_INVOKE_NAME.len() as i64);
            ctx.emitter.label(&name_ready);
            abi::emit_push_reg_pair(ctx.emitter, "r8", "r9");
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
            abi::emit_symbol_address(ctx.emitter, "r8", "__rt_ob_invoke_descriptor");
            abi::emit_push_reg_pair(ctx.emitter, "r8", "rax");
        }
    }
}

/// Pushes the handler triple for a runtime string callback name held in the
/// platform string result registers: the name itself becomes the display name,
/// and the shared callable descriptor cases resolve the target. A miss writes
/// PHP's invalid-callback warning plus the failed-create notice and pushes the
/// -1 rejection sentinel.
fn emit_push_string_handler_triple(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let (ptr_reg, len_reg) = (ptr_reg.to_string(), len_reg.to_string());
    abi::emit_push_reg_pair(ctx.emitter, &ptr_reg, &len_reg);
    let call_reg = abi::nested_call_reg(ctx.emitter);
    let cases = runtime_string_descriptor_cases(ctx, None)?;
    let matched_join = ctx.next_label("ob_start_cb_matched");
    let selector = callable_dispatch::RuntimeCallableSelector::StringNameStack {
        ptr_offset: 0,
        len_offset: 8,
        call_reg,
    };
    for case in &cases {
        let next_case = ctx.next_label("ob_start_cb_next");
        let matched_label = ctx.next_label("ob_start_cb_case");
        callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next_case,
            ctx.emitter,
            &matched_label,
            ctx.data,
        );
        abi::emit_jump(ctx.emitter, &matched_join);
        ctx.emitter.label(&next_case);
    }
    // -- miss: PHP's invalid-callback warning + failed-create notice --
    emit_static_funnel_write(
        ctx,
        "_ob_warn_bad_callback_prefix",
        OB_WARN_BAD_CALLBACK_PREFIX.len(),
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp]");                            // warning body = the rejected callback name pointer
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // warning body length = the rejected name length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // warning body = the rejected callback name pointer
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // warning body length = the rejected name length
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdout_write");
    emit_static_funnel_write(
        ctx,
        "_ob_warn_bad_callback_suffix",
        OB_WARN_BAD_CALLBACK_SUFFIX.len(),
    );
    emit_static_funnel_write(ctx, "_ob_ntc_create_fail", OB_NTC_CREATE_FAIL.len());
    emit_push_rejection_sentinel(ctx);
    let stage_done = ctx.next_label("ob_start_cb_staged");
    abi::emit_jump(ctx.emitter, &stage_done);
    // -- match: the descriptor sits in the nested-call register --
    ctx.emitter.label(&matched_join);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x0, {}", call_reg));          // move the matched descriptor into the retain register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rax, {}", call_reg));         // move the matched descriptor into the retain register
        }
    }
    callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x4", "__rt_ob_invoke_descriptor");
            abi::emit_push_reg_pair(ctx.emitter, "x4", "x0");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r8", "__rt_ob_invoke_descriptor");
            abi::emit_push_reg_pair(ctx.emitter, "r8", "rax");
        }
    }
    ctx.emitter.label(&stage_done);
    Ok(())
}

/// Pushes the handler triple for a boxed `Mixed` callback: unboxes the cell and
/// dispatches on its runtime tag (callable descriptor, string name, null, or —
/// for anything else — PHP's generic invalid-callback warning + rejection).
fn emit_push_mixed_handler_triple(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let desc_case = ctx.next_label("ob_start_mixed_desc");
    let string_case = ctx.next_label("ob_start_mixed_string");
    let null_case = ctx.next_label("ob_start_mixed_null");
    let bad_case = ctx.next_label("ob_start_mixed_bad");
    let staged = ctx.next_label("ob_start_mixed_staged");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #10");                             // is the boxed callback a callable descriptor?
            ctx.emitter.instruction(&format!("b.eq {}", desc_case));            // dispatch the descriptor shape
            ctx.emitter.instruction("cmp x0, #1");                              // is the boxed callback a string name?
            ctx.emitter.instruction(&format!("b.eq {}", string_case));          // dispatch the string shape
            ctx.emitter.instruction("cmp x0, #8");                              // is the boxed callback null?
            ctx.emitter.instruction(&format!("b.eq {}", null_case));            // null selects the default handler
            ctx.emitter.instruction(&format!("b {}", bad_case));                // anything else is not a supported handler
            ctx.emitter.label(&desc_case);
            ctx.emitter.instruction("mov x0, x1");                              // descriptor pointer = the unboxed low payload word
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 10");                             // is the boxed callback a callable descriptor?
            ctx.emitter.instruction(&format!("je {}", desc_case));              // dispatch the descriptor shape
            ctx.emitter.instruction("cmp rax, 1");                              // is the boxed callback a string name?
            ctx.emitter.instruction(&format!("je {}", string_case));            // dispatch the string shape
            ctx.emitter.instruction("cmp rax, 8");                              // is the boxed callback null?
            ctx.emitter.instruction(&format!("je {}", null_case));              // null selects the default handler
            ctx.emitter.instruction(&format!("jmp {}", bad_case));              // anything else is not a supported handler
            ctx.emitter.label(&desc_case);
            ctx.emitter.instruction("mov rax, rdi");                            // descriptor pointer = the unboxed low payload word
        }
    }
    emit_push_descriptor_handler_triple(ctx);
    abi::emit_jump(ctx.emitter, &staged);
    ctx.emitter.label(&string_case);
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rax, rdi");                                // string pointer = the unboxed low payload word
    }
    emit_push_string_handler_triple(ctx)?;
    abi::emit_jump(ctx.emitter, &staged);
    ctx.emitter.label(&null_case);
    emit_push_default_handler_triple(ctx);
    abi::emit_jump(ctx.emitter, &staged);
    ctx.emitter.label(&bad_case);
    emit_static_funnel_write(
        ctx,
        "_ob_warn_bad_callback_generic",
        OB_WARN_BAD_CALLBACK_GENERIC.len(),
    );
    emit_static_funnel_write(ctx, "_ob_ntc_create_fail", OB_NTC_CREATE_FAIL.len());
    emit_push_default_handler_name_pair(ctx);
    emit_push_rejection_sentinel_pair_only(ctx);
    ctx.emitter.label(&staged);
    Ok(())
}

/// Writes one static diagnostic line through the capture-aware stdout funnel.
fn emit_static_funnel_write(ctx: &mut FunctionContext<'_>, symbol: &str, len: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x0", symbol);
            abi::emit_load_int_immediate(ctx.emitter, "x1", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", symbol);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdout_write");
}

/// Pushes the default handler name pair (used before a rejection sentinel).
fn emit_push_default_handler_name_pair(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x4", "_ob_handler_name");
            abi::emit_load_int_immediate(ctx.emitter, "x5", OB_DEFAULT_HANDLER_NAME.len() as i64);
            abi::emit_push_reg_pair(ctx.emitter, "x4", "x5");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r8", "_ob_handler_name");
            abi::emit_load_int_immediate(ctx.emitter, "r9", OB_DEFAULT_HANDLER_NAME.len() as i64);
            abi::emit_push_reg_pair(ctx.emitter, "r8", "r9");
        }
    }
}

/// Pushes the rejection sentinel stub/env pair on top of an already-pushed
/// name pair (string-miss path: the rejected name stays as the name pair).
fn emit_push_rejection_sentinel(ctx: &mut FunctionContext<'_>) {
    emit_push_rejection_sentinel_pair_only(ctx);
}

/// Pushes only the -1 stub / 0 env rejection sentinel pair.
fn emit_push_rejection_sentinel_pair_only(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x4, #-1");                             // stub sentinel -1 marks a rejected callback
            ctx.emitter.instruction("mov x5, #0");                              // rejected callbacks carry no env word
            abi::emit_push_reg_pair(ctx.emitter, "x4", "x5");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r8, -1");                              // stub sentinel -1 marks a rejected callback
            ctx.emitter.instruction("xor r9d, r9d");                            // rejected callbacks carry no env word
            abi::emit_push_reg_pair(ctx.emitter, "r8", "r9");
        }
    }
}

/// Lowers `ob_get_contents()` and boxes the runtime string-or-false result.
pub(crate) fn lower_ob_get_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "ob_get_contents", 0, 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_ob_contents");
    super::io::box_owned_string_or_false_result(ctx, "ob_contents");
    store_if_result(ctx, inst)
}

/// Lowers `ob_get_clean()` through the composite runtime helper: REMOVABLE
/// gating, handler CLEAN|FINAL phase, pop, and the raw contents (or `false`).
pub(crate) fn lower_ob_get_clean(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "ob_get_clean", 0, 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_ob_get_clean_pop");
    super::io::box_owned_string_or_false_result(ctx, "ob_get_clean");
    store_if_result(ctx, inst)
}

/// Lowers `ob_get_flush()` through the composite runtime helper: REMOVABLE
/// gating, handler FINAL phase, parent-sink flush, pop, and the raw contents.
pub(crate) fn lower_ob_get_flush(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "ob_get_flush", 0, 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_ob_get_flush_pop");
    super::io::box_owned_string_or_false_result(ctx, "ob_get_flush");
    store_if_result(ctx, inst)
}

/// Lowers `ob_get_length()` and boxes the length-or-false result (the runtime
/// returns -1 when no buffer is active).
pub(crate) fn lower_ob_get_length(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "ob_get_length", 0, 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_ob_length");
    box_int_or_false_result(ctx, "ob_length");
    store_if_result(ctx, inst)
}

/// Lowers `ob_get_level()` to the plain integer nesting-depth query.
pub(crate) fn lower_ob_get_level(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "ob_get_level", 0, 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_ob_level");
    store_if_result(ctx, inst)
}

/// Lowers `ob_clean()` to the truncate-top-buffer helper (bool result).
pub(crate) fn lower_ob_clean(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ob_bool_query(ctx, inst, "ob_clean", "__rt_ob_clean")
}

/// Lowers `ob_end_clean()` to the discard-and-pop helper (bool result).
pub(crate) fn lower_ob_end_clean(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_ob_bool_query(ctx, inst, "ob_end_clean", "__rt_ob_end_clean")
}

/// Lowers `ob_end_flush()` to the flush-and-pop helper (bool result).
pub(crate) fn lower_ob_end_flush(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_ob_bool_query(ctx, inst, "ob_end_flush", "__rt_ob_end_flush")
}

/// Lowers `ob_flush()` to the flush-keep-buffer helper (bool result).
pub(crate) fn lower_ob_flush(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ob_bool_query(ctx, inst, "ob_flush", "__rt_ob_flush")
}

/// Shared lowering for the zero-argument bool-returning ob_* helpers.
fn lower_ob_bool_query(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_symbol: &str,
) -> Result<()> {
    ensure_arg_count_between(inst, name, 0, 0)?;
    abi::emit_call_label(ctx.emitter, runtime_symbol);
    store_if_result(ctx, inst)
}

/// Lowers `ob_implicit_flush([$enable])`: store the flag (semantically inert in
/// elephc — terminal writes are unbuffered syscalls) and return `true` like PHP 8.
pub(crate) fn lower_ob_implicit_flush(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "ob_implicit_flush", 0, 1)?;
    match inst.operands.first().copied() {
        Some(enable) => {
            resolve_integer_arg_to_result(ctx, enable, "ob_implicit_flush enable flag")?
        }
        None => abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1),
    }
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_ob_implicit_flush",
        0,
    );
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    store_if_result(ctx, inst)
}

/// Lowers `ob_get_status([$full_status])` through the status-hash runtime helper
/// and boxes the hash pointer as a Mixed associative array.
pub(crate) fn lower_ob_get_status(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "ob_get_status", 0, 1)?;
    match inst.operands.first().copied() {
        Some(flag) => resolve_integer_arg_to_result(ctx, flag, "ob_get_status full_status flag")?,
        None => abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0),
    }
    abi::emit_call_label(ctx.emitter, "__rt_ob_get_status");
    emit_box_hash_pointer_as_assoc_mixed(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `ob_list_handlers()` to the handler-name string-array helper.
pub(crate) fn lower_ob_list_handlers(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "ob_list_handlers", 0, 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_ob_list_handlers");
    store_if_result(ctx, inst)
}

/// Boxes a raw integer-or-sentinel result into PHP `int|false` Mixed form, where
/// -1 in the integer result register marks the failure branch.
fn box_int_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmn x0, #1");                              // compare the raw result against the -1 failure sentinel
            ctx.emitter.instruction(&format!("b.eq {}", false_label));          // box PHP false when no buffer was active
            ctx.emitter.instruction("mov x1, x0");                              // pass the length as the Mixed integer payload
            ctx.emitter.instruction("mov x2, #0");                              // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the integer result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, -1");                             // compare the raw result against the -1 failure sentinel
            ctx.emitter.instruction(&format!("je {}", false_label));            // box PHP false when no buffer was active
            ctx.emitter.instruction("mov rdi, rax");                            // pass the length as the Mixed integer payload
            ctx.emitter.instruction("xor esi, esi");                            // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the integer result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes the raw associative-array hash pointer in the integer result register
/// into a `Mixed` cell (runtime tag 5), mirroring `getdate`/`localtime`/`stat`.
fn emit_box_hash_pointer_as_assoc_mixed(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // Mixed payload low word = hash pointer
            ctx.emitter.instruction("mov x2, #0");                              // associative-array payloads do not use the high word
            ctx.emitter.instruction("mov x0, #5");                              // runtime tag 5 = associative array
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // Mixed payload low word = hash pointer
            ctx.emitter.instruction("xor esi, esi");                            // associative-array payloads do not use the high word
            ctx.emitter.instruction("mov rax, 5");                              // runtime tag 5 = associative array
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
}

/// Resolves one boolean/integer argument into the canonical integer result
/// register, unboxing a boxed `Mixed`/`Union` value through `__rt_mixed_cast_int`.
fn resolve_integer_arg_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    context: &str,
) -> Result<()> {
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        ty => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for PHP type {:?}",
                context, ty
            )));
        }
    }
    Ok(())
}

/// Verifies that the builtin call has between the expected lowered operand counts.
fn ensure_arg_count_between(inst: &Instruction, name: &str, min: usize, max: usize) -> Result<()> {
    if (min..=max).contains(&inst.operands.len()) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} to {} args, got {}",
        name,
        min,
        max,
        inst.operands.len()
    )))
}
