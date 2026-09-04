//! Purpose:
//! Stream resource unboxing, release sentinels, and TypeErrors.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Loads a resource or boxed resource handle into the target integer result register.
pub(in crate::codegen::lower_inst::builtins) fn load_stream_fd_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    function_name: &str,
) -> Result<()> {
    load_stream_fd_to_result_at(ctx, value, function_name, 0)
}

/// Same, for a stream parameter that is not the FIRST one.
pub(in crate::codegen::lower_inst::builtins) fn load_stream_fd_to_result_at(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    function_name: &str,
    param_index: usize,
) -> Result<()> {
    load_stream_handle_to_result_at(ctx, value, function_name, param_index)?;
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the opaque resource handle to the registry lookup helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_fd");
    let open_label = ctx.next_label("stream_resource_open");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // reject a stale, closed, or non-stream registry handle
            ctx.emitter.instruction(&format!("b.ge {}", open_label));           // continue only when the registry resolved an open backend fd
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // reject a stale, closed, or non-stream registry handle
            ctx.emitter.instruction(&format!("jns {}", open_label));            // continue only when the registry resolved an open backend fd
        }
    }
    emit_closed_stream_type_error(ctx, function_name);
    ctx.emitter.label(&open_label);
    Ok(())
}

/// Publishes the name php would print if this builtin ends up reading a wrapper that has no
/// `stream_eof`.
///
/// php asks a userspace wrapper, after EVERY `stream_read()`, whether that read reached the end —
/// the wrapper has no other way to say so. A class that does not implement `stream_eof` therefore
/// makes the READ ITSELF fail, and php names the function the user called. MEASURED on `php -n`
/// 8.5.6 against a wrapper with `stream_read` but no `stream_eof`, eleven callers, eleven names:
///
/// ```text
/// Warning: fread(): C::stream_eof is not implemented! Assuming EOF        → false
/// Warning: file_get_contents(): C::stream_eof is not implemented! ...     → ""
/// Warning: fpassthru(): C::stream_eof is not implemented! ...             → -1
/// Warning: file(): C::stream_eof is not implemented! ...                  → []
/// ```
///
/// …and the same for `fgets`, `fgetc`, `fgetcsv`, `stream_get_contents`, `stream_get_line`,
/// `readfile` and `fscanf`. Every failure shape is that one failed read travelling out through
/// each builtin's ORDINARY failure path, so the runtime needs the name and nothing else.
///
/// It is published HERE, in the one place every stream builtin already passes its own name
/// through, rather than at each reader: a reader added later cannot forget a step it does not
/// take, where a per-caller publication that was missed would silently name whichever builtin
/// published last. The single read of the slot runs only on the missing-method path.
pub(in crate::codegen::lower_inst::builtins) fn emit_publish_wrapper_read_caller(
    ctx: &mut FunctionContext<'_>,
    function_name: &str,
) {
    let (label, len) = ctx
        .data
        .add_string(format!("Warning: {function_name}(): ").as_bytes());
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, scratch, &label);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_uwmh_head", 0);
    abi::emit_load_int_immediate(ctx.emitter, scratch, len as i64);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_uwmh_head", 8);
}

/// Loads a generic resource payload without interpreting it as a stream-registry handle.
///
/// Internal resource-backed objects such as `HashContext` still store a native
/// pointer in a tag-9 Mixed payload. They must use this path instead of resolving
/// that payload through the stream registry, which would reject them as non-streams.
pub(in crate::codegen::lower_inst::builtins) fn load_resource_payload_to_result(
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
            "{} resource argument PHP type {:?}",
            function_name, other
        ))),
    }
}

/// Loads and validates an open stream while leaving its opaque handle in the result register.
pub(super) fn load_open_stream_handle_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    function_name: &str,
) -> Result<()> {
    load_stream_handle_to_result(ctx, value, function_name)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the opaque handle to descriptor validation
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_fd");
    let open_label = ctx.next_label("stream_handle_open");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // reject stale, closed, or non-stream handles
            ctx.emitter.instruction(&format!("b.ge {}", open_label));           // continue only when an open backend resolved
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // reject stale, closed, or non-stream handles
            ctx.emitter.instruction(&format!("jns {}", open_label));            // continue only when an open backend resolved
        }
    }
    emit_closed_stream_type_error(ctx, function_name);
    ctx.emitter.label(&open_label);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Starts an exact-once close and leaves the backend descriptor in the result register.
