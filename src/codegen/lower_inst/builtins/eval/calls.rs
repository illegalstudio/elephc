//! Purpose:
//! Lowers eval entry points and literal EIR AOT fast paths.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - The bridge fallback preserves PHP source order and result ownership.

use super::*;
use crate::codegen::lower_inst::callable_guards;
use crate::codegen_support::try_handlers::EXCEPTION_GUARD_SLOT_SIZE;

const EVAL_RESULT_GUARD_OFFSET: usize = EVAL_STACK_BYTES;
/// Parks the literal fragment's result across the reload and the native-binding clear, which both
/// reuse `EVAL_TEMP_CELL_OFFSET` as scratch (a reference entry's value view travels through it).
const EVAL_RESULT_SAVE_OFFSET: usize = EVAL_STACK_BYTES + EXCEPTION_GUARD_SLOT_SIZE;
const EVAL_GUARDED_STACK_BYTES: usize = EVAL_RESULT_SAVE_OFFSET + 16;

/// Lowers `eval($code)` through internal EIR AOT or the bridge ABI and leaves its result in registers.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "eval", 1)?;
    if let Some(fragment) = eval_literal_fragment(ctx, inst)? {
        let published = publish_eval_fragment_site(ctx, inst);
        let lowered = lower_eval_literal_eir_function(ctx, inst, &fragment)?;
        if published {
            restore_eval_fragment_site(ctx);
        }
        if lowered {
            return Ok(());
        }
    }
    emit_eval_literal_aot_marker(ctx, inst)?;
    let code = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(code)?.codegen_repr();
    if ty != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "eval() argument lowering for PHP type {:?}",
            ty
        )));
    }

    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_BRIDGE_STACK_BYTES);
    save_eval_code_string(ctx);
    ensure_eval_context_handle(ctx)?;
    mark_eval_strict_php(ctx, inst);
    mark_eval_php_version(ctx);
    mark_eval_web_sapi(ctx);
    set_eval_call_site(ctx, inst);
    ensure_eval_scope(ctx)?;
    ensure_eval_global_scope(ctx)?;
    let (sync_locals, global_bound_locals) = split_eval_global_bound_locals(ctx, eval_sync_locals(ctx));
    let sync_globals = eval_sync_globals(ctx);
    prepare_dynamic_eval_scope_access_mask(ctx, &sync_locals, &sync_globals)?;
    let catalog = prepare_eval_scope_flush(ctx)?;
    let global_aliases = eval_global_aliases(ctx);
    flush_dynamic_eval_scope_locals(ctx, &sync_locals)?;
    flush_dynamic_eval_global_scope(ctx, &sync_globals)?;
    clear_eval_global_scope_dirty(ctx, &sync_globals);
    mark_eval_scope_global_aliases(ctx, &global_aliases);
    flush_eval_global_bound_locals(ctx, &global_bound_locals)?;
    set_eval_context_global_scope(ctx);
    prepare_eval_native_catalog(ctx, catalog)?;
    let pushed_class_scope = push_eval_context_class_scope(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    load_eval_scope_to_arg(ctx, 1);
    move_saved_eval_code_to_eval_args(ctx);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_execute_with_deferred_catalog");
    abi::emit_call_label(ctx.emitter, &symbol);
    pop_eval_context_class_scope(ctx, pushed_class_scope);
    prepare_eval_scope_reload(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    protect_eval_reload_owners(ctx);
    ctx.emitter.comment("reload eval scope before propagating its pending exception");
    reload_dynamic_eval_scope_locals(ctx, &sync_locals, Some(EVAL_RELOAD_THROWABLE_OFFSET))?;
    reload_eval_global_bound_locals(ctx, &global_bound_locals, Some(EVAL_RELOAD_THROWABLE_OFFSET))?;
    reload_dynamic_eval_global_scope(ctx, &sync_globals, Some(EVAL_RELOAD_THROWABLE_OFFSET))?;
    finish_eval_scope_reload(ctx);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_BRIDGE_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers a runtime include/require through Magician while sharing and reloading caller scope.
pub(in crate::codegen::lower_inst::builtins) fn lower_dynamic_include(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    once: bool,
    required: bool,
    strict_php: bool,
    value_discarded: bool,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "dynamic include", 1)?;
    let path = expect_operand(inst, 0)?;
    if ctx.module.closed_world {
        return lower_closed_world_include(ctx, inst, path, once, required, value_discarded);
    }
    let unit_done = emit_include_unit_dispatch(ctx, inst, path, once)?;
    let fast_done = if once || value_discarded {
        emit_dynamic_include_fast_path(ctx, inst, path, once, value_discarded)?
    } else {
        None
    };
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    // Scope synchronization owns the generic temp-cell slot, so preserve the borrowed
    // path in the otherwise-unused saved-code pointer slot until the include call.
    // A statically typed path is boxed for the bridge, which only borrows it: the cell stays
    // this frame's to release once the include has run (or has thrown).
    let boxed: EvalBoxedOperands = store_eval_mixed_operand_at(ctx, path, EVAL_CODE_PTR_OFFSET)?
        .into_iter()
        .collect();
    ensure_eval_context(ctx)?;
    mark_eval_strict_php_value(ctx, strict_php);
    mark_eval_php_version(ctx);
    mark_eval_web_sapi(ctx);
    set_eval_call_site(ctx, inst);
    ensure_eval_scope(ctx)?;
    ensure_eval_global_scope(ctx)?;
    let (sync_locals, global_bound_locals) = split_eval_global_bound_locals(ctx, eval_sync_locals(ctx));
    let sync_globals = eval_sync_globals(ctx);
    let global_aliases = eval_global_aliases(ctx);
    flush_eval_scope_locals(ctx, &sync_locals)?;
    flush_eval_global_scope(ctx, &sync_globals)?;
    clear_eval_global_scope_dirty(ctx, &sync_globals);
    mark_eval_scope_global_aliases(ctx, &global_aliases);
    flush_eval_global_bound_locals(ctx, &global_bound_locals)?;
    set_eval_context_global_scope(ctx);
    let pushed_class_scope = push_eval_context_class_scope(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    load_eval_scope_to_arg(ctx, 1);
    let path_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, path_arg, EVAL_CODE_PTR_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        i64::from(required),
    );
    // The x86_64 C ABI hands over only six integer arguments in registers, and this call
    // already uses all six. `value_discarded` therefore travels as a BIT of the `once` word
    // rather than as a seventh argument that would have to be passed on the stack on one target
    // and in a register on the other. `INCLUDE_FLAG_*` in
    // `crates/elephc-magician/src/ffi/include.rs` decodes it; the two must agree.
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        i64::from(once) | (i64::from(value_discarded) << 1),
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_include");
    abi::emit_call_label(ctx.emitter, &symbol);
    pop_eval_context_class_scope(ctx, pushed_class_scope);
    emit_eval_status_check_releasing(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    emit_release_eval_boxed_operands(ctx, &boxed);
    reload_eval_scope_locals(ctx, &sync_locals, None)?;
    reload_eval_global_bound_locals(ctx, &global_bound_locals, None)?;
    reload_eval_global_scope(ctx, &sync_globals, None)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)?;
    if let Some(done) = fast_done {
        ctx.emitter.label(&done);
    }
    if let Some(done) = unit_done {
        ctx.emitter.label(&done);
    }
    Ok(())
}

/// Lowers a runtime include of a `--closed-world` build: one call into `__rt_closed_include`,
/// which answers from the compiled-source table (see `source_units::closed_include`).
fn lower_closed_world_include(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: ValueId,
    once: bool,
    required: bool,
    value_discarded: bool,
) -> Result<()> {
    use crate::codegen::source_units::{CLOSED_INCLUDE, FLAG_DISCARDED, FLAG_ONCE, FLAG_REQUIRED};
    let boxed = match ctx.value_php_type(path)?.codegen_repr() {
        PhpType::Str => false,
        PhpType::Mixed | PhpType::Union(_) => true,
        // Any other scalar would be cast to a string by php; no compiled file is named by one.
        _ => false,
    };
    let flags = (if once { FLAG_ONCE } else { 0 })
        | (if required { FLAG_REQUIRED } else { 0 })
        | (if value_discarded { FLAG_DISCARDED } else { 0 });
    ctx.load_value_to_result(path)?;
    let string_ok = ctx.next_label("closed_include_string");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if boxed {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                ctx.emitter.instruction("cmp x0, #1");                          // runtime tag 1 = string
                ctx.emitter.instruction(&format!("b.eq {string_ok}"));
                ctx.emitter.instruction("mov x1, #0");                          // not a string: no path
                ctx.emitter.instruction("mov x2, #0");
                ctx.emitter.label(&string_ok);
            }
            ctx.emitter.instruction(&format!("mov x3, #{flags}"));              // once / require / discarded
        }
        Arch::X86_64 => {
            if boxed {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                ctx.emitter.instruction("cmp rax, 1");                          // runtime tag 1 = string
                ctx.emitter.instruction("mov rax, rdi");                        // string bytes
                // The high payload word is already in rdx, as required by the include helper.
                ctx.emitter.instruction(&format!("je {string_ok}"));
                ctx.emitter.instruction("xor eax, eax");                        // not a string: no path
                ctx.emitter.instruction("xor edx, edx");
                ctx.emitter.label(&string_ok);
            }
            ctx.emitter.instruction(&format!("mov ecx, {flags}"));              // once / require / discarded
        }
    }
    let (path_ptr, path_len) = abi::string_result_regs(ctx.emitter);
    if required {
        // Keep the path bytes across the routine: a failed require names it in its `Error`.
        abi::emit_push_reg_pair(ctx.emitter, path_ptr, path_len);
    }
    abi::emit_call_label(ctx.emitter, CLOSED_INCLUDE);
    if required {
        emit_closed_world_require_failure(ctx);
    }
    store_if_result(ctx, inst)
}

