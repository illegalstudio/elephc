//! Purpose:
//! Emits the `__rt_var_dump_*` runtime walkers that render PHP `var_dump`
//! output for indexed arrays, associative arrays (hashes), and the recursive
//! single-value renderer `__rt_var_dump_value`, matching PHP's
//! `array(N) {\n  [key]=>\n  TYPE(VAL)\n}\n` layout with 2-space-per-level
//! indentation.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
//! - The `var_dump` builtin emitter (`codegen_ir::lower_inst::builtins::debug`)
//!   when the value's static type is a homogeneous array, a hash, a boxed
//!   Mixed cell, or a union (all routed through `__rt_var_dump_value`).
//!
//! Key details:
//! - Array layout reused from the existing JSON encoders: 24-byte header
//!   (len at offset 0, value_type at offset 8, refcount at offset 16)
//!   followed by 8-byte elements starting at offset 24.
//! - String elements use the elephc string-result ABI: 16-byte slots
//!   storing (ptr, len) — so element[N] for an indexed string array lives
//!   at offsets `24 + N*16` (ptr) and `32 + N*16` (len).
//! - The rodata prefixes (`_vd_int_prefix`, `_vd_str_key_open`, …) carry no
//!   leading indent; indentation is written separately by
//!   `__rt_var_dump_spaces` so nested arrays indent correctly.
//! - `__rt_var_dump_value` is the recursive entry point modeled on
//!   `__rt_print_r_value`: tags 4/5 recurse into `__rt_var_dump_indexed` /
//!   `__rt_var_dump_hash` with `entry_indent = indent + 2`, tag 7 unboxes a
//!   Mixed cell and redispatches, tag 6 (object) stays `NULL` (documented
//!   limitation). A depth cap (`indent > 128`) stops cyclic arrays.
//! - Homogeneous typed-array walkers (`__rt_var_dump_array_int`, …) remain as
//!   a fast path for arrays that cannot contain nested containers.

use crate::codegen::{emit::Emitter, platform::Arch};
use crate::codegen::abi;

