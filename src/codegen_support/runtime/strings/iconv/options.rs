//! Purpose:
//! Emits `__rt_iconv_mime_option`, which reads one `iconv_mime_encode()` option out of the
//! caller's PHP array and stages it into the bridge argument block.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::iconv::emit_iconv()`.
//! - `crate::codegen::lower_inst::builtins::iconv`, once per recognized option key.
//!
//! Key details:
//! - `$options` is an ordinary PHP associative array, so the option is fetched with the
//!   shared `__rt_hash_get` and then normalized from whatever runtime tag it carries.
//! - php-src ignores a non-string `scheme`/charset option but coerces `line-length` with
//!   `zval_get_long()`, which is why the caller selects between the two normalizations.
//! - A missing key leaves the staged slot untouched, so the caller must clear the slot's
//!   presence flag first.
//! - `__rt_iconv_option_table` resolves a receiver whose static type is `mixed` into the
//!   associative array it boxes, so those calls honor their options too.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Byte offsets inside one staged argument slot, mirroring `IconvArgSlot`.
const SLOT_PRESENT: usize = 0;
const SLOT_PTR: usize = 8;
const SLOT_LEN: usize = 16;
const SLOT_INT: usize = 24;

/// Emits the option reader for the active target.
pub(super) fn emit_iconv_mime_option(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mime_option_x86_64(emitter);
    } else {
        emit_mime_option_aarch64(emitter);
    }
    emit_iconv_option_table(emitter);
}

/// Emits the AArch64 `__rt_iconv_mime_option` helper.
///
/// Input: x0 = options hash pointer, x1 = key pointer, x2 = key length,
///        x3 = staged slot pointer, x4 = non-zero to normalize as an integer.
/// The slot is written only when the key exists and carries a usable value.
fn emit_mime_option_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv mime option ---");
    emitter.label_global("__rt_iconv_mime_option");
    emitter.instruction("sub sp, sp, #48");                                     // reserve the saved destination, mode, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable helper frame
    emitter.instruction("str x3, [sp, #0]");                                    // retain the staged slot pointer across the lookup
    emitter.instruction("str x4, [sp, #8]");                                    // retain the requested normalization mode
    emitter.instruction("cbz x0, __rt_iconv_option_missing");                   // a null table cannot carry the option
    emitter.instruction("bl __rt_hash_get");                                    // look the option up by its literal key
    emitter.instruction("cbz x0, __rt_iconv_option_missing");                   // absent keys leave the slot untouched
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the requested normalization mode
    emitter.instruction("cbnz x9, __rt_iconv_option_int");                      // line-length is coerced like zval_get_long()

    // -- string option: php-src only accepts a genuine string value --
    emitter.instruction("cmp x3, #1");                                          // runtime tag 1 = string value
    emitter.instruction("b.eq __rt_iconv_option_store_string");                 // store the string payload directly
    emitter.instruction("cmp x3, #7");                                          // runtime tag 7 = boxed Mixed value
    emitter.instruction("b.ne __rt_iconv_option_missing");                      // any other tag is ignored, as php-src does
    emitter.instruction("mov x0, x1");                                          // unbox the Mixed cell before reading its bytes
    emitter.instruction("bl __rt_mixed_cast_string");                           // materialize the option as a PHP string
    emitter.label("__rt_iconv_option_store_string");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the staged slot pointer
    emitter.instruction(&format!("str x1, [x9, #{}]", SLOT_PTR));               // stage the option's byte pointer
    emitter.instruction(&format!("str x2, [x9, #{}]", SLOT_LEN));               // stage the option's byte length
    emitter.instruction("mov x10, #1");                                         // mark the argument as supplied
    emitter.instruction(&format!("str x10, [x9, #{}]", SLOT_PRESENT));          // publish the presence flag
    emitter.instruction("b __rt_iconv_option_missing");                         // share the frame teardown

    emitter.label("__rt_iconv_option_int");
    emitter.instruction("cmp x3, #0");                                          // runtime tag 0 = integer value
    emitter.instruction("b.eq __rt_iconv_option_store_int");                    // an integer needs no coercion
    emitter.instruction("cmp x3, #1");                                          // runtime tag 1 = string value
    emitter.instruction("b.eq __rt_iconv_option_int_from_string");              // parse the leading numeric run
    emitter.instruction("cmp x3, #7");                                          // runtime tag 7 = boxed Mixed value
    emitter.instruction("b.ne __rt_iconv_option_missing");                      // any other tag is ignored
    emitter.instruction("mov x0, x1");                                          // unbox the Mixed cell before coercing it
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce the option the way zval_get_long() does
    emitter.instruction("mov x1, x0");                                          // move the coerced value into the shared store register
    emitter.instruction("b __rt_iconv_option_store_int");                       // stage the coerced integer

    emitter.label("__rt_iconv_option_int_from_string");
    emitter.instruction("bl __rt_str_to_int");                                  // parse PHP's leading numeric run
    emitter.instruction("mov x1, x0");                                          // move the parsed value into the shared store register

    emitter.label("__rt_iconv_option_store_int");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the staged slot pointer
    emitter.instruction(&format!("str x1, [x9, #{}]", SLOT_INT));               // stage the option's integer value
    emitter.instruction("mov x10, #1");                                         // mark the argument as supplied
    emitter.instruction(&format!("str x10, [x9, #{}]", SLOT_PRESENT));          // publish the presence flag

    emitter.label("__rt_iconv_option_missing");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the staged-argument builder
}