/// Throws php's catchable `Error` when `__rt_closed_include` answered a `require` with no value,
/// then drops the path pair the include site parked on the temporary stack.
///
/// The routine already printed the stream warning; php then raises
/// `Failed opening required '<path>' (include_path='...')` as an `Error`, so a `catch`, a
/// `finally` and the uncaught report all see it. elephc has no `include_path` (relative paths
/// resolve against the working directory), so the message stops after the path, as the
/// closed-world tests' include_path normalization expects.
fn emit_closed_world_require_failure(ctx: &mut FunctionContext<'_>) {
    let opened = ctx.next_label("closed_include_opened");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {opened}"));             // a boxed value means the require succeeded
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the require answer a value?
            ctx.emitter.instruction(&format!("jnz {opened}"));                  // a boxed value means the require succeeded
        }
    }
    let (path_ptr, path_len) = abi::string_result_regs(ctx.emitter);
    abi::emit_pop_reg_pair(ctx.emitter, path_ptr, path_len);
    crate::codegen::lower_inst::runtime_class_messages::emit_concat_static_prefix(
        ctx,
        "Failed opening required '",
    );
    crate::codegen::lower_inst::runtime_class_messages::emit_concat_static_suffix(ctx, "'");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    crate::codegen::lower_inst::exceptions::emit_error_from_string_result(ctx);
    ctx.emitter.label(&opened);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Runs a compiled include unit when the path names one: absolute, canonicalized through the
