//! Purpose:
//! Out-of-line entry, exit and error tails shared by every descriptor-invoker body.
//!
//! Called from:
//! - `emit_runtime_callable_invoker_impl` (the per-signature invoker bodies).
//!
//! Key details:
//! - A body used to carry its own copy of the frame setup (callee-saved saves, owner-slot
//!   clearing, exception-boundary record), the owner releases of both exits (one unrolled
//!   release per parameter) and the `ArgumentCountError` constructor. Measured on the Symfony
//!   `--web` build that was about 40% of the 2.8M instructions in 3,400 invoker bodies, none of
//!   it dependent on the signature except the owner COUNT and which owners are callable
//!   descriptors. The tails take both as run-time data (a count and a bit mask) and loop.
//! - The routines work in the CALLER'S frame: they are reached by `bl`/`call` or `b`/`jmp` from a
//!   body whose frame pointer is still live, address the fixed invoker slots relative to it, and
//!   scratch the callee-saved registers the body already saved. They never create a frame of
//!   their own, so `setjmp` (which must run in the body) and the unwinder's view of the stack are
//!   unchanged.
//! - Owner slot `i` lives at `OWNER_FIRST + 8*i` below the frame pointer; slot `count` holds the
//!   in-flight boxed result and slot `count + 1` the pending exception, as in
//!   `InvokerArgumentOwners`. The mask has bit `i` set for a raw callable descriptor owner, which
//!   needs `__rt_callable_descriptor_release` instead of `__rt_decref_any`.

use super::{
    abi, emit_consume_invoker_boundary_request, emit_invoker_callee_saved_restores,
    emit_invoker_callee_saved_saves, emit_invoker_exception_boundary_pop,
    emit_invoker_exception_boundary_save, Arch, DataSection, Emitter, INVOKER_ARG_ARRAY_OFFSET,
    INVOKER_BOUNDARY_BASE_OFFSET, INVOKER_BOUNDARY_FRAME_SIZE, INVOKER_DESCRIPTOR_OFFSET,
    INVOKER_SCOPE_OFFSET, INVOKER_TABLE_OFFSET,
};

pub(super) const INVOKER_ENTER: &str = "_eir_invoker_enter";
pub(super) const INVOKER_FINISH: &str = "_eir_invoker_finish";
pub(super) const INVOKER_ESCAPE: &str = "_eir_invoker_escape";
pub(super) const INVOKER_LEAVE: &str = "_eir_invoker_leave";
pub(super) const INVOKER_LEAVE_THROW: &str = "_eir_invoker_leave_throw";
pub(super) const INVOKER_MISSING_ARGUMENT: &str = "_eir_invoker_missing_argument_error";

/// Most owner slots whose descriptor-ness fits the 64-bit mask (bit `count` stays clear).
pub(super) const MAX_SHARED_OWNERS: usize = 62;

/// Frame offset of owner slot 0.
const OWNER_FIRST: usize = INVOKER_BOUNDARY_FRAME_SIZE - 8;

/// Whether the shared tails are in use (`ELEPHC_NO_SHARED_INVOKER_TAILS=1` restores the inline
/// copies, for size comparisons).
pub(super) fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("ELEPHC_NO_SHARED_INVOKER_TAILS").is_none())
}

/// Emits every shared tail once per module (the helper markers deduplicate across workers).
pub(super) fn emit_once(emitter: &mut Emitter, data: &mut DataSection) {
    let open = crate::codegen::context::HELPER_MARKER_OPEN;
    let close = crate::codegen::context::HELPER_MARKER_CLOSE;
    emitter.emit_keyed_once(INVOKER_ENTER, open, close, emit_enter);
    emitter.emit_keyed_once(INVOKER_FINISH, open, close, emit_finish);
    emitter.emit_keyed_once(INVOKER_ESCAPE, open, close, emit_escape);
    emitter.emit_keyed_once(INVOKER_LEAVE, open, close, |emitter| emit_leave(emitter, false));
    emitter.emit_keyed_once(INVOKER_LEAVE_THROW, open, close, |emitter| emit_leave(emitter, true));
    emitter.emit_keyed_once(INVOKER_MISSING_ARGUMENT, open, close, |emitter| {
        emit_missing_argument_error(emitter, data)
    });
}

/// Starts one shared routine: alignment, global label.
fn begin(emitter: &mut Emitter, label: &str, what: &str) {
    emitter.blank();
    emitter.comment(what);
    emitter.raw(".align 2");
    emitter.label_global(label);
}

