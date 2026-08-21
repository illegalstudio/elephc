//! Purpose:
//! Filter ids, params, user filters, and boxed stream resources.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Maps runtime-supported built-in stream filter names to byte-table ids.
///
/// `string.strip_tags` is deliberately absent: php removed that filter in 8.0
/// (php-src ext/standard/filters.c registers no `strip_tags` factory), so an
/// attach must miss here and report `Unable to locate filter`.
///
/// `consumed` takes 13 because `__rt_fwrite` claims 4, 10 and 12 for the
/// zlib/bzip2/iconv write paths, and an id no arm claims is left byte-for-byte
/// alone by `__rt_apply_stream_filter` — which is exactly php's `consumed`
/// transform: it appends every bucket to its output brigade unchanged and only
/// counts bytes (php-src ext/standard/filters.c:1649-1653).
///
/// Kept in step with `BUILTIN_FILTER_NAMES`, the run-time table the dynamic
/// name path scans.
pub(super) fn stream_filter_id(name: &str) -> Option<u8> {
    match name {
        "string.toupper" => Some(1),
        "string.tolower" => Some(2),
        "string.rot13" => Some(3),
        "dechunk" => Some(5),
        "convert.base64-encode" => Some(6),
        "convert.base64-decode" => Some(7),
        "convert.quoted-printable-encode" => Some(8),
        "convert.quoted-printable-decode" => Some(9),
        "consumed" => Some(13),
        _ => None,
    }
}

/// Reads a compile-time integer filter parameter from the fourth builtin operand.
pub(super) fn const_int_filter_param(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    _key: &str,
    primary: bool,
    min: i64,
    max: i64,
) -> Result<Option<i64>> {
    if !primary {
        return Ok(None);
    }
    let Some(value) = inst.operands.get(3).copied() else {
        return Ok(None);
    };
    Ok(optional_const_i64_operand(ctx, value)?.map(|n| n.clamp(min, max)))
}

/// Returns a literal integer operand when the value was produced by `ConstI64`.
pub(super) fn optional_const_i64_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<i64>> {
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
    if inst_ref.op != Op::ConstI64 {
        return Ok(None);
    }
    match inst_ref.immediate {
        Some(Immediate::I64(value)) => Ok(Some(value)),
        _ => Err(CodegenIrError::invalid_module(
            "integer literal operand has no i64 immediate",
        )),
    }
}

/// Attaches a built-in stream filter by writing its id into per-fd direction tables.
pub(super) fn lower_builtin_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    filter_id: u8,
    prepend: bool,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_open_stream_handle_to_result(ctx, stream, "stream_filter_append")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    materialize_stream_filter_mode(ctx, inst, Some(0))?;
    // OMITTING `$params` is not the same as passing null. php tests the zval POINTER, which is
    // NULL only when the argument was not supplied — which is why `stream_filter_append($h,
    // "convert.base64-encode", STREAM_FILTER_WRITE)` succeeds while the same call with an explicit
    // `null` fourth argument is REFUSED. Retaining nothing for a three-operand call is what keeps
    // the two apart at run time.
    let params_inst = (inst.operands.len() >= 4).then_some(inst);
    emit_attach_filter_node(ctx, Some(filter_id), prepend, false, params_inst)?;
    // Box the registry handle itself. `emit_boxed_stream_resource` mints a fresh
    // display id, which the legacy design could afford because its "filter
    // resource" was really the stream descriptor; a chain node has to be findable
    // again by `stream_filter_remove()`.
    emit_filter_handle_or_param_refusal(ctx, inst, prepend)?;
    store_if_result(ctx, inst)
}

