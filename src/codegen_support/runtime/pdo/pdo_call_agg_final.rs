//! Purpose:
//! Emits `__rt_pdo_call_agg_final`, the codegen adapter that re-enters a compiled-PHP
//! aggregate finalize callback on behalf of the `elephc-pdo` SQLite bridge
//! (`Pdo\Sqlite::createAggregate`). It is the once-per-group half of the aggregate
//! pair: the bridge's `x_agg_final` dispatcher calls it after the last row with the
//! group's final accumulator + row count, and it produces the SQL result AND releases
//! the accumulator (finalize is terminal for the group).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::pdo`, gated by `RuntimeFeatures::pdo_udf`.
//! - The `elephc-pdo` bridge (`SqliteConn::create_aggregate` → `x_agg_final`), which
//!   only stores and calls this adapter's address; it never references a `__rt_*`
//!   symbol itself, so the accumulator's final release happens here, not in the bridge.
//!
//! Key details:
//! - C ABI: `__rt_pdo_call_agg_final(descriptor, accumulator, rownumber, out)`
//!   returning nothing; the result is written through `out` (a bridge-owned
//!   `ElephcResult`, exactly the scalar adapter's protocol). `accumulator` is the
//!   group's final boxed-Mixed accumulator (null for an empty group that never
//!   stepped); `rownumber` is the step count.
//! - Argument array: `[accumulator, rownumber]` (the PHP `finalize($context,
//!   $rownumber)` contract). Slot 0 is the accumulator (`__rt_incref`-ed when
//!   non-null; a null accumulator boxes as PHP null); slot 1 is the row count boxed
//!   as a Mixed int.
//! - Return decode is identical to the scalar adapter (`__rt_mixed_unbox` once, then
//!   int→`out.tag = 1`, float→`out.tag = 2`, string→bytes staged via
//!   `elephc_pdo_udf_stash_bytes` then `out.tag = 3`, bool→`out.tag = 5`, null/other→
//!   `out.tag = 0`, throw→`out.tag = -1`).
//! - Accumulator release: after producing the result (or on a throw), the adapter
//!   releases the args container (dropping the slot-0 incref) and then releases the
//!   accumulator's own group-slot ref — finalize is the terminal use, so the
//!   accumulator box is freed exactly once here.
//! - Exception firewall + caller-saved-only register discipline: identical to the
//!   scalar / step adapters (the prologue saves only x29/x30 / rbp).

use crate::codegen_support::callable_descriptor::CALLABLE_DESC_INVOKER_OFFSET;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

// The firewall handler record must match the layout __rt_throw_current assumes:
// next@0, survivor@8, diag@TRY_HANDLER_DIAG_DEPTH_OFFSET, jmp_buf@TRY_HANDLER_JMP_BUF_OFFSET.
const _: () = assert!(TRY_HANDLER_DIAG_DEPTH_OFFSET == 16);
const _: () = assert!(TRY_HANDLER_JMP_BUF_OFFSET == 24);