/// Emits the Linux x86_64 `__rt_iconv_mime_option` helper.
///
/// Input: rdi = options hash pointer, rsi = key pointer, rdx = key length,
///        rcx = staged slot pointer, r8 = non-zero to normalize as an integer.
fn emit_mime_option_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv mime option ---");
    emitter.label_global("__rt_iconv_mime_option");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved slot and mode
    emitter.instruction("sub rsp, 32");                                         // reserve the saved destination and normalization mode
    emitter.instruction("mov QWORD PTR [rbp - 8], rcx");                        // retain the staged slot pointer across the lookup
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // retain the requested normalization mode
    emitter.instruction("test rdi, rdi");                                       // a null table cannot carry the option
    emitter.instruction("jz __rt_iconv_option_missing_linux_x86_64");           // leave the slot untouched
    emitter.instruction("call __rt_hash_get");                                  // look the option up by its literal key
    emitter.instruction("test rax, rax");                                       // absent keys leave the slot untouched
    emitter.instruction("jz __rt_iconv_option_missing_linux_x86_64");           // leave the slot untouched
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the requested normalization mode
    emitter.instruction("test r10, r10");                                       // line-length is coerced like zval_get_long()
    emitter.instruction("jnz __rt_iconv_option_int_linux_x86_64");              // take the integer normalization path

    // -- string option: php-src only accepts a genuine string value --
    emitter.instruction("cmp rcx, 1");                                          // runtime tag 1 = string value
    emitter.instruction("je __rt_iconv_option_store_string_linux_x86_64");      // store the string payload directly
    emitter.instruction("cmp rcx, 7");                                          // runtime tag 7 = boxed Mixed value
    emitter.instruction("jne __rt_iconv_option_missing_linux_x86_64");          // any other tag is ignored, as php-src does
    emitter.instruction("mov rax, rdi");                                        // unbox the Mixed cell before reading its bytes
    emitter.instruction("call __rt_mixed_cast_string");                         // materialize the option as a PHP string
    emitter.instruction("mov rdi, rax");                                        // move the byte pointer into the shared store register
    emitter.instruction("mov rsi, rdx");                                        // move the byte length into the shared store register
    emitter.label("__rt_iconv_option_store_string_linux_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the staged slot pointer
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], rdi", SLOT_PTR));   // stage the option's byte pointer
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], rsi", SLOT_LEN));   // stage the option's byte length
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], 1", SLOT_PRESENT)); // publish the presence flag
    emitter.instruction("jmp __rt_iconv_option_missing_linux_x86_64");          // share the frame teardown

    emitter.label("__rt_iconv_option_int_linux_x86_64");
    emitter.instruction("cmp rcx, 0");                                          // runtime tag 0 = integer value
    emitter.instruction("je __rt_iconv_option_store_int_linux_x86_64");         // an integer needs no coercion
    emitter.instruction("cmp rcx, 1");                                          // runtime tag 1 = string value
    emitter.instruction("je __rt_iconv_option_int_from_string_linux_x86_64");   // parse the leading numeric run
    emitter.instruction("cmp rcx, 7");                                          // runtime tag 7 = boxed Mixed value
    emitter.instruction("jne __rt_iconv_option_missing_linux_x86_64");          // any other tag is ignored
    emitter.instruction("mov rax, rdi");                                        // unbox the Mixed cell before coercing it
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce the option the way zval_get_long() does
    emitter.instruction("mov rdi, rax");                                        // move the coerced value into the shared store register
    emitter.instruction("jmp __rt_iconv_option_store_int_linux_x86_64");        // stage the coerced integer

    emitter.label("__rt_iconv_option_int_from_string_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // string-to-int reads the string result register pair
    emitter.instruction("mov rdx, rsi");                                        // move the byte length into the string length register
    emitter.instruction("call __rt_str_to_int");                                // parse PHP's leading numeric run
    emitter.instruction("mov rdi, rax");                                        // move the parsed value into the shared store register

    emitter.label("__rt_iconv_option_store_int_linux_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the staged slot pointer
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], rdi", SLOT_INT));   // stage the option's integer value
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], 1", SLOT_PRESENT)); // publish the presence flag

    emitter.label("__rt_iconv_option_missing_linux_x86_64");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the staged-argument builder
}

