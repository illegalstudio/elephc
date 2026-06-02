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
//! - Hash / Mixed-element arrays are not handled by these walkers; the
//!   v1 fallback in the builtin emitter prints just `array(N) {\n}\n`.

use crate::codegen::{emit::Emitter, platform::Arch};

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
    emitter.instruction("mov rdi, r11");
    emitter.instruction("call __rt_var_dump_emit_indexed_key");

    // -- emit `  int(VAL)\n` --
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload index
    emitter.instruction("mov r12, r11");
    emitter.instruction("add r12, 3");                                          // skip 3-quad header
    emitter.instruction("mov rdi, QWORD PTR [r9 + r12 * 8]");                   // load element[index] into the emit helper's first arg
    emitter.instruction("call __rt_var_dump_emit_int_line");

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
    emitter.instruction("sub sp, sp, #32");
    emitter.instruction("stp x29, x30, [sp, #16]");
    emitter.instruction("mov x29, sp");
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction("str xzr, [sp, #8]");

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
    emitter.instruction("ldp x29, x30, [sp, #16]");
    emitter.instruction("add sp, sp, #32");
    emitter.instruction("ret");
}

fn emit_var_dump_array_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_str ---");
    emitter.label_global("__rt_var_dump_array_str");

    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // index = 0

    emitter.label("__rt_vd_arr_str_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the array pointer
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the element count
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the current index
    emitter.instruction("cmp r11, r10");                                        // processed every element?
    emitter.instruction("jge __rt_vd_arr_str_done_x86");                        // walk complete

    emitter.instruction("mov rdi, r11");
    emitter.instruction("call __rt_var_dump_emit_indexed_key");

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload index
    emitter.instruction("mov r12, r11");
    emitter.instruction("shl r12, 4");                                          // index * 16
    emitter.instruction("add r12, 24");                                         // element base offset
    emitter.instruction("add r12, r9");                                         // element address
    emitter.instruction("mov rdi, QWORD PTR [r12]");                            // string ptr → emit helper's first arg
    emitter.instruction("mov rsi, QWORD PTR [r12 + 8]");                        // string len → emit helper's second arg
    emitter.instruction("call __rt_var_dump_emit_string_line");

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");
    emitter.instruction("add r11, 1");
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");
    emitter.instruction("jmp __rt_vd_arr_str_loop_x86");

    emitter.label("__rt_vd_arr_str_done_x86");
    emitter.instruction("add rsp, 16");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
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

    emitter.instruction("sub sp, sp, #16");
    emitter.instruction("stp x29, x30, [sp, #0]");
    emitter.instruction("mov x29, sp");

    // Emit "  ["
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_indent_open");
    emitter.instruction("mov x2, #3");                                          // len("  [") = 3
    emitter.instruction("mov x0, #1");                                          // fd=stdout
    emitter.syscall(4);

    // itoa(index) → x1/x2
    emitter.instruction("mov x0, x11");                                         // x11 holds the index from the caller's loop
    emitter.instruction("bl __rt_itoa");
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    // Emit "]=>\n"
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_close_arrow");
    emitter.instruction("mov x2, #4");                                          // len("]=>\n") = 4
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    emitter.instruction("ldp x29, x30, [sp, #0]");
    emitter.instruction("add sp, sp, #16");
    emitter.instruction("ret");
}

fn emit_var_dump_emit_indexed_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_indexed_key ---");
    emitter.label_global("__rt_var_dump_emit_indexed_key");

    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the index

    // Emit "  ["
    emitter.instruction("lea rsi, [rip + _vd_indent_open]");
    emitter.instruction("mov edx, 3");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    // itoa(index)
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
    emitter.instruction("call __rt_itoa");
    emitter.instruction("mov rsi, rax");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    // Emit "]=>\n"
    emitter.instruction("lea rsi, [rip + _vd_close_arrow]");
    emitter.instruction("mov edx, 4");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.instruction("add rsp, 16");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
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

    emitter.instruction("sub sp, sp, #16");
    emitter.instruction("stp x29, x30, [sp, #0]");
    emitter.instruction("mov x29, sp");

    // Emit "  int("
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_int_prefix");
    emitter.instruction("mov x2, #6");                                          // len("  int(") = 6
    emitter.instruction("mov x9, x0");                                          // preserve value
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    // itoa(value)
    emitter.instruction("mov x0, x9");
    emitter.instruction("bl __rt_itoa");
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    // Emit ")\n"
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_close_paren");
    emitter.instruction("mov x2, #2");                                          // len(")\n") = 2
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    emitter.instruction("ldp x29, x30, [sp, #0]");
    emitter.instruction("add sp, sp, #16");
    emitter.instruction("ret");
}