/// realpath cache, and found in the unit table. The `_once` forms answer `true` for a file already
/// included, exactly as the interpreter does from the same guard cell; otherwise the cell is
/// marked BEFORE the unit runs, as php marks a file before executing it, and the unit's return
/// value is the include's value. Anything else falls through to the paths below.
///
/// Returns the label the unit path jumps to, or `None` when the module has no unit or the path
/// operand cannot carry a string.
fn emit_include_unit_dispatch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: ValueId,
    once: bool,
) -> Result<Option<String>> {
    if ctx.module.include_units.is_empty() {
        return Ok(None);
    }
    let path_ty = ctx.value_php_type(path)?.codegen_repr();
    let boxed = match path_ty {
        PhpType::Str => false,
        PhpType::Mixed | PhpType::Union(_) => true,
        _ => return Ok(None),
    };
    let skip = ctx.next_label("include_unit_skip");
    let run = ctx.next_label("include_unit_run");
    let done = ctx.next_label("include_unit_done");
    let lookup = crate::codegen::source_units::INCLUDE_UNIT_LOOKUP;
    let candidate = crate::codegen::source_units::INCLUDE_UNIT_CANDIDATE;
    let function_offset = crate::codegen::source_units::INCLUDE_UNIT_FUNCTION_OFFSET;
    let guard_offset = crate::codegen::source_units::INCLUDE_UNIT_GUARD_OFFSET;
    ctx.load_value_to_result(path)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if boxed {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                ctx.emitter.instruction("cmp x0, #1");                          // runtime tag 1 = string
                ctx.emitter.instruction(&format!("b.ne {skip}"));
            }
            ctx.emitter.instruction(&format!("cbz x2, {skip}"));                // an empty path names no unit
            ctx.emitter.instruction("ldrb w9, [x1]");                           // first path byte
            ctx.emitter.instruction("cmp w9, #47");                             // absolute ('/') paths only
            ctx.emitter.instruction(&format!("b.ne {skip}"));
            abi::emit_call_label(ctx.emitter, candidate);                       // x0 = 1 when a unit has this file name
            ctx.emitter.instruction(&format!("cbz x0, {skip}"));                // spare every other include the realpath
            abi::emit_call_label(ctx.emitter, "__rt_realpath");                 // x1/x2 = owned canonical path, or 0/0
            ctx.emitter.instruction(&format!("cbz x1, {skip}"));                // an unresolvable path names no unit
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // keep the canonical path to free it
            abi::emit_call_label(ctx.emitter, lookup);                          // x0 = unit record, or 0
            ctx.emitter.instruction("str x0, [sp, #8]");                        // park the record where the length was
            ctx.emitter.instruction("ldr x0, [sp]");                            // the canonical path buffer
            abi::emit_call_label(ctx.emitter, "__rt_heap_free");                // it was only needed for the lookup
            ctx.emitter.instruction("ldp x0, x9, [sp], #16");                   // x9 = unit record
            ctx.emitter.instruction(&format!("cbz x9, {skip}"));                // no unit for this file
            ctx.emitter.instruction(&format!("ldr x10, [x9, #{guard_offset}]")); // the file's guard cell
            ctx.emitter.instruction("ldr x11, [x10]");                          // 0 until something includes the file
            if once {
                ctx.emitter.instruction(&format!("cbz x11, {run}"));            // first inclusion: run the unit
                ctx.emitter.instruction("mov x0, #1");                          // already included: the include is `true`
                emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
                store_if_result(ctx, inst)?;
                abi::emit_jump(ctx.emitter, &done);
                ctx.emitter.label(&run);
            } else {
                ctx.emitter.instruction(&format!("cbnz x11, {run}"));           // keep a compiler mark as it is
            }
            if !once {
                ctx.emitter.instruction("mov x11, #1");                         // included by this request
                ctx.emitter.instruction("str x11, [x10]");
                ctx.emitter.label(&run);
            } else {
                ctx.emitter.instruction("mov x11, #1");                         // included by this request
                ctx.emitter.instruction("str x11, [x10]");
            }
            ctx.emitter.instruction(&format!("ldr x9, [x9, #{function_offset}]")); // the unit function
            ctx.emitter.instruction("mov x0, #0");                              // null func_get_args collector: x0 still held the freed path
            ctx.emitter.instruction("blr x9");                                  // x0 = the include's value
        }
        Arch::X86_64 => {
            if boxed {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                ctx.emitter.instruction("cmp rax, 1");                          // runtime tag 1 = string
                ctx.emitter.instruction(&format!("jne {skip}"));
                ctx.emitter.instruction("mov rax, rdi");                        // string bytes
                // mixed_unbox already returns the string length in rdx; rsi is caller scratch.
            }
            ctx.emitter.instruction("test rdx, rdx");                           // an empty path names no unit
            ctx.emitter.instruction(&format!("jz {skip}"));
            ctx.emitter.instruction("cmp BYTE PTR [rax], 47");                  // absolute ('/') paths only
            ctx.emitter.instruction(&format!("jne {skip}"));
            abi::emit_call_label(ctx.emitter, candidate);                       // r8 = 1 when a unit has this file name
            ctx.emitter.instruction("test r8, r8");
            ctx.emitter.instruction(&format!("jz {skip}"));                     // spare every other include the realpath
            abi::emit_call_label(ctx.emitter, "__rt_realpath");                 // rax/rdx = owned canonical path, or 0/0
            ctx.emitter.instruction("test rax, rax");                           // an unresolvable path names no unit
            ctx.emitter.instruction(&format!("jz {skip}"));
            ctx.emitter.instruction("push rax");                                // keep the canonical path to free it
            ctx.emitter.instruction("push rdx");
            abi::emit_call_label(ctx.emitter, lookup);                          // rax = unit record, or 0
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // park the record where the length was
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // the canonical path buffer
            abi::emit_call_label(ctx.emitter, "__rt_heap_free");                // it was only needed for the lookup
            ctx.emitter.instruction("pop r9");                                  // r9 = unit record
            ctx.emitter.instruction("add rsp, 8");                              // drop the freed pointer's slot
            ctx.emitter.instruction("test r9, r9");                             // no unit for this file?
            ctx.emitter.instruction(&format!("jz {skip}"));
            ctx.emitter.instruction(&format!("mov r10, QWORD PTR [r9 + {guard_offset}]")); // the file's guard cell
            ctx.emitter.instruction("cmp QWORD PTR [r10], 0");                  // 0 until something includes the file
            if once {
                ctx.emitter.instruction(&format!("je {run}"));                  // first inclusion: run the unit
                ctx.emitter.instruction("mov eax, 1");                          // already included: the include is `true`
                emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
                store_if_result(ctx, inst)?;
                abi::emit_jump(ctx.emitter, &done);
                ctx.emitter.label(&run);
                ctx.emitter.instruction("mov QWORD PTR [r10], 1");              // included by this request
            } else {
                ctx.emitter.instruction(&format!("jne {run}"));                 // keep a compiler mark as it is
                ctx.emitter.instruction("mov QWORD PTR [r10], 1");              // included by this request
                ctx.emitter.label(&run);
            }
            ctx.emitter.instruction(&format!("mov r9, QWORD PTR [r9 + {function_offset}]")); // the unit function
            ctx.emitter.instruction("xor edi, edi");                            // null func_get_args collector: rdi may hold a freed pointer
            ctx.emitter.instruction("call r9");                                 // rax = the include's value
        }
    }
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&skip);
    Ok(Some(done))
}

