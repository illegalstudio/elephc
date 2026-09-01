//! Purpose:
//! Emits the `__rt_var_dump_*` runtime walkers that render a PHP `var_dump`
//! array/hash body: one `<indent>[KEY]=>\n<indent>TYPE(VAL)\n` block per entry,
//! recursing into nested arrays/hashes to arbitrary depth. The opening
//! `array(N) {\n` and closing `}\n` of the TOP-LEVEL value are emitted by the
//! builtin caller (`codegen::lower_inst::builtins::debug`) around these walks;
//! nested containers open and close themselves inside `__rt_var_dump_value`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The `var_dump` builtin emitter when the value's static type is an array or
//!   an associative array.
//!
//! Key details:
//! - Indentation is a runtime global, `_vd_indent` (spaces). Every line emitter
//!   starts with `__rt_vd_pad`, which writes `_vd_indent` spaces, so the body
//!   literals themselves carry NO leading indent (`"int("`, not `"  int("`).
//!   The builtin sets `_vd_indent = 2` around a top-level body; each nested
//!   container walk bumps it by 2 and restores it afterwards. PHP's format is
//!   exactly 2 spaces per nesting level.
//! - Array layout reused from the existing JSON encoders: 24-byte header
//!   (len at offset 0, value_type at offset 8, refcount at offset 16)
//!   followed by 8-byte elements starting at offset 24.
//! - String elements use the elephc string-result ABI: 16-byte slots
//!   storing (ptr, len) — so element[N] for an indexed string array lives
//!   at offsets `24 + N*16` (ptr) and `32 + N*16` (len).
//! - `__rt_var_dump_indexed` is the UNIVERSAL indexed walker: it self-dispatches
//!   on the array's runtime value_type stamp (`[arr-8]` byte 1: 0=int, 1=str,
//!   2=float, 3=bool, 4=array, 5=hash, 6=object, 7=mixed-cell), the same stamp
//!   `__rt_print_r_indexed` reads, so it needs no static element type and copes
//!   with arrays whose static type and runtime slots disagree. The homogeneous
//!   `__rt_var_dump_array_int` / `_str` / `_bool` / `_float` walkers remain for
//!   statically-typed scalar arrays.
//! - `__rt_var_dump_value` renders one value at the current indent; tags 4/5
//!   open a nested `array(N) {` block, recurse, and close it; tag 6 opens an
//!   `object(C) (n) {` block through `super::var_dump_object` under the
//!   `_vd_seen` recursion guard; tag 7 unboxes a Mixed cell and redispatches.
//!   Mutual recursion with the container walkers gives arbitrary nesting depth.
//! - Associative arrays (hashes) are handled by `__rt_var_dump_hash`, which
//!   iterates entries via `__rt_hash_iter_next`, formats string/integer keys and
//!   delegates every value to `__rt_var_dump_value`.
//! - Objects (tag 6) render their full body — see `super::var_dump_object` for
//!   the per-class descriptor, the initialized-property count, and the
//!   `*RECURSION*` guard, plus why PHP's `#id` handle is deliberately absent.
//! - REMAINING GAP: resources (tag 9) nested inside a container render as
//!   `NULL` rather than PHP's `resource(N) of type (...)`.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::abi;

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

    // -- emit `<indent>[N]=>\n` --
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // emits "  [N]=>\n" for x11=index

    // -- emit `<indent>int(VAL)\n` --
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

    // -- emit `<indent>[N]=>\n` (helper expects index in rdi) --
    emitter.instruction("mov rdi, r11");                                        // prepare SysV call argument
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // call runtime helper

    // -- emit `<indent>int(VAL)\n` --
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

    // -- emit `<indent>[N]=>\n` --
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // emits "  [N]=>\n" for x11=index

    // -- emit `<indent>string(LEN) "VAL"\n` --
    // String elements are 16-byte slots: ptr at offset 24+16*N, len at 32+16*N.
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload index
    emitter.instruction("lsl x12, x11, #4");                                    // index * 16
    emitter.instruction("add x12, x12, #24");                                   // element base offset = 24 + index*16
    emitter.instruction("add x13, x9, x12");                                    // element address
    emitter.instruction("ldr x1, [x13]");                                       // load element string ptr
    emitter.instruction("ldr x2, [x13, #8]");                                   // load element string len
    emitter.instruction("bl __rt_var_dump_emit_string_line");                   // emits `<indent>string(LEN) "VAL"\n`

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