fn emit_var_dump_emit_int_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_int_line ---");
    emitter.label_global("__rt_var_dump_emit_int_line");

    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save value

    emitter.instruction("lea rsi, [rip + _vd_int_prefix]");
    emitter.instruction("mov edx, 6");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
    emitter.instruction("call __rt_itoa");
    emitter.instruction("mov rsi, rax");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.instruction("lea rsi, [rip + _vd_close_paren]");
    emitter.instruction("mov edx, 2");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.instruction("add rsp, 16");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
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

    emitter.instruction("sub sp, sp, #32");
    emitter.instruction("stp x29, x30, [sp, #16]");
    emitter.instruction("mov x29, sp");
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save ptr/len

    // Emit "  string("
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_str_prefix");
    emitter.instruction("mov x2, #9");                                          // len("  string(") = 9
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    // itoa(len)
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload len
    emitter.instruction("bl __rt_itoa");
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    // Emit ") "
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_close_paren_space");
    emitter.instruction("mov x2, #3");                                          // len(") \"") = 3 — includes the opening quote
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    // Write the actual bytes
    emitter.instruction("ldr x1, [sp, #0]");                                    // ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // len
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    // Emit "\"\n"
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_close_quote");
    emitter.instruction("mov x2, #2");                                          // len("\"\n") = 2
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    emitter.instruction("ldp x29, x30, [sp, #16]");
    emitter.instruction("add sp, sp, #32");
    emitter.instruction("ret");
}

fn emit_var_dump_emit_string_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_string_line ---");
    emitter.label_global("__rt_var_dump_emit_string_line");

    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save len

    emitter.instruction("lea rsi, [rip + _vd_str_prefix]");
    emitter.instruction("mov edx, 9");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");
    emitter.instruction("call __rt_itoa");
    emitter.instruction("mov rsi, rax");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.instruction("lea rsi, [rip + _vd_close_paren_space]");
    emitter.instruction("mov edx, 3");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // len
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.instruction("lea rsi, [rip + _vd_close_quote]");
    emitter.instruction("mov edx, 2");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.instruction("add rsp, 16");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
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
    emitter.instruction(&format!("b {}", done_label));
    emitter.label(false_label);
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_bool_false_line");
    emitter.instruction("mov x2, #14");                                         // len("  bool(false)\n") = 14
    emitter.label(done_label);
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("ret");
}

fn emit_var_dump_emit_bool_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_bool_line ---");
    emitter.label_global("__rt_var_dump_emit_bool_line");

    let false_label = "__rt_vd_bool_false_x86";
    let done_label = "__rt_vd_bool_done_x86";
    emitter.instruction("test rdi, rdi");
    emitter.instruction(&format!("jz {}", false_label));
    emitter.instruction("lea rsi, [rip + _vd_bool_true_line]");
    emitter.instruction("mov edx, 13");
    emitter.instruction(&format!("jmp {}", done_label));
    emitter.label(false_label);
    emitter.instruction("lea rsi, [rip + _vd_bool_false_line]");
    emitter.instruction("mov edx, 14");
    emitter.label(done_label);
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");
    emitter.instruction("ret");
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

    emitter.instruction("sub sp, sp, #32");
    emitter.instruction("stp x29, x30, [sp, #16]");
    emitter.instruction("mov x29, sp");
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction("str xzr, [sp, #8]");

    emitter.label("__rt_vd_arr_bool_loop");
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction("ldr x10, [x9]");                                       // element count
    emitter.instruction("ldr x11, [sp, #8]");
    emitter.instruction("cmp x11, x10");
    emitter.instruction("b.ge __rt_vd_arr_bool_done");

    emitter.instruction("bl __rt_var_dump_emit_indexed_key");

    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction("ldr x11, [sp, #8]");
    emitter.instruction("add x12, x11, #3");                                    // skip 3-quad header
    emitter.instruction("ldr x0, [x9, x12, lsl #3]");                           // load element[index] (0 or 1)
    emitter.instruction("bl __rt_var_dump_emit_bool_line");

    emitter.instruction("ldr x11, [sp, #8]");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("str x11, [sp, #8]");
    emitter.instruction("b __rt_vd_arr_bool_loop");

    emitter.label("__rt_vd_arr_bool_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");
    emitter.instruction("add sp, sp, #32");
    emitter.instruction("ret");
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

    emitter.instruction("sub sp, sp, #16");
    emitter.instruction("stp x29, x30, [sp, #0]");
    emitter.instruction("mov x29, sp");

    // Emit "  float("
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_float_prefix");
    emitter.instruction("mov x2, #8");                                          // len("  float(") = 8
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    // ftoa(d0) → x1=ptr, x2=len
    emitter.instruction("bl __rt_ftoa");
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    // Emit ")\n"
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_close_paren");
    emitter.instruction("mov x2, #2");
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    emitter.instruction("ldp x29, x30, [sp, #0]");
    emitter.instruction("add sp, sp, #16");
    emitter.instruction("ret");
}