/// Asks `__elephc_eval_include_fast` first when the include may need nothing run: an `_once`
/// form, or a statement include of a file the compiled program already included. On an answer
/// the include evaluates to `true` and the bridge call, with its context and its scope flush and
/// reload, is jumped over; otherwise execution falls through to it.
///
/// Returns the label the answered path jumps to, or `None` when the path operand's type cannot
/// carry a string (the bridge then reports whatever php reports).
fn emit_dynamic_include_fast_path(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: ValueId,
    once: bool,
    value_discarded: bool,
) -> Result<Option<String>> {
    let path_ty = ctx.value_php_type(path)?.codegen_repr();
    let boxed = match path_ty {
        PhpType::Str => false,
        PhpType::Mixed | PhpType::Union(_) => true,
        _ => return Ok(None),
    };
    let slow = ctx.next_label("dynamic_include_slow");
    let done = ctx.next_label("dynamic_include_done");
    let flags = i64::from(once) | (i64::from(value_discarded) << 1);
    ctx.load_value_to_result(path)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if boxed {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                ctx.emitter.instruction("cmp x0, #1");                          // runtime tag 1 = string; anything else takes the bridge
                ctx.emitter.instruction(&format!("b.ne {}", slow));
            }
            ctx.emitter.instruction("mov x0, x1");                              // path bytes
            ctx.emitter.instruction("mov x1, x2");                              // path length
            ctx.emitter.instruction(&format!("mov x2, #{}", flags));            // the include's once / value-discarded flags
        }
        Arch::X86_64 => {
            if boxed {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                ctx.emitter.instruction("cmp rax, 1");                          // runtime tag 1 = string; anything else takes the bridge
                ctx.emitter.instruction(&format!("jne {}", slow));
                ctx.emitter.instruction("mov rsi, rdx");                        // move the unboxed length to the bridge's second C argument
            } else {
                ctx.emitter.instruction("mov rdi, rax");                        // path bytes
                ctx.emitter.instruction("mov rsi, rdx");                        // path length
            }
            ctx.emitter.instruction(&format!("mov edx, {}", flags));            // the include's once / value-discarded flags
        }
    }
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_include_fast");
    abi::emit_call_label(ctx.emitter, &symbol);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz w0, {}", slow));              // not answered: take the bridge
            ctx.emitter.instruction("mov x0, #1");                              // the include evaluates to true
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test eax, eax");                           // was the include answered?
            ctx.emitter.instruction(&format!("jz {}", slow));                   // not answered: take the bridge
            ctx.emitter.instruction("mov eax, 1");                              // the include evaluates to true
        }
    }
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&slow);
    Ok(Some(done))
}

