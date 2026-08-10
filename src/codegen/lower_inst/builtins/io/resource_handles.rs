//! Purpose:
//! Stream resource unboxing, release sentinels, and TypeErrors.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Loads a path string, calls a stat helper, boxes int success or PHP false, and stores it.
pub(super) fn lower_unary_path_stat_int_or_false(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    box_stat_int_or_false_result(ctx);
    store_if_result(ctx, inst)
}

/// Loads a resource or boxed resource handle into the target integer result register.
pub(in crate::codegen::lower_inst::builtins) fn load_stream_fd_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    function_name: &str,
) -> Result<()> {
    let raw_ty = ctx.raw_value_php_type(value)?;
    ctx.load_value_to_result(value)?;
    match raw_ty {
        PhpType::Resource(_) => Ok(()),
        PhpType::Mixed | PhpType::Union(_) => {
            emit_unbox_stream_or_type_error(ctx, function_name);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} stream argument PHP type {:?}",
            function_name, other
        ))),
    }
}

/// Stashes the Mixed box pointer of a resource operand on the stack so an
/// explicit closer (`fclose`/`pclose`/`closedir`) can stamp a release sentinel
/// into it after the handle is unboxed.
///
/// Returns `true` when a box was captured (Mixed/Union-typed operands, which are
/// the only ones that participate in scope cleanup) and `false` for unboxed
/// `Resource`-typed handles, which have no Mixed cell. The push keeps the stack
/// 16-byte aligned across the `__rt_mixed_unbox` call performed during unboxing;
/// the matching pop lives in `apply_resource_release_sentinel`.
pub(super) fn capture_resource_box_for_release(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    let raw_ty = ctx.raw_value_php_type(value)?;
    if !matches!(raw_ty, PhpType::Mixed | PhpType::Union(_)) {
        return Ok(false);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(value, "x9")?;
            ctx.emitter.instruction("str x9, [sp, #-16]!");                     // stash the resource Mixed box pointer across the unbox call
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(value, "r11")?;
            ctx.emitter.instruction("sub rsp, 16");                             // reserve a 16-byte aligned slot for the stashed box pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp], r11");                // stash the resource Mixed box pointer across the unbox call
        }
    }
    Ok(true)
}

/// Pops the stashed Mixed box pointer and writes a NEGATIVE release sentinel into
/// its low payload word so scope cleanup (`__rt_mixed_free_deep`) skips the
/// already-closed handle — preventing a second `close`/`pclose`/`closedir` on a
/// descriptor whose number may have been reused. A no-op when nothing was
/// captured. Preserves the native handle already in the int result register, which
/// the caller still needs for its own close dispatch.
///
/// The sentinel is `-id`, not a bare `-1`, and that is what keeps PHP's display
/// identity intact: php-src leaves `zend_resource.handle` untouched when a resource
/// is closed, so `fclose($r); echo "$r";` still prints `Resource id #5` and
/// `get_resource_id($r)` still answers 5 under 8.5.6. Stamping a bare `-1` erased
/// the only key the resource-id registry had, so every later display path missed the
/// table, minted a FRESH id, and printed `Resource id #6` while also stealing an id
/// from the next `fopen()`. Encoding the id in the sentinel lets
/// `__rt_resource_id_of` answer negative payloads directly (see
/// `runtime::resource_ids`) without a table lookup and without a mint.
///
/// Every existing consumer of the sentinel is unaffected: the three
/// `__rt_mixed_free_deep` resource arms gate on the UNSIGNED threshold
/// `0x40000000` (`b.hs` / `jae`), and every negative payload is unsigned-huge, so a
/// `-id` sentinel is skipped exactly like `-1` was. No native payload can collide
/// with it either: descriptors are small positives, `DIR*`/`FILE*`/HashContext
/// handles are userspace addresses with bit 63 clear, the synthetic wrapper and PHAR
/// bases are `0x40000000`/`0x50000000`, and `EVAL_RESOURCE_PAYLOAD_BASE` is `1 << 62`.
pub(super) fn apply_resource_release_sentinel(ctx: &mut FunctionContext<'_>, captured: bool) {
    if !captured {
        return;
    }
    emit_resource_release_sentinel(ctx.emitter);
}