///
/// The opaque handle stays on a 16-byte temporary stack slot until
/// `finish_stream_close` publishes the final Closed state. Marking the entry
/// Closing happens before any filter, TLS, wrapper, or backend callback can
/// re-enter close.
pub(super) fn begin_stream_close(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    function_name: &str,
) -> Result<()> {
    load_stream_handle_to_result(ctx, value, function_name)?;
    begin_stream_close_from_result_handle(ctx, function_name);
    Ok(())
}

/// The half of `begin_stream_close` that runs once the opaque handle is already in the
/// integer result register.
///
/// `closedir()` with no argument sources its handle from `_last_dir_handle` rather than from an
/// operand, and must still take the exact-once Closing transition and the 16-byte stash
/// `finish_stream_close` reads back.
pub(super) fn begin_stream_close_from_result_handle(
    ctx: &mut FunctionContext<'_>,
    function_name: &str,
) {
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {}
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the opaque handle to the stream-state resolver
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_fd");
    let resolved_label = ctx.next_label("stream_close_resolved");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // reject stale, closed, and non-stream handles before close
            ctx.emitter.instruction(&format!("b.ge {}", resolved_label));       // continue only with a resolved backend descriptor
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // reject stale, closed, and non-stream handles before close
            ctx.emitter.instruction(&format!("jns {}", resolved_label));        // continue only with a resolved backend descriptor
        }
    }
    emit_closed_stream_type_error(ctx, function_name);
    ctx.emitter.label(&resolved_label);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the opaque handle while preserving the backend descriptor
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // reload the opaque handle while preserving the backend descriptor
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_resource_mark_closing");
    let marked_label = ctx.next_label("stream_close_marked");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", marked_label));     // only the first closer may run the backend destructor
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did this handle transition from Live to Closing?
            ctx.emitter.instruction(&format!("jnz {}", marked_label));          // only the first closer may run the backend destructor
        }
    }
    emit_closed_stream_type_error(ctx, function_name);
    ctx.emitter.label(&marked_label);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
}

/// Publishes Closed for the handle stashed by `begin_stream_close`, preserving the PHP result.
pub(super) fn finish_stream_close(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the opaque handle after backend cleanup completed
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // reload the opaque handle after backend cleanup completed
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_resource_mark_closed");
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Loads an opaque resource handle without exposing its backend descriptor.
///
/// Stream lifecycle operations use this form so close, retain, and metadata
/// lookup remain keyed by the registry generation rather than by a reusable OS
/// descriptor.
pub(in crate::codegen::lower_inst::builtins) fn load_stream_handle_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    function_name: &str,
) -> Result<()> {
    load_stream_handle_to_result_at(ctx, value, function_name, 0)
}

/// Same, for a stream parameter that is not the FIRST one.
///
/// php numbers the argument and names it in the TypeError below, and `stream_copy_to_stream()`
/// is the one builtin in this family whose second parameter is also a stream.
pub(in crate::codegen::lower_inst::builtins) fn load_stream_handle_to_result_at(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    function_name: &str,
    param_index: usize,
) -> Result<()> {
    emit_publish_wrapper_read_caller(ctx, function_name);
    let raw_ty = ctx.raw_value_php_type(value)?;
    ctx.load_value_to_result(value)?;
    match raw_ty {
        PhpType::Resource(_) => Ok(()),
        PhpType::Mixed | PhpType::Union(_) => {
            emit_unbox_stream_or_type_error(ctx, function_name);
            Ok(())
        }
        // A statically NULL argument is php's catchable TypeError, raised when the call runs.
        // The checker used to refuse the program for it, so an undefined `$h` reaching
        // `fgetc($h)` failed the whole file where php runs it and reports one error.
        PhpType::Void => {
            emit_null_stream_type_error(ctx, function_name, param_index);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} stream argument PHP type {:?}",
            function_name, other
        ))),
    }
}

/// Emits php's `TypeError` for a null where a stream resource is required.
///
/// NEVER RETURNS: the throw leaves through `__rt_throw_current`, so the caller's own code after
/// it is the not-taken path. Catchable, like every other argument TypeError in this family.
fn emit_null_stream_type_error(
    ctx: &mut FunctionContext<'_>,
    function_name: &str,
    param_index: usize,
) {
    let message = format!(
        "{}(): Argument #{} (${}) must be of type resource, null given",
        function_name,
        param_index + 1,
        parameter_name_at(function_name, param_index)
    );
    super::super::exceptions::emit_type_error(ctx, &message);
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
// Production close moved to the registry (`begin_stream_close` /
// `finish_stream_close`), which keeps the entry and its id instead of stamping
// the Mixed box, so nothing emits this stamp any more. The emitter and its
// assembly tests are kept as the pinned reference for the identity guarantee
// they document until an equivalent registry-identity test exists.
#[cfg(test)]
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
            ctx.emitter.instruction("mov x0, x1");                              // expose the opaque registry handle as the integer result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // expose the opaque registry handle as the integer result
        }
    }
}