/// Publishes this `eval()` site as `FILE(LINE) : eval()'d code` for the diagnostics of the
/// fragment it may call, saving the previous site on the temporary stack. Returns whether it did.
///
/// A literal fragment compiled ahead of time is ONE function shared by every site with the same
/// text, so its `_php_site` records point at the `_rt_eval_site_file` descriptor rather than at
/// a file of their own (see `prepare_call_site` and `emit_publish_diag_location`); without it a
/// warning raised inside `eval()` named the host file, where php names the eval'd code. Only
/// scratch registers are used, so nothing the fragment call loads afterwards is disturbed. A
/// fragment calling `eval()` keeps its own site.
fn publish_eval_fragment_site(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> bool {
    if crate::eval_aot::is_eir_fragment_function_name(&ctx.function.name) {
        return false;
    }
    // The file the `eval()` itself sits in: an included file's spans carry their own identity.
    let Some(host) = ctx
        .span_source_file(inst.span)
        .or_else(|| super::eval_source_path_for_function(ctx.module, ctx.function))
        .map(str::to_string)
    else {
        return false;
    };
    let line = inst.span.map(|span| span.source_line()).unwrap_or(0);
    let site = format!("{host}({line}) : eval()'d code");
    let (site_label, site_len) = ctx.data.add_string(site.as_bytes());
    let (ptr_reg, len_reg) = eval_site_scratch_regs(ctx);
    abi::emit_load_symbol_to_reg(ctx.emitter, ptr_reg, "_rt_eval_site_file", 0);
    abi::emit_load_symbol_to_reg(ctx.emitter, len_reg, "_rt_eval_site_file", 8);
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &site_label);
    abi::emit_store_reg_to_symbol(ctx.emitter, ptr_reg, "_rt_eval_site_file", 0);
    abi::emit_load_int_immediate(ctx.emitter, ptr_reg, site_len as i64);
    abi::emit_store_reg_to_symbol(ctx.emitter, ptr_reg, "_rt_eval_site_file", 8);
    true
}