/// Emits the sentinel stamp itself, split out of `apply_resource_release_sentinel` so
/// both target variants can be pinned at assembly level without a `FunctionContext`.
///
/// Entry state: the native handle is in the int result register (`x0` / `rax`) and the
/// resource's Mixed box pointer is on top of the stack, where
/// `capture_resource_box_for_release` pushed it. Exit state: the box's low payload word
/// holds `-id`, the stash slot is released, and the int result register still holds the
/// native handle. `__rt_resource_id_of` preserves every register it touches on both
/// targets (AArch64 saves `x9`–`x14`, x86_64 pushes `rcx`, `rdx`, `rsi`, `r8`–`r11`), so
/// the box pointer and the saved handle survive the call in `x9`/`x11` and `r11`/`r10`.
pub(super) fn emit_resource_release_sentinel(emitter: &mut crate::codegen::emit::Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp], #16");                           // restore the stashed resource Mixed box pointer
            emitter.instruction("mov x11, x0");                                 // preserve the native handle the caller still has to close
            abi::emit_call_label(emitter, "__rt_resource_id_of");               // resolve the id this handle keeps for the rest of the request
            emitter.instruction("neg x10, x0");                                 // a negative payload encodes "closed, PHP id = -payload"
            emitter.instruction("str x10, [x9, #8]");                           // overwrite the low payload word so scope cleanup skips it
            emitter.instruction("mov x0, x11");                                 // restore the native handle for the caller's close dispatch
        }
        Arch::X86_64 => {
            emitter.instruction("mov r11, QWORD PTR [rsp]");                    // restore the stashed resource Mixed box pointer
            emitter.instruction("add rsp, 16");                                 // release the stash slot
            emitter.instruction("mov r10, rax");                                // preserve the native handle the caller still has to close
            abi::emit_call_label(emitter, "__rt_resource_id_of");               // resolve the id this handle keeps for the rest of the request
            emitter.instruction("neg rax");                                     // a negative payload encodes "closed, PHP id = -payload"
            emitter.instruction("mov QWORD PTR [r11 + 8], rax");                // overwrite the low payload word so scope cleanup skips it
            emitter.instruction("mov rax, r10");                                // restore the native handle for the caller's close dispatch
        }
    }
}

/// Unboxes a Mixed stream resource or emits a fatal TypeError for non-resource values.
pub(super) fn emit_unbox_stream_or_type_error(ctx: &mut FunctionContext<'_>, function_name: &str) {
    let ok_label = ctx.next_label("stream_resource_ok");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #9");                              // check whether the boxed stream value uses the resource tag
            ctx.emitter.instruction(&format!("b.eq {}", ok_label));             // continue only when the boxed value is a resource
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 9");                              // check whether the boxed stream value uses the resource tag
            ctx.emitter.instruction(&format!("je {}", ok_label));               // continue only when the boxed value is a resource
        }
    }
    emit_stream_type_error(ctx, function_name);
    ctx.emitter.label(&ok_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // expose the unboxed native stream fd as the integer result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // expose the unboxed native stream fd as the integer result
        }
    }
}