/// `_eir_invoker_enter(descriptor, container, scope, owner count)` with the name table in the
/// table register: everything the body did between its frame prologue and `setjmp`.
fn emit_enter(emitter: &mut Emitter) {
    begin(emitter, INVOKER_ENTER, "shared descriptor-invoker entry: saves, owner slots, boundary record");
    let arch = emitter.target.arch;
    abi::store_at_offset(emitter, super::invoker_table_reg(arch), INVOKER_TABLE_OFFSET);
    emit_invoker_callee_saved_saves(emitter);
    emit_consume_invoker_boundary_request(emitter);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), INVOKER_DESCRIPTOR_OFFSET);
    // Clear owner slots 0..=count+1.
    match arch {
        Arch::AArch64 => {
            abi::emit_frame_slot_address(emitter, "x5", OWNER_FIRST);
            emitter.instruction("add x4, x3, #2");                              // owner slots plus the boxed-result and pending slots
            let clear = "_eir_invoker_enter_clear";
            emitter.label(clear);
            emitter.instruction("str xzr, [x5], #-8");                          // empty cleanup marker, next slot down
            emitter.instruction("subs x4, x4, #1");                             // one slot fewer
            emitter.instruction(&format!("b.ne {clear}"));                      // until every slot is empty
        }
        Arch::X86_64 => {
            abi::emit_frame_slot_address(emitter, "r8", OWNER_FIRST);
            emitter.instruction("lea r9, [rcx + 2]");                           // owner slots plus the boxed-result and pending slots
            let clear = "_eir_invoker_enter_clear";
            emitter.label(clear);
            emitter.instruction("mov QWORD PTR [r8], 0");                       // empty cleanup marker
            emitter.instruction("sub r8, 8");                                   // next slot down
            emitter.instruction("dec r9");                                      // one slot fewer
            emitter.instruction(&format!("jnz {clear}"));                       // until every slot is empty
        }
    }
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), INVOKER_ARG_ARRAY_OFFSET);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 2), INVOKER_SCOPE_OFFSET);
    emit_invoker_exception_boundary_save(emitter, INVOKER_BOUNDARY_BASE_OFFSET);
    abi::emit_return(emitter);
}

/// `_eir_invoker_finish(result, owner count, descriptor mask)`: the normal exit. Releases every
/// owner (clearing each slot first), pops the exception boundary and answers the boxed result.
fn emit_finish(emitter: &mut Emitter) {
    begin(emitter, INVOKER_FINISH, "shared descriptor-invoker normal exit: release owners, pop boundary");
    let desc = "_eir_invoker_finish_descriptor";
    let next = "_eir_invoker_finish_next";
    let lp = "_eir_invoker_finish_loop";
    let done = "_eir_invoker_finish_done";
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x30, [sp, #-16]!");                        // keep the return address across the release calls
            abi::emit_frame_slot_address(emitter, "x19", OWNER_FIRST);
            emitter.instruction("mov x20, x1");                                 // owner count
            emitter.instruction("mov x21, x2");                                 // descriptor mask
            emitter.instruction("sub x22, x19, x20, lsl #3");                   // address of the boxed-result slot
            emitter.instruction("str x0, [x22]");                               // root the result while the owners are released
            emitter.instruction("mov x23, #0");                                 // owner index
            emitter.label(lp);
            emitter.instruction("cmp x23, x20");                                // every owner released?
            emitter.instruction(&format!("b.ge {done}"));                       // yes
            emitter.instruction("sub x9, x19, x23, lsl #3");                    // this owner's slot
            emitter.instruction("ldr x0, [x9]");                                // the owned value (or empty)
            emitter.instruction("str xzr, [x9]");                               // clear before release so re-entrant cleanup cannot see it twice
            emitter.instruction("lsr x10, x21, x23");                           // this owner's descriptor bit
            emitter.instruction(&format!("tbnz x10, #0, {desc}"));              // a raw callable descriptor
            emitter.instruction("bl __rt_decref_any");                          // release a refcounted value
            emitter.instruction(&format!("b {next}"));                          // next owner
            emitter.label(desc);
            emitter.instruction("bl __rt_callable_descriptor_release");         // release a raw callable descriptor
            emitter.label(next);
            emitter.instruction("add x23, x23, #1");                            // next owner
            emitter.instruction(&format!("b {lp}"));                            // keep releasing
            emitter.label(done);
            emitter.instruction("ldr x0, [x22]");                               // take the boxed result back
            emitter.instruction("str xzr, [x22]");                              // retire the result root
            emit_invoker_exception_boundary_pop(emitter, INVOKER_BOUNDARY_BASE_OFFSET);
            emitter.instruction("ldr x30, [sp], #16");                          // restore the return address
            emitter.instruction("ret");                                         // back to the body
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 8");                                  // keep the stack 16-byte aligned for the release calls
            abi::emit_frame_slot_address(emitter, "r14", OWNER_FIRST);
            emitter.instruction("mov r12, rsi");                                // owner count
            emitter.instruction("mov r13, rdx");                                // descriptor mask
            emitter.instruction("mov rcx, r12");                                // owner count
            emitter.instruction("shl rcx, 3");                                  // byte offset of the boxed-result slot
            emitter.instruction("mov r15, r14");                                // first owner slot
            emitter.instruction("sub r15, rcx");                                // address of the boxed-result slot
            emitter.instruction("mov QWORD PTR [r15], rax");                    // root the result while the owners are released
            emitter.instruction("xor ebx, ebx");                                // owner index
            emitter.label(lp);
            emitter.instruction("cmp rbx, r12");                                // every owner released?
            emitter.instruction(&format!("jge {done}"));                        // yes
            emitter.instruction("mov rcx, rbx");                                // owner index
            emitter.instruction("shl rcx, 3");                                  // byte offset of this owner's slot
            emitter.instruction("mov r8, r14");                                 // first owner slot
            emitter.instruction("sub r8, rcx");                                 // this owner's slot
            emitter.instruction("mov rax, QWORD PTR [r8]");                     // the owned value (or empty)
            emitter.instruction("mov QWORD PTR [r8], 0");                       // clear before release so re-entrant cleanup cannot see it twice
            emitter.instruction("mov rcx, rbx");                                // owner index as the shift count
            emitter.instruction("mov rdx, r13");                                // descriptor mask
            emitter.instruction("shr rdx, cl");                                 // this owner's descriptor bit
            emitter.instruction("test dl, 1");                                  // a raw callable descriptor?
            emitter.instruction(&format!("jnz {desc}"));                        // yes
            emitter.instruction("call __rt_decref_any");                        // release a refcounted value
            emitter.instruction(&format!("jmp {next}"));                        // next owner
            emitter.label(desc);
            emitter.instruction("call __rt_callable_descriptor_release");       // release a raw callable descriptor
            emitter.label(next);
            emitter.instruction("inc rbx");                                     // next owner
            emitter.instruction(&format!("jmp {lp}"));                          // keep releasing
            emitter.label(done);
            emitter.instruction("mov rax, QWORD PTR [r15]");                    // take the boxed result back
            emitter.instruction("mov QWORD PTR [r15], 0");                      // retire the result root
            emit_invoker_exception_boundary_pop(emitter, INVOKER_BOUNDARY_BASE_OFFSET);
            emitter.instruction("add rsp, 8");                                  // drop the alignment pad
            emitter.instruction("ret");                                         // back to the body
        }
    }
}