/// Puts back the `eval()` site [`publish_eval_fragment_site`] saved, so a fragment that called a
/// function running another `eval()` names its own site again.
fn restore_eval_fragment_site(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = eval_site_scratch_regs(ctx);
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    abi::emit_store_reg_to_symbol(ctx.emitter, ptr_reg, "_rt_eval_site_file", 0);
    // A store at a non-zero offset materializes the address in x86_64's symbol scratch (r11),
    // which is `len_reg` there, so the length goes through `ptr_reg`.
    abi::emit_reg_move(ctx.emitter, ptr_reg, len_reg);
    abi::emit_store_reg_to_symbol(ctx.emitter, ptr_reg, "_rt_eval_site_file", 8);
}

/// Scratch registers that hold the eval site words: never argument or result registers, and
/// never the AArch64 symbol-address scratch (x9). Only `ptr_reg` may be stored at a non-zero
/// symbol offset, x86_64's `len_reg` being its symbol scratch.
fn eval_site_scratch_regs(ctx: &FunctionContext<'_>) -> (&'static str, &'static str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ("x10", "x11"),
        Arch::X86_64 => ("r10", "r11"),
    }
}

/// Calls a pre-lowered internal EIR function for no-scope literal eval fragments.
pub(super) fn lower_eval_literal_eir_function(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    fragment: &str,
) -> Result<bool> {
    let strict_php = super::super::instruction_strict_php_profile(inst);
    let function_name = crate::eval_aot::eir_function_name(fragment, strict_php);
    if let Some(callee) = ctx.callable_function_by_name(&function_name) {
        if callee.params.is_empty() && callee.return_php_type.codegen_repr() == PhpType::Mixed {
            ctx.emitter
                .comment("eval literal AOT compiled EIR function");
            let caller_stack_pad_bytes = abi::outgoing_call_stack_pad_bytes(ctx.emitter.target, 0);
            abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
            abi::emit_call_label(ctx.emitter, &function_symbol(&function_name));
            abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
            store_if_result(ctx, inst)?;
            return Ok(true);
        }
    }
    lower_eval_literal_scope_eir_function(ctx, inst, fragment)
}

/// Calls a pre-lowered internal EIR function that uses direct params or eval scope.
pub(super) fn lower_eval_literal_scope_eir_function(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    fragment: &str,
) -> Result<bool> {
    let strict_php = super::super::instruction_strict_php_profile(inst);
    let function_name = crate::eval_aot::eir_scope_function_name(fragment, strict_php);
    let Some(callee) = ctx.callable_function_by_name(&function_name) else {
        return Ok(false);
    };
    let param_types = callee
        .params
        .iter()
        .map(|param| param.php_type.codegen_repr())
        .collect::<Vec<_>>();
    let return_type = callee.return_php_type.codegen_repr();
    let source_path = eval_source_path_for_function(ctx.module, ctx.function);
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        fragment,
        source_path,
        strict_php,
        |name, args| eval_literal_static_function_supported_by_codegen(ctx, name, args),
        |receiver, method, args| {
            eval_literal_static_method_supported_by_codegen(ctx, receiver, method, args)
        },
    );
    if plan.uses_scope_read_params() {
        return lower_eval_literal_scope_read_param_eir_function(
            ctx,
            inst,
            &function_name,
            &param_types,
            &return_type,
            plan.reads(),
            plan.quiet_reads(),
            plan.array_read_constraints(),
            plan.assoc_array_read_constraints(),
            plan.float_predicate_read_constraints(),
        );
    }
    if !eval_scope_read_constraints_supported(
        ctx,
        plan.array_read_constraints(),
        plan.assoc_array_read_constraints(),
        plan.float_predicate_read_constraints(),
    ) {
        return Ok(false);
    }
    if param_types.len() != 1 || return_type != PhpType::Mixed {
        return Ok(false);
    }
    ctx.emitter
        .comment("eval literal AOT compiled EIR function with eval scope");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_GUARDED_STACK_BYTES);
    ensure_eval_scope(ctx)?;
    let read_names = plan.reads().clone();
    let write_names = plan.writes().clone();
    let mut flush_names = read_names.clone();
    flush_names.extend(write_names.iter().cloned());
    let sync_locals = eval_sync_locals(ctx);
    // The compiled scope contains lexical locals. A same-named program global
    // must not overwrite that entry or be restored from its unrelated value.
    // Only the top level's scope IS the global scope. Inside a function a fragment reaches a
    // program global through `global $g`, which this AOT path refuses; syncing the globals
    // into the function's eval scope made `function e() { eval('$g = 5;'); }` overwrite a
    // top-level `$g` that the fragment only shadowed.
    let sync_globals = eval_sync_globals(ctx).into_iter()
        .filter(|_| ctx.is_main)
        .filter(|global| !sync_locals.iter().any(|local| local.name == global.name))
        .collect::<Vec<_>>();
    let flush_locals = filter_eval_sync_locals_by_name(sync_locals.clone(), &flush_names);
    let flush_globals = filter_eval_sync_globals_by_name(sync_globals.clone(), &flush_names);
    let reload_locals = filter_eval_sync_locals_by_name(sync_locals, &write_names);
    let reload_globals = filter_eval_sync_globals_by_name(sync_globals, &write_names);
    flush_eval_scope_locals(ctx, &flush_locals)?;
    flush_eval_globals_to_local_scope(ctx, &flush_globals);
    prepare_native_scope_bindings(ctx, &flush_locals, &flush_globals)?;
    load_eval_scope_to_arg(ctx, 0);
    abi::emit_call_label(ctx.emitter, &function_symbol(&function_name));
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_RESULT_SAVE_OFFSET);
    callable_guards::guard_saved_owner(
        ctx.emitter,
        EVAL_RESULT_GUARD_OFFSET,
        EVAL_RESULT_SAVE_OFFSET,
    );
    reload_eval_scope_locals(ctx, &reload_locals, None)?;
    reload_eval_globals_from_local_scope(ctx, &reload_globals)?;
    clear_native_scope_bindings(ctx, &flush_locals, &flush_globals);
    callable_guards::unguard_saved_owner(ctx.emitter, EVAL_RESULT_GUARD_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_SAVE_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_GUARDED_STACK_BYTES);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Calls a read-only scope eval AOT function by passing direct boxed Mixed params.
