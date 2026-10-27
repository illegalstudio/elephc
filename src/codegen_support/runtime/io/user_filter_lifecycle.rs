//! Purpose:
//! Retires the object owners held by both directions of a user-filtered descriptor.
//!
//! Called from:
//! - The fclose and stream_filter_remove lowerers through the shared runtime emitter.
//! - The attach helper when onCreate refuses its provisional instance.
//!
//! Key details:
//! - Both table roots detach before PHP callbacks can reenter or throw.
//! - Each direction has an exception guard, including while another direction closes.
//! - A refused onCreate rolls back only the provisional object's roots, without onClose.
//! - The descriptor is returned unchanged for the caller's remaining native teardown.
//! - During fclose, a separate descriptor guard completes native close when PHP cleanup throws.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::exceptions::guards;

/// Emits close and refused-creation rollback entries that consume every selected table owner.
pub(crate) fn emit_user_filter_release_fd(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let (arg, result, frame, selected) = if arm { ("x0", "x0", "sp", "x1") } else { ("rdi", "rax", "rsp", "rsi") };
    emitter.blank();
    emitter.comment("--- runtime: user_filter_release_fd ---");
    emitter.label_global("__rt_user_filter_release_fd");
    emitter.instruction(if arm { "mov x1, #0" } else { "xor esi, esi" });       // ordinary close retires every attached direction
    // Cross-entry jumps must name the global body so Mach-O keeps its separate atom.
    abi::emit_jump(emitter, "__rt_user_filter_reject_fd");
    emitter.label_global("__rt_user_filter_reject_fd");
    // -- detach both direction owners before publishing their exceptional cleanup --
    // Frame112: fd0, objects8/16, rejected object24, guards32/64, caller linkage96.
    abi::emit_frame_prologue(emitter, 112);
    abi::emit_store_to_address(emitter, arg, frame, 0);
    abi::emit_store_to_address(emitter, selected, frame, 24);
    emitter.instruction(if arm { "add x10, x0, x0" } else { "lea r10, [rdi + rdi]" }); // derive the read-direction table index
    abi::emit_symbol_address(emitter, if arm { "x9" } else { "r9" }, "_user_filter_instances");
    for direction in 0..2 {
        let detach = format!("__rt_user_filter_detach_{direction}");
        let saved = format!("__rt_user_filter_saved_{direction}");
        if direction == 1 {
            emitter.instruction(if arm { "add x10, x10, #1" } else { "inc r10" }); // select the write-direction table slot
        }
        emitter.instruction(if arm { "ldr x0, [x9, x10, lsl #3]" } else { "mov rax, QWORD PTR [r9 + r10 * 8]" }); // transfer the table's object reference
        emitter.instruction(if arm { "cmp x1, #0" } else { "test rsi, rsi" });  // zero selects the ordinary close of every direction
        emitter.instruction(&format!("{} {detach}", if arm { "b.eq" } else { "jz" })); // a full close has no object selection filter
        emitter.instruction(if arm { "cmp x0, x1" } else { "cmp rax, rsi" });   // a refused attach owns only matching provisional roots
        emitter.instruction(&format!("{} {detach}", if arm { "b.eq" } else { "je" })); // preserve unrelated direction instances
        abi::emit_load_int_immediate(emitter, result, 0);
        abi::emit_jump(emitter, &saved);
        emitter.label(&detach);
        emitter.instruction(if arm { "str xzr, [x9, x10, lsl #3]" } else { "mov QWORD PTR [r9 + r10 * 8], 0" }); // retire the root before any PHP callback
        emitter.instruction(if arm { "cmp x0, #0" } else { "test rax, rax" });  // preserve builtin filter IDs in slots without user objects
        emitter.instruction(&format!("{} {saved}", if arm { "b.eq" } else { "jz" })); // empty user slots have no filter ID to retire
        abi::emit_load_from_address(emitter, if arm { "x12" } else { "r8" }, frame, 0);
        abi::emit_symbol_address(emitter, if arm { "x11" } else { "r11" }, if direction == 0 { "_stream_read_filters" } else { "_stream_write_filters" });
        emitter.instruction(if arm { "strb wzr, [x11, x12]" } else { "mov BYTE PTR [r11 + r8], 0" }); // retire the matching user-filter ID with its instance
        emitter.label(&saved);
        abi::emit_store_to_address(emitter, result, frame, 8 + direction * 8);
    }
    abi::emit_load_from_address(emitter, result, frame, 8);
    abi::emit_load_from_address(emitter, if arm { "x11" } else { "r11" }, frame, 16);
    emitter.instruction(if arm { "orr x0, x0, x11" } else { "or rax, r11" });   // plain descriptors need no ownership guard publication
    emitter.instruction(if arm { "cmp x0, #0" } else { "test rax, rax" });      // avoid guard-chain work when both user-filter slots are empty
    emitter.instruction(&format!("{} __rt_user_filter_retire_done", if arm { "b.eq" } else { "jz" })); // return the descriptor directly when no user objects exist
    for (object, guard, x86_guard) in [(8, 32, 64), (16, 64, 32)] {
        abi::emit_load_from_address(emitter, result, frame, object);
        guards::guard(emitter, guard, x86_guard);
    }
    // -- run close hooks with both owners protected from escaping exceptions --
    for (direction, object, guard, x86_guard) in [(0, 8, 32, 64), (1, 16, 64, 32)] {
        let release = format!("__rt_user_filter_retire_{direction}");
        abi::emit_load_from_address(emitter, arg, frame, object);
        emitter.instruction(if arm { "cmp x0, #0" } else { "test rdi, rdi" });  // empty direction slots have no lifecycle callback
        emitter.instruction(&format!("{} {release}", if arm { "b.eq" } else { "jz" })); // retire even the empty guard in normal order
        abi::emit_load_from_address(emitter, if arm { "x9" } else { "r9" }, frame, 24);
        emitter.instruction(if arm { "cmp x9, #0" } else { "test r9, r9" });    // rejected creation never invokes the close hook
        emitter.instruction(&format!("{} {release}", if arm { "b.ne" } else { "jnz" })); // roll back without observing onClose
        abi::emit_load_symbol_to_reg(emitter, if arm { "x9" } else { "r9" }, "_stream_close_silent", 0);
        emitter.instruction(if arm { "cmp x9, #0" } else { "test r9, r9" });    // honor pending native-extension unwinding
        emitter.instruction(&format!("{} {release}", if arm { "b.ne" } else { "jnz" })); // skip PHP hooks while still releasing ownership
        emitter.instruction(if arm { "ldr x10, [x0]" } else { "mov r10, QWORD PTR [rdi]" }); // resolve the instance's class id
        abi::emit_symbol_address(emitter, if arm { "x11" } else { "r11" }, "_user_filter_vtable_ptrs");
        emitter.instruction(if arm { "ldr x11, [x11, x10, lsl #3]" } else { "mov r11, QWORD PTR [r11 + r10 * 8]" }); // borrow the class filter method table
        emitter.instruction(if arm { "ldr x11, [x11, #16]" } else { "mov r11, QWORD PTR [r11 + 16]" }); // slot two contains the optional onClose method
        emitter.instruction(if arm { "cmp x11, #0" } else { "test r11, r11" }); // detect an omitted lifecycle hook
        emitter.instruction(&format!("{} {release}", if arm { "b.eq" } else { "jz" })); // release objects without onClose as well
        abi::emit_call_reg(emitter, if arm { "x11" } else { "r11" });
        emitter.label(&release);
        // -- unlink normal cleanup before destructors can reenter the runtime --
        guards::unguard(emitter, guard, x86_guard);
        abi::emit_load_from_address(emitter, result, frame, object);
        abi::emit_call_label(emitter, "__rt_decref_object");
    }
    // -- restore the caller's descriptor and native frame --
    emitter.label("__rt_user_filter_retire_done");
    abi::emit_load_from_address(emitter, result, frame, 0);
    abi::emit_frame_restore(emitter, 112);
    abi::emit_return(emitter);
    emit_close_fd_guard(emitter);
}

