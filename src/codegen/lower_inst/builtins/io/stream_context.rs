//! Purpose:
//! Stream wrapper registration, context state, and stream content reads.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `stream_wrapper_register(protocol, class, flags?)`.
pub(crate) fn lower_stream_wrapper_register(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_wrapper_register", 2, 3)?;
    let protocol = expect_operand(inst, 0)?;
    let class = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, protocol, "stream_wrapper_register protocol")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, class, "stream_wrapper_register class")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            if let Some(flags) = inst.operands.get(2).copied() {
                require_int(
                    ctx.load_value_to_result(flags)?.codegen_repr(),
                    "stream_wrapper_register flags",
                )?;
                ctx.emitter.instruction("mov x4, x0");                          // pass registration flags as the fifth runtime argument
            } else {
                ctx.emitter.instruction("mov x4, #0");                          // omitted registration flags default to zero
            }
            abi::emit_pop_reg_pair(ctx.emitter, "x2", "x3");
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, protocol, "stream_wrapper_register protocol")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, class, "stream_wrapper_register class")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            if let Some(flags) = inst.operands.get(2).copied() {
                require_int(
                    ctx.load_value_to_result(flags)?.codegen_repr(),
                    "stream_wrapper_register flags",
                )?;
                ctx.emitter.instruction("mov r8, rax");                         // pass registration flags as the fifth runtime argument
            } else {
                ctx.emitter.instruction("xor r8d, r8d");                        // omitted registration flags default to zero
            }
            abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_register");
    store_if_result(ctx, inst)
}

/// Lowers `stream_wrapper_unregister(protocol)`.
pub(crate) fn lower_stream_wrapper_unregister(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_wrapper_unregister", 1)?;
    let protocol = expect_operand(inst, 0)?;
    load_string_to_result(ctx, protocol, "stream_wrapper_unregister protocol")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the protocol pointer as the first runtime argument
            ctx.emitter.instruction("mov x1, x2");                              // pass the protocol byte length as the second runtime argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the protocol pointer as the first runtime argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the protocol byte length as the second runtime argument
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_unregister");
    store_if_result(ctx, inst)
}