pub(super) fn lower_eval_literal_scope_read_param_eir_function(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    function_name: &str,
    param_types: &[PhpType],
    return_type: &PhpType,
    read_names: &BTreeSet<String>,
    quiet_read_names: &BTreeSet<String>,
    array_read_constraints: &BTreeSet<String>,
    assoc_array_read_constraints: &BTreeSet<String>,
    float_predicate_read_constraints: &BTreeSet<String>,
) -> Result<bool> {
    if param_types.len() != read_names.len()
        || param_types
            .iter()
            .any(|ty| ty.codegen_repr() != PhpType::Mixed)
        || return_type.codegen_repr() != PhpType::Mixed
    {
        return Ok(false);
    }
    if !eval_scope_read_constraints_supported(
        ctx,
        array_read_constraints,
        assoc_array_read_constraints,
        float_predicate_read_constraints,
    ) {
        return Ok(false);
    }
    let Some(param_sources) = eval_scope_read_param_sources(ctx, read_names, quiet_read_names) else {
        return Ok(false);
    };
    ctx.emitter
        .comment("eval literal AOT compiled EIR function with direct read params");
    // The fragment BORROWS its params (it clones what it keeps). A box made here for the call
    // -- the null of an unset variable, or a typed local boxed for the Mixed ABI -- is owned by
    // this site, so it is kept in an owner area and released after the call; dropping it on the
    // floor leaked one box per read variable per call (`function f() { eval('isset($g)…'); }`).
    let result_reg = abi::int_result_reg(ctx.emitter);
    let owners_bytes = param_sources.len() * 16;
    abi::emit_reserve_temporary_stack(ctx.emitter, owners_bytes);
    let mut fresh = Vec::with_capacity(param_sources.len());
    for (index, source) in param_sources.iter().enumerate() {
        fresh.push(emit_eval_scope_read_param_source(ctx, source)?);
        abi::emit_store_to_sp(ctx.emitter, result_reg, index * 16);
    }
    for index in 0..param_sources.len() {
        // Every earlier push moved the owner area 16 bytes further from `sp`.
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, index * 16 + index * 16);
        abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
    }
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, param_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(ctx.emitter, &assignments);
    let caller_stack_pad_bytes =
        abi::outgoing_call_stack_pad_bytes(ctx.emitter.target, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &function_symbol(function_name));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    store_if_result(ctx, inst)?;
    for (index, owned) in fresh.iter().enumerate() {
        if *owned {
            abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, index * 16);
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, owners_bytes);
    Ok(true)
}

/// Resolves read-only eval variables to direct locals, rejecting undefined reads.
pub(super) fn eval_scope_read_param_sources(
    ctx: &FunctionContext<'_>,
    read_names: &BTreeSet<String>,
    quiet_read_names: &BTreeSet<String>,
) -> Option<Vec<EvalScopeReadParamSource>> {
    let sync_locals = eval_sync_locals(ctx);
    read_names
        .iter()
        .map(|name| {
            if let Some(local) = sync_locals.iter().find(|local| local.name == *name) {
                return Some(EvalScopeReadParamSource::Local(local.clone()));
            }
            if ctx.function.locals.iter().any(|local| {
                local.name.as_deref() == Some(name.as_str())
                    && local.kind == LocalKind::PhpLocal
                    && !local_uses_eval_global_sync(ctx, local.name.as_deref())
                    && local.php_type.codegen_repr() == PhpType::Void
            }) {
                return Some(EvalScopeReadParamSource::Null);
            }
            quiet_read_names
                .contains(name)
                .then_some(EvalScopeReadParamSource::Null)
        })
        .collect()
}

