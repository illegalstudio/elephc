//! Purpose:
//! Emits the `__rt_var_dump_array_int` / `__rt_var_dump_array_str` runtime
//! walkers that iterate a homogeneous indexed array and emit one
//! `[N]=>\n  TYPE(VAL)\n` block per element (PHP `var_dump` body format,
//! 2-space indentation). The opening `array(N) {\n` and closing `}\n`
//! are emitted by the builtin caller around these walks.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
//! - The `var_dump` builtin emitter when the value's static type is
//!   `Array(Int)` or `Array(Str)`.
//!
//! Key details:
//! - Array layout reused from the existing JSON encoders: 24-byte header
//!   (len at offset 0, value_type at offset 8, refcount at offset 16)
//!   followed by 8-byte elements starting at offset 24.
//! - String elements use the elephc string-result ABI: 16-byte slots
//!   storing (ptr, len) — so element[N] for an indexed string array lives
//!   at offsets `24 + N*16` (ptr) and `32 + N*16` (len).
//! - Associative arrays (hashes) are handled by `__rt_var_dump_hash`, which
//!   iterates entries via `__rt_hash_iter_next` and formats string/integer keys
//!   plus scalar (and boxed-Mixed scalar) values. Nested arrays/objects inside a
//!   hash fall back to `NULL`, matching the indexed Mixed walker's limitation.

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

    // Frame (32 bytes): [0..8] array ptr, [8..16] element index,
    //   [16..24] saved x29, [24..32] saved x30.
    emitter.instruction("sub sp, sp, #32");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
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
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // emits "  [N]=>\n" for x11=index

    // -- emit `  int(VAL)\n` --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload index
    emitter.instruction("add x12, x11, #3");                                    // skip the 24-byte (3 quads) header
    emitter.instruction("ldr x0, [x9, x12, lsl #3]");                           // load element[index]
    emitter.instruction("bl __rt_var_dump_emit_int_line");                      // emits "  int(VAL)\n" for x0=value

    emitter.instruction("ldr x11, [sp, #8]");                                   // reload index
    emitter.instruction("add x11, x11, #1");                                    // advance index
    emitter.instruction("str x11, [sp, #8]");                                   // save updated index
    emitter.instruction("b __rt_vd_arr_int_loop");                              // continue scanning

    emitter.label("__rt_vd_arr_int_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
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
    emitter.instruction("mov rdi, r11");                                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // call runtime helper

    // -- emit `  int(VAL)\n` --
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload index
    emitter.instruction("mov r12, r11");                                        // move runtime value between registers
    emitter.instruction("add r12, 3");                                          // skip 3-quad header
    emitter.instruction("mov rdi, QWORD PTR [r9 + r12 * 8]");                   // load element[index] into the emit helper's first arg
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

    // Frame: same layout as the int walker.
    emitter.instruction("sub sp, sp, #32");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
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
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // emits "  [N]=>\n" for x11=index

    // -- emit `  string(LEN) "VAL"\n` --
    // String elements are 16-byte slots: ptr at offset 24+16*N, len at 32+16*N.
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload index
    emitter.instruction("lsl x12, x11, #4");                                    // index * 16
    emitter.instruction("add x12, x12, #24");                                   // element base offset = 24 + index*16
    emitter.instruction("add x13, x9, x12");                                    // element address
    emitter.instruction("ldr x1, [x13]");                                       // load element string ptr
    emitter.instruction("ldr x2, [x13, #8]");                                   // load element string len
    emitter.instruction("bl __rt_var_dump_emit_string_line");                   // emits `  string(LEN) "VAL"\n`

    emitter.instruction("ldr x11, [sp, #8]");                                   // reload index
    emitter.instruction("add x11, x11, #1");                                    // advance index
    emitter.instruction("str x11, [sp, #8]");                                   // save updated index
    emitter.instruction("b __rt_vd_arr_str_loop");                              // continue scanning

    emitter.label("__rt_vd_arr_str_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump array str.
fn emit_var_dump_array_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_str ---");
    emitter.label_global("__rt_var_dump_array_str");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // index = 0

    emitter.label("__rt_vd_arr_str_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the array pointer
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the element count
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the current index
    emitter.instruction("cmp r11, r10");                                        // processed every element?
    emitter.instruction("jge __rt_vd_arr_str_done_x86");                        // walk complete

    emitter.instruction("mov rdi, r11");                                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // call runtime helper

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload index
    emitter.instruction("mov r12, r11");                                        // move runtime value between registers
    emitter.instruction("shl r12, 4");                                          // index * 16
    emitter.instruction("add r12, 24");                                         // element base offset
    emitter.instruction("add r12, r9");                                         // element address
    emitter.instruction("mov rdi, QWORD PTR [r12]");                            // string ptr → emit helper's first arg
    emitter.instruction("mov rsi, QWORD PTR [r12 + 8]");                        // string len → emit helper's second arg
    emitter.instruction("call __rt_var_dump_emit_string_line");                 // call runtime helper

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("add r11, 1");                                          // advance runtime pointer or counter
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // store runtime value
    emitter.instruction("jmp __rt_vd_arr_str_loop_x86");                        // continue at target label

    emitter.label("__rt_vd_arr_str_done_x86");
    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_indexed_key`: emit `  [N]=>\n` for a numeric index.
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

    // Emit "  ["
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_indent_open");
    emitter.instruction("mov x2, #3");                                          // len("  [") = 3
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

    // Emit "  ["
    abi::emit_symbol_address(emitter, "rsi", "_vd_indent_open");                // load runtime data address
    emitter.instruction("mov edx, 3");                                          // prepare SysV call argument
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

/// `__rt_var_dump_emit_int_line`: emit `  int(VAL)\n` for a single int.
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

    // Emit "  int("
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_int_prefix");
    emitter.instruction("mov x2, #6");                                          // len("  int(") = 6
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
    emitter.instruction("mov edx, 6");                                          // prepare SysV call argument
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

/// `__rt_var_dump_emit_string_line`: emit `  string(LEN) "VAL"\n` for a
/// string. Input: AArch64 x1=ptr x2=len / x86_64 rdi=ptr rsi=len.
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

    // Emit "  string("
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_str_prefix");
    emitter.instruction("mov x2, #9");                                          // len("  string(") = 9
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    // itoa(len)
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload len
    emitter.instruction("bl __rt_itoa");                                        // call runtime helper
    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.syscall(4);

    // Emit ") "
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
    emitter.instruction("mov edx, 9");                                          // prepare SysV call argument
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

/// `__rt_var_dump_emit_bool_line`: emit `  bool(true)\n` or
/// `  bool(false)\n` for a single bool. Input: AArch64 x0 / x86_64 rdi =
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
    emitter.instruction("mov x2, #13");                                         // len("  bool(true)\n") = 13
    emitter.instruction(&format!("b {}", done_label));                          // continue at target label
    emitter.label(false_label);
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_bool_false_line");
    emitter.instruction("mov x2, #14");                                         // len("  bool(false)\n") = 14
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
    emitter.instruction("mov edx, 13");                                         // prepare SysV call argument
    emitter.instruction(&format!("jmp {}", done_label));                        // continue at target label
    emitter.label(false_label);
    abi::emit_symbol_address(emitter, "rsi", "_vd_bool_false_line");            // load runtime data address
    emitter.instruction("mov edx, 14");                                         // prepare SysV call argument
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

    emitter.instruction("sub sp, sp, #32");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // store runtime value
    emitter.instruction("str xzr, [sp, #8]");                                   // store runtime value

    emitter.label("__rt_vd_arr_bool_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // load runtime value
    emitter.instruction("ldr x10, [x9]");                                       // element count
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("cmp x11, x10");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_vd_arr_bool_done");                          // branch when comparison is at least target

    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // call runtime helper

    emitter.instruction("ldr x9, [sp, #0]");                                    // load runtime value
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("add x12, x11, #3");                                    // skip 3-quad header
    emitter.instruction("ldr x0, [x9, x12, lsl #3]");                           // load element[index] (0 or 1)
    emitter.instruction("bl __rt_var_dump_emit_bool_line");                     // call runtime helper

    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("add x11, x11, #1");                                    // advance runtime pointer or counter
    emitter.instruction("str x11, [sp, #8]");                                   // store runtime value
    emitter.instruction("b __rt_vd_arr_bool_loop");                             // continue at target label

    emitter.label("__rt_vd_arr_bool_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_float_line`: emit `  float(VAL)\n` for a single
/// f64. Input: AArch64 d0 / x86_64 xmm0 = value.
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

    // Emit "  float("
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_float_prefix");
    emitter.instruction("mov x2, #8");                                          // len("  float(") = 8
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
    emitter.instruction("mov edx, 8");                                          // prepare SysV call argument
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

    emitter.instruction("sub sp, sp, #32");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // store runtime value
    emitter.instruction("str xzr, [sp, #8]");                                   // store runtime value

    emitter.label("__rt_vd_arr_float_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // load runtime value
    emitter.instruction("ldr x10, [x9]");                                       // load runtime value
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("cmp x11, x10");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_vd_arr_float_done");                         // branch when comparison is at least target

    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // call runtime helper

    emitter.instruction("ldr x9, [sp, #0]");                                    // load runtime value
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("add x12, x11, #3");                                    // skip 3-quad header
    emitter.instruction("ldr d0, [x9, x12, lsl #3]");                           // load f64 element[index]
    emitter.instruction("bl __rt_var_dump_emit_float_line");                    // call runtime helper

    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("add x11, x11, #1");                                    // advance runtime pointer or counter
    emitter.instruction("str x11, [sp, #8]");                                   // store runtime value
    emitter.instruction("b __rt_vd_arr_float_loop");                            // continue at target label

    emitter.label("__rt_vd_arr_float_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump array float.
fn emit_var_dump_array_float_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_float ---");
    emitter.label_global("__rt_var_dump_array_float");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // store runtime value

    emitter.label("__rt_vd_arr_float_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // prepare SysV call argument
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // move runtime value between registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("cmp r11, r10");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_vd_arr_float_done_x86");                      // branch when comparison is at least target

    emitter.instruction("mov rdi, r11");                                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // call runtime helper

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // prepare SysV call argument
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("mov r12, r11");                                        // move runtime value between registers
    emitter.instruction("add r12, 3");                                          // advance runtime pointer or counter
    emitter.instruction("movsd xmm0, QWORD PTR [r9 + r12 * 8]");                // load f64 element[index] into xmm0
    emitter.instruction("call __rt_var_dump_emit_float_line");                  // call runtime helper

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("add r11, 1");                                          // advance runtime pointer or counter
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // store runtime value
    emitter.instruction("jmp __rt_vd_arr_float_loop_x86");                      // continue at target label

    emitter.label("__rt_vd_arr_float_done_x86");
    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_null_line`: emit `  NULL\n` for a null payload.
pub fn emit_var_dump_emit_null_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: var_dump_emit_null_line ---");
        emitter.label_global("__rt_var_dump_emit_null_line");
        abi::emit_symbol_address(emitter, "rsi", "_vd_null_line");              // load runtime data address
        emitter.instruction("mov edx, 7");                                      // len("  NULL\n") = 7
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
    emitter.instruction("mov x2, #7");                                          // len("  NULL\n") = 7
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_array_mixed`: walk an indexed array of Mixed cell
/// pointers and dispatch on each cell's runtime tag. Supports int,
/// string, float, bool payloads; unknown tags (nested arrays/objects)
/// fall back to NULL — full recursive nesting needs the var_dump entry
/// point to drive the walker, not the walker itself.
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

    emitter.instruction("sub sp, sp, #32");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // array ptr
    emitter.instruction("str xzr, [sp, #8]");                                   // index = 0

    emitter.label("__rt_vd_arr_mixed_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // load runtime value
    emitter.instruction("ldr x10, [x9]");                                       // element count
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("cmp x11, x10");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_vd_arr_mixed_done");                         // branch when comparison is at least target

    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // call runtime helper

    emitter.instruction("ldr x9, [sp, #0]");                                    // load runtime value
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("add x12, x11, #3");                                    // skip 3-quad header
    emitter.instruction("ldr x13, [x9, x12, lsl #3]");                          // Mixed cell pointer
    emitter.instruction("ldr x14, [x13]");                                      // runtime value tag at cell[0]
    emitter.instruction("cmp x14, #0");                                         // tag 0 = int
    emitter.instruction("b.eq __rt_vd_arr_mixed_int");                          // branch when the checked value is zero or equal
    emitter.instruction("cmp x14, #1");                                         // tag 1 = string
    emitter.instruction("b.eq __rt_vd_arr_mixed_str");                          // branch when the checked value is zero or equal
    emitter.instruction("cmp x14, #2");                                         // tag 2 = float
    emitter.instruction("b.eq __rt_vd_arr_mixed_flt");                          // branch when the checked value is zero or equal
    emitter.instruction("cmp x14, #3");                                         // tag 3 = bool
    emitter.instruction("b.eq __rt_vd_arr_mixed_bool");                         // branch when the checked value is zero or equal
    emitter.instruction("bl __rt_var_dump_emit_null_line");                     // unsupported tag → NULL fallback
    emitter.instruction("b __rt_vd_arr_mixed_next");                            // continue at target label

    emitter.label("__rt_vd_arr_mixed_int");
    emitter.instruction("ldr x0, [x13, #8]");                                   // load runtime value
    emitter.instruction("bl __rt_var_dump_emit_int_line");                      // call runtime helper
    emitter.instruction("b __rt_vd_arr_mixed_next");                            // continue at target label

    emitter.label("__rt_vd_arr_mixed_str");
    emitter.instruction("ldr x1, [x13, #8]");                                   // string ptr → x1 per elephc string ABI
    emitter.instruction("ldr x2, [x13, #16]");                                  // string len → x2
    emitter.instruction("bl __rt_var_dump_emit_string_line");                   // call runtime helper
    emitter.instruction("b __rt_vd_arr_mixed_next");                            // continue at target label

    emitter.label("__rt_vd_arr_mixed_flt");
    emitter.instruction("ldr d0, [x13, #8]");                                   // load runtime value
    emitter.instruction("bl __rt_var_dump_emit_float_line");                    // call runtime helper
    emitter.instruction("b __rt_vd_arr_mixed_next");                            // continue at target label

    emitter.label("__rt_vd_arr_mixed_bool");
    emitter.instruction("ldr x0, [x13, #8]");                                   // load runtime value
    emitter.instruction("bl __rt_var_dump_emit_bool_line");                     // call runtime helper

    emitter.label("__rt_vd_arr_mixed_next");
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("add x11, x11, #1");                                    // advance runtime pointer or counter
    emitter.instruction("str x11, [sp, #8]");                                   // store runtime value
    emitter.instruction("b __rt_vd_arr_mixed_loop");                            // continue at target label

    emitter.label("__rt_vd_arr_mixed_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release runtime stack frame
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
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // store runtime value

    emitter.label("__rt_vd_arr_mixed_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // prepare SysV call argument
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // move runtime value between registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("cmp r11, r10");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_vd_arr_mixed_done_x86");                      // branch when comparison is at least target

    emitter.instruction("mov rdi, r11");                                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // call runtime helper

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // prepare SysV call argument
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("mov r12, r11");                                        // move runtime value between registers
    emitter.instruction("add r12, 3");                                          // advance runtime pointer or counter
    emitter.instruction("mov r13, QWORD PTR [r9 + r12 * 8]");                   // move runtime value between registers
    emitter.instruction("mov r14, QWORD PTR [r13]");                            // move runtime value between registers

    emitter.instruction("cmp r14, 0");                                          // compare runtime values for the next branch
    emitter.instruction("je __rt_vd_arr_mixed_int_x86");                        // branch when the checked value is zero or equal
    emitter.instruction("cmp r14, 1");                                          // compare runtime values for the next branch
    emitter.instruction("je __rt_vd_arr_mixed_str_x86");                        // branch when the checked value is zero or equal
    emitter.instruction("cmp r14, 2");                                          // compare runtime values for the next branch
    emitter.instruction("je __rt_vd_arr_mixed_flt_x86");                        // branch when the checked value is zero or equal
    emitter.instruction("cmp r14, 3");                                          // compare runtime values for the next branch
    emitter.instruction("je __rt_vd_arr_mixed_bool_x86");                       // branch when the checked value is zero or equal
    emitter.instruction("call __rt_var_dump_emit_null_line");                   // call runtime helper
    emitter.instruction("jmp __rt_vd_arr_mixed_next_x86");                      // continue at target label

    emitter.label("__rt_vd_arr_mixed_int_x86");
    emitter.instruction("mov rdi, QWORD PTR [r13 + 8]");                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_int_line");                    // call runtime helper
    emitter.instruction("jmp __rt_vd_arr_mixed_next_x86");                      // continue at target label

    emitter.label("__rt_vd_arr_mixed_str_x86");
    emitter.instruction("mov rdi, QWORD PTR [r13 + 8]");                        // prepare SysV call argument
    emitter.instruction("mov rsi, QWORD PTR [r13 + 16]");                       // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_string_line");                 // call runtime helper
    emitter.instruction("jmp __rt_vd_arr_mixed_next_x86");                      // continue at target label

    emitter.label("__rt_vd_arr_mixed_flt_x86");
    emitter.instruction("movsd xmm0, QWORD PTR [r13 + 8]");                     // load the mixed float payload into the SysV float argument register
    emitter.instruction("call __rt_var_dump_emit_float_line");                  // call runtime helper
    emitter.instruction("jmp __rt_vd_arr_mixed_next_x86");                      // continue at target label

    emitter.label("__rt_vd_arr_mixed_bool_x86");
    emitter.instruction("mov rdi, QWORD PTR [r13 + 8]");                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_bool_line");                   // call runtime helper

    emitter.label("__rt_vd_arr_mixed_next_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("add r11, 1");                                          // advance runtime pointer or counter
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // store runtime value
    emitter.instruction("jmp __rt_vd_arr_mixed_loop_x86");                      // continue at target label

    emitter.label("__rt_vd_arr_mixed_done_x86");
    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
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
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // store runtime value

    emitter.label("__rt_vd_arr_bool_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // prepare SysV call argument
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // move runtime value between registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("cmp r11, r10");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_vd_arr_bool_done_x86");                       // branch when comparison is at least target

    emitter.instruction("mov rdi, r11");                                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // call runtime helper

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // prepare SysV call argument
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("mov r12, r11");                                        // move runtime value between registers
    emitter.instruction("add r12, 3");                                          // advance runtime pointer or counter
    emitter.instruction("mov rdi, QWORD PTR [r9 + r12 * 8]");                   // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_bool_line");                   // call runtime helper

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // move runtime value between registers
    emitter.instruction("add r11, 1");                                          // advance runtime pointer or counter
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // store runtime value
    emitter.instruction("jmp __rt_vd_arr_bool_loop_x86");                       // continue at target label

    emitter.label("__rt_vd_arr_bool_done_x86");
    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_string_key`: emit `  ["KEY"]=>\n` for a string hash key.
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

    // Emit `  ["`
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_str_key_open");
    emitter.instruction("mov x2, #4");                                          // len("  [\"") = 4
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
    emitter.instruction("mov edx, 4");                                          // len("  [\"") = 4
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

    // Frame (96 bytes): [0]=hash ptr, [8]=cursor, [16]=count, [24]=items,
    //   [32]=key_ptr, [40]=key_len, [48]=val_lo, [56]=val_hi, [64]=val_tag,
    //   [80]=x29, [88]=x30.
    emitter.instruction("sub sp, sp, #96");                                     // allocate the hash-walk frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the hash table pointer
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
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload key len
    emitter.instruction("cmn x2, #1");                                          // integer key? (len == -1)
    emitter.instruction("b.eq __rt_vd_hash_int_key");                           // format integer keys as [N]
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload key ptr
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload key len
    emitter.instruction("bl __rt_var_dump_emit_string_key");                    // emit `  ["KEY"]=>\n`
    emitter.instruction("b __rt_vd_hash_after_key");                            // continue to the value line
    emitter.label("__rt_vd_hash_int_key");
    emitter.instruction("ldr x11, [sp, #32]");                                  // integer key payload → indexed-key helper's x11 input
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // emit `  [N]=>\n`

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
    emitter.instruction("bl __rt_var_dump_emit_null_line");                     // tags 4/5/6/8 (nested/object/null) → NULL line
    emitter.instruction("b __rt_vd_hash_next");                                 // advance to the next entry

    emitter.label("__rt_vd_hash_v_int");
    emitter.instruction("ldr x0, [sp, #48]");                                   // load the integer payload
    emitter.instruction("bl __rt_var_dump_emit_int_line");                      // emit `  int(VAL)\n`
    emitter.instruction("b __rt_vd_hash_next");                                 // advance to the next entry

    emitter.label("__rt_vd_hash_v_str");
    emitter.instruction("ldr x1, [sp, #48]");                                   // load the string pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // load the string length
    emitter.instruction("bl __rt_var_dump_emit_string_line");                   // emit `  string(LEN) "VAL"\n`
    emitter.instruction("b __rt_vd_hash_next");                                 // advance to the next entry

    emitter.label("__rt_vd_hash_v_flt");
    emitter.instruction("ldr d0, [sp, #48]");                                   // load the float bit pattern
    emitter.instruction("bl __rt_var_dump_emit_float_line");                    // emit `  float(VAL)\n`
    emitter.instruction("b __rt_vd_hash_next");                                 // advance to the next entry

    emitter.label("__rt_vd_hash_v_bool");
    emitter.instruction("ldr x0, [sp, #48]");                                   // load the bool payload (0 or 1)
    emitter.instruction("bl __rt_var_dump_emit_bool_line");                     // emit `  bool(true|false)\n`

    emitter.label("__rt_vd_hash_next");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload items emitted
    emitter.instruction("add x9, x9, #1");                                      // count this entry
    emitter.instruction("str x9, [sp, #24]");                                   // save the updated item count
    emitter.instruction("b __rt_vd_hash_loop");                                 // continue with the next entry

    emitter.label("__rt_vd_hash_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the hash-walk frame
    emitter.instruction("ret");                                                 // return to the var_dump caller
}

/// Emits the Linux x86_64 runtime helper for walking an associative array in var_dump.
fn emit_var_dump_hash_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_hash ---");
    emitter.label_global("__rt_var_dump_hash");

    // rbp-relative frame: [-8]=hash ptr, [-16]=cursor, [-24]=count, [-32]=items,
    //   [-40]=key_ptr, [-48]=key_len, [-56]=val_lo, [-64]=val_hi, [-72]=val_tag.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 96");                                         // allocate the hash-walk frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash table pointer
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

    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload key len
    emitter.instruction("cmp rdx, -1");                                         // integer key?
    emitter.instruction("je __rt_vd_hash_int_key_x86");                         // format integer keys as [N]
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload key ptr → string-key helper's rdi
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload key len → string-key helper's rsi
    emitter.instruction("call __rt_var_dump_emit_string_key");                  // emit `  ["KEY"]=>\n`
    emitter.instruction("jmp __rt_vd_hash_after_key_x86");                      // continue to the value line
    emitter.label("__rt_vd_hash_int_key_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // integer key payload → indexed-key helper's rdi
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // emit `  [N]=>\n`

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
    emitter.instruction("call __rt_var_dump_emit_null_line");                   // tags 4/5/6/8 (nested/object/null) → NULL line
    emitter.instruction("jmp __rt_vd_hash_next_x86");                           // advance to the next entry

    emitter.label("__rt_vd_hash_v_int_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // load the integer payload
    emitter.instruction("call __rt_var_dump_emit_int_line");                    // emit `  int(VAL)\n`
    emitter.instruction("jmp __rt_vd_hash_next_x86");                           // advance to the next entry

    emitter.label("__rt_vd_hash_v_str_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // load the string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // load the string length
    emitter.instruction("call __rt_var_dump_emit_string_line");                 // emit `  string(LEN) "VAL"\n`
    emitter.instruction("jmp __rt_vd_hash_next_x86");                           // advance to the next entry

    emitter.label("__rt_vd_hash_v_flt_x86");
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 56]");                    // load the float bit pattern
    emitter.instruction("call __rt_var_dump_emit_float_line");                  // emit `  float(VAL)\n`
    emitter.instruction("jmp __rt_vd_hash_next_x86");                           // advance to the next entry

    emitter.label("__rt_vd_hash_v_bool_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // load the bool payload (0 or 1)
    emitter.instruction("call __rt_var_dump_emit_bool_line");                   // emit `  bool(true|false)\n`

    emitter.label("__rt_vd_hash_next_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload items emitted
    emitter.instruction("add r10, 1");                                          // count this entry
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the updated item count
    emitter.instruction("jmp __rt_vd_hash_loop_x86");                           // continue with the next entry

    emitter.label("__rt_vd_hash_done_x86");
    emitter.instruction("add rsp, 96");                                         // release the hash-walk frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the var_dump caller
}