/// Lowers `stream_wrapper_restore(protocol)` by clearing its disabled bit.
///
/// PHP answers all three cases differently, and elephc now matches each:
/// a built-in that `stream_wrapper_unregister()` disabled is restored silently and reports
/// `true`; a built-in that was never disabled reports `true` with a Notice; a scheme that
/// never existed reports `false` with a Warning. Only the two diagnostics were missing —
/// the return values already matched.
///
/// The scheme bytes are spilled once around the whole body: `__rt_builtin_wrapper_index`
/// clobbers them, and every one of the three exits needs them or the matching release, so a
/// single reserve/release pair keeps the stack balanced whichever way the call goes.
pub(crate) fn lower_stream_wrapper_restore(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_wrapper_restore", 1)?;
    let protocol = expect_operand(inst, 0)?;
    load_string_to_result(ctx, protocol, "stream_wrapper_restore protocol")?;
    let restored = ctx.next_label("swr_restored");
    let unchanged = ctx.next_label("swr_unchanged");
    let done = ctx.next_label("swr_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x1, [sp, #0]");                        // keep the scheme pointer for the diagnostics
            ctx.emitter.instruction("str x2, [sp, #8]");                        // keep the scheme length for the diagnostics
            ctx.emitter.instruction("mov x0, x1");                              // protocol pointer
            ctx.emitter.instruction("mov x1, x2");                              // protocol length
            abi::emit_call_label(ctx.emitter, "__rt_builtin_wrapper_index");
            ctx.emitter.instruction("cmp x0, #0");
            ctx.emitter.instruction(&format!("b.ge {}", restored));             // a built-in name can be restored
            ctx.emitter.instruction("mov x0, #1");                              // diagnostic kind 1 = warning
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // scheme pointer
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // scheme length
            abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_restore_diag");
            ctx.emitter.instruction("mov x0, #0");                              // unknown scheme reports false
            ctx.emitter.instruction(&format!("b {}", done));
            ctx.emitter.label(&restored);
            abi::emit_symbol_address(ctx.emitter, "x9", "_disabled_builtin_wrappers");
            ctx.emitter.instruction("ldr x10, [x9]");                           // current disabled mask
            ctx.emitter.instruction("mov x11, #1");
            ctx.emitter.instruction("lsl x11, x11, x0");                        // bit for this wrapper
            ctx.emitter.instruction("tst x10, x11");                            // was it actually unregistered?
            ctx.emitter.instruction(&format!("b.eq {}", unchanged));            // never changed: PHP emits a Notice
            ctx.emitter.instruction("bic x10, x10, x11");                       // clear it: the built-in is available again
            ctx.emitter.instruction("str x10, [x9]");
            ctx.emitter.instruction("mov x0, #1");                              // report success
            ctx.emitter.instruction(&format!("b {}", done));
            ctx.emitter.label(&unchanged);
            ctx.emitter.instruction("mov x0, #0");                              // diagnostic kind 0 = notice
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // scheme pointer
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // scheme length
            abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_restore_diag");
            ctx.emitter.instruction("mov x0, #1");                              // PHP still reports success here
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // keep the scheme pointer for the diagnostics
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // keep the scheme length for the diagnostics
            ctx.emitter.instruction("mov rdi, rax");                            // protocol pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // protocol length
            abi::emit_call_label(ctx.emitter, "__rt_builtin_wrapper_index");
            ctx.emitter.instruction("cmp rax, 0");
            ctx.emitter.instruction(&format!("jge {}", restored));              // a built-in name can be restored
            ctx.emitter.instruction("mov rdi, 1");                              // diagnostic kind 1 = warning
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");            // scheme pointer
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // scheme length
            abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_restore_diag");
            ctx.emitter.instruction("xor eax, eax");                            // unknown scheme reports false
            ctx.emitter.instruction(&format!("jmp {}", done));
            ctx.emitter.label(&restored);
            ctx.emitter.instruction("mov rcx, rax");                            // built-in index
            abi::emit_symbol_address(ctx.emitter, "r9", "_disabled_builtin_wrappers");
            ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                 // current disabled mask
            ctx.emitter.instruction("mov r11, 1");
            ctx.emitter.instruction("shl r11, cl");                             // bit for this wrapper
            ctx.emitter.instruction("test r10, r11");                           // was it actually unregistered?
            ctx.emitter.instruction(&format!("jz {}", unchanged));              // never changed: PHP emits a Notice
            ctx.emitter.instruction("not r11");
            ctx.emitter.instruction("and r10, r11");                            // clear it: the built-in is available again
            ctx.emitter.instruction("mov QWORD PTR [r9], r10");
            ctx.emitter.instruction("mov eax, 1");                              // report success
            ctx.emitter.instruction(&format!("jmp {}", done));
            ctx.emitter.label(&unchanged);
            ctx.emitter.instruction("mov rdi, 0");                              // diagnostic kind 0 = notice
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");            // scheme pointer
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // scheme length
            abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_restore_diag");
            ctx.emitter.instruction("mov eax, 1");                              // PHP still reports success here
            ctx.emitter.label(&done);
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_create(options?, params?)` into a dynamic ContextState resource.
pub(crate) fn lower_stream_context_create(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_context_create", 0, 2)?;
    if let Some(options) = inst.operands.first().copied() {
        store_stream_context_options(ctx, options, true)?;
    } else {
        clear_stream_context_options(ctx);
    }
    clear_stream_notification_callback(ctx);
    if let Some(params) = inst.operands.get(1).copied() {
        capture_stream_notification_callback(ctx, params)?;
        merge_stream_context_params_options_into_scratch(ctx, params)?;
    }
    emit_dynamic_stream_context_allocation(ctx, "stream_context_create");
    store_if_result(ctx, inst)
}

/// Allocates a 32-byte ContextState and boxes its generation-safe registry handle.
///
/// The construction globals are detached into the state before publishing the
/// handle. A failed state or registry allocation releases every acquired child.
pub(super) fn emit_dynamic_stream_context_allocation(
    ctx: &mut FunctionContext<'_>,
    _label_prefix: &str,
) {
    let state_alloc_failed = ctx.next_label("sctx_state_alloc_failed");
    let registry_alloc_failed = ctx.next_label("sctx_registry_alloc_failed");
    let done_label = ctx.next_label("sctx_alloc_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x10, [x9]");                           // detach the retained options hash from construction scratch
            ctx.emitter.instruction("str xzr, [x9]");                           // the new ContextState takes ownership of the options reference
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_notification_callback");
            ctx.emitter.instruction("ldr x11, [x9]");                           // detach the retained notification descriptor
            ctx.emitter.instruction("str xzr, [x9]");                           // the new ContextState takes ownership of the notifier
            abi::emit_push_reg_pair(ctx.emitter, "x10", "x11");
            ctx.emitter.instruction("mov x0, #32");                             // ContextState stores options, params, notifier, and flags
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!("cbz x0, {}", state_alloc_failed)); // release detached children when state allocation fails
            ctx.emitter.instruction("ldr x9, [sp]");                            // reload the retained options hash
            ctx.emitter.instruction("str x9, [x0]");                            // ContextState.options = detached options
            ctx.emitter.instruction("str xzr, [x0, #8]");                       // ContextState.params starts empty
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // reload the retained notifier descriptor
            ctx.emitter.instruction("str x9, [x0, #16]");                       // ContextState.notifier = detached notifier
            ctx.emitter.instruction("str xzr, [x0, #24]");                      // ContextState.flags starts clear
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x1, x0");                              // pass ContextState as registry state
            ctx.emitter.instruction("mov x0, #2");                              // registry resource kind 2 = Context
            ctx.emitter.instruction("mov x2, #1");                              // context is request-owned
            abi::emit_call_label(ctx.emitter, "__rt_resource_alloc");
            ctx.emitter.instruction(&format!("cbz x0, {}", registry_alloc_failed)); // unwind ContextState when registry growth fails
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip construction-error cleanup after success
            ctx.emitter.label(&registry_alloc_failed);
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_heap_free");
            release_detached_context_children_aarch64(ctx);
            emit_fd_result(ctx, 0);
            ctx.emitter.instruction(&format!("b {}", done_label));              // share the false result after registry failure
            ctx.emitter.label(&state_alloc_failed);
            release_detached_context_children_aarch64(ctx);
            emit_fd_result(ctx, 0);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                 // detach the retained options hash from construction scratch
            ctx.emitter.instruction("mov QWORD PTR [r9], 0");                   // the new ContextState takes ownership of the options reference
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_notification_callback");
            ctx.emitter.instruction("mov r11, QWORD PTR [r9]");                 // detach the retained notification descriptor
            ctx.emitter.instruction("mov QWORD PTR [r9], 0");                   // the new ContextState takes ownership of the notifier
            abi::emit_push_reg_pair(ctx.emitter, "r10", "r11");
            ctx.emitter.instruction("mov eax, 32");                             // ContextState stores options, params, notifier, and flags
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("test rax, rax");                           // did libc allocate ContextState?
            ctx.emitter.instruction(&format!("jz {}", state_alloc_failed));     // release detached children on allocation failure
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp]");                 // reload the retained options hash
            ctx.emitter.instruction("mov QWORD PTR [rax], r9");                 // ContextState.options = detached options
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], 0");              // ContextState.params starts empty
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");             // reload the retained notifier descriptor
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], r9");            // ContextState.notifier = detached notifier
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");             // ContextState.flags starts clear
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rsi, rax");                            // pass ContextState as registry state
            ctx.emitter.instruction("mov edi, 2");                              // registry resource kind 2 = Context
            ctx.emitter.instruction("mov edx, 1");                              // context is request-owned
            abi::emit_call_label(ctx.emitter, "__rt_resource_alloc");
            ctx.emitter.instruction("test rax, rax");                           // did the registry allocate a generation handle?
            ctx.emitter.instruction(&format!("jz {}", registry_alloc_failed));  // unwind ContextState when registry growth fails
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip construction-error cleanup after success
            ctx.emitter.label(&registry_alloc_failed);
            abi::emit_pop_reg(ctx.emitter, "rax");
            abi::emit_call_label(ctx.emitter, "__rt_heap_free");
            release_detached_context_children_x86_64(ctx);
            emit_fd_result(ctx, 0);
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // share the false result after registry failure
            ctx.emitter.label(&state_alloc_failed);
            release_detached_context_children_x86_64(ctx);
            emit_fd_result(ctx, 0);
        }
    }
    ctx.emitter.label(&done_label);
}

/// Releases the options and notifier pair detached during failed AArch64 context construction.
fn release_detached_context_children_aarch64(ctx: &mut FunctionContext<'_>) {
    let options_done = ctx.next_label("sctx_release_options_done");
    let notifier_done = ctx.next_label("sctx_release_notifier_done");
    ctx.emitter.instruction("ldr x0, [sp]");                                    // load the retained options hash from the construction pair
    ctx.emitter.instruction(&format!("cbz x0, {}", options_done));              // skip release when no options were supplied
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    ctx.emitter.label(&options_done);
    ctx.emitter.instruction("ldr x0, [sp, #8]");                                // load the retained notification descriptor
    ctx.emitter.instruction(&format!("cbz x0, {}", notifier_done));             // skip release when no notification callback was supplied
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    ctx.emitter.label(&notifier_done);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Releases the options and notifier pair detached during failed x86_64 context construction.
fn release_detached_context_children_x86_64(ctx: &mut FunctionContext<'_>) {
    let options_done = ctx.next_label("sctx_release_options_done");
    let notifier_done = ctx.next_label("sctx_release_notifier_done");
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                        // load the retained options hash from the construction pair
    ctx.emitter.instruction("test rax, rax");                                   // were context options supplied?
    ctx.emitter.instruction(&format!("jz {}", options_done));                   // skip release when the options pointer is null
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    ctx.emitter.label(&options_done);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                    // load the retained notification descriptor
    ctx.emitter.instruction("test rax, rax");                                   // was a notification callback supplied?
    ctx.emitter.instruction(&format!("jz {}", notifier_done));                  // skip release when the descriptor pointer is null
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    ctx.emitter.label(&notifier_done);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Lowers `stream_context_get_default(options?)`.
pub(crate) fn lower_stream_context_get_default(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_context_get_default", 0, 1)?;
    emit_default_stream_context(ctx, inst.operands.first().copied())?;
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_set_default(options)`.
pub(crate) fn lower_stream_context_set_default(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_context_set_default", 1)?;
    emit_default_stream_context(ctx, inst.operands.first().copied())?;
    store_if_result(ctx, inst)
}

/// Returns the retained, lazily allocated default stream-context handle.
///
/// The global keeps one permanent registry reference. Every PHP return receives
/// a separate retain, while supplied options replace the existing ContextState
/// child or initialize the state during first allocation.
fn emit_default_stream_context(
    ctx: &mut FunctionContext<'_>,
    options: Option<ValueId>,
) -> Result<()> {
    if let Some(options) = options {
        store_stream_context_options(ctx, options, true)?;
    } else {
        clear_stream_context_options(ctx);
    }
    clear_stream_notification_callback(ctx);

    let existing_label = ctx.next_label("sctx_default_existing");
    let retain_label = ctx.next_label("sctx_default_retain");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_default_context_handle");
            ctx.emitter.instruction("ldr x0, [x9]");                            // load the registry handle owned by the default-context global
            ctx.emitter.instruction(&format!("cbnz x0, {}", existing_label));   // reuse the stable default context after first allocation
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_default_context_handle");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9]");                 // load the registry handle owned by the default-context global
            ctx.emitter.instruction("test rax, rax");                           // has the default ContextState been allocated?
            ctx.emitter.instruction(&format!("jnz {}", existing_label));        // reuse the stable default context after first allocation
        }
    }
    emit_dynamic_stream_context_allocation(ctx, "stream_context_default");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_default_context_handle");
            ctx.emitter.instruction("str x0, [x9]");                            // transfer the creator reference to the global owner
            ctx.emitter.instruction(&format!("b {}", retain_label));            // skip replacement for the newly initialized state
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_default_context_handle");
            ctx.emitter.instruction("mov QWORD PTR [r9], rax");                 // transfer the creator reference to the global owner
            ctx.emitter.instruction(&format!("jmp {}", retain_label));          // skip replacement for the newly initialized state
        }
    }

    ctx.emitter.label(&existing_label);
    if options.is_some() {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {}
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the existing opaque handle to typed context lookup
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_context_state");
        emit_transfer_stream_context_options_to_loaded_state(ctx);
    }

    ctx.emitter.label(&retain_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_default_context_handle");
            ctx.emitter.instruction("ldr x0, [x9]");                            // reload the global-owned opaque handle for the PHP return
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_default_context_handle");
            ctx.emitter.instruction("mov rdi, QWORD PTR [r9]");                 // pass the global-owned opaque handle to registry retain
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_resource_retain");
    Ok(())
}

