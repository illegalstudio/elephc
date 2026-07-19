//! Purpose:
//! Emits `__rt_pdo_call_collation`, the codegen adapter that re-enters a
//! compiled-PHP collation comparator on behalf of the `elephc-pdo` bridge. It is
//! the runtime half of the PDO Tier-D "decompose-at-PHP" design: the bridge stores
//! this adapter's address (obtained through `__elephc_pdo_adapter_addr(0)`) together
//! with the callable's descriptor pointer per SQLite registration, and its
//! `x_compare` dispatcher calls back here with the two byte buffers SQLite provides.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::pdo`, gated by `RuntimeFeatures::pdo_udf`.
//! - The `elephc-pdo` bridge (`SqliteConn::create_collation` → `x_compare`), which
//!   only stores and calls this adapter's address; it never references a `__rt_*`
//!   symbol itself (respecting the non-whole-archive linker boundary).
//!
//! Key details:
//! - C ABI: `__rt_pdo_call_collation(descriptor, a_ptr, a_len, b_ptr, b_len) -> i64`
//!   returning the comparison sign; the bridge clamps it to -1/0/1.
//! - Marshalling mirrors `__rt_http_fire_notification` (the offset-56 uniform
//!   invoker path): the two transient SQLite `(ptr, len)` buffers are boxed as owned
//!   Mixed strings via `__rt_mixed_from_value` (tag 1), which deep-copies through
//!   `__rt_str_persist`, so SQLite's buffers may be invalidated the moment this
//!   returns. The boxed strings fill a two-slot `value_type = 7` (Mixed) indexed
//!   array, which is boxed as a Mixed cell and passed as the invoker's argument
//!   container. The owned boxed return is `__rt_mixed_cast_int`-ed to the sign and
//!   released, then the container (cell + raw array + element boxes) is released.
//! - Exception firewall: a compiled-PHP `throw` is a `longjmp` to the nearest
//!   handler. Without interception it would `longjmp` over SQLite's VDBE and this
//!   Rust/bridge frame — leaving the bridge mutex locked (deadlock), running
//!   `Drop` across a `longjmp` (UB), or terminating the process. So the adapter
//!   pushes its own `setjmp` handler record (identical 224-byte layout to the EIR
//!   try/catch slot) around the invoke: on a normal return it pops the handler and
//!   returns the comparator sign; on a `longjmp` it pops the handler, swallows the
//!   pending exception (SQLite's `xCompare` has no error channel), and returns 0
//!   (equal). Surfacing the exception at the query boundary is a later hardening
//!   step; the load-bearing guarantee here is that the `throw` never unwinds past
//!   this C boundary.

use crate::codegen_support::callable_descriptor::CALLABLE_DESC_INVOKER_OFFSET;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

// The firewall builds its handler record by hand and must match the layout the EIR
// try/catch machinery and `__rt_throw_current` assume: next@0, survivor@8,
// diag@TRY_HANDLER_DIAG_DEPTH_OFFSET, jmp_buf@TRY_HANDLER_JMP_BUF_OFFSET. Assert the
// two ABI-critical offsets at compile time so a constant drift breaks the build
// here rather than corrupting a `longjmp` at runtime.
const _: () = assert!(TRY_HANDLER_DIAG_DEPTH_OFFSET == 16);
const _: () = assert!(TRY_HANDLER_JMP_BUF_OFFSET == 24);

