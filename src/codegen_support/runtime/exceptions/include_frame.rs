//! Purpose:
//! Emits the shared `__rt_include_frame_reader` that materializes the synthetic frame of a
//! statically inlined `require_once`/`include_once` body.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed` next to the backtrace helpers.
//! - The activation chain: `Op::IncludeFrameEnter` stores this label in the record's `[24]` word.
//!
//! Key details:
//! - The frame reader contract is the one every compiled PHP frame publishes: it receives the
//!   record in `x0`/`rdi`, the `debug_backtrace()` option mask in `x1`/`rsi`, and the print index
//!   in `x2`/`rdx`, and returns one OWNED BOXED Mixed cell holding the frame hash. Returning the
//!   hash payload instead of its box corrupts the trace array that consumes the answer.
//! - The record's `[16]` word carries this site's descriptor (the walkers only compare `[16]`
//!   with the record address to recognize an ownership guard), so one reader serves every site:
//!   `{file ptr, file len, line, info ptr, path ptr, path len}`. The first two words are also the
//!   `{ptr, len}` file record `__rt_backtrace_frame_hash` expects.
//! - `args` reproduces PHP's `require_once('<resolved path>')`, unless the caller asked for
//!   `DEBUG_BACKTRACE_IGNORE_ARGS`.
//! - The descriptor, the path string, and the function name are emitted into the module's data
//!   pool by `crate::codegen::lower_inst::core_includes`, so no allocation or copy of the path
//!   happens here.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::types::PhpType;

/// Emits `__rt_include_frame_reader` and its builder for the target architecture.
pub fn emit_include_frame_reader(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_include_frame_aarch64(emitter),
        Arch::X86_64 => emit_include_frame_x86_64(emitter),
    }
}

/// AArch64 reader and builder.
fn emit_include_frame_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: include_frame_reader ---");
    emitter.label_global("__rt_include_frame_reader");
    emitter.instruction("ldr x0, [x0, #16]");                                   // load this site's descriptor from the record
    emitter.instruction("b __rt_include_frame_build");                          // share the frame construction

    emitter.blank();
    emitter.label_global("__rt_include_frame_build");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve frame linkage and the return address
    emitter.instruction("mov x29, sp");                                         // establish the builder frame
    emitter.instruction("sub sp, sp, #48");                                     // reserve descriptor, options, args, array, string box
    emitter.instruction("str x0, [sp, #0]");                                    // [sp+0] the site descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // [sp+8] the option mask
    emitter.instruction("str xzr, [sp, #16]");                                  // [sp+16] args = none
    // php keeps an include frame's `args` even under DEBUG_BACKTRACE_IGNORE_ARGS: the flag
    // suppresses function arguments, and an include frame carries its resolved path instead.
    emitter.instruction("mov x0, #1");                                          // one argument
    emitter.instruction("mov x1, #8");                                          // one boxed Mixed word per value
    emitter.instruction("bl __rt_array_new");                                   // allocate the argument array
    crate::codegen_support::arrays::emit_array_value_type_stamp(emitter, "x0", &PhpType::Mixed);
    emitter.instruction("str x0, [sp, #24]");                                   // keep the array across the string boxing
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the descriptor
    emitter.instruction("ldp x1, x2, [x9, #32]");                               // the resolved include path
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // box the static path into a Mixed cell
    emitter.instruction("str x0, [sp, #32]");                                   // keep the box for its release
    emitter.instruction("mov x1, x0");                                          // the value the array retains
    emitter.instruction("ldr x0, [sp, #24]");                                   // the argument array
    emitter.instruction("bl __rt_array_push_refcounted");                       // append, retaining the box
    emitter.instruction("str x0, [sp, #16]");                                   // publish the final argument array
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the temporary box
    emitter.instruction("bl __rt_decref_mixed");                                // the array owns its own reference now
    emitter.instruction("ldr x0, [sp, #0]");                                    // the descriptor is also the file record
    emitter.instruction("ldr x1, [x0, #16]");                                   // the include statement's line
    emitter.instruction("ldr x2, [x0, #24]");                                   // the function info record
    emitter.instruction("mov x3, #0");                                          // require_once has no receiver
    emitter.instruction("ldr x4, [sp, #16]");                                   // the argument array (or none)
    emitter.instruction("bl __rt_backtrace_frame_hash");                        // build the owned boxed frame hash
    emitter.instruction("add sp, sp, #48");                                     // release the builder slots
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame linkage and the return address
    emitter.instruction("ret");                                                 // return the boxed frame
}

/// x86_64 reader and builder.
fn emit_include_frame_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: include_frame_reader ---");
    emitter.label_global("__rt_include_frame_reader");
    emitter.instruction("mov rdi, QWORD PTR [rdi + 16]");                       // load this site's descriptor from the record
    emitter.instruction("jmp __rt_include_frame_build");                        // share the frame construction

    emitter.blank();
    emitter.label_global("__rt_include_frame_build");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the builder frame
    emitter.instruction("sub rsp, 48");                                         // reserve descriptor, options, args, array, string box
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // [rbp-8] the site descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // [rbp-16] the option mask
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // [rbp-24] args = none
    // See the AArch64 sibling: php keeps an include frame's `args` under IGNORE_ARGS.
    emitter.instruction("mov rdi, 1");                                          // one argument
    emitter.instruction("mov rsi, 8");                                          // one boxed Mixed word per value
    emitter.instruction("call __rt_array_new");                                 // allocate the argument array
    crate::codegen_support::arrays::emit_array_value_type_stamp(emitter, "rax", &PhpType::Mixed);
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // keep the array across the string boxing
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the descriptor
    emitter.instruction("mov rdi, QWORD PTR [r10 + 32]");                       // the resolved include path pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 40]");                       // the resolved include path length
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // box the static path into a Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // keep the box for its release
    emitter.instruction("mov rsi, rax");                                        // the value the array retains
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // the argument array
    emitter.instruction("call __rt_array_push_refcounted");                     // append, retaining the box
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // publish the final argument array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the temporary box
    emitter.instruction("call __rt_decref_mixed");                              // the array owns its own reference now
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the descriptor is also the file record
    emitter.instruction("mov rsi, QWORD PTR [rdi + 16]");                       // the include statement's line
    emitter.instruction("mov rdx, QWORD PTR [rdi + 24]");                       // the function info record
    emitter.instruction("xor ecx, ecx");                                        // require_once has no receiver
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // the argument array (or none)
    emitter.instruction("call __rt_backtrace_frame_hash");                      // build the owned boxed frame hash
    emitter.instruction("add rsp, 48");                                         // release the builder slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed frame
}