/// php-src's `E_DEPRECATED` text for the two-argument `stream_context_set_option()` form.
///
/// The notice fires on the ARITY, not on the argument's type: `stream_context_set_option($c,
/// $array, null)` is a THREE-argument call and stays quiet, which MEASURED on `php -n` 8.5.6.
/// It was added in php 8.3, so it is version-gated like the rest of the diagnostic surface.
const STREAM_CONTEXT_SET_OPTION_TWO_ARG_DEPRECATION: &str =
    "Deprecated: Calling stream_context_set_option() with 2 arguments is deprecated, \
     use stream_context_set_options() instead\n";

/// php-src's `ValueError` when a STRING wrapper reaches the three-argument form.
///
/// The stub's `mixed $value` carries no default at all — it is `UNKNOWN`, not `null` — so an
/// absent fourth argument is a refusal rather than "pass null".
const STREAM_CONTEXT_SET_OPTION_VALUE_REQUIRED: &str =
    "stream_context_set_option(): Argument #4 ($value) must be provided when argument #2 \
     ($wrapper_or_options) is a string";

/// php-src's `ValueError` when an ARRAY wrapper is paired with a fourth `$value` argument.
const STREAM_CONTEXT_SET_OPTION_VALUE_FORBIDDEN: &str =
    "stream_context_set_option(): Argument #4 ($value) cannot be provided when argument #2 \
     ($wrapper_or_options) is an array";

/// php-src's `ValueError` when an ARRAY wrapper is paired with a non-null `$option_name`.
const STREAM_CONTEXT_SET_OPTION_NAME_MUST_BE_NULL: &str =
    "stream_context_set_option(): Argument #3 ($option_name) must be null when argument #2 \
     ($wrapper_or_options) is an array";

