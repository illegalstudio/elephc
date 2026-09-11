//! Purpose:
//! Releases every abandoned PHP-frame owner through protected, resumable cleanup steps.
//!
//! Called from:
//! - Native destructor return epilogues and PHP-frame exception activation callbacks.
//!
//! Key details:
//! - Each step borrows the original PHP frame and clears its owner slot before releasing it.
//! - Pending child exceptions replace or chain the active exception without skipping later owners.
//! - Raw reference-cell storage is freed after its payload cleanup, including when that cleanup throws.
//! - Tracked Mixed references release their managed wrapper, preserving any other identity owners.

use super::*;

/// One owner whose cleanup can execute PHP code while the abandoned frame remains readable.
enum Owner {
    Local(LocalSlotId, PhpType, usize),
    Reference(PhpType, usize),
    EvalScope(usize),
    EvalContext(usize),
}

/// Recognizes destructor bodies using the same case-insensitive method identity as PHP.
pub(in crate::codegen) fn is_destructor(function: &Function) -> bool {
    function.name.rsplit_once("::")
        .is_some_and(|(_, method)| method.eq_ignore_ascii_case("__destruct"))
}

/// Calls the complete cleanup walker on normal return and propagates throws after removing its activation.
pub(super) fn emit_call(ctx: &mut FunctionContext<'_>) {
    let entry = ctx.epilogue_label.as_deref().unwrap().strip_suffix("_epilogue").unwrap();
    let callback = format!("{entry}__cdylib_exception_cleanup");
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    ctx.emitter.instruction(if arm { "mov x0, x29" } else { "mov rdi, rbp" });  // borrow this PHP frame until all owned locals have been released
    abi::emit_call_label(ctx.emitter, &callback);
    let done = ctx.next_label("destructor_cleanup_complete");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &done);
    super::emit_exception_activation_pop(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_throw_current");
    ctx.emitter.label(&done);
}

/// Emits the shared normal/exceptional cleanup walker and its individual protected owner steps.
pub(super) fn emit_callback(ctx: &mut FunctionContext<'_>, entry: &str) {
    let callback = format!("{entry}__cdylib_exception_cleanup");
    let mut owners = super::ref_cell_owner_locals(ctx).into_iter()
        .map(|(_, _, ty, offset)| Owner::Reference(ty, offset)).collect::<Vec<_>>();
    owners.extend(super::function_cleanup_locals(ctx, None).into_iter()
        .map(|(_, slot, ty, offset)| Owner::Local(slot, ty, offset)));
    owners.extend(super::eval_scope_locals(ctx).into_iter().map(|(_, offset)| Owner::EvalScope(offset)));
    owners.extend(super::eval_context_locals(ctx).into_iter().map(|(_, offset)| Owner::EvalContext(offset)));
    ctx.emitter.label_global(&callback);
    enter_borrowed_frame(ctx.emitter);
    super::emit_instr_exit(ctx);
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    ctx.emitter.instruction(if arm { "str xzr, [sp]" } else { "mov QWORD PTR [rsp], 0" }); // accumulate child failures while continuing the cleanup walk
    for index in 0..owners.len() {
        let step = format!("{callback}_owner_{index}");
        abi::emit_symbol_address(ctx.emitter, if arm { "x0" } else { "rdi" }, &step);
        ctx.emitter.instruction(if arm { "mov x1, x29" } else { "mov rsi, rbp" }); // pass the still-readable original frame to this cleanup step
        ctx.emitter.instruction(if arm { "mov x2, sp" } else { "mov rdx, rsp" }); // retain pending status in this walker's own stack storage
        abi::emit_call_label(ctx.emitter, "__rt_cleanup_call");
    }
    ctx.emitter.instruction(if arm { "ldr x0, [sp]" } else { "mov rax, QWORD PTR [rsp]" }); // report whether any owner cleanup produced a throwable
    ctx.emitter.instruction(if arm { "lsl x0, x0, #1" } else { "shl eax, 1" }); // convert the cleanup flag to the shared pending status
    leave_borrowed_frame(ctx.emitter);
    for (index, owner) in owners.iter().enumerate() {
        let step = format!("{callback}_owner_{index}");
        ctx.emitter.label(&step);
        enter_borrowed_frame(ctx.emitter);
        match owner {
            Owner::Local(slot, ty, offset) => super::emit_owned_local_cleanup(ctx, *slot, *offset, ty),
            Owner::Reference(ty, offset) => emit_reference_cleanup(ctx, ty, *offset, &step),
            Owner::EvalScope(offset) => super::emit_eval_scope_cleanup(ctx, *offset),
            Owner::EvalContext(offset) => super::emit_eval_context_cleanup(ctx, *offset),
        }
        leave_borrowed_frame(ctx.emitter);
        if let Owner::Reference(ty, _) = owner {
            if ty.codegen_repr() != PhpType::Mixed {
                emit_reference_payload(ctx.emitter, ty, &format!("{step}_payload"));
            }
        }
    }
}