/// Emits `__rt_pdo_call_collation(descriptor, a_ptr, a_len, b_ptr, b_len) -> sign`.
///
/// Inputs (AArch64): x0 = descriptor, x1 = a_ptr, x2 = a_len, x3 = b_ptr,
/// x4 = b_len. Result in x0 = comparison sign (bridge clamps to -1/0/1).
/// (x86_64): rdi, rsi, rdx, rcx, r8 in the same order; result in rax.
pub fn emit_pdo_call_collation(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pdo_call_collation_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: pdo_call_collation ---");
    emitter.label_global("__rt_pdo_call_collation");

    // Stack frame (320 bytes):
    //   [sp, #0]   = handler record (224 bytes): next@0, survivor@8, diag@16,
    //                jmp_buf@24 (matches TRY_HANDLER_* / __rt_throw_current).
    //   [sp, #224] = descriptor      [sp, #232] = a_ptr      [sp, #240] = a_len
    //   [sp, #248] = b_ptr           [sp, #256] = b_len
    //   [sp, #264] = args array ptr  [sp, #272] = boxed args cell
    //   [sp, #280] = boxed return    [sp, #288] = comparator sign
    //   [sp, #304] = saved x29       [sp, #312] = saved x30
    emitter.instruction("sub sp, sp, #320"); // allocate the collation-adapter frame
    emitter.instruction("stp x29, x30, [sp, #304]"); // save frame pointer and return address
    emitter.instruction("add x29, sp, #304"); // establish the adapter frame pointer

    emitter.instruction("str x0, [sp, #224]"); // save descriptor pointer
    emitter.instruction("str x1, [sp, #232]"); // save a_ptr
    emitter.instruction("str x2, [sp, #240]"); // save a_len
    emitter.instruction("str x3, [sp, #248]"); // save b_ptr
    emitter.instruction("str x4, [sp, #256]"); // save b_len

    // -- fast path: a descriptor without a uniform invoker compares as equal --
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the invoker slot
    emitter.instruction("cbz x9, __rt_pdo_call_collation_ret_zero"); // no invoker → sign 0, nothing to release

    // -- allocate a 2-slot argument array and stamp value_type = Mixed once --
    emitter.instruction("mov x0, #2"); // capacity: two comparator arguments
    emitter.instruction("mov x1, #8"); // boxed Mixed slots store one pointer each
    emitter.instruction("bl __rt_array_new"); // x0 = indexed array backing storage
    emitter.instruction("ldr x10, [x0, #-8]"); // load the packed array kind word from the header
    emitter.instruction("mov x12, #0x80ff"); // preserve the indexed-array kind and persistent COW flag
    emitter.instruction("and x10, x10, x12"); // keep only the persistent metadata bits
    emitter.instruction("mov x11, #7"); // value_type tag 7 = boxed Mixed
    emitter.instruction("lsl x11, x11, #8"); // move the tag into the packed kind-word byte lane
    emitter.instruction("orr x10, x10, x11"); // combine the heap kind with the value_type tag
    emitter.instruction("str x10, [x0, #-8]"); // persist the stamped kind word (never re-stamped: no push_int)
    emitter.instruction("str x0, [sp, #264]"); // save the args array pointer

    // -- slot 0: string a (from_value tag 1 persists the bytes into an owned copy) --
    emitter.instruction("mov x0, #1"); // runtime tag 1 = string
    emitter.instruction("ldr x1, [sp, #232]"); // value_lo = a_ptr
    emitter.instruction("ldr x2, [sp, #240]"); // value_hi = a_len
    emitter.instruction("bl __rt_mixed_from_value"); // x0 = boxed Mixed(string) a
    emitter.instruction("ldr x9, [sp, #264]"); // reload the args array pointer
    emitter.instruction("str x0, [x9, #24]"); // store boxed a into slot 0 (data region + 0)
    emitter.instruction("mov x10, #1"); // running element count = 1
    emitter.instruction("str x10, [x9]"); // update the array length field

    // -- slot 1: string b --
    emitter.instruction("mov x0, #1"); // runtime tag 1 = string
    emitter.instruction("ldr x1, [sp, #248]"); // value_lo = b_ptr
    emitter.instruction("ldr x2, [sp, #256]"); // value_hi = b_len
    emitter.instruction("bl __rt_mixed_from_value"); // x0 = boxed Mixed(string) b
    emitter.instruction("ldr x9, [sp, #264]"); // reload the args array pointer
    emitter.instruction("str x0, [x9, #32]"); // store boxed b into slot 1 (data region + 8)
    emitter.instruction("mov x10, #2"); // running element count = 2
    emitter.instruction("str x10, [x9]"); // update the array length field

    // -- box the indexed array as a Mixed cell (tag 4 increfs the array) --
    emitter.instruction("ldr x1, [sp, #264]"); // raw args array pointer → payload lo
    emitter.instruction("mov x2, #0"); // payload hi unused for an array
    emitter.instruction("mov x0, #4"); // runtime tag 4 = indexed array
    emitter.instruction("bl __rt_mixed_from_value"); // x0 = boxed Mixed argument cell
    emitter.instruction("str x0, [sp, #272]"); // save the boxed Mixed argument cell

    // -- push a setjmp firewall handler around the invoke --
    // record.next = _exc_handler_top
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction("str x10, [sp, #0]"); // handler record: previous handler-stack top
    // record.survivor = live _exc_call_frame_top → cleanup stops at this C boundary
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
    emitter.instruction("str x10, [sp, #8]"); // handler record: activation frame to survive a throw
    // record.diag = _rt_diag_suppression
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction("str x10, [sp, #16]"); // handler record: saved diagnostic-suppression depth
    // _exc_handler_top = &record (record base = sp + 0)
    emitter.instruction("mov x10, sp"); // x10 = address of this handler record
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // link the record as the active handler
    // setjmp(&jmp_buf) where jmp_buf = record + 24
    emitter.instruction("add x0, sp, #24"); // x0 = &jmp_buf inside the handler record
    emitter.bl_c("setjmp"); // returns 0 on first pass, 1 when a throw longjmps back
    emitter.instruction("cbnz x0, __rt_pdo_call_collation_threw"); // nonzero → arrived via longjmp

    // -- normal path: invoke the comparator through its descriptor (offset 56) --
    emitter.instruction("ldr x0, [sp, #224]"); // arg0 = descriptor pointer
    emitter.instruction("ldr x1, [sp, #272]"); // arg1 = boxed Mixed argument cell
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the uniform invoker pointer
    emitter.instruction("blr x9"); // invoke comparator($a, $b) → OWNED boxed Mixed return in x0
    emitter.instruction("str x0, [sp, #280]"); // save the boxed return for later release

    // pop the firewall handler before any further runtime calls
    emitter.instruction("ldr x10, [sp, #0]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("ldr x10, [sp, #16]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0); // restore it

    // extract the comparator sign (borrows), then release the owned return
    emitter.instruction("ldr x0, [sp, #280]"); // boxed return
    emitter.instruction("bl __rt_mixed_cast_int"); // x0 = raw i64 comparator sign (PHP (int) rules)
    emitter.instruction("str x0, [sp, #288]"); // save the sign
    emitter.instruction("ldr x0, [sp, #280]"); // boxed return
    emitter.instruction("bl __rt_decref_mixed"); // release the invoker's owned return
    emitter.instruction("b __rt_pdo_call_collation_cleanup"); // join the shared container-release path

    // -- longjmp path: a throw crossed the invoke --
    emitter.label("__rt_pdo_call_collation_threw");
    emitter.instruction("ldr x10, [sp, #0]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("ldr x10, [sp, #16]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0); // restore it
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0); // swallow the pending exception (no xCompare error channel)
    emitter.instruction("str xzr, [sp, #288]"); // comparator sign = 0 (treat as equal)

    // -- shared cleanup: release the argument container --
    emitter.label("__rt_pdo_call_collation_cleanup");
    emitter.instruction("ldr x0, [sp, #272]"); // boxed Mixed argument cell
    emitter.instruction("bl __rt_decref_mixed"); // release the cell (drops the array ref boxing took)
    emitter.instruction("ldr x0, [sp, #264]"); // raw args array pointer
    emitter.instruction("bl __rt_decref_any"); // release the array and deep-free its two boxed strings
    emitter.instruction("ldr x0, [sp, #288]"); // load the comparator sign into the result register
    emitter.instruction("ldp x29, x30, [sp, #304]"); // restore frame pointer and return address
    emitter.instruction("add sp, sp, #320"); // release the adapter frame
    emitter.instruction("ret"); // return the comparison sign to the bridge dispatcher

    // -- fast path: no uniform invoker → equal, with nothing allocated to release --
    emitter.label("__rt_pdo_call_collation_ret_zero");
    emitter.instruction("mov x0, #0"); // comparator sign = 0 (treat as equal)
    emitter.instruction("ldp x29, x30, [sp, #304]"); // restore frame pointer and return address
    emitter.instruction("add sp, sp, #320"); // release the adapter frame
    emitter.instruction("ret"); // return to the bridge dispatcher
}

/// x86_64 implementation of `__rt_pdo_call_collation`.
fn emit_pdo_call_collation_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pdo_call_collation ---");
    emitter.label_global("__rt_pdo_call_collation");

    // Frame (304 bytes below rbp):
    //   [rbp-8]   descriptor   [rbp-16] a_ptr    [rbp-24] a_len
    //   [rbp-32]  b_ptr        [rbp-40] b_len
    //   [rbp-48]  args array   [rbp-56] boxed args cell
    //   [rbp-64]  boxed return [rbp-72] comparator sign
    //   [rbp-296] handler record (224 bytes): next@0, survivor@8, diag@16, jmp_buf@24
    //             → record.next=[rbp-296], survivor=[rbp-288], diag=[rbp-280],
    //               jmp_buf base=[rbp-272].
    //   push rbp + sub rsp,304 keeps rsp 16-aligned for the nested calls.
    emitter.instruction("push rbp"); // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp"); // establish the adapter frame pointer
    emitter.instruction("sub rsp, 304"); // reserve the slots and the 224-byte handler record

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi"); // save descriptor pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi"); // save a_ptr
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx"); // save a_len
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx"); // save b_ptr
    emitter.instruction("mov QWORD PTR [rbp - 40], r8"); // save b_len

    // -- fast path: a descriptor without a uniform invoker compares as equal --
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the invoker slot
    emitter.instruction("test r10, r10"); // does the descriptor expose a uniform invoker?
    emitter.instruction("jz __rt_pdo_call_collation_ret_zero_x86"); // no invoker → sign 0, nothing to release

    // -- allocate a 2-slot argument array and stamp value_type = Mixed once --
    emitter.instruction("mov rdi, 2"); // capacity: two comparator arguments
    emitter.instruction("mov rsi, 8"); // boxed Mixed slots store one pointer each
    emitter.instruction("call __rt_array_new"); // rax = indexed array backing storage
    emitter.instruction("mov r10, QWORD PTR [rax - 8]"); // load the packed array kind word from the header
    emitter.instruction("mov r11, 0xffffffff000080ff"); // preserve heap marker + indexed-array kind + COW bit
    emitter.instruction("and r10, r11"); // keep only the persistent metadata bits
    emitter.instruction("mov r11, 7"); // value_type tag 7 = boxed Mixed
    emitter.instruction("shl r11, 8"); // move the tag into the packed kind-word byte lane
    emitter.instruction("or r10, r11"); // combine the heap kind with the value_type tag
    emitter.instruction("mov QWORD PTR [rax - 8], r10"); // persist the stamped kind word (never re-stamped: no push_int)
    emitter.instruction("mov QWORD PTR [rbp - 48], rax"); // save the args array pointer

    // -- slot 0: string a (from_value tag 1 persists the bytes into an owned copy) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]"); // value_lo = a_ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]"); // value_hi = a_len
    emitter.instruction("mov eax, 1"); // runtime tag 1 = string (tag goes in RAX)
    emitter.instruction("call __rt_mixed_from_value"); // rax = boxed Mixed(string) a
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]"); // reload the args array pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], rax"); // store boxed a into slot 0 (data region + 0)
    emitter.instruction("mov QWORD PTR [r10], 1"); // update the array length field to 1

    // -- slot 1: string b --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]"); // value_lo = b_ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]"); // value_hi = b_len
    emitter.instruction("mov eax, 1"); // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value"); // rax = boxed Mixed(string) b
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]"); // reload the args array pointer
    emitter.instruction("mov QWORD PTR [r10 + 32], rax"); // store boxed b into slot 1 (data region + 8)
    emitter.instruction("mov QWORD PTR [r10], 2"); // update the array length field to 2

    // -- box the indexed array as a Mixed cell (tag 4 increfs the array) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]"); // raw args array pointer → payload lo
    emitter.instruction("xor esi, esi"); // payload hi unused for an array
    emitter.instruction("mov eax, 4"); // runtime tag 4 = indexed array
    emitter.instruction("call __rt_mixed_from_value"); // rax = boxed Mixed argument cell
    emitter.instruction("mov QWORD PTR [rbp - 56], rax"); // save the boxed Mixed argument cell

    // -- push a setjmp firewall handler around the invoke --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0); // previous handler-stack top
    emitter.instruction("mov QWORD PTR [rbp - 296], r10"); // handler record: record.next
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0); // live activation-frame top
    emitter.instruction("mov QWORD PTR [rbp - 288], r10"); // handler record: survivor frame (cleanup stops here)
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0); // current diagnostic-suppression depth
    emitter.instruction("mov QWORD PTR [rbp - 280], r10"); // handler record: saved diagnostic depth
    emitter.instruction("lea r10, [rbp - 296]"); // r10 = address of this handler record
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // link the record as the active handler
    emitter.instruction("lea rdi, [rbp - 272]"); // rdi = &jmp_buf inside the handler record (record + 24)
    emitter.bl_c("setjmp"); // returns 0 on first pass, 1 when a throw longjmps back
    emitter.instruction("test rax, rax"); // did control arrive via longjmp?
    emitter.instruction("jne __rt_pdo_call_collation_threw_x86"); // nonzero → arrived via longjmp

    // -- normal path: invoke the comparator through its descriptor (offset 56) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]"); // arg0 = descriptor pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]"); // arg1 = boxed Mixed argument cell
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the uniform invoker pointer
    emitter.instruction("call r10"); // invoke comparator($a, $b) → OWNED boxed Mixed return in rax
    emitter.instruction("mov QWORD PTR [rbp - 64], rax"); // save the boxed return for later release

    // pop the firewall handler before any further runtime calls
    emitter.instruction("mov r10, QWORD PTR [rbp - 296]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("mov r10, QWORD PTR [rbp - 280]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0); // restore it

    // extract the comparator sign (borrows), then release the owned return
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]"); // boxed return (cast_int reads RAX)
    emitter.instruction("call __rt_mixed_cast_int"); // rax = raw i64 comparator sign (PHP (int) rules)
    emitter.instruction("mov QWORD PTR [rbp - 72], rax"); // save the sign
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]"); // boxed return
    emitter.instruction("call __rt_decref_mixed"); // release the invoker's owned return
    emitter.instruction("jmp __rt_pdo_call_collation_cleanup_x86"); // join the shared container-release path

    // -- longjmp path: a throw crossed the invoke --
    emitter.label("__rt_pdo_call_collation_threw_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 296]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("mov r10, QWORD PTR [rbp - 280]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0); // restore it
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0); // swallow the pending exception (no xCompare error channel)
    emitter.instruction("mov QWORD PTR [rbp - 72], 0"); // comparator sign = 0 (treat as equal)

    // -- shared cleanup: release the argument container --
    emitter.label("__rt_pdo_call_collation_cleanup_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]"); // boxed Mixed argument cell
    emitter.instruction("call __rt_decref_mixed"); // release the cell (drops the array ref boxing took)
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]"); // raw args array pointer
    emitter.instruction("call __rt_decref_any"); // release the array and deep-free its two boxed strings
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]"); // load the comparator sign into the result register
    emitter.instruction("add rsp, 304"); // release the adapter frame
    emitter.instruction("pop rbp"); // restore the caller frame pointer
    emitter.instruction("ret"); // return the comparison sign to the bridge dispatcher

    // -- fast path: no uniform invoker → equal, with nothing allocated to release --
    emitter.label("__rt_pdo_call_collation_ret_zero_x86");
    emitter.instruction("xor eax, eax"); // comparator sign = 0 (treat as equal)
    emitter.instruction("add rsp, 304"); // release the adapter frame
    emitter.instruction("pop rbp"); // restore the caller frame pointer
    emitter.instruction("ret"); // return to the bridge dispatcher
}
