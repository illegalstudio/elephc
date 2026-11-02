//! Purpose:
//! Emits `__rt_pdo_call_agg_step`, the codegen adapter that re-enters a compiled-PHP
//! aggregate step callback on behalf of the `elephc-pdo` SQLite bridge
//! (`Pdo\Sqlite::createAggregate`). It is the per-row half of the aggregate pair: the
//! bridge's `x_agg_step` dispatcher calls it once per row with the group's running
//! accumulator + row number + the SQLite row values, and it returns the new
//! accumulator the callback produced.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::pdo`, gated by `RuntimeFeatures::pdo_udf`.
//! - The `elephc-pdo` bridge (`SqliteConn::create_aggregate` → `x_agg_step`), which
//!   only stores and calls this adapter's address; it never references a `__rt_*`
//!   symbol itself (respecting the non-whole-archive linker boundary), so ALL of the
//!   accumulator's refcount traffic happens here, not in the bridge.
//!
//! Key details:
//! - C ABI: `__rt_pdo_call_agg_step(descriptor, accumulator, rownumber, argv, argc,
//!   threw) -> new_accumulator`. `accumulator` is the group's current boxed-Mixed
//!   accumulator (null before the first row); `argv`/`argc` are the row's `ElephcVal[]`
//!   (40-byte stride, same as the scalar adapter); `threw` is an out-param i64 the
//!   adapter sets to 1 iff the callback threw. The owned new accumulator is returned;
//!   the bridge stores it into `AggCtx.accumulator`.
//! - Argument array: `[accumulator, rownumber, ...rowValues]` (the two prepended slots
//!   are the PHP `step($context, $rownumber, ...$values)` contract). Slot 0 is the
//!   accumulator (`__rt_incref`-ed when non-null so the args container's later release
//!   balances it; a null accumulator boxes as PHP null instead); slot 1 is the row
//!   number boxed as a Mixed int; slots 2.. are the row values boxed exactly as the
//!   scalar adapter's loop does.
//! - Refcount protocol: on the NORMAL path the adapter releases the args container
//!   (dropping the slot-0 incref) and then releases the OLD accumulator's own ref (it
//!   is being replaced), returning the callback's owned result. If the callback
//!   returned `$context` (aliasing the old accumulator), the invoker's return-incref
//!   keeps it alive at exactly one ref. On the THROW path the adapter releases the
//!   args container (still dropping the slot-0 incref) but does NOT release the old
//!   accumulator — it stays valid in the group's slot so `x_agg_final` can free it —
//!   sets `*threw = 1`, and returns null.
//! - Exception firewall: identical shared-size setjmp handler record as the scalar /
//!   collation adapters. A compiled-PHP `throw` inside the step callback longjmps back
//!   here rather than unwinding across SQLite's VDBE and the Rust bridge frame.
//! - Registers: the body uses ONLY caller-saved scratch (aarch64 x9-x15; x86_64
//!   r8-r11 + rax/rcx/rdi/rsi) since the prologue saves only x29/x30 (rbp): clobbering
//!   a callee-saved register would corrupt a value the bridge's `x_agg_step` caller
//!   holds live across the call.

use crate::codegen_support::callable_descriptor::CALLABLE_DESC_INVOKER_OFFSET;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET,
    TRY_HANDLER_SLOT_SIZE,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

// The firewall handler record must match the layout __rt_throw_current assumes:
// next@0, survivor@8, diag@TRY_HANDLER_DIAG_DEPTH_OFFSET,
// jmp_buf@TRY_HANDLER_JMP_BUF_OFFSET.
const _: () = assert!(TRY_HANDLER_DIAG_DEPTH_OFFSET == 16);
const _: () = assert!(TRY_HANDLER_JMP_BUF_OFFSET == 24);
const HANDLER_AREA_SIZE: usize = TRY_HANDLER_SLOT_SIZE + 16;
const AARCH64_FRAME_SIZE: usize = HANDLER_AREA_SIZE + 96;
const X86_HANDLER_BASE: usize = HANDLER_AREA_SIZE + 80;
const X86_FRAME_SIZE: usize = HANDLER_AREA_SIZE + 80;

