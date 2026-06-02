//! Purpose:
//! Emits `__rt_touch_meta_array`, which builds the PHP `[mtime, atime]` value
//! array that `touch()` hands to a userspace stream wrapper's
//! `stream_metadata($path, STREAM_META_TOUCH, $value)`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
//! - The `touch()` builtin emitter, on the registered-wrapper branch, to
//!   produce the boxed `mixed` value before calling `__rt_user_wrapper_path_op`.
//!
//! Key details:
//! - PHP always passes `touch()` a 2-element indexed int array; an omitted /
//!   "now" timestamp resolves to `time(NULL)` here via `__rt_time`. The flags
//!   byte mirrors `touch.rs`: bit0 = atime-now, bit1 = mtime-now.
//! - Refcount lifecycle: `__rt_array_new` returns the array at refcount 1;
//!   boxing it as `Mixed(tag 4)` increfs it to 2, so this helper decrefs the
//!   array once (back to 1) before returning, leaving the boxed `Mixed` as the
//!   sole owner. The caller releases the returned cell with `__rt_decref_mixed`,
//!   which deep-frees the array and its boxed int elements.

use crate::codegen::expr::arrays::emit_array_value_type_stamp;
use crate::codegen::{abi, emit::Emitter, platform::Arch};
use crate::types::PhpType;

/// Emits `__rt_touch_meta_array(mtime, atime, flags) -> Mixed(array)`.
///
/// Inputs (AArch64): x0 = mtime seconds, x1 = atime seconds, x2 = flags
/// (bit0 = atime-now, bit1 = mtime-now). (x86_64): rdi = mtime, rsi = atime,
/// rdx = flags. Output: x0 / rax = an owned `Mixed` cell wrapping the indexed
/// array `[mtime, atime]`. The caller owns it and must `__rt_decref_mixed` it.
pub fn emit_touch_meta_array(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_touch_meta_array_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: touch_meta_array ---");
    emitter.label_global("__rt_touch_meta_array");

    // Frame: 48 bytes. [sp,#0] mtime (later boxed-mixed ptr), [sp,#8] atime,
    //   [sp,#16] flags, [sp,#24] array ptr, [sp,#32..48] saved x29/x30.
    emitter.instruction("sub sp, sp, #48");                                     // helper frame for the touch metadata array
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the mtime seconds
    emitter.instruction("str x1, [sp, #8]");                                    // save the atime seconds
    emitter.instruction("str x2, [sp, #16]");                                   // save the current-time flags

    // -- resolve any "now" timestamp to the current Unix time --
    emitter.instruction("ands xzr, x2, #3");                                    // any current-time flag bit set?
    emitter.instruction("b.eq __rt_tma_built");                                 // no now-flags: both timestamps are explicit
    abi::emit_call_label(emitter, "__rt_time");                                 // x0 = current Unix timestamp
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the flags after the time call
    emitter.instruction("tbz x9, #1, __rt_tma_now_atime");                      // mtime-now (bit1) requested?
    emitter.instruction("str x0, [sp, #0]");                                    // mtime = current time
    emitter.label("__rt_tma_now_atime");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the flags for the atime check
    emitter.instruction("tbz x9, #0, __rt_tma_built");                          // atime-now (bit0) requested?
    emitter.instruction("str x0, [sp, #8]");                                    // atime = current time
    emitter.label("__rt_tma_built");

    // -- allocate the 2-element indexed array and stamp its value_type --
    emitter.instruction("mov x0, #2");                                          // capacity: two elements (mtime, atime)
    emitter.instruction("mov x1, #8");                                          // boxed Mixed slots store one pointer each
    abi::emit_call_label(emitter, "__rt_array_new");                            // x0 = indexed array backing storage
    emit_array_value_type_stamp(emitter, "x0", &PhpType::Mixed);
    emitter.instruction("str x0, [sp, #24]");                                   // save the array pointer while filling slots

    // -- element 0: box mtime as Mixed(int) and store at array[0] --
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #0]");                                    // value_lo = mtime seconds
    emitter.instruction("mov x2, #0");                                          // value_hi = 0 for an integer scalar
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = boxed Mixed(int) mtime
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the array pointer
    emitter.instruction("str x0, [x9, #24]");                                   // array[0] = boxed mtime

    // -- element 1: box atime as Mixed(int) and store at array[1] --
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #8]");                                    // value_lo = atime seconds
    emitter.instruction("mov x2, #0");                                          // value_hi = 0 for an integer scalar
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = boxed Mixed(int) atime
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the array pointer
    emitter.instruction("str x0, [x9, #32]");                                   // array[1] = boxed atime
    emitter.instruction("mov x10, #2");                                         // logical length after both inserts
    emitter.instruction("str x10, [x9]");                                       // publish the indexed-array length

    // -- box the array as Mixed(tag 4); this increfs the array to refcount 2 --
    emitter.instruction("mov x0, #4");                                          // runtime tag 4 = indexed array
    emitter.instruction("ldr x1, [sp, #24]");                                   // value_lo = array pointer
    emitter.instruction("mov x2, #0");                                          // value_hi unused for arrays
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = boxed Mixed(array)
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed Mixed pointer (mtime slot reused)

    // -- drop our own array reference so the Mixed cell is the sole owner --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the array pointer
    abi::emit_call_label(emitter, "__rt_decref_any");                           // array refcount 2 -> 1 (owned by the Mixed)

    emitter.instruction("ldr x0, [sp, #0]");                                    // return the boxed Mixed(array) pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed metadata array
}