fn emit_var_dump_emit_float_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_float_line ---");
    emitter.label_global("__rt_var_dump_emit_float_line");

    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");
    emitter.instruction("movsd QWORD PTR [rbp - 8], xmm0");                     // preserve xmm0 across the prefix syscall

    emitter.instruction("lea rsi, [rip + _vd_float_prefix]");
    emitter.instruction("mov edx, 8");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // reload xmm0 for ftoa
    emitter.instruction("call __rt_ftoa");                                      // rax=ptr, rdx=len
    emitter.instruction("mov rsi, rax");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.instruction("lea rsi, [rip + _vd_close_paren]");
    emitter.instruction("mov edx, 2");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.instruction("add rsp, 16");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
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

    emitter.instruction("sub sp, sp, #32");
    emitter.instruction("stp x29, x30, [sp, #16]");
    emitter.instruction("mov x29, sp");
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction("str xzr, [sp, #8]");

    emitter.label("__rt_vd_arr_float_loop");
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("ldr x11, [sp, #8]");
    emitter.instruction("cmp x11, x10");
    emitter.instruction("b.ge __rt_vd_arr_float_done");

    emitter.instruction("bl __rt_var_dump_emit_indexed_key");

    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction("ldr x11, [sp, #8]");
    emitter.instruction("add x12, x11, #3");                                    // skip 3-quad header
    emitter.instruction("ldr d0, [x9, x12, lsl #3]");                           // load f64 element[index]
    emitter.instruction("bl __rt_var_dump_emit_float_line");

    emitter.instruction("ldr x11, [sp, #8]");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("str x11, [sp, #8]");
    emitter.instruction("b __rt_vd_arr_float_loop");

    emitter.label("__rt_vd_arr_float_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");
    emitter.instruction("add sp, sp, #32");
    emitter.instruction("ret");
}

fn emit_var_dump_array_float_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_float ---");
    emitter.label_global("__rt_var_dump_array_float");

    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");

    emitter.label("__rt_vd_arr_float_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");
    emitter.instruction("mov r10, QWORD PTR [r9]");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");
    emitter.instruction("cmp r11, r10");
    emitter.instruction("jge __rt_vd_arr_float_done_x86");

    emitter.instruction("mov rdi, r11");
    emitter.instruction("call __rt_var_dump_emit_indexed_key");

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");
    emitter.instruction("mov r12, r11");
    emitter.instruction("add r12, 3");
    emitter.instruction("movsd xmm0, QWORD PTR [r9 + r12 * 8]");                // load f64 element[index] into xmm0
    emitter.instruction("call __rt_var_dump_emit_float_line");

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");
    emitter.instruction("add r11, 1");
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");
    emitter.instruction("jmp __rt_vd_arr_float_loop_x86");

    emitter.label("__rt_vd_arr_float_done_x86");
    emitter.instruction("add rsp, 16");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// `__rt_var_dump_emit_null_line`: emit `  NULL\n` for a null payload.
