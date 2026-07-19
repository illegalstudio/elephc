//! Purpose:
//! Emits `__rt_pdo_call_scalar`, the codegen adapter that re-enters a compiled-PHP
//! scalar SQL user function on behalf of the `elephc-pdo` bridge. It is the runtime
//! half of the PDO Tier-D "decompose-at-PHP" design for `Pdo\Sqlite::createFunction`:
//! the bridge stores this adapter's address (obtained through
//! `__elephc_pdo_adapter_addr(1)`) together with the callable's descriptor pointer per
//! SQLite registration, and its `x_scalar` dispatcher calls back here once per row with
//! the argument vector SQLite provides.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::pdo`, gated by `RuntimeFeatures::pdo_udf`.
//! - The `elephc-pdo` bridge (`SqliteConn::create_function` → `x_scalar`), which only
//!   stores and calls this adapter's address; it never references a `__rt_*` symbol
//!   itself (respecting the non-whole-archive linker boundary).
//!
//! Key details:
//! - C ABI: `__rt_pdo_call_scalar(descriptor, argv, argc, out)` returning nothing; the
//!   result is written through `out` (a bridge-owned `ElephcResult`). `argv` is a
//!   contiguous array of `argc` `ElephcVal` records (40 bytes each: tag@0, i@8, f@16,
//!   ptr@24, len@32), and `out` is an `ElephcResult` (24 bytes: tag@0, i@8, f@16).
//! - Argument boxing (unlike the fixed two-string collation build) is a dynamic loop
//!   over `argc`: each `ElephcVal` is translated from its SQLite storage-class tag
//!   (1=INT, 2=FLOAT, 3=TEXT, 4=BLOB, 0=NULL) into a runtime Mixed tag (INT→0, FLOAT→2
//!   with the f64 bit-pattern kept in the integer lo register, TEXT/BLOB→1 string,
//!   NULL→8) and boxed through `__rt_mixed_from_value` into a `value_type = 7` (Mixed)
//!   indexed args array. `__rt_mixed_from_value` with tag 1 deep-copies the bytes via
//!   `__rt_str_persist`, so SQLite's transient argv buffers may be invalidated the
//!   moment this returns. The array is boxed as a Mixed cell (tag 4) and passed as the
//!   invoker's argument container.
//! - Return decoding is type-preserving (mirroring the bind path rather than a lossy
//!   `__rt_mixed_cast_*`): the owned boxed return is `__rt_mixed_unbox`-ed once and the
//!   tag dispatched — int→`out.tag = 1`, float→`out.tag = 2` (raw f64 bits into
//!   `out.f`), string→bytes staged into the bridge via `elephc_pdo_udf_stash_bytes`
//!   then `out.tag = 3`, bool→`out.tag = 5`, null→`out.tag = 0`, arrays→6, and
//!   objects/callables→7. The latter two let the bridge reject unsupported callback
//!   results instead of silently converting them to SQL NULL. The boxed return
//!   is released after the bytes are staged (the stash deep-copies them out of the
//!   about-to-be-freed cell).
//! - Exception firewall: identical to the collation adapter. A compiled-PHP `throw` is
//!   a `longjmp`; letting it cross this C boundary would unwind over SQLite's VDBE and
//!   the Rust bridge frame (deadlock/UB/exit). The adapter pushes its own `setjmp`
//!   handler record (the same 224-byte layout as the EIR try/catch slot) around the
//!   invoke; on a `longjmp` it pops the handler, swallows the pending exception, and
//!   writes `out.tag = -1`, which the bridge dispatcher turns into a `sqlite3_result_error`
//!   (surfacing as a PDOException at the query boundary). Re-raising the original
//!   exception object is a later hardening step; the load-bearing guarantee is that the
//!   `throw` never unwinds past this C boundary.

use crate::codegen_support::callable_descriptor::CALLABLE_DESC_INVOKER_OFFSET;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

// The firewall builds its handler record by hand and must match the layout the EIR
// try/catch machinery and `__rt_throw_current` assume: next@0, survivor@8,
// diag@TRY_HANDLER_DIAG_DEPTH_OFFSET, jmp_buf@TRY_HANDLER_JMP_BUF_OFFSET. Assert the
// two ABI-critical offsets at compile time so a constant drift breaks the build here
// rather than corrupting a `longjmp` at runtime.
const _: () = assert!(TRY_HANDLER_DIAG_DEPTH_OFFSET == 16);
const _: () = assert!(TRY_HANDLER_JMP_BUF_OFFSET == 24);

