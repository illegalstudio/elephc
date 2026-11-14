//! Purpose:
//! Emits `__rt_user_wrapper_stream_cast`, the runtime helper that resolves a
//! synthetic user-wrapper file descriptor to a real, select()-able OS file
//! descriptor by invoking the wrapper object's `stream_cast()` method (vtable
//! slot 10). `__rt_stream_select` calls it for every descriptor in its sets so a
//! userspace stream that wraps a real fd (e.g. a socket) becomes selectable.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - `__rt_stream_select` (src/codegen/runtime/io/stream_select.rs) at both the
//!   fd_set build and the post-select compaction sites.
//!
//! Key details:
//! - Passthrough contract: a descriptor WITHOUT the synthetic wrapper bit
//!   (`0x40000000`) is returned unchanged, so stream_select can call this
//!   unconditionally and real OS fds flow through untouched.
//! - A synthetic fd with no live handle, or a wrapper class without a
//!   `stream_cast` method, returns `-1` (stream_select skips negative fds).
//! - The wrapper's `stream_cast(int $cast_as): resource|false` is expected to
//!   return the underlying int fd (declare `: int`). The raw result is returned
//!   verbatim: stream_select only sets bits for fds in `0..1023`, so a boxed/large
//!   or out-of-range return is harmlessly skipped there rather than dereferenced.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// vtable slot index of `stream_cast` (see `USER_WRAPPER_METHOD_NAMES`).
const VTABLE_SLOT_CAST: usize = 10;
/// The synthetic user-wrapper fd marker bit (`0x40000000 | slot`).
const FD_WRAPPER_BIT: u32 = 0x4000_0000;