/// Creates a filter node and links it into the direction chains the mode selects.
///
/// On entry the stream handle is on the stack and the mode bits are in the int
/// result register. On exit the new filter handle is in the result register, ready
/// to be boxed as the resource `stream_filter_append()` returns.
///
/// A node attached with `STREAM_FILTER_ALL` is linked into both chains, which is
/// why the direction bits are stored on the node rather than inferred from which
/// list it sits in.
///
/// With `user_object`, the node carries a `php_user_filter` instance parked on the
/// stack under the stream handle, and its built-in id stays 0. The node retains no
/// params value in that case: the attach helper already exposed `$params` on the
/// instance, which is what `filter()` reads.
///
/// A BUILT-IN node retains `$params` instead, because php's own built-in filters read it:
/// `convert.base64-encode` and `convert.quoted-printable-encode` take `line-length` and
/// `line-break-chars`, and the quoted-printable pair also takes `binary`. `params_inst` is the call
/// whose fourth operand carries them; `None` retains nothing, which is what the user path wants.
fn emit_attach_filter_node(
    ctx: &mut FunctionContext<'_>,
    filter_id: Option<u8>,
    prepend: bool,
    user_object: bool,
    params_inst: Option<&Instruction>,
) -> Result<()> {
    let skip_read = ctx.next_label("sf_chain_skip_read");
    let skip_write = ctx.next_label("sf_chain_skip_write");
    let prepend_flag = i64::from(prepend);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x4");                               // recover the owning stream handle
            if user_object {
                abi::emit_pop_reg(ctx.emitter, "x5");                           // recover the php_user_filter instance
            }
            abi::emit_reserve_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction("str x4, [sp, #0]");                        // preserve the stream handle across the calls
            ctx.emitter.instruction("str x0, [sp, #8]");                        // preserve the requested direction bits
            if user_object {
                ctx.emitter.instruction("str x5, [sp, #24]");                   // preserve the instance across the calls
            }
            if filter_id.is_none() && !user_object {
                ctx.emitter.instruction("str x9, [sp, #16]");                    // park the run-time id: the params call clobbers x9
            }
            if let Some(params_inst) = params_inst {
                materialize_stream_filter_params(ctx, params_inst)?;             // the boxed `$params` the node retains
                ctx.emitter.instruction("str x0, [sp, #24]");                    // a built-in node has this slot free
            }
            ctx.emitter.instruction("ldr x2, [sp, #8]");                         // direction bits, past any params call
            match filter_id {
                // A literal name resolves at compile time; a dynamic one arrives in
                // x9 from __rt_builtin_filter_id.
                Some(id) => ctx.emitter.instruction(&format!("mov x0, #{id}")),  // built-in filter id
                None if user_object => ctx.emitter.instruction("mov x0, #0"),    // a user filter has no built-in id
                None => ctx.emitter.instruction("ldr x0, [sp, #16]"),            // run-time resolved filter id, reparked above
            }
            if user_object {
                ctx.emitter.instruction("ldr x1, [sp, #24]");                   // the instance this node owns
            } else {
                ctx.emitter.instruction("mov x1, #0");                          // built-ins carry no user-filter object
            }
            match params_inst {
                // php's built-in filters read `$params`; a user filter reads it off its instance.
                Some(_) => ctx.emitter.instruction("ldr x3, [sp, #24]"),         // the retained params box
                None => ctx.emitter.instruction("mov x3, #0"),                   // params live on the instance
            }
            abi::emit_call_label(ctx.emitter, "__rt_filter_create");            // x0 = the new filter handle
            ctx.emitter.instruction("str x0, [sp, #16]");                       // preserve the filter handle

            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // direction bits
            ctx.emitter.instruction("tst x9, #1");                              // is STREAM_FILTER_READ set?
            ctx.emitter.instruction(&format!("b.eq {}", skip_read));
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // owning stream handle
            ctx.emitter.instruction("ldr x1, [sp, #16]");                       // filter handle
            ctx.emitter.instruction(&format!("mov x2, #{STREAM_READ_FILTER_HEAD_OFFSET}"));
            ctx.emitter.instruction(&format!("mov x3, #{prepend_flag}"));       // prepend selects head insertion
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_link");
            ctx.emitter.label(&skip_read);

            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // direction bits
            ctx.emitter.instruction("tst x9, #2");                              // is STREAM_FILTER_WRITE set?
            ctx.emitter.instruction(&format!("b.eq {}", skip_write));
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // owning stream handle
            ctx.emitter.instruction("ldr x1, [sp, #16]");                       // filter handle
            ctx.emitter.instruction(&format!("mov x2, #{STREAM_WRITE_FILTER_HEAD_OFFSET}"));
            ctx.emitter.instruction(&format!("mov x3, #{prepend_flag}"));       // prepend selects head insertion
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_link");
            ctx.emitter.label(&skip_write);

            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // return the filter handle
            abi::emit_release_temporary_stack(ctx.emitter, 32);
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rcx");                              // recover the owning stream handle
            if user_object {
                abi::emit_pop_reg(ctx.emitter, "r14");                          // recover the php_user_filter instance
            }
            abi::emit_reserve_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rcx");            // preserve the stream handle across the calls
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // preserve the requested direction bits
            if user_object {
                ctx.emitter.instruction("mov QWORD PTR [rsp + 24], r14");       // preserve the instance across the calls
            }
            if filter_id.is_none() && !user_object {
                ctx.emitter.instruction("mov QWORD PTR [rsp + 16], r13");        // park the run-time id: the params call clobbers r13
            }
            if let Some(params_inst) = params_inst {
                materialize_stream_filter_params(ctx, params_inst)?;             // the boxed `$params` the node retains
                ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rax");        // a built-in node has this slot free
            }
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");             // direction bits, past any params call
            match filter_id {
                // A literal name resolves at compile time; a dynamic one arrives in
                // r13 from __rt_builtin_filter_id.
                Some(id) => ctx.emitter.instruction(&format!("mov rdi, {id}")),  // built-in filter id
                None if user_object => ctx.emitter.instruction("xor edi, edi"),  // a user filter has no built-in id
                None => ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]"), // run-time resolved filter id, reparked above
            }
            if user_object {
                ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");       // the instance this node owns
            } else {
                ctx.emitter.instruction("xor esi, esi");                        // built-ins carry no user-filter object
            }
            match params_inst {
                // php's built-in filters read `$params`; a user filter reads it off its instance.
                Some(_) => ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 24]"), // the retained params box
                None => ctx.emitter.instruction("xor ecx, ecx"),                  // params live on the instance
            }
            abi::emit_call_label(ctx.emitter, "__rt_filter_create");            // rax = the new filter handle
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the filter handle

            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");             // direction bits
            ctx.emitter.instruction("test r9, 1");                              // is STREAM_FILTER_READ set?
            ctx.emitter.instruction(&format!("jz {}", skip_read));
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // owning stream handle
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");           // filter handle
            ctx.emitter.instruction(&format!("mov rdx, {STREAM_READ_FILTER_HEAD_OFFSET}"));
            ctx.emitter.instruction(&format!("mov rcx, {prepend_flag}"));       // prepend selects head insertion
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_link");
            ctx.emitter.label(&skip_read);

            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");             // direction bits
            ctx.emitter.instruction("test r9, 2");                              // is STREAM_FILTER_WRITE set?
            ctx.emitter.instruction(&format!("jz {}", skip_write));
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // owning stream handle
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");           // filter handle
            ctx.emitter.instruction(&format!("mov rdx, {STREAM_WRITE_FILTER_HEAD_OFFSET}"));
            ctx.emitter.instruction(&format!("mov rcx, {prepend_flag}"));       // prepend selects head insertion
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_link");
            ctx.emitter.label(&skip_write);

            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // return the filter handle
            abi::emit_release_temporary_stack(ctx.emitter, 32);
        }
    }
    Ok(())
}