pub fn emit_var_dump_emit_null_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: var_dump_emit_null_line ---");
        emitter.label_global("__rt_var_dump_emit_null_line");
        emitter.instruction("lea rsi, [rip + _vd_null_line]");
        emitter.instruction("mov edx, 7");                                      // len("  NULL\n") = 7
        emitter.instruction("mov edi, 1");
        emitter.instruction("mov eax, 1");
        emitter.instruction("syscall");
        emitter.instruction("ret");
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_null_line ---");
    emitter.label_global("__rt_var_dump_emit_null_line");
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_vd_null_line");
    emitter.instruction("mov x2, #7");                                          // len("  NULL\n") = 7
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("ret");
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
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the tag
    emitter.instruction("cmp x9, #7");                                          // Mixed?
    emitter.instruction("b.ne __rt_vd_arr_mixed_skip");                         // not Mixed → leave the body empty

    emitter.instruction("sub sp, sp, #32");
    emitter.instruction("stp x29, x30, [sp, #16]");
    emitter.instruction("mov x29, sp");
    emitter.instruction("str x0, [sp, #0]");                                    // array ptr
    emitter.instruction("str xzr, [sp, #8]");                                   // index = 0

    emitter.label("__rt_vd_arr_mixed_loop");
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction("ldr x10, [x9]");                                       // element count
    emitter.instruction("ldr x11, [sp, #8]");
    emitter.instruction("cmp x11, x10");
    emitter.instruction("b.ge __rt_vd_arr_mixed_done");

    emitter.instruction("bl __rt_var_dump_emit_indexed_key");

    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction("ldr x11, [sp, #8]");
    emitter.instruction("add x12, x11, #3");                                    // skip 3-quad header
    emitter.instruction("ldr x13, [x9, x12, lsl #3]");                          // Mixed cell pointer
    emitter.instruction("ldr x14, [x13]");                                      // runtime value tag at cell[0]
    emitter.instruction("cmp x14, #0");                                         // tag 0 = int
    emitter.instruction("b.eq __rt_vd_arr_mixed_int");
    emitter.instruction("cmp x14, #1");                                         // tag 1 = string
    emitter.instruction("b.eq __rt_vd_arr_mixed_str");
    emitter.instruction("cmp x14, #2");                                         // tag 2 = float
    emitter.instruction("b.eq __rt_vd_arr_mixed_flt");
    emitter.instruction("cmp x14, #3");                                         // tag 3 = bool
    emitter.instruction("b.eq __rt_vd_arr_mixed_bool");
    emitter.instruction("bl __rt_var_dump_emit_null_line");                     // unsupported tag → NULL fallback
    emitter.instruction("b __rt_vd_arr_mixed_next");

    emitter.label("__rt_vd_arr_mixed_int");
    emitter.instruction("ldr x0, [x13, #8]");
    emitter.instruction("bl __rt_var_dump_emit_int_line");
    emitter.instruction("b __rt_vd_arr_mixed_next");

    emitter.label("__rt_vd_arr_mixed_str");
    emitter.instruction("ldr x1, [x13, #8]");                                   // string ptr → x1 per elephc string ABI
    emitter.instruction("ldr x2, [x13, #16]");                                  // string len → x2
    emitter.instruction("bl __rt_var_dump_emit_string_line");
    emitter.instruction("b __rt_vd_arr_mixed_next");

    emitter.label("__rt_vd_arr_mixed_flt");
    emitter.instruction("ldr d0, [x13, #8]");
    emitter.instruction("bl __rt_var_dump_emit_float_line");
    emitter.instruction("b __rt_vd_arr_mixed_next");

    emitter.label("__rt_vd_arr_mixed_bool");
    emitter.instruction("ldr x0, [x13, #8]");
    emitter.instruction("bl __rt_var_dump_emit_bool_line");

    emitter.label("__rt_vd_arr_mixed_next");
    emitter.instruction("ldr x11, [sp, #8]");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("str x11, [sp, #8]");
    emitter.instruction("b __rt_vd_arr_mixed_loop");

    emitter.label("__rt_vd_arr_mixed_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");
    emitter.instruction("add sp, sp, #32");
    emitter.label("__rt_vd_arr_mixed_skip");                                    // wrong stamp → return without any body
    emitter.instruction("ret");
}