/// Emits `__rt_user_wrapper_stream_cast(fd, cast_as) -> real_fd | fd | -1`.
///
/// Inputs: AArch64 x0 = descriptor, x1 = `$cast_as`; x86_64 rdi/rsi. Output in
/// x0/rax: the descriptor unchanged when it is not a synthetic wrapper fd, the
/// wrapper's `stream_cast` result when it is, or `-1` when the handle/method is
/// absent.
pub fn emit_user_wrapper_stream_cast(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_stream_cast_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_stream_cast ---");
    emitter.label_global("__rt_user_wrapper_stream_cast");

    emitter.instruction("sub sp, sp, #32");                                     // helper frame for the wrapper dispatch and record flag
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the cast-error record flag across stream_cast

    // -- passthrough for ordinary (non-wrapper) descriptors --
    emitter.instruction(&format!("tst x0, #{:#x}", FD_WRAPPER_BIT));            // is the synthetic user-wrapper fd bit set?
    emitter.instruction("b.eq __rt_uwcast_passthrough");                        // real OS fd → return it unchanged

    // $cast_as stays in x1 across both lookups (neither touches it).
    emit_handle_lookup(emitter, "__rt_uwcast_missing");                         // resolve obj into x0; missing handle → -1
    emit_method_lookup(emitter, "__rt_uwcast_notimpl");                         // resolve stream_cast method into x11; missing → report

    // -- call stream_cast($this, $cast_as) → resource fd (or boxed/false) in x0 --
    emitter.instruction("blr x11");                                             // invoke stream_cast on the wrapper object

    // -- normalize the return to a raw int fd --
    // A `: int` return arrives as a raw fd; an untyped/`resource` return arrives
    // as a boxed Mixed cell (heap pointer) whose payload word holds the fd.
    emitter.instruction("cbz x0, __rt_uwcast_neg1");                            // null/false-ish → not selectable
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the return below the managed heap?
    emitter.instruction("b.lo __rt_uwcast_ret");                                // raw small int → it is already the fd
    abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // current heap byte length
    emitter.instruction("add x10, x9, x10");                                    // managed heap end address
    emitter.instruction("cmp x0, x10");                                         // is the return at or beyond the heap end?
    emitter.instruction("b.hs __rt_uwcast_ret");                                // raw int above the heap → already the fd
    emitter.instruction("ldr x9, [x0]");                                        // boxed Mixed runtime tag
    emitter.instruction("cmp x9, #0");                                          // tag 0 = int?
    emitter.instruction("b.eq __rt_uwcast_unbox");                              // unbox the integer payload as the fd
    emitter.instruction("cmp x9, #9");                                          // tag 9 = resource?
    emitter.instruction("b.eq __rt_uwcast_unbox");                              // unbox the resource payload as the fd
    emitter.instruction("mov x0, #-1");                                         // any other boxed kind (bool false/null/string) → not selectable
    emitter.instruction("b __rt_uwcast_ret");                                   // return the not-selectable sentinel
    emitter.label("__rt_uwcast_unbox");
    emitter.instruction("ldr x0, [x0, #8]");                                    // payload word of the Mixed cell is the underlying fd
    emitter.instruction("b __rt_uwcast_ret");                                   // return the unboxed fd

    emitter.label("__rt_uwcast_passthrough");
    // x0 already holds the original descriptor; fall through to the return.
    emitter.instruction("b __rt_uwcast_ret");                                   // return the original descriptor unchanged

    emitter.label("__rt_uwcast_notimpl");
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the cast-error record flag
    emitter.instruction("bl __rt_user_wrapper_cast_error");                     // record the missing stream_cast for the PHP dispatcher
    emitter.instruction("mov x0, #-1");                                         // the descriptor is not selectable

    emitter.label("__rt_uwcast_neg1");
    emitter.instruction("mov x0, #-1");                                         // null/false return → not selectable

    emitter.label("__rt_uwcast_missing");
    emitter.instruction("mov x0, #-1");                                         // no live wrapper handle → not selectable

    emitter.label("__rt_uwcast_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the resolved descriptor
}

/// AArch64: resolve the wrapper object for the synthetic fd in `x0` into `x0`.
/// Branches to `missing_label` when the handle slot is empty.
fn emit_handle_lookup(emitter: &mut Emitter, missing_label: &str) {
    // slot = fd - 0x40000000 (low 6 bits select the handle); reuse the same
    // shift-and-subtract sequence as the other wrapper dispatch helpers.
    emitter.instruction("mov w9, #0x4000");                                     // high half of the synthetic fd base (0x4000 << 16 = 0x40000000)
    emitter.instruction("lsl x9, x9, #16");                                     // form 0x40000000 in x9
    emitter.instruction("sub x9, x0, x9");                                      // x9 = fd - 0x40000000 = handle slot index
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_handles");
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // obj = _user_wrapper_handles[slot]
    emitter.instruction(&format!("cbz x0, {}", missing_label));                 // slot empty (fclose'd or never registered): not selectable
}

/// AArch64: resolve the stream_cast method pointer for the object in `x0` into
/// `x11`. Branches to `missing_label` when the wrapper class omits the method.
fn emit_method_lookup(emitter: &mut Emitter, missing_label: &str) {
    emitter.instruction("ldr x10, [x0]");                                       // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x11", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("ldr x11, [x11, #{}]", VTABLE_SLOT_CAST * 8)); // load the stream_cast method pointer (slot 10)
    emitter.instruction(&format!("cbz x11, {}", missing_label));                // method absent: not selectable
}

/// x86_64 implementation of `__rt_user_wrapper_stream_cast`.
fn emit_user_wrapper_stream_cast_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_stream_cast ---");
    emitter.label_global("__rt_user_wrapper_stream_cast");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve the cast-error record flag slot
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // save the cast-error record flag across stream_cast

    // -- passthrough for ordinary (non-wrapper) descriptors --
    emitter.instruction(&format!("test rdi, {:#x}", FD_WRAPPER_BIT));           // is the synthetic user-wrapper fd bit set?
    emitter.instruction("jz __rt_uwcast_passthrough_x86");                      // real OS fd → return it unchanged

    // $cast_as stays in rsi across both lookups (neither touches it).
    emitter.instruction(&format!("sub rdi, {:#x}", FD_WRAPPER_BIT));            // rdi = fd - 0x40000000 = handle slot index
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_handles");          // handle table base
    emitter.instruction("mov rdi, QWORD PTR [r10 + rdi * 8]");                  // obj = _user_wrapper_handles[slot]
    emitter.instruction("test rdi, rdi");                                       // is the slot empty?
    emitter.instruction("jz __rt_uwcast_missing_x86");                          // fclose'd or never registered: not selectable
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_vtable_ptrs");      // base of the per-class user-wrapper vtable pointer table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", VTABLE_SLOT_CAST * 8)); // load the stream_cast method pointer (slot 10)
    emitter.instruction("test r11, r11");                                       // is stream_cast missing?
    emitter.instruction("jz __rt_uwcast_notimpl_x86");                          // method absent: report the cast failure

    // -- call stream_cast($this, $cast_as) → resource fd (or boxed/false) in rax --
    emitter.instruction("call r11");                                            // invoke stream_cast on the wrapper object

    // -- normalize the return to a raw int fd (see the AArch64 path for the rationale) --
    emitter.instruction("test rax, rax");                                       // null/false-ish return?
    emitter.instruction("jz __rt_uwcast_neg1_x86");                             // → not selectable
    abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("cmp rax, r10");                                        // is the return below the managed heap?
    emitter.instruction("jb __rt_uwcast_ret_x86");                              // raw small int → already the fd
    abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // current heap byte length
    emitter.instruction("add r11, r10");                                        // managed heap end address
    emitter.instruction("cmp rax, r11");                                        // is the return at or beyond the heap end?
    emitter.instruction("jae __rt_uwcast_ret_x86");                             // raw int above the heap → already the fd
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // boxed Mixed runtime tag
    emitter.instruction("cmp r10, 0");                                          // tag 0 = int?
    emitter.instruction("je __rt_uwcast_unbox_x86");                            // unbox the integer payload as the fd
    emitter.instruction("cmp r10, 9");                                          // tag 9 = resource?
    emitter.instruction("je __rt_uwcast_unbox_x86");                            // unbox the resource payload as the fd
    emitter.instruction("mov rax, -1");                                         // other boxed kind → not selectable
    emitter.instruction("jmp __rt_uwcast_ret_x86");                             // return the not-selectable sentinel
    emitter.label("__rt_uwcast_unbox_x86");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // payload word of the Mixed cell is the underlying fd
    emitter.instruction("jmp __rt_uwcast_ret_x86");                             // return the unboxed fd

    emitter.label("__rt_uwcast_passthrough_x86");
    emitter.instruction("mov rax, rdi");                                        // return the original descriptor unchanged
    emitter.instruction("jmp __rt_uwcast_ret_x86");                             // fall through to the return

    emitter.label("__rt_uwcast_notimpl_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the cast-error record flag
    emitter.instruction("call __rt_user_wrapper_cast_error");                   // record the missing stream_cast for the PHP dispatcher
    emitter.instruction("mov rax, -1");                                         // the descriptor is not selectable

    emitter.label("__rt_uwcast_neg1_x86");
    emitter.instruction("mov rax, -1");                                         // null/false return → not selectable

    emitter.label("__rt_uwcast_missing_x86");
    emitter.instruction("mov rax, -1");                                         // no live wrapper handle → not selectable

    emitter.label("__rt_uwcast_ret_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard the record-flag slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the resolved descriptor
}

/// Emits `__rt_user_wrapper_cast_error`, which records one missing `stream_cast()`
/// failure (wrapper class and event count) for the PHP `stream_select()` dispatcher.
///
/// Inputs: AArch64 x0 = wrapper object (or 0); x86_64 rdi = wrapper object (or 0).
pub(crate) fn emit_user_wrapper_cast_error(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_cast_error_x86_64(emitter);
        return;
    }

    let count = crate::names::ir_global_symbol("__elephc_stream_cast_count");
    let class = crate::names::ir_global_symbol("__elephc_stream_cast_class");

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_cast_error ---");
    emitter.label_global("__rt_user_wrapper_cast_error");
    emitter.instruction("cbz x1, __rt_uwce_skip");                              // compaction probes do not record a second event
    emitter.instruction("sub sp, sp, #80");                                     // reserve the cast-event tracking frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the wrapper object for its class name
    abi::emit_symbol_address(emitter, "x10", &count);
    emitter.instruction("ldr x0, [x10]");                                       // previous boxed cast-event count
    emitter.instruction("cbz x0, __rt_uwce_count_one");                         // a missing count starts at one
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    emitter.instruction("cmp x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("b.ne __rt_uwce_count_one");                            // a non-int count restarts at one
    emitter.instruction("add x9, x1, #1");                                      // increment the previous event count
    emitter.instruction("b __rt_uwce_count_ready");                             // continue with the new count
    emitter.label("__rt_uwce_count_one");
    emitter.instruction("mov x9, #1");                                          // first cast event in this operation
    emitter.label("__rt_uwce_count_ready");
    emitter.instruction("str x9, [sp, #24]");                                   // preserve the new count across boxing
    abi::emit_symbol_address(emitter, "x10", &count);
    emitter.instruction("ldr x0, [x10]");                                       // reload the previous box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #24]");                                   // new event count payload
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "x10", &count);
    emitter.instruction("str x0, [x10]");                                       // publish the boxed event count
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the wrapper object
    emitter.instruction("cbz x11, __rt_uwce_anon");                             // a missing object has no class name
    emitter.instruction("ldr x11, [x11]");                                      // object header word 0 = runtime class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");
    emitter.instruction("ldr x10, [x10]");                                      // number of named class ids
    emitter.instruction("cmp x11, x10");                                        // is the class id within the table?
    emitter.instruction("b.hs __rt_uwce_anon");                                 // unknown ids contribute no name
    abi::emit_symbol_address(emitter, "x12", "_class_name_entries");
    emitter.instruction("add x12, x12, x11, lsl #4");                           // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("ldr x1, [x12]");                                       // class-name pointer
    emitter.instruction("ldr x2, [x12, #8]");                                   // class-name length
    emitter.instruction("b __rt_uwce_named");                                   // emit the resolved name
    emitter.label("__rt_uwce_anon");
    abi::emit_symbol_address(emitter, "x1", "_class_name_missing");             // empty fallback name
    emitter.instruction("mov x2, #0");                                          // zero-length class name
    emitter.label("__rt_uwce_named");
    emitter.instruction("str x1, [sp, #32]");                                   // preserve the class-name pointer
    emitter.instruction("str x2, [sp, #40]");                                   // preserve the class-name length
    abi::emit_symbol_address(emitter, "x10", &class);
    emitter.instruction("ldr x0, [x10]");                                       // previous boxed class name
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("ldr x1, [sp, #32]");                                   // class-name pointer payload
    emitter.instruction("ldr x2, [sp, #40]");                                   // class-name byte length
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "x10", &class);
    emitter.instruction("str x0, [x10]");                                       // publish the boxed class name
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.label("__rt_uwce_skip");
    emitter.instruction("ret");                                                 // return to the cast helper
}

