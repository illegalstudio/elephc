//! Purpose:
//! Emits the compact PHP value formatter used by AOT `debug_print_backtrace()`.
//!
//! Called from:
//! - Per-call-site AOT backtrace frame readers emitted by `crate::codegen`.
//!
//! Key details:
//! - Input is a borrowed boxed Mixed cell and is never retained or released here.
//! - String bytes follow php-src's printable ASCII and uppercase hexadecimal escaping.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_backtrace_print_arg` for the active target.
pub(crate) fn emit_backtrace_print_arg(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_backtrace_print_arg_x86_64(emitter);
        return;
    }
    emit_backtrace_print_arg_aarch64(emitter);
}

/// Emits the AArch64 compact backtrace argument formatter.
fn emit_backtrace_print_arg_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: backtrace_print_arg ---");
    emitter.label_global("__rt_backtrace_print_arg");
    emitter.instruction("sub sp, sp, #64");                                     // reserve value, string-loop, and escape state
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve frame linkage and the return address
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame
    emitter.instruction("bl __rt_mixed_unbox");                                 // normalize nested Mixed wrappers to one tag and payload pair
    emitter.instruction("stp x0, x1, [sp]");                                    // save the concrete tag and low payload
    emitter.instruction("str x2, [sp, #16]");                                   // save the high payload word
    emitter.instruction("cmp x0, #0");                                          // integer value?
    emitter.instruction("b.eq __rt_bt_arg_int");                                // format integers as decimal digits
    emitter.instruction("cmp x0, #1");                                          // string value?
    emitter.instruction("b.eq __rt_bt_arg_string");                             // quote and escape string bytes
    emitter.instruction("cmp x0, #2");                                          // floating-point value?
    emitter.instruction("b.eq __rt_bt_arg_float");                              // format floats with the shared PHP formatter
    emitter.instruction("cmp x0, #3");                                          // boolean value?
    emitter.instruction("b.eq __rt_bt_arg_bool");                               // print true or false
    emitter.instruction("cmp x0, #4");                                          // indexed array value?
    emitter.instruction("b.eq __rt_bt_arg_array");                              // both PHP array storage shapes print as Array
    emitter.instruction("cmp x0, #5");                                          // associative array value?
    emitter.instruction("b.eq __rt_bt_arg_array");                              // share the Array spelling
    emitter.instruction("cmp x0, #6");                                          // object value?
    emitter.instruction("b.eq __rt_bt_arg_object");                             // print Object(ClassName)
    emitter.instruction("cmp x0, #8");                                          // null value?
    emitter.instruction("b.eq __rt_bt_arg_null");                               // print PHP's uppercase NULL spelling
    emitter.instruction("cmp x0, #9");                                          // resource value?
    emitter.instruction("b.eq __rt_bt_arg_resource");                           // delegate resource-id rendering
    emitter.instruction("cmp x0, #10");                                         // callable descriptor?
    emitter.instruction("b.eq __rt_bt_arg_callable");                           // expose the Closure object spelling
    abi::emit_symbol_address(emitter, "x0", "_bt_arg_unknown");
    emitter.instruction("mov x1, #7");                                          // len("Unknown")
    emitter.instruction("b __rt_bt_arg_write_done");                            // write the fallback and return

    emitter.label("__rt_bt_arg_int");
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the integer payload
    emitter.instruction("bl __rt_itoa");                                        // x1/x2 = decimal byte pair
    emitter.instruction("mov x0, x1");                                          // stdout pointer argument
    emitter.instruction("mov x1, x2");                                          // stdout length argument
    emitter.instruction("b __rt_bt_arg_write_done");                            // write the converted integer

    emitter.label("__rt_bt_arg_float");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the floating-point payload bits
    emitter.instruction("fmov d0, x9");                                         // move bits into the float argument register
    emitter.instruction("bl __rt_ftoa");                                        // x1/x2 = PHP-formatted float byte pair
    emitter.instruction("mov x0, x1");                                          // stdout pointer argument
    emitter.instruction("mov x1, x2");                                          // stdout length argument
    emitter.instruction("b __rt_bt_arg_write_done");                            // write the converted float

    emitter.label("__rt_bt_arg_bool");
    emitter.instruction("ldr x9, [sp, #8]");                                    // inspect the boolean payload
    abi::emit_symbol_address(emitter, "x0", "_bt_arg_false");
    emitter.instruction("mov x1, #5");                                          // len("false")
    emitter.instruction("cbz x9, __rt_bt_arg_write_done");                      // zero selects false
    abi::emit_symbol_address(emitter, "x0", "_bt_arg_true");
    emitter.instruction("mov x1, #4");                                          // len("true")
    emitter.instruction("b __rt_bt_arg_write_done");                            // nonzero selects true

    emitter.label("__rt_bt_arg_array");
    abi::emit_symbol_address(emitter, "x0", "_bt_arg_array");
    emitter.instruction("mov x1, #5");                                          // len("Array")
    emitter.instruction("b __rt_bt_arg_write_done");                            // write the compact array spelling

    emitter.label("__rt_bt_arg_null");
    abi::emit_symbol_address(emitter, "x0", "_bt_arg_null");
    emitter.instruction("mov x1, #4");                                          // len("NULL")
    emitter.instruction("b __rt_bt_arg_write_done");                            // write the compact null spelling

    emitter.label("__rt_bt_arg_callable");
    abi::emit_symbol_address(emitter, "x0", "_bt_arg_callable");
    emitter.instruction("mov x1, #15");                                         // len("Object(Closure)")
    emitter.instruction("b __rt_bt_arg_write_done");                            // write the callable object spelling

    emitter.label("__rt_bt_arg_resource");
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the native resource payload
    emitter.instruction("bl __rt_resource_write_stdout");                       // write Resource id #N through the shared sink
    emitter.instruction("b __rt_bt_arg_done");                                  // the resource helper already emitted the value

    emitter.label("__rt_bt_arg_object");
    abi::emit_symbol_address(emitter, "x0", "_bt_arg_object_open");
    emitter.instruction("mov x1, #7");                                          // len("Object(")
    emitter.instruction("bl __rt_stdout_write");                                // write the object prefix
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the object payload
    emitter.instruction("cbz x9, __rt_bt_arg_object_fallback");                 // malformed null objects use the generic name
    emitter.instruction("ldr x9, [x9]");                                        // load the runtime class id
    abi::emit_load_symbol_to_reg(emitter, "x10", "_class_name_count", 0);
    emitter.instruction("cmp x9, x10");                                         // is the class id inside the dense name table?
    emitter.instruction("b.hs __rt_bt_arg_object_fallback");                    // negative or out-of-range ids use object
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // select the 16-byte class-name row
    emitter.instruction("ldp x0, x1, [x10]");                                   // load class-name pointer and length
    emitter.instruction("cbnz x1, __rt_bt_arg_object_name_ready");              // non-empty metadata is authoritative
    emitter.label("__rt_bt_arg_object_fallback");
    abi::emit_symbol_address(emitter, "x0", "_unser_type_object");
    emitter.instruction("mov x1, #6");                                          // len("object")
    emitter.label("__rt_bt_arg_object_name_ready");
    emitter.instruction("bl __rt_stdout_write");                                // write the resolved class name
    abi::emit_symbol_address(emitter, "x0", "_bt_arg_close");
    emitter.instruction("mov x1, #1");                                          // len(")")
    emitter.instruction("b __rt_bt_arg_write_done");                            // close the object spelling

    emitter.label("__rt_bt_arg_string");
    abi::emit_symbol_address(emitter, "x0", "_bt_arg_quote");
    emitter.instruction("mov x1, #1");                                          // opening quote length
    emitter.instruction("bl __rt_stdout_write");                                // write the opening single quote
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the source string pointer
    emitter.instruction("ldr x10, [sp, #16]");                                  // load the source byte length
    emitter.instruction("str x9, [sp, #24]");                                   // initialize the byte cursor
    emitter.instruction("str x10, [sp, #32]");                                  // initialize the remaining-byte count
    emitter.label("__rt_bt_arg_string_loop");
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the remaining-byte count
    emitter.instruction("cbz x10, __rt_bt_arg_string_close");                   // stop after every source byte is rendered
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the current source cursor
    emitter.instruction("ldrb w11, [x9]");                                      // load one unsigned source byte
    emitter.instruction("cmp w11, #92");                                        // backslash?
    emitter.instruction("b.eq __rt_bt_arg_escape_slash");                       // duplicate backslashes
    for (value, suffix) in [(10, "n"), (9, "t"), (13, "r"), (11, "v"), (12, "f"), (27, "e")] {
        emitter.instruction(&format!("cmp w11, #{value}"));                     // compare against one named escape byte
        emitter.instruction(&format!("b.eq __rt_bt_arg_escape_{suffix}"));      // select the matching two-byte escape
    }
    emitter.instruction("cmp w11, #32");                                        // printable ASCII lower bound
    emitter.instruction("b.lo __rt_bt_arg_escape_hex");                         // control bytes use uppercase hex
    emitter.instruction("cmp w11, #126");                                       // printable ASCII upper bound
    emitter.instruction("b.hi __rt_bt_arg_escape_hex");                         // non-ASCII bytes use uppercase hex
    emitter.instruction("mov x0, x9");                                          // write directly from the source cursor
    emitter.instruction("mov x1, #1");                                          // one printable byte
    emitter.instruction("b __rt_bt_arg_string_write");                          // share cursor advancement

    for suffix in ["slash", "n", "t", "r", "v", "f", "e"] {
        emitter.label(&format!("__rt_bt_arg_escape_{suffix}"));
        abi::emit_symbol_address(emitter, "x0", &format!("_bt_arg_escape_{suffix}"));
        emitter.instruction("mov x1, #2");                                      // every named escape occupies two bytes
        emitter.instruction("b __rt_bt_arg_string_write");                      // write the escape and advance
    }

    emitter.label("__rt_bt_arg_escape_hex");
    emitter.instruction("mov w12, #92");                                        // first byte is a backslash
    emitter.instruction("strb w12, [sp, #40]");                                 // store the escape introducer
    emitter.instruction("mov w12, #120");                                       // second byte is lowercase x
    emitter.instruction("strb w12, [sp, #41]");                                 // store the hexadecimal marker
    emitter.instruction("lsr w12, w11, #4");                                    // isolate the high nibble
    emitter.instruction("cmp w12, #10");                                        // numeric or alphabetic nibble?
    emitter.instruction("b.lo __rt_bt_arg_hex_high_decimal");                   // 0 through 9 use decimal digits
    emitter.instruction("add w12, w12, #55");                                   // 10 through 15 become uppercase A through F
    emitter.instruction("b __rt_bt_arg_hex_high_ready");                        // store the converted high digit
    emitter.label("__rt_bt_arg_hex_high_decimal");
    emitter.instruction("add w12, w12, #48");                                   // 0 through 9 become ASCII digits
    emitter.label("__rt_bt_arg_hex_high_ready");
    emitter.instruction("strb w12, [sp, #42]");                                 // store the high hexadecimal digit
    emitter.instruction("and w12, w11, #15");                                   // isolate the low nibble
    emitter.instruction("cmp w12, #10");                                        // numeric or alphabetic nibble?
    emitter.instruction("b.lo __rt_bt_arg_hex_low_decimal");                    // 0 through 9 use decimal digits
    emitter.instruction("add w12, w12, #55");                                   // 10 through 15 become uppercase A through F
    emitter.instruction("b __rt_bt_arg_hex_low_ready");                         // store the converted low digit
    emitter.label("__rt_bt_arg_hex_low_decimal");
    emitter.instruction("add w12, w12, #48");                                   // 0 through 9 become ASCII digits
    emitter.label("__rt_bt_arg_hex_low_ready");
    emitter.instruction("strb w12, [sp, #43]");                                 // store the low hexadecimal digit
    emitter.instruction("add x0, sp, #40");                                     // point at the four-byte local escape
    emitter.instruction("mov x1, #4");                                          // len("\\xHH")

    emitter.label("__rt_bt_arg_string_write");
    emitter.instruction("bl __rt_stdout_write");                                // write the raw byte or selected escape
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the source cursor
    emitter.instruction("add x9, x9, #1");                                      // consume one source byte
    emitter.instruction("str x9, [sp, #24]");                                   // preserve the advanced cursor
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the remaining-byte count
    emitter.instruction("sub x10, x10, #1");                                    // account for the consumed source byte
    emitter.instruction("str x10, [sp, #32]");                                  // preserve the decremented count
    emitter.instruction("b __rt_bt_arg_string_loop");                           // render the next source byte

    emitter.label("__rt_bt_arg_string_close");
    abi::emit_symbol_address(emitter, "x0", "_bt_arg_quote");
    emitter.instruction("mov x1, #1");                                          // closing quote length

    emitter.label("__rt_bt_arg_write_done");
    emitter.instruction("bl __rt_stdout_write");                                // write the final selected byte sequence
    emitter.label("__rt_bt_arg_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame linkage and the return address
    emitter.instruction("add sp, sp, #64");                                     // release formatter state
    emitter.instruction("ret");                                                 // return without changing ownership
}

