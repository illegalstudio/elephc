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
//!   verbatim: stream_select only sets bits for fds in `0..63`, so a boxed/large
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

    emitter.instruction("sub sp, sp, #16");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- passthrough for ordinary (non-wrapper) descriptors --
    emitter.instruction(&format!("tst x0, #{:#x}", FD_WRAPPER_BIT));            // is the synthetic user-wrapper fd bit set?
    emitter.instruction("b.eq __rt_uwcast_passthrough");                        // real OS fd → return it unchanged

    // $cast_as stays in x1 across both lookups (neither touches it).
    emit_handle_lookup(emitter, "__rt_uwcast_neg1");                            // resolve obj into x0; missing handle → -1
    emit_method_lookup(emitter, "__rt_uwcast_neg1");                            // resolve stream_cast method into x11; missing → -1

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

    emitter.label("__rt_uwcast_neg1");
    emitter.instruction("mov x0, #-1");                                         // no handle / no stream_cast method → not selectable

    emitter.label("__rt_uwcast_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
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

    // -- passthrough for ordinary (non-wrapper) descriptors --
    emitter.instruction(&format!("test rdi, {:#x}", FD_WRAPPER_BIT));           // is the synthetic user-wrapper fd bit set?
    emitter.instruction("jz __rt_uwcast_passthrough_x86");                      // real OS fd → return it unchanged

    // $cast_as stays in rsi across both lookups (neither touches it).
    emitter.instruction(&format!("sub rdi, {:#x}", FD_WRAPPER_BIT));            // rdi = fd - 0x40000000 = handle slot index
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_handles");          // handle table base
    emitter.instruction("mov rdi, QWORD PTR [r10 + rdi * 8]");                  // obj = _user_wrapper_handles[slot]
    emitter.instruction("test rdi, rdi");                                       // is the slot empty?
    emitter.instruction("jz __rt_uwcast_neg1_x86");                             // fclose'd or never registered: not selectable
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_vtable_ptrs");      // base of the per-class user-wrapper vtable pointer table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", VTABLE_SLOT_CAST * 8)); // load the stream_cast method pointer (slot 10)
    emitter.instruction("test r11, r11");                                       // is stream_cast missing?
    emitter.instruction("jz __rt_uwcast_neg1_x86");                             // method absent: not selectable

    // -- call stream_cast($this, $cast_as) → resource fd (or boxed/false) in rax --
    emitter.emit_platform_callback_call("r11", 2);

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

    emitter.label("__rt_uwcast_neg1_x86");
    emitter.instruction("mov rax, -1");                                         // no handle / no stream_cast method → not selectable

    emitter.label("__rt_uwcast_ret_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the resolved descriptor
}