/// Reports whether an operand's declared type is an array, the shape php's `array|string`
/// `$wrapper_or_options` takes when the caller means "merge this whole options map".
fn stream_context_wrapper_is_array(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
}

/// Lowers `stream_context_set_option(context, options)` and the four-argument form.
pub(crate) fn lower_stream_context_set_option(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_context_set_option", 2, 4)?;
    match inst.operands.len() {
        2 => {
            let context = expect_operand(inst, 0)?;
            let options = expect_operand(inst, 1)?;
            emit_stream_context_set_option_two_arg_deprecation(ctx);
            // php prints the deprecation from the ARITY and only then judges the shape, so a
            // two-argument call with a STRING wrapper gets the notice AND the refusal. Only a
            // DECLARED string takes that branch: a `Mixed` operand is almost always an options
            // array behind a variable, and guessing "string" there would refuse a legal call.
            if matches!(ctx.raw_value_php_type(options)?, PhpType::Str) {
                super::super::exceptions::emit_value_error(
                    ctx,
                    STREAM_CONTEXT_SET_OPTION_NAME_CANNOT_BE_NULL,
                );
                return store_if_result(ctx, inst);
            }
            clear_stream_context_options(ctx);
            restore_stream_context_from_handle(ctx, context)?;
            retain_stream_context_options_scratch(ctx);
            merge_stream_context_options_into_scratch(ctx, options)?;
            update_stream_context_state_from_handle(ctx, context)?;
            emit_bool_result(ctx, true);
        }
        3 => lower_stream_context_set_option_3(ctx, inst)?,
        4 => {
            let wrapper = expect_operand(inst, 1)?;
            // php refuses an array wrapper here outright, so the refusal replaces the store —
            // this used to be a compile-time "unsupported EIR backend feature" instead.
            if stream_context_wrapper_is_array(&ctx.raw_value_php_type(wrapper)?) {
                super::super::exceptions::emit_value_error(
                    ctx,
                    STREAM_CONTEXT_SET_OPTION_VALUE_FORBIDDEN,
                );
                return store_if_result(ctx, inst);
            }
            // A string wrapper still needs a real option name: php refuses an explicit null
            // `$option_name` even when the fourth argument IS present. MEASURED — the wording
            // is the same one the three-argument string form raises.
            let option_name = expect_operand(inst, 2)?;
            if !matches!(ctx.raw_value_php_type(option_name)?, PhpType::Str) {
                let refuse = ctx.next_label("sctx_set_option_4_null_name");
                let named = ctx.next_label("sctx_set_option_4_named");
                emit_branch_on_stream_context_option_name_null(ctx, option_name, &refuse)?;
                abi::emit_jump(ctx.emitter, &named);
                ctx.emitter.label(&refuse);
                super::super::exceptions::emit_value_error(
                    ctx,
                    STREAM_CONTEXT_SET_OPTION_NAME_CANNOT_BE_NULL,
                );
                ctx.emitter.label(&named);
            }
            let context = expect_operand(inst, 0)?;
            clear_stream_context_options(ctx);
            restore_stream_context_from_handle(ctx, context)?;
            retain_stream_context_options_scratch(ctx);
            lower_stream_context_set_option_4(ctx, inst)?;
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            update_stream_context_state_from_handle(ctx, context)?;
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        }
        _ => emit_bool_result(ctx, true),
    }
    store_if_result(ctx, inst)
}

/// Lowers the three-argument `stream_context_set_option()` form, which php mostly REFUSES.
///
/// MEASURED on `php -n` 8.5.6 — three of the four shapes are a `ValueError`, and the fourth is
/// the two-argument merge spelled with an explicit `null`:
///
/// ```text
/// stream_context_set_option($c, ['http' => [...]], null)  bool(true), and no deprecation
/// stream_context_set_option($c, ['http' => [...]], 'x')   ValueError: Argument #3 ($option_name) must be null when argument #2 ($wrapper_or_options) is an array
/// stream_context_set_option($c, 'http', 'header')         ValueError: Argument #4 ($value) must be provided when argument #2 ($wrapper_or_options) is a string
/// stream_context_set_option($c, 'http', null)             ValueError: Argument #3 ($option_name) cannot be null when argument #2 ($wrapper_or_options) is a string
/// ```
///
/// This used to answer a silent `bool(true)` for all four and store nothing, so a caller who
/// forgot the value read the refusal as a successful write.
///
/// The wrapper's DECLARED type picks the family, and a wrapper that is neither — a `Mixed`
/// local — is resolved by the one thing that does not depend on it: a non-null `$option_name`
/// is a ValueError for BOTH families, so only the wording is a guess there, and it takes the
/// string wording. A null `$option_name` behind a `Mixed` wrapper stays on the old permissive
/// path rather than refusing what may well be the legal array shape.
fn lower_stream_context_set_option_3(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let wrapper = expect_operand(inst, 1)?;
    let option_name = expect_operand(inst, 2)?;
    let wrapper_ty = ctx.raw_value_php_type(wrapper)?;
    if stream_context_wrapper_is_array(&wrapper_ty) || !matches!(wrapper_ty, PhpType::Str) {
        // An array wrapper only survives with an explicit null `$option_name`, and then it is
        // the two-argument merge — WITHOUT the deprecation, which counts arguments.
        let merge = ctx.next_label("sctx_set_option_3_merge");
        emit_branch_on_stream_context_option_name_null(ctx, option_name, &merge)?;
        super::super::exceptions::emit_value_error(
            ctx,
            if stream_context_wrapper_is_array(&wrapper_ty) {
                STREAM_CONTEXT_SET_OPTION_NAME_MUST_BE_NULL
            } else {
                STREAM_CONTEXT_SET_OPTION_VALUE_REQUIRED
            },
        );
        ctx.emitter.label(&merge);
        let context = expect_operand(inst, 0)?;
        let options = expect_operand(inst, 1)?;
        clear_stream_context_options(ctx);
        restore_stream_context_from_handle(ctx, context)?;
        retain_stream_context_options_scratch(ctx);
        merge_stream_context_options_into_scratch(ctx, options)?;
        update_stream_context_state_from_handle(ctx, context)?;
        emit_bool_result(ctx, true);
        return Ok(());
    }
    let null_name = ctx.next_label("sctx_set_option_3_null_name");
    emit_branch_on_stream_context_option_name_null(ctx, option_name, &null_name)?;
    super::super::exceptions::emit_value_error(ctx, STREAM_CONTEXT_SET_OPTION_VALUE_REQUIRED);
    ctx.emitter.label(&null_name);
    super::super::exceptions::emit_value_error(ctx, STREAM_CONTEXT_SET_OPTION_NAME_CANNOT_BE_NULL);
    Ok(())
}

