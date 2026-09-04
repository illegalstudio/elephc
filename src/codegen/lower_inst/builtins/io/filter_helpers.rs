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

/// Which direction bits a node is created with.
enum NodeDirection {
    /// Whatever the mode asked for, as parked in the attach frame.
    Requested,
    /// One fixed direction: the half of a `STREAM_FILTER_ALL` this node serves.
    Fixed(i64),
}

/// Creates a filter node and links it into the direction chains the mode selects.
///
/// On entry the stream handle is on the stack and the mode bits are in the int
/// result register. On exit the new filter handle is in the result register, ready
/// to be boxed as the resource `stream_filter_append()` returns.
///
/// A mode naming BOTH directions mints TWO nodes, one per chain, because that is what php does:
/// `apply_filter_to_stream` creates a filter for the read chain, then a second one for the write
/// chain, and hands back the one it created LAST. MEASURED on `php -n` 8.5.6 with a user filter
/// that uppercases:
///
/// ```text
/// $r = stream_filter_append($h, "up");   // no mode: the default names both directions
/// stream_filter_remove($r);
/// fwrite($h, "abc");                     // ON DISK  : abc   — the write filter went
/// rewind($h); stream_get_contents($h);   // READ BACK: ABC   — the read filter stayed
/// ```
///
/// So the returned resource names the WRITE node, and removing it leaves the read side
/// filtering. One node linked into both chains could not express that: removing it stopped
/// both, which is what elephc did.
///
/// A USER filter still mints one node for both chains. php instantiates the class once per
/// direction — `onCreate()` runs twice — and that second instance is not this function's to
/// make: it comes from the attach helper the caller ran before parking the object here.
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
    let one_node = ctx.next_label("sf_chain_one_node");
    let linked = ctx.next_label("sf_chain_linked");
    let has_params = params_inst.is_some();
    // Frame: [0]=stream handle [8]=requested direction bits [16]=the node
    //        [24]=params box or user-filter instance [32]=run-time built-in id
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x4");                               // recover the owning stream handle
            if user_object {
                abi::emit_pop_reg(ctx.emitter, "x5");                           // recover the php_user_filter instance
            }
            abi::emit_reserve_temporary_stack(ctx.emitter, 48);
            ctx.emitter.instruction("str x4, [sp, #0]");                        // preserve the stream handle across the calls
            ctx.emitter.instruction("str x0, [sp, #8]");                        // preserve the requested direction bits
            if user_object {
                ctx.emitter.instruction("str x5, [sp, #24]");                   // preserve the instance across the calls
            }
            if filter_id.is_none() && !user_object {
                ctx.emitter.instruction("str x9, [sp, #32]");                    // park the run-time id: the params call clobbers x9
            }
            if let Some(params_inst) = params_inst {
                materialize_stream_filter_params(ctx, params_inst)?;             // the boxed `$params` the node retains
                ctx.emitter.instruction("str x0, [sp, #24]");                    // a built-in node has this slot free
            }
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rcx");                              // recover the owning stream handle
            if user_object {
                abi::emit_pop_reg(ctx.emitter, "r14");                          // recover the php_user_filter instance
            }
            abi::emit_reserve_temporary_stack(ctx.emitter, 48);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rcx");            // preserve the stream handle across the calls
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // preserve the requested direction bits
            if user_object {
                ctx.emitter.instruction("mov QWORD PTR [rsp + 24], r14");       // preserve the instance across the calls
            }
            if filter_id.is_none() && !user_object {
                ctx.emitter.instruction("mov QWORD PTR [rsp + 32], r13");        // park the run-time id: the params call clobbers r13
            }
            if let Some(params_inst) = params_inst {
                materialize_stream_filter_params(ctx, params_inst)?;             // the boxed `$params` the node retains
                ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rax");        // a built-in node has this slot free
            }
        }
    }

    if !user_object {
        // -- both directions: one node each, read first, and the write one is the answer --
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x9, [sp, #8]");                    // requested direction bits
                ctx.emitter.instruction("and x9, x9, #3");                      // only the two chain bits decide
                ctx.emitter.instruction("cmp x9, #3");
                ctx.emitter.instruction(&format!("b.ne {}", one_node));         // a single direction takes the plain path
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");         // requested direction bits
                ctx.emitter.instruction("and r9, 3");                           // only the two chain bits decide
                ctx.emitter.instruction("cmp r9, 3");
                ctx.emitter.instruction(&format!("jne {}", one_node));          // a single direction takes the plain path
            }
        }
        if has_params {
            // Two nodes, two owners: the box is released once per node when the chain goes.
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("ldr x0, [sp, #24]");               // the retained params box
                    abi::emit_call_label(ctx.emitter, "__rt_incref");
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 24]");   // the retained params box
                    abi::emit_call_label(ctx.emitter, "__rt_incref");
                }
            }
        }
        emit_filter_node_create(ctx, filter_id, user_object, has_params, NodeDirection::Fixed(1));
        emit_filter_node_link(ctx, STREAM_READ_FILTER_HEAD_OFFSET, prepend);
        emit_filter_node_create(ctx, filter_id, user_object, has_params, NodeDirection::Fixed(2));
        emit_filter_node_link(ctx, STREAM_WRITE_FILTER_HEAD_OFFSET, prepend);
        abi::emit_jump(ctx.emitter, &linked);
        ctx.emitter.label(&one_node);
    }

    emit_filter_node_create(ctx, filter_id, user_object, has_params, NodeDirection::Requested);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // direction bits
            ctx.emitter.instruction("tst x9, #1");                              // is STREAM_FILTER_READ set?
            ctx.emitter.instruction(&format!("b.eq {}", skip_read));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");             // direction bits
            ctx.emitter.instruction("test r9, 1");                              // is STREAM_FILTER_READ set?
            ctx.emitter.instruction(&format!("jz {}", skip_read));
        }
    }
    emit_filter_node_link(ctx, STREAM_READ_FILTER_HEAD_OFFSET, prepend);
    ctx.emitter.label(&skip_read);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // direction bits
            ctx.emitter.instruction("tst x9, #2");                              // is STREAM_FILTER_WRITE set?
            ctx.emitter.instruction(&format!("b.eq {}", skip_write));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");             // direction bits
            ctx.emitter.instruction("test r9, 2");                              // is STREAM_FILTER_WRITE set?
            ctx.emitter.instruction(&format!("jz {}", skip_write));
        }
    }
    emit_filter_node_link(ctx, STREAM_WRITE_FILTER_HEAD_OFFSET, prepend);
    ctx.emitter.label(&skip_write);

    ctx.emitter.label(&linked);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // return the filter handle
            abi::emit_release_temporary_stack(ctx.emitter, 48);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // return the filter handle
            abi::emit_release_temporary_stack(ctx.emitter, 48);
        }
    }
    Ok(())
}

