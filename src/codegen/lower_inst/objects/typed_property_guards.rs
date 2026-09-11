//! Purpose:
//! Emits uninitialized typed-property guards and fatal paths.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Catchable Error publication and uncaught diagnostics retain their existing behavior.

use super::*;

/// Emits a fatal guard for reads from uninitialized typed properties.
pub(super) fn emit_uninitialized_typed_property_guard(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    object_reg: &str,
) {
    let initialized_label = ctx.next_label("typed_prop_initialized");
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, marker_reg, object_reg, slot.offset + 8);
    abi::emit_load_int_immediate(
        ctx.emitter,
        sentinel_reg,
        UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the property marker against the uninitialized sentinel
            ctx.emitter
                .instruction(&format!("b.ne {}", initialized_label)); // continue the property read once the slot has been initialized
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the property marker against the uninitialized sentinel
            ctx.emitter
                .instruction(&format!("jne {}", initialized_label)); // continue the property read once the slot has been initialized
        }
    }
    emit_uninitialized_typed_property_fatal(ctx, slot);
    ctx.emitter.label(&initialized_label);
}

/// Compares a typed instance-property marker with the uninitialized sentinel.
pub(super) fn emit_typed_property_initialized_bool(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    object_reg: &str,
) {
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, marker_reg, object_reg, slot.offset + 8);
    abi::emit_load_int_immediate(
        ctx.emitter,
        sentinel_reg,
        UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(
                &format!("cmp {}, {}", marker_reg, sentinel_reg)
            );                                                                  // compare the property marker against the uninitialized sentinel
            ctx.emitter.instruction("cset x0, ne");                             // materialize true when the instance property is initialized
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(
                &format!("cmp {}, {}", marker_reg, sentinel_reg)
            );                                                                  // compare the property marker against the uninitialized sentinel
            ctx.emitter.instruction("setne al");                                // materialize true when the instance property is initialized
            ctx.emitter.instruction("movzx rax, al");                           // widen the initialization flag into the integer result register
        }
    }
}

/// Emits the runtime throw for an uninitialized typed-property read.
///
/// Constructs an `Error` object with the diagnostic message, publishes it to
/// `_exc_value`, and branches to `__rt_throw_current` so surrounding try/catch
/// blocks can observe and catch it. When no handler is registered, the uncaught
/// fast path prints the specific fatal diagnostic and exits, preserving the old behavior.
pub(super) fn emit_uninitialized_typed_property_fatal(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
) {
    let message = format!(
        "Typed property {}::${} must not be accessed before initialization",
        slot.class_name, slot.property
    );
    let fatal_message = format!("Fatal error: {}\n", message);
    let (fatal_label, fatal_len) = ctx.data.add_string(fatal_message.as_bytes());
    emit_uninitialized_typed_property_uncaught_fatal_if_no_handler(
        ctx,
        &fatal_label,
        fatal_len,
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #56");                             // request Throwable payload storage (message/code/previous)
            ctx.emitter.instruction("bl __rt_heap_alloc");                      // allocate the Error object payload
            ctx.emitter.instruction("mov x9, #6");                              // heap kind 6 = object instance
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp allocation as a runtime object
            ctx.emitter.instruction("bl __rt_object_handle_acquire");           // bind the new object to its PHP object handle
            abi::emit_symbol_address(ctx.emitter, "x9", "_spl_error_class_id");   // load Error's runtime class id symbol
            ctx.emitter.instruction("ldr x9, [x9]");                            // load Error's runtime class id for this program
            ctx.emitter.instruction("str x9, [x0]");                            // store class id at the object header
            abi::emit_symbol_address(ctx.emitter, "x9", &message_label);          // materialize static Error message pointer
            ctx.emitter.instruction("str x9, [x0, #8]");                        // store static Error message pointer
            ctx.emitter.instruction(&format!("mov x9, #{}", message_len));      // load Error message length
            ctx.emitter.instruction("str x9, [x0, #16]");                       // store exception message length
            ctx.emitter.instruction("str xzr, [x0, #24]");                      // exception code defaults to zero
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(ctx.emitter, "x0");
            ctx.emitter.instruction("str xzr, [x0, #40]");                      // previous defaults to null
            abi::emit_store_reg_to_symbol(ctx.emitter, "x0", "_exc_value", 0);  // publish the active exception object
            ctx.emitter.instruction("b __rt_throw_current");                    // enter the standard exception unwinder
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rbp");                                // preserve caller frame pointer for exception allocation
            ctx.emitter.instruction("mov rbp, rsp");                            // establish aligned helper frame
            ctx.emitter.instruction("sub rsp, 16");                             // keep the nested heap allocation call 16-byte aligned
            ctx.emitter.instruction("mov rax, 56");                             // request Throwable payload storage (message/code/previous)
            ctx.emitter.instruction("call __rt_heap_alloc");                    // allocate the Error object payload
            ctx.emitter.instruction(
                &format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))
            );                                                                  // stamp the canonical x86_64 heap-kind word (magic + kind 6 throwable)
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp allocation as a runtime object
            ctx.emitter.instruction("call __rt_object_handle_acquire");         // bind the new object to its PHP object handle
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_spl_error_class_id", 0); // load Error's runtime class id for this program
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store class id at the object header
            abi::emit_symbol_address(ctx.emitter, "r10", &message_label);          // materialize static Error message pointer
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store static Error message pointer
            ctx.emitter.instruction(
                &format!("mov QWORD PTR [rax + 16], {}", message_len)
            );                                                                  // store Error message length
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");             // exception code defaults to zero
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(ctx.emitter, "rax");
            ctx.emitter.instruction("mov QWORD PTR [rax + 40], 0");             // previous defaults to null
            abi::emit_store_reg_to_symbol(ctx.emitter, "rax", "_exc_value", 0);   // publish the active exception object
            ctx.emitter.instruction("mov rsp, rbp");                            // release helper frame before throwing
            ctx.emitter.instruction("pop rbp");                                 // restore caller frame pointer before throwing
            ctx.emitter.instruction("jmp __rt_throw_current");                  // enter the standard exception unwinder
        }
    }
}

/// Emits a no-handler fast path that preserves the specific typed-property fatal text.
pub(super) fn emit_uninitialized_typed_property_uncaught_fatal_if_no_handler(
    ctx: &mut FunctionContext<'_>,
    fatal_label: &str,
    fatal_len: usize,
) {
    let throw_label = ctx.next_label("typed_property_throw");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_exc_handler_top", 0);
            ctx.emitter.instruction(&format!("cbnz x9, {}", throw_label));      // keep typed-property errors catchable when a handler is active
            abi::emit_symbol_address(ctx.emitter, "x1", fatal_label);          // load the specific uninitialized typed-property fatal text
            ctx.emitter.instruction(&format!("mov x2, #{}", fatal_len));        // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the uninitialized typed-property fatal
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_exc_handler_top", 0);
            ctx.emitter.instruction("test r10, r10");                           // is there an active handler that can catch the Error?
            ctx.emitter.instruction(&format!("jne {}", throw_label));           // keep typed-property errors catchable when a handler is active
            abi::emit_symbol_address(ctx.emitter, "rsi", fatal_label);          // load the specific uninitialized typed-property fatal text
            ctx.emitter.instruction(&format!("mov edx, {}", fatal_len));        // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the uninitialized typed-property fatal
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall
            ctx.emitter.instruction("syscall");                                 // write the specific uninitialized typed-property fatal diagnostic
            abi::emit_exit(ctx.emitter, 1);
        }
    }
    ctx.emitter.label(&throw_label);
}