/// Emits `__rt_pdo_call_agg_final(descriptor, accumulator, rownumber, out)`.
///
/// Inputs (AArch64): x0 = descriptor, x1 = accumulator, x2 = rownumber, x3 = out.
/// No return value; the result is written through `out`.
/// (x86_64): rdi, rsi, rdx, rcx in the same order.
pub fn emit_pdo_call_agg_final(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pdo_call_agg_final_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: pdo_call_agg_final ---");
    emitter.label_global("__rt_pdo_call_agg_final");

    // Stack frame (304 bytes):
    //   [sp, #0]   = handler record (224 bytes): next@0, survivor@8, diag@16, jmp_buf@24
    //   [sp, #224] = descriptor    [sp, #232] = accumulator   [sp, #240] = rownumber
    //   [sp, #248] = out ptr       [sp, #256] = args array    [sp, #264] = boxed args cell
    //   [sp, #272] = boxed return
    //   [sp, #288] = saved x29     [sp, #296] = saved x30
    emitter.instruction("sub sp, sp, #304"); // allocate the agg-final adapter frame
    emitter.instruction("stp x29, x30, [sp, #288]"); // save frame pointer and return address
    emitter.instruction("add x29, sp, #288"); // establish the adapter frame pointer

    emitter.instruction("str x0, [sp, #224]"); // save finalize descriptor pointer
    emitter.instruction("str x1, [sp, #232]"); // save the final accumulator (null for an empty group)
    emitter.instruction("str x2, [sp, #240]"); // save the row count
    emitter.instruction("str x3, [sp, #248]"); // save the result-out pointer

    // -- fast path: no uniform invoker → SQL NULL, but still release the accumulator --
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the invoker slot
    emitter.instruction("cbz x9, __rt_pdo_call_agg_final_no_invoker"); // no invoker → NULL result + free accumulator

    // -- allocate a 2-slot argument array and stamp value_type = Mixed once --
    emitter.instruction("mov x0, #2"); // capacity: [context, rownumber]
    emitter.instruction("mov x1, #8"); // boxed Mixed slots store one pointer each
    emitter.instruction("bl __rt_array_new"); // x0 = indexed array backing storage
    emitter.instruction("ldr x10, [x0, #-8]"); // load the packed array kind word from the header
    emitter.instruction("mov x12, #0x80ff"); // preserve the indexed-array kind and persistent COW flag
    emitter.instruction("and x10, x10, x12"); // keep only the persistent metadata bits
    emitter.instruction("mov x11, #7"); // value_type tag 7 = boxed Mixed
    emitter.instruction("lsl x11, x11, #8"); // move the tag into the packed kind-word byte lane
    emitter.instruction("orr x10, x10, x11"); // combine the heap kind with the value_type tag
    emitter.instruction("str x10, [x0, #-8]"); // persist the stamped kind word (never re-stamped)
    emitter.instruction("str x0, [sp, #256]"); // save the args array pointer

    // -- slot 0: the accumulator (incref when non-null; box PHP null otherwise) --
    emitter.instruction("ldr x0, [sp, #232]"); // final accumulator
    emitter.instruction("cbz x0, __rt_pdo_call_agg_final_slot0_null"); // empty group → box PHP null
    emitter.instruction("bl __rt_incref"); // retain the accumulator for its args-array slot
    emitter.instruction("ldr x0, [sp, #232]"); // reload the accumulator pointer to store
    emitter.instruction("b __rt_pdo_call_agg_final_slot0_store");
    emitter.label("__rt_pdo_call_agg_final_slot0_null");
    emitter.instruction("mov x0, #8"); // runtime tag 8 = Void/NULL
    emitter.instruction("mov x1, #0"); // value_lo unused
    emitter.instruction("mov x2, #0"); // value_hi unused
    emitter.instruction("bl __rt_mixed_from_value"); // x0 = boxed PHP null
    emitter.label("__rt_pdo_call_agg_final_slot0_store");
    emitter.instruction("ldr x10, [sp, #256]"); // args array pointer (caller-saved temp)
    emitter.instruction("str x0, [x10, #24]"); // store the accumulator/null into slot 0
    emitter.instruction("mov x11, #1"); // running element count = 1
    emitter.instruction("str x11, [x10, #0]"); // update the array length field

    // -- slot 1: the row count boxed as a Mixed int --
    emitter.instruction("mov x0, #0"); // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #240]"); // value_lo = row count
    emitter.instruction("mov x2, #0"); // value_hi unused
    emitter.instruction("bl __rt_mixed_from_value"); // x0 = boxed Mixed(int) row count
    emitter.instruction("ldr x10, [sp, #256]"); // args array pointer
    emitter.instruction("str x0, [x10, #32]"); // store the row count into slot 1
    emitter.instruction("mov x11, #2"); // running element count = 2
    emitter.instruction("str x11, [x10, #0]"); // update the array length field

    // -- box the indexed array as a Mixed cell (tag 4 increfs the array) --
    emitter.instruction("ldr x1, [sp, #256]"); // raw args array pointer → payload lo
    emitter.instruction("mov x2, #0"); // payload hi unused for an array
    emitter.instruction("mov x0, #4"); // runtime tag 4 = indexed array
    emitter.instruction("bl __rt_mixed_from_value"); // x0 = boxed Mixed argument cell
    emitter.instruction("str x0, [sp, #264]"); // save the boxed Mixed argument cell

    // -- push a setjmp firewall handler around the invoke --
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction("str x10, [sp, #0]"); // handler record: previous handler-stack top
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
    emitter.instruction("str x10, [sp, #8]"); // handler record: activation frame to survive a throw
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction("str x10, [sp, #16]"); // handler record: saved diagnostic-suppression depth
    emitter.instruction("mov x10, sp"); // x10 = address of this handler record
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // link the record as the active handler
    emitter.instruction("add x0, sp, #24"); // x0 = &jmp_buf inside the handler record
    emitter.bl_c("setjmp"); // returns 0 on first pass, 1 when a throw longjmps back
    emitter.instruction("cbnz x0, __rt_pdo_call_agg_final_threw"); // nonzero → arrived via longjmp

    // -- normal path: invoke the finalize callback through its descriptor (offset 56) --
    emitter.instruction("ldr x0, [sp, #224]"); // arg0 = descriptor pointer
    emitter.instruction("ldr x1, [sp, #264]"); // arg1 = boxed Mixed argument cell
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the uniform invoker pointer
    emitter.instruction("blr x9"); // invoke finalize(...) → OWNED boxed Mixed return in x0
    emitter.instruction("str x0, [sp, #272]"); // save the boxed return for decode + release

    // pop the firewall handler before any further runtime calls
    emitter.instruction("ldr x10, [sp, #0]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("ldr x10, [sp, #16]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0); // restore it

    // -- type-preserving decode: unbox once and dispatch on the runtime tag --
    emitter.instruction("ldr x0, [sp, #272]"); // boxed return
    emitter.instruction("bl __rt_mixed_unbox"); // x0 = tag, x1 = lo, x2 = hi (tag-7 wrappers peeled)
    emitter.instruction("cmp x0, #0"); // Mixed int?
    emitter.instruction("b.eq __rt_pdo_call_agg_final_ret_int");
    emitter.instruction("cmp x0, #2"); // Mixed float?
    emitter.instruction("b.eq __rt_pdo_call_agg_final_ret_float");
    emitter.instruction("cmp x0, #1"); // Mixed string?
    emitter.instruction("b.eq __rt_pdo_call_agg_final_ret_string");
    emitter.instruction("cmp x0, #3"); // Mixed bool?
    emitter.instruction("b.eq __rt_pdo_call_agg_final_ret_bool");
    // -- tag 8 (null) or any non-scalar → SQL NULL --
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("str xzr, [x11, #0]"); // out.tag = 0 (NULL)
    emitter.instruction("b __rt_pdo_call_agg_final_release_return");
    emitter.label("__rt_pdo_call_agg_final_ret_int");
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("mov x10, #1"); // ElephcResult tag 1 = INT
    emitter.instruction("str x10, [x11, #0]"); // out.tag = 1
    emitter.instruction("str x1, [x11, #8]"); // out.i = lo (int64 value)
    emitter.instruction("b __rt_pdo_call_agg_final_release_return");
    emitter.label("__rt_pdo_call_agg_final_ret_float");
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("mov x10, #2"); // ElephcResult tag 2 = FLOAT
    emitter.instruction("str x10, [x11, #0]"); // out.tag = 2
    emitter.instruction("str x1, [x11, #16]"); // out.f = lo (raw f64 bit-pattern → stored as f64)
    emitter.instruction("b __rt_pdo_call_agg_final_release_return");
    emitter.label("__rt_pdo_call_agg_final_ret_bool");
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("mov x10, #5"); // ElephcResult tag 5 = BOOL
    emitter.instruction("str x10, [x11, #0]"); // out.tag = 5
    emitter.instruction("str x1, [x11, #8]"); // out.i = lo (0/1)
    emitter.instruction("b __rt_pdo_call_agg_final_release_return");
    // -- string: stage the bytes into the bridge BEFORE releasing the owned box --
    emitter.label("__rt_pdo_call_agg_final_ret_string");
    emitter.instruction("mov x0, x1"); // stash arg0 = byte pointer (unbox lo)
    emitter.instruction("mov x1, x2"); // stash arg1 = byte length (unbox hi)
    emitter.instruction("mov x2, #0"); // stash arg2 = is_blob 0 (return text)
    emitter.bl_c("elephc_pdo_udf_stash_bytes"); // deep-copy the string bytes into the bridge's stash
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("mov x10, #3"); // ElephcResult tag 3 = TEXT (bytes live in the stash)
    emitter.instruction("str x10, [x11, #0]"); // out.tag = 3
    emitter.instruction("b __rt_pdo_call_agg_final_release_return");

    // -- release the owned boxed return, then join the accumulator-release cleanup --
    emitter.label("__rt_pdo_call_agg_final_release_return");
    emitter.instruction("ldr x0, [sp, #272]"); // boxed return
    emitter.instruction("bl __rt_decref_mixed"); // release the invoker's owned return (bytes already staged)
    emitter.instruction("b __rt_pdo_call_agg_final_cleanup"); // join the shared cleanup path

    // -- longjmp path: the finalize callback threw --
    emitter.label("__rt_pdo_call_agg_final_threw");
    emitter.instruction("ldr x10, [sp, #0]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("ldr x10, [sp, #16]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0); // restore it
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0); // swallow the pending exception (surfaced as a SQL error)
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("mov x10, #-1"); // ElephcResult tag -1 = ERROR
    emitter.instruction("str x10, [x11, #0]"); // out.tag = -1

    // -- shared cleanup: release the argument container, then the accumulator (terminal) --
    emitter.label("__rt_pdo_call_agg_final_cleanup");
    emitter.instruction("ldr x0, [sp, #264]"); // boxed Mixed argument cell
    emitter.instruction("bl __rt_decref_mixed"); // release the cell (drops the array ref boxing took)
    emitter.instruction("ldr x0, [sp, #256]"); // raw args array pointer
    emitter.instruction("bl __rt_decref_any"); // release the array (drops the slot-0 accumulator incref)
    emitter.instruction("ldr x0, [sp, #232]"); // the accumulator
    emitter.instruction("cbz x0, __rt_pdo_call_agg_final_done"); // empty group had no accumulator to free
    emitter.instruction("bl __rt_decref_mixed"); // finalize is terminal → free the accumulator's group ref
    emitter.label("__rt_pdo_call_agg_final_done");
    emitter.instruction("ldp x29, x30, [sp, #288]"); // restore frame pointer and return address
    emitter.instruction("add sp, sp, #304"); // release the adapter frame
    emitter.instruction("ret"); // return to the bridge dispatcher (result is in *out)

    // -- fast path: no uniform invoker → NULL result; still free the accumulator --
    emitter.label("__rt_pdo_call_agg_final_no_invoker");
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("str xzr, [x11, #0]"); // out.tag = 0 (NULL)
    emitter.instruction("ldr x0, [sp, #232]"); // the accumulator
    emitter.instruction("cbz x0, __rt_pdo_call_agg_final_no_invoker_done"); // nothing to free
    emitter.instruction("bl __rt_decref_mixed"); // free the accumulator's group ref (terminal)
    emitter.label("__rt_pdo_call_agg_final_no_invoker_done");
    emitter.instruction("ldp x29, x30, [sp, #288]"); // restore frame pointer and return address
    emitter.instruction("add sp, sp, #304"); // release the adapter frame
    emitter.instruction("ret"); // return to the bridge dispatcher
}

/// x86_64 implementation of `__rt_pdo_call_agg_final`.
fn emit_pdo_call_agg_final_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pdo_call_agg_final ---");
    emitter.label_global("__rt_pdo_call_agg_final");

    // Frame (288 bytes below rbp):
    //   [rbp-8]   descriptor   [rbp-16] accumulator  [rbp-24] rownumber  [rbp-32] out ptr
    //   [rbp-40]  args array   [rbp-48] boxed args cell   [rbp-56] boxed return
    //   [rbp-288] handler record (224 bytes): next@[rbp-288], survivor@[rbp-280],
    //             diag@[rbp-272], jmp_buf base@[rbp-264].
    emitter.instruction("push rbp"); // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp"); // establish the adapter frame pointer
    emitter.instruction("sub rsp, 288"); // reserve the slots and the 224-byte handler record

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi"); // save finalize descriptor pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi"); // save the final accumulator
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx"); // save the row count
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx"); // save the result-out pointer

    // -- fast path: no uniform invoker → SQL NULL, but still release the accumulator --
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the invoker slot
    emitter.instruction("test r10, r10"); // does the descriptor expose a uniform invoker?
    emitter.instruction("jz __rt_pdo_call_agg_final_no_invoker_x86"); // no invoker → NULL + free accumulator

    // -- allocate a 2-slot argument array and stamp value_type = Mixed once --
    emitter.instruction("mov rdi, 2"); // capacity: [context, rownumber]
    emitter.instruction("mov rsi, 8"); // boxed Mixed slots store one pointer each
    emitter.instruction("call __rt_array_new"); // rax = indexed array backing storage
    emitter.instruction("mov r10, QWORD PTR [rax - 8]"); // load the packed array kind word from the header
    emitter.instruction("mov r11, 0xffffffff000080ff"); // preserve heap marker + indexed-array kind + COW bit
    emitter.instruction("and r10, r11"); // keep only the persistent metadata bits
    emitter.instruction("mov r11, 7"); // value_type tag 7 = boxed Mixed
    emitter.instruction("shl r11, 8"); // move the tag into the packed kind-word byte lane
    emitter.instruction("or r10, r11"); // combine the heap kind with the value_type tag
    emitter.instruction("mov QWORD PTR [rax - 8], r10"); // persist the stamped kind word (never re-stamped)
    emitter.instruction("mov QWORD PTR [rbp - 40], rax"); // save the args array pointer

    // -- slot 0: the accumulator (incref when non-null; box PHP null otherwise) --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]"); // final accumulator (incref reads RAX)
    emitter.instruction("test rax, rax"); // empty group?
    emitter.instruction("jz __rt_pdo_call_agg_final_slot0_null_x86"); // yes → box PHP null
    emitter.instruction("call __rt_incref"); // retain the accumulator for its args-array slot
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]"); // reload the accumulator pointer to store
    emitter.instruction("jmp __rt_pdo_call_agg_final_slot0_store_x86");
    emitter.label("__rt_pdo_call_agg_final_slot0_null_x86");
    emitter.instruction("mov rdi, 0"); // value_lo unused
    emitter.instruction("mov rsi, 0"); // value_hi unused
    emitter.instruction("mov eax, 8"); // runtime tag 8 = Void/NULL
    emitter.instruction("call __rt_mixed_from_value"); // rax = boxed PHP null
    emitter.label("__rt_pdo_call_agg_final_slot0_store_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]"); // args array pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], rax"); // store the accumulator/null into slot 0
    emitter.instruction("mov QWORD PTR [r10], 1"); // update the array length field to 1

    // -- slot 1: the row count boxed as a Mixed int --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]"); // value_lo = row count
    emitter.instruction("xor esi, esi"); // value_hi unused
    emitter.instruction("mov eax, 0"); // runtime tag 0 = int
    emitter.instruction("call __rt_mixed_from_value"); // rax = boxed Mixed(int) row count
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]"); // args array pointer
    emitter.instruction("mov QWORD PTR [r10 + 32], rax"); // store the row count into slot 1
    emitter.instruction("mov QWORD PTR [r10], 2"); // update the array length field to 2

    // -- box the indexed array as a Mixed cell (tag 4 increfs the array) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]"); // raw args array pointer → payload lo
    emitter.instruction("xor esi, esi"); // payload hi unused for an array
    emitter.instruction("mov eax, 4"); // runtime tag 4 = indexed array
    emitter.instruction("call __rt_mixed_from_value"); // rax = boxed Mixed argument cell
    emitter.instruction("mov QWORD PTR [rbp - 48], rax"); // save the boxed Mixed argument cell

    // -- push a setjmp firewall handler around the invoke --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction("mov QWORD PTR [rbp - 288], r10"); // handler record: record.next
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
    emitter.instruction("mov QWORD PTR [rbp - 280], r10"); // handler record: survivor frame
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction("mov QWORD PTR [rbp - 272], r10"); // handler record: saved diagnostic depth
    emitter.instruction("lea r10, [rbp - 288]"); // r10 = address of this handler record
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // link the record as the active handler
    emitter.instruction("lea rdi, [rbp - 264]"); // rdi = &jmp_buf inside the handler record (record + 24)
    emitter.bl_c("setjmp"); // returns 0 on first pass, 1 when a throw longjmps back
    emitter.instruction("test rax, rax"); // did control arrive via longjmp?
    emitter.instruction("jne __rt_pdo_call_agg_final_threw_x86"); // nonzero → arrived via longjmp

    // -- normal path: invoke the finalize callback through its descriptor (offset 56) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]"); // arg0 = descriptor pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]"); // arg1 = boxed Mixed argument cell
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the uniform invoker pointer
    emitter.instruction("call r10"); // invoke finalize(...) → OWNED boxed Mixed return in rax
    emitter.instruction("mov QWORD PTR [rbp - 56], rax"); // save the boxed return for decode + release

    // pop the firewall handler before any further runtime calls
    emitter.instruction("mov r10, QWORD PTR [rbp - 288]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("mov r10, QWORD PTR [rbp - 272]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0); // restore it

    // -- type-preserving decode: unbox once and dispatch on the runtime tag --
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]"); // boxed return (unbox reads RAX)
    emitter.instruction("call __rt_mixed_unbox"); // rax = tag, rdi = lo, rdx = hi
    emitter.instruction("cmp rax, 0"); // Mixed int?
    emitter.instruction("je __rt_pdo_call_agg_final_ret_int_x86");
    emitter.instruction("cmp rax, 2"); // Mixed float?
    emitter.instruction("je __rt_pdo_call_agg_final_ret_float_x86");
    emitter.instruction("cmp rax, 1"); // Mixed string?
    emitter.instruction("je __rt_pdo_call_agg_final_ret_string_x86");
    emitter.instruction("cmp rax, 3"); // Mixed bool?
    emitter.instruction("je __rt_pdo_call_agg_final_ret_bool_x86");
    // -- tag 8 (null) or any non-scalar → SQL NULL --
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 0"); // out.tag = 0 (NULL)
    emitter.instruction("jmp __rt_pdo_call_agg_final_release_return_x86");
    emitter.label("__rt_pdo_call_agg_final_ret_int_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 1"); // out.tag = 1 (INT)
    emitter.instruction("mov QWORD PTR [r11 + 8], rdi"); // out.i = lo (int64 value)
    emitter.instruction("jmp __rt_pdo_call_agg_final_release_return_x86");
    emitter.label("__rt_pdo_call_agg_final_ret_float_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 2"); // out.tag = 2 (FLOAT)
    emitter.instruction("mov QWORD PTR [r11 + 16], rdi"); // out.f = lo (raw f64 bit-pattern → stored as f64)
    emitter.instruction("jmp __rt_pdo_call_agg_final_release_return_x86");
    emitter.label("__rt_pdo_call_agg_final_ret_bool_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 5"); // out.tag = 5 (BOOL)
    emitter.instruction("mov QWORD PTR [r11 + 8], rdi"); // out.i = lo (0/1)
    emitter.instruction("jmp __rt_pdo_call_agg_final_release_return_x86");
    // -- string: stage the bytes into the bridge BEFORE releasing the owned box --
    emitter.label("__rt_pdo_call_agg_final_ret_string_x86");
    emitter.instruction("mov rsi, rdx"); // stash arg1 = byte length (unbox hi), before rdx is reused
    emitter.instruction("xor edx, edx"); // stash arg2 = is_blob 0 (return text)
    // rdi already holds the unbox lo (byte pointer) = stash arg0.
    emitter.bl_c("elephc_pdo_udf_stash_bytes"); // deep-copy the string bytes into the bridge's stash
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 3"); // out.tag = 3 (TEXT; bytes live in the stash)
    emitter.instruction("jmp __rt_pdo_call_agg_final_release_return_x86");

    // -- release the owned boxed return, then join the accumulator-release cleanup --
    emitter.label("__rt_pdo_call_agg_final_release_return_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]"); // boxed return
    emitter.instruction("call __rt_decref_mixed"); // release the invoker's owned return (bytes already staged)
    emitter.instruction("jmp __rt_pdo_call_agg_final_cleanup_x86"); // join the shared cleanup path

    // -- longjmp path: the finalize callback threw --
    emitter.label("__rt_pdo_call_agg_final_threw_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 288]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("mov r10, QWORD PTR [rbp - 272]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0); // restore it
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0); // swallow the pending exception
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], -1"); // out.tag = -1 (ERROR)

    // -- shared cleanup: release the argument container, then the accumulator (terminal) --
    emitter.label("__rt_pdo_call_agg_final_cleanup_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]"); // boxed Mixed argument cell
    emitter.instruction("call __rt_decref_mixed"); // release the cell (drops the array ref boxing took)
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]"); // raw args array pointer
    emitter.instruction("call __rt_decref_any"); // release the array (drops the slot-0 accumulator incref)
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]"); // the accumulator
    emitter.instruction("test rax, rax"); // empty group had no accumulator?
    emitter.instruction("jz __rt_pdo_call_agg_final_done_x86"); // nothing to free
    emitter.instruction("call __rt_decref_mixed"); // finalize is terminal → free the accumulator's group ref
    emitter.label("__rt_pdo_call_agg_final_done_x86");
    emitter.instruction("add rsp, 288"); // release the adapter frame
    emitter.instruction("pop rbp"); // restore the caller frame pointer
    emitter.instruction("ret"); // return to the bridge dispatcher (result is in *out)

    // -- fast path: no uniform invoker → NULL result; still free the accumulator --
    emitter.label("__rt_pdo_call_agg_final_no_invoker_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 0"); // out.tag = 0 (NULL)
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]"); // the accumulator
    emitter.instruction("test rax, rax"); // nothing to free?
    emitter.instruction("jz __rt_pdo_call_agg_final_no_invoker_done_x86");
    emitter.instruction("call __rt_decref_mixed"); // free the accumulator's group ref (terminal)
    emitter.label("__rt_pdo_call_agg_final_no_invoker_done_x86");
    emitter.instruction("add rsp, 288"); // release the adapter frame
    emitter.instruction("pop rbp"); // restore the caller frame pointer
    emitter.instruction("ret"); // return to the bridge dispatcher
}