/// Materializes the stream-filter mode operand, deducing php's `$mode = 0`
/// default from the stream's own open mode.
///
/// php's default is `0`, not `STREAM_FILTER_ALL`, and `0` does not mean "no
/// chain": php reads `stream->mode` and enables the chains that mode can use
/// (php-src streamsfuncs.c:1202-1214). The same rule covers any mode with no
/// `STREAM_FILTER_ALL` bit set, which is why the test is `mode & 3` rather than
/// `mode == 0` — php's own guard is `(read_write & PHP_STREAM_FILTER_ALL) == 0`.
///
/// One measured gap remains: a mode naming no direction (`x`, `c`) deduces 0
/// here and the node joins no chain, while php refuses the attach and returns
/// `false` (`fopen($f,"x"); var_dump(stream_filter_append($h,"string.toupper"))`
/// prints `bool(false)` on 8.5.6). Both leave the bytes unfiltered; only the
/// return value differs, and it differed the same way before this default
/// existed.
///
/// `handle_sp_offset` is where the caller parked the stream HANDLE on the
/// temporary stack — the attach paths stage different amounts before calling.
/// `None` means the caller holds a raw descriptor rather than a handle (the
/// `convert.iconv.*` and legacy paths), and there the default stays
/// `STREAM_FILTER_ALL`: the deduction reads the mode string off the stream
/// state, which only a handle can reach.
pub(super) fn materialize_stream_filter_mode(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    handle_sp_offset: Option<usize>,
) -> Result<()> {
    if inst.operands.len() < 3 {
        match handle_sp_offset {
            Some(offset) => emit_stream_filter_deduced_mode(ctx, offset),
            None => emit_fd_result(ctx, 3),
        }
        return Ok(());
    }
    let mode = expect_operand(inst, 2)?;
    require_int_or_bool(
        ctx.load_value_to_result(mode)?.codegen_repr(),
        "stream_filter_append mode",
    )?;
    let Some(offset) = handle_sp_offset else {
        return Ok(());
    };
    let keep = ctx.next_label("sf_mode_explicit");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("tst x0, #3");                              // did the caller name at least one chain?
            ctx.emitter.instruction(&format!("b.ne {}", keep));                 // an explicit READ/WRITE/ALL wins over the stream's own mode
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, 3");                             // did the caller name at least one chain?
            ctx.emitter.instruction(&format!("jnz {}", keep));                  // an explicit READ/WRITE/ALL wins over the stream's own mode
        }
    }
    emit_stream_filter_deduced_mode(ctx, offset);
    ctx.emitter.label(&keep);
    Ok(())
}