/// Emits `__rt_pdo_call_scalar(descriptor, argv, argc, out)`.
///
/// Inputs (AArch64): x0 = descriptor, x1 = argv, x2 = argc, x3 = out. No return value;
/// the result is written through `out`.
/// (x86_64): rdi, rsi, rdx, rcx in the same order.
pub fn emit_pdo_call_scalar(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pdo_call_scalar_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: pdo_call_scalar ---");
    emitter.label_global("__rt_pdo_call_scalar");

    // Stack frame (304 bytes):
    //   [sp, #0]   = handler record (224 bytes): next@0, survivor@8, diag@16,
    //                jmp_buf@24 (matches TRY_HANDLER_* / __rt_throw_current).
    //   [sp, #224] = descriptor      [sp, #232] = argv       [sp, #240] = argc
    //   [sp, #248] = out ptr         [sp, #256] = args array ptr
    //   [sp, #264] = boxed args cell [sp, #272] = boxed return  [sp, #280] = loop index
    //   [sp, #288] = saved x29       [sp, #296] = saved x30
    emitter.instruction("sub sp, sp, #304"); // allocate the scalar-adapter frame
    emitter.instruction("stp x29, x30, [sp, #288]"); // save frame pointer and return address
    emitter.instruction("add x29, sp, #288"); // establish the adapter frame pointer

    emitter.instruction("str x0, [sp, #224]"); // save descriptor pointer
    emitter.instruction("str x1, [sp, #232]"); // save argv base pointer
    emitter.instruction("str x2, [sp, #240]"); // save argument count
    emitter.instruction("str x3, [sp, #248]"); // save the result-out pointer

    // -- fast path: a descriptor without a uniform invoker yields SQL NULL --
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the invoker slot
    emitter.instruction("cbz x9, __rt_pdo_call_scalar_null_result"); // no invoker → NULL result, nothing to release

    // -- allocate an argc-slot argument array and stamp value_type = Mixed once --
    emitter.instruction("ldr x0, [sp, #240]"); // capacity = argument count (0 is a valid header-only array)
    emitter.instruction("mov x1, #8"); // boxed Mixed slots store one pointer each
    emitter.instruction("bl __rt_array_new"); // x0 = indexed array backing storage
    emitter.instruction("ldr x10, [x0, #-8]"); // load the packed array kind word from the header
    emitter.instruction("mov x12, #0x80ff"); // preserve the indexed-array kind and persistent COW flag
    emitter.instruction("and x10, x10, x12"); // keep only the persistent metadata bits
    emitter.instruction("mov x11, #7"); // value_type tag 7 = boxed Mixed
    emitter.instruction("lsl x11, x11, #8"); // move the tag into the packed kind-word byte lane
    emitter.instruction("orr x10, x10, x11"); // combine the heap kind with the value_type tag
    emitter.instruction("str x10, [x0, #-8]"); // persist the stamped kind word (never re-stamped: no push_int)
    emitter.instruction("str x0, [sp, #256]"); // save the args array pointer

    // -- boxing loop: box each ElephcVal arg into a Mixed and store it into the array --
    emitter.instruction("str xzr, [sp, #280]"); // loop index k = 0
    emitter.label("__rt_pdo_call_scalar_loop");
    emitter.instruction("ldr x9, [sp, #280]"); // reload the loop index
    emitter.instruction("ldr x10, [sp, #240]"); // reload the argument count
    emitter.instruction("cmp x9, x10"); // have all arguments been boxed?
    emitter.instruction("b.ge __rt_pdo_call_scalar_loop_done"); // yes → box the container and invoke
    emitter.instruction("ldr x11, [sp, #232]"); // reload the argv base pointer
    emitter.instruction("mov x12, #40"); // ElephcVal stride in bytes (tag@0,i@8,f@16,ptr@24,len@32)
    emitter.instruction("mul x13, x9, x12"); // byte offset of ElephcVal[k]
    emitter.instruction("add x11, x11, x13"); // x11 = &argv[k]
    emitter.instruction("ldr x14, [x11, #0]"); // load the ElephcVal storage-class tag
    emitter.instruction("cmp x14, #1"); // SQLITE_INTEGER?
    emitter.instruction("b.eq __rt_pdo_call_scalar_box_int");
    emitter.instruction("cmp x14, #2"); // SQLITE_FLOAT?
    emitter.instruction("b.eq __rt_pdo_call_scalar_box_float");
    emitter.instruction("cmp x14, #3"); // SQLITE_TEXT?
    emitter.instruction("b.eq __rt_pdo_call_scalar_box_str");
    emitter.instruction("cmp x14, #4"); // SQLITE_BLOB?
    emitter.instruction("b.eq __rt_pdo_call_scalar_box_str");
    // -- tag 0 (SQLITE_NULL) or any unexpected code → PHP null (Mixed tag 8) --
    emitter.instruction("mov x0, #8"); // runtime tag 8 = Void/NULL
    emitter.instruction("mov x1, #0"); // value_lo unused
    emitter.instruction("mov x2, #0"); // value_hi unused
    emitter.instruction("b __rt_pdo_call_scalar_box_call");
    // -- SQLITE_INTEGER → Mixed int (tag 0) --
    emitter.label("__rt_pdo_call_scalar_box_int");
    emitter.instruction("mov x0, #0"); // runtime tag 0 = int
    emitter.instruction("ldr x1, [x11, #8]"); // value_lo = ElephcVal.i
    emitter.instruction("mov x2, #0"); // value_hi unused
    emitter.instruction("b __rt_pdo_call_scalar_box_call");
    // -- SQLITE_FLOAT → Mixed float (tag 2); the f64 bit-pattern travels in lo --
    emitter.label("__rt_pdo_call_scalar_box_float");
    emitter.instruction("mov x0, #2"); // runtime tag 2 = float
    emitter.instruction("ldr x1, [x11, #16]"); // value_lo = ElephcVal.f raw f64 bit-pattern (integer reg, not FP)
    emitter.instruction("mov x2, #0"); // value_hi unused
    emitter.instruction("b __rt_pdo_call_scalar_box_call");
    // -- SQLITE_TEXT/BLOB → Mixed string (tag 1); from_value deep-copies the bytes --
    emitter.label("__rt_pdo_call_scalar_box_str");
    emitter.instruction("mov x0, #1"); // runtime tag 1 = string (binary-safe: no separate blob tag)
    emitter.instruction("ldr x1, [x11, #24]"); // value_lo = ElephcVal.ptr (byte pointer)
    emitter.instruction("ldr x2, [x11, #32]"); // value_hi = ElephcVal.len (explicit byte length)
    emitter.instruction("b __rt_pdo_call_scalar_box_call");
    // -- box the (tag, lo, hi) triple and store it into args array slot k --
    emitter.label("__rt_pdo_call_scalar_box_call");
    emitter.instruction("bl __rt_mixed_from_value"); // x0 = owned boxed Mixed argument
    emitter.instruction("ldr x9, [sp, #280]"); // reload the loop index (clobbered by the call)
    // The args-array reload uses the caller-saved temporary x10 (like the collation
    // template's x9), NOT a callee-saved register: this adapter's prologue saves only
    // x29/x30, so clobbering an x19-x28 register would corrupt a value the bridge's
    // x_scalar caller may be holding live across the call. x9 (loop index) and x12
    // (element offset) are the only other live temporaries here.
    emitter.instruction("ldr x10, [sp, #256]"); // reload the args array pointer
    emitter.instruction("lsl x12, x9, #3"); // k * 8 (boxed-Mixed slot stride)
    emitter.instruction("add x12, x12, #24"); // element region begins 24 bytes past the header
    emitter.instruction("str x0, [x10, x12]"); // store the boxed arg into slot k
    emitter.instruction("add x9, x9, #1"); // advance the loop index
    emitter.instruction("str x9, [sp, #280]"); // persist the loop index
    emitter.instruction("str x9, [x10, #0]"); // update the array length field to k+1
    emitter.instruction("b __rt_pdo_call_scalar_loop"); // box the next argument
    emitter.label("__rt_pdo_call_scalar_loop_done");

    // -- box the indexed array as a Mixed cell (tag 4 increfs the array) --
    emitter.instruction("ldr x1, [sp, #256]"); // raw args array pointer → payload lo
    emitter.instruction("mov x2, #0"); // payload hi unused for an array
    emitter.instruction("mov x0, #4"); // runtime tag 4 = indexed array
    emitter.instruction("bl __rt_mixed_from_value"); // x0 = boxed Mixed argument cell
    emitter.instruction("str x0, [sp, #264]"); // save the boxed Mixed argument cell

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
    emitter.instruction("cbnz x0, __rt_pdo_call_scalar_threw"); // nonzero → arrived via longjmp

    // -- normal path: invoke the user function through its descriptor (offset 56) --
    emitter.instruction("ldr x0, [sp, #224]"); // arg0 = descriptor pointer
    emitter.instruction("ldr x1, [sp, #264]"); // arg1 = boxed Mixed argument cell
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the uniform invoker pointer
    emitter.instruction("blr x9"); // invoke callable(...args) → OWNED boxed Mixed return in x0
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
    emitter.instruction("b.eq __rt_pdo_call_scalar_ret_int");
    emitter.instruction("cmp x0, #2"); // Mixed float?
    emitter.instruction("b.eq __rt_pdo_call_scalar_ret_float");
    emitter.instruction("cmp x0, #1"); // Mixed string?
    emitter.instruction("b.eq __rt_pdo_call_scalar_ret_string");
    emitter.instruction("cmp x0, #3"); // Mixed bool?
    emitter.instruction("b.eq __rt_pdo_call_scalar_ret_bool");
    emitter.instruction("cmp x0, #4"); // Mixed indexed array?
    emitter.instruction("b.eq __rt_pdo_call_scalar_ret_array");
    emitter.instruction("cmp x0, #5"); // Mixed associative array?
    emitter.instruction("b.eq __rt_pdo_call_scalar_ret_array");
    emitter.instruction("cmp x0, #6"); // Mixed object?
    emitter.instruction("b.eq __rt_pdo_call_scalar_ret_object");
    emitter.instruction("cmp x0, #10"); // Mixed callable descriptor?
    emitter.instruction("b.eq __rt_pdo_call_scalar_ret_object");
    // -- tag 8 (null) or an unknown tag → SQL NULL --
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("str xzr, [x11, #0]"); // out.tag = 0 (NULL)
    emitter.instruction("b __rt_pdo_call_scalar_release_return");
    emitter.label("__rt_pdo_call_scalar_ret_int");
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("mov x10, #1"); // ElephcResult tag 1 = INT
    emitter.instruction("str x10, [x11, #0]"); // out.tag = 1
    emitter.instruction("str x1, [x11, #8]"); // out.i = lo (int64 value)
    emitter.instruction("b __rt_pdo_call_scalar_release_return");
    emitter.label("__rt_pdo_call_scalar_ret_float");
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("mov x10, #2"); // ElephcResult tag 2 = FLOAT
    emitter.instruction("str x10, [x11, #0]"); // out.tag = 2
    emitter.instruction("str x1, [x11, #16]"); // out.f = lo (raw f64 bit-pattern → stored as f64)
    emitter.instruction("b __rt_pdo_call_scalar_release_return");
    emitter.label("__rt_pdo_call_scalar_ret_bool");
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("mov x10, #5"); // ElephcResult tag 5 = BOOL
    emitter.instruction("str x10, [x11, #0]"); // out.tag = 5
    emitter.instruction("str x1, [x11, #8]"); // out.i = lo (0/1)
    emitter.instruction("b __rt_pdo_call_scalar_release_return");
    emitter.label("__rt_pdo_call_scalar_ret_array");
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("mov x10, #6"); // ElephcResult tag 6 = unsupported PHP array
    emitter.instruction("str x10, [x11, #0]"); // report the array type to the bridge
    emitter.instruction("b __rt_pdo_call_scalar_release_return");
    emitter.label("__rt_pdo_call_scalar_ret_object");
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("mov x10, #7"); // ElephcResult tag 7 = unsupported PHP object/callable
    emitter.instruction("str x10, [x11, #0]"); // report the object type to the bridge
    emitter.instruction("b __rt_pdo_call_scalar_release_return");
    // -- string: stage the bytes into the bridge BEFORE releasing the owned box --
    emitter.label("__rt_pdo_call_scalar_ret_string");
    emitter.instruction("mov x0, x1"); // stash arg0 = byte pointer (unbox lo)
    emitter.instruction("mov x1, x2"); // stash arg1 = byte length (unbox hi)
    emitter.instruction("mov x2, #0"); // stash arg2 = is_blob 0 (return text; embedded NULs still preserved by length)
    emitter.bl_c("elephc_pdo_udf_stash_bytes"); // deep-copy the string bytes into the bridge's per-thread stash
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("mov x10, #3"); // ElephcResult tag 3 = TEXT (bytes live in the stash)
    emitter.instruction("str x10, [x11, #0]"); // out.tag = 3
    emitter.instruction("b __rt_pdo_call_scalar_release_return");

    // -- release the owned boxed return, then the argument container --
    emitter.label("__rt_pdo_call_scalar_release_return");
    emitter.instruction("ldr x0, [sp, #272]"); // boxed return
    emitter.instruction("bl __rt_decref_mixed"); // release the invoker's owned return (bytes already staged)
    emitter.instruction("b __rt_pdo_call_scalar_cleanup"); // join the shared container-release path

    // -- longjmp path: the callback threw --
    emitter.label("__rt_pdo_call_scalar_threw");
    emitter.instruction("ldr x10, [sp, #0]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("ldr x10, [sp, #16]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0); // restore it
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0); // swallow the pending exception (surfaced as a SQL error)
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("mov x10, #-1"); // ElephcResult tag -1 = ERROR (bridge raises sqlite3_result_error)
    emitter.instruction("str x10, [x11, #0]"); // out.tag = -1

    // -- shared cleanup: release the argument container --
    emitter.label("__rt_pdo_call_scalar_cleanup");
    emitter.instruction("ldr x0, [sp, #264]"); // boxed Mixed argument cell
    emitter.instruction("bl __rt_decref_mixed"); // release the cell (drops the array ref boxing took)
    emitter.instruction("ldr x0, [sp, #256]"); // raw args array pointer
    emitter.instruction("bl __rt_decref_any"); // release the array and deep-free its boxed args
    emitter.instruction("ldp x29, x30, [sp, #288]"); // restore frame pointer and return address
    emitter.instruction("add sp, sp, #304"); // release the adapter frame
    emitter.instruction("ret"); // return to the bridge dispatcher (result is in *out)

    // -- fast path: no uniform invoker → NULL result, nothing allocated to release --
    emitter.label("__rt_pdo_call_scalar_null_result");
    emitter.instruction("ldr x11, [sp, #248]"); // out pointer
    emitter.instruction("str xzr, [x11, #0]"); // out.tag = 0 (NULL)
    emitter.instruction("ldp x29, x30, [sp, #288]"); // restore frame pointer and return address
    emitter.instruction("add sp, sp, #304"); // release the adapter frame
    emitter.instruction("ret"); // return to the bridge dispatcher
}

/// x86_64 implementation of `__rt_pdo_call_scalar`.
fn emit_pdo_call_scalar_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pdo_call_scalar ---");
    emitter.label_global("__rt_pdo_call_scalar");

    // Frame (288 bytes below rbp):
    //   [rbp-8]   descriptor   [rbp-16] argv     [rbp-24] argc     [rbp-32] out ptr
    //   [rbp-40]  args array   [rbp-48] boxed args cell           [rbp-56] boxed return
    //   [rbp-64]  loop index
    //   [rbp-288] handler record (224 bytes): next@0, survivor@8, diag@16, jmp_buf@24
    //             → record.next=[rbp-288], survivor=[rbp-280], diag=[rbp-272],
    //               jmp_buf base=[rbp-264].
    //   push rbp + sub rsp,288 keeps rsp 16-aligned for the nested calls.
    emitter.instruction("push rbp"); // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp"); // establish the adapter frame pointer
    emitter.instruction("sub rsp, 288"); // reserve the slots and the 224-byte handler record

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi"); // save descriptor pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi"); // save argv base pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx"); // save argument count
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx"); // save the result-out pointer

    // -- fast path: a descriptor without a uniform invoker yields SQL NULL --
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the invoker slot
    emitter.instruction("test r10, r10"); // does the descriptor expose a uniform invoker?
    emitter.instruction("jz __rt_pdo_call_scalar_null_result_x86"); // no invoker → NULL result, nothing to release

    // -- allocate an argc-slot argument array and stamp value_type = Mixed once --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]"); // capacity = argument count (0 is a valid header-only array)
    emitter.instruction("mov rsi, 8"); // boxed Mixed slots store one pointer each
    emitter.instruction("call __rt_array_new"); // rax = indexed array backing storage
    emitter.instruction("mov r10, QWORD PTR [rax - 8]"); // load the packed array kind word from the header
    emitter.instruction("mov r11, 0xffffffff000080ff"); // preserve heap marker + indexed-array kind + COW bit
    emitter.instruction("and r10, r11"); // keep only the persistent metadata bits
    emitter.instruction("mov r11, 7"); // value_type tag 7 = boxed Mixed
    emitter.instruction("shl r11, 8"); // move the tag into the packed kind-word byte lane
    emitter.instruction("or r10, r11"); // combine the heap kind with the value_type tag
    emitter.instruction("mov QWORD PTR [rax - 8], r10"); // persist the stamped kind word (never re-stamped: no push_int)
    emitter.instruction("mov QWORD PTR [rbp - 40], rax"); // save the args array pointer

    // -- boxing loop: box each ElephcVal arg into a Mixed and store it into the array --
    emitter.instruction("mov QWORD PTR [rbp - 64], 0"); // loop index k = 0
    emitter.label("__rt_pdo_call_scalar_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]"); // reload the loop index
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]"); // reload the argument count
    emitter.instruction("cmp r9, r10"); // have all arguments been boxed?
    emitter.instruction("jge __rt_pdo_call_scalar_loop_done_x86"); // yes → box the container and invoke
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]"); // reload the argv base pointer
    emitter.instruction("imul rax, r9, 40"); // byte offset of ElephcVal[k] (40-byte stride)
    emitter.instruction("add r11, rax"); // r11 = &argv[k]
    emitter.instruction("mov r8, QWORD PTR [r11 + 0]"); // load the ElephcVal storage-class tag
    emitter.instruction("cmp r8, 1"); // SQLITE_INTEGER?
    emitter.instruction("je __rt_pdo_call_scalar_box_int_x86");
    emitter.instruction("cmp r8, 2"); // SQLITE_FLOAT?
    emitter.instruction("je __rt_pdo_call_scalar_box_float_x86");
    emitter.instruction("cmp r8, 3"); // SQLITE_TEXT?
    emitter.instruction("je __rt_pdo_call_scalar_box_str_x86");
    emitter.instruction("cmp r8, 4"); // SQLITE_BLOB?
    emitter.instruction("je __rt_pdo_call_scalar_box_str_x86");
    // -- tag 0 (SQLITE_NULL) or any unexpected code → PHP null (Mixed tag 8) --
    emitter.instruction("mov eax, 8"); // runtime tag 8 = Void/NULL
    emitter.instruction("xor edi, edi"); // value_lo unused
    emitter.instruction("xor esi, esi"); // value_hi unused
    emitter.instruction("jmp __rt_pdo_call_scalar_box_call_x86");
    // -- SQLITE_INTEGER → Mixed int (tag 0) --
    emitter.label("__rt_pdo_call_scalar_box_int_x86");
    emitter.instruction("mov eax, 0"); // runtime tag 0 = int
    emitter.instruction("mov rdi, QWORD PTR [r11 + 8]"); // value_lo = ElephcVal.i
    emitter.instruction("xor esi, esi"); // value_hi unused
    emitter.instruction("jmp __rt_pdo_call_scalar_box_call_x86");
    // -- SQLITE_FLOAT → Mixed float (tag 2); the f64 bit-pattern travels in lo --
    emitter.label("__rt_pdo_call_scalar_box_float_x86");
    emitter.instruction("mov eax, 2"); // runtime tag 2 = float
    emitter.instruction("mov rdi, QWORD PTR [r11 + 16]"); // value_lo = ElephcVal.f raw f64 bit-pattern (integer reg, not xmm)
    emitter.instruction("xor esi, esi"); // value_hi unused
    emitter.instruction("jmp __rt_pdo_call_scalar_box_call_x86");
    // -- SQLITE_TEXT/BLOB → Mixed string (tag 1); from_value deep-copies the bytes --
    emitter.label("__rt_pdo_call_scalar_box_str_x86");
    emitter.instruction("mov eax, 1"); // runtime tag 1 = string (binary-safe: no separate blob tag)
    emitter.instruction("mov rdi, QWORD PTR [r11 + 24]"); // value_lo = ElephcVal.ptr (byte pointer)
    emitter.instruction("mov rsi, QWORD PTR [r11 + 32]"); // value_hi = ElephcVal.len (explicit byte length)
    emitter.instruction("jmp __rt_pdo_call_scalar_box_call_x86");
    // -- box the (tag, lo, hi) triple and store it into args array slot k --
    emitter.label("__rt_pdo_call_scalar_box_call_x86");
    emitter.instruction("call __rt_mixed_from_value"); // rax = owned boxed Mixed argument
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]"); // reload the loop index (clobbered by the call)
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]"); // reload the args array pointer
    emitter.instruction("mov rcx, r9"); // compute the element byte offset
    emitter.instruction("shl rcx, 3"); // k * 8 (boxed-Mixed slot stride)
    emitter.instruction("add rcx, 24"); // element region begins 24 bytes past the header
    emitter.instruction("mov QWORD PTR [r11 + rcx], rax"); // store the boxed arg into slot k
    emitter.instruction("add r9, 1"); // advance the loop index
    emitter.instruction("mov QWORD PTR [rbp - 64], r9"); // persist the loop index
    emitter.instruction("mov QWORD PTR [r11], r9"); // update the array length field to k+1
    emitter.instruction("jmp __rt_pdo_call_scalar_loop_x86"); // box the next argument
    emitter.label("__rt_pdo_call_scalar_loop_done_x86");

    // -- box the indexed array as a Mixed cell (tag 4 increfs the array) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]"); // raw args array pointer → payload lo
    emitter.instruction("xor esi, esi"); // payload hi unused for an array
    emitter.instruction("mov eax, 4"); // runtime tag 4 = indexed array
    emitter.instruction("call __rt_mixed_from_value"); // rax = boxed Mixed argument cell
    emitter.instruction("mov QWORD PTR [rbp - 48], rax"); // save the boxed Mixed argument cell

    // -- push a setjmp firewall handler around the invoke --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0); // previous handler-stack top
    emitter.instruction("mov QWORD PTR [rbp - 288], r10"); // handler record: record.next
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0); // live activation-frame top
    emitter.instruction("mov QWORD PTR [rbp - 280], r10"); // handler record: survivor frame (cleanup stops here)
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0); // current diagnostic-suppression depth
    emitter.instruction("mov QWORD PTR [rbp - 272], r10"); // handler record: saved diagnostic depth
    emitter.instruction("lea r10, [rbp - 288]"); // r10 = address of this handler record
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // link the record as the active handler
    emitter.instruction("lea rdi, [rbp - 264]"); // rdi = &jmp_buf inside the handler record (record + 24)
    emitter.bl_c("setjmp"); // returns 0 on first pass, 1 when a throw longjmps back
    emitter.instruction("test rax, rax"); // did control arrive via longjmp?
    emitter.instruction("jne __rt_pdo_call_scalar_threw_x86"); // nonzero → arrived via longjmp

    // -- normal path: invoke the user function through its descriptor (offset 56) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]"); // arg0 = descriptor pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]"); // arg1 = boxed Mixed argument cell
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the uniform invoker pointer
    emitter.instruction("call r10"); // invoke callable(...args) → OWNED boxed Mixed return in rax
    emitter.instruction("mov QWORD PTR [rbp - 56], rax"); // save the boxed return for decode + release

    // pop the firewall handler before any further runtime calls
    emitter.instruction("mov r10, QWORD PTR [rbp - 288]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("mov r10, QWORD PTR [rbp - 272]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0); // restore it

    // -- type-preserving decode: unbox once and dispatch on the runtime tag --
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]"); // boxed return (unbox reads RAX)
    emitter.instruction("call __rt_mixed_unbox"); // rax = tag, rdi = lo, rdx = hi (tag-7 wrappers peeled)
    emitter.instruction("cmp rax, 0"); // Mixed int?
    emitter.instruction("je __rt_pdo_call_scalar_ret_int_x86");
    emitter.instruction("cmp rax, 2"); // Mixed float?
    emitter.instruction("je __rt_pdo_call_scalar_ret_float_x86");
    emitter.instruction("cmp rax, 1"); // Mixed string?
    emitter.instruction("je __rt_pdo_call_scalar_ret_string_x86");
    emitter.instruction("cmp rax, 3"); // Mixed bool?
    emitter.instruction("je __rt_pdo_call_scalar_ret_bool_x86");
    emitter.instruction("cmp rax, 4"); // Mixed indexed array?
    emitter.instruction("je __rt_pdo_call_scalar_ret_array_x86");
    emitter.instruction("cmp rax, 5"); // Mixed associative array?
    emitter.instruction("je __rt_pdo_call_scalar_ret_array_x86");
    emitter.instruction("cmp rax, 6"); // Mixed object?
    emitter.instruction("je __rt_pdo_call_scalar_ret_object_x86");
    emitter.instruction("cmp rax, 10"); // Mixed callable descriptor?
    emitter.instruction("je __rt_pdo_call_scalar_ret_object_x86");
    // -- tag 8 (null) or an unknown tag → SQL NULL --
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 0"); // out.tag = 0 (NULL)
    emitter.instruction("jmp __rt_pdo_call_scalar_release_return_x86");
    emitter.label("__rt_pdo_call_scalar_ret_int_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 1"); // out.tag = 1 (INT)
    emitter.instruction("mov QWORD PTR [r11 + 8], rdi"); // out.i = lo (int64 value)
    emitter.instruction("jmp __rt_pdo_call_scalar_release_return_x86");
    emitter.label("__rt_pdo_call_scalar_ret_float_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 2"); // out.tag = 2 (FLOAT)
    emitter.instruction("mov QWORD PTR [r11 + 16], rdi"); // out.f = lo (raw f64 bit-pattern → stored as f64)
    emitter.instruction("jmp __rt_pdo_call_scalar_release_return_x86");
    emitter.label("__rt_pdo_call_scalar_ret_bool_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 5"); // out.tag = 5 (BOOL)
    emitter.instruction("mov QWORD PTR [r11 + 8], rdi"); // out.i = lo (0/1)
    emitter.instruction("jmp __rt_pdo_call_scalar_release_return_x86");
    emitter.label("__rt_pdo_call_scalar_ret_array_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 6"); // out.tag = unsupported PHP array
    emitter.instruction("jmp __rt_pdo_call_scalar_release_return_x86");
    emitter.label("__rt_pdo_call_scalar_ret_object_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 7"); // out.tag = unsupported PHP object/callable
    emitter.instruction("jmp __rt_pdo_call_scalar_release_return_x86");
    // -- string: stage the bytes into the bridge BEFORE releasing the owned box --
    emitter.label("__rt_pdo_call_scalar_ret_string_x86");
    emitter.instruction("mov rsi, rdx"); // stash arg1 = byte length (unbox hi), before rdx is reused
    emitter.instruction("xor edx, edx"); // stash arg2 = is_blob 0 (return text; embedded NULs preserved by length)
    // rdi already holds the unbox lo (byte pointer) = stash arg0.
    emitter.bl_c("elephc_pdo_udf_stash_bytes"); // deep-copy the string bytes into the bridge's per-thread stash
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 3"); // out.tag = 3 (TEXT; bytes live in the stash)
    emitter.instruction("jmp __rt_pdo_call_scalar_release_return_x86");

    // -- release the owned boxed return, then the argument container --
    emitter.label("__rt_pdo_call_scalar_release_return_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]"); // boxed return
    emitter.instruction("call __rt_decref_mixed"); // release the invoker's owned return (bytes already staged)
    emitter.instruction("jmp __rt_pdo_call_scalar_cleanup_x86"); // join the shared container-release path

    // -- longjmp path: the callback threw --
    emitter.label("__rt_pdo_call_scalar_threw_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 288]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // unlink the handler record
    emitter.instruction("mov r10, QWORD PTR [rbp - 272]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0); // restore it
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0); // swallow the pending exception (surfaced as a SQL error)
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], -1"); // out.tag = -1 (ERROR; bridge raises sqlite3_result_error)

    // -- shared cleanup: release the argument container --
    emitter.label("__rt_pdo_call_scalar_cleanup_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]"); // boxed Mixed argument cell
    emitter.instruction("call __rt_decref_mixed"); // release the cell (drops the array ref boxing took)
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]"); // raw args array pointer
    emitter.instruction("call __rt_decref_any"); // release the array and deep-free its boxed args
    emitter.instruction("add rsp, 288"); // release the adapter frame
    emitter.instruction("pop rbp"); // restore the caller frame pointer
    emitter.instruction("ret"); // return to the bridge dispatcher (result is in *out)

    // -- fast path: no uniform invoker → NULL result, nothing allocated to release --
    emitter.label("__rt_pdo_call_scalar_null_result_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]"); // out pointer
    emitter.instruction("mov QWORD PTR [r11], 0"); // out.tag = 0 (NULL)
    emitter.instruction("add rsp, 288"); // release the adapter frame
    emitter.instruction("pop rbp"); // restore the caller frame pointer
    emitter.instruction("ret"); // return to the bridge dispatcher
}