/// Closes the native descriptor on an escaping filter hook or destructor exception during fclose.
/// The guard's payload is a raw descriptor, including zero, and its callback invokes no PHP.
fn emit_close_fd_guard(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let (arg, result, frame) = if arm { ("x0", "x0", "sp") } else { ("rdi", "rax", "rsp") };
    emitter.label_global("__rt_user_filter_close_fd");
    // -- ordinary descriptors need no exception guard when no filter object can run PHP --
    emitter.instruction(if arm { "add x10, x0, x0" } else { "lea r10, [rdi + rdi]" }); // select both user-filter direction entries
    abi::emit_symbol_address(emitter, if arm { "x9" } else { "r9" }, "_user_filter_instances");
    if arm {
        emitter.instruction("add x9, x9, x10, lsl #3");                         // derive the adjacent read/write instance pair
        emitter.instruction("ldp x11, x12, [x9]");                              // inspect both owners without consuming them
        emitter.instruction("orr x11, x11, x12");                               // either live instance can run a PHP lifecycle hook
        emitter.instruction("cbnz x11, __rt_user_filter_close_guard");          // guard descriptor ownership before running PHP
    } else {
        emitter.instruction("mov r11, QWORD PTR [r9 + r10 * 8]");               // inspect the read-direction instance
        emitter.instruction("or r11, QWORD PTR [r9 + r10 * 8 + 8]");            // include the write-direction instance
        emitter.instruction("jnz __rt_user_filter_close_guard");                // publish cleanup when either instance is live
    }
    abi::emit_jump(emitter, "__rt_user_filter_release_fd");
    emitter.label("__rt_user_filter_close_guard");
    // -- publish descriptor ownership before a close hook can escape --
    abi::emit_frame_prologue(emitter, 64);
    abi::emit_store_to_address(emitter, arg, frame, 0);
    abi::emit_store_to_address(emitter, arg, frame, 40);
    abi::emit_symbol_address(emitter, if arm { "x9" } else { "r9" }, "__rt_user_filter_close_unwind");
    abi::emit_store_to_address(emitter, if arm { "x9" } else { "r9" }, frame, 24);
    emitter.instruction(if arm { "add x0, sp, #16" } else { "lea rdi, [rsp + 16]" }); // identify the stack-local descriptor cleanup guard
    emitter.instruction(if arm { "mov x1, #0" } else { "xor esi, esi" });       // publish this guard as a new activation group
    abi::emit_call_label(emitter, "__rt_exception_guard_owned");
    abi::emit_load_from_address(emitter, arg, frame, 0);
    abi::emit_call_label(emitter, "__rt_user_filter_release_fd");
    // -- normal completion leaves the final native close to the existing fclose lowerer --
    guards::unguard(emitter, 16, 32);
    abi::emit_load_from_address(emitter, result, frame, 0);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);

    // -- exceptional completion retires filter state and closes the descriptor without PHP --
    emitter.label_global("__rt_user_filter_close_unwind");
    abi::emit_load_from_address(emitter, if arm { "x1" } else { "rax" }, arg, 24);
    abi::emit_load_int_immediate(emitter, if arm { "x9" } else { "r9" }, -1);
    abi::emit_store_to_address(emitter, if arm { "x9" } else { "r9" }, arg, 24);
    emitter.instruction(if arm { "mov x0, x1" } else { "mov rdi, rax" });       // transfer the descriptor into the native close ABI
    for symbol in ["_stream_read_filters", "_stream_write_filters"] {
        abi::emit_symbol_address(emitter, if arm { "x9" } else { "r9" }, symbol);
        emitter.instruction(if arm { "strb wzr, [x9, x0]" } else { "mov BYTE PTR [r9 + rdi], 0" }); // prevent filter IDs surviving descriptor reuse
    }
    if arm {
        emitter.syscall(6);
    } else {
        emitter.instruction("mov eax, 3");                                      // Linux x86_64 close consumes the descriptor in rdi
        emitter.instruction("syscall");                                         // release the descriptor without invoking PHP during unwind
    }
    abi::emit_return(emitter);
}