/// Leaves php's stream-mode-deduced filter direction in the integer result.
fn emit_stream_filter_deduced_mode(ctx: &mut FunctionContext<'_>, handle_sp_offset: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x0, [sp, #{}]", handle_sp_offset));  // reload the stream handle the caller parked
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_sp_offset)); // reload the stream handle the caller parked
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_filter_default_mode");
}

/// Materializes the optional stream-filter params operand as an owned boxed
/// Mixed cell, defaulting to PHP null when the caller omitted it.
pub(super) fn materialize_stream_filter_params(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() < 4 {
        emit_null_mixed(ctx);
        return Ok(());
    }
    let params = expect_operand(inst, 3)?;
    let params_ty = ctx.value_php_type(params)?.codegen_repr();
    ctx.load_value_to_result(params)?;
    if matches!(params_ty, PhpType::Mixed | PhpType::Union(_)) {
        if !ctx.value_can_own_mixed_box_source(params)? {
            abi::emit_incref_if_refcounted(ctx.emitter, &params_ty);
        }
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &params_ty);
    }
    Ok(())
}

/// Mints the `stream filter` resource php hands back for a filter compiled as an inline shape.
///
/// `zlib.*`, `bzip2.*` and `convert.iconv.*` filter through code emitted over the DESCRIPTOR rather
/// than through a chain node, so they filtered correctly and minted nothing: `is_resource()` on the
/// result answered false and `get_resource_type()` answered "Unknown", where php answers a live
/// `stream filter`. The node created here carries no built-in id and no `php_user_filter`, which is
/// already enough for the chain applier to pass it by, and joining the chain is what makes closing
/// the stream close it too — php invalidates the filter resource on `fclose()`.
///
/// The stream operand is re-materialized, which is the pattern the surrounding lowerings already
/// use for their diagnostics; every caller reaches this with the operand already evaluated once.
pub(super) fn emit_inert_filter_resource(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    prepend: bool,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_open_stream_handle_to_result(ctx, stream, "stream_filter_append")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    materialize_stream_filter_mode(ctx, inst, Some(0))?;
    emit_attach_filter_node(ctx, Some(0), prepend, false, None)?;
    abi::emit_call_label(ctx.emitter, "__rt_filter_mark_inert");
    emit_boxed_filter_handle(ctx);
    Ok(())
}

