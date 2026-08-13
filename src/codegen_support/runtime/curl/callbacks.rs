//! Purpose:
//! Emits `__rt_curl_invoke_callback`, the codegen adapter that re-enters compiled PHP on
//! behalf of the `elephc-curl` bridge when libcurl calls one of `curl_setopt()`'s
//! callback options mid-transfer (`CURLOPT_WRITEFUNCTION`, `CURLOPT_HEADERFUNCTION`,
//! `CURLOPT_READFUNCTION`, `CURLOPT_PROGRESSFUNCTION`, `CURLOPT_XFERINFOFUNCTION`,
//! `CURLOPT_DEBUGFUNCTION`). Also emits `__rt_curl_rethrow_pending`, the post-transfer
//! hook that re-raises an exception a callback threw.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::curl`.
//! - The `elephc-curl` bridge (`crate::callbacks`'s six libcurl trampolines), which only
//!   ever stores and calls this adapter's ADDRESS — obtained by the curl prelude through
//!   `__elephc_curl_adapter_addr()` and passed down as an opaque word. The bridge names
//!   no `__rt_*` symbol, and this adapter names no `elephc_curl_*` symbol, so the
//!   pay-for-use boundary (`crate::codegen_support::runtime::curl`'s pinned test) holds
//!   in both directions. That is also why this family, unlike the PDO adapters, needs no
//!   `RuntimeFeatures` gate: it references nothing outside the shared runtime.
//!
//! Key details:
//! - C ABI: `__rt_curl_invoke_callback(descriptor, curl_handle_object, spec) -> i64`.
//!   `spec` is a bridge-owned `CallSpec` (`crates/elephc-curl/src/callbacks.rs`) with
//!   fixed offsets — `argc@0`, `argv@8`, `result_kind@16`, `out_buf@24`, `out_cap@32`,
//!   `out_len@40`, `status@48` — and `argv` is `argc` 24-byte `(tag, lo, hi)` triples in
//!   `__rt_mixed_from_value`'s own encoding.
//! - ONE adapter serves all six options. The per-callback shaping (which arguments,
//!   which return convention, which libcurl abort code) is plain Rust in the bridge;
//!   only the two things Rust cannot do — boxing a PHP object and invoking a descriptor
//!   — happen here. That is why there is one assembly body per target instead of six.
//! - Argument 0 is ALWAYS the `CurlHandle` object, boxed as runtime tag 6, because only
//!   this side can. `__rt_mixed_from_value` increfs it for the duration of the call and
//!   the argument container's release decrefs it, so the borrow the bridge holds never
//!   has to touch a refcount. A null object normalizes to PHP `null` rather than boxing
//!   a wild pointer.
//! - Return decoding follows `result_kind`. `0` casts through `__rt_mixed_cast_int`,
//!   which is `zval_get_long` — php-src reads write/header/progress returns exactly that
//!   way, so `true` becomes `1` and `null` becomes `0` and both mismatch the chunk length
//!   (measured: `CURLE_WRITE_ERROR`). `1` (READFUNCTION) copies a STRING return into the
//!   bridge's `out_buf`, capped at `out_cap` — php-src's `MIN(size * nmemb, len)` — and
//!   leaves `out_len` at `0` for any non-string return, which libcurl reads as EOF.
//! - Exception firewall: a compiled-PHP `throw` is a `longjmp`, and letting it cross this
//!   boundary would abandon libcurl's own frames mid-transfer. The adapter pushes its own
//!   `setjmp` handler record (the same 224-byte layout as the EIR try/catch slot) around
//!   the invoke; on a `longjmp` it pops the record and reports `status = -1`, which the
//!   bridge turns into that callback's abort code. Unlike the PDO adapters it does NOT
//!   swallow `_exc_value`: the pending throwable stays live so `__rt_curl_rethrow_pending`
//!   can re-raise it the moment `curl_easy_perform`/`curl_multi_perform` returns, which is
//!   what makes `try { curl_exec($ch); } catch (…)` behave as it does in php-src.