/// Emits one `__rt_filter_create` call, reading its arguments out of the attach frame.
///
/// The node it makes lands in [16], which is both the answer `stream_filter_append()` boxes and
/// the handle the link below needs — so a second create overwrites the first, and the LAST node
/// created is the one the caller receives. That is php's own order: it returns the filter it
/// created last, the write-side one.
fn emit_filter_node_create(
    ctx: &mut FunctionContext<'_>,
    filter_id: Option<u8>,
    user_object: bool,
    has_params: bool,
    direction: NodeDirection,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            match direction {
                NodeDirection::Requested => ctx.emitter.instruction("ldr x2, [sp, #8]"), // direction bits, past any params call
                NodeDirection::Fixed(bits) => ctx.emitter.instruction(&format!("mov x2, #{bits}")), // this node's own half
            }
            match filter_id {
                // A literal name resolves at compile time; a dynamic one was parked from x9.
                Some(id) => ctx.emitter.instruction(&format!("mov x0, #{id}")),  // built-in filter id
                None if user_object => ctx.emitter.instruction("mov x0, #0"),    // a user filter has no built-in id
                None => ctx.emitter.instruction("ldr x0, [sp, #32]"),            // run-time resolved filter id
            }
            if user_object {
                ctx.emitter.instruction("ldr x1, [sp, #24]");                   // the instance this node owns
            } else {
                ctx.emitter.instruction("mov x1, #0");                          // built-ins carry no user-filter object
            }
            if has_params {
                // php's built-in filters read `$params`; a user filter reads it off its instance.
                ctx.emitter.instruction("ldr x3, [sp, #24]");                   // the retained params box
            } else {
                ctx.emitter.instruction("mov x3, #0");                          // params live on the instance
            }
            abi::emit_call_label(ctx.emitter, "__rt_filter_create");            // x0 = the new filter handle
            ctx.emitter.instruction("str x0, [sp, #16]");                       // preserve the filter handle
        }
        Arch::X86_64 => {
            match direction {
                NodeDirection::Requested => {
                    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");    // direction bits, past any params call
                }
                NodeDirection::Fixed(bits) => {
                    ctx.emitter.instruction(&format!("mov rdx, {bits}"));       // this node's own half
                }
            }
            match filter_id {
                // A literal name resolves at compile time; a dynamic one was parked from r13.
                Some(id) => ctx.emitter.instruction(&format!("mov rdi, {id}")),  // built-in filter id
                None if user_object => ctx.emitter.instruction("xor edi, edi"),  // a user filter has no built-in id
                None => ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 32]"), // run-time resolved filter id
            }
            if user_object {
                ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");       // the instance this node owns
            } else {
                ctx.emitter.instruction("xor esi, esi");                        // built-ins carry no user-filter object
            }
            if has_params {
                // php's built-in filters read `$params`; a user filter reads it off its instance.
                ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 24]");       // the retained params box
            } else {
                ctx.emitter.instruction("xor ecx, ecx");                        // params live on the instance
            }
            abi::emit_call_label(ctx.emitter, "__rt_filter_create");            // rax = the new filter handle
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the filter handle
        }
    }
}