/// Emits the Linux x86_64 compact backtrace argument formatter.
fn emit_backtrace_print_arg_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: backtrace_print_arg ---");
    emitter.label_global("__rt_backtrace_print_arg");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve value, string-loop, and escape state
    emitter.instruction("call __rt_mixed_unbox");                               // normalize nested Mixed wrappers to one tag and payload pair
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the concrete tag
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the low payload word
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the high payload word
    emitter.instruction("cmp rax, 0");                                          // integer value?
    emitter.instruction("je __rt_bt_arg_int_x");                                // format integers as decimal digits
    emitter.instruction("cmp rax, 1");                                          // string value?
    emitter.instruction("je __rt_bt_arg_string_x");                             // quote and escape string bytes
    emitter.instruction("cmp rax, 2");                                          // floating-point value?
    emitter.instruction("je __rt_bt_arg_float_x");                              // format floats with the shared PHP formatter
    emitter.instruction("cmp rax, 3");                                          // boolean value?
    emitter.instruction("je __rt_bt_arg_bool_x");                               // print true or false
    emitter.instruction("cmp rax, 4");                                          // indexed array value?
    emitter.instruction("je __rt_bt_arg_array_x");                              // both array storage shapes print as Array
    emitter.instruction("cmp rax, 5");                                          // associative array value?
    emitter.instruction("je __rt_bt_arg_array_x");                              // share the Array spelling
    emitter.instruction("cmp rax, 6");                                          // object value?
    emitter.instruction("je __rt_bt_arg_object_x");                             // print Object(ClassName)
    emitter.instruction("cmp rax, 8");                                          // null value?
    emitter.instruction("je __rt_bt_arg_null_x");                               // print PHP's uppercase NULL spelling
    emitter.instruction("cmp rax, 9");                                          // resource value?
    emitter.instruction("je __rt_bt_arg_resource_x");                           // delegate resource-id rendering
    emitter.instruction("cmp rax, 10");                                         // callable descriptor?
    emitter.instruction("je __rt_bt_arg_callable_x");                           // expose the Closure object spelling
    abi::emit_symbol_address(emitter, "rdi", "_bt_arg_unknown");
    emitter.instruction("mov rsi, 7");                                          // len("Unknown")
    emitter.instruction("jmp __rt_bt_arg_write_done_x");                        // write the fallback and return

    emitter.label("__rt_bt_arg_int_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the integer payload
    emitter.instruction("call __rt_itoa");                                      // rax/rdx = decimal byte pair
    emitter.instruction("mov rdi, rax");                                        // stdout pointer argument
    emitter.instruction("mov rsi, rdx");                                        // stdout length argument
    emitter.instruction("jmp __rt_bt_arg_write_done_x");                        // write the converted integer

    emitter.label("__rt_bt_arg_float_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // load floating-point payload bits
    emitter.instruction("movq xmm0, r10");                                      // move bits into the float argument register
    emitter.instruction("call __rt_ftoa");                                      // rax/rdx = PHP-formatted float byte pair
    emitter.instruction("mov rdi, rax");                                        // stdout pointer argument
    emitter.instruction("mov rsi, rdx");                                        // stdout length argument
    emitter.instruction("jmp __rt_bt_arg_write_done_x");                        // write the converted float

    emitter.label("__rt_bt_arg_bool_x");
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // inspect the boolean payload
    abi::emit_symbol_address(emitter, "rdi", "_bt_arg_false");
    emitter.instruction("mov rsi, 5");                                          // len("false")
    emitter.instruction("je __rt_bt_arg_write_done_x");                         // zero selects false
    abi::emit_symbol_address(emitter, "rdi", "_bt_arg_true");
    emitter.instruction("mov rsi, 4");                                          // len("true")
    emitter.instruction("jmp __rt_bt_arg_write_done_x");                        // nonzero selects true

    emitter.label("__rt_bt_arg_array_x");
    abi::emit_symbol_address(emitter, "rdi", "_bt_arg_array");
    emitter.instruction("mov rsi, 5");                                          // len("Array")
    emitter.instruction("jmp __rt_bt_arg_write_done_x");                        // write the compact array spelling

    emitter.label("__rt_bt_arg_null_x");
    abi::emit_symbol_address(emitter, "rdi", "_bt_arg_null");
    emitter.instruction("mov rsi, 4");                                          // len("NULL")
    emitter.instruction("jmp __rt_bt_arg_write_done_x");                        // write the compact null spelling

    emitter.label("__rt_bt_arg_callable_x");
    abi::emit_symbol_address(emitter, "rdi", "_bt_arg_callable");
    emitter.instruction("mov rsi, 15");                                         // len("Object(Closure)")
    emitter.instruction("jmp __rt_bt_arg_write_done_x");                        // write the callable object spelling

    emitter.label("__rt_bt_arg_resource_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the native resource payload
    emitter.instruction("call __rt_resource_write_stdout");                     // write Resource id #N through the shared sink
    emitter.instruction("jmp __rt_bt_arg_done_x");                              // the resource helper already emitted the value

    emitter.label("__rt_bt_arg_object_x");
    abi::emit_symbol_address(emitter, "rdi", "_bt_arg_object_open");
    emitter.instruction("mov rsi, 7");                                          // len("Object(")
    emitter.instruction("call __rt_stdout_write");                              // write the object prefix
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the object payload
    emitter.instruction("test r9, r9");                                         // is the object pointer structurally present?
    emitter.instruction("jz __rt_bt_arg_object_fallback_x");                    // malformed null objects use the generic name
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the runtime class id
    abi::emit_load_symbol_to_reg(emitter, "r10", "_class_name_count", 0);
    emitter.instruction("cmp r9, r10");                                         // is the class id inside the dense name table?
    emitter.instruction("jae __rt_bt_arg_object_fallback_x");                   // negative or out-of-range ids use object
    abi::emit_symbol_address(emitter, "r10", "_class_name_entries");
    emitter.instruction("shl r9, 4");                                           // scale the id to a 16-byte row
    emitter.instruction("add r10, r9");                                         // select the class-name row
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // load the class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // load the class-name length
    emitter.instruction("test rsi, rsi");                                       // reject empty metadata rows
    emitter.instruction("jnz __rt_bt_arg_object_name_ready_x");                 // non-empty metadata is authoritative
    emitter.label("__rt_bt_arg_object_fallback_x");
    abi::emit_symbol_address(emitter, "rdi", "_unser_type_object");
    emitter.instruction("mov rsi, 6");                                          // len("object")
    emitter.label("__rt_bt_arg_object_name_ready_x");
    emitter.instruction("call __rt_stdout_write");                              // write the resolved class name
    abi::emit_symbol_address(emitter, "rdi", "_bt_arg_close");
    emitter.instruction("mov rsi, 1");                                          // len(")")
    emitter.instruction("jmp __rt_bt_arg_write_done_x");                        // close the object spelling

    emitter.label("__rt_bt_arg_string_x");
    abi::emit_symbol_address(emitter, "rdi", "_bt_arg_quote");
    emitter.instruction("mov rsi, 1");                                          // opening quote length
    emitter.instruction("call __rt_stdout_write");                              // write the opening single quote
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // load the source string pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // load the source byte length
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // initialize the byte cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // initialize the remaining-byte count
    emitter.label("__rt_bt_arg_string_loop_x");
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // are any source bytes left?
    emitter.instruction("je __rt_bt_arg_string_close_x");                       // stop after every byte is rendered
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the current source cursor
    emitter.instruction("movzx r11d, BYTE PTR [r9]");                           // load one unsigned source byte
    emitter.instruction("cmp r11d, 92");                                        // backslash?
    emitter.instruction("je __rt_bt_arg_escape_slash_x");                       // duplicate backslashes
    for (value, suffix) in [(10, "n"), (9, "t"), (13, "r"), (11, "v"), (12, "f"), (27, "e")] {
        emitter.instruction(&format!("cmp r11d, {value}"));                     // compare against one named escape byte
        emitter.instruction(&format!("je __rt_bt_arg_escape_{suffix}_x"));      // select the matching two-byte escape
    }
    emitter.instruction("cmp r11d, 32");                                        // printable ASCII lower bound
    emitter.instruction("jb __rt_bt_arg_escape_hex_x");                         // control bytes use uppercase hex
    emitter.instruction("cmp r11d, 126");                                       // printable ASCII upper bound
    emitter.instruction("ja __rt_bt_arg_escape_hex_x");                         // non-ASCII bytes use uppercase hex
    emitter.instruction("mov rdi, r9");                                         // write directly from the source cursor
    emitter.instruction("mov rsi, 1");                                          // one printable byte
    emitter.instruction("jmp __rt_bt_arg_string_write_x");                      // share cursor advancement

    for suffix in ["slash", "n", "t", "r", "v", "f", "e"] {
        emitter.label(&format!("__rt_bt_arg_escape_{suffix}_x"));
        abi::emit_symbol_address(emitter, "rdi", &format!("_bt_arg_escape_{suffix}"));
        emitter.instruction("mov rsi, 2");                                      // every named escape occupies two bytes
        emitter.instruction("jmp __rt_bt_arg_string_write_x");                  // write the escape and advance
    }

    emitter.label("__rt_bt_arg_escape_hex_x");
    emitter.instruction("mov BYTE PTR [rbp - 48], 92");                         // first byte is a backslash
    emitter.instruction("mov BYTE PTR [rbp - 47], 120");                        // second byte is lowercase x
    emitter.instruction("mov r10d, r11d");                                      // copy the source byte for its high nibble
    emitter.instruction("shr r10d, 4");                                         // isolate the high nibble
    emitter.instruction("cmp r10d, 10");                                        // numeric or alphabetic nibble?
    emitter.instruction("jb __rt_bt_arg_hex_high_decimal_x");                   // 0 through 9 use decimal digits
    emitter.instruction("add r10d, 55");                                        // 10 through 15 become uppercase A through F
    emitter.instruction("jmp __rt_bt_arg_hex_high_ready_x");                    // store the converted high digit
    emitter.label("__rt_bt_arg_hex_high_decimal_x");
    emitter.instruction("add r10d, 48");                                        // 0 through 9 become ASCII digits
    emitter.label("__rt_bt_arg_hex_high_ready_x");
    emitter.instruction("mov BYTE PTR [rbp - 46], r10b");                       // store the high hexadecimal digit
    emitter.instruction("and r11d, 15");                                        // isolate the low nibble
    emitter.instruction("cmp r11d, 10");                                        // numeric or alphabetic nibble?
    emitter.instruction("jb __rt_bt_arg_hex_low_decimal_x");                    // 0 through 9 use decimal digits
    emitter.instruction("add r11d, 55");                                        // 10 through 15 become uppercase A through F
    emitter.instruction("jmp __rt_bt_arg_hex_low_ready_x");                     // store the converted low digit
    emitter.label("__rt_bt_arg_hex_low_decimal_x");
    emitter.instruction("add r11d, 48");                                        // 0 through 9 become ASCII digits
    emitter.label("__rt_bt_arg_hex_low_ready_x");
    emitter.instruction("mov BYTE PTR [rbp - 45], r11b");                       // store the low hexadecimal digit
    emitter.instruction("lea rdi, [rbp - 48]");                                 // point at the four-byte local escape
    emitter.instruction("mov rsi, 4");                                          // len("\\xHH")

    emitter.label("__rt_bt_arg_string_write_x");
    emitter.instruction("call __rt_stdout_write");                              // write the raw byte or selected escape
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // consume one source byte
    emitter.instruction("sub QWORD PTR [rbp - 40], 1");                         // account for the consumed byte
    emitter.instruction("jmp __rt_bt_arg_string_loop_x");                       // render the next source byte

    emitter.label("__rt_bt_arg_string_close_x");
    abi::emit_symbol_address(emitter, "rdi", "_bt_arg_quote");
    emitter.instruction("mov rsi, 1");                                          // closing quote length

    emitter.label("__rt_bt_arg_write_done_x");
    emitter.instruction("call __rt_stdout_write");                              // write the final selected byte sequence
    emitter.label("__rt_bt_arg_done_x");
    emitter.instruction("leave");                                               // release formatter state and restore rbp
    emitter.instruction("ret");                                                 // return without changing ownership
}