/// Dispatches a stream TypeError to the concrete PHP type name from the Mixed tag.
pub(super) fn emit_stream_type_error(ctx: &mut FunctionContext<'_>, function_name: &str) {
    let int_label = ctx.next_label("stream_type_error_int");
    let string_label = ctx.next_label("stream_type_error_string");
    let float_label = ctx.next_label("stream_type_error_float");
    let bool_label = ctx.next_label("stream_type_error_bool");
    let false_label = ctx.next_label("stream_type_error_false");
    let true_label = ctx.next_label("stream_type_error_true");
    let array_label = ctx.next_label("stream_type_error_array");
    let object_label = ctx.next_label("stream_type_error_object");
    let null_label = ctx.next_label("stream_type_error_null");
    let unknown_label = ctx.next_label("stream_type_error_unknown");

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // did the bad stream value unwrap to an integer?
            ctx.emitter.instruction(&format!("b.eq {}", int_label));            // report PHP's int-given stream TypeError
            ctx.emitter.instruction("cmp x0, #1");                              // did the bad stream value unwrap to a string?
            ctx.emitter.instruction(&format!("b.eq {}", string_label));         // report PHP's string-given stream TypeError
            ctx.emitter.instruction("cmp x0, #2");                              // did the bad stream value unwrap to a float?
            ctx.emitter.instruction(&format!("b.eq {}", float_label));          // report PHP's float-given stream TypeError
            ctx.emitter.instruction("cmp x0, #3");                              // did the bad stream value unwrap to a boolean?
            ctx.emitter.instruction(&format!("b.eq {}", bool_label));           // split boolean payloads into true/false diagnostics
            ctx.emitter.instruction("cmp x0, #4");                              // did the bad stream value unwrap to an indexed array?
            ctx.emitter.instruction(&format!("b.eq {}", array_label));          // report PHP's array-given stream TypeError
            ctx.emitter.instruction("cmp x0, #5");                              // did the bad stream value unwrap to an associative array?
            ctx.emitter.instruction(&format!("b.eq {}", array_label));          // associative arrays share PHP's array-given wording
            ctx.emitter.instruction("cmp x0, #6");                              // did the bad stream value unwrap to an object?
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // report PHP's object-given stream TypeError
            ctx.emitter.instruction("cmp x0, #8");                              // did the bad stream value unwrap to null?
            ctx.emitter.instruction(&format!("b.eq {}", null_label));           // report PHP's null-given stream TypeError
            ctx.emitter.instruction(&format!("b {}", unknown_label));           // fall back for unsupported boxed payload tags
            ctx.emitter.label(&bool_label);
            ctx.emitter.instruction("cmp x1, #0");                              // is the unboxed boolean payload false?
            ctx.emitter.instruction(&format!("b.eq {}", false_label));          // report PHP's false-given stream TypeError
            ctx.emitter.instruction(&format!("b {}", true_label));              // report PHP's true-given stream TypeError
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // did the bad stream value unwrap to an integer?
            ctx.emitter.instruction(&format!("je {}", int_label));              // report PHP's int-given stream TypeError
            ctx.emitter.instruction("cmp rax, 1");                              // did the bad stream value unwrap to a string?
            ctx.emitter.instruction(&format!("je {}", string_label));           // report PHP's string-given stream TypeError
            ctx.emitter.instruction("cmp rax, 2");                              // did the bad stream value unwrap to a float?
            ctx.emitter.instruction(&format!("je {}", float_label));            // report PHP's float-given stream TypeError
            ctx.emitter.instruction("cmp rax, 3");                              // did the bad stream value unwrap to a boolean?
            ctx.emitter.instruction(&format!("je {}", bool_label));             // split boolean payloads into true/false diagnostics
            ctx.emitter.instruction("cmp rax, 4");                              // did the bad stream value unwrap to an indexed array?
            ctx.emitter.instruction(&format!("je {}", array_label));            // report PHP's array-given stream TypeError
            ctx.emitter.instruction("cmp rax, 5");                              // did the bad stream value unwrap to an associative array?
            ctx.emitter.instruction(&format!("je {}", array_label));            // associative arrays share PHP's array-given wording
            ctx.emitter.instruction("cmp rax, 6");                              // did the bad stream value unwrap to an object?
            ctx.emitter.instruction(&format!("je {}", object_label));           // report PHP's object-given stream TypeError
            ctx.emitter.instruction("cmp rax, 8");                              // did the bad stream value unwrap to null?
            ctx.emitter.instruction(&format!("je {}", null_label));             // report PHP's null-given stream TypeError
            ctx.emitter.instruction(&format!("jmp {}", unknown_label));         // fall back for unsupported boxed payload tags
            ctx.emitter.label(&bool_label);
            ctx.emitter.instruction("test rdi, rdi");                           // is the unboxed boolean payload false?
            ctx.emitter.instruction(&format!("je {}", false_label));            // report PHP's false-given stream TypeError
            ctx.emitter.instruction(&format!("jmp {}", true_label));            // report PHP's true-given stream TypeError
        }
    }

    emit_stream_type_error_case(ctx, function_name, "int", &int_label);
    emit_stream_type_error_case(ctx, function_name, "string", &string_label);
    emit_stream_type_error_case(ctx, function_name, "float", &float_label);
    emit_stream_type_error_case(ctx, function_name, "false", &false_label);
    emit_stream_type_error_case(ctx, function_name, "true", &true_label);
    emit_stream_type_error_case(ctx, function_name, "array", &array_label);
    emit_stream_type_error_case(ctx, function_name, "object", &object_label);
    emit_stream_type_error_case(ctx, function_name, "null", &null_label);
    emit_stream_type_error_case(ctx, function_name, "unknown", &unknown_label);
}

/// Emits one concrete stream TypeError branch and terminates the process.
pub(super) fn emit_stream_type_error_case(
    ctx: &mut FunctionContext<'_>,
    function_name: &str,
    given_type: &str,
    case_label: &str,
) {
    ctx.emitter.label(case_label);
    let message = format!(
        "Fatal error: Uncaught TypeError: {}(): Argument #1 ($stream) must be of type resource, {} given\n",
        function_name, given_type
    );
    let (label, len) = ctx.data.add_string(message.as_bytes());
    emit_stream_type_error_and_exit(ctx, &label, len);
}

/// Emits a fatal stream TypeError diagnostic and terminates with exit status 1.
pub(super) fn emit_stream_type_error_and_exit(ctx: &mut FunctionContext<'_>, label: &str, len: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the stream TypeError diagnostic to stderr
            ctx.emitter.adrp("x1", label);                                      // load the diagnostic string page
            ctx.emitter.add_lo12("x1", "x1", label);                            // resolve the diagnostic string address within the page
            ctx.emitter.instruction(&format!("mov x2, #{}", len));              // pass the diagnostic byte length to write()
            ctx.emitter.syscall(4);
            ctx.emitter.instruction("mov x0, #1");                              // exit with status 1 after reporting the TypeError
            ctx.emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rsi", label);
            ctx.emitter.instruction(&format!("mov edx, {}", len));              // pass the diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // write the stream TypeError diagnostic to stderr
            ctx.emitter.instruction("mov eax, 1");                              // select Linux x86_64 write syscall
            ctx.emitter.instruction("syscall");                                 // emit the stream TypeError diagnostic
            ctx.emitter.instruction("mov edi, 1");                              // exit with status 1 after reporting the TypeError
            ctx.emitter.instruction("mov eax, 231");                            // select Linux x86_64 exit_group syscall
            ctx.emitter.instruction("syscall");                                 // terminate the process after the fatal TypeError
        }
    }
}