/// Returns true when constrained direct read params have compatible local sources.
pub(super) fn eval_scope_read_constraints_supported(
    ctx: &FunctionContext<'_>,
    array_read_constraints: &BTreeSet<String>,
    assoc_array_read_constraints: &BTreeSet<String>,
    float_predicate_read_constraints: &BTreeSet<String>,
) -> bool {
    let sync_locals = eval_sync_locals(ctx);
    array_read_constraints.iter().all(|name| {
        sync_locals
            .iter()
            .find(|local| local.name == *name)
            .is_some_and(|local| eval_scope_read_array_param_type_supported(&local.ty))
    }) && assoc_array_read_constraints.iter().all(|name| {
        sync_locals
            .iter()
            .find(|local| local.name == *name)
            .is_some_and(|local| eval_scope_read_assoc_array_param_type_supported(&local.ty))
    }) && float_predicate_read_constraints.iter().all(|name| {
        sync_locals
            .iter()
            .find(|local| local.name == *name)
            .is_some_and(|local| eval_scope_read_float_predicate_param_type_supported(&local.ty))
    })
}

/// Returns true when a direct read-param source has array-only semantics.
pub(super) fn eval_scope_read_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. }
    )
}

/// Returns true when a direct read-param source has associative-array-only semantics.
pub(super) fn eval_scope_read_assoc_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::AssocArray { .. })
}

/// Returns true when a direct read-param source can feed IEEE float predicates.
pub(super) fn eval_scope_read_float_predicate_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Int | PhpType::Float)
}

/// Emits one direct read-param value as a boxed Mixed result.
/// Materializes one direct read param as a Mixed box, returning whether the box was made here
/// (and is therefore this call site's to release).
pub(super) fn emit_eval_scope_read_param_source(
    ctx: &mut FunctionContext<'_>,
    source: &EvalScopeReadParamSource,
) -> Result<bool> {
    match source {
        EvalScopeReadParamSource::Local(local) => {
            let ty = ctx.load_local_to_result(local.slot)?.codegen_repr();
            if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
                // A `?ptr<T>` local keeps its union, so address 0 is PHP null.
                let box_ty = ctx.local_mixed_box_type(local.slot)?;
                emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
                return Ok(true);
            }
            Ok(false)
        }
        EvalScopeReadParamSource::Null => {
            emit_core_mixed_null_cell(ctx);
            Ok(true)
        }
    }
}

/// Boxes PHP null with the core Mixed runtime helper.
pub(super) fn emit_core_mixed_null_cell(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #8");                              // materialize the core Mixed null runtime tag
            ctx.emitter.instruction("mov x1, #0");                              // null has no low payload word
            ctx.emitter.instruction("mov x2, #0");                              // null has no high payload word
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, 8");                              // materialize the core Mixed null runtime tag
            ctx.emitter.instruction("xor edi, edi");                            // null has no low payload word
            ctx.emitter.instruction("xor esi, esi");                            // null has no high payload word
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
}

/// Returns true when a static function call matches the EIR eval AOT codegen subset.
pub(super) fn eval_literal_static_function_supported_by_codegen(
    ctx: &FunctionContext<'_>,
    name: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let key = php_symbol_key(name.trim_start_matches('\\'));
    let Some(function) = ctx
        .module
        .functions
        .iter()
        .find(|function| php_symbol_key(function.name.trim_start_matches('\\')) == key)
    else {
        return false;
    };
    let signature = function_signature_from_eir(function);
    crate::eval_aot::static_function_signature_supported(&signature, args)
}

/// Returns true when a static method call matches the EIR eval AOT codegen subset.
pub(super) fn eval_literal_static_method_supported_by_codegen(
    ctx: &FunctionContext<'_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let StaticReceiver::Named(class_name) = receiver else {
        return false;
    };
    let class_name = class_name.as_str().trim_start_matches('\\');
    let method_key = php_symbol_key(method);
    let Some(receiver_info) = ctx.module.class_infos.get(class_name) else {
        return false;
    };
    if receiver_info
        .static_method_visibilities
        .get(&method_key)
        .unwrap_or(&Visibility::Public)
        != &Visibility::Public
    {
        return false;
    }
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    let Some(signature) = ctx
        .module
        .class_infos
        .get(impl_class)
        .and_then(|class_info| class_info.static_methods.get(&method_key))
    else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}