fn emit_var_dump_array_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_mixed ---");
    emitter.label_global("__rt_var_dump_array_mixed");

    // Defensive stamp check (see ARM64 prologue): only walk arrays
    // whose value_type stamp says Mixed (=7).
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // packed array kind word
    emitter.instruction("shr r9, 8");                                           // shift the value_type tag into the low byte
    emitter.instruction("and r9, 0xff");                                        // isolate the tag
    emitter.instruction("cmp r9, 7");                                           // Mixed?
    emitter.instruction("jne __rt_vd_arr_mixed_skip_x86");                      // not Mixed → leave the body empty

    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");

    emitter.label("__rt_vd_arr_mixed_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");
    emitter.instruction("mov r10, QWORD PTR [r9]");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");
    emitter.instruction("cmp r11, r10");
    emitter.instruction("jge __rt_vd_arr_mixed_done_x86");

    emitter.instruction("mov rdi, r11");
    emitter.instruction("call __rt_var_dump_emit_indexed_key");

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");
    emitter.instruction("mov r12, r11");
    emitter.instruction("add r12, 3");
    emitter.instruction("mov r13, QWORD PTR [r9 + r12 * 8]");
    emitter.instruction("mov r14, QWORD PTR [r13]");

    emitter.instruction("cmp r14, 0");
    emitter.instruction("je __rt_vd_arr_mixed_int_x86");
    emitter.instruction("cmp r14, 1");
    emitter.instruction("je __rt_vd_arr_mixed_str_x86");
    emitter.instruction("cmp r14, 2");
    emitter.instruction("je __rt_vd_arr_mixed_flt_x86");
    emitter.instruction("cmp r14, 3");
    emitter.instruction("je __rt_vd_arr_mixed_bool_x86");
    emitter.instruction("call __rt_var_dump_emit_null_line");
    emitter.instruction("jmp __rt_vd_arr_mixed_next_x86");

    emitter.label("__rt_vd_arr_mixed_int_x86");
    emitter.instruction("mov rdi, QWORD PTR [r13 + 8]");
    emitter.instruction("call __rt_var_dump_emit_int_line");
    emitter.instruction("jmp __rt_vd_arr_mixed_next_x86");

    emitter.label("__rt_vd_arr_mixed_str_x86");
    emitter.instruction("mov rdi, QWORD PTR [r13 + 8]");
    emitter.instruction("mov rsi, QWORD PTR [r13 + 16]");
    emitter.instruction("call __rt_var_dump_emit_string_line");
    emitter.instruction("jmp __rt_vd_arr_mixed_next_x86");

    emitter.label("__rt_vd_arr_mixed_flt_x86");
    emitter.instruction("movsd xmm0, QWORD PTR [r13 + 8]");
    emitter.instruction("call __rt_var_dump_emit_float_line");
    emitter.instruction("jmp __rt_vd_arr_mixed_next_x86");

    emitter.label("__rt_vd_arr_mixed_bool_x86");
    emitter.instruction("mov rdi, QWORD PTR [r13 + 8]");
    emitter.instruction("call __rt_var_dump_emit_bool_line");

    emitter.label("__rt_vd_arr_mixed_next_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");
    emitter.instruction("add r11, 1");
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");
    emitter.instruction("jmp __rt_vd_arr_mixed_loop_x86");

    emitter.label("__rt_vd_arr_mixed_done_x86");
    emitter.instruction("add rsp, 16");
    emitter.instruction("pop rbp");
    emitter.label("__rt_vd_arr_mixed_skip_x86");                                // wrong stamp → return without any body
    emitter.instruction("ret");
}

fn emit_var_dump_array_bool_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_array_bool ---");
    emitter.label_global("__rt_var_dump_array_bool");

    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");

    emitter.label("__rt_vd_arr_bool_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");
    emitter.instruction("mov r10, QWORD PTR [r9]");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");
    emitter.instruction("cmp r11, r10");
    emitter.instruction("jge __rt_vd_arr_bool_done_x86");

    emitter.instruction("mov rdi, r11");
    emitter.instruction("call __rt_var_dump_emit_indexed_key");

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");
    emitter.instruction("mov r12, r11");
    emitter.instruction("add r12, 3");
    emitter.instruction("mov rdi, QWORD PTR [r9 + r12 * 8]");
    emitter.instruction("call __rt_var_dump_emit_bool_line");

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");
    emitter.instruction("add r11, 1");
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");
    emitter.instruction("jmp __rt_vd_arr_bool_loop_x86");

    emitter.label("__rt_vd_arr_bool_done_x86");
    emitter.instruction("add rsp, 16");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}