/// php-src's `ValueError` when a STRING wrapper is paired with a null `$option_name`.
const STREAM_CONTEXT_SET_OPTION_NAME_CANNOT_BE_NULL: &str =
    "stream_context_set_option(): Argument #3 ($option_name) cannot be null when argument #2 \
     ($wrapper_or_options) is a string";

/// Branches to `null_label` when the loaded `$option_name` operand is PHP null.
///
/// A declared-`Str` operand can never be null, so the branch is resolved at emit time there
/// rather than testing a register that cannot hold the sentinel.
fn emit_branch_on_stream_context_option_name_null(
    ctx: &mut FunctionContext<'_>,
    option_name: ValueId,
    null_label: &str,
) -> Result<()> {
    let ty = ctx.raw_value_php_type(option_name)?;
    if matches!(ty, PhpType::Str) {
        return Ok(());
    }
    if matches!(ty, PhpType::Void | PhpType::Never) {
        abi::emit_jump(ctx.emitter, null_label);
        return Ok(());
    }
    ctx.load_value_to_result(option_name)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let scratch = match ctx.emitter.target.arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r10",
    };
    abi::emit_load_int_immediate(ctx.emitter, scratch, NULL_SENTINEL);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", result_reg, scratch));        // is the option name php null?
            ctx.emitter.instruction(&format!("b.eq {}", null_label));             // a null name selects php's other wording
            ctx.emitter.instruction(&format!("cbz {}, {}", result_reg, null_label)); // an unboxed null pointer is the same absence
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", result_reg, scratch));        // is the option name php null?
            ctx.emitter.instruction(&format!("je {}", null_label));               // a null name selects php's other wording
            ctx.emitter
                .instruction(&format!("test {}, {}", result_reg, result_reg));    // an unboxed null pointer is the same absence
            ctx.emitter.instruction(&format!("jz {}", null_label));
        }
    }
    Ok(())
}

/// Emits php 8.3+'s `E_DEPRECATED` notice for the two-argument form.
fn emit_stream_context_set_option_two_arg_deprecation(ctx: &mut FunctionContext<'_>) {
    if crate::codegen::compile_php_version() < crate::php_version::PhpVersion::Php83 {
        return;
    }
    let (label, len) = ctx
        .data
        .add_string(STREAM_CONTEXT_SET_OPTION_TWO_ARG_DEPRECATION.as_bytes());
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
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");                       // stderr, and `@` suppresses it
}

/// Lowers `stream_context_set_params(context, params)` onto the addressed ContextState.
pub(crate) fn lower_stream_context_set_params(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_context_set_params", 2)?;
    let context = expect_operand(inst, 0)?;
    let params = expect_operand(inst, 1)?;
    apply_stream_context_notification_param(ctx, context, params)?;
    clear_stream_context_options(ctx);
    restore_stream_context_from_handle(ctx, context)?;
    retain_stream_context_options_scratch(ctx);
    merge_stream_context_params_options_into_scratch(ctx, params)?;
    update_stream_context_state_from_handle(ctx, context)?;
    emit_bool_result(ctx, true);
    store_if_result(ctx, inst)
}

/// Transfers the retained notifier scratch into the loaded ContextState.
///
/// The notifier is acquired before lookup by `capture_stream_notification_callback`.
/// A valid state takes that owner and releases its previous descriptor; an invalid
/// handle releases the staged descriptor instead of leaking it or contaminating a
/// later context operation.
pub(super) fn emit_transfer_stream_notification_to_loaded_state(ctx: &mut FunctionContext<'_>) {
    let invalid_label = ctx.next_label("sctx_notifier_invalid");
    let release_old_label = ctx.next_label("sctx_notifier_release_old");
    let done_label = ctx.next_label("sctx_notifier_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x10, x0");                             // preserve the resolved ContextState while detaching notifier scratch
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_notification_callback");
            ctx.emitter.instruction("ldr x11, [x9]");                           // load the newly retained notification descriptor
            ctx.emitter.instruction("str xzr, [x9]");                           // detach scratch ownership before any descriptor release
            ctx.emitter.instruction(&format!("cbz x10, {}", invalid_label));    // invalid handles must release the detached descriptor
            ctx.emitter.instruction(&format!(
                "ldr x0, [x10, #{}]",
                CONTEXT_NOTIFIER_OFFSET
            ));                                                                 // load the descriptor previously owned by this context
            ctx.emitter.instruction(&format!(
                "str x11, [x10, #{}]",
                CONTEXT_NOTIFIER_OFFSET
            ));                                                                 // publish the replacement on the targeted ContextState
            ctx.emitter.instruction(&format!("b {}", release_old_label));       // release the previous state owner when present
            ctx.emitter.label(&invalid_label);
            ctx.emitter.instruction("mov x0, x11");                             // invalid state cleanup releases the staged descriptor
            ctx.emitter.label(&release_old_label);
            ctx.emitter.instruction(&format!("cbz x0, {}", done_label));        // absent descriptors require no cleanup
            callable_descriptor::emit_release_current_descriptor(ctx.emitter);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // preserve the resolved ContextState while detaching notifier scratch
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_notification_callback");
            ctx.emitter.instruction("mov r11, QWORD PTR [r9]");                 // load the newly retained notification descriptor
            ctx.emitter.instruction("mov QWORD PTR [r9], 0");                   // detach scratch ownership before any descriptor release
            ctx.emitter.instruction("test r10, r10");                           // did the opaque handle resolve to a ContextState?
            ctx.emitter.instruction(&format!("jz {}", invalid_label));          // invalid handles must release the detached descriptor
            ctx.emitter.instruction(&format!(
                "mov rax, QWORD PTR [r10 + {}]",
                CONTEXT_NOTIFIER_OFFSET
            ));                                                                 // load the descriptor previously owned by this context
            ctx.emitter.instruction(&format!(
                "mov QWORD PTR [r10 + {}], r11",
                CONTEXT_NOTIFIER_OFFSET
            ));                                                                 // publish the replacement on the targeted ContextState
            ctx.emitter.instruction(&format!("jmp {}", release_old_label));     // release the previous state owner when present
            ctx.emitter.label(&invalid_label);
            ctx.emitter.instruction("mov rax, r11");                            // invalid state cleanup releases the staged descriptor
            ctx.emitter.label(&release_old_label);
            ctx.emitter.instruction("test rax, rax");                           // is there a descriptor owner to release?
            ctx.emitter.instruction(&format!("jz {}", done_label));             // absent descriptors require no cleanup
            callable_descriptor::emit_release_current_descriptor(ctx.emitter);
        }
    }
    ctx.emitter.label(&done_label);
}