/// Links the node parked in [16] into one direction's chain.
fn emit_filter_node_link(ctx: &mut FunctionContext<'_>, head_offset: i64, prepend: bool) {
    let prepend_flag = i64::from(prepend);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // owning stream handle
            ctx.emitter.instruction("ldr x1, [sp, #16]");                       // filter handle
            ctx.emitter.instruction(&format!("mov x2, #{head_offset}"));
            ctx.emitter.instruction(&format!("mov x3, #{prepend_flag}"));       // prepend selects head insertion
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_link");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // owning stream handle
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");           // filter handle
            ctx.emitter.instruction(&format!("mov rdx, {head_offset}"));
            ctx.emitter.instruction(&format!("mov rcx, {prepend_flag}"));       // prepend selects head insertion
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_link");
        }
    }
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

/// Reports a filter name that produced no filter, the way php-src does.
///
/// The message names the filter, so it is composed at run time. Each function names
/// ITSELF in the prefix, chosen here, so the runtime composer needs no branch.
///
/// `registered` picks between php's two sentences: a name it cannot FIND is `Unable to locate
/// filter`, and a name it found but could not MAKE a filter from — `onCreate()` returned false —
/// is `Unable to create or locate filter`. MEASURED on `php -n` 8.5.6.
fn emit_missing_filter_warning(
    ctx: &mut FunctionContext<'_>,
    filter: ValueId,
    prepend: bool,
    registered: bool,
) -> Result<()> {
    let (symbol, text): (&str, &str) = match (prepend, registered) {
        (true, false) => (
            "_diag_filter_missing_prepend_prefix",
            "Warning: stream_filter_prepend(): Unable to locate filter \"",
        ),
        (false, false) => (
            "_diag_filter_missing_append_prefix",
            "Warning: stream_filter_append(): Unable to locate filter \"",
        ),
        (true, true) => (
            "_diag_filter_uncreatable_prepend_prefix",
            "Warning: stream_filter_prepend(): Unable to create or locate filter \"",
        ),
        (false, true) => (
            "_diag_filter_uncreatable_append_prefix",
            "Warning: stream_filter_append(): Unable to create or locate filter \"",
        ),
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
///
/// A mode naming BOTH directions instantiates the class TWICE, because php does: it creates
/// one filter per chain, each with its own `onCreate()`. MEASURED on `php -n` 8.5.6, a class
/// that announces itself prints `onCreate` twice for `stream_filter_append($h, "cc")`, one
/// `onClose` at `stream_filter_remove()` and the second at `fclose()`. A refused `onCreate`
/// stops at the FIRST: php prints one `onCreate`, warns, and answers `false` without trying
/// the other direction — which is why the second instantiation sits after the first has
/// succeeded rather than beside it.
///
/// Everything the two instantiations need is parked in one frame, laid out so the shared node
/// helpers can read it: [0] the stream, [8] the direction bits, [16] the node, [24] the
/// instance, then the name and the `$params` box the second call needs a reference of its own
/// to.
fn lower_user_stream_filter_attach_node(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    prepend: bool,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    let filter = expect_operand(inst, 1)?;
    let fail = ctx.next_label("sfan_false");
    let fail_unused_params = ctx.next_label("sfan_false_params");
    let uncreatable = ctx.next_label("sfan_uncreatable");
    let warned = ctx.next_label("sfan_warned");
    let first_made = ctx.next_label("sfan_read_made");
    let one_direction = ctx.next_label("sfan_one_direction");
    let linked = ctx.next_label("sfan_linked");
    let skip_read = ctx.next_label("sfan_skip_read");
    let skip_write = ctx.next_label("sfan_skip_write");
    let done = ctx.next_label("sfan_done");

    // Frame (64): [0]=stream handle [8]=direction bits [16]=the node [24]=the instance
    //             [32]=unused — the built-in id slot `emit_filter_node_create` reads for a
    //                  dynamic name, which a user node never has
    //             [40]=name pointer [48]=name length [56]=the boxed `$params`
    load_open_stream_handle_to_result(ctx, stream, "stream_filter_append")?;
    abi::emit_reserve_temporary_stack(ctx.emitter, 64);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("str x0, [sp, #0]"),           // the stream every call below names
        Arch::X86_64 => ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax"),
    }
    load_string_to_result(ctx, filter, "stream_filter_append filter")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x1, [sp, #40]");                       // filter-name pointer
            ctx.emitter.instruction("str x2, [sp, #48]");                       // filter-name length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 40], rax");           // filter-name pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 48], rdx");           // filter-name length
        }
    }
    materialize_stream_filter_mode(ctx, inst, Some(0))?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("str x0, [sp, #8]"),           // the directions this attach serves
        Arch::X86_64 => ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax"),
    }
    materialize_stream_filter_params(ctx, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("str x0, [sp, #56]"),          // the boxed `$params` an instance takes
        Arch::X86_64 => ctx.emitter.instruction("mov QWORD PTR [rsp + 56], rax"),
    }

    // -- both directions: two instances, two nodes, the read one first --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #8]");
            ctx.emitter.instruction("and x9, x9, #3");                          // only the two chain bits decide
            ctx.emitter.instruction("cmp x9, #3");
            ctx.emitter.instruction(&format!("b.ne {}", one_direction));
            ctx.emitter.instruction("ldr x0, [sp, #56]");                       // the second instance takes a
            abi::emit_call_label(ctx.emitter, "__rt_incref");                   // reference of its own to `$params`
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");
            ctx.emitter.instruction("and r9, 3");                               // only the two chain bits decide
            ctx.emitter.instruction("cmp r9, 3");
            ctx.emitter.instruction(&format!("jne {}", one_direction));
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 56]");           // the second instance takes a
            abi::emit_call_label(ctx.emitter, "__rt_incref");                   // reference of its own to `$params`
        }
    }
    emit_user_filter_instantiate(ctx, NodeDirection::Fixed(1));
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbnz x0, {}", first_made)),
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jnz {}", first_made));
        }
    }
    abi::emit_jump(ctx.emitter, &fail_unused_params);                           // the second instance will never exist
    ctx.emitter.label(&first_made);
    emit_park_user_filter_instance(ctx);
    emit_filter_node_create(ctx, None, true, false, NodeDirection::Fixed(1));
    emit_filter_node_link(ctx, STREAM_READ_FILTER_HEAD_OFFSET, prepend);
    emit_user_filter_instantiate(ctx, NodeDirection::Fixed(2));
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbz x0, {}", fail)),
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {}", fail));
        }
    }
    emit_park_user_filter_instance(ctx);
    emit_filter_node_create(ctx, None, true, false, NodeDirection::Fixed(2));
    emit_filter_node_link(ctx, STREAM_WRITE_FILTER_HEAD_OFFSET, prepend);
    abi::emit_jump(ctx.emitter, &linked);

    // -- one named direction: one instance, one node --
    ctx.emitter.label(&one_direction);
    emit_user_filter_instantiate(ctx, NodeDirection::Requested);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbz x0, {}", fail)), // unknown name or refused onCreate
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {}", fail));                   // unknown name or refused onCreate
        }
    }
    emit_park_user_filter_instance(ctx);
    emit_filter_node_create(ctx, None, true, false, NodeDirection::Requested);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #8]");
            ctx.emitter.instruction("tst x9, #1");                              // is STREAM_FILTER_READ set?
            ctx.emitter.instruction(&format!("b.eq {}", skip_read));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");
            ctx.emitter.instruction("test r9, 1");                              // is STREAM_FILTER_READ set?
            ctx.emitter.instruction(&format!("jz {}", skip_read));
        }
    }
    emit_filter_node_link(ctx, STREAM_READ_FILTER_HEAD_OFFSET, prepend);
    ctx.emitter.label(&skip_read);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #8]");
            ctx.emitter.instruction("tst x9, #2");                              // is STREAM_FILTER_WRITE set?
            ctx.emitter.instruction(&format!("b.eq {}", skip_write));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");
            ctx.emitter.instruction("test r9, 2");                              // is STREAM_FILTER_WRITE set?
            ctx.emitter.instruction(&format!("jz {}", skip_write));
        }
    }
    emit_filter_node_link(ctx, STREAM_WRITE_FILTER_HEAD_OFFSET, prepend);
    ctx.emitter.label(&skip_write);

    ctx.emitter.label(&linked);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("ldr x0, [sp, #16]"),          // the node the caller receives
        Arch::X86_64 => ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]"),
    }
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    emit_boxed_filter_handle(ctx);
    abi::emit_jump(ctx.emitter, &done);

    // The reference taken for an instance the failure means nobody will make.
    ctx.emitter.label(&fail_unused_params);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("ldr x0, [sp, #56]"),
        Arch::X86_64 => ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 56]"),
    }
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    ctx.emitter.label(&fail);
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    // -- the line php stamps on these warnings is THIS call's, not the last one that ran --
    //
    // The location is published before an instruction that may warn, and the user's `onCreate()`
    // ran in between: its own statements published THEIR lines, so a refused attach was reported
    // ` on line 4` — inside the filter class — where php names line 10, the call site. php stamps
    // a diagnostic with the frame that RAISED it, and returning from a php frame restores the
    // caller's. Publishing again here is that restore, for the one family that runs user code
    // between the publish and the warning.
    if let Some(span) = inst.span {
        crate::codegen::lower_inst::publish_diagnostic_line(ctx, span.line);
    }
    emit_missing_filter_class_warning(ctx, filter, prepend)?;
    // php-src names the filter it could not find. Returning false silently left a
    // misspelled name indistinguishable from one that attached.
    //
    // WHICH sentence depends on whether the name is registered: a registration that exists and
    // still produced no filter is php's `Unable to create or locate filter`, and only an unknown
    // name is `Unable to locate filter`. The attach helper answers 0 for both, so the question is
    // put again here — to the registry, which is the only thing that can tell them apart.
    load_string_to_result(ctx, filter, "stream_filter_append filter")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // filter-name pointer
            ctx.emitter.instruction("mov x1, x2");                              // filter-name length
            abi::emit_call_label(ctx.emitter, "__rt_resolve_user_filter_id");
            ctx.emitter.instruction(&format!("cbnz x0, {}", uncreatable));      // registered: php blames the creation
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // filter-name pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // filter-name length
            abi::emit_call_label(ctx.emitter, "__rt_resolve_user_filter_id");
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jnz {}", uncreatable));           // registered: php blames the creation
        }
    }
    emit_missing_filter_warning(ctx, filter, prepend, false)?;
    abi::emit_jump(ctx.emitter, &warned);
    ctx.emitter.label(&uncreatable);
    emit_missing_filter_warning(ctx, filter, prepend, true)?;
    ctx.emitter.label(&warned);
    emit_boxed_bool(ctx, false);
    ctx.emitter.label(&done);
    Ok(())
}