/// Boxes the new filter handle, or reports a refused `$params` and answers php's `false`.
///
/// `__rt_filter_create` hands back 0 when the node's filter PARSES `$params` and was given
/// something that is not an array — which only the four `convert.*` filters do. php raises TWO
/// warnings for that, `Stream filter (<name>): invalid filter parameter` then `Unable to create or
/// locate filter "<name>"`, and returns `false`; elephc attached a working filter and said nothing.
/// Measured on `php -n` 8.5.6.
///
/// The check is emitted only when a fourth argument was written, so a call that cannot be refused
/// pays nothing. The name is re-materialized rather than saved: this path is reached only for a
/// LITERAL filter name, so producing it a second time evaluates no user code.
fn emit_filter_handle_or_param_refusal(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    prepend: bool,
) -> Result<()> {
    if inst.operands.len() < 4 {
        emit_boxed_filter_handle(ctx);                                          // nothing was passed to refuse
        return Ok(());
    }
    let filter = expect_operand(inst, 1)?;
    let live = ctx.next_label("filter_params_ok");
    let done = ctx.next_label("filter_attach_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbnz x0, {live}")),  // a live handle attached
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jnz {live}"));                    // a live handle attached
        }
    }
    load_string_to_result(ctx, filter, "stream_filter_append filter")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // The name arrives in x1/x2 and the composer wants it in x0/x1, so the length moves
            // first — the pointer move would otherwise clobber it.
            ctx.emitter.instruction("mov x0, x1");                              // filter-name pointer
            ctx.emitter.instruction("mov x1, x2");                              // filter-name length
            ctx.emitter.instruction(&format!("mov x2, #{}", i64::from(prepend)));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // filter-name pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // filter-name length
            ctx.emitter.instruction(&format!("mov rdx, {}", i64::from(prepend)));
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_filter_param_warning");
    emit_boxed_bool(ctx, false);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&live);
    emit_boxed_filter_handle(ctx);
    ctx.emitter.label(&done);
    Ok(())
}

/// Reports a filter name that resolves to nothing, the way php-src does.
///
/// The message names the filter, so it is composed at run time. Each function names
/// ITSELF in the prefix, chosen here, so the runtime composer needs no branch.
fn emit_missing_filter_warning(
    ctx: &mut FunctionContext<'_>,
    filter: ValueId,
    prepend: bool,
) -> Result<()> {
    let (symbol, text): (&str, &str) = if prepend {
        (
            "_diag_filter_missing_prepend_prefix",
            "Warning: stream_filter_prepend(): Unable to locate filter \"",
        )
    } else {
        (
            "_diag_filter_missing_append_prefix",
            "Warning: stream_filter_append(): Unable to locate filter \"",
        )
    };
    load_string_to_result(ctx, filter, "stream_filter_append filter")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // The name arrives in x1/x2 and the prefix has to land in x0/x1, so the name
            // moves up FIRST — length before pointer, or the pointer move clobbers it.
            ctx.emitter.instruction("mov x3, x2");                              // name length
            ctx.emitter.instruction("mov x2, x1");                              // name pointer
            ctx.emitter.adrp("x0", symbol);
            ctx.emitter.add_lo12("x0", "x0", symbol);
            ctx.emitter.instruction(&format!("mov x1, #{}", text.len()));       // prefix length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rdx");                            // name length
            ctx.emitter.instruction("mov rdx, rax");                            // name pointer
            ctx.emitter.instruction(&format!("lea rdi, [rip + {symbol}]"));     // prefix pointer
            ctx.emitter.instruction(&format!("mov esi, {}", text.len()));       // prefix length
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_filter_missing_warning");
    Ok(())
}

/// Attaches a user-defined stream filter through the runtime registry.