/// Emits `__rt_pdo_call_agg_step(descriptor, accumulator, rownumber, argv, argc, threw)
/// -> new_accumulator`.
///
/// Inputs (AArch64): x0 = descriptor, x1 = accumulator, x2 = rownumber, x3 = argv,
/// x4 = argc, x5 = threw (out-param). Result in x0 = owned new accumulator.
/// (x86_64): rdi, rsi, rdx, rcx, r8, r9 in the same order; result in rax.
pub fn emit_pdo_call_agg_step(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pdo_call_agg_step_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: pdo_call_agg_step ---");
    emitter.label_global("__rt_pdo_call_agg_step");

    // Stack frame (AARCH64_FRAME_SIZE bytes):
    //   [sp, #0]   = handler area (shared-size record + 16-byte pad): next@0, survivor@8, diag@16,
    //                jmp_buf@24
    //   [sp, HANDLER_AREA_SIZE] = descriptor    [sp, HANDLER_AREA_SIZE + 8] = accumulator   [sp, HANDLER_AREA_SIZE + 16] = rownumber
    //   [sp, HANDLER_AREA_SIZE + 24] = argv          [sp, HANDLER_AREA_SIZE + 32] = argc          [sp, HANDLER_AREA_SIZE + 40] = threw ptr
    //   [sp, HANDLER_AREA_SIZE + 48] = args array    [sp, HANDLER_AREA_SIZE + 56] = boxed args cell
    //   [sp, HANDLER_AREA_SIZE + 64] = boxed return  [sp, HANDLER_AREA_SIZE + 72] = loop index
    //   [sp, HANDLER_AREA_SIZE + 80] = saved x29     [sp, HANDLER_AREA_SIZE + 88] = saved x30
    emitter.instruction(&format!("sub sp, sp, #{}", AARCH64_FRAME_SIZE));       // allocate the agg-step adapter frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", HANDLER_AREA_SIZE + 80)); // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", HANDLER_AREA_SIZE + 80));  // establish the adapter frame pointer

    emitter.instruction(&format!("str x0, [sp, #{}]", HANDLER_AREA_SIZE));      // save step descriptor pointer
    emitter.instruction(&format!("str x1, [sp, #{}]", HANDLER_AREA_SIZE + 8));  // save current accumulator (null before the first row)
    emitter.instruction(&format!("str x2, [sp, #{}]", HANDLER_AREA_SIZE + 16)); // save row number
    emitter.instruction(&format!("str x3, [sp, #{}]", HANDLER_AREA_SIZE + 24)); // save argv base pointer
    emitter.instruction(&format!("str x4, [sp, #{}]", HANDLER_AREA_SIZE + 32)); // save row argument count
    emitter.instruction(&format!("str x5, [sp, #{}]", HANDLER_AREA_SIZE + 40)); // save the threw out-param pointer

    // -- initialise *threw = 0 (no exception unless the firewall fires) --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HANDLER_AREA_SIZE + 40)); // threw out-param pointer
    emitter.instruction("str xzr, [x9]");                                       // *threw = 0

    // -- fast path: no uniform invoker → return the accumulator unchanged --
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the invoker slot
    emitter.instruction("cbz x9, __rt_pdo_call_agg_step_no_invoker");           // no invoker → pass the accumulator straight through

    // -- allocate an (argc + 2)-slot argument array and stamp value_type = Mixed once --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HANDLER_AREA_SIZE + 32)); // row argument count
    emitter.instruction("add x0, x0, #2");                                      // + 2 for the prepended [context, rownumber] slots
    emitter.instruction("mov x1, #8");                                          // boxed Mixed slots store one pointer each
    emitter.instruction("bl __rt_array_new");                                   // x0 = indexed array backing storage
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the packed array kind word from the header
    emitter.instruction("mov x12, #0x80ff");                                    // preserve the indexed-array kind and persistent COW flag
    emitter.instruction("and x10, x10, x12");                                   // keep only the persistent metadata bits
    emitter.instruction("mov x11, #7");                                         // value_type tag 7 = boxed Mixed
    emitter.instruction("lsl x11, x11, #8");                                    // move the tag into the packed kind-word byte lane
    emitter.instruction("orr x10, x10, x11");                                   // combine the heap kind with the value_type tag
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the stamped kind word (never re-stamped: no push_int)
    emitter.instruction(&format!("str x0, [sp, #{}]", HANDLER_AREA_SIZE + 48)); // save the args array pointer

    // -- slot 0: the current accumulator (incref when non-null; box PHP null otherwise) --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HANDLER_AREA_SIZE + 8));  // current accumulator
    emitter.instruction("cbz x0, __rt_pdo_call_agg_step_slot0_null");           // null before the first row → box PHP null
    emitter.instruction("bl __rt_incref");                                      // retain the accumulator for its args-array slot
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HANDLER_AREA_SIZE + 8));  // reload the accumulator pointer to store
    emitter.instruction("b __rt_pdo_call_agg_step_slot0_store");                // retained accumulator in x0 → store it into slot 0
    emitter.label("__rt_pdo_call_agg_step_slot0_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = Void/NULL
    emitter.instruction("mov x1, #0");                                          // value_lo unused
    emitter.instruction("mov x2, #0");                                          // value_hi unused
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = boxed PHP null
    emitter.label("__rt_pdo_call_agg_step_slot0_store");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", HANDLER_AREA_SIZE + 48)); // args array pointer (caller-saved temp)
    emitter.instruction("str x0, [x10, #24]");                                  // store the accumulator/null into slot 0
    emitter.instruction("mov x11, #1");                                         // running element count = 1
    emitter.instruction("str x11, [x10, #0]");                                  // update the array length field

    // -- slot 1: the row number boxed as a Mixed int --
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction(&format!("ldr x1, [sp, #{}]", HANDLER_AREA_SIZE + 16)); // value_lo = row number
    emitter.instruction("mov x2, #0");                                          // value_hi unused
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = boxed Mixed(int) row number
    emitter.instruction(&format!("ldr x10, [sp, #{}]", HANDLER_AREA_SIZE + 48)); // args array pointer
    emitter.instruction("str x0, [x10, #32]");                                  // store the row number into slot 1
    emitter.instruction("mov x11, #2");                                         // running element count = 2
    emitter.instruction("str x11, [x10, #0]");                                  // update the array length field

    // -- boxing loop: box each row ElephcVal into slots 2.. --
    emitter.instruction(&format!("str xzr, [sp, #{}]", HANDLER_AREA_SIZE + 72)); // loop index k = 0
    emitter.label("__rt_pdo_call_agg_step_loop");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HANDLER_AREA_SIZE + 72)); // reload the loop index
    emitter.instruction(&format!("ldr x10, [sp, #{}]", HANDLER_AREA_SIZE + 32)); // reload the row argument count
    emitter.instruction("cmp x9, x10");                                         // have all row values been boxed?
    emitter.instruction("b.ge __rt_pdo_call_agg_step_loop_done");               // yes → box the container and invoke
    emitter.instruction(&format!("ldr x11, [sp, #{}]", HANDLER_AREA_SIZE + 24)); // reload the argv base pointer
    emitter.instruction("mov x12, #40");                                        // ElephcVal stride (tag@0,i@8,f@16,ptr@24,len@32)
    emitter.instruction("mul x13, x9, x12");                                    // byte offset of ElephcVal[k]
    emitter.instruction("add x11, x11, x13");                                   // x11 = &argv[k]
    emitter.instruction("ldr x14, [x11, #0]");                                  // load the ElephcVal storage-class tag
    emitter.instruction("cmp x14, #1");                                         // SQLITE_INTEGER?
    emitter.instruction("b.eq __rt_pdo_call_agg_step_box_int");                 // yes → box as a Mixed int
    emitter.instruction("cmp x14, #2");                                         // SQLITE_FLOAT?
    emitter.instruction("b.eq __rt_pdo_call_agg_step_box_float");               // yes → box as a Mixed float
    emitter.instruction("cmp x14, #3");                                         // SQLITE_TEXT?
    emitter.instruction("b.eq __rt_pdo_call_agg_step_box_str");                 // yes → box as a Mixed string
    emitter.instruction("cmp x14, #4");                                         // SQLITE_BLOB?
    emitter.instruction("b.eq __rt_pdo_call_agg_step_box_str");                 // yes → blobs box as Mixed strings too
    // -- tag 0 (SQLITE_NULL) or any unexpected code → PHP null (Mixed tag 8) --
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = Void/NULL
    emitter.instruction("mov x1, #0");                                          // value_lo unused
    emitter.instruction("mov x2, #0");                                          // value_hi unused
    emitter.instruction("b __rt_pdo_call_agg_step_box_call");                   // box the PHP null via the shared boxing tail
    emitter.label("__rt_pdo_call_agg_step_box_int");
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [x11, #8]");                                   // value_lo = ElephcVal.i
    emitter.instruction("mov x2, #0");                                          // value_hi unused
    emitter.instruction("b __rt_pdo_call_agg_step_box_call");                   // box the int via the shared boxing tail
    emitter.label("__rt_pdo_call_agg_step_box_float");
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = float
    emitter.instruction("ldr x1, [x11, #16]");                                  // value_lo = ElephcVal.f raw f64 bit-pattern
    emitter.instruction("mov x2, #0");                                          // value_hi unused
    emitter.instruction("b __rt_pdo_call_agg_step_box_call");                   // box the float via the shared boxing tail
    emitter.label("__rt_pdo_call_agg_step_box_str");
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string (binary-safe)
    emitter.instruction("ldr x1, [x11, #24]");                                  // value_lo = ElephcVal.ptr
    emitter.instruction("ldr x2, [x11, #32]");                                  // value_hi = ElephcVal.len
    emitter.instruction("b __rt_pdo_call_agg_step_box_call");                   // box the string via the shared boxing tail
    emitter.label("__rt_pdo_call_agg_step_box_call");
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = owned boxed Mixed row value
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HANDLER_AREA_SIZE + 72)); // reload the loop index (clobbered by the call)
    emitter.instruction(&format!("ldr x10, [sp, #{}]", HANDLER_AREA_SIZE + 48)); // reload the args array pointer (caller-saved temp)
    emitter.instruction("add x12, x9, #2");                                     // element index = k + 2 (past [context, rownumber])
    emitter.instruction("lsl x12, x12, #3");                                    // (k + 2) * 8
    emitter.instruction("add x12, x12, #24");                                   // element region begins 24 bytes past the header
    emitter.instruction("str x0, [x10, x12]");                                  // store the boxed row value into slot k+2
    emitter.instruction("add x9, x9, #1");                                      // advance the loop index
    emitter.instruction(&format!("str x9, [sp, #{}]", HANDLER_AREA_SIZE + 72)); // persist the loop index
    emitter.instruction("add x11, x9, #2");                                     // total element count = (k+1) + 2
    emitter.instruction("str x11, [x10, #0]");                                  // update the array length field
    emitter.instruction("b __rt_pdo_call_agg_step_loop");                       // box the next row value
    emitter.label("__rt_pdo_call_agg_step_loop_done");

    // -- box the indexed array as a Mixed cell (tag 4 increfs the array) --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", HANDLER_AREA_SIZE + 48)); // raw args array pointer → payload lo
    emitter.instruction("mov x2, #0");                                          // payload hi unused for an array
    emitter.instruction("mov x0, #4");                                          // runtime tag 4 = indexed array
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = boxed Mixed argument cell
    emitter.instruction(&format!("str x0, [sp, #{}]", HANDLER_AREA_SIZE + 56)); // save the boxed Mixed argument cell

    // -- push a setjmp firewall handler around the invoke --
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction("str x10, [sp, #0]");                                   // handler record: previous handler-stack top
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
    emitter.instruction("str x10, [sp, #8]");                                   // handler record: activation frame to survive a throw
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction("str x10, [sp, #16]");                                  // handler record: saved diagnostic-suppression depth
    emitter.instruction("mov x10, sp");                                         // x10 = address of this handler record
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // link the record as the active handler
    emitter.instruction("add x0, sp, #24");                                     // x0 = &jmp_buf inside the handler record
    emitter.save_exception_handler();                                           // returns 0 on first pass, 1 when a throw longjmps back
    emitter.instruction("cbnz x0, __rt_pdo_call_agg_step_threw");               // nonzero → arrived via longjmp

    // -- normal path: invoke the step callback through its descriptor (offset 56) --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HANDLER_AREA_SIZE));      // arg0 = descriptor pointer
    emitter.instruction(&format!("ldr x1, [sp, #{}]", HANDLER_AREA_SIZE + 56)); // arg1 = boxed Mixed argument cell
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the uniform invoker pointer
    emitter.instruction("blr x9");                                              // invoke step(...) → OWNED boxed Mixed new accumulator in x0
    emitter.instruction(&format!("str x0, [sp, #{}]", HANDLER_AREA_SIZE + 64)); // save the new accumulator

    // pop the firewall handler before any further runtime calls
    emitter.instruction("ldr x10, [sp, #0]");                                   // record.next
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("ldr x10, [sp, #16]");                                  // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0); // restore it

    // -- release the args container (drops the slot-0 accumulator incref) --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HANDLER_AREA_SIZE + 56)); // boxed Mixed argument cell
    emitter.instruction("bl __rt_decref_mixed");                                // release the cell (drops the array ref boxing took)
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HANDLER_AREA_SIZE + 48)); // raw args array pointer
    emitter.instruction("bl __rt_decref_any");                                  // release the array and deep-free its boxed args

    // -- release the OLD accumulator's own ref (it is being replaced) --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HANDLER_AREA_SIZE + 8));  // old accumulator
    emitter.instruction("cbz x0, __rt_pdo_call_agg_step_return");               // null (first row) → nothing to release
    emitter.instruction("bl __rt_decref_mixed");                                // drop the old accumulator's group-slot ref
    emitter.label("__rt_pdo_call_agg_step_return");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HANDLER_AREA_SIZE + 64)); // return the owned new accumulator
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", HANDLER_AREA_SIZE + 80)); // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", AARCH64_FRAME_SIZE));       // release the adapter frame
    emitter.instruction("ret");                                                 // return the new accumulator to the bridge dispatcher

    // -- longjmp path: the step callback threw --
    emitter.label("__rt_pdo_call_agg_step_threw");
    emitter.instruction("ldr x10, [sp, #0]");                                   // record.next
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("ldr x10, [sp, #16]");                                  // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0); // restore it
    abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0); // take ownership of the pending Throwable
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0); // clear the exception slot before release
    emitter.instruction("cbz x0, __rt_pdo_call_agg_step_threw_released");       // tolerate a defensive null exception slot
    emitter.instruction("bl __rt_decref_any");                                  // release the caught Throwable object
    emitter.label("__rt_pdo_call_agg_step_threw_released");
    // release the args container (drops the slot-0 incref) but PRESERVE the accumulator
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HANDLER_AREA_SIZE + 56)); // boxed Mixed argument cell
    emitter.instruction("bl __rt_decref_mixed");                                // release the cell
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HANDLER_AREA_SIZE + 48)); // raw args array pointer
    emitter.instruction("bl __rt_decref_any");                                  // release the array (the slot-0 incref is dropped here)
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HANDLER_AREA_SIZE + 40)); // threw out-param pointer
    emitter.instruction("mov x10, #1");                                         // signal the callback threw
    emitter.instruction("str x10, [x9]");                                       // *threw = 1
    emitter.instruction("mov x0, #0");                                          // return null (the bridge preserves the old accumulator)
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", HANDLER_AREA_SIZE + 80)); // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", AARCH64_FRAME_SIZE));       // release the adapter frame
    emitter.instruction("ret");                                                 // return to the bridge dispatcher

    // -- fast path: no uniform invoker → pass the accumulator through, nothing allocated --
    emitter.label("__rt_pdo_call_agg_step_no_invoker");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HANDLER_AREA_SIZE + 8));  // return the accumulator unchanged
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", HANDLER_AREA_SIZE + 80)); // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", AARCH64_FRAME_SIZE));       // release the adapter frame
    emitter.instruction("ret");                                                 // return to the bridge dispatcher
}