/// Captures a runtime `notification` callable from stream context params.
pub(super) fn capture_stream_notification_callback(
    ctx: &mut FunctionContext<'_>,
    params: ValueId,
) -> Result<()> {
    let done = ctx.next_label("sctx_notification_absent");
    load_stream_notification_param_descriptor(ctx, params, &done)?;
    callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    store_current_result_as_stream_notification_callback(ctx);
    ctx.emitter.label(&done);
    Ok(())
}

/// Applies a present runtime notification param only to the addressed context.
pub(super) fn apply_stream_context_notification_param(
    ctx: &mut FunctionContext<'_>,
    context: ValueId,
    params: ValueId,
) -> Result<()> {
    let done = ctx.next_label("sctx_notification_unchanged");
    load_stream_notification_param_descriptor(ctx, params, &done)?;
    callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    store_current_result_as_stream_notification_callback(ctx);
    load_context_state_to_result(ctx, context, "stream_context_set_params")?;
    emit_transfer_stream_notification_to_loaded_state(ctx);
    ctx.emitter.label(&done);
    Ok(())
}

/// Loads a present callable notification descriptor from a runtime params hash.
///
/// Both direct callable entries and callable values boxed behind Mixed storage
/// are accepted. Missing or non-callable entries branch to `absent_label`
/// without modifying the current context notifier.
pub(super) fn load_stream_notification_param_descriptor(
    ctx: &mut FunctionContext<'_>,
    params: ValueId,
    absent_label: &str,
) -> Result<()> {
    let direct = ctx.next_label("sctx_notification_direct");
    let normalized = ctx.next_label("sctx_notification_normalized");
    let (key, key_len) = ctx.data.add_string(b"notification");
    ctx.load_value_to_result(params)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &key);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction(&format!("cbz x0, {}", absent_label));      // an absent notification key preserves the current notifier
            ctx.emitter.instruction("cmp x3, #10");                             // is the entry a direct callable descriptor?
            ctx.emitter.instruction(&format!("b.eq {}", direct));               // direct callables already expose their descriptor in x1
            ctx.emitter.instruction("cmp x3, #7");                              // is the entry boxed behind Mixed storage?
            ctx.emitter.instruction(&format!("b.ne {}", absent_label));         // reject non-callable parameter values
            ctx.emitter.instruction("mov x0, x1");                              // pass the boxed parameter value to Mixed unboxing
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("cmp x0, #10");                             // did the Mixed cell contain a callable descriptor?
            ctx.emitter.instruction(&format!("b.ne {}", absent_label));         // reject boxed non-callable values
            ctx.emitter.instruction("mov x0, x1");                              // move the unboxed descriptor into the canonical result register
            ctx.emitter.instruction(&format!("b {}", normalized));              // join direct and boxed callable paths
            ctx.emitter.label(&direct);
            ctx.emitter.instruction("mov x0, x1");                              // move the direct descriptor into the canonical result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the runtime params hash to lookup
            abi::emit_symbol_address(ctx.emitter, "rsi", &key);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction("test rax, rax");                           // was a notification key present?
            ctx.emitter.instruction(&format!("jz {}", absent_label));           // an absent key preserves the current notifier
            ctx.emitter.instruction("cmp rcx, 10");                             // is the entry a direct callable descriptor?
            ctx.emitter.instruction(&format!("je {}", direct));                 // direct callables already expose their descriptor in rdi
            ctx.emitter.instruction("cmp rcx, 7");                              // is the entry boxed behind Mixed storage?
            ctx.emitter.instruction(&format!("jne {}", absent_label));          // reject non-callable parameter values
            ctx.emitter.instruction("mov rax, rdi");                            // pass the boxed parameter value to Mixed unboxing
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("cmp rax, 10");                             // did the Mixed cell contain a callable descriptor?
            ctx.emitter.instruction(&format!("jne {}", absent_label));          // reject boxed non-callable values
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed descriptor into the canonical result register
            ctx.emitter.instruction(&format!("jmp {}", normalized));            // join direct and boxed callable paths
            ctx.emitter.label(&direct);
            ctx.emitter.instruction("mov rax, rdi");                            // move the direct descriptor into the canonical result register
        }
    }
    ctx.emitter.label(&normalized);
    Ok(())
}

/// Stores the loaded callable descriptor into `_stream_notification_callback`.
pub(super) fn store_current_result_as_stream_notification_callback(ctx: &mut FunctionContext<'_>) {
    let addr_reg = abi::symbol_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, addr_reg, "_stream_notification_callback");
    abi::emit_store_to_address(ctx.emitter, result_reg, addr_reg, 0);
}

/// Clears `_stream_notification_callback` so later transfers do not fire stale callbacks.
pub(super) fn clear_stream_notification_callback(ctx: &mut FunctionContext<'_>) {
    let addr_reg = abi::symbol_scratch_reg(ctx.emitter);
    let zero_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, addr_reg, "_stream_notification_callback");
    abi::emit_load_int_immediate(ctx.emitter, zero_reg, 0);
    abi::emit_store_to_address(ctx.emitter, zero_reg, addr_reg, 0);
}