use crate::codegen_support::callable_descriptor::CALLABLE_DESC_INVOKER_OFFSET;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

// The firewall builds its handler record by hand and must match the layout the EIR
// try/catch machinery and `__rt_throw_current` assume: next@0, survivor@8,
// diag@TRY_HANDLER_DIAG_DEPTH_OFFSET, jmp_buf@TRY_HANDLER_JMP_BUF_OFFSET. Assert the two
// ABI-critical offsets at compile time so a constant drift breaks the build here rather
// than corrupting a `longjmp` at runtime.
const _: () = assert!(TRY_HANDLER_DIAG_DEPTH_OFFSET == 16);
const _: () = assert!(TRY_HANDLER_JMP_BUF_OFFSET == 24);

/// Byte stride of one `CallArg` triple in the bridge's `argv` array.
const CALL_ARG_STRIDE: i64 = 24;

/// Emits `__rt_curl_invoke_callback(descriptor, curl_handle_object, spec) -> i64`.
///
/// Inputs (AArch64): x0 = descriptor, x1 = `CurlHandle` object, x2 = `CallSpec *`.
/// (x86_64): rdi, rsi, rdx in the same order. Result in x0 / rax.
pub fn emit_curl_invoke_callback(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_curl_invoke_callback_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: curl_invoke_callback ---");
    emitter.label_global("__rt_curl_invoke_callback");

    // Stack frame (304 bytes):
    //   [sp, #0]   = handler record (224 bytes): next@0, survivor@8, diag@16,
    //                jmp_buf@24 (matches TRY_HANDLER_* / __rt_throw_current).
    //   [sp, #224] = descriptor    [sp, #232] = CurlHandle object  [sp, #240] = spec
    //   [sp, #248] = args array    [sp, #256] = boxed args cell    [sp, #264] = boxed return
    //   [sp, #272] = loop index    [sp, #280] = integer result
    //   [sp, #288] = saved x29     [sp, #296] = saved x30
    emitter.instruction("sub sp, sp, #304"); // allocate the callback-adapter frame
    emitter.instruction("stp x29, x30, [sp, #288]"); // save frame pointer and return address
    emitter.instruction("add x29, sp, #288"); // establish the adapter frame pointer

    emitter.instruction("str x0, [sp, #224]"); // save the callable descriptor pointer
    emitter.instruction("str x1, [sp, #232]"); // save the CurlHandle object pointer
    emitter.instruction("str x2, [sp, #240]"); // save the bridge's CallSpec pointer
    emitter.instruction("str xzr, [sp, #256]"); // no argument container allocated yet
    emitter.instruction("str xzr, [sp, #264]"); // no boxed return held yet
    emitter.instruction("str xzr, [sp, #280]"); // default integer result is 0

    // -- a descriptor without a uniform invoker cannot be called: report "did nothing" --
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); // load the invoker slot
    emitter.instruction("cbz x9, __rt_curl_invoke_cb_bail"); // no invoker → nothing allocated to release

    // -- allocate an (argc + 1)-slot argument array: $ch plus the bridge's own args --
    emitter.instruction("ldr x10, [x2, #0]"); // spec.argc
    emitter.instruction("add x0, x10, #1"); // capacity = argc + 1 (slot 0 is $ch)
    emitter.instruction("mov x1, #8"); // boxed Mixed slots store one pointer each
    emitter.instruction("bl __rt_array_new"); // x0 = indexed array backing storage
    emitter.instruction("str x0, [sp, #248]"); // save the args array pointer

    // -- argument 0: the CurlHandle object, boxed as runtime tag 6 --
    emitter.instruction("ldr x1, [sp, #232]"); // payload lo = the object pointer
    emitter.instruction("mov x2, #0"); // objects use no high payload word
    emitter.instruction("mov x0, #6"); // runtime tag 6 = object (from_value increfs it)
    emitter.instruction("bl __rt_mixed_from_value"); // x0 = owned boxed Mixed for $ch
    emitter.instruction("str x0, [sp, #264]"); // stash the cell across the append
    emitter.instruction("mov x1, x0"); // append value = the boxed object cell
    emitter.instruction("ldr x0, [sp, #248]"); // append target = the args array
    emitter.instruction("bl __rt_array_push_refcounted"); // append (and retain) the cell
    emitter.instruction("str x0, [sp, #248]"); // save the (possibly grown) args array
    emitter.instruction("ldr x0, [sp, #264]"); // reload the boxed object cell
    emitter.instruction("bl __rt_decref_any"); // drop the temp retain (the array owns it)
    emitter.instruction("str xzr, [sp, #264]"); // the stash slot is free again

    // -- arguments 1..argc: one boxed Mixed per (tag, lo, hi) triple the bridge built --
    emitter.instruction("str xzr, [sp, #272]"); // loop index k = 0
    emitter.label("__rt_curl_invoke_cb_loop");
    emitter.instruction("ldr x9, [sp, #272]"); // reload the loop index
    emitter.instruction("ldr x10, [sp, #240]"); // reload the spec pointer
    emitter.instruction("ldr x10, [x10, #0]"); // spec.argc
    emitter.instruction("cmp x9, x10"); // has every bridge argument been boxed?
    emitter.instruction("b.ge __rt_curl_invoke_cb_loop_done"); // yes → box the container
    emitter.instruction("ldr x11, [sp, #240]"); // reload the spec pointer
    emitter.instruction("ldr x11, [x11, #8]"); // spec.argv base pointer
    emitter.instruction(&format!("mov x12, #{}", CALL_ARG_STRIDE)); // CallArg stride (tag@0, lo@8, hi@16)
    emitter.instruction("mul x13, x9, x12"); // byte offset of argv[k]
    emitter.instruction("add x11, x11, x13"); // x11 = &argv[k]
    emitter.instruction("ldr x0, [x11, #0]"); // runtime value tag
    emitter.instruction("ldr x1, [x11, #8]"); // payload lo (int value, or string pointer)
    emitter.instruction("ldr x2, [x11, #16]"); // payload hi (string byte length)
    emitter.instruction("bl __rt_mixed_from_value"); // x0 = owned boxed Mixed argument
    emitter.instruction("str x0, [sp, #264]"); // stash the cell across the append
    emitter.instruction("mov x1, x0"); // append value = the boxed argument
    emitter.instruction("ldr x0, [sp, #248]"); // append target = the args array
    emitter.instruction("bl __rt_array_push_refcounted"); // append (and retain) the cell
    emitter.instruction("str x0, [sp, #248]"); // save the (possibly grown) args array
    emitter.instruction("ldr x0, [sp, #264]"); // reload the boxed argument cell
    emitter.instruction("bl __rt_decref_any"); // drop the temp retain (the array owns it)
    emitter.instruction("str xzr, [sp, #264]"); // the stash slot is free again
    emitter.instruction("ldr x9, [sp, #272]"); // reload the loop index
    emitter.instruction("add x9, x9, #1"); // advance it
    emitter.instruction("str x9, [sp, #272]"); // persist the loop index
    emitter.instruction("b __rt_curl_invoke_cb_loop"); // box the next argument
    emitter.label("__rt_curl_invoke_cb_loop_done");

    // -- box the argument array itself as a Mixed indexed array (tag 4 increfs it) --
    emitter.instruction("ldr x1, [sp, #248]"); // Mixed payload lo = the args array
    emitter.instruction("mov x2, #0"); // heap payloads use no high word
    emitter.instruction("mov x0, #4"); // runtime tag 4 = indexed array
    emitter.instruction("bl __rt_mixed_from_value"); // x0 = boxed Mixed argument container
    emitter.instruction("str x0, [sp, #256]"); // save the container (released in cleanup)
    emitter.instruction("ldr x0, [sp, #248]"); // reload the raw args array
    emitter.instruction("bl __rt_decref_any"); // drop the extra retain (the box owns it)

    // -- push a setjmp firewall so a PHP throw cannot unwind through libcurl --
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction("str x10, [sp, #0]"); // handler record: previous handler-stack top
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
    emitter.instruction("str x10, [sp, #8]"); // handler record: frame cleanup stops here
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction("str x10, [sp, #16]"); // handler record: saved diagnostic depth
    emitter.instruction("mov x10, sp"); // x10 = address of this handler record
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // link it as active
    emitter.instruction("add x0, sp, #24"); // x0 = &jmp_buf inside the handler record
    emitter.bl_c("setjmp"); // 0 on the first pass, 1 when a throw longjmps back
    emitter.instruction("cbnz x0, __rt_curl_invoke_cb_threw"); // nonzero → arrived via longjmp

    // -- normal path: invoke the PHP callable through its uniform invoker (offset 56) --
    emitter.instruction("ldr x0, [sp, #224]"); // arg0 = the callable descriptor
    emitter.instruction("ldr x1, [sp, #256]"); // arg1 = the boxed argument container
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); // uniform invoker
    emitter.instruction("blr x9"); // call the PHP callable → OWNED boxed Mixed return
    emitter.instruction("str x0, [sp, #264]"); // save the boxed return for decode + release

    // -- pop the firewall before any further runtime call --
    emitter.instruction("ldr x10, [sp, #0]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // unlink the record
    emitter.instruction("ldr x10, [sp, #16]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0); // restore it

    // -- decode the return according to spec.result_kind --
    emitter.instruction("ldr x11, [sp, #240]"); // spec pointer
    emitter.instruction("ldr x11, [x11, #16]"); // spec.result_kind
    emitter.instruction("cbnz x11, __rt_curl_invoke_cb_string"); // 1 = string-into-buffer

    // result_kind 0: PHP's own `zval_get_long` of the return value.
    emitter.instruction("ldr x0, [sp, #264]"); // boxed return
    emitter.instruction("bl __rt_mixed_cast_int"); // x0 = the return cast to a PHP int
    emitter.instruction("str x0, [sp, #280]"); // publish it as this adapter's result
    emitter.instruction("b __rt_curl_invoke_cb_release_return");

    // result_kind 1: copy a STRING return into the bridge's buffer, capped at out_cap.
    emitter.label("__rt_curl_invoke_cb_string");
    emitter.instruction("ldr x0, [sp, #264]"); // boxed return
    emitter.instruction("bl __rt_mixed_unbox"); // x0 = tag, x1 = lo, x2 = hi
    emitter.instruction("cmp x0, #1"); // is the return a PHP string?
    emitter.instruction("b.ne __rt_curl_invoke_cb_release_return"); // no → out_len stays 0 (EOF)
    emitter.instruction("ldr x11, [sp, #240]"); // spec pointer
    emitter.instruction("ldr x12, [x11, #24]"); // spec.out_buf
    emitter.instruction("cbz x12, __rt_curl_invoke_cb_release_return"); // no buffer → nothing to copy
    emitter.instruction("ldr x13, [x11, #32]"); // spec.out_cap
    emitter.instruction("cmp x2, x13"); // is the string longer than libcurl's buffer?
    emitter.instruction("csel x2, x2, x13, lt"); // length = MIN(strlen, cap) — php-src truncates
    emitter.instruction("cmp x2, #0"); // is there anything left to copy?
    emitter.instruction("b.le __rt_curl_invoke_cb_release_return"); // no → leave out_len at 0
    emitter.instruction("str x2, [x11, #40]"); // spec.out_len = the copied byte count
    emitter.instruction("mov x0, x12"); // memcpy dst = spec.out_buf
    emitter.bl_c("memcpy"); // src (x1) and length (x2) are already in place
    emitter.instruction("b __rt_curl_invoke_cb_release_return");

    // -- release the invoker's owned boxed return --
    emitter.label("__rt_curl_invoke_cb_release_return");
    emitter.instruction("ldr x0, [sp, #264]"); // boxed return
    emitter.instruction("cbz x0, __rt_curl_invoke_cb_cleanup"); // nothing to release
    emitter.instruction("bl __rt_decref_mixed"); // release it (bytes already copied out)
    emitter.instruction("b __rt_curl_invoke_cb_cleanup");

    // -- longjmp path: the PHP callable threw --
    emitter.label("__rt_curl_invoke_cb_threw");
    emitter.instruction("ldr x10, [sp, #0]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // unlink the record
    emitter.instruction("ldr x10, [sp, #16]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0); // restore it
    // `_exc_value` is deliberately LEFT SET: the throwable stays pending so
    // `__rt_curl_rethrow_pending` can re-raise it once libcurl has unwound its own
    // frames. Swallowing it here (as the PDO adapters do) would lose the exception.
    emitter.instruction("ldr x11, [sp, #240]"); // spec pointer
    emitter.instruction("mov x10, #-1"); // status -1 = the callable threw
    emitter.instruction("str x10, [x11, #48]"); // spec.status = -1
    emitter.instruction("str xzr, [x11, #40]"); // spec.out_len = 0 (nothing was produced)

    // -- shared cleanup: release the argument container --
    emitter.label("__rt_curl_invoke_cb_cleanup");
    emitter.instruction("ldr x0, [sp, #256]"); // boxed argument container
    emitter.instruction("cbz x0, __rt_curl_invoke_cb_done"); // never allocated
    emitter.instruction("bl __rt_decref_any"); // release it (cascades to the boxed args)
    emitter.label("__rt_curl_invoke_cb_done");
    emitter.instruction("ldr x0, [sp, #280]"); // the integer result the bridge reads
    emitter.instruction("ldp x29, x30, [sp, #288]"); // restore frame pointer and return address
    emitter.instruction("add sp, sp, #304"); // release the adapter frame
    emitter.instruction("ret"); // return to the bridge trampoline

    // -- fast path: no uniform invoker → nothing allocated, nothing to release --
    emitter.label("__rt_curl_invoke_cb_bail");
    emitter.instruction("mov x0, #0"); // report "the callable did nothing"
    emitter.instruction("ldp x29, x30, [sp, #288]"); // restore frame pointer and return address
    emitter.instruction("add sp, sp, #304"); // release the adapter frame
    emitter.instruction("ret"); // return to the bridge trampoline
}

/// x86_64 implementation of `__rt_curl_invoke_callback`.
fn emit_curl_invoke_callback_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: curl_invoke_callback ---");
    emitter.label_global("__rt_curl_invoke_callback");

    // Frame (288 bytes below rbp):
    //   [rbp-8]  descriptor   [rbp-16] CurlHandle object  [rbp-24] spec
    //   [rbp-32] args array   [rbp-40] boxed args cell    [rbp-48] boxed return
    //   [rbp-56] loop index   [rbp-64] integer result
    //   [rbp-288] handler record (224 bytes): next@0, survivor@8, diag@16, jmp_buf@24
    //             → next=[rbp-288], survivor=[rbp-280], diag=[rbp-272], jmp_buf=[rbp-264].
    //   push rbp + sub rsp,288 keeps rsp 16-aligned for the nested calls.
    emitter.instruction("push rbp"); // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp"); // establish the adapter frame pointer
    emitter.instruction("sub rsp, 288"); // reserve the slots and the handler record

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi"); // save the callable descriptor pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi"); // save the CurlHandle object pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx"); // save the bridge's CallSpec pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], 0"); // no argument container allocated yet
    emitter.instruction("mov QWORD PTR [rbp - 48], 0"); // no boxed return held yet
    emitter.instruction("mov QWORD PTR [rbp - 64], 0"); // default integer result is 0

    // -- a descriptor without a uniform invoker cannot be called --
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rdi + {}]",
        CALLABLE_DESC_INVOKER_OFFSET
    )); // load the invoker slot
    emitter.instruction("test r10, r10"); // does the descriptor expose a uniform invoker?
    emitter.instruction("jz __rt_curl_invoke_cb_bail_x86"); // no → nothing allocated to release

    // -- allocate an (argc + 1)-slot argument array --
    emitter.instruction("mov rdi, QWORD PTR [rdx + 0]"); // spec.argc
    emitter.instruction("add rdi, 1"); // capacity = argc + 1 (slot 0 is $ch)
    emitter.instruction("mov esi, 8"); // boxed Mixed slots store one pointer each
    emitter.instruction("call __rt_array_new"); // rax = indexed array backing storage
    emitter.instruction("mov QWORD PTR [rbp - 32], rax"); // save the args array pointer

    // -- argument 0: the CurlHandle object, boxed as runtime tag 6 --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]"); // payload lo = the object pointer
    emitter.instruction("xor esi, esi"); // objects use no high payload word
    emitter.instruction("mov eax, 6"); // runtime tag 6 = object (from_value increfs it)
    emitter.instruction("call __rt_mixed_from_value"); // rax = owned boxed Mixed for $ch
    emitter.instruction("mov QWORD PTR [rbp - 48], rax"); // stash the cell across the append
    emitter.instruction("mov rsi, rax"); // append value = the boxed object cell
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]"); // append target = the args array
    emitter.instruction("call __rt_array_push_refcounted"); // append (and retain) the cell
    emitter.instruction("mov QWORD PTR [rbp - 32], rax"); // save the (possibly grown) array
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]"); // reload the boxed object cell
    emitter.instruction("call __rt_decref_any"); // drop the temp retain (the array owns it)
    emitter.instruction("mov QWORD PTR [rbp - 48], 0"); // the stash slot is free again

    // -- arguments 1..argc --
    emitter.instruction("mov QWORD PTR [rbp - 56], 0"); // loop index k = 0
    emitter.label("__rt_curl_invoke_cb_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]"); // reload the loop index
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]"); // reload the spec pointer
    emitter.instruction("mov r10, QWORD PTR [r10 + 0]"); // spec.argc
    emitter.instruction("cmp r9, r10"); // has every bridge argument been boxed?
    emitter.instruction("jge __rt_curl_invoke_cb_loop_done_x86"); // yes → box the container
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]"); // reload the spec pointer
    emitter.instruction("mov r11, QWORD PTR [r11 + 8]"); // spec.argv base pointer
    emitter.instruction(&format!("imul rax, r9, {}", CALL_ARG_STRIDE)); // byte offset of argv[k]
    emitter.instruction("add r11, rax"); // r11 = &argv[k]
    emitter.instruction("mov rdi, QWORD PTR [r11 + 8]"); // payload lo (int value, or string pointer)
    emitter.instruction("mov rsi, QWORD PTR [r11 + 16]"); // payload hi (string byte length)
    emitter.instruction("mov rax, QWORD PTR [r11 + 0]"); // runtime value tag
    emitter.instruction("call __rt_mixed_from_value"); // rax = owned boxed Mixed argument
    emitter.instruction("mov QWORD PTR [rbp - 48], rax"); // stash the cell across the append
    emitter.instruction("mov rsi, rax"); // append value = the boxed argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]"); // append target = the args array
    emitter.instruction("call __rt_array_push_refcounted"); // append (and retain) the cell
    emitter.instruction("mov QWORD PTR [rbp - 32], rax"); // save the (possibly grown) array
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]"); // reload the boxed argument cell
    emitter.instruction("call __rt_decref_any"); // drop the temp retain (the array owns it)
    emitter.instruction("mov QWORD PTR [rbp - 48], 0"); // the stash slot is free again
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]"); // reload the loop index
    emitter.instruction("add r9, 1"); // advance it
    emitter.instruction("mov QWORD PTR [rbp - 56], r9"); // persist the loop index
    emitter.instruction("jmp __rt_curl_invoke_cb_loop_x86"); // box the next argument
    emitter.label("__rt_curl_invoke_cb_loop_done_x86");

    // -- box the argument array itself as a Mixed indexed array (tag 4 increfs it) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]"); // Mixed payload lo = the args array
    emitter.instruction("xor esi, esi"); // heap payloads use no high word
    emitter.instruction("mov eax, 4"); // runtime tag 4 = indexed array
    emitter.instruction("call __rt_mixed_from_value"); // rax = boxed Mixed argument container
    emitter.instruction("mov QWORD PTR [rbp - 40], rax"); // save the container (released in cleanup)
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]"); // reload the raw args array
    emitter.instruction("call __rt_decref_any"); // drop the extra retain (the box owns it)

    // -- push a setjmp firewall so a PHP throw cannot unwind through libcurl --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction("mov QWORD PTR [rbp - 288], r10"); // handler record: record.next
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
    emitter.instruction("mov QWORD PTR [rbp - 280], r10"); // handler record: survivor frame
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction("mov QWORD PTR [rbp - 272], r10"); // handler record: saved diag depth
    emitter.instruction("lea r10, [rbp - 288]"); // r10 = address of this handler record
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // link it as active
    emitter.instruction("lea rdi, [rbp - 264]"); // rdi = &jmp_buf inside the handler record
    emitter.bl_c("setjmp"); // 0 on the first pass, 1 when a throw longjmps back
    emitter.instruction("test rax, rax"); // did control arrive via longjmp?
    emitter.instruction("jne __rt_curl_invoke_cb_threw_x86"); // nonzero → the callable threw

    // -- normal path: invoke the PHP callable through its uniform invoker (offset 56) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]"); // arg0 = the callable descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]"); // arg1 = the boxed argument container
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rdi + {}]",
        CALLABLE_DESC_INVOKER_OFFSET
    )); // uniform invoker
    emitter.instruction("call r10"); // call the PHP callable → OWNED boxed Mixed return
    emitter.instruction("mov QWORD PTR [rbp - 48], rax"); // save the boxed return

    // -- pop the firewall before any further runtime call --
    emitter.instruction("mov r10, QWORD PTR [rbp - 288]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // unlink the record
    emitter.instruction("mov r10, QWORD PTR [rbp - 272]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0); // restore it

    // -- decode the return according to spec.result_kind --
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]"); // spec pointer
    emitter.instruction("mov r11, QWORD PTR [r11 + 16]"); // spec.result_kind
    emitter.instruction("test r11, r11"); // 0 = integer, 1 = string-into-buffer
    emitter.instruction("jnz __rt_curl_invoke_cb_string_x86");

    // result_kind 0: PHP's own `zval_get_long` of the return value.
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]"); // boxed return (cast reads rax)
    emitter.instruction("call __rt_mixed_cast_int"); // rax = the return cast to a PHP int
    emitter.instruction("mov QWORD PTR [rbp - 64], rax"); // publish it as this adapter's result
    emitter.instruction("jmp __rt_curl_invoke_cb_release_return_x86");

    // result_kind 1: copy a STRING return into the bridge's buffer, capped at out_cap.
    emitter.label("__rt_curl_invoke_cb_string_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]"); // boxed return (unbox reads rax)
    emitter.instruction("call __rt_mixed_unbox"); // rax = tag, rdi = lo, rdx = hi
    emitter.instruction("cmp rax, 1"); // is the return a PHP string?
    emitter.instruction("jne __rt_curl_invoke_cb_release_return_x86"); // no → out_len stays 0
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]"); // spec pointer
    emitter.instruction("mov r8, QWORD PTR [r11 + 24]"); // spec.out_buf
    emitter.instruction("test r8, r8"); // is there a destination buffer?
    emitter.instruction("jz __rt_curl_invoke_cb_release_return_x86"); // no → nothing to copy
    emitter.instruction("mov r9, QWORD PTR [r11 + 32]"); // spec.out_cap
    emitter.instruction("cmp rdx, r9"); // is the string longer than libcurl's buffer?
    emitter.instruction("cmovg rdx, r9"); // length = MIN(strlen, cap) — php-src truncates
    emitter.instruction("cmp rdx, 0"); // is there anything left to copy?
    emitter.instruction("jle __rt_curl_invoke_cb_release_return_x86"); // no → leave out_len at 0
    emitter.instruction("mov QWORD PTR [r11 + 40], rdx"); // spec.out_len = the copied byte count
    emitter.instruction("mov rsi, rdi"); // memcpy src = the string bytes
    emitter.instruction("mov rdi, r8"); // memcpy dst = spec.out_buf
    emitter.bl_c("memcpy"); // length is already in rdx
    emitter.instruction("jmp __rt_curl_invoke_cb_release_return_x86");

    // -- release the invoker's owned boxed return --
    emitter.label("__rt_curl_invoke_cb_release_return_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]"); // boxed return
    emitter.instruction("test rax, rax"); // nothing to release?
    emitter.instruction("jz __rt_curl_invoke_cb_cleanup_x86");
    emitter.instruction("call __rt_decref_mixed"); // release it (bytes already copied out)
    emitter.instruction("jmp __rt_curl_invoke_cb_cleanup_x86");

    // -- longjmp path: the PHP callable threw --
    emitter.label("__rt_curl_invoke_cb_threw_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 288]"); // record.next
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // unlink the record
    emitter.instruction("mov r10, QWORD PTR [rbp - 272]"); // saved diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0); // restore it
    // `_exc_value` stays SET on purpose — see the AArch64 body's comment.
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]"); // spec pointer
    emitter.instruction("mov QWORD PTR [r11 + 48], -1"); // spec.status = -1 (the callable threw)
    emitter.instruction("mov QWORD PTR [r11 + 40], 0"); // spec.out_len = 0

    // -- shared cleanup: release the argument container --
    emitter.label("__rt_curl_invoke_cb_cleanup_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]"); // boxed argument container
    emitter.instruction("test rax, rax"); // never allocated?
    emitter.instruction("jz __rt_curl_invoke_cb_done_x86");
    emitter.instruction("call __rt_decref_any"); // release it (cascades to the boxed args)
    emitter.label("__rt_curl_invoke_cb_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]"); // the integer result the bridge reads
    emitter.instruction("add rsp, 288"); // release the adapter frame
    emitter.instruction("pop rbp"); // restore the caller frame pointer
    emitter.instruction("ret"); // return to the bridge trampoline

    // -- fast path: no uniform invoker → nothing allocated, nothing to release --
    emitter.label("__rt_curl_invoke_cb_bail_x86");
    emitter.instruction("xor eax, eax"); // report "the callable did nothing"
    emitter.instruction("add rsp, 288"); // release the adapter frame
    emitter.instruction("pop rbp"); // restore the caller frame pointer
    emitter.instruction("ret"); // return to the bridge trampoline
}