/// `__rt_var_dump_emit_indexed_key`: emit `<indent>[N]=>\n` for a numeric index.
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

    // Indent the key line (x11 survives: __rt_vd_pad only clobbers x0-x2)
    emitter.instruction("bl __rt_vd_pad");                                      // write `_vd_indent` spaces before the key

    // Emit "["
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_indent_open");
    emitter.instruction("mov x2, #1");                                          // len("[") = 1
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

    // itoa(index) → x1/x2
    emitter.instruction("mov x0, x11");                                         // x11 holds the index from the caller's loop
    emitter.instruction("bl __rt_itoa");                                        // call runtime helper
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

    // Emit "]=>\n"
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_close_arrow");
    emitter.instruction("mov x2, #4");                                          // len("]=>\n") = 4
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

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

    emitter.instruction("call __rt_vd_pad");                                    // write `_vd_indent` spaces before the key

    // Emit "["
    abi::emit_symbol_address(emitter, "rsi", "_vd_indent_open");                // load runtime data address
    emitter.instruction("mov edx, 1");                                          // len("[") = 1
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    // itoa(index)
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // prepare runtime result value
    emitter.instruction("call __rt_itoa");                                      // call runtime helper
    emitter.instruction("mov rsi, rax");                                        // prepare SysV call argument
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    // Emit "]=>\n"
    abi::emit_symbol_address(emitter, "rsi", "_vd_close_arrow");                // load runtime data address
    emitter.instruction("mov edx, 4");                                          // prepare SysV call argument
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_int_line`: emit `<indent>int(VAL)\n` for a single int.
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

    // Emit "int(" after the indent pad
    emitter.instruction("mov x9, x0");                                          // preserve value across the pad and the prefix write
    emitter.instruction("bl __rt_vd_pad");                                      // write `_vd_indent` spaces before the value
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_int_prefix");
    emitter.instruction("mov x2, #4");                                          // len("int(") = 4
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

    // itoa(value)
    emitter.instruction("mov x0, x9");                                          // prepare AArch64 call argument
    emitter.instruction("bl __rt_itoa");                                        // call runtime helper
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

    // Emit ")\n"
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_close_paren");
    emitter.instruction("mov x2, #2");                                          // len(")\n") = 2
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

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

    emitter.instruction("call __rt_vd_pad");                                    // write `_vd_indent` spaces before the value
    abi::emit_symbol_address(emitter, "rsi", "_vd_int_prefix");                 // load runtime data address
    emitter.instruction("mov edx, 4");                                          // len("int(") = 4
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // prepare runtime result value
    emitter.instruction("call __rt_itoa");                                      // call runtime helper
    emitter.instruction("mov rsi, rax");                                        // prepare SysV call argument
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    abi::emit_symbol_address(emitter, "rsi", "_vd_close_paren");                // load runtime data address
    emitter.instruction("mov edx, 2");                                          // prepare SysV call argument
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_string_line`: emit `<indent>string(LEN) "VAL"\n` for a
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

    emitter.instruction("bl __rt_vd_pad");                                      // write `_vd_indent` spaces before the value

    // Emit "string("
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_str_prefix");
    emitter.instruction("mov x2, #7");                                          // len("string(") = 7
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

    // itoa(len)
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload len
    emitter.instruction("bl __rt_itoa");                                        // call runtime helper
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

    // Emit ") "
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_close_paren_space");
    emitter.instruction("mov x2, #3");                                          // len(") \"") = 3 — includes the opening quote
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

    // Write the actual bytes
    emitter.instruction("ldr x1, [sp, #0]");                                    // ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // len
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

    // Emit "\"\n"
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_close_quote");
    emitter.instruction("mov x2, #2");                                          // len("\"\n") = 2
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

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

    emitter.instruction("call __rt_vd_pad");                                    // write `_vd_indent` spaces before the value
    abi::emit_symbol_address(emitter, "rsi", "_vd_str_prefix");                 // load runtime data address
    emitter.instruction("mov edx, 7");                                          // len("string(") = 7
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // prepare runtime result value
    emitter.instruction("call __rt_itoa");                                      // call runtime helper
    emitter.instruction("mov rsi, rax");                                        // prepare SysV call argument
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    abi::emit_symbol_address(emitter, "rsi", "_vd_close_paren_space");          // load runtime data address
    emitter.instruction("mov edx, 3");                                          // prepare SysV call argument
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // len
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    abi::emit_symbol_address(emitter, "rsi", "_vd_close_quote");                // load runtime data address
    emitter.instruction("mov edx, 2");                                          // prepare SysV call argument
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_bool_line`: emit `<indent>bool(true)\n` or
/// `<indent>bool(false)\n` for a single bool. Input: AArch64 x0 / x86_64 rdi =
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
    emitter.instruction("sub sp, sp, #16");                                     // allocate runtime stack frame (the indent pad needs x30 saved)
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("mov x9, x0");                                          // preserve the bool payload across the pad
    emitter.instruction("bl __rt_vd_pad");                                      // write `_vd_indent` spaces before the value
    emitter.instruction(&format!("cbz x9, {}", false_label));                   // value == 0 → false line
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_bool_true_line");
    emitter.instruction("mov x2, #11");                                         // len("bool(true)\n") = 11
    emitter.instruction(&format!("b {}", done_label));                          // continue at target label
    emitter.label(false_label);
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_bool_false_line");
    emitter.instruction("mov x2, #12");                                         // len("bool(false)\n") = 12
    emitter.label(done_label);
    emitter.instruction("bl __rt_vd_write");                                    // write the bool line through the ob/web-aware stdout sink
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for var dump emit bool line.
fn emit_var_dump_emit_bool_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_bool_line ---");
    emitter.label_global("__rt_var_dump_emit_bool_line");

    let false_label = "__rt_vd_bool_false_x86";
    let done_label = "__rt_vd_bool_done_x86";
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the bool payload across the pad
    emitter.instruction("call __rt_vd_pad");                                    // write `_vd_indent` spaces before the value
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the bool payload
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction(&format!("jz {}", false_label));                        // branch when the checked value is zero or equal
    abi::emit_symbol_address(emitter, "rsi", "_vd_bool_true_line");             // load runtime data address
    emitter.instruction("mov edx, 11");                                         // len("bool(true)\n") = 11
    emitter.instruction(&format!("jmp {}", done_label));                        // continue at target label
    emitter.label(false_label);
    abi::emit_symbol_address(emitter, "rsi", "_vd_bool_false_line");            // load runtime data address
    emitter.instruction("mov edx, 12");                                         // len("bool(false)\n") = 12
    emitter.label(done_label);
    emitter.instruction("call __rt_vd_write");                                  // write the bool line through the ob/web-aware stdout sink
    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
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

/// `__rt_var_dump_emit_float_line`: emit `<indent>float(VAL)\n` for a single
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

    emitter.instruction("bl __rt_vd_pad");                                      // write `_vd_indent` spaces (d0 survives: __rt_vd_write preserves it)

    // Emit "float("
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_float_prefix");
    emitter.instruction("mov x2, #6");                                          // len("float(") = 6
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

    // ftoa(d0) → x1=ptr, x2=len
    emitter.instruction("bl __rt_ftoa_repr");                                   // render at serialize_precision=-1 (var_dump layout)
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

    // Emit ")\n"
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_close_paren");
    emitter.instruction("mov x2, #2");                                          // prepare AArch64 call argument
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

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

    emitter.instruction("call __rt_vd_pad");                                    // write `_vd_indent` spaces before the value
    abi::emit_symbol_address(emitter, "rsi", "_vd_float_prefix");               // load runtime data address
    emitter.instruction("mov edx, 6");                                          // len("float(") = 6
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // reload xmm0 for ftoa
    emitter.instruction("call __rt_ftoa_repr");                                 // serialize_precision=-1 layout: rax=ptr, rdx=len
    emitter.instruction("mov rsi, rax");                                        // prepare SysV call argument
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    abi::emit_symbol_address(emitter, "rsi", "_vd_close_paren");                // load runtime data address
    emitter.instruction("mov edx, 2");                                          // prepare SysV call argument
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

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

/// `__rt_var_dump_emit_null_line`: emit `<indent>NULL\n` for a null payload.
pub fn emit_var_dump_emit_null_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: var_dump_emit_null_line ---");
        emitter.label_global("__rt_var_dump_emit_null_line");
        emitter.instruction("push rbp");                                        // save caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish runtime frame pointer
        emitter.instruction("call __rt_vd_pad");                                // write `_vd_indent` spaces before the value
        abi::emit_symbol_address(emitter, "rsi", "_vd_null_line");              // load runtime data address
        emitter.instruction("mov edx, 5");                                      // len("NULL\n") = 5
        emitter.instruction("call __rt_vd_write");                              // write the NULL line through the ob/web-aware stdout sink
        emitter.instruction("pop rbp");                                         // restore caller frame pointer
        emitter.instruction("ret");                                             // return to caller
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_null_line ---");
    emitter.label_global("__rt_var_dump_emit_null_line");
    emitter.instruction("sub sp, sp, #16");                                     // allocate runtime stack frame (the indent pad needs x30 saved)
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("bl __rt_vd_pad");                                      // write `_vd_indent` spaces before the value
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_null_line");
    emitter.instruction("mov x2, #5");                                          // len("NULL\n") = 5
    emitter.instruction("bl __rt_vd_write");                                    // write the NULL line through the ob/web-aware stdout sink
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_vd_pad`: write `_vd_indent` ASCII spaces to the var_dump sink in
/// <=64-byte chunks (the `_pr_spaces` pad is 64 bytes wide).
///
/// Every var_dump line emitter calls this first, which is what makes one set of
/// unindented body literals serve every nesting depth. It clobbers only the
/// scratch/argument registers `x0`-`x2` (x86_64: `rax`/`rcx`/`rsi`/`rdx`/`r11`)
/// and keeps its loop counter on the stack, so callers may park a pending value
/// in `x9` (or an rbp-relative slot) across the call. Takes no arguments.
pub fn emit_var_dump_pad(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_pad_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vd_pad ---");
    emitter.label_global("__rt_vd_pad");

    emitter.instruction("sub sp, sp, #32");                                     // allocate the pad frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the pad frame pointer
    abi::emit_symbol_address(emitter, "x0", "_vd_indent");                      // resolve the current-indent global
    emitter.instruction("ldr x0, [x0]");                                        // load the indent width in spaces
    emitter.instruction("str x0, [sp, #0]");                                    // remaining space count

    emitter.label("__rt_vd_pad_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the remaining count
    emitter.instruction("cmp x0, #0");                                          // any spaces left to write?
    emitter.instruction("b.le __rt_vd_pad_done");                               // none → finish
    emitter.instruction("mov x2, #64");                                         // the pad buffer is 64 bytes wide
    emitter.instruction("cmp x0, x2");                                          // remaining vs the chunk cap
    emitter.instruction("csel x2, x0, x2, lt");                                 // chunk len = min(remaining, 64)
    emitter.instruction("sub x0, x0, x2");                                      // remaining -= chunk
    emitter.instruction("str x0, [sp, #0]");                                    // save the decremented count
    abi::emit_symbol_address(emitter, "x1", "_pr_spaces");                      // buffer = the shared 64-space pad
    emitter.instruction("bl __rt_vd_write");                                    // write the space chunk through the var_dump sink
    emitter.instruction("b __rt_vd_pad_loop");                                  // continue padding

    emitter.label("__rt_vd_pad_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the pad frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 variant of `__rt_vd_pad`.
fn emit_var_dump_pad_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: vd_pad ---");
    emitter.label_global("__rt_vd_pad");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the pad frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the pad frame
    abi::emit_symbol_address(emitter, "rax", "_vd_indent");                     // resolve the current-indent global
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // load the indent width in spaces
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // remaining space count

    emitter.label("__rt_vd_pad_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the remaining count
    emitter.instruction("cmp rax, 0");                                          // any spaces left to write?
    emitter.instruction("jle __rt_vd_pad_done_x86");                            // none → finish
    emitter.instruction("mov rdx, 64");                                         // the pad buffer is 64 bytes wide
    emitter.instruction("cmp rax, 64");                                         // remaining vs the chunk cap
    emitter.instruction("cmovl rdx, rax");                                      // chunk len = min(remaining, 64)
    emitter.instruction("sub rax, rdx");                                        // remaining -= chunk
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the decremented count
    abi::emit_symbol_address(emitter, "rsi", "_pr_spaces");                     // buffer = the shared 64-space pad
    emitter.instruction("call __rt_vd_write");                                  // write the space chunk through the var_dump sink
    emitter.instruction("jmp __rt_vd_pad_loop_x86");                            // continue padding

    emitter.label("__rt_vd_pad_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the pad frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_vd_indent_push` / `__rt_vd_indent_pop`: widen / narrow `_vd_indent` by
/// one PHP nesting level (2 spaces) around a nested container walk.
///
/// Both are leaf helpers (no `bl`/`call`, so `x30` is untouched) that clobber
/// only `x9`/`x10` (x86_64: `rax`/`rcx`).
pub fn emit_var_dump_indent_step(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: vd_indent_push/pop ---");
        emitter.label_global("__rt_vd_indent_push");
        abi::emit_symbol_address(emitter, "rax", "_vd_indent");                 // resolve the current-indent global
        emitter.instruction("mov rcx, QWORD PTR [rax]");                        // load the current indent
        emitter.instruction("add rcx, 2");                                      // one PHP nesting level = 2 spaces
        emitter.instruction("mov QWORD PTR [rax], rcx");                        // store the widened indent
        emitter.instruction("ret");                                             // return to caller
        emitter.label_global("__rt_vd_indent_pop");
        abi::emit_symbol_address(emitter, "rax", "_vd_indent");                 // resolve the current-indent global
        emitter.instruction("mov rcx, QWORD PTR [rax]");                        // load the current indent
        emitter.instruction("sub rcx, 2");                                      // leave the nesting level
        emitter.instruction("mov QWORD PTR [rax], rcx");                        // store the narrowed indent
        emitter.instruction("ret");                                             // return to caller
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vd_indent_push/pop ---");
    emitter.label_global("__rt_vd_indent_push");
    abi::emit_symbol_address(emitter, "x9", "_vd_indent");                      // resolve the current-indent global
    emitter.instruction("ldr x10, [x9]");                                       // load the current indent
    emitter.instruction("add x10, x10, #2");                                    // one PHP nesting level = 2 spaces
    emitter.instruction("str x10, [x9]");                                       // store the widened indent
    emitter.instruction("ret");                                                 // return to caller
    emitter.label_global("__rt_vd_indent_pop");
    abi::emit_symbol_address(emitter, "x9", "_vd_indent");                      // resolve the current-indent global
    emitter.instruction("ldr x10, [x9]");                                       // load the current indent
    emitter.instruction("sub x10, x10, #2");                                    // leave the nesting level
    emitter.instruction("str x10, [x9]");                                       // store the narrowed indent
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_open_container`: emit `<indent>array(N) {\n` for a nested
/// array or hash, where `N` is the element count at header offset 0 (the layout
/// indexed arrays and hash tables share — see `__rt_hash_count`).
/// Input: AArch64 x0 / x86_64 rdi = container pointer.
pub fn emit_var_dump_open_container(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_open_container_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_open_container ---");
    emitter.label_global("__rt_var_dump_open_container");

    emitter.instruction("sub sp, sp, #32");                                     // allocate the header frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the header frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the container pointer across the writes

    emitter.instruction("bl __rt_vd_pad");                                      // indent the value line
    abi::emit_symbol_address(emitter, "x1", "_vd_array_prefix");                // load the `array(` prefix
    emitter.instruction("mov x2, #6");                                          // len("array(") = 6
    emitter.instruction("bl __rt_vd_write");                                    // write `array(`
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the container pointer
    emitter.instruction("ldr x0, [x9]");                                        // element count from header offset 0
    emitter.instruction("bl __rt_itoa");                                        // x1=digits ptr, x2=digits len
    emitter.instruction("bl __rt_vd_write");                                    // write the count digits
    abi::emit_symbol_address(emitter, "x1", "_vd_brace_open");                  // load the `) {\n` opener
    emitter.instruction("mov x2, #4");                                          // len(") {\n") = 4
    emitter.instruction("bl __rt_vd_write");                                    // write `) {\n`

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the header frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 nested-container header helper.
fn emit_var_dump_open_container_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_open_container ---");
    emitter.label_global("__rt_var_dump_open_container");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the header frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the header frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the container pointer across the writes

    emitter.instruction("call __rt_vd_pad");                                    // indent the value line
    abi::emit_symbol_address(emitter, "rsi", "_vd_array_prefix");               // load the `array(` prefix
    emitter.instruction("mov edx, 6");                                          // len("array(") = 6
    emitter.instruction("call __rt_vd_write");                                  // write `array(`
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the container pointer
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // element count from header offset 0
    emitter.instruction("call __rt_itoa");                                      // rax=digits ptr, rdx=digits len
    emitter.instruction("mov rsi, rax");                                        // digits ptr → write buffer
    emitter.instruction("call __rt_vd_write");                                  // write the count digits
    abi::emit_symbol_address(emitter, "rsi", "_vd_brace_open");                 // load the `) {\n` opener
    emitter.instruction("mov edx, 4");                                          // len(") {\n") = 4
    emitter.instruction("call __rt_vd_write");                                  // write `) {\n`

    emitter.instruction("add rsp, 16");                                         // release the header frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_close_container`: emit `<indent>}\n`, closing a nested
/// array/hash at the indent of the value line that opened it. Takes no arguments.
pub fn emit_var_dump_close_container(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: var_dump_close_container ---");
        emitter.label_global("__rt_var_dump_close_container");
        emitter.instruction("push rbp");                                        // save caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish the footer frame pointer
        emitter.instruction("call __rt_vd_pad");                                // indent the closing brace
        abi::emit_symbol_address(emitter, "rsi", "_vd_brace_close");            // load the `}\n` closer
        emitter.instruction("mov edx, 2");                                      // len("}\n") = 2
        emitter.instruction("call __rt_vd_write");                              // write `}\n`
        emitter.instruction("pop rbp");                                         // restore caller frame pointer
        emitter.instruction("ret");                                             // return to caller
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_close_container ---");
    emitter.label_global("__rt_var_dump_close_container");
    emitter.instruction("sub sp, sp, #16");                                     // allocate the footer frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the footer frame pointer
    emitter.instruction("bl __rt_vd_pad");                                      // indent the closing brace
    abi::emit_symbol_address(emitter, "x1", "_vd_brace_close");                 // load the `}\n` closer
    emitter.instruction("mov x2, #2");                                          // len("}\n") = 2
    emitter.instruction("bl __rt_vd_write");                                    // write `}\n`
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the footer frame
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_value`: render ONE PHP value as a full var_dump line at the
/// current `_vd_indent`.
///
/// Scalars delegate to the matching `__rt_var_dump_emit_*_line` helper. Tags 4
/// (indexed array) and 5 (hash) open a nested `array(N) {` block, bump the
/// indent, recurse into `__rt_var_dump_indexed` / `__rt_var_dump_hash`, then
/// restore the indent and close the block — the mutual recursion is what gives
/// arbitrary nesting depth. Tag 6 first offers the object to the program-owned
/// ext/date dispatcher, then falls back to the generic property walker. Tag 7
/// unboxes a Mixed cell and redispatches. A null container/cell pointer, tag 8
/// (null), and the currently unsupported tag 9 (resource) all render `NULL`.
///
/// Input: AArch64 x0=tag x1=lo x2=hi / x86_64 rdi=tag rsi=lo rdx=hi.
pub fn emit_var_dump_value(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_value_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_value ---");
    emitter.label_global("__rt_var_dump_value");

    emitter.instruction("cmp x0, #11");                                         // inline TaggedScalar property descriptor?
    emitter.instruction("b.ne __rt_vd_value_input_ready");                     // ordinary tags already use canonical value words
    emitter.instruction("mov x0, x2");                                         // dispatch using the slot's int/null runtime tag
    emitter.instruction("mov x2, xzr");                                        // tagged scalar payloads have no third word
    emitter.label("__rt_vd_value_input_ready");

    // Frame (48 bytes): [0]lo [8]hi [32]x29 [40]x30.
    emitter.instruction("sub sp, sp, #48");                                     // allocate the value frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the value frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the value low word
    emitter.instruction("str x2, [sp, #8]");                                    // save the value high word

    emitter.instruction("cmp x0, #7");                                          // boxed Mixed cell?
    emitter.instruction("b.eq __rt_vd_val_mixed");                              // unbox then redispatch
    emitter.instruction("cmp x0, #0");                                          // tag 0 = int
    emitter.instruction("b.eq __rt_vd_val_int");                                // render the integer line
    emitter.instruction("cmp x0, #1");                                          // tag 1 = string
    emitter.instruction("b.eq __rt_vd_val_str");                                // render the string line
    emitter.instruction("cmp x0, #2");                                          // tag 2 = float
    emitter.instruction("b.eq __rt_vd_val_flt");                                // render the float line
    emitter.instruction("cmp x0, #3");                                          // tag 3 = bool
    emitter.instruction("b.eq __rt_vd_val_bool");                               // render the bool line
    emitter.instruction("cmp x0, #4");                                          // tag 4 = indexed array
    emitter.instruction("b.eq __rt_vd_val_arr");                                // recurse into the indexed walker
    emitter.instruction("cmp x0, #5");                                          // tag 5 = hash
    emitter.instruction("b.eq __rt_vd_val_hash");                               // recurse into the hash walker
    emitter.instruction("cmp x0, #6");                                          // tag 6 = object
    emitter.instruction("b.eq __rt_vd_val_obj");                                // recurse into the object walker
    emitter.instruction("b __rt_vd_val_null");                                  // tag 8 null / 9 resource → NULL

    emitter.label("__rt_vd_val_int");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the integer payload
    emitter.instruction("bl __rt_var_dump_emit_int_line");                      // emit `<indent>int(VAL)\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_str");
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the string ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the string len
    emitter.instruction("bl __rt_var_dump_emit_string_line");                   // emit `<indent>string(LEN) "VAL"\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_flt");
    emitter.instruction("ldr d0, [sp, #0]");                                    // reload the float bit pattern
    emitter.instruction("bl __rt_var_dump_emit_float_line");                    // emit `<indent>float(VAL)\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_bool");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the bool payload
    emitter.instruction("bl __rt_var_dump_emit_bool_line");                     // emit `<indent>bool(true|false)\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_arr");
    emitter.instruction("ldr x0, [sp, #0]");                                    // nested indexed-array pointer
    emitter.instruction("cbz x0, __rt_vd_val_null");                            // defensive: a null container renders NULL
    emitter.instruction("bl __rt_var_dump_open_container");                     // write `<indent>array(N) {\n`
    emitter.instruction("bl __rt_vd_indent_push");                              // entries sit one level deeper
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the nested array pointer
    emitter.instruction("bl __rt_var_dump_indexed");                            // recurse into the indexed walker
    emitter.instruction("bl __rt_vd_indent_pop");                               // back to the value-line indent
    emitter.instruction("bl __rt_var_dump_close_container");                    // write `<indent>}\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_hash");
    emitter.instruction("ldr x0, [sp, #0]");                                    // nested hash pointer
    emitter.instruction("cbz x0, __rt_vd_val_null");                            // defensive: a null container renders NULL
    emitter.instruction("bl __rt_var_dump_open_container");                     // write `<indent>array(N) {\n`
    emitter.instruction("bl __rt_vd_indent_push");                              // entries sit one level deeper
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the nested hash pointer
    emitter.instruction("bl __rt_var_dump_hash");                               // recurse into the hash walker
    emitter.instruction("bl __rt_vd_indent_pop");                               // back to the value-line indent
    emitter.instruction("bl __rt_var_dump_close_container");                    // write `<indent>}\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_obj");
    emitter.instruction("ldr x0, [sp, #0]");                                    // nested object pointer
    emitter.instruction("cbz x0, __rt_vd_val_null");                            // defensive: a null instance renders NULL
    // PHP renders an enum case as `enum(E::C)`, never as an object body, so the
    // enum test happens before anything object-shaped is written.
    emitter.instruction("bl __rt_obj_enum_name_offset");                        // x0 = enum `name` slot offset, or -1 for a plain class
    emitter.instruction("cmp x0, #0");                                          // is this instance an enum case?
    emitter.instruction("b.lt __rt_vd_val_obj_plain");                          // a plain class falls through to the object body
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the enum instance pointer
    emitter.instruction("bl __rt_var_dump_emit_enum_line");                     // emit `<indent>enum(E::C)\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered
    emitter.label("__rt_vd_val_obj_plain");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __elephc_var_dump_datetime_object");                // let program-specific ext/date handlers render virtual fields
    emitter.instruction("cbnz x0, __rt_vd_val_done");                           // the special handler emitted the complete object block
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer after special dispatch
    emitter.instruction("bl __rt_vd_seen_find");                                // is this object already on the walk stack?
    emitter.instruction("cbnz x0, __rt_vd_val_recursion");                      // PHP renders a revisited object as *RECURSION*
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __rt_vd_seen_push");                                // mark the object as being walked
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __rt_var_dump_open_object");                        // write `<indent>object(NAME) (N) {\n`
    emitter.instruction("bl __rt_vd_indent_push");                              // properties sit one level deeper
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __rt_var_dump_object");                             // recurse into the property walker
    emitter.instruction("bl __rt_vd_indent_pop");                               // back to the value-line indent
    emitter.instruction("bl __rt_var_dump_close_container");                    // write `<indent>}\n`
    emitter.instruction("bl __rt_vd_seen_pop");                                 // the object is no longer on the walk stack
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_recursion");
    emitter.instruction("bl __rt_var_dump_emit_recursion_line");                // emit `<indent>*RECURSION*\n`
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_mixed");
    emitter.instruction("ldr x0, [sp, #0]");                                    // boxed Mixed cell pointer
    emitter.instruction("cbz x0, __rt_vd_val_null");                            // defensive: a null cell renders NULL
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=inner tag, x1=lo, x2=hi
    emitter.instruction("bl __rt_var_dump_value");                              // redispatch the unboxed scalar/container
    emitter.instruction("b __rt_vd_val_done");                                  // value rendered

    emitter.label("__rt_vd_val_null");
    emitter.instruction("bl __rt_var_dump_emit_null_line");                     // emit `<indent>NULL\n`

    emitter.label("__rt_vd_val_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the value frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 single-value var_dump renderer.
fn emit_var_dump_value_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_value ---");
    emitter.label_global("__rt_var_dump_value");

    emitter.instruction("cmp rdi, 11");                                        // inline TaggedScalar property descriptor?
    emitter.instruction("jne __rt_vd_value_input_ready_x86");                  // ordinary tags already use canonical value words
    emitter.instruction("mov rdi, rdx");                                       // dispatch using the slot's int/null runtime tag
    emitter.instruction("xor edx, edx");                                       // tagged scalar payloads have no third word
    emitter.label("__rt_vd_value_input_ready_x86");

    // rbp-relative frame: [-8]lo [-16]hi.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the value frame pointer
    emitter.instruction("sub rsp, 32");                                         // allocate the value frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the value low word
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the value high word
    emitter.instruction("mov rax, rdi");                                        // tag → dispatch register

    emitter.instruction("cmp rax, 7");                                          // boxed Mixed cell?
    emitter.instruction("je __rt_vd_val_mixed_x86");                            // unbox then redispatch
    emitter.instruction("cmp rax, 0");                                          // tag 0 = int
    emitter.instruction("je __rt_vd_val_int_x86");                              // render the integer line
    emitter.instruction("cmp rax, 1");                                          // tag 1 = string
    emitter.instruction("je __rt_vd_val_str_x86");                              // render the string line
    emitter.instruction("cmp rax, 2");                                          // tag 2 = float
    emitter.instruction("je __rt_vd_val_flt_x86");                              // render the float line
    emitter.instruction("cmp rax, 3");                                          // tag 3 = bool
    emitter.instruction("je __rt_vd_val_bool_x86");                             // render the bool line
    emitter.instruction("cmp rax, 4");                                          // tag 4 = indexed array
    emitter.instruction("je __rt_vd_val_arr_x86");                              // recurse into the indexed walker
    emitter.instruction("cmp rax, 5");                                          // tag 5 = hash
    emitter.instruction("je __rt_vd_val_hash_x86");                             // recurse into the hash walker
    emitter.instruction("cmp rax, 6");                                          // tag 6 = object
    emitter.instruction("je __rt_vd_val_obj_x86");                              // recurse into the object walker
    emitter.instruction("jmp __rt_vd_val_null_x86");                            // tag 8 null / 9 resource → NULL

    emitter.label("__rt_vd_val_int_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the integer payload
    emitter.instruction("call __rt_var_dump_emit_int_line");                    // emit `<indent>int(VAL)\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_str_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the string ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the string len
    emitter.instruction("call __rt_var_dump_emit_string_line");                 // emit `<indent>string(LEN) "VAL"\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_flt_x86");
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // reload the float bit pattern
    emitter.instruction("call __rt_var_dump_emit_float_line");                  // emit `<indent>float(VAL)\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_bool_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the bool payload
    emitter.instruction("call __rt_var_dump_emit_bool_line");                   // emit `<indent>bool(true|false)\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_arr_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // nested indexed-array pointer
    emitter.instruction("test rdi, rdi");                                       // defensive null-container check
    emitter.instruction("jz __rt_vd_val_null_x86");                             // a null container renders NULL
    emitter.instruction("call __rt_var_dump_open_container");                   // write `<indent>array(N) {\n`
    emitter.instruction("call __rt_vd_indent_push");                            // entries sit one level deeper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the nested array pointer
    emitter.instruction("call __rt_var_dump_indexed");                          // recurse into the indexed walker
    emitter.instruction("call __rt_vd_indent_pop");                             // back to the value-line indent
    emitter.instruction("call __rt_var_dump_close_container");                  // write `<indent>}\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_hash_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // nested hash pointer
    emitter.instruction("test rdi, rdi");                                       // defensive null-container check
    emitter.instruction("jz __rt_vd_val_null_x86");                             // a null container renders NULL
    emitter.instruction("call __rt_var_dump_open_container");                   // write `<indent>array(N) {\n`
    emitter.instruction("call __rt_vd_indent_push");                            // entries sit one level deeper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the nested hash pointer
    emitter.instruction("call __rt_var_dump_hash");                             // recurse into the hash walker
    emitter.instruction("call __rt_vd_indent_pop");                             // back to the value-line indent
    emitter.instruction("call __rt_var_dump_close_container");                  // write `<indent>}\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_obj_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // nested object pointer
    emitter.instruction("test rdi, rdi");                                       // defensive null-instance check
    emitter.instruction("jz __rt_vd_val_null_x86");                             // a null instance renders NULL
    // PHP renders an enum case as `enum(E::C)`, never as an object body, so the
    // enum test happens before anything object-shaped is written.
    emitter.instruction("call __rt_obj_enum_name_offset");                      // rax = enum `name` slot offset, or -1 for a plain class
    emitter.instruction("cmp rax, 0");                                          // is this instance an enum case?
    emitter.instruction("jl __rt_vd_val_obj_plain_x86");                        // a plain class falls through to the object body
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the enum instance pointer
    emitter.instruction("call __rt_var_dump_emit_enum_line");                   // emit `<indent>enum(E::C)\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered
    emitter.label("__rt_vd_val_obj_plain_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __elephc_var_dump_datetime_object");              // let program-specific ext/date handlers render virtual fields
    emitter.instruction("test rax, rax");                                       // did the special handler consume the object?
    emitter.instruction("jnz __rt_vd_val_done_x86");                            // the handler emitted the complete object block
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer after special dispatch
    emitter.instruction("call __rt_vd_seen_find");                              // is this object already on the walk stack?
    emitter.instruction("test rax, rax");                                       // did the guard report a revisit?
    emitter.instruction("jnz __rt_vd_val_recursion_x86");                       // PHP renders a revisited object as *RECURSION*
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __rt_vd_seen_push");                              // mark the object as being walked
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __rt_var_dump_open_object");                      // write `<indent>object(NAME) (N) {\n`
    emitter.instruction("call __rt_vd_indent_push");                            // properties sit one level deeper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __rt_var_dump_object");                           // recurse into the property walker
    emitter.instruction("call __rt_vd_indent_pop");                             // back to the value-line indent
    emitter.instruction("call __rt_var_dump_close_container");                  // write `<indent>}\n`
    emitter.instruction("call __rt_vd_seen_pop");                               // the object is no longer on the walk stack
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_recursion_x86");
    emitter.instruction("call __rt_var_dump_emit_recursion_line");              // emit `<indent>*RECURSION*\n`
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_mixed_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // boxed Mixed cell pointer → RAX
    emitter.instruction("test rax, rax");                                       // defensive null-cell check
    emitter.instruction("jz __rt_vd_val_null_x86");                             // a null cell renders NULL
    emitter.instruction("call __rt_mixed_unbox");                               // rax=inner tag, rdi=lo, rdx=hi
    emitter.instruction("mov rsi, rdi");                                        // unboxed lo → value low argument
    emitter.instruction("mov rdi, rax");                                        // unboxed tag → value tag argument
    emitter.instruction("call __rt_var_dump_value");                            // redispatch the unboxed scalar/container
    emitter.instruction("jmp __rt_vd_val_done_x86");                            // value rendered

    emitter.label("__rt_vd_val_null_x86");
    emitter.instruction("call __rt_var_dump_emit_null_line");                   // emit `<indent>NULL\n`

    emitter.label("__rt_vd_val_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the value frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_indexed`: the universal indexed-array walker. Emits one
/// `<indent>[N]=>\n<indent>VALUE` block per element, self-dispatching on the
/// array's runtime value_type stamp (`[arr-8]` byte 1, low nibble: 0=int, 1=str,
/// 2=float, 3=bool, 4=array, 5=hash, 6=object, 7=mixed-cell) rather than on a
/// static element type — so an array whose static type and runtime slots
/// disagree still walks its real layout, and nested containers recurse through
/// `__rt_var_dump_value`.
///
/// Input: AArch64 x0 / x86_64 rdi = array pointer.
pub fn emit_var_dump_indexed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_indexed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_indexed ---");
    emitter.label_global("__rt_var_dump_indexed");

    // Frame (64 bytes): [0]arr [8]index [16]count [24]stamp [48]x29 [56]x30.
    emitter.instruction("sub sp, sp, #64");                                     // allocate the indexed-walk frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the walk frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the array pointer
    emitter.instruction("str xzr, [sp, #8]");                                   // index = 0
    emitter.instruction("ldr x9, [x0]");                                        // load the element count from the header
    emitter.instruction("str x9, [sp, #16]");                                   // save the element count
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed array kind word
    emitter.instruction("lsr x9, x9, #8");                                      // shift the value_type stamp into the low byte
    emitter.instruction("and x9, x9, #0x0f");                                   // isolate the value_type field (low nibble), dropping the COW bit
    emitter.instruction("str x9, [sp, #24]");                                   // save the element value_type stamp

    emitter.label("__rt_vd_idx_loop");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the current index
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the element count
    emitter.instruction("cmp x9, x10");                                         // processed every element?
    emitter.instruction("b.ge __rt_vd_idx_done");                               // walk complete

    emitter.instruction("ldr x11, [sp, #8]");                                   // index → the key helper's x11 input
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // emit `<indent>[N]=>\n`

    emitter.instruction("ldr x12, [sp, #24]");                                  // reload the element stamp
    emitter.instruction("ldr x13, [sp, #0]");                                   // reload the array pointer
    emitter.instruction("ldr x14, [sp, #8]");                                   // reload the current index
    emitter.instruction("cmp x12, #1");                                         // string elements use a 16-byte stride
    emitter.instruction("b.eq __rt_vd_idx_str");                                // handle string elements
    emitter.instruction("cmp x12, #7");                                         // mixed elements are boxed cells
    emitter.instruction("b.eq __rt_vd_idx_mixed");                              // handle mixed cells

    // 8-byte-stride elements: int(0) / float(2) / bool(3) / array(4) / hash(5) / object(6).
    emitter.instruction("add x15, x14, #3");                                    // skip the 24-byte (3-quad) header
    emitter.instruction("ldr x1, [x13, x15, lsl #3]");                          // load the raw element word → value low
    emitter.instruction("mov x0, x12");                                         // tag = the array stamp
    emitter.instruction("mov x2, #0");                                          // high word unused for 8-byte elements
    emitter.instruction("bl __rt_var_dump_value");                              // render the element line
    emitter.instruction("b __rt_vd_idx_next");                                  // advance to the next element

    emitter.label("__rt_vd_idx_str");
    emitter.instruction("lsl x15, x14, #4");                                    // index * 16
    emitter.instruction("add x15, x15, #24");                                   // element base offset = 24 + index*16
    emitter.instruction("add x15, x13, x15");                                   // element address
    emitter.instruction("ldr x1, [x15]");                                       // string ptr → value low
    emitter.instruction("ldr x2, [x15, #8]");                                   // string len → value high
    emitter.instruction("mov x0, #1");                                          // tag = string
    emitter.instruction("bl __rt_var_dump_value");                              // render the element line
    emitter.instruction("b __rt_vd_idx_next");                                  // advance to the next element

    emitter.label("__rt_vd_idx_mixed");
    emitter.instruction("add x15, x14, #3");                                    // skip the 24-byte (3-quad) header
    emitter.instruction("ldr x15, [x13, x15, lsl #3]");                         // load the Mixed cell pointer
    emitter.instruction("cbz x15, __rt_vd_idx_mixed_null");                     // a null cell renders NULL
    emitter.instruction("ldr x0, [x15]");                                       // cell tag → value tag
    emitter.instruction("ldr x1, [x15, #8]");                                   // cell low word → value low
    emitter.instruction("ldr x2, [x15, #16]");                                  // cell high word → value high
    emitter.instruction("bl __rt_var_dump_value");                              // render the element line
    emitter.instruction("b __rt_vd_idx_next");                                  // advance to the next element

    emitter.label("__rt_vd_idx_mixed_null");
    emitter.instruction("bl __rt_var_dump_emit_null_line");                     // emit `<indent>NULL\n`

    emitter.label("__rt_vd_idx_next");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the index
    emitter.instruction("add x9, x9, #1");                                      // advance the index
    emitter.instruction("str x9, [sp, #8]");                                    // save the updated index
    emitter.instruction("b __rt_vd_idx_loop");                                  // continue scanning

    emitter.label("__rt_vd_idx_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the indexed-walk frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 universal indexed-array var_dump walker.
fn emit_var_dump_indexed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_indexed ---");
    emitter.label_global("__rt_var_dump_indexed");

    // rbp-relative frame: [-8]arr [-16]index [-24]count [-32]stamp.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the walk frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate the indexed-walk frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // index = 0
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the element count from the header
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the element count
    emitter.instruction("mov rax, QWORD PTR [rdi - 8]");                        // load the packed array kind word
    emitter.instruction("shr rax, 8");                                          // shift the value_type stamp into the low byte
    emitter.instruction("and rax, 0x0f");                                       // isolate the value_type field (low nibble), dropping the COW bit
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the element value_type stamp

    emitter.label("__rt_vd_idx_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the current index
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the element count
    emitter.instruction("cmp rax, rcx");                                        // processed every element?
    emitter.instruction("jge __rt_vd_idx_done_x86");                            // walk complete

    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // index → the key helper's rdi input
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // emit `<indent>[N]=>\n`

    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the element stamp
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the current index
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
    emitter.instruction("call __rt_var_dump_value");                            // render the element line
    emitter.instruction("jmp __rt_vd_idx_next_x86");                            // advance to the next element

    emitter.label("__rt_vd_idx_str_x86");
    emitter.instruction("mov rax, r11");                                        // copy the index
    emitter.instruction("shl rax, 4");                                          // index * 16
    emitter.instruction("add rax, 24");                                         // element base offset = 24 + index*16
    emitter.instruction("add rax, r9");                                         // element address
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // string ptr → value low
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // string len → value high
    emitter.instruction("mov rdi, 1");                                          // tag = string
    emitter.instruction("call __rt_var_dump_value");                            // render the element line
    emitter.instruction("jmp __rt_vd_idx_next_x86");                            // advance to the next element

    emitter.label("__rt_vd_idx_mixed_x86");
    emitter.instruction("mov rax, r11");                                        // copy the index
    emitter.instruction("add rax, 3");                                          // skip the 24-byte (3-quad) header
    emitter.instruction("mov rax, QWORD PTR [r9 + rax * 8]");                   // load the Mixed cell pointer
    emitter.instruction("test rax, rax");                                       // null cell?
    emitter.instruction("jz __rt_vd_idx_mixed_null_x86");                       // a null cell renders NULL
    emitter.instruction("mov rdi, QWORD PTR [rax]");                            // cell tag → value tag
    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                        // cell low word → value low
    emitter.instruction("mov rdx, QWORD PTR [rax + 16]");                       // cell high word → value high
    emitter.instruction("call __rt_var_dump_value");                            // render the element line
    emitter.instruction("jmp __rt_vd_idx_next_x86");                            // advance to the next element

    emitter.label("__rt_vd_idx_mixed_null_x86");
    emitter.instruction("call __rt_var_dump_emit_null_line");                   // emit `<indent>NULL\n`

    emitter.label("__rt_vd_idx_next_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the index
    emitter.instruction("add rax, 1");                                          // advance the index
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the updated index
    emitter.instruction("jmp __rt_vd_idx_loop_x86");                            // continue scanning

    emitter.label("__rt_vd_idx_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the indexed-walk frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
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

/// `__rt_var_dump_emit_string_key`: emit `<indent>["KEY"]=>\n` for a string hash key.
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

    emitter.instruction("bl __rt_vd_pad");                                      // write `_vd_indent` spaces before the key

    // Emit `["`
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_str_key_open");
    emitter.instruction("mov x2, #2");                                          // len("[\"") = 2
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

    // Write the raw key bytes
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload key ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload key len
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

    // Emit `"]=>\n`
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_vd_str_key_close");
    emitter.instruction("mov x2, #5");                                          // len("\"]=>\n") = 5
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)

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

    emitter.instruction("call __rt_vd_pad");                                    // write `_vd_indent` spaces before the key
    abi::emit_symbol_address(emitter, "rsi", "_vd_str_key_open");               // load runtime data address
    emitter.instruction("mov edx, 2");                                          // len("[\"") = 2
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload key ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload key len
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    abi::emit_symbol_address(emitter, "rsi", "_vd_str_key_close");              // load runtime data address
    emitter.instruction("mov edx, 5");                                          // len("\"]=>\n") = 5
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)

    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_hash`: walk an associative array (hash) and emit one
/// `<indent>[KEY]=>\n<indent>TYPE(VAL)\n` block per entry, matching PHP's var_dump body.
/// Integer keys render as `[N]`, string keys as `["KEY"]`. Every value is handed
/// to `__rt_var_dump_value`, which unboxes Mixed cells and recurses into nested
/// arrays/hashes, so a hash nests to arbitrary depth. Objects still render as
/// `NULL` (see the module preamble).
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
    emitter.instruction("bl __rt_var_dump_emit_string_key");                    // emit `<indent>["KEY"]=>\n`
    emitter.instruction("b __rt_vd_hash_after_key");                            // continue to the value line
    emitter.label("__rt_vd_hash_int_key");
    emitter.instruction("ldr x11, [sp, #32]");                                  // integer key payload → indexed-key helper's x11 input
    emitter.instruction("bl __rt_var_dump_emit_indexed_key");                   // emit `<indent>[N]=>\n`

    emitter.label("__rt_vd_hash_after_key");
    // -- render the value line; __rt_var_dump_value unboxes Mixed cells (tag 7)
    //    and recurses into nested arrays/hashes (tags 4/5) on its own --
    emitter.instruction("ldr x0, [sp, #64]");                                   // value tag → value renderer
    emitter.instruction("ldr x1, [sp, #48]");                                   // value low word → value renderer
    emitter.instruction("ldr x2, [sp, #56]");                                   // value high word → value renderer
    emitter.instruction("bl __rt_var_dump_value");                              // emit `<indent>TYPE(VAL)\n` (recursing when needed)

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
    emitter.instruction("call __rt_var_dump_emit_string_key");                  // emit `<indent>["KEY"]=>\n`
    emitter.instruction("jmp __rt_vd_hash_after_key_x86");                      // continue to the value line
    emitter.label("__rt_vd_hash_int_key_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // integer key payload → indexed-key helper's rdi
    emitter.instruction("call __rt_var_dump_emit_indexed_key");                 // emit `<indent>[N]=>\n`

    emitter.label("__rt_vd_hash_after_key_x86");
    // -- render the value line; __rt_var_dump_value unboxes Mixed cells (tag 7)
    //    and recurses into nested arrays/hashes (tags 4/5) on its own --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // value tag → value renderer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // value low word → value renderer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // value high word → value renderer
    emitter.instruction("call __rt_var_dump_value");                            // emit `<indent>TYPE(VAL)\n` (recursing when needed)

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

/// Emits `__rt_vd_write`: the var_dump walker terminal-write indirection.
///
/// Inputs match the walkers' pre-syscall register layout (AArch64 `x1`=buf,
/// `x2`=len / x86_64 `rsi`=buf, `rdx`=len). Routes the bytes through
/// `__rt_stdout_write` so output-buffering (`ob_*`), print_r return-mode, and
/// `--web` capture all see var_dump output, while preserving every register the
/// walkers expect a raw `write` syscall to leave untouched (AArch64 `x3`-`x15`
/// plus the float walker's pending `d0`; x86_64 `rdi`/`rsi`/`rdx`/`r8`/`r9`/`r10`).
pub fn emit_var_dump_write(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_write_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vd_write ---");
    emitter.label_global("__rt_vd_write");
    emitter.instruction("sub sp, sp, #128");                                    // allocate the register-preserving vd_write frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish the vd_write frame pointer
    emitter.instruction("stp x3, x4, [sp, #0]");                                // preserve walker state the raw syscall left untouched
    emitter.instruction("stp x5, x6, [sp, #16]");                               // preserve walker state the raw syscall left untouched
    emitter.instruction("stp x7, x8, [sp, #32]");                               // preserve walker state the raw syscall left untouched
    emitter.instruction("stp x9, x10, [sp, #48]");                              // preserve walker state the raw syscall left untouched
    emitter.instruction("stp x11, x12, [sp, #64]");                             // preserve walker state the raw syscall left untouched
    emitter.instruction("stp x13, x14, [sp, #80]");                             // preserve walker state the raw syscall left untouched
    emitter.instruction("str x15, [sp, #96]");                                  // preserve walker state the raw syscall left untouched
    emitter.instruction("str d0, [sp, #104]");                                  // preserve the float walker's pending d0 across the funnel call
    emitter.instruction("mov x0, x1");                                          // __rt_stdout_write buf arg = incoming buf pointer
    emitter.instruction("mov x1, x2");                                          // __rt_stdout_write len arg = incoming length
    emitter.instruction("bl __rt_stdout_write");                                // write through the ob/print_r/web-aware stdout funnel
    emitter.instruction("ldp x3, x4, [sp, #0]");                                // restore preserved walker state
    emitter.instruction("ldp x5, x6, [sp, #16]");                               // restore preserved walker state
    emitter.instruction("ldp x7, x8, [sp, #32]");                               // restore preserved walker state
    emitter.instruction("ldp x9, x10, [sp, #48]");                              // restore preserved walker state
    emitter.instruction("ldp x11, x12, [sp, #64]");                             // restore preserved walker state
    emitter.instruction("ldp x13, x14, [sp, #80]");                             // restore preserved walker state
    emitter.instruction("ldr x15, [sp, #96]");                                  // restore preserved walker state
    emitter.instruction("ldr d0, [sp, #104]");                                  // restore the float walker's pending d0
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the vd_write frame
    emitter.instruction("ret");                                                 // return to the walker
}

/// Emits the Linux x86_64 variant of `__rt_vd_write`.
fn emit_var_dump_write_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: vd_write ---");
    emitter.label_global("__rt_vd_write");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the vd_write frame pointer
    emitter.instruction("push rdi");                                            // preserve walker state the raw syscall left untouched
    emitter.instruction("push rsi");                                            // preserve walker state the raw syscall left untouched
    emitter.instruction("push rdx");                                            // preserve walker state the raw syscall left untouched
    emitter.instruction("push r8");                                             // preserve walker state the raw syscall left untouched
    emitter.instruction("push r9");                                             // preserve walker state the raw syscall left untouched
    emitter.instruction("push r10");                                            // preserve walker state the raw syscall left untouched
    emitter.instruction("mov rdi, rsi");                                        // __rt_stdout_write buf arg = incoming buf pointer
    emitter.instruction("mov rsi, rdx");                                        // __rt_stdout_write len arg = incoming length
    emitter.instruction("call __rt_stdout_write");                              // write through the ob/print_r/web-aware stdout funnel
    emitter.instruction("pop r10");                                             // restore preserved walker state
    emitter.instruction("pop r9");                                              // restore preserved walker state
    emitter.instruction("pop r8");                                              // restore preserved walker state
    emitter.instruction("pop rdx");                                             // restore preserved walker state
    emitter.instruction("pop rsi");                                             // restore preserved walker state
    emitter.instruction("pop rdi");                                             // restore preserved walker state
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the walker
}