/// Lowers `stream_context_get_options(context)`.
pub(crate) fn lower_stream_context_get_options(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_context_get_options", 1)?;
    let context = expect_operand(inst, 0)?;
    let empty_label = ctx.next_label("scgo_empty");
    let done_label = ctx.next_label("scgo_done");
    load_context_state_to_result(ctx, context, "stream_context_get_options")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", empty_label));       // reject an invalid or stale context handle
            ctx.emitter.instruction("ldr x0, [x0]");                            // load ContextState.options
            ctx.emitter.instruction(&format!("cbz x0, {}", empty_label));       // allocate an empty hash when no context options exist
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the empty-hash fallback after retaining options
            ctx.emitter.label(&empty_label);
            ctx.emitter.instruction("mov x0, #1");                              // pass the empty fallback hash capacity
            ctx.emitter.instruction("mov x1, #7");                              // select Mixed values for the fallback hash
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the opaque handle resolve to ContextState?
            ctx.emitter.instruction(&format!("jz {}", empty_label));            // reject invalid or stale context handles
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // load ContextState.options
            ctx.emitter.instruction("test rax, rax");                           // test whether a context options pointer exists
            ctx.emitter.instruction(&format!("jz {}", empty_label));            // allocate an empty hash when no context options exist
            ctx.emitter.instruction("mov rdi, rax");                            // pass the options pointer to incref
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the empty-hash fallback after retaining options
            ctx.emitter.label(&empty_label);
            ctx.emitter.instruction("mov edi, 1");                              // pass the empty fallback hash capacity
            ctx.emitter.instruction("mov esi, 7");                              // select Mixed values for the fallback hash
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.label(&done_label);
        }
    }
    store_if_result(ctx, inst)
}

/// Resolves a context resource operand to its stable `ContextState` pointer.
pub(super) fn load_context_state_to_result(
    ctx: &mut FunctionContext<'_>,
    context: ValueId,
    function_name: &str,
) -> Result<()> {
    load_stream_handle_to_result(ctx, context, function_name)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the opaque context handle to the typed registry lookup
    }
    abi::emit_call_label(ctx.emitter, "__rt_context_state");
    Ok(())
}

/// Lowers `stream_context_get_params(context)` to PHP's reconstructed params map.
///
/// php-src does not retain the caller's params hash. It returns the live
/// notification callable first, when present, followed by an `options` entry
/// that owns a COW-safe reference to the context's current options hash.
pub(crate) fn lower_stream_context_get_params(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_context_get_params", 1)?;
    let context = expect_operand(inst, 0)?;
    let options_label = ctx.next_label("scgp_options");
    let empty_options_label = ctx.next_label("scgp_empty_options");
    let insert_options_label = ctx.next_label("scgp_insert_options");
    let (notification_key, notification_len) = ctx.data.add_string(b"notification");
    let (options_key, options_len) = ctx.data.add_string(b"options");
    load_context_state_to_result(ctx, context, "stream_context_get_params")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve stable ContextState and result-hash slots
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve the resolved ContextState across hash operations
            ctx.emitter.instruction("mov x0, #4");                              // allocate room for notification and options entries
            ctx.emitter.instruction("mov x1, #7");                              // params maps expose dynamically typed values
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction("str x0, [sp, #8]");                        // preserve the owned params result hash
            ctx.emitter.instruction("ldr x9, [sp, #0]");                        // reload ContextState before optional notifier lookup
            ctx.emitter.instruction(&format!("cbz x9, {}", options_label));     // invalid contexts expose only the empty options fallback
            ctx.emitter.instruction(&format!(
                "ldr x0, [x9, #{}]",
                CONTEXT_NOTIFIER_OFFSET
            ));                                                                 // load the descriptor owned by this context
            ctx.emitter.instruction(&format!("cbz x0, {}", options_label));     // omit notification when no user callback is installed
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
            ctx.emitter.instruction("mov x3, x0");                              // transfer the retained descriptor into the params hash
            ctx.emitter.instruction("mov x4, xzr");                             // callable descriptors use no high payload word
            ctx.emitter.instruction("mov x5, #10");                             // runtime tag 10 identifies callable descriptors
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // reload the params result hash for insertion
            abi::emit_symbol_address(ctx.emitter, "x1", &notification_key);
            ctx.emitter.instruction(&format!("mov x2, #{}", notification_len)); // pass the notification key byte length
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            ctx.emitter.instruction("str x0, [sp, #8]");                        // retain a possibly relocated result hash
            ctx.emitter.label(&options_label);
            ctx.emitter.instruction("ldr x9, [sp, #0]");                        // reload ContextState before options lookup
            ctx.emitter.instruction(&format!("cbz x9, {}", empty_options_label)); // invalid contexts receive an empty options hash
            ctx.emitter.instruction(&format!(
                "ldr x0, [x9, #{}]",
                CONTEXT_OPTIONS_OFFSET
            ));                                                                 // load this context's owned options hash
            ctx.emitter.instruction(&format!("cbz x0, {}", empty_options_label)); // contexts without options receive a fresh empty hash
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction(&format!("b {}", insert_options_label));    // insert the retained live options snapshot
            ctx.emitter.label(&empty_options_label);
            ctx.emitter.instruction("mov x0, #1");                              // allocate the mandatory empty options entry
            ctx.emitter.instruction("mov x1, #7");                              // empty options hashes hold dynamically typed values
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.label(&insert_options_label);
            ctx.emitter.instruction("mov x3, x0");                              // transfer the owned options hash into the params result
            ctx.emitter.instruction("mov x4, xzr");                             // associative arrays use no high payload word
            ctx.emitter.instruction("mov x5, #5");                              // runtime tag 5 identifies associative arrays
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // reload the params result hash for its final insertion
            abi::emit_symbol_address(ctx.emitter, "x1", &options_key);
            ctx.emitter.instruction(&format!("mov x2, #{}", options_len));      // pass the options key byte length
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            ctx.emitter.instruction("add sp, sp, #32");                         // release params construction scratch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                             // reserve stable ContextState and result-hash slots
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // preserve the resolved ContextState across hash operations
            ctx.emitter.instruction("mov edi, 4");                              // allocate room for notification and options entries
            ctx.emitter.instruction("mov esi, 7");                              // params maps expose dynamically typed values
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // preserve the owned params result hash
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // reload ContextState before optional notifier lookup
            ctx.emitter.instruction("test r10, r10");                           // did the context handle resolve?
            ctx.emitter.instruction(&format!("jz {}", options_label));          // invalid contexts expose only the empty options fallback
            ctx.emitter.instruction(&format!(
                "mov rax, QWORD PTR [r10 + {}]",
                CONTEXT_NOTIFIER_OFFSET
            ));                                                                 // load the descriptor owned by this context
            ctx.emitter.instruction("test rax, rax");                           // is a user notification callback installed?
            ctx.emitter.instruction(&format!("jz {}", options_label));          // omit notification when no callback exists
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
            ctx.emitter.instruction("mov rcx, rax");                            // transfer the retained descriptor into the params hash
            ctx.emitter.instruction("xor r8, r8");                              // callable descriptors use no high payload word
            ctx.emitter.instruction("mov r9, 10");                              // runtime tag 10 identifies callable descriptors
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");            // reload the params result hash for insertion
            abi::emit_symbol_address(ctx.emitter, "rsi", &notification_key);
            ctx.emitter.instruction(&format!("mov rdx, {}", notification_len)); // pass the notification key byte length
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // retain a possibly relocated result hash
            ctx.emitter.label(&options_label);
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // reload ContextState before options lookup
            ctx.emitter.instruction("test r10, r10");                           // did the context handle resolve?
            ctx.emitter.instruction(&format!("jz {}", empty_options_label));    // invalid contexts receive an empty options hash
            ctx.emitter.instruction(&format!(
                "mov rax, QWORD PTR [r10 + {}]",
                CONTEXT_OPTIONS_OFFSET
            ));                                                                 // load this context's owned options hash
            ctx.emitter.instruction("test rax, rax");                           // does this context have any options?
            ctx.emitter.instruction(&format!("jz {}", empty_options_label));    // contexts without options receive a fresh empty hash
            ctx.emitter.instruction("mov rdi, rax");                            // pass the live options hash to the retain helper
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction(&format!("jmp {}", insert_options_label));  // insert the retained live options snapshot
            ctx.emitter.label(&empty_options_label);
            ctx.emitter.instruction("mov edi, 1");                              // allocate the mandatory empty options entry
            ctx.emitter.instruction("mov esi, 7");                              // empty options hashes hold dynamically typed values
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.label(&insert_options_label);
            ctx.emitter.instruction("mov rcx, rax");                            // transfer the owned options hash into the params result
            ctx.emitter.instruction("xor r8, r8");                              // associative arrays use no high payload word
            ctx.emitter.instruction("mov r9, 5");                               // runtime tag 5 identifies associative arrays
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");            // reload the params result hash for its final insertion
            abi::emit_symbol_address(ctx.emitter, "rsi", &options_key);
            ctx.emitter.instruction(&format!("mov rdx, {}", options_len));      // pass the options key byte length
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            ctx.emitter.instruction("add rsp, 32");                             // release params construction scratch
        }
    }
    store_if_result(ctx, inst)
}