/// Emits `__rt_curl_rethrow_pending()`: re-raises a throwable a PHP callback left pending.
///
/// `__rt_curl_invoke_callback`'s firewall stops a `longjmp` at the libcurl boundary but
/// keeps `_exc_value` set. The transfer then aborts through libcurl's own error path and
/// unwinds its frames normally; this helper, called by `__rt_curl_easy_perform` and
/// `__rt_curl_multi_exec` right after the bridge returns, is where the exception resumes
/// its journey to the nearest PHP `catch`. Without it the throwable would sit in
/// `_exc_value` and surface at some unrelated later throw site.
///
/// Preserves every register it does not name: it must be callable with the bridge's
/// return value already live in the result register. Does not return when a throwable is
/// pending (`__rt_throw_current` either `longjmp`s to a `catch` or reports and exits).
pub fn emit_curl_rethrow_pending(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: curl_rethrow_pending ---");
    emitter.label_global("__rt_curl_rethrow_pending");

    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", "_exc_value", 0); // pending throwable, if any
            emitter.instruction("cbz x9, __rt_curl_rethrow_pending_none"); // nothing pending → return
            emitter.instruction("b __rt_throw_current"); // resume the throw (never returns)
            emitter.label("__rt_curl_rethrow_pending_none");
            emitter.instruction("ret"); // no exception: leave the caller's result untouched
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_value", 0); // pending throwable, if any
            emitter.instruction("test r10, r10"); // is a throwable waiting?
            emitter.instruction("jz __rt_curl_rethrow_pending_none_x86"); // no → return
            emitter.instruction("jmp __rt_throw_current"); // resume the throw (never returns)
            emitter.label("__rt_curl_rethrow_pending_none_x86");
            emitter.instruction("ret"); // no exception: leave the caller's result untouched
        }
    }
}