/// `_eir_invoker_escape(_, owner count, descriptor mask)`: the exception exit. Pops the boundary,
/// then releases every owner through the guarded cleanup call, accumulating destructor throws in
/// the pending slot, and re-arms the pending exception.
fn emit_escape(emitter: &mut Emitter) {
    begin(emitter, INVOKER_ESCAPE, "shared descriptor-invoker escape: pop boundary, release owners guarded");
    let desc = "_eir_invoker_escape_descriptor";
    let call = "_eir_invoker_escape_call";
    let lp = "_eir_invoker_escape_loop";
    let done = "_eir_invoker_escape_done";
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x30, [sp, #-16]!");                        // keep the return address across the cleanup calls
            emit_invoker_exception_boundary_pop(emitter, INVOKER_BOUNDARY_BASE_OFFSET);
            abi::emit_frame_slot_address(emitter, "x19", OWNER_FIRST);
            emitter.instruction("mov x20, x1");                                 // owner count
            emitter.instruction("mov x21, x2");                                 // descriptor mask
            emitter.instruction("add x9, x20, #1");                             // the pending slot follows the boxed-result slot
            emitter.instruction("sub x22, x19, x9, lsl #3");                    // address of the pending-exception slot
            abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0);
            emitter.instruction("str x0, [x22]");                               // park the in-flight exception
            abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
            emitter.instruction("mov x23, #0");                                 // owner index (the boxed-result slot included)
            emitter.label(lp);
            emitter.instruction("cmp x23, x20");                                // past the boxed-result slot?
            emitter.instruction(&format!("b.gt {done}"));                       // yes
            emitter.instruction("sub x9, x19, x23, lsl #3");                    // this owner's slot
            emitter.instruction("ldr x1, [x9]");                                // the owned value (or empty)
            emitter.instruction("str xzr, [x9]");                               // clear before release so re-entrant cleanup cannot see it twice
            emitter.instruction("lsr x10, x21, x23");                           // this owner's descriptor bit
            emitter.instruction(&format!("tbnz x10, #0, {desc}"));              // a raw callable descriptor
            abi::emit_symbol_address(emitter, "x0", "__rt_decref_any");
            emitter.instruction(&format!("b {call}"));                          // run the guarded release
            emitter.label(desc);
            abi::emit_symbol_address(emitter, "x0", "__rt_callable_descriptor_release");
            emitter.label(call);
            emitter.instruction("mov x2, x22");                                 // the pending-exception slot
            emitter.instruction("bl __rt_cleanup_invoke");                      // release under a guard that chains destructor throws
            emitter.instruction("add x23, x23, #1");                            // next owner
            emitter.instruction(&format!("b {lp}"));                            // keep releasing
            emitter.label(done);
            emitter.instruction("ldr x0, [x22]");                               // the exception to re-arm
            abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);
            emitter.instruction("ldr x30, [sp], #16");                          // restore the return address
            emitter.instruction("ret");                                         // back to the body
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 8");                                  // keep the stack 16-byte aligned for the cleanup calls
            emit_invoker_exception_boundary_pop(emitter, INVOKER_BOUNDARY_BASE_OFFSET);
            abi::emit_frame_slot_address(emitter, "r14", OWNER_FIRST);
            emitter.instruction("mov r12, rsi");                                // owner count
            emitter.instruction("mov r13, rdx");                                // descriptor mask
            emitter.instruction("lea rcx, [r12 + 1]");                          // the pending slot follows the boxed-result slot
            emitter.instruction("shl rcx, 3");                                  // byte offset of the pending-exception slot
            emitter.instruction("mov r15, r14");                                // first owner slot
            emitter.instruction("sub r15, rcx");                                // address of the pending-exception slot
            abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0);
            emitter.instruction("mov QWORD PTR [r15], rax");                    // park the in-flight exception
            abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
            emitter.instruction("xor ebx, ebx");                                // owner index (the boxed-result slot included)
            emitter.label(lp);
            emitter.instruction("cmp rbx, r12");                                // past the boxed-result slot?
            emitter.instruction(&format!("jg {done}"));                         // yes
            emitter.instruction("mov rcx, rbx");                                // owner index
            emitter.instruction("shl rcx, 3");                                  // byte offset of this owner's slot
            emitter.instruction("mov r8, r14");                                 // first owner slot
            emitter.instruction("sub r8, rcx");                                 // this owner's slot
            emitter.instruction("mov rsi, QWORD PTR [r8]");                     // the owned value (or empty)
            emitter.instruction("mov QWORD PTR [r8], 0");                       // clear before release so re-entrant cleanup cannot see it twice
            emitter.instruction("mov rcx, rbx");                                // owner index as the shift count
            emitter.instruction("mov rdx, r13");                                // descriptor mask
            emitter.instruction("shr rdx, cl");                                 // this owner's descriptor bit
            emitter.instruction("test dl, 1");                                  // a raw callable descriptor?
            emitter.instruction(&format!("jnz {desc}"));                        // yes
            abi::emit_symbol_address(emitter, "rdi", "__rt_decref_any");
            emitter.instruction(&format!("jmp {call}"));                        // run the guarded release
            emitter.label(desc);
            abi::emit_symbol_address(emitter, "rdi", "__rt_callable_descriptor_release");
            emitter.label(call);
            emitter.instruction("mov rdx, r15");                                // the pending-exception slot
            emitter.instruction("call __rt_cleanup_invoke");                    // release under a guard that chains destructor throws
            emitter.instruction("inc rbx");                                     // next owner
            emitter.instruction(&format!("jmp {lp}"));                          // keep releasing
            emitter.label(done);
            emitter.instruction("mov rax, QWORD PTR [r15]");                    // the exception to re-arm
            abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
            emitter.instruction("add rsp, 8");                                  // drop the alignment pad
            emitter.instruction("ret");                                         // back to the body
        }
    }
}