/// Preserves the caller's real stack while selecting a borrowed PHP frame for ordinary slot helpers.
fn enter_borrowed_frame(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("sub sp, sp, #48");                                 // reserve cleanup state and saved linkage outside the borrowed PHP frame
        emitter.instruction("stp x29, x30, [sp, #32]");                         // preserve caller linkage across protected cleanup calls
        emitter.instruction("str x19, [sp, #16]");                              // retain the caller's callee-saved stack anchor
        emitter.instruction("mov x19, sp");                                     // remember this helper's actual stack independently of the borrowed frame
        emitter.instruction("mov x29, x0");                                     // address the abandoned frame's original owned local slots
    } else {
        emitter.instruction("push rbp");                                        // retain the caller's frame pointer before borrowing the abandoned frame
        emitter.instruction("push r12");                                        // preserve the register used to anchor this helper's real stack
        emitter.instruction("sub rsp, 24");                                     // align nested calls and reserve cleanup state outside the PHP frame
        emitter.instruction("mov r12, rsp");                                    // remember the real helper stack across slot-based cleanup calls
        emitter.instruction("mov rbp, rdi");                                    // address the abandoned frame's original owned local slots
    }
}

/// Restores actual helper storage and caller registers while preserving the cleanup result.
fn leave_borrowed_frame(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("mov sp, x19");                                     // leave any borrowed-frame addressing behind before restoring linkage
        emitter.instruction("ldr x19, [sp, #16]");                              // restore the caller's stack-anchor register
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore the caller's frame and return address
        emitter.instruction("add sp, sp, #48");                                 // release only this helper's cleanup state
    } else {
        emitter.instruction("mov rsp, r12");                                    // recover this helper's real stack after borrowed-frame cleanup
        emitter.instruction("add rsp, 24");                                     // release aligned cleanup state
        emitter.instruction("pop r12");                                         // restore the caller's callee-saved stack anchor
        emitter.instruction("pop rbp");                                         // restore the caller's frame pointer
    }
    emitter.instruction("ret");                                                 // return without propagating a pending exception through the cleanup walker
}

/// Releases a managed reference or frees raw storage after protected payload destruction.
fn emit_reference_cleanup(ctx: &mut FunctionContext<'_>, ty: &PhpType, offset: usize, step: &str) {
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    let done = ctx.next_label("destructor_reference_cleanup_done");
    let result = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result, offset);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &done);
    abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
    if ty.codegen_repr() == PhpType::Mixed {
        abi::emit_release_local_ref_cell(ctx.emitter, result, ty);
        ctx.emitter.label(&done);
        return;
    }
    ctx.emitter.instruction(if arm { "str x0, [sp]" } else { "mov QWORD PTR [rsp], rax" }); // retain the raw cell owner until its payload cleanup finishes
    ctx.emitter.instruction(if arm { "str xzr, [sp, #8]" } else { "mov QWORD PTR [rsp + 8], 0" }); // keep this cell's pending flag separate from the enclosing owner walk
    if matches!(ty.codegen_repr(), PhpType::Str | PhpType::Callable) || ty.codegen_repr().is_refcounted() {
        ctx.emitter.instruction(if arm { "mov x1, x0" } else { "mov rsi, rax" }); // borrow the cell only while the payload step reads its owned value
        abi::emit_symbol_address(ctx.emitter, if arm { "x0" } else { "rdi" }, &format!("{step}_payload"));
        ctx.emitter.instruction(if arm { "add x2, sp, #8" } else { "lea rdx, [rsp + 8]" }); // contain a payload exception without abandoning the raw cell
        abi::emit_call_label(ctx.emitter, "__rt_cleanup_call");
    }
    ctx.emitter.instruction(if arm { "ldr x0, [sp]" } else { "mov rax, QWORD PTR [rsp]" }); // recover the raw cell after its payload owner was consumed
    abi::emit_call_label(ctx.emitter, "__rt_heap_free");
    ctx.emitter.instruction(if arm { "ldr x0, [sp, #8]" } else { "mov rax, QWORD PTR [rsp + 8]" }); // propagate payload failure only after the cell storage is freed
    abi::emit_branch_if_int_result_zero(ctx.emitter, &done);
    abi::emit_call_label(ctx.emitter, "__rt_throw_current");
    ctx.emitter.label(&done);
}

/// Releases one borrowed raw reference cell's payload using the authoritative target-aware value ABI.
fn emit_reference_payload(emitter: &mut Emitter, ty: &PhpType, symbol: &str) {
    emitter.label(symbol);
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.instruction(if arm { "stp x29, x30, [sp, #-16]!" } else { "sub rsp, 8" }); // preserve linkage and align payload cleanup calls
    let result = abi::int_result_reg(emitter);
    abi::emit_load_from_address(emitter, result, result, 0);
    match ty.codegen_repr() {
        PhpType::Str => abi::emit_call_label(emitter, "__rt_heap_free_safe"),
        other => abi::emit_decref_if_refcounted(emitter, &other),
    }
    emitter.instruction(if arm { "ldp x29, x30, [sp], #16" } else { "add rsp, 8" }); // restore linkage after successful payload cleanup
    emitter.instruction("ret");                                                 // transfer any exceptional exit to the enclosing protected caller
}