/// Reports a registration whose CLASS is not defined, the way php-src does — first.
///
/// MEASURED on `php -n` 8.5.6: an attach against a registration naming an undeclared class prints
/// TWO warnings, and the first is
/// `User-filter "p.one" requires class "MissingFamilyClass", but that class is not defined`.
/// It names the ATTACH name — `p.one`, not the registered pattern `p.*` — and the class the
/// registration named, which only the registry knows; `__rt_stream_filter_attach_user` publishes
/// it, leaving a null pointer for every other failure so this warning stays off those paths.
///
/// Five pieces, because two of them are run-time strings: `__rt_diag_warning` accumulates and
/// writes the line once a piece ends it with a newline.
fn emit_missing_filter_class_warning(
    ctx: &mut FunctionContext<'_>,
    filter: ValueId,
    prepend: bool,
) -> Result<()> {
    let head_text = if prepend {
        "Warning: stream_filter_prepend(): User-filter \""
    } else {
        "Warning: stream_filter_append(): User-filter \""
    };
    let (head, head_len) = ctx.data.add_string(head_text.as_bytes());
    let (middle, middle_len) = ctx.data.add_string(b"\" requires class \"");
    let (tail, tail_len) = ctx
        .data
        .add_string(b"\", but that class is not defined\n");
    let present = ctx.next_label("sfan_class_present");

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_sfau_class_ptr");
            ctx.emitter.instruction("ldr x9, [x9]");                            // the class the attach could not find
            ctx.emitter.instruction(&format!("cbz x9, {}", present));           // it found one: this warning is not due
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_sfau_class_ptr");
            ctx.emitter.instruction("mov r9, QWORD PTR [r9]");                  // the class the attach could not find
            ctx.emitter.instruction("test r9, r9");
            ctx.emitter.instruction(&format!("jz {}", present));                // it found one: this warning is not due
        }
    }

    emit_diag_piece_from_symbol(ctx, &head, head_len);
    load_string_to_result(ctx, filter, "stream_filter_append filter")?;
    match ctx.emitter.target.arch {
        // The name arrives where the composer wants it on AArch64, and needs one move on x86_64.
        Arch::AArch64 => {}
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");
            ctx.emitter.instruction("mov rsi, rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    emit_diag_piece_from_symbol(ctx, &middle, middle_len);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", "_sfau_class_ptr");
            ctx.emitter.instruction("ldr x1, [x1]");
            abi::emit_symbol_address(ctx.emitter, "x2", "_sfau_class_len");
            ctx.emitter.instruction("ldr x2, [x2]");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", "_sfau_class_ptr");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rdi]");
            abi::emit_symbol_address(ctx.emitter, "rsi", "_sfau_class_len");
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsi]");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    emit_diag_piece_from_symbol(ctx, &tail, tail_len);                          // the newline writes the line
    ctx.emitter.label(&present);
    Ok(())
}