/// `_eir_invoker_leave` / `_eir_invoker_leave_throw`: restore the caller's callee-saved registers,
/// tear the body's frame down and return (or rethrow the pending exception).
fn emit_leave(emitter: &mut Emitter, rethrow: bool) {
    let (label, what) = if rethrow {
        (INVOKER_LEAVE_THROW, "shared descriptor-invoker exit: unwind and rethrow")
    } else {
        (INVOKER_LEAVE, "shared descriptor-invoker exit: unwind and return")
    };
    begin(emitter, label, what);
    emit_invoker_callee_saved_restores(emitter);
    abi::emit_frame_restore(emitter, INVOKER_BOUNDARY_FRAME_SIZE);
    if rethrow {
        abi::emit_jump(emitter, "__rt_throw_current");
    } else {
        abi::emit_return(emitter);
    }
}

/// `_eir_invoker_missing_argument_error`: throws the catchable `ArgumentCountError` of a
/// descriptor call that omitted a required argument. Never returns.
fn emit_missing_argument_error(emitter: &mut Emitter, data: &mut DataSection) {
    begin(emitter, INVOKER_MISSING_ARGUMENT, "shared descriptor-invoker missing-argument error");
    super::emit_call_user_func_array_missing_arg_error_inline(emitter, data);
}