pub(super) fn lower_user_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    prepend: bool,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    let filter = expect_operand(inst, 1)?;
    // A dynamic name may still be a built-in. Resolving it here keeps such
    // filters on the chain instead of the legacy per-descriptor slots, which hold
    // only 256 descriptors.
    let user_path = ctx.next_label("sfa_user_path");
    let attached = ctx.next_label("sfa_attached");
    load_string_to_result(ctx, filter, "stream_filter_append filter")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // name pointer
            ctx.emitter.instruction("mov x1, x2");                              // name length
            abi::emit_call_label(ctx.emitter, "__rt_builtin_filter_id");
            ctx.emitter.instruction(&format!("cbz x0, {}", user_path));         // not a built-in: use the user-filter path
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // preserve the resolved id across the operand loads
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // name pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // name length
            abi::emit_call_label(ctx.emitter, "__rt_builtin_filter_id");
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {}", user_path));              // not a built-in: use the user-filter path
            ctx.emitter.instruction("push rax");                                // preserve the resolved id across the operand loads
        }
    }
    load_open_stream_handle_to_result(ctx, stream, "stream_filter_append")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    materialize_stream_filter_mode(ctx, inst, Some(0))?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // The node creator reads the run-time id from x9 and pops the handle itself.
            ctx.emitter.instruction("ldr x9, [sp, #16]");                       // resolved id, below the pushed handle
        }
        Arch::X86_64 => {
            // 16, not 8: a push reserves a whole 16-byte slot, so the id pushed before the
            // handle sits one full slot down. Reading at 8 picked up that slot's padding, so the
            // node was created with a garbage filter id and the filter did nothing — silently,
            // and only for a filter named by a run-time expression, which is why the literal and
            // user-filter paths stayed green.
            ctx.emitter.instruction("mov r13, QWORD PTR [rsp + 16]");           // resolved id, below the pushed handle
        }
    }
    let params_inst = (inst.operands.len() >= 4).then_some(inst);
    emit_attach_filter_node(ctx, None, prepend, false, params_inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add sp, sp, #16");                         // drop the preserved id
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rsp, 8");                              // drop the preserved id
        }
    }
    emit_boxed_filter_handle(ctx);
    abi::emit_jump(ctx.emitter, &attached);
    ctx.emitter.label(&user_path);
    lower_user_stream_filter_attach_node(ctx, inst, prepend)?;
    // Both paths leave their boxed result in the same register, so the store belongs
    // here: the built-in branch used to jump over the user branch's own store and
    // dropped the resource `$f = stream_filter_append($s, $dynamicBuiltinName)`
    // should have received.
    ctx.emitter.label(&attached);
    store_if_result(ctx, inst)
}

/// Attaches a user-registered filter as a chain node carrying its `php_user_filter`.
///
/// PHP hands back a `stream filter` resource distinct from the stream, and closing the
/// stream invalidates it. The per-descriptor tables could support neither: they returned
/// the STREAM's own descriptor boxed as the filter resource, so `is_resource($filter)`
/// stayed true after `fclose()` and `stream_filter_remove()` had no node to unlink.
///
/// The attach helper runs in node mode — signalled by the negative descriptor — so it
/// resolves the name, instantiates the class, exposes `$params` and runs `onCreate()`
/// exactly as before, but hands the instance back instead of registering it. A filter
/// lives in exactly one mechanism, and for user filters that mechanism is now the chain.
fn lower_user_stream_filter_attach_node(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    prepend: bool,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    let filter = expect_operand(inst, 1)?;
    let fail = ctx.next_label("sfan_false");
    let done = ctx.next_label("sfan_done");

    load_open_stream_handle_to_result(ctx, stream, "stream_filter_append")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_string_to_result(ctx, filter, "stream_filter_append filter")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");                   // name pointer/length
            materialize_stream_filter_mode(ctx, inst, Some(16))?;
            abi::emit_push_reg(ctx.emitter, "x0");                              // requested direction bits
            materialize_stream_filter_params(ctx, inst)?;
            ctx.emitter.instruction("mov x4, x0");                              // boxed stream-filter params
            abi::emit_pop_reg(ctx.emitter, "x3");                               // direction bits
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");                    // name pointer/length
            ctx.emitter.instruction("mov x0, #-1");                             // node mode: instantiate, register nothing
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_attach_user");
            ctx.emitter.instruction(&format!("cbz x0, {}", fail));              // unknown name or refused onCreate
            ctx.emitter.instruction("ldr x9, [sp]");                            // the stream handle parked before the call
            ctx.emitter.instruction("str x0, [sp]");                            // park the instance in its place
            abi::emit_push_reg(ctx.emitter, "x9");                              // handle on top, instance beneath
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");                 // name pointer/length
            materialize_stream_filter_mode(ctx, inst, Some(16))?;
            abi::emit_push_reg(ctx.emitter, "rax");                             // requested direction bits
            materialize_stream_filter_params(ctx, inst)?;
            ctx.emitter.instruction("mov r8, rax");                             // boxed stream-filter params
            abi::emit_pop_reg(ctx.emitter, "rcx");                              // direction bits
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");                  // name pointer/length
            ctx.emitter.instruction("mov rdi, -1");                             // node mode: instantiate, register nothing
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_attach_user");
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {}", fail));                   // unknown name or refused onCreate
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp]");                 // the stream handle parked before the call
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // park the instance in its place
            abi::emit_push_reg(ctx.emitter, "r9");                              // handle on top, instance beneath
        }
    }
    materialize_stream_filter_mode(ctx, inst, Some(0))?;                                 // direction bits for the node
    emit_attach_filter_node(ctx, None, prepend, true, None)?;
    emit_boxed_filter_handle(ctx);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&fail);
    abi::emit_release_temporary_stack(ctx.emitter, 16);                         // drop the parked stream handle
    // php-src names the filter it could not find. Returning false silently left a
    // misspelled name indistinguishable from one that attached.
    emit_missing_filter_warning(ctx, filter, prepend)?;
    emit_boxed_bool(ctx, false);
    ctx.emitter.label(&done);
    Ok(())
}

