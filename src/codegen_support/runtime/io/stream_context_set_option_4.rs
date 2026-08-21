//! Purpose:
//! Emits the `__rt_stream_context_set_option_4` runtime helper used by
//! the 4-arg form of `stream_context_set_option($ctx, $wrapper, $opt,
//! $value)`. The helper navigates the nested
//! `_stream_context_options[wrapper][option] = value` structure,
//! creating intermediate sub-hashes as needed.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The `stream_context_set_option` builtin emitter when the call has
//!   four args.
//!
//! Key details:
//! - The caller passes one owned boxed Mixed cell, so every PHP value shape
//!   round-trips without string coercion and the option hash takes ownership.
//! - The top hash is made unique before lookup, and an existing wrapper map is
//!   shallow-cloned before mutation, so previously returned snapshots remain
//!   isolated at both nesting levels.
//! - The sub-hash is re-inserted into the top-level hash after every
//!   mutation so a `__rt_hash_set` growth doesn't leave a stale pointer.
//! - On a top-level hash relocation, `_stream_context_options` is
//!   updated to the new pointer.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// `__rt_stream_context_set_option_4`:
/// Input:  AArch64 x0=wrapper_ptr x1=wrapper_len x2=opt_ptr x3=opt_len
///                 x4=owned boxed Mixed value, x5=0.
///         x86_64  rdi=wrapper_ptr rsi=wrapper_len rdx=opt_ptr rcx=opt_len
///                 r8=owned boxed Mixed value, r9=0.
/// Output: 1 always (the helper never fails — all allocation failures
///         abort through `__rt_heap_alloc`'s exhaustion path).
pub fn emit_stream_context_set_option_4(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_context_set_option_4_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_context_set_option_4 ---");
    emitter.label_global("__rt_stream_context_set_option_4");

    // Frame (96 bytes):
    //   [sp,  0] wrapper_ptr
    //   [sp,  8] wrapper_len
    //   [sp, 16] opt_ptr
    //   [sp, 24] opt_len
    //   [sp, 32] owned boxed Mixed value
    //   [sp, 40] reserved
    //   [sp, 48] top hash pointer (after any grow)
    //   [sp, 56] sub hash pointer (after any grow)
    //   [sp, 72] saved x29
    //   [sp, 80] saved x30
    emitter.instruction("sub sp, sp, #96");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #72]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #72");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save wrapper_ptr
    emitter.instruction("str x1, [sp, #8]");                                    // save wrapper_len
    emitter.instruction("str x2, [sp, #16]");                                   // save opt_ptr
    emitter.instruction("str x3, [sp, #24]");                                   // save opt_len
    emitter.instruction("str x4, [sp, #32]");                                   // save the owned boxed Mixed option value
    emitter.instruction("str x5, [sp, #40]");                                   // save the reserved high payload word

    // -- ensure _stream_context_options has a top-level hash --
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("ldr x10, [x9]");                                       // x10 = current top hash (may be null)
    emitter.instruction("cbnz x10, __rt_scso4_top_ok");                         // already initialised → skip allocation
    emitter.instruction("mov x0, #4");                                          // initial capacity for the wrapper-keyed top hash
    emitter.instruction("mov x1, #7");                                          // value tag = Mixed (the top hash holds Mixed-boxed sub-hashes)
    emitter.instruction("bl __rt_hash_new");                                    // call runtime helper
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("str x0, [x9]");                                        // publish the fresh top hash
    emitter.instruction("mov x10, x0");                                         // move runtime value between registers
    emitter.label("__rt_scso4_top_ok");
    emitter.instruction("mov x0, x10");                                         // pass the current scratch owner to the COW helper
    emitter.instruction("bl __rt_hash_ensure_unique");                          // isolate the wrapper map before nested mutation
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("str x0, [x9]");                                        // publish the unique top hash returned by the helper
    emitter.instruction("mov x10, x0");                                         // keep the unique top hash ready for lookup
    emitter.instruction("str x10, [sp, #48]");                                  // save the top hash pointer

    // -- look up the sub-hash by wrapper key --
    emitter.instruction("mov x0, x10");                                         // top hash
    emitter.instruction("ldr x1, [sp, #0]");                                    // wrapper_ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // wrapper_len
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=value_lo (sub-hash ptr on hit)
    emitter.instruction("cbnz x0, __rt_scso4_sub_found");                       // branch when the checked value is nonzero or different
    // Not found → allocate a new sub-hash.
    emitter.instruction("mov x0, #4");                                          // initial capacity for the option sub-hash
    emitter.instruction("mov x1, #7");                                          // value tag = Mixed (per-option boxed values)
    emitter.instruction("bl __rt_hash_new");                                    // call runtime helper
    emitter.instruction("str x0, [sp, #56]");                                   // save the new sub-hash
    emitter.instruction("b __rt_scso4_have_sub");                               // continue at target label

    emitter.label("__rt_scso4_sub_found");
    emitter.instruction("mov x0, x1");                                          // pass the borrowed wrapper map to the clone helper
    emitter.instruction("bl __rt_hash_clone_shallow");                          // create one owned sub-hash for isolated mutation
    emitter.instruction("str x0, [sp, #56]");                                   // save the owned wrapper-map clone

    emitter.label("__rt_scso4_have_sub");
    // -- transfer the boxed Mixed option value into the sub-hash --
    emitter.instruction("ldr x0, [sp, #56]");                                   // sub-hash
    emitter.instruction("ldr x1, [sp, #16]");                                   // opt_ptr
    emitter.instruction("ldr x2, [sp, #24]");                                   // opt_len
    emitter.instruction("ldr x3, [sp, #32]");                                   // owned boxed Mixed cell becomes value_lo
    emitter.instruction("mov x4, xzr");                                         // boxed Mixed values use no high payload word
    emitter.instruction("mov x5, #7");                                          // runtime tag 7 identifies a boxed Mixed cell
    emitter.instruction("bl __rt_hash_set");                                    // x0 = possibly-grown sub-hash
    emitter.instruction("str x0, [sp, #56]");                                   // record the updated sub-hash

    // -- re-insert the owned sub-hash into the unique top-level hash so any
    //    growth is visible to the next set_option call. The insertion consumes
    //    the owned clone/new hash while releasing the previous child. --
    emitter.instruction("ldr x0, [sp, #48]");                                   // top hash
    emitter.instruction("ldr x1, [sp, #0]");                                    // wrapper_ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // wrapper_len
    emitter.instruction("ldr x3, [sp, #56]");                                   // updated sub-hash → value_lo
    emitter.instruction("mov x4, #0");                                          // value_hi unused for refcounted heap pointers
    emitter.instruction("mov x5, #5");                                          // value tag = assoc array
    emitter.instruction("bl __rt_hash_set");                                    // x0 = possibly-grown top hash
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("str x0, [x9]");                                        // publish any new top pointer

    emitter.instruction("mov x0, #1");                                          // PHP true
    emitter.instruction("ldp x29, x30, [sp, #72]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for stream context set option 4.
fn emit_stream_context_set_option_4_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_context_set_option_4 ---");
    emitter.label_global("__rt_stream_context_set_option_4");

    // rbp-relative frame:
    //   [rbp -  8] wrapper_ptr
    //   [rbp - 16] wrapper_len
    //   [rbp - 24] opt_ptr
    //   [rbp - 32] opt_len
    //   [rbp - 40] owned boxed Mixed value
    //   [rbp - 48] reserved
    //   [rbp - 56] top hash pointer
    //   [rbp - 64] sub hash pointer
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 80");                                         // 64 used + 16 alignment padding
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save wrapper_ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save wrapper_len
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save opt_ptr
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save opt_len
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the owned boxed Mixed option value
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the reserved high payload word

    // -- ensure top-level hash exists --
    abi::emit_load_symbol_to_reg(emitter, "rax", "_stream_context_options", 0); // current top hash (may be null)
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jnz __rt_scso4_top_ok_x86");                           // branch when the checked value is nonzero or different
    emitter.instruction("mov edi, 4");                                          // initial capacity
    emitter.instruction("mov esi, 7");                                          // value tag = Mixed
    emitter.instruction("call __rt_hash_new");                                  // call runtime helper
    abi::emit_store_reg_to_symbol(emitter, "rax", "_stream_context_options", 0); // store runtime value
    emitter.label("__rt_scso4_top_ok_x86");
    emitter.instruction("mov rdi, rax");                                        // pass the current scratch owner to the COW helper
    emitter.instruction("call __rt_hash_ensure_unique");                        // isolate the wrapper map before nested mutation
    abi::emit_store_reg_to_symbol(emitter, "rax", "_stream_context_options", 0); // publish the unique top hash returned by the helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save top hash ptr

    // -- look up the sub-hash --
    emitter.instruction("mov rdi, rax");                                        // top hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // wrapper_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // wrapper_len
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=value_lo, rsi=value_hi, rcx=tag (SysV mirror of x1/x2/x3 on ARM64)
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jnz __rt_scso4_sub_found_x86");                        // branch when the checked value is nonzero or different
    emitter.instruction("mov edi, 4");                                          // prepare SysV call argument
    emitter.instruction("mov esi, 7");                                          // prepare SysV call argument
    emitter.instruction("call __rt_hash_new");                                  // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // new sub-hash
    emitter.instruction("jmp __rt_scso4_have_sub_x86");                         // continue at target label
    emitter.label("__rt_scso4_sub_found_x86");
    emitter.instruction("call __rt_hash_clone_shallow");                        // clone the borrowed sub-hash still carried in rdi
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the owned wrapper-map clone
    emitter.label("__rt_scso4_have_sub_x86");

    // -- insert option → value into the sub-hash --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // sub-hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // opt_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // opt_len
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // owned boxed Mixed cell becomes value_lo
    emitter.instruction("xor r8, r8");                                          // boxed Mixed values use no high payload word
    emitter.instruction("mov r9, 7");                                           // runtime tag 7 identifies a boxed Mixed cell
    emitter.instruction("call __rt_hash_set");                                  // rax = possibly-grown sub-hash
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // store runtime value

    // -- re-insert the owned clone/new sub-hash into the unique top hash. --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // top hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // prepare SysV call argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // prepare SysV call argument
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // updated sub-hash
    emitter.instruction("xor r8, r8");                                          // clear register value
    emitter.instruction("mov r9, 5");                                           // value tag = assoc array
    emitter.instruction("call __rt_hash_set");                                  // rax = updated top
    abi::emit_store_reg_to_symbol(emitter, "rax", "_stream_context_options", 0); // store runtime value

    emitter.instruction("mov eax, 1");                                          // PHP true
    emitter.instruction("add rsp, 80");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