/// Emits the PHP diagnostic used when a resource handle no longer resolves to an open stream.
///
/// `stream_filter_remove()` words this one differently, and it is not a variation on the same
/// sentence: php says `supplied resource is not a valid stream filter resource` for EVERY resource
/// it will not accept — a live stream, a closed stream, or a filter already removed. The
/// argument-name form is reserved there for a value that is not a resource at all, which
/// `emit_stream_type_error_case` already spells with the catalog's `$stream_filter`. Measured on
/// `php -n` 8.5.6.
pub(super) fn emit_closed_stream_type_error(ctx: &mut FunctionContext<'_>, function_name: &str) {
    let message = if function_name == "stream_filter_remove" {
        "stream_filter_remove(): supplied resource is not a valid stream filter resource".to_string()
    } else {
        // php names the ACTUAL parameter here too, and the directory family calls it
        // `$dir_handle`: MEASURED, `closedir()`, `readdir()` and `rewinddir()` on a closed handle
        // all say `Argument #1 ($dir_handle)`. `$stream` was hard-coded, so the one place that
        // already reads the name from the shared contract — `emit_stream_type_error_case`, right
        // below — and this one disagreed about the same function.
        format!(
            "{}(): Argument #1 (${}) must be an open stream resource",
            function_name,
            first_parameter_name(function_name)
        )
    };
    // The shared emitter, not a second copy of it. This used to hand-roll the same allocation,
    // class id, message pair, code and `__rt_throw_current` jump — and the one thing it left out
    // was the creation line, so php reported ` in FILE:LINE` for this TypeError and elephc
    // reported nothing. A duplicate of the throw machinery could only ever drift from it.
    super::super::exceptions::emit_type_error(ctx, &message);
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

/// Names argument #1 the way php-src's own `TypeError` spells it for this builtin.
///
/// php reads the wording out of the function's stub, so the spelling is NOT always
/// `$stream`. MEASURED on `php -n` 8.5.6:
///
/// ```text
/// stream_context_get_options("x") ... Argument #1 ($stream_or_context) ...
/// stream_context_get_params("x")  ... Argument #1 ($context) ...
/// stream_socket_recvfrom("x", 5)  ... Argument #1 ($socket) ...
/// stream_filter_remove("x")       ... Argument #1 ($stream_filter) ...
/// stream_copy_to_stream("x", "y") ... Argument #1 ($from) ...
/// fread("x", 5)                   ... Argument #1 ($stream) ...
/// ```
///
/// The shared builtin contract already carries that name, so it is read from there rather
/// than duplicated in a codegen table that would drift away from the catalog. Builtins with
/// no contract entry (internal lowering helpers) keep php's most common spelling.
fn first_parameter_name(function_name: &str) -> &'static str {
    parameter_name_at(function_name, 0)
}

/// Returns the name php gives one parameter of a builtin, from the shared contract.
///
/// The contract is the only place that knows `closedir()` calls its handle `$dir_handle` and
/// `fgetc()` calls its own `$stream`; hard-coding either produced a message php never prints.
fn parameter_name_at(function_name: &str, index: usize) -> &'static str {
    crate::builtins::registry::lookup(function_name)
        .and_then(|def| def.spec.params.get(index))
        .map_or("stream", |param| param.name)
}

/// Emits one concrete stream `TypeError` branch as a CATCHABLE throw.
///
/// php raises a real `TypeError` here, so `catch (TypeError $e)` observes the message and an
/// unhandled one leaves the interpreter with status 255. This used to write the diagnostic and
/// `exit(1)` directly, which no `try` block could intercept and which reported the wrong status.
pub(super) fn emit_stream_type_error_case(
    ctx: &mut FunctionContext<'_>,
    function_name: &str,
    given_type: &str,
    case_label: &str,
) {
    ctx.emitter.label(case_label);
    let message = format!(
        "{}(): Argument #1 (${}) must be of type resource, {} given",
        function_name,
        first_parameter_name(function_name),
        given_type
    );
    super::super::exceptions::emit_type_error(ctx, &message);
}