/// x86_64 implementation of `__rt_touch_meta_array`.
fn emit_touch_meta_array_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: touch_meta_array ---");
    emitter.label_global("__rt_touch_meta_array");

    // Frame: [rbp-8] mtime (later boxed-mixed ptr), [rbp-16] atime,
    //   [rbp-24] flags, [rbp-32] array ptr. push rbp + sub 48 stays 16-aligned.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // spill slots for timestamps, flags, and pointers
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the mtime seconds
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the atime seconds
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the current-time flags

    // -- resolve any "now" timestamp to the current Unix time --
    emitter.instruction("test rdx, 3");                                         // any current-time flag bit set?
    emitter.instruction("jz __rt_tma_built_x86");                               // no now-flags: both timestamps are explicit
    abi::emit_call_label(emitter, "__rt_time");                                 // rax = current Unix timestamp
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the flags after the time call
    emitter.instruction("test rcx, 2");                                         // mtime-now (bit1) requested?
    emitter.instruction("jz __rt_tma_now_atime_x86");                           // skip when mtime is explicit
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // mtime = current time
    emitter.label("__rt_tma_now_atime_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the flags for the atime check
    emitter.instruction("test rcx, 1");                                         // atime-now (bit0) requested?
    emitter.instruction("jz __rt_tma_built_x86");                               // skip when atime is explicit
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // atime = current time
    emitter.label("__rt_tma_built_x86");

    // -- allocate the 2-element indexed array and stamp its value_type --
    emitter.instruction("mov rdi, 2");                                          // capacity: two elements (mtime, atime)
    emitter.instruction("mov rsi, 8");                                          // boxed Mixed slots store one pointer each
    abi::emit_call_label(emitter, "__rt_array_new");                            // rax = indexed array backing storage
    emit_array_value_type_stamp(emitter, "rax", &PhpType::Mixed);
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the array pointer while filling slots

    // -- element 0: box mtime as Mixed(int) and store at array[0] --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // value_lo = mtime seconds
    emitter.instruction("xor esi, esi");                                        // value_hi = 0 for an integer scalar
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // rax = boxed Mixed(int) mtime
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the array pointer
    emitter.instruction("mov QWORD PTR [r11 + 24], rax");                       // array[0] = boxed mtime

    // -- element 1: box atime as Mixed(int) and store at array[1] --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // value_lo = atime seconds
    emitter.instruction("xor esi, esi");                                        // value_hi = 0 for an integer scalar
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // rax = boxed Mixed(int) atime
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the array pointer
    emitter.instruction("mov QWORD PTR [r11 + 32], rax");                       // array[1] = boxed atime
    emitter.instruction("mov QWORD PTR [r11], 2");                              // publish the indexed-array length

    // -- box the array as Mixed(tag 4); this increfs the array to refcount 2 --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // value_lo = array pointer
    emitter.instruction("xor esi, esi");                                        // value_hi unused for arrays
    emitter.instruction("mov eax, 4");                                          // runtime tag 4 = indexed array
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // rax = boxed Mixed(array)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the boxed Mixed pointer (mtime slot reused)

    // -- drop our own array reference so the Mixed cell is the sole owner --
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the array pointer
    abi::emit_call_label(emitter, "__rt_decref_any");                           // array refcount 2 -> 1 (owned by the Mixed)

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the boxed Mixed(array) pointer
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed metadata array
}