/// php-src's verbatim `ValueError` wording for a `stream_get_contents()` `$length` below `-1`.
const STREAM_GET_CONTENTS_NEGATIVE_LENGTH_MESSAGE: &str =
    "stream_get_contents(): Argument #2 ($length) must be greater than or equal to -1";

/// Lowers `stream_get_contents(stream, length?, offset?)` to `string|false`.
pub(crate) fn lower_stream_get_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_get_contents", 1, 3)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_handle_to_result(ctx, stream, "stream_get_contents")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the opaque stream handle to state-owned chunk lookup
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_chunk_size");
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_open_stream_handle_to_result(ctx, stream, "stream_get_contents")?;
    if inst.operands.len() == 1 {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_pop_reg(ctx.emitter, "x1");
                abi::emit_pop_reg(ctx.emitter, "x0");
            }
            Arch::X86_64 => {
                abi::emit_pop_reg(ctx.emitter, "rsi");
                abi::emit_pop_reg(ctx.emitter, "rax");
            }
        }
        lower_stream_get_contents_read_all(ctx);
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
        return store_if_result(ctx, inst);
    }

    let read_all = ctx.next_label("sgc_read_all");
    let skip_seek = ctx.next_label("sgc_skip_seek");
    let wrap_seek = ctx.next_label("sgc_wrap_seek");
    let seek_failed = ctx.next_label("sgc_seek_failed");
    let done = ctx.next_label("sgc_done");

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x9");
            abi::emit_pop_reg(ctx.emitter, "x10");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "r9");
            abi::emit_pop_reg(ctx.emitter, "r10");
        }
    }
    emit_stream_get_contents_frame_enter(ctx);
    emit_stream_get_contents_save_fd(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x9, [sp, #16]");                       // save state-owned chunk size across length and offset evaluation
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], r9");            // save state-owned chunk size across length and offset evaluation
        }
    }
    let length = expect_operand(inst, 1)?;
    require_optional_int(
        ctx.load_value_to_result(length)?.codegen_repr(),
        "stream_get_contents length",
    )?;
    // php-src accepts exactly one negative `$length`: `-1`, its documented "read to EOF"
    // sentinel. Anything below that is a catchable ValueError raised BEFORE a byte is read,
    // where this used to silently fall into the read-all path and answer the whole stream.
    // A null `$length` carries NULL_SENTINEL, a large positive, so it clears the bound.
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedAtLeast(
            abi::int_result_reg(ctx.emitter),
            -1,
        ),
        STREAM_GET_CONTENTS_NEGATIVE_LENGTH_MESSAGE,
    );
    emit_stream_get_contents_save_length(ctx);

    if inst.operands.len() == 3 {
        let offset = expect_operand(inst, 2)?;
        require_int(
            ctx.load_value_to_result(offset)?.codegen_repr(),
            "stream_get_contents offset",
        )?;
        lower_stream_get_contents_seek(ctx, &skip_seek, &wrap_seek, &seek_failed);
    }

    lower_stream_get_contents_bounded_or_all(ctx, &read_all, &done);
    ctx.emitter.label(&read_all);
    lower_stream_get_contents_reload_fd_and_leave_frame(ctx);
    lower_stream_get_contents_read_all(ctx);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the seek-failure false result after reading successfully
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the seek-failure false result after reading successfully
        }
    }
    ctx.emitter.label(&seek_failed);
    emit_stream_get_contents_frame_leave(ctx);
    emit_bool_result(ctx, false);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}