/// Attaches a user-registered filter through the legacy per-descriptor slots.
///
/// Kept while the chain path is being validated test by test; see the node variant.
#[allow(dead_code)]
fn lower_user_stream_filter_attach_legacy(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    let filter = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_string_to_result(ctx, filter, "stream_filter_append filter")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            materialize_stream_filter_mode(ctx, inst, None)?;
            abi::emit_push_reg(ctx.emitter, "x0");
            materialize_stream_filter_params(ctx, inst)?;
            ctx.emitter.instruction("mov x4, x0");                              // pass the boxed stream-filter params to the attach helper
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("ldr x0, [sp]");                            // pass the saved stream descriptor without popping it yet
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            materialize_stream_filter_mode(ctx, inst, None)?;
            abi::emit_push_reg(ctx.emitter, "rax");
            materialize_stream_filter_params(ctx, inst)?;
            ctx.emitter.instruction("mov r8, rax");                             // pass the boxed stream-filter params to the attach helper
            abi::emit_pop_reg(ctx.emitter, "rcx");
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // pass the saved stream descriptor without popping it yet
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_filter_attach_user");
    let fail = ctx.next_label("sfau_false");
    let done = ctx.next_label("sfau_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", fail));              // unknown filter or failed onCreate returns PHP false
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload the descriptor for the returned filter resource
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_boxed_stream_resource(ctx);
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the PHP false boxing path
            ctx.emitter.label(&fail);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_boxed_bool(ctx, false);
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the attach helper report success?
            ctx.emitter.instruction(&format!("jz {}", fail));                   // unknown filter or failed onCreate returns PHP false
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // reload the descriptor for the returned filter resource
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_boxed_stream_resource(ctx);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the PHP false boxing path
            ctx.emitter.label(&fail);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_boxed_bool(ctx, false);
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Boxes the current integer result as a PHP stream resource Mixed cell.
///
/// Mints a fresh resource id first: like a descriptor, a filter handle can repeat a
/// number a previous, now-released filter used, and PHP never hands the same
/// resource id out twice.
pub(super) fn emit_boxed_stream_resource(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_resource_id_mint");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // use the descriptor as the resource payload
            ctx.emitter.instruction("mov x2, #0");                              // resource Mixed payloads do not use the high word
            ctx.emitter.instruction("mov x0, #9");                              // runtime tag 9 = resource
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // use the descriptor as the resource payload
            ctx.emitter.instruction("xor esi, esi");                            // resource Mixed payloads do not use the high word
            ctx.emitter.instruction("mov eax, 9");                              // runtime tag 9 = resource
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
}

/// Boxes a PHP boolean Mixed cell in the current result register.
pub(super) fn emit_boxed_bool(ctx: &mut FunctionContext<'_>, value: bool) {
    emit_bool_result(ctx, value);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
}

/// Boxes a PHP null Mixed cell in the current result register.
pub(super) fn emit_null_mixed(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #0");                              // null has no payload
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor eax, eax");                            // null has no payload
        }
    }
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}