/// x86_64 implementation of `__rt_pdo_call_agg_step`.
fn emit_pdo_call_agg_step_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pdo_call_agg_step ---");
    emitter.label_global("__rt_pdo_call_agg_step");

    // Frame (X86_FRAME_SIZE bytes below rbp):
    //   [rbp-8]   descriptor   [rbp-16] accumulator  [rbp-24] rownumber  [rbp-32] argv
    //   [rbp-40]  argc         [rbp-48] threw ptr    [rbp-56] args array [rbp-64] boxed args cell
    //   [rbp-72]  boxed return [rbp-80] loop index
    //   [rbp-X86_HANDLER_BASE+0] handler record (HANDLER_AREA_SIZE bytes): next@[rbp-X86_HANDLER_BASE+0], survivor@[rbp-X86_HANDLER_BASE+8],
    //             diag@[rbp-X86_HANDLER_BASE+16], jmp_buf@[rbp-X86_HANDLER_BASE+24].
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the adapter frame pointer
    emitter.instruction(&format!("sub rsp, {}", X86_FRAME_SIZE));               // reserve slots, the 224-byte handler record, and padding

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save step descriptor pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save current accumulator
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save row number
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save argv base pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save row argument count
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the threw out-param pointer

    // -- initialise *threw = 0 --
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // threw out-param pointer
    emitter.instruction("mov QWORD PTR [rax], 0");                              // *threw = 0

    // -- fast path: no uniform invoker → return the accumulator unchanged --
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the invoker slot
    emitter.instruction("test r10, r10");                                       // does the descriptor expose a uniform invoker?
    emitter.instruction("jz __rt_pdo_call_agg_step_no_invoker_x86");            // no invoker → pass the accumulator straight through

    // -- allocate an (argc + 2)-slot argument array and stamp value_type = Mixed once --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // row argument count
    emitter.instruction("add rdi, 2");                                          // + 2 for the prepended [context, rownumber] slots
    emitter.instruction("mov rsi, 8");                                          // boxed Mixed slots store one pointer each
    emitter.instruction("call __rt_array_new");                                 // rax = indexed array backing storage
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed array kind word from the header
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve heap marker + indexed-array kind + COW bit
    emitter.instruction("and r10, r11");                                        // keep only the persistent metadata bits
    emitter.instruction("mov r11, 7");                                          // value_type tag 7 = boxed Mixed
    emitter.instruction("shl r11, 8");                                          // move the tag into the packed kind-word byte lane
    emitter.instruction("or r10, r11");                                         // combine the heap kind with the value_type tag
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // persist the stamped kind word (never re-stamped)
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the args array pointer

    // -- slot 0: the current accumulator (incref when non-null; box PHP null otherwise) --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // current accumulator (incref reads RAX)
    emitter.instruction("test rax, rax");                                       // null before the first row?
    emitter.instruction("jz __rt_pdo_call_agg_step_slot0_null_x86");            // yes → box PHP null
    emitter.instruction("call __rt_incref");                                    // retain the accumulator for its args-array slot
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the accumulator pointer to store
    emitter.instruction("jmp __rt_pdo_call_agg_step_slot0_store_x86");          // retained accumulator in rax → store it into slot 0
    emitter.label("__rt_pdo_call_agg_step_slot0_null_x86");
    emitter.instruction("mov rdi, 0");                                          // value_lo unused
    emitter.instruction("mov rsi, 0");                                          // value_hi unused
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = Void/NULL
    emitter.instruction("call __rt_mixed_from_value");                          // rax = boxed PHP null
    emitter.label("__rt_pdo_call_agg_step_slot0_store_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // args array pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], rax");                       // store the accumulator/null into slot 0
    emitter.instruction("mov QWORD PTR [r10], 1");                              // update the array length field to 1

    // -- slot 1: the row number boxed as a Mixed int --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // value_lo = row number
    emitter.instruction("xor esi, esi");                                        // value_hi unused
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = int
    emitter.instruction("call __rt_mixed_from_value");                          // rax = boxed Mixed(int) row number
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // args array pointer
    emitter.instruction("mov QWORD PTR [r10 + 32], rax");                       // store the row number into slot 1
    emitter.instruction("mov QWORD PTR [r10], 2");                              // update the array length field to 2

    // -- boxing loop: box each row ElephcVal into slots 2.. --
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // loop index k = 0
    emitter.label("__rt_pdo_call_agg_step_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");                        // reload the loop index
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the row argument count
    emitter.instruction("cmp r9, r10");                                         // have all row values been boxed?
    emitter.instruction("jge __rt_pdo_call_agg_step_loop_done_x86");            // yes → box the container and invoke
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the argv base pointer
    emitter.instruction("imul rax, r9, 40");                                    // byte offset of ElephcVal[k]
    emitter.instruction("add r11, rax");                                        // r11 = &argv[k]
    emitter.instruction("mov r8, QWORD PTR [r11 + 0]");                         // load the ElephcVal storage-class tag
    emitter.instruction("cmp r8, 1");                                           // SQLITE_INTEGER?
    emitter.instruction("je __rt_pdo_call_agg_step_box_int_x86");               // yes → box as a Mixed int
    emitter.instruction("cmp r8, 2");                                           // SQLITE_FLOAT?
    emitter.instruction("je __rt_pdo_call_agg_step_box_float_x86");             // yes → box as a Mixed float
    emitter.instruction("cmp r8, 3");                                           // SQLITE_TEXT?
    emitter.instruction("je __rt_pdo_call_agg_step_box_str_x86");               // yes → box as a Mixed string
    emitter.instruction("cmp r8, 4");                                           // SQLITE_BLOB?
    emitter.instruction("je __rt_pdo_call_agg_step_box_str_x86");               // yes → blobs box as Mixed strings too
    // -- tag 0 (SQLITE_NULL) or any unexpected code → PHP null (Mixed tag 8) --
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = Void/NULL
    emitter.instruction("xor edi, edi");                                        // value_lo unused
    emitter.instruction("xor esi, esi");                                        // value_hi unused
    emitter.instruction("jmp __rt_pdo_call_agg_step_box_call_x86");             // box the PHP null via the shared boxing tail
    emitter.label("__rt_pdo_call_agg_step_box_int_x86");
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = int
    emitter.instruction("mov rdi, QWORD PTR [r11 + 8]");                        // value_lo = ElephcVal.i
    emitter.instruction("xor esi, esi");                                        // value_hi unused
    emitter.instruction("jmp __rt_pdo_call_agg_step_box_call_x86");             // box the int via the shared boxing tail
    emitter.label("__rt_pdo_call_agg_step_box_float_x86");
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = float
    emitter.instruction("mov rdi, QWORD PTR [r11 + 16]");                       // value_lo = ElephcVal.f raw f64 bit-pattern
    emitter.instruction("xor esi, esi");                                        // value_hi unused
    emitter.instruction("jmp __rt_pdo_call_agg_step_box_call_x86");             // box the float via the shared boxing tail
    emitter.label("__rt_pdo_call_agg_step_box_str_x86");
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string (binary-safe)
    emitter.instruction("mov rdi, QWORD PTR [r11 + 24]");                       // value_lo = ElephcVal.ptr
    emitter.instruction("mov rsi, QWORD PTR [r11 + 32]");                       // value_hi = ElephcVal.len
    emitter.instruction("jmp __rt_pdo_call_agg_step_box_call_x86");             // box the string via the shared boxing tail
    emitter.label("__rt_pdo_call_agg_step_box_call_x86");
    emitter.instruction("call __rt_mixed_from_value");                          // rax = owned boxed Mixed row value
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");                        // reload the loop index (clobbered by the call)
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the args array pointer
    emitter.instruction("mov rcx, r9");                                         // compute the element byte offset
    emitter.instruction("add rcx, 2");                                          // element index = k + 2 (past [context, rownumber])
    emitter.instruction("shl rcx, 3");                                          // (k + 2) * 8
    emitter.instruction("add rcx, 24");                                         // element region begins 24 bytes past the header
    emitter.instruction("mov QWORD PTR [r11 + rcx], rax");                      // store the boxed row value into slot k+2
    emitter.instruction("add r9, 1");                                           // advance the loop index
    emitter.instruction("mov QWORD PTR [rbp - 80], r9");                        // persist the loop index
    emitter.instruction("add r9, 2");                                           // total element count = (k+1) + 2
    emitter.instruction("mov QWORD PTR [r11], r9");                             // update the array length field
    emitter.instruction("jmp __rt_pdo_call_agg_step_loop_x86");                 // box the next row value
    emitter.label("__rt_pdo_call_agg_step_loop_done_x86");

    // -- box the indexed array as a Mixed cell (tag 4 increfs the array) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // raw args array pointer → payload lo
    emitter.instruction("xor esi, esi");                                        // payload hi unused for an array
    emitter.instruction("mov eax, 4");                                          // runtime tag 4 = indexed array
    emitter.instruction("call __rt_mixed_from_value");                          // rax = boxed Mixed argument cell
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the boxed Mixed argument cell

    // -- push a setjmp firewall handler around the invoke --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", X86_HANDLER_BASE)); // handler record: record.next
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", X86_HANDLER_BASE - 8)); // handler record: survivor frame
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", X86_HANDLER_BASE - 16)); // handler record: saved diagnostic depth
    emitter.instruction(&format!("lea r10, [rbp - {}]", X86_HANDLER_BASE));     // r10 = address of this handler record
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // link the record as the active handler
    emitter.instruction(&format!("lea rdi, [rbp - {}]", X86_HANDLER_BASE - 24)); // rdi = &jmp_buf inside the handler record (record + 24)
    emitter.save_exception_handler();                                           // returns 0 on first pass, 1 when a throw longjmps back
    emitter.instruction("test rax, rax");                                       // did control arrive via longjmp?
    emitter.instruction("jne __rt_pdo_call_agg_step_threw_x86");                // nonzero → arrived via longjmp

    // -- normal path: invoke the step callback through its descriptor (offset 56) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // arg0 = descriptor pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // arg1 = boxed Mixed argument cell
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the uniform invoker pointer
    emitter.instruction("call r10");                                            // invoke step(...) → OWNED boxed Mixed new accumulator in rax
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the new accumulator

    // pop the firewall handler before any further runtime calls
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", X86_HANDLER_BASE)); // record.next
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", X86_HANDLER_BASE - 16)); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0); // restore it

    // -- release the args container (drops the slot-0 accumulator incref) --
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // boxed Mixed argument cell
    emitter.instruction("call __rt_decref_mixed");                              // release the cell
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // raw args array pointer
    emitter.instruction("call __rt_decref_any");                                // release the array and deep-free its boxed args

    // -- release the OLD accumulator's own ref (it is being replaced) --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // old accumulator
    emitter.instruction("test rax, rax");                                       // null (first row)?
    emitter.instruction("jz __rt_pdo_call_agg_step_return_x86");                // yes → nothing to release
    emitter.instruction("call __rt_decref_mixed");                              // drop the old accumulator's group-slot ref
    emitter.label("__rt_pdo_call_agg_step_return_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // return the owned new accumulator
    emitter.instruction(&format!("add rsp, {}", X86_FRAME_SIZE));               // release the adapter frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the new accumulator to the bridge dispatcher

    // -- longjmp path: the step callback threw --
    emitter.label("__rt_pdo_call_agg_step_threw_x86");
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", X86_HANDLER_BASE)); // record.next
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", X86_HANDLER_BASE - 16)); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0); // restore it
    abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0); // take ownership of the pending Throwable
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0); // clear the exception slot before release
    emitter.instruction("test rax, rax");                                       // was a Throwable actually published?
    emitter.instruction("jz __rt_pdo_call_agg_step_threw_released_x86");        // tolerate a defensive null exception slot
    emitter.instruction("call __rt_decref_any");                                // release the caught Throwable object
    emitter.label("__rt_pdo_call_agg_step_threw_released_x86");
    // release the args container (drops the slot-0 incref) but PRESERVE the accumulator
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // boxed Mixed argument cell
    emitter.instruction("call __rt_decref_mixed");                              // release the cell
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // raw args array pointer
    emitter.instruction("call __rt_decref_any");                                // release the array (the slot-0 incref is dropped here)
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // threw out-param pointer
    emitter.instruction("mov QWORD PTR [rax], 1");                              // *threw = 1
    emitter.instruction("xor eax, eax");                                        // return null (the bridge preserves the old accumulator)
    emitter.instruction(&format!("add rsp, {}", X86_FRAME_SIZE));               // release the adapter frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the bridge dispatcher

    // -- fast path: no uniform invoker → pass the accumulator through, nothing allocated --
    emitter.label("__rt_pdo_call_agg_step_no_invoker_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the accumulator unchanged
    emitter.instruction(&format!("add rsp, {}", X86_FRAME_SIZE));               // release the adapter frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the bridge dispatcher
}