/// Emits `__rt_iconv_option_table`, which resolves a boxed `$options` receiver.
///
/// Input:  the boxed Mixed cell in the integer result register, or zero.
/// Output: the associative-array pointer it wraps, or zero when it wraps anything else.
///
/// `iconv_mime_encode()`'s option lookups need a hash pointer. A receiver whose static
/// type is `mixed` arrives as a boxed cell instead, and only a cell wrapping an
/// associative array can carry PHP string keys.
pub(super) fn emit_iconv_option_table(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv option table ---");
    emitter.label_global("__rt_iconv_option_table");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // preserve the caller frame pointer and return address
            emitter.instruction("mov x29, sp");                                 // establish a frame for the unboxing call
            emitter.instruction("cbz x0, __rt_iconv_option_table_none");        // a null receiver carries no options
            emitter.instruction("bl __rt_mixed_unbox");                         // read the boxed cell's tag and payload
            emitter.instruction("cmp x0, #5");                                  // runtime tag 5 = associative array
            emitter.instruction("b.ne __rt_iconv_option_table_none");           // any other shape has no PHP string keys
            emitter.instruction("mov x0, x1");                                  // return the wrapped hash pointer
            emitter.instruction("b __rt_iconv_option_table_done");              // skip the empty-table answer
            emitter.label("__rt_iconv_option_table_none");
            emitter.instruction("mov x0, #0");                                  // report that no option table is available
            emitter.label("__rt_iconv_option_table_done");
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore the caller frame pointer and return address
            emitter.instruction("ret");                                         // return the resolved option table
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish an aligned frame for the unboxing call
            emitter.instruction("test rax, rax");                               // a null receiver carries no options
            emitter.instruction("jz __rt_iconv_option_table_none_linux_x86_64"); // answer with an empty table
            emitter.instruction("call __rt_mixed_unbox");                       // read the boxed cell's tag and payload
            emitter.instruction("cmp rax, 5");                                  // runtime tag 5 = associative array
            emitter.instruction("jne __rt_iconv_option_table_none_linux_x86_64"); // any other shape has no PHP string keys
            emitter.instruction("mov rax, rdi");                                // return the wrapped hash pointer
            emitter.instruction("jmp __rt_iconv_option_table_done_linux_x86_64"); // skip the empty-table answer
            emitter.label("__rt_iconv_option_table_none_linux_x86_64");
            emitter.instruction("xor eax, eax");                                // report that no option table is available
            emitter.label("__rt_iconv_option_table_done_linux_x86_64");
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the resolved option table
        }
    }
}