/// `__rt_var_dump_array_int`: emit one `[N]=>\n  int(VAL)\n` block per
/// element of an indexed `int[]` array. Input: AArch64 x0 / x86_64 rdi =
/// array pointer.
pub fn emit_var_dump_array_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_array_int_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_int ---");
    emitter.label_global("__rt_var_dump_array_int");

    // Frame (48 bytes): [0]=array ptr, [8]=index, [16]=value scratch,
    //   [32]=x29, [40]=x30.
    emitter.instruction("sub sp, sp, #48");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the array pointer
    emitter.instruction("str xzr, [sp, #8]");                                   // index = 0

    emitter.label("__rt_vd_arr_int_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the array pointer
    emitter.instruction("ldr x10, [x9]");                                       // load the element count from header offset 0
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the current index
    emitter.instruction("cmp x11, x10");                                        // processed every element?
    emitter.instruction("b.ge __rt_vd_arr_int_done");                           // walk complete

    // -- emit `  [N]=>\n` --
    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // emits "[N]=>\n" for x11=index

    // -- emit `  int(VAL)\n` --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload index
    emitter.instruction("add x12, x11, #3");                                    // skip the 24-byte (3 quads) header
    emitter.instruction("ldr x0, [x9, x12, lsl #3]");                           // load element[index]
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the value across the spaces call
    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr x0, [sp, #16]");                                   // restore the integer value
    emitter.instruction("bl __rt_var_dump_emit_int_line");                      // emits "int(VAL)\n" for x0=value

    emitter.instruction("ldr x11, [sp, #8]");                                   // reload index
    emitter.instruction("add x11, x11, #1");                                    // advance index
    emitter.instruction("str x11, [sp, #8]");                                   // save updated index
    emitter.instruction("b __rt_vd_arr_int_loop");                              // continue scanning

    emitter.label("__rt_vd_arr_int_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the var_dump builtin caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump array int.
fn emit_var_dump_array_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_int ---");
    emitter.label_global("__rt_var_dump_array_int");

    // rbp-relative scratch:
    //   [rbp - 8]  array pointer
    //   [rbp - 16] element index
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // scratch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // index = 0

    emitter.label("__rt_vd_arr_int_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the array pointer
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the element count
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the current index
    emitter.instruction("cmp r11, r10");                                        // processed every element?
    emitter.instruction("jge __rt_vd_arr_int_done_x86");                        // walk complete

    // -- emit `  [N]=>\n` (helper expects index in rdi) --
    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, r11");                                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // call runtime helper

    // -- emit `  int(VAL)\n` --
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload index
    emitter.instruction("mov r12, r11");                                        // move runtime value between registers
    emitter.instruction("add r12, 3");                                          // skip 3-quad header
    emitter.instruction("mov r13, QWORD PTR [r9 + r12 * 8]");                   // load element[index] into the emit helper's first arg
    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, r13");                                        // restore the integer value
    emitter.instruction("call __rt_var_dump_emit_int_line");                    // call runtime helper

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload index
    emitter.instruction("add r11, 1");                                          // advance index
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save updated index
    emitter.instruction("jmp __rt_vd_arr_int_loop_x86");                        // continue scanning

    emitter.label("__rt_vd_arr_int_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the var_dump builtin caller
}

/// `__rt_var_dump_array_str`: emit one `[N]=>\n  string(LEN) "VAL"\n`
/// block per element of an indexed `string[]` array. Input: AArch64 x0 /
/// x86_64 rdi = array pointer.
pub fn emit_var_dump_array_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_array_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_str ---");
    emitter.label_global("__rt_var_dump_array_str");

    // Frame (48 bytes): [0]arr [8]index [16]ptr [24]len [32]x29 [40]x30.
    emitter.instruction("sub sp, sp, #48");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // store runtime value
    emitter.instruction("str xzr, [sp, #8]");                                   // store runtime value

    emitter.label("__rt_vd_arr_str_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the array pointer
    emitter.instruction("ldr x10, [x9]");                                       // load the element count
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the current index
    emitter.instruction("cmp x11, x10");                                        // processed every element?
    emitter.instruction("b.ge __rt_vd_arr_str_done");                           // walk complete

    // -- emit `  [N]=>\n` --
    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // emits "[N]=>\n" for x11=index

    // -- emit `  string(LEN) "VAL"\n` --
    // String elements are 16-byte slots: ptr at offset 24+16*N, len at 32+16*N.
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload index
    emitter.instruction("lsl x12, x11, #4");                                    // index * 16
    emitter.instruction("add x12, x12, #24");                                   // element base offset = 24 + index*16
    emitter.instruction("add x13, x9, x12");                                    // element address
    emitter.instruction("ldr x1, [x13]");                                       // load element string ptr
    emitter.instruction("ldr x2, [x13, #8]");                                   // load element string len
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save ptr/len across the spaces call
    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload string ptr
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload string len
    emitter.instruction("bl __rt_var_dump_emit_string_line");                   // emits `string(LEN) "VAL"\n`

    emitter.instruction("ldr x11, [sp, #8]");                                   // reload index
    emitter.instruction("add x11, x11, #1");                                    // advance index
    emitter.instruction("str x11, [sp, #8]");                                   // save updated index
    emitter.instruction("b __rt_vd_arr_str_loop");                              // continue scanning

    emitter.label("__rt_vd_arr_str_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump array str.
fn emit_var_dump_array_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_str ---");
    emitter.label_global("__rt_var_dump_array_str");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 32");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // index = 0

    emitter.label("__rt_vd_arr_str_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the array pointer
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the element count
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the current index
    emitter.instruction("cmp r11, r10");                                        // processed every element?
    emitter.instruction("jge __rt_vd_arr_str_done_x86");                        // walk complete

    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, r11");                                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // call runtime helper

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload index
    emitter.instruction("mov r12, r11");                                        // move runtime value between registers
    emitter.instruction("shl r12, 4");                                          // index * 16
    emitter.instruction("add r12, 24");                                         // element base offset
    emitter.instruction("add r12, r9");                                         // element address
    emitter.instruction("mov rax, QWORD PTR [r12]");                            // string ptr
    emitter.instruction("mov rcx, QWORD PTR [r12 + 8]");                        // string len
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save string ptr across the spaces call
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save string len across the spaces call
    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload string ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload string len
    emitter.instruction("call __rt_var_dump_emit_string_line");                 // call runtime helper

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("add r11, 1");                                          // advance runtime pointer or counter
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // store runtime value
    emitter.instruction("jmp __rt_vd_arr_str_loop_x86");                        // continue at target label

    emitter.label("__rt_vd_arr_str_done_x86");
    emitter.instruction("add rsp, 32");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_indexed_key`: emit `[N]=>\n` for a numeric index.
/// The caller writes the entry indent via `__rt_var_dump_spaces` first.
/// Input: AArch64 x11 / x86_64 rdi = index value.
pub fn emit_var_dump_emit_indexed_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_emit_indexed_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_indexed_key ---");
    emitter.label_global("__rt_var_dump_emit_indexed_key");

    emitter.instruction("sub sp, sp, #16");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer

    // Emit "["
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_indent_open");
    emitter.instruction("mov x2, #1");                                          // len("[") = 1
    emitter.instruction("mov x0, #1");                                          // fd=stdout
    emitter.syscall(4);

    // itoa(index) → x1/x2
    emitter.instruction("mov x0, x11");                                         // x11 holds the index from the caller's loop
    emitter.instruction("bl __rt_itoa");                                        // call runtime helper
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    // Emit "]=>\n"
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_close_arrow");
    emitter.instruction("mov x2, #4");                                          // len("]=>\n") = 4
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump emit indexed key.
fn emit_var_dump_emit_indexed_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_indexed_key ---");
    emitter.label_global("__rt_var_dump_emit_indexed_key");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the index

    // Emit "["
    abi::emit_symbol_address(emitter, "rsi", "_vd_indent_open");                // load runtime data address
    emitter.instruction("mov edx, 1");                                          // len("[") = 1
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    // itoa(index)
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // prepare runtime result value
    emitter.instruction("call __rt_itoa");                                      // call runtime helper
    emitter.instruction("mov rsi, rax");                                        // prepare SysV call argument
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    // Emit "]=>\n"
    abi::emit_symbol_address(emitter, "rsi", "_vd_close_arrow");                // load runtime data address
    emitter.instruction("mov edx, 4");                                          // prepare SysV call argument
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_int_line`: emit `int(VAL)\n` for a single int. The
/// caller writes the entry indent via `__rt_var_dump_spaces` first.
/// Input: AArch64 x0 / x86_64 rdi = value.
pub fn emit_var_dump_emit_int_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_emit_int_line_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_int_line ---");
    emitter.label_global("__rt_var_dump_emit_int_line");

    emitter.instruction("sub sp, sp, #16");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer

    // Emit "int("
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_int_prefix");
    emitter.instruction("mov x2, #4");                                          // len("int(") = 4
    emitter.instruction("mov x9, x0");                                          // preserve value
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    // itoa(value)
    emitter.instruction("mov x0, x9");                                          // prepare AArch64 call argument
    emitter.instruction("bl __rt_itoa");                                        // call runtime helper
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    // Emit ")\n"
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_close_paren");
    emitter.instruction("mov x2, #2");                                          // len(")\n") = 2
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump emit int line.
fn emit_var_dump_emit_int_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_int_line ---");
    emitter.label_global("__rt_var_dump_emit_int_line");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save value

    abi::emit_symbol_address(emitter, "rsi", "_vd_int_prefix");                 // load runtime data address
    emitter.instruction("mov edx, 4");                                          // len("int(") = 4
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // prepare runtime result value
    emitter.instruction("call __rt_itoa");                                      // call runtime helper
    emitter.instruction("mov rsi, rax");                                        // prepare SysV call argument
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    abi::emit_symbol_address(emitter, "rsi", "_vd_close_paren");                // load runtime data address
    emitter.instruction("mov edx, 2");                                          // prepare SysV call argument
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_string_line`: emit `string(LEN) "VAL"\n` for a
/// string. The caller writes the entry indent via `__rt_var_dump_spaces` first.
/// Input: AArch64 x1=ptr x2=len / x86_64 rdi=ptr rsi=len.
pub fn emit_var_dump_emit_string_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_emit_string_line_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_string_line ---");
    emitter.label_global("__rt_var_dump_emit_string_line");

    emitter.instruction("sub sp, sp, #32");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save ptr/len

    // Emit "string("
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_str_prefix");
    emitter.instruction("mov x2, #7");                                          // len("string(") = 7
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    // itoa(len)
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload len
    emitter.instruction("bl __rt_itoa");                                        // call runtime helper
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    // Emit ") \""
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_close_paren_space");
    emitter.instruction("mov x2, #3");                                          // len(") \"") = 3 — includes the opening quote
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    // Write the actual bytes
    emitter.instruction("ldr x1, [sp, #0]");                                    // ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // len
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    // Emit "\"\n"
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_close_quote");
    emitter.instruction("mov x2, #2");                                          // len("\"\n") = 2
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump emit string line.
fn emit_var_dump_emit_string_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_string_line ---");
    emitter.label_global("__rt_var_dump_emit_string_line");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save len

    abi::emit_symbol_address(emitter, "rsi", "_vd_str_prefix");                 // load runtime data address
    emitter.instruction("mov edx, 7");                                          // len("string(") = 7
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // prepare runtime result value
    emitter.instruction("call __rt_itoa");                                      // call runtime helper
    emitter.instruction("mov rsi, rax");                                        // prepare SysV call argument
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    abi::emit_symbol_address(emitter, "rsi", "_vd_close_paren_space");          // load runtime data address
    emitter.instruction("mov edx, 3");                                          // prepare SysV call argument
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // len
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    abi::emit_symbol_address(emitter, "rsi", "_vd_close_quote");                // load runtime data address
    emitter.instruction("mov edx, 2");                                          // prepare SysV call argument
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_bool_line`: emit `bool(true)\n` or
/// `bool(false)\n` for a single bool. The caller writes the entry indent via
/// `__rt_var_dump_spaces` first. Input: AArch64 x0 / x86_64 rdi =
/// value (0 = false, non-zero = true).
pub fn emit_var_dump_emit_bool_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_emit_bool_line_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_bool_line ---");
    emitter.label_global("__rt_var_dump_emit_bool_line");

    let false_label = "__rt_vd_bool_false";
    let done_label = "__rt_vd_bool_done";
    emitter.instruction(&format!("cbz x0, {}", false_label));                   // value == 0 → false line
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_bool_true_line");
    emitter.instruction("mov x2, #11");                                         // len("bool(true)\n") = 11
    emitter.instruction(&format!("b {}", done_label));                          // continue at target label
    emitter.label(false_label);
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_bool_false_line");
    emitter.instruction("mov x2, #12");                                         // len("bool(false)\n") = 12
    emitter.label(done_label);
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump emit bool line.
fn emit_var_dump_emit_bool_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_bool_line ---");
    emitter.label_global("__rt_var_dump_emit_bool_line");

    let false_label = "__rt_vd_bool_false_x86";
    let done_label = "__rt_vd_bool_done_x86";
    emitter.instruction("test rdi, rdi");                                       // check whether the runtime value is zero
    emitter.instruction(&format!("jz {}", false_label));                        // branch when the checked value is zero or equal
    abi::emit_symbol_address(emitter, "rsi", "_vd_bool_true_line");             // load runtime data address
    emitter.instruction("mov edx, 11");                                         // len("bool(true)\n") = 11
    emitter.instruction(&format!("jmp {}", done_label));                        // continue at target label
    emitter.label(false_label);
    abi::emit_symbol_address(emitter, "rsi", "_vd_bool_false_line");            // load runtime data address
    emitter.instruction("mov edx, 12");                                         // len("bool(false)\n") = 12
    emitter.label(done_label);
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // invoke kernel service
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_array_bool`: walk an indexed `bool[]` array and emit
/// one `[N]=>\n  bool(true|false)\n` block per element. Input: AArch64 x0 /
/// x86_64 rdi = array pointer.
pub fn emit_var_dump_array_bool(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_array_bool_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_bool ---");
    emitter.label_global("__rt_var_dump_array_bool");

    emitter.instruction("sub sp, sp, #48");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // store runtime value
    emitter.instruction("str xzr, [sp, #8]");                                   // store runtime value

    emitter.label("__rt_vd_arr_bool_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // load runtime value
    emitter.instruction("ldr x10, [x9]");                                       // element count
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("cmp x11, x10");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_vd_arr_bool_done");                          // branch when comparison is at least target

    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // call runtime helper

    emitter.instruction("ldr x9, [sp, #0]");                                    // load runtime value
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("add x12, x11, #3");                                    // skip 3-quad header
    emitter.instruction("ldr x13, [x9, x12, lsl #3]");                          // load element[index] (0 or 1)
    emitter.instruction("str x13, [sp, #16]");                                  // save the bool value across the spaces call
    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the bool value
    emitter.instruction("bl __rt_var_dump_emit_bool_line");                     // call runtime helper

    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("add x11, x11, #1");                                    // advance runtime pointer or counter
    emitter.instruction("str x11, [sp, #8]");                                   // store runtime value
    emitter.instruction("b __rt_vd_arr_bool_loop");                             // continue at target label

    emitter.label("__rt_vd_arr_bool_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_float_line`: emit `float(VAL)\n` for a single
/// f64. The caller writes the entry indent via `__rt_var_dump_spaces` first.
/// Input: AArch64 d0 / x86_64 xmm0 = value.
pub fn emit_var_dump_emit_float_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_emit_float_line_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_float_line ---");
    emitter.label_global("__rt_var_dump_emit_float_line");

    emitter.instruction("sub sp, sp, #16");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer

    // Emit "float("
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_float_prefix");
    emitter.instruction("mov x2, #6");                                          // len("float(") = 6
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    // ftoa(d0) → x1=ptr, x2=len
    emitter.instruction("bl __rt_ftoa");                                        // call runtime helper
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    // Emit ")\n"
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_close_paren");
    emitter.instruction("mov x2, #2");                                          // prepare AArch64 call argument
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump emit float line.
fn emit_var_dump_emit_float_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_float_line ---");
    emitter.label_global("__rt_var_dump_emit_float_line");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("movsd QWORD PTR [rbp - 8], xmm0");                     // preserve xmm0 across the prefix syscall

    abi::emit_symbol_address(emitter, "rsi", "_vd_float_prefix");               // load runtime data address
    emitter.instruction("mov edx, 6");                                          // len("float(") = 6
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // reload xmm0 for ftoa
    emitter.instruction("call __rt_ftoa");                                      // rax=ptr, rdx=len
    emitter.instruction("mov rsi, rax");                                        // prepare SysV call argument
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    abi::emit_symbol_address(emitter, "rsi", "_vd_close_paren");                // load runtime data address
    emitter.instruction("mov edx, 2");                                          // prepare SysV call argument
    emitter.instruction("mov edi, 1");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("syscall");                                             // invoke kernel service

    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_array_float`: walk an indexed `float[]` array. Each
/// element is an 8-byte f64 stored at `arr + 24 + N*8`.
pub fn emit_var_dump_array_float(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_array_float_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_float ---");
    emitter.label_global("__rt_var_dump_array_float");

    emitter.instruction("sub sp, sp, #48");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // store runtime value
    emitter.instruction("str xzr, [sp, #8]");                                   // store runtime value

    emitter.label("__rt_vd_arr_float_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // load runtime value
    emitter.instruction("ldr x10, [x9]");                                       // load runtime value
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("cmp x11, x10");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_vd_arr_float_done");                         // branch when comparison is at least target

    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // call runtime helper

    emitter.instruction("ldr x9, [sp, #0]");                                    // load runtime value
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("add x12, x11, #3");                                    // skip 3-quad header
    emitter.instruction("ldr d0, [x9, x12, lsl #3]");                           // load f64 element[index]
    emitter.instruction("str d0, [sp, #16]");                                   // save the float across the spaces call
    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr d0, [sp, #16]");                                   // reload the float
    emitter.instruction("bl __rt_var_dump_emit_float_line");                    // call runtime helper

    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("add x11, x11, #1");                                    // advance runtime pointer or counter
    emitter.instruction("str x11, [sp, #8]");                                   // store runtime value
    emitter.instruction("b __rt_vd_arr_float_loop");                            // continue at target label

    emitter.label("__rt_vd_arr_float_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump array float.
fn emit_var_dump_array_float_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_float ---");
    emitter.label_global("__rt_var_dump_array_float");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 24");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // store runtime value

    emitter.label("__rt_vd_arr_float_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // prepare SysV call argument
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // move runtime value between registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("cmp r11, r10");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_vd_arr_float_done_x86");                      // branch when comparison is at least target

    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, r11");                                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // call runtime helper

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // prepare SysV call argument
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("mov r12, r11");                                        // move runtime value between registers
    emitter.instruction("add r12, 3");                                          // advance runtime pointer or counter
    emitter.instruction("movsd xmm0, QWORD PTR [r9 + r12 * 8]");                // load f64 element[index] into xmm0
    emitter.instruction("movsd QWORD PTR [rbp - 24], xmm0");                    // save the float across the spaces call
    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 24]");                    // reload the float
    emitter.instruction("call __rt_var_dump_emit_float_line");                  // call runtime helper

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("add r11, 1");                                          // advance runtime pointer or counter
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // store runtime value
    emitter.instruction("jmp __rt_vd_arr_float_loop_x86");                      // continue at target label

    emitter.label("__rt_vd_arr_float_done_x86");
    emitter.instruction("add rsp, 24");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_null_line`: emit `NULL\n` for a null payload. The
/// caller writes the entry indent via `__rt_var_dump_spaces` first.
pub fn emit_var_dump_emit_null_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: var_dump_emit_null_line ---");
        emitter.label_global("__rt_var_dump_emit_null_line");
        abi::emit_symbol_address(emitter, "rsi", "_vd_null_line");              // load runtime data address
        emitter.instruction("mov edx, 5");                                      // len("NULL\n") = 5
        emitter.instruction("mov edi, 1");                                      // prepare SysV call argument
        emitter.instruction("mov eax, 1");                                      // prepare runtime result value
        emitter.instruction("syscall");                                         // invoke kernel service
        emitter.instruction("ret");                                             // return to caller
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_null_line ---");
    emitter.label_global("__rt_var_dump_emit_null_line");
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_null_line");
    emitter.instruction("mov x2, #5");                                          // len("NULL\n") = 5
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_array_mixed`: walk an indexed array of Mixed cell
/// pointers and dispatch on each cell's runtime tag. Supports int,
/// string, float, bool payloads; nested arrays/objects fall back to NULL
/// (the recursive `__rt_var_dump_value` handles full nesting and is the
/// preferred entry point for `Array(Mixed)`).
pub fn emit_var_dump_array_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_array_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_mixed ---");
    emitter.label_global("__rt_var_dump_array_mixed");

    // -- Verify the array's value_type stamp says Mixed (=7). Static type
    //    Array<Mixed> can reach here for arrays that were boxed-into-Mixed
    //    and then unboxed back to indexed; those arrays still have their
    //    original concrete-typed slots, not Mixed cells. Walking them as
    //    Mixed cells would dereference garbage. The stamp lives in the
    //    packed heap kind word at [arr - 8] byte 1. --
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed array kind word
    emitter.instruction("lsr x9, x9, #8");                                      // shift the value_type tag into the low byte
    emitter.instruction("and x9, x9, #0x0f");                                   // isolate the value_type field (low nibble), dropping the COW bit
    emitter.instruction("cmp x9, #7");                                          // Mixed?
    emitter.instruction("b.ne __rt_vd_arr_mixed_skip");                         // not Mixed → leave the body empty

    // Frame (48 bytes): [0]arr [8]index [16]cell_ptr [32]x29 [40]x30.
    emitter.instruction("sub sp, sp, #48");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // array ptr
    emitter.instruction("str xzr, [sp, #8]");                                   // index = 0

    emitter.label("__rt_vd_arr_mixed_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // load runtime value
    emitter.instruction("ldr x10, [x9]");                                       // element count
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("cmp x11, x10");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_vd_arr_mixed_done");                         // branch when comparison is at least target

    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // call runtime helper

    emitter.instruction("ldr x9, [sp, #0]");                                    // load runtime value
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("add x12, x11, #3");                                    // skip 3-quad header
    emitter.instruction("ldr x13, [x9, x12, lsl #3]");                          // Mixed cell pointer
    emitter.instruction("str x13, [sp, #16]");                                  // save the Mixed cell pointer
    emitter.instruction("ldr x14, [x13]");                                      // runtime value tag at cell[0]
    emitter.instruction("cmp x14, #0");                                         // tag 0 = int
    emitter.instruction("b.eq __rt_vd_arr_mixed_int");                          // branch when the checked value is zero or equal
    emitter.instruction("cmp x14, #1");                                         // tag 1 = string
    emitter.instruction("b.eq __rt_vd_arr_mixed_str");                          // branch when the checked value is zero or equal
    emitter.instruction("cmp x14, #2");                                         // tag 2 = float
    emitter.instruction("b.eq __rt_vd_arr_mixed_flt");                          // branch when the checked value is zero or equal
    emitter.instruction("cmp x14, #3");                                         // tag 3 = bool
    emitter.instruction("b.eq __rt_vd_arr_mixed_bool");                         // branch when the checked value is zero or equal
    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("bl __rt_var_dump_emit_null_line");                     // unsupported tag → NULL fallback
    emitter.instruction("b __rt_vd_arr_mixed_next");                            // continue at target label

    emitter.label("__rt_vd_arr_mixed_int");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the Mixed cell pointer
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the int payload
    emitter.instruction("str x9, [sp, #16]");                                   // save the int payload across the spaces call
    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the int payload
    emitter.instruction("bl __rt_var_dump_emit_int_line");                      // call runtime helper
    emitter.instruction("b __rt_vd_arr_mixed_next");                            // continue at target label

    emitter.label("__rt_vd_arr_mixed_str");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the Mixed cell pointer
    emitter.instruction("ldr x1, [x0, #8]");                                    // string ptr
    emitter.instruction("ldr x2, [x0, #16]");                                   // string len
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save ptr/len across the spaces call
    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload string ptr
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload string len
    emitter.instruction("bl __rt_var_dump_emit_string_line");                   // call runtime helper
    emitter.instruction("b __rt_vd_arr_mixed_next");                            // continue at target label

    emitter.label("__rt_vd_arr_mixed_flt");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the Mixed cell pointer
    emitter.instruction("ldr d0, [x0, #8]");                                    // load the float payload
    emitter.instruction("str d0, [sp, #16]");                                   // save the float across the spaces call
    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr d0, [sp, #16]");                                   // reload the float
    emitter.instruction("bl __rt_var_dump_emit_float_line");                    // call runtime helper
    emitter.instruction("b __rt_vd_arr_mixed_next");                            // continue at target label

    emitter.label("__rt_vd_arr_mixed_bool");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the Mixed cell pointer
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the bool payload
    emitter.instruction("str x9, [sp, #16]");                                   // save the bool payload across the spaces call
    emitter.instruction("mov x0, #2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the bool payload
    emitter.instruction("bl __rt_var_dump_emit_bool_line");                     // call runtime helper

    emitter.label("__rt_vd_arr_mixed_next");
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("add x11, x11, #1");                                    // advance runtime pointer or counter
    emitter.instruction("str x11, [sp, #8]");                                   // store runtime value
    emitter.instruction("b __rt_vd_arr_mixed_loop");                            // continue at target label

    emitter.label("__rt_vd_arr_mixed_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release runtime stack frame
    emitter.label("__rt_vd_arr_mixed_skip");                                    // wrong stamp → return without any body
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump array mixed.
fn emit_var_dump_array_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_mixed ---");
    emitter.label_global("__rt_var_dump_array_mixed");

    // Defensive stamp check (see ARM64 prologue): only walk arrays
    // whose value_type stamp says Mixed (=7).
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // packed array kind word
    emitter.instruction("shr r9, 8");                                           // shift the value_type tag into the low byte
    emitter.instruction("and r9, 0x0f");                                        // isolate the value_type field (low nibble), dropping the COW bit
    emitter.instruction("cmp r9, 7");                                           // Mixed?
    emitter.instruction("jne __rt_vd_arr_mixed_skip_x86");                      // not Mixed → leave the body empty

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 32");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // store runtime value

    emitter.label("__rt_vd_arr_mixed_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // prepare SysV call argument
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // move runtime value between registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("cmp r11, r10");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_vd_arr_mixed_done_x86");                      // branch when comparison is at least target

    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, r11");                                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // call runtime helper

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // prepare SysV call argument
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("mov r12, r11");                                        // move runtime value between registers
    emitter.instruction("add r12, 3");                                          // advance runtime pointer or counter
    emitter.instruction("mov r13, QWORD PTR [r9 + r12 * 8]");                   // Mixed cell pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], r13");                       // save the Mixed cell pointer
    emitter.instruction("mov r14, QWORD PTR [r13]");                            // runtime value tag at cell[0]

    emitter.instruction("cmp r14, 0");                                          // compare runtime values for the next branch
    emitter.instruction("je __rt_vd_arr_mixed_int_x86");                        // branch when the checked value is zero or equal
    emitter.instruction("cmp r14, 1");                                          // compare runtime values for the next branch
    emitter.instruction("je __rt_vd_arr_mixed_str_x86");                        // branch when the checked value is zero or equal
    emitter.instruction("cmp r14, 2");                                          // compare runtime values for the next branch
    emitter.instruction("je __rt_vd_arr_mixed_flt_x86");                        // branch when the checked value is zero or equal
    emitter.instruction("cmp r14, 3");                                          // compare runtime values for the next branch
    emitter.instruction("je __rt_vd_arr_mixed_bool_x86");                       // branch when the checked value is zero or equal
    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("call __rt_var_dump_emit_null_line");                   // call runtime helper
    emitter.instruction("jmp __rt_vd_arr_mixed_next_x86");                      // continue at target label

    emitter.label("__rt_vd_arr_mixed_int_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the Mixed cell pointer
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the int payload
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the int payload across the spaces call
    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the int payload
    emitter.instruction("call __rt_var_dump_emit_int_line");                    // call runtime helper
    emitter.instruction("jmp __rt_vd_arr_mixed_next_x86");                      // continue at target label

    emitter.label("__rt_vd_arr_mixed_str_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the Mixed cell pointer
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the string ptr
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the Mixed cell pointer
    emitter.instruction("mov rcx, QWORD PTR [rcx + 16]");                       // load the string len
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the string ptr
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the string len
    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the string ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the string len
    emitter.instruction("call __rt_var_dump_emit_string_line");                 // call runtime helper
    emitter.instruction("jmp __rt_vd_arr_mixed_next_x86");                      // continue at target label

    emitter.label("__rt_vd_arr_mixed_flt_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the Mixed cell pointer
    emitter.instruction("movsd xmm0, QWORD PTR [rax + 8]");                     // load the mixed float payload
    emitter.instruction("movsd QWORD PTR [rbp - 32], xmm0");                    // save the float across the spaces call
    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 32]");                    // reload the float
    emitter.instruction("call __rt_var_dump_emit_float_line");                  // call runtime helper
    emitter.instruction("jmp __rt_vd_arr_mixed_next_x86");                      // continue at target label

    emitter.label("__rt_vd_arr_mixed_bool_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the Mixed cell pointer
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the bool payload
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the bool payload across the spaces call
    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the bool payload
    emitter.instruction("call __rt_var_dump_emit_bool_line");                   // call runtime helper

    emitter.label("__rt_vd_arr_mixed_next_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("add r11, 1");                                          // advance runtime pointer or counter
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // store runtime value
    emitter.instruction("jmp __rt_vd_arr_mixed_loop_x86");                      // continue at target label

    emitter.label("__rt_vd_arr_mixed_done_x86");
    emitter.instruction("add rsp, 32");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.label("__rt_vd_arr_mixed_skip_x86");                                // wrong stamp → return without any body
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump array bool.
fn emit_var_dump_array_bool_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_bool ---");
    emitter.label_global("__rt_var_dump_array_bool");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 24");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // store runtime value

    emitter.label("__rt_vd_arr_bool_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // prepare SysV call argument
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // move runtime value between registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("cmp r11, r10");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_vd_arr_bool_done_x86");                       // branch when comparison is at least target

    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, r11");                                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // call runtime helper

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // prepare SysV call argument
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("mov r12, r11");                                        // move runtime value between registers
    emitter.instruction("add r12, 3");                                          // advance runtime pointer or counter
    emitter.instruction("mov rax, QWORD PTR [r9 + r12 * 8]");                   // load element[index] (0 or 1)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the bool value across the spaces call
    emitter.instruction("mov edi, 2");                                          // top-level entry indent = 2 spaces
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the bool value
    emitter.instruction("call __rt_var_dump_emit_bool_line");                   // call runtime helper

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("add r11, 1");                                          // advance runtime pointer or counter
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // store runtime value
    emitter.instruction("jmp __rt_vd_arr_bool_loop_x86");                       // continue at target label

    emitter.label("__rt_vd_arr_bool_done_x86");
    emitter.instruction("add rsp, 24");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_string_key`: emit `["KEY"]=>\n` for a string hash key.
/// The caller writes the entry indent via `__rt_var_dump_spaces` first.
/// Input: AArch64 x1=ptr x2=len / x86_64 rdi=ptr rsi=len.
pub fn emit_var_dump_emit_string_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_emit_string_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_string_key ---");
    emitter.label_global("__rt_var_dump_emit_string_key");

    emitter.instruction("sub sp, sp, #32");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save key ptr/len across the writes

    // Emit `["`
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_str_key_open");
    emitter.instruction("mov x2, #2");                                          // len("[\"") = 2
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);

    // Write the raw key bytes
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload key ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload key len
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);

    // Emit `"]=>\n`
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_str_key_close");
    emitter.instruction("mov x2, #5");                                          // len("\"]=>\n") = 5
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 runtime helper for var_dump string-key formatting.
fn emit_var_dump_emit_string_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_string_key ---");
    emitter.label_global("__rt_var_dump_emit_string_key");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save key ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save key len

    abi::emit_symbol_address(emitter, "rsi", "_vd_str_key_open");               // load runtime data address
    emitter.instruction("mov edx, 2");                                          // len("[\"") = 2
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // invoke kernel service

    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload key ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload key len
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // invoke kernel service

    abi::emit_symbol_address(emitter, "rsi", "_vd_str_key_close");              // load runtime data address
    emitter.instruction("mov edx, 5");                                          // len("\"]=>\n") = 5
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // invoke kernel service

    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_hash`: walk an associative array (hash) and emit one
/// `  [KEY]=>\n  TYPE(VAL)\n` block per entry, matching PHP's var_dump body.
/// Integer keys render as `[N]`, string keys as `["KEY"]`. Scalar values
/// (int/string/float/bool) and null are formatted in full; boxed Mixed cells
/// are unboxed and their scalar payload formatted. Nested arrays/objects fall
/// back to `NULL` (the same limitation as the indexed Mixed walker).
/// Input: AArch64 x0 / x86_64 rdi = hash table pointer.
pub fn emit_var_dump_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_hash_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_hash ---");
    emitter.label_global("__rt_var_dump_hash");

    // Frame (112 bytes): [0]=hash ptr, [8]=cursor, [16]=count, [24]=items,
    //   [32]=key_ptr, [40]=key_len, [48]=val_lo, [56]=val_hi, [64]=val_tag,
    //   [72]=scratch0, [80]=scratch1, [88]=entry_indent, [96]=x29, [104]=x30.
    emitter.instruction("sub sp, sp, #112");                                    // allocate the hash-walk frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the hash table pointer
    emitter.instruction("add x9, x1, #2");                                      // entry indent = indent + 2
    emitter.instruction("str x9, [sp, #88]");                                   // save the entry indent
    emitter.instruction("bl __rt_hash_count");                                  // x0 = number of entries
    emitter.instruction("str x0, [sp, #16]");                                   // save the entry count
    emitter.instruction("str xzr, [sp, #8]");                                   // iterator cursor = 0
    emitter.instruction("str xzr, [sp, #24]");                                  // items emitted = 0

    emitter.label("__rt_vd_hash_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload items emitted
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload entry count
    emitter.instruction("cmp x9, x10");                                         // processed every entry?
    emitter.instruction("b.ge __rt_vd_hash_done");                              // walk complete

    emitter.instruction("ldr x0, [sp, #0]");                                    // reload hash pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // x0=next cursor, x1=key ptr, x2=key len, x3=val_lo, x4=val_hi, x5=val_tag
    emitter.instruction("str x0, [sp, #8]");                                    // save the next iterator cursor
    emitter.instruction("str x1, [sp, #32]");                                   // save key ptr (or integer payload)
    emitter.instruction("str x2, [sp, #40]");                                   // save key len (-1 sentinel for integer keys)
    emitter.instruction("str x3, [sp, #48]");                                   // save value low payload word
    emitter.instruction("str x4, [sp, #56]");                                   // save value high payload word
    emitter.instruction("str x5, [sp, #64]");                                   // save value runtime tag

    // -- emit the key prefix --
    emitter.instruction("ldr x0, [sp, #88]");                                   // entry indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload key len
    emitter.instruction("cmn x2, #1");                                          // integer key? (len == -1)
    emitter.instruction("b.eq __rt_vd_hash_int_key");                           // format integer keys as [N]
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload key ptr
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload key len
    emitter.instruction("bl __rt_var_dump_emit_string_key");                    // emit `["KEY"]=>\n`
    emitter.instruction("b __rt_vd_hash_after_key");                            // continue to the value line
    emitter.label("__rt_vd_hash_int_key");
    emitter.instruction("ldr x11, [sp, #32]");                                  // integer key payload → indexed-key helper's x11 input
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // emit `[N]=>\n`

    emitter.label("__rt_vd_hash_after_key");
    // -- dispatch the value on its runtime tag; unbox boxed Mixed cells first --
    emitter.instruction("ldr x12, [sp, #64]");                                  // reload value tag
    emitter.instruction("cmp x12, #7");                                         // boxed Mixed cell?
    emitter.instruction("b.ne __rt_vd_hash_dispatch");                          // concrete tags dispatch directly
    emitter.instruction("ldr x0, [sp, #48]");                                   // boxed Mixed cell pointer
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=inner tag, x1=payload lo, x2=payload hi
    emitter.instruction("str x0, [sp, #64]");                                   // replace the tag with the unboxed inner tag
    emitter.instruction("str x1, [sp, #48]");                                   // store the unboxed payload low word
    emitter.instruction("str x2, [sp, #56]");                                   // store the unboxed payload high word

    emitter.label("__rt_vd_hash_dispatch");
    emitter.instruction("ldr x12, [sp, #64]");                                  // reload the (possibly unboxed) value tag
    emitter.instruction("cmp x12, #0");                                         // tag 0 = int
    emitter.instruction("b.eq __rt_vd_hash_v_int");                             // format integer values
    emitter.instruction("cmp x12, #1");                                         // tag 1 = string
    emitter.instruction("b.eq __rt_vd_hash_v_str");                             // format string values
    emitter.instruction("cmp x12, #2");                                         // tag 2 = float
    emitter.instruction("b.eq __rt_vd_hash_v_flt");                             // format float values
    emitter.instruction("cmp x12, #3");                                         // tag 3 = bool
    emitter.instruction("b.eq __rt_vd_hash_v_bool");                            // format bool values
    emitter.instruction("cmp x12, #4");                                         // tag 4 = indexed array
    emitter.instruction("b.eq __rt_vd_hash_v_arr");                             // recurse into the value renderer
    emitter.instruction("cmp x12, #5");                                         // tag 5 = hash
    emitter.instruction("b.eq __rt_vd_hash_v_arr");                             // recurse into the value renderer
    emitter.instruction("ldr x0, [sp, #88]");                                   // entry indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("bl __rt_var_dump_emit_null_line");                     // tags 6/8 (object/null) → NULL line
    emitter.instruction("b __rt_vd_hash_next");                                 // advance to the next entry

    emitter.label("__rt_vd_hash_v_int");
    emitter.instruction("ldr x9, [sp, #48]");                                   // load the integer payload
    emitter.instruction("str x9, [sp, #72]");                                   // save it across the spaces call
    emitter.instruction("ldr x0, [sp, #88]");                                   // entry indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr x0, [sp, #72]");                                   // reload the integer payload
    emitter.instruction("bl __rt_var_dump_emit_int_line");                      // emit `int(VAL)\n`
    emitter.instruction("b __rt_vd_hash_next");                                 // advance to the next entry

    emitter.label("__rt_vd_hash_v_str");
    emitter.instruction("ldr x1, [sp, #48]");                                   // load the string pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // load the string length
    emitter.instruction("stp x1, x2, [sp, #72]");                               // save ptr/len across the spaces call
    emitter.instruction("ldr x0, [sp, #88]");                                   // entry indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr x1, [sp, #72]");                                   // reload the string pointer
    emitter.instruction("ldr x2, [sp, #80]");                                   // reload the string length
    emitter.instruction("bl __rt_var_dump_emit_string_line");                   // emit `string(LEN) "VAL"\n`
    emitter.instruction("b __rt_vd_hash_next");                                 // advance to the next entry

    emitter.label("__rt_vd_hash_v_flt");
    emitter.instruction("ldr d0, [sp, #48]");                                   // load the float bit pattern
    emitter.instruction("str d0, [sp, #72]");                                   // save the float across the spaces call
    emitter.instruction("ldr x0, [sp, #88]");                                   // entry indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr d0, [sp, #72]");                                   // reload the float
    emitter.instruction("bl __rt_var_dump_emit_float_line");                    // emit `float(VAL)\n`
    emitter.instruction("b __rt_vd_hash_next");                                 // advance to the next entry

    emitter.label("__rt_vd_hash_v_bool");
    emitter.instruction("ldr x9, [sp, #48]");                                   // load the bool payload (0 or 1)
    emitter.instruction("str x9, [sp, #72]");                                   // save the bool payload across the spaces call
    emitter.instruction("ldr x0, [sp, #88]");                                   // entry indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr x0, [sp, #72]");                                   // reload the bool payload
    emitter.instruction("bl __rt_var_dump_emit_bool_line");                     // emit `bool(true|false)\n`
    emitter.instruction("b __rt_vd_hash_next");                                 // advance to the next entry

    emitter.label("__rt_vd_hash_v_arr");
    emitter.instruction("ldr x0, [sp, #64]");                                   // reload the value tag
    emitter.instruction("ldr x1, [sp, #48]");                                   // reload the value low (array/hash pointer)
    emitter.instruction("mov x2, #0");                                          // high word unused for containers
    emitter.instruction("ldr x3, [sp, #88]");                                   // entry indent for the nested container
    emitter.instruction("bl __rt_var_dump_value");                              // recurse into the value renderer

    emitter.label("__rt_vd_hash_next");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload items emitted
    emitter.instruction("add x9, x9, #1");                                      // count this entry
    emitter.instruction("str x9, [sp, #24]");                                   // save the updated item count
    emitter.instruction("b __rt_vd_hash_loop");                                 // continue with the next entry

    emitter.label("__rt_vd_hash_done");
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the hash-walk frame
    emitter.instruction("ret");                                                 // return to the var_dump caller
}

/// Emits the Linux x86_64 runtime helper for walking an associative array in var_dump.
fn emit_var_dump_hash_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_hash ---");
    emitter.label_global("__rt_var_dump_hash");

    // rbp-relative frame: [-8]=hash ptr, [-16]=cursor, [-24]=count, [-32]=items,
    //   [-40]=key_ptr, [-48]=key_len, [-56]=val_lo, [-64]=val_hi, [-72]=val_tag,
    //   [-80]=scratch0, [-88]=scratch1, [-96]=entry_indent.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 128");                                        // allocate the hash-walk frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash table pointer
    emitter.instruction("mov rax, rsi");                                        // copy the indent
    emitter.instruction("add rax, 2");                                          // entry indent = indent + 2
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // save the entry indent
    emitter.instruction("call __rt_hash_count");                                // rax = number of entries (hash ptr already in rdi)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the entry count
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // iterator cursor = 0
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // items emitted = 0

    emitter.label("__rt_vd_hash_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload items emitted
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload entry count
    emitter.instruction("cmp r10, r11");                                        // processed every entry?
    emitter.instruction("jge __rt_vd_hash_done_x86");                           // walk complete

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // rax=next cursor, rdi=key ptr, rdx=key len, rcx=val_lo, r8=val_hi, r9=val_tag
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the next iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save key ptr (or integer payload)
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save key len (-1 sentinel for integer keys)
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save value low payload word
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // save value high payload word
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // save value runtime tag

    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // entry indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload key len
    emitter.instruction("cmp rdx, -1");                                         // integer key?
    emitter.instruction("je __rt_vd_hash_int_key_x86");                         // format integer keys as [N]
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload key ptr → string-key helper's rdi
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload key len → string-key helper's rsi
    emitter.instruction("call __rt_var_dump_emit_string_key");                  // emit `["KEY"]=>\n`
    emitter.instruction("jmp __rt_vd_hash_after_key_x86");                      // continue to the value line
    emitter.label("__rt_vd_hash_int_key_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // integer key payload → indexed-key helper's rdi
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // emit `[N]=>\n`

    emitter.label("__rt_vd_hash_after_key_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload value tag
    emitter.instruction("cmp r10, 7");                                          // boxed Mixed cell?
    emitter.instruction("jne __rt_vd_hash_dispatch_x86");                       // concrete tags dispatch directly
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // boxed Mixed cell pointer → RAX for __rt_mixed_unbox
    emitter.instruction("call __rt_mixed_unbox");                               // rax=inner tag, rdi=payload lo, rdx=payload hi
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // replace the tag with the unboxed inner tag
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // store the unboxed payload low word
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // store the unboxed payload high word

    emitter.label("__rt_vd_hash_dispatch_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the (possibly unboxed) value tag
    emitter.instruction("cmp r10, 0");                                          // tag 0 = int
    emitter.instruction("je __rt_vd_hash_v_int_x86");                           // format integer values
    emitter.instruction("cmp r10, 1");                                          // tag 1 = string
    emitter.instruction("je __rt_vd_hash_v_str_x86");                           // format string values
    emitter.instruction("cmp r10, 2");                                          // tag 2 = float
    emitter.instruction("je __rt_vd_hash_v_flt_x86");                           // format float values
    emitter.instruction("cmp r10, 3");                                          // tag 3 = bool
    emitter.instruction("je __rt_vd_hash_v_bool_x86");                          // format bool values
    emitter.instruction("cmp r10, 4");                                          // tag 4 = indexed array
    emitter.instruction("je __rt_vd_hash_v_arr_x86");                           // recurse into the value renderer
    emitter.instruction("cmp r10, 5");                                          // tag 5 = hash
    emitter.instruction("je __rt_vd_hash_v_arr_x86");                           // recurse into the value renderer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // entry indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("call __rt_var_dump_emit_null_line");                   // tags 6/8 (object/null) → NULL line
    emitter.instruction("jmp __rt_vd_hash_next_x86");                           // advance to the next entry

    emitter.label("__rt_vd_hash_v_int_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // load the integer payload
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save it across the spaces call
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // entry indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // reload the integer payload
    emitter.instruction("call __rt_var_dump_emit_int_line");                    // emit `int(VAL)\n`
    emitter.instruction("jmp __rt_vd_hash_next_x86");                           // advance to the next entry

    emitter.label("__rt_vd_hash_v_str_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // load the string pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // load the string length
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the string ptr
    emitter.instruction("mov QWORD PTR [rbp - 88], rcx");                       // save the string len
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // entry indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // reload the string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 88]");                       // reload the string length
    emitter.instruction("call __rt_var_dump_emit_string_line");                 // emit `string(LEN) "VAL"\n`
    emitter.instruction("jmp __rt_vd_hash_next_x86");                           // advance to the next entry

    emitter.label("__rt_vd_hash_v_flt_x86");
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 56]");                    // load the float bit pattern
    emitter.instruction("movsd QWORD PTR [rbp - 80], xmm0");                    // save the float across the spaces call
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // entry indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 80]");                    // reload the float
    emitter.instruction("call __rt_var_dump_emit_float_line");                  // emit `float(VAL)\n`
    emitter.instruction("jmp __rt_vd_hash_next_x86");                           // advance to the next entry

    emitter.label("__rt_vd_hash_v_bool_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // load the bool payload (0 or 1)
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the bool payload across the spaces call
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // entry indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // reload the bool payload
    emitter.instruction("call __rt_var_dump_emit_bool_line");                   // emit `bool(true|false)\n`
    emitter.instruction("jmp __rt_vd_hash_next_x86");                           // advance to the next entry

    emitter.label("__rt_vd_hash_v_arr_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // reload the value tag
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the value low (array/hash pointer)
    emitter.instruction("mov edx, 0");                                          // high word unused for containers
    emitter.instruction("mov rcx, QWORD PTR [rbp - 96]");                       // entry indent for the nested container
    emitter.instruction("call __rt_var_dump_value");                            // recurse into the value renderer

    emitter.label("__rt_vd_hash_next_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload items emitted
    emitter.instruction("add r10, 1");                                          // count this entry
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the updated item count
    emitter.instruction("jmp __rt_vd_hash_loop_x86");                           // continue with the next entry

    emitter.label("__rt_vd_hash_done_x86");
    emitter.instruction("add rsp, 128");                                        // release the hash-walk frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the var_dump caller
}

/// `__rt_var_dump_spaces`: write `n` ASCII spaces to stdout in <=64-byte chunks.
/// Input: AArch64 x0 / x86_64 rdi = space count.
pub fn emit_var_dump_spaces(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_spaces_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_spaces ---");
    emitter.label_global("__rt_var_dump_spaces");

    emitter.instruction("sub sp, sp, #32");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // remaining space count

    emitter.label("__rt_vd_spaces_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the remaining count
    emitter.instruction("cmp x0, #0");                                          // any spaces left to write?
    emitter.instruction("b.le __rt_vd_spaces_done");                            // none → finish
    emitter.instruction("mov x9, #64");                                         // the pad buffer is 64 bytes wide
    emitter.instruction("cmp x0, x9");                                          // remaining vs the chunk cap
    emitter.instruction("csel x2, x0, x9, lt");                                 // chunk len = min(remaining, 64)
    emitter.instruction("sub x0, x0, x2");                                      // remaining -= chunk
    emitter.instruction("str x0, [sp, #0]");                                    // save the decremented count
    abi::emit_symbol_address(emitter, "x1", "_vd_spaces");                      // buffer = the 64-space pad
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write the space chunk
    emitter.instruction("b __rt_vd_spaces_loop");                               // continue padding

    emitter.label("__rt_vd_spaces_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 helper that writes `n` spaces in <=64-byte chunks.
fn emit_var_dump_spaces_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_spaces ---");
    emitter.label_global("__rt_var_dump_spaces");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // remaining space count

    emitter.label("__rt_vd_spaces_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the remaining count
    emitter.instruction("cmp rax, 0");                                          // any spaces left to write?
    emitter.instruction("jle __rt_vd_spaces_done_x86");                         // none → finish
    emitter.instruction("mov rdx, 64");                                         // the pad buffer is 64 bytes wide
    emitter.instruction("cmp rax, 64");                                         // remaining vs the chunk cap
    emitter.instruction("cmovl rdx, rax");                                      // chunk len = min(remaining, 64)
    emitter.instruction("sub rax, rdx");                                        // remaining -= chunk
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the decremented count
    abi::emit_symbol_address(emitter, "rsi", "_vd_spaces");                     // buffer = the 64-space pad
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write the space chunk
    emitter.instruction("jmp __rt_vd_spaces_loop_x86");                         // continue padding

    emitter.label("__rt_vd_spaces_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_value`: render one PHP value with PHP `var_dump` formatting
/// and `indent`-space indentation. Tags 4/5 recurse into
/// `__rt_var_dump_indexed` / `__rt_var_dump_hash` with `entry_indent = indent
/// + 2`, tag 7 unboxes a Mixed cell and redispatches, tag 6 (object) and tag
/// 8 (null) emit `NULL` (object is a documented limitation). A depth cap
/// (`indent > 128`) stops cyclic arrays from overflowing the stack.
/// Input: AArch64 x0=tag x1=lo x2=hi x3=indent /
///        x86_64 rdi=tag rsi=lo rdx=hi rcx=indent.
pub fn emit_var_dump_value(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_value_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_value ---");
    emitter.label_global("__rt_var_dump_value");

    // Frame (48 bytes): [0]=lo [8]=hi [16]=indent [32]=x29 [40]=x30.
    emitter.instruction("sub sp, sp, #48");                                     // allocate the value frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the value frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the value low word
    emitter.instruction("str x2, [sp, #8]");                                    // save the value high word
    emitter.instruction("str x3, [sp, #16]");                                   // save the indent

    // -- depth cap: indent > 128 → render NULL to avoid stack overflow on cycles --
    emitter.instruction("cmp x3, #128");                                        // depth beyond the cap?
    emitter.instruction("b.hi __rt_vd_val_null");                               // too deep → render NULL

    emitter.instruction("cmp x0, #7");                                          // boxed Mixed cell?
    emitter.instruction("b.eq __rt_vd_val_mixed");                              // unbox then redispatch
    emitter.instruction("cmp x0, #0");                                          // tag 0 = int
    emitter.instruction("b.eq __rt_vd_val_int");                                // render the integer
    emitter.instruction("cmp x0, #1");                                          // tag 1 = string
    emitter.instruction("b.eq __rt_vd_val_str");                                // render the string
    emitter.instruction("cmp x0, #2");                                          // tag 2 = float
    emitter.instruction("b.eq __rt_vd_val_flt");                                // render the float
    emitter.instruction("cmp x0, #3");                                          // tag 3 = bool
    emitter.instruction("b.eq __rt_vd_val_bool");                               // render the bool
    emitter.instruction("cmp x0, #4");                                          // tag 4 = indexed array
    emitter.instruction("b.eq __rt_vd_val_arr");                                // recurse into the indexed walker
    emitter.instruction("cmp x0, #5");                                          // tag 5 = hash
    emitter.instruction("b.eq __rt_vd_val_hash");                               // recurse into the hash walker
    emitter.instruction("b __rt_vd_val_null");                                  // tag 6 object / 8 null → NULL line

    emitter.label("__rt_vd_val_int");
    emitter.instruction("ldr x0, [sp, #16]");                                   // indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the indent
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the integer payload
    emitter.instruction("bl __rt_var_dump_emit_int_line");                      // emit `int(VAL)\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_str");
    emitter.instruction("ldr x0, [sp, #16]");                                   // indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the indent
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the string ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the string len
    emitter.instruction("bl __rt_var_dump_emit_string_line");                   // emit `string(LEN) "VAL"\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_flt");
    emitter.instruction("ldr x0, [sp, #16]");                                   // indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the indent
    emitter.instruction("ldr d0, [sp, #0]");                                    // reload the float bit pattern
    emitter.instruction("bl __rt_var_dump_emit_float_line");                    // emit `float(VAL)\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_bool");
    emitter.instruction("ldr x0, [sp, #16]");                                   // indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the indent
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the bool payload
    emitter.instruction("bl __rt_var_dump_emit_bool_line");                     // emit `bool(true|false)\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_arr");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the indexed-array pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the indent → entry base
    emitter.instruction("bl __rt_var_dump_indexed");                            // recurse into the indexed walker (writes its own header indent)
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_hash");
    // -- emit `<indent>array(N) {\n` header --
    emitter.instruction("ldr x0, [sp, #16]");                                   // indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the indent for the array header
    abi::emit_symbol_address(emitter, "x1", "_vd_array_open");                  // load the `array(` literal
    emitter.instruction("mov x2, #6");                                          // len("array(") = 6
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `array(`
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash pointer
    emitter.instruction("bl __rt_hash_count");                                  // x0 = number of entries
    emitter.instruction("bl __rt_itoa");                                        // x1=digits ptr, x2=digits len
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write the count digits
    abi::emit_symbol_address(emitter, "x1", "_vd_array_close_brace");           // load the `) {\n` literal
    emitter.instruction("mov x2, #4");                                          // len(") {\n") = 4
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `) {\n`
    // -- emit the hash entries (body) --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the indent → entry base
    emitter.instruction("bl __rt_var_dump_hash");                               // walk the hash entries
    // -- emit `<indent>}\n` footer --
    emitter.instruction("ldr x0, [sp, #16]");                                   // indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the indent for the closing brace
    abi::emit_symbol_address(emitter, "x1", "_vd_close_brace_nl");              // load the `}\n` literal
    emitter.instruction("mov x2, #2");                                          // len("}\n") = 2
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `}\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_mixed");
    emitter.instruction("ldr x0, [sp, #0]");                                    // boxed Mixed cell pointer
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=inner tag, x1=lo, x2=hi
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the indent
    emitter.instruction("bl __rt_var_dump_value");                              // redispatch the unboxed value
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_null");
    emitter.instruction("ldr x0, [sp, #16]");                                   // indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the indent
    emitter.instruction("bl __rt_var_dump_emit_null_line");                     // emit `NULL\n`

    emitter.label("__rt_vd_val_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the value frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 single-value renderer for var_dump.
fn emit_var_dump_value_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_value ---");
    emitter.label_global("__rt_var_dump_value");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the value frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate the value frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the value low word
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the value high word
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the indent
    emitter.instruction("mov rax, rdi");                                        // tag → dispatch register

    // -- depth cap: indent > 128 → render NULL to avoid stack overflow on cycles --
    emitter.instruction("cmp rcx, 128");                                        // depth beyond the cap?
    emitter.instruction("ja __rt_vd_val_null_x86");                             // too deep → render NULL

    emitter.instruction("cmp rax, 7");                                          // boxed Mixed cell?
    emitter.instruction("je __rt_vd_val_mixed_x86");                            // unbox then redispatch
    emitter.instruction("cmp rax, 0");                                          // tag 0 = int
    emitter.instruction("je __rt_vd_val_int_x86");                              // render the integer
    emitter.instruction("cmp rax, 1");                                          // tag 1 = string
    emitter.instruction("je __rt_vd_val_str_x86");                              // render the string
    emitter.instruction("cmp rax, 2");                                          // tag 2 = float
    emitter.instruction("je __rt_vd_val_flt_x86");                              // render the float
    emitter.instruction("cmp rax, 3");                                          // tag 3 = bool
    emitter.instruction("je __rt_vd_val_bool_x86");                             // render the bool
    emitter.instruction("cmp rax, 4");                                          // tag 4 = indexed array
    emitter.instruction("je __rt_vd_val_arr_x86");                              // recurse into the indexed walker
    emitter.instruction("cmp rax, 5");                                          // tag 5 = hash
    emitter.instruction("je __rt_vd_val_hash_x86");                             // recurse into the hash walker
    emitter.instruction("jmp __rt_vd_val_null_x86");                            // tag 6 object / 8 null → NULL line

    emitter.label("__rt_vd_val_int_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the integer payload
    emitter.instruction("call __rt_var_dump_emit_int_line");                    // emit `int(VAL)\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_str_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the string ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the string len
    emitter.instruction("call __rt_var_dump_emit_string_line");                 // emit `string(LEN) "VAL"\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_flt_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the indent
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // reload the float bit pattern
    emitter.instruction("call __rt_var_dump_emit_float_line");                  // emit `float(VAL)\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_bool_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the bool payload
    emitter.instruction("call __rt_var_dump_emit_bool_line");                   // emit `bool(true|false)\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_arr_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the indent → entry base
    emitter.instruction("call __rt_var_dump_indexed");                          // recurse into the indexed walker (writes its own header indent)
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_hash_x86");
    // -- emit `<indent>array(N) {\n` header --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the indent for the array header
    abi::emit_symbol_address(emitter, "rsi", "_vd_array_open");                 // load the `array(` literal
    emitter.instruction("mov edx, 6");                                          // len("array(") = 6
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `array(`
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash pointer
    emitter.instruction("call __rt_hash_count");                                // rax = number of entries
    emitter.instruction("call __rt_itoa");                                      // rax=digits ptr, rdx=digits len
    emitter.instruction("mov rsi, rax");                                        // digits ptr → write buffer
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write the count digits
    abi::emit_symbol_address(emitter, "rsi", "_vd_array_close_brace");          // load the `) {\n` literal
    emitter.instruction("mov edx, 4");                                          // len(") {\n") = 4
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `) {\n`
    // -- emit the hash entries (body) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the indent → entry base
    emitter.instruction("call __rt_var_dump_hash");                             // walk the hash entries
    // -- emit `<indent>}\n` footer --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the indent for the closing brace
    abi::emit_symbol_address(emitter, "rsi", "_vd_close_brace_nl");             // load the `}\n` literal
    emitter.instruction("mov edx, 2");                                          // len("}\n") = 2
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `}\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_mixed_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // boxed Mixed cell pointer → RAX
    emitter.instruction("call __rt_mixed_unbox");                               // rax=inner tag, rdi=lo, rdx=hi
    emitter.instruction("mov rsi, rdi");                                        // unboxed lo → value low argument
    emitter.instruction("mov rdi, rax");                                        // unboxed tag → value tag argument
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the indent
    emitter.instruction("call __rt_var_dump_value");                            // redispatch the unboxed value
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_null_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the indent
    emitter.instruction("call __rt_var_dump_emit_null_line");                   // emit `NULL\n`

    emitter.label("__rt_vd_val_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the value frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_indexed`: render an indexed array body `array(N) {\n ...
/// }\n` for the recursive value renderer, emitting the `array(N) {` header and
/// closing `}` at `indent` and each entry's key/value at `indent + 2`. The
/// array self-dispatches each element on its value_type stamp and recurses
/// through `__rt_var_dump_value` so nested containers render fully.
/// Input: AArch64 x0=arr x1=indent / x86_64 rdi=arr rsi=indent.
pub fn emit_var_dump_indexed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_indexed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_indexed ---");
    emitter.label_global("__rt_var_dump_indexed");

    // Frame (64 bytes): [0]arr [8]indent [16]entry_indent [24]count
    //   [32]index [40]stamp [48]x29 [56]x30.
    emitter.instruction("sub sp, sp, #64");                                     // allocate the indexed-walk frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the walk frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the indent
    emitter.instruction("add x9, x1, #2");                                      // entry indent = indent + 2
    emitter.instruction("str x9, [sp, #16]");                                   // save the entry indent
    emitter.instruction("ldr x10, [x0]");                                       // load the element count from the header
    emitter.instruction("str x10, [sp, #24]");                                  // save the element count
    emitter.instruction("str xzr, [sp, #32]");                                  // index = 0
    emitter.instruction("ldr x11, [x0, #-8]");                                  // load the packed array kind word
    emitter.instruction("lsr x11, x11, #8");                                    // shift the value_type stamp into the low byte
    emitter.instruction("and x11, x11, #0x0f");                                 // isolate the value_type field (low nibble), dropping the COW bit
    emitter.instruction("str x11, [sp, #40]");                                  // save the element value_type stamp

    // -- emit `<indent>array(N) {\n` --
    emitter.instruction("ldr x0, [sp, #8]");                                    // indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the indent for the header
    abi::emit_symbol_address(emitter, "x1", "_vd_array_open");                  // load the `array(` literal
    emitter.instruction("mov x2, #6");                                          // len("array(") = 6
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `array(`
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the element count
    emitter.instruction("bl __rt_itoa");                                        // x1=digits ptr, x2=digits len
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write the count digits
    abi::emit_symbol_address(emitter, "x1", "_vd_array_close_brace");           // load the `) {\n` literal
    emitter.instruction("mov x2, #4");                                          // len(") {\n") = 4
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `) {\n`

    emitter.label("__rt_vd_idx_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the current index
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the element count
    emitter.instruction("cmp x9, x10");                                         // processed every element?
    emitter.instruction("b.ge __rt_vd_idx_done");                               // walk complete

    // -- emit `<entry_indent>[i]=>\n` --
    emitter.instruction("ldr x0, [sp, #16]");                                   // entry indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the entry indent
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the current index → key helper's x11
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // write `[i]=>\n`

    // -- render the element via __rt_var_dump_value --
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the element stamp
    emitter.instruction("ldr x13, [sp, #0]");                                   // reload the array pointer
    emitter.instruction("ldr x14, [sp, #32]");                                  // reload the current index
    emitter.instruction("cmp x12, #1");                                         // string elements use a 16-byte stride
    emitter.instruction("b.eq __rt_vd_idx_str");                                // handle string elements
    emitter.instruction("cmp x12, #7");                                         // mixed elements are boxed cells
    emitter.instruction("b.eq __rt_vd_idx_mixed");                              // handle mixed cells

    // 8-byte-stride elements: int(0) / float(2) / bool(3) / array(4) / hash(5) / object(6).
    emitter.instruction("add x15, x14, #3");                                    // skip the 24-byte (3-quad) header
    emitter.instruction("ldr x1, [x13, x15, lsl #3]");                          // load the raw element word → value low
    emitter.instruction("mov x0, x12");                                         // tag = the array stamp
    emitter.instruction("mov x2, #0");                                          // high word unused for 8-byte elements
    emitter.instruction("ldr x3, [sp, #16]");                                   // entry indent → value renderer indent
    emitter.instruction("bl __rt_var_dump_value");                              // render the element
    emitter.instruction("b __rt_vd_idx_next");                                  // advance to the next element

    emitter.label("__rt_vd_idx_str");
    emitter.instruction("lsl x15, x14, #4");                                    // index * 16
    emitter.instruction("add x15, x15, #24");                                   // element base offset = 24 + index*16
    emitter.instruction("add x15, x13, x15");                                   // element address
    emitter.instruction("ldr x1, [x15]");                                       // string ptr → value low
    emitter.instruction("ldr x2, [x15, #8]");                                   // string len → value high
    emitter.instruction("mov x0, #1");                                          // tag = string
    emitter.instruction("ldr x3, [sp, #16]");                                   // entry indent → value renderer indent
    emitter.instruction("bl __rt_var_dump_value");                              // render the element
    emitter.instruction("b __rt_vd_idx_next");                                  // advance to the next element

    emitter.label("__rt_vd_idx_mixed");
    emitter.instruction("add x15, x14, #3");                                    // skip the 24-byte (3-quad) header
    emitter.instruction("ldr x15, [x13, x15, lsl #3]");                         // load the Mixed cell pointer
    emitter.instruction("ldr x0, [x15]");                                       // cell tag → value tag
    emitter.instruction("ldr x1, [x15, #8]");                                   // cell low word → value low
    emitter.instruction("ldr x2, [x15, #16]");                                  // cell high word → value high
    emitter.instruction("ldr x3, [sp, #16]");                                   // entry indent → value renderer indent
    emitter.instruction("bl __rt_var_dump_value");                              // render the element

    emitter.label("__rt_vd_idx_next");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the index
    emitter.instruction("add x9, x9, #1");                                      // advance the index
    emitter.instruction("str x9, [sp, #32]");                                   // save the updated index
    emitter.instruction("b __rt_vd_idx_loop");                                  // continue scanning

    emitter.label("__rt_vd_idx_done");
    // -- emit `<indent>}\n` --
    emitter.instruction("ldr x0, [sp, #8]");                                    // indent → spaces helper
    emitter.instruction("bl __rt_var_dump_spaces");                             // pad the indent for the closing brace
    abi::emit_symbol_address(emitter, "x1", "_vd_close_brace_nl");              // load the `}\n` literal
    emitter.instruction("mov x2, #2");                                          // len("}\n") = 2
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `}\n`
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the indexed-walk frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 indexed-array recursive walker for var_dump.
fn emit_var_dump_indexed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_indexed ---");
    emitter.label_global("__rt_var_dump_indexed");

    // rbp-relative frame: [-8]arr [-16]indent [-24]entry_indent [-32]count
    //   [-40]index [-48]stamp.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the walk frame pointer
    emitter.instruction("sub rsp, 64");                                         // allocate the indexed-walk frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the indent
    emitter.instruction("mov rax, rsi");                                        // copy the indent
    emitter.instruction("add rax, 2");                                          // entry indent = indent + 2
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the entry indent
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the element count from the header
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the element count
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // index = 0
    emitter.instruction("mov rax, QWORD PTR [rdi - 8]");                        // load the packed array kind word
    emitter.instruction("shr rax, 8");                                          // shift the value_type stamp into the low byte
    emitter.instruction("and rax, 0x0f");                                       // isolate the value_type field (low nibble), dropping the COW bit
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the element value_type stamp

    // -- emit `<indent>array(N) {\n` --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the indent for the header
    abi::emit_symbol_address(emitter, "rsi", "_vd_array_open");                 // load the `array(` literal
    emitter.instruction("mov edx, 6");                                          // len("array(") = 6
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `array(`
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the element count
    emitter.instruction("call __rt_itoa");                                      // rax=digits ptr, rdx=digits len
    emitter.instruction("mov rsi, rax");                                        // digits ptr → write buffer
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write the count digits
    abi::emit_symbol_address(emitter, "rsi", "_vd_array_close_brace");          // load the `) {\n` literal
    emitter.instruction("mov edx, 4");                                          // len(") {\n") = 4
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `) {\n`

    emitter.label("__rt_vd_idx_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the current index
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the element count
    emitter.instruction("cmp rax, rcx");                                        // processed every element?
    emitter.instruction("jge __rt_vd_idx_done_x86");                            // walk complete

    // -- emit `<entry_indent>[i]=>\n` --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // entry indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the entry indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the current index → key helper's rdi
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // write `[i]=>\n`

    // -- render the element via __rt_var_dump_value --
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the element stamp
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the current index
    emitter.instruction("cmp r10, 1");                                          // string elements use a 16-byte stride
    emitter.instruction("je __rt_vd_idx_str_x86");                              // handle string elements
    emitter.instruction("cmp r10, 7");                                          // mixed elements are boxed cells
    emitter.instruction("je __rt_vd_idx_mixed_x86");                            // handle mixed cells

    // 8-byte-stride elements: int(0) / float(2) / bool(3) / array(4) / hash(5) / object(6).
    emitter.instruction("mov rax, r11");                                        // copy the index
    emitter.instruction("add rax, 3");                                          // skip the 24-byte (3-quad) header
    emitter.instruction("mov rsi, QWORD PTR [r9 + rax * 8]");                   // load the raw element word → value low
    emitter.instruction("mov rdi, r10");                                        // tag = the array stamp
    emitter.instruction("mov rdx, 0");                                          // high word unused for 8-byte elements
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // entry indent → value renderer indent
    emitter.instruction("call __rt_var_dump_value");                            // render the element
    emitter.instruction("jmp __rt_vd_idx_next_x86");                            // advance to the next element

    emitter.label("__rt_vd_idx_str_x86");
    emitter.instruction("mov rax, r11");                                        // copy the index
    emitter.instruction("shl rax, 4");                                          // index * 16
    emitter.instruction("add rax, 24");                                         // element base offset = 24 + index*16
    emitter.instruction("add rax, r9");                                         // element address
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // string ptr → value low
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // string len → value high
    emitter.instruction("mov rdi, 1");                                          // tag = string
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // entry indent → value renderer indent
    emitter.instruction("call __rt_var_dump_value");                            // render the element
    emitter.instruction("jmp __rt_vd_idx_next_x86");                            // advance to the next element

    emitter.label("__rt_vd_idx_mixed_x86");
    emitter.instruction("mov rax, r11");                                        // copy the index
    emitter.instruction("add rax, 3");                                          // skip the 24-byte (3-quad) header
    emitter.instruction("mov rax, QWORD PTR [r9 + rax * 8]");                   // load the Mixed cell pointer
    emitter.instruction("mov rdi, QWORD PTR [rax]");                            // cell tag → value tag
    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                        // cell low word → value low
    emitter.instruction("mov rdx, QWORD PTR [rax + 16]");                       // cell high word → value high
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // entry indent → value renderer indent
    emitter.instruction("call __rt_var_dump_value");                            // render the element

    emitter.label("__rt_vd_idx_next_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the index
    emitter.instruction("add rax, 1");                                          // advance the index
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the updated index
    emitter.instruction("jmp __rt_vd_idx_loop_x86");                            // continue scanning

    emitter.label("__rt_vd_idx_done_x86");
    // -- emit `<indent>}\n` --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // indent → spaces helper
    emitter.instruction("call __rt_var_dump_spaces");                           // pad the indent for the closing brace
    abi::emit_symbol_address(emitter, "rsi", "_vd_close_brace_nl");             // load the `}\n` literal
    emitter.instruction("mov edx, 2");                                          // len("}\n") = 2
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `}\n`
    emitter.instruction("add rsp, 64");                                         // release the indexed-walk frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