/// Emits the x86_64 `__rt_user_wrapper_cast_error` variant.
fn emit_user_wrapper_cast_error_x86_64(emitter: &mut Emitter) {
    let count = crate::names::ir_global_symbol("__elephc_stream_cast_count");
    let class = crate::names::ir_global_symbol("__elephc_stream_cast_class");

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_cast_error ---");
    emitter.label_global("__rt_user_wrapper_cast_error");
    emitter.instruction("test rsi, rsi");                                       // compaction probes do not record a second event
    emitter.instruction("jz __rt_uwce_skip_x86");                               // skip recording for the keep pass
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve the cast-event tracking frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the wrapper object for its class name
    abi::emit_symbol_address(emitter, "r10", &count);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // previous boxed cast-event count
    emitter.instruction("test rax, rax");                                       // is there a previous count?
    emitter.instruction("jz __rt_uwce_count_one_x86");                          // a missing count starts at one
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    emitter.instruction("test rax, rax");                                       // runtime tag 0 = int
    emitter.instruction("jnz __rt_uwce_count_one_x86");                         // a non-int count restarts at one
    emitter.instruction("lea r9, [rdi + 1]");                                   // increment the previous event count
    emitter.instruction("jmp __rt_uwce_count_ready_x86");                       // continue with the new count
    emitter.label("__rt_uwce_count_one_x86");
    emitter.instruction("mov r9, 1");                                           // first cast event in this operation
    emitter.label("__rt_uwce_count_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // preserve the new count across boxing
    abi::emit_symbol_address(emitter, "r10", &count);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // reload the previous box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // new event count payload
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a second word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "r10", &count);
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the boxed event count
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the wrapper object
    emitter.instruction("test r11, r11");                                       // a missing object has no class name
    emitter.instruction("jz __rt_uwce_anon_x86");                               // fall back to an empty class name
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // object header word 0 = runtime class id
    abi::emit_symbol_address(emitter, "r10", "_class_name_count");
    emitter.instruction("cmp r11, QWORD PTR [r10]");                            // is the class id within the table?
    emitter.instruction("jae __rt_uwce_anon_x86");                              // unknown ids contribute no name
    abi::emit_symbol_address(emitter, "r10", "_class_name_entries");
    emitter.instruction("shl r11, 4");                                          // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("add r10, r11");                                        // advance to this class's entry
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // class-name length
    emitter.instruction("jmp __rt_uwce_named_x86");                             // emit the resolved name
    emitter.label("__rt_uwce_anon_x86");
    abi::emit_symbol_address(emitter, "rdi", "_class_name_missing");            // empty fallback name
    emitter.instruction("xor esi, esi");                                        // zero-length class name
    emitter.label("__rt_uwce_named_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // preserve the class-name length
    abi::emit_symbol_address(emitter, "r10", &class);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // previous boxed class name
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("mov rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // class-name pointer payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // class-name byte length
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "r10", &class);
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the boxed class name
    emitter.instruction("mov rsp, rbp");                                        // discard the helper slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.label("__rt_uwce_skip_x86");
    emitter.instruction("ret");                                                 // return to the cast helper
}