/// Hands `__rt_diag_warning` one constant piece of a composed message.
fn emit_diag_piece_from_symbol(ctx: &mut FunctionContext<'_>, symbol: &str, len: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", symbol);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", symbol);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}

/// Instantiates the filter class for ONE direction, from the frame above.
///
/// Node mode — the negative descriptor — resolves the name, builds the instance, exposes
/// `$params` and `$filtername` on it and runs `onCreate()`, then hands the instance back
/// instead of registering it in the per-descriptor tables. Each call consumes one reference
/// to the `$params` box, which is why the two-instance path takes an extra one first.
fn emit_user_filter_instantiate(ctx: &mut FunctionContext<'_>, direction: NodeDirection) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #-1");                             // node mode: instantiate, register nothing
            ctx.emitter.instruction("ldr x1, [sp, #40]");                       // filter-name pointer
            ctx.emitter.instruction("ldr x2, [sp, #48]");                       // filter-name length
            match direction {
                NodeDirection::Requested => ctx.emitter.instruction("ldr x3, [sp, #8]"),
                NodeDirection::Fixed(bits) => ctx.emitter.instruction(&format!("mov x3, #{bits}")),
            }
            ctx.emitter.instruction("ldr x4, [sp, #56]");                       // the boxed `$params` it takes
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_attach_user");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, -1");                             // node mode: instantiate, register nothing
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 40]");           // filter-name pointer
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 48]");           // filter-name length
            match direction {
                NodeDirection::Requested => {
                    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 8]");
                }
                NodeDirection::Fixed(bits) => {
                    ctx.emitter.instruction(&format!("mov rcx, {bits}"));
                }
            }
            ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 56]");            // the boxed `$params` it takes
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_attach_user");
        }
    }
}

/// Parks the instance the attach helper just handed back where the node creator reads it.
fn emit_park_user_filter_instance(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("str x0, [sp, #24]"),
        Arch::X86_64 => ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rax"),
    }
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

