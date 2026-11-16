//! Purpose:
//! Emits OpenSSL cipher metadata helpers backed by the elephc-crypto bridge.
//!
//! Called from:
//! - `runtime::emitters::emit_runtime()` through the string-runtime module.
//!
//! Key details:
//! - Function pointers are published only at OpenSSL call sites.
//! - The method bridge returns trailing-NUL packed names, which are copied into owned array elements.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits IV-length lookup and supported-cipher-list runtime helpers.
pub fn emit_openssl_methods(emitter: &mut Emitter) {
    emit_openssl_cipher_iv_length(emitter);
    emit_openssl_get_cipher_methods(emitter);
}

/// Emits `__rt_openssl_cipher_iv_length`, returning a length or negative bridge status.
fn emit_openssl_cipher_iv_length(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: openssl_cipher_iv_length ---");
    emitter.label_global("__rt_openssl_cipher_iv_length");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // preserve the helper frame across the indirect bridge call
            emitter.instruction("mov x29, sp");                                 // establish a conventional AArch64 frame
            emitter.instruction("mov x0, x1");                                  // C arg0 = cipher-name pointer
            emitter.instruction("mov x1, x2");                                  // C arg1 = cipher-name length
            abi::emit_symbol_address(emitter, "x9", "_elephc_openssl_cipher_iv_length_fn");
            emitter.instruction("ldr x9, [x9]");                                // load the published IV-length bridge entry
            emitter.instruction("cbz x9, __rt_openssl_iv_length_missing");      // a missing bridge behaves like an unknown cipher
            abi::emit_call_reg(emitter, "x9");
            emitter.instruction("b __rt_openssl_iv_length_done");               // retain the bridge result in x0
            emitter.label("__rt_openssl_iv_length_missing");
            emitter.instruction("mov x0, #-1");                                 // stable unknown-cipher status
            emitter.label("__rt_openssl_iv_length_done");
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore the caller frame
            emitter.instruction("ret");                                         // return length/status in x0
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish an aligned SysV frame
            emitter.instruction("mov rdi, rax");                                // C arg0 = cipher-name pointer
            emitter.instruction("mov rsi, rdx");                                // C arg1 = cipher-name length
            abi::emit_load_symbol_to_reg(
                emitter,
                "r11",
                "_elephc_openssl_cipher_iv_length_fn",
                0,
            );
            emitter.instruction("test r11, r11");                               // check whether the bridge entry was published
            emitter.instruction("jz __rt_openssl_iv_length_missing_linux_x86_64"); // a missing bridge behaves like an unknown cipher
            abi::emit_call_reg(emitter, "r11");
            emitter.instruction("jmp __rt_openssl_iv_length_done_linux_x86_64"); // retain the bridge result in rax
            emitter.label("__rt_openssl_iv_length_missing_linux_x86_64");
            emitter.instruction("mov rax, -1");                                 // stable unknown-cipher status
            emitter.label("__rt_openssl_iv_length_done_linux_x86_64");
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return length/status in rax
        }
    }
}

/// Emits `__rt_openssl_get_cipher_methods`, decoding the bridge's packed inventory.
fn emit_openssl_get_cipher_methods(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_openssl_get_cipher_methods_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: openssl_get_cipher_methods ---");
    emitter.label_global("__rt_openssl_get_cipher_methods");
    emitter.instruction("sub sp, sp, #4096");                                   // reserve the first half of the packed-name buffer and state
    emitter.instruction("sub sp, sp, #64");                                     // complete the 4160-byte frame without an unencodable immediate
    emitter.instruction("add x9, sp, #4096");                                   // address the state area
    emitter.instruction("add x9, x9, #48");                                     // point at the saved-frame slot
    emitter.instruction("stp x29, x30, [x9]");                                  // preserve frame pointer and return address
    emitter.instruction("mov x29, x9");                                         // establish the helper frame
    emitter.instruction("add x1, sp, #0");                                      // C arg1 = packed-name output buffer
    emitter.instruction("mov x2, #4096");                                       // C arg2 = packed-name buffer capacity
    emitter.instruction("add x3, sp, #4096");                                   // C arg3 = packed output byte length
    abi::emit_symbol_address(emitter, "x9", "_elephc_openssl_cipher_methods_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the published method-inventory entry
    emitter.instruction("cbz x9, __rt_openssl_methods_empty");                  // missing bridge produces an empty supported list
    abi::emit_call_reg(emitter, "x9");
    emitter.instruction("cmp x0, #0");                                          // negative values are stable bridge failures
    emitter.instruction("b.lt __rt_openssl_methods_empty");                     // failed bridge yields the empty list
    emitter.instruction("add x9, sp, #4096");                                   // address the state area after the bridge call
    emitter.instruction("str x0, [x9, #8]");                                    // retain method count across array allocation
    emitter.instruction("mov x1, #16");                                         // string array element size = ptr + len
    abi::emit_call_label(emitter, "__rt_array_new");
    emitter.instruction("add x9, sp, #4096");                                   // address the state area after array allocation
    emitter.instruction("str x0, [x9, #16]");                                   // save the result array across pushes
    emitter.instruction("mov x10, sp");                                         // packed-name buffer start
    emitter.instruction("str x10, [x9, #24]");                                  // initialize packed-name cursor
    emitter.instruction("b __rt_openssl_methods_loop");                         // begin decoding packed names

    emitter.label("__rt_openssl_methods_empty");
    emitter.instruction("mov x0, #0");                                          // create an empty string array on bridge failure
    emitter.instruction("mov x1, #16");                                         // string array element size = ptr + len
    abi::emit_call_label(emitter, "__rt_array_new");
    emitter.instruction("b __rt_openssl_methods_done");                         // return the empty array

    emitter.label("__rt_openssl_methods_loop");
    emitter.instruction("ldr x9, [sp, #4096]");                                 // remaining packed bytes
    emitter.instruction("cbz x9, __rt_openssl_methods_finish");                 // all packed names consumed
    emitter.instruction("add x10, sp, #4096");                                  // address the state area
    emitter.instruction("ldr x10, [x10, #24]");                                 // current name start
    emitter.instruction("mov x11, x10");                                        // scanning cursor
    emitter.label("__rt_openssl_methods_scan");
    emitter.instruction("ldrb w12, [x11], #1");                                 // read one packed-name byte
    emitter.instruction("sub x9, x9, #1");                                      // consume one packed byte
    emitter.instruction("cbnz w12, __rt_openssl_methods_scan");                 // stop at the trailing NUL
    emitter.instruction("sub x2, x11, x10");                                    // byte count including NUL
    emitter.instruction("sub x2, x2, #1");                                      // PHP string length excludes NUL
    emitter.instruction("str x9, [sp, #4096]");                                 // save remaining packed byte count
    emitter.instruction("add x9, sp, #4096");                                   // address the state area after scanning
    emitter.instruction("str x11, [x9, #24]");                                  // advance cursor to the next name
    emitter.instruction("ldr x0, [x9, #16]");                                   // reload result array
    emitter.instruction("mov x1, x10");                                         // name pointer
    abi::emit_call_label(emitter, "__rt_array_push_str");
    emitter.instruction("add x9, sp, #4096");                                   // address the state area after the push
    emitter.instruction("str x0, [x9, #16]");                                   // retain possibly-grown result array
    emitter.instruction("b __rt_openssl_methods_loop");                         // decode the next packed name

    emitter.label("__rt_openssl_methods_finish");
    emitter.instruction("add x9, sp, #4096");                                   // address the state area
    emitter.instruction("ldr x0, [x9, #16]");                                   // return completed method array
    emitter.label("__rt_openssl_methods_done");
    emitter.instruction("add x9, sp, #4096");                                   // address the saved-frame slot
    emitter.instruction("add x9, x9, #48");                                     // complete the frame address
    emitter.instruction("ldp x29, x30, [x9]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #4096");                                   // release the first half of the helper frame
    emitter.instruction("add sp, sp, #64");                                     // release the remainder of the helper frame
    emitter.instruction("ret");                                                 // return array handle in x0
}

/// Emits the Linux x86_64 method-inventory bridge and packed-name decoder.
fn emit_openssl_get_cipher_methods_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: openssl_get_cipher_methods ---");
    emitter.label_global("__rt_openssl_get_cipher_methods");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable local addressing
    emitter.instruction("sub rsp, 4096");                                       // reserve the packed-name buffer
    emitter.instruction("sub rsp, 48");                                         // reserve parser state without breaking 16-byte alignment
    emitter.instruction("lea rsi, [rbp - 4144]");                               // C arg1 = packed-name output buffer
    emitter.instruction("mov edx, 4096");                                       // C arg2 = packed-name buffer capacity
    emitter.instruction("lea rcx, [rbp - 48]");                                 // C arg3 = packed output byte length
    abi::emit_load_symbol_to_reg(
        emitter,
        "r11",
        "_elephc_openssl_cipher_methods_fn",
        0,
    );
    emitter.instruction("test r11, r11");                                       // check whether the method bridge was published
    emitter.instruction("jz __rt_openssl_methods_empty_linux_x86_64");          // missing bridge produces an empty supported list
    abi::emit_call_reg(emitter, "r11");
    emitter.instruction("test rax, rax");                                       // negative values are stable bridge failures
    emitter.instruction("js __rt_openssl_methods_empty_linux_x86_64");          // failed bridge yields the empty list
    emitter.instruction("mov edi, eax");                                        // initial array capacity = method count
    emitter.instruction("mov esi, 16");                                         // string array element size = ptr + len
    abi::emit_call_label(emitter, "__rt_array_new");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save result array across pushes
    emitter.instruction("lea r10, [rbp - 4144]");                               // packed-name buffer start
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // initialize packed-name cursor
    emitter.instruction("jmp __rt_openssl_methods_loop_linux_x86_64");          // begin decoding packed names

    emitter.label("__rt_openssl_methods_empty_linux_x86_64");
    emitter.instruction("xor edi, edi");                                        // create an empty string array on bridge failure
    emitter.instruction("mov esi, 16");                                         // string array element size = ptr + len
    abi::emit_call_label(emitter, "__rt_array_new");
    emitter.instruction("jmp __rt_openssl_methods_done_linux_x86_64");          // return the empty array

    emitter.label("__rt_openssl_methods_loop_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // remaining packed bytes
    emitter.instruction("test r9, r9");                                         // any packed bytes left?
    emitter.instruction("jz __rt_openssl_methods_finish_linux_x86_64");         // all packed names consumed
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // current name start
    emitter.instruction("mov r11, r10");                                        // scanning cursor
    emitter.label("__rt_openssl_methods_scan_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [r11]");                           // read one packed-name byte
    emitter.instruction("add r11, 1");                                          // advance scanning cursor
    emitter.instruction("sub r9, 1");                                           // consume one packed byte
    emitter.instruction("test eax, eax");                                       // is this the trailing NUL?
    emitter.instruction("jnz __rt_openssl_methods_scan_linux_x86_64");          // stop at the trailing NUL
    emitter.instruction("mov rdx, r11");                                        // compute byte count including NUL
    emitter.instruction("sub rdx, r10");                                        // end minus start = byte count
    emitter.instruction("sub rdx, 1");                                          // PHP string length excludes NUL
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save remaining packed byte count
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // advance cursor to next name
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload result array
    emitter.instruction("mov rsi, r10");                                        // name pointer
    abi::emit_call_label(emitter, "__rt_array_push_str");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // retain possibly-grown result array
    emitter.instruction("jmp __rt_openssl_methods_loop_linux_x86_64");          // decode the next packed name

    emitter.label("__rt_openssl_methods_finish_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return completed method array
    emitter.label("__rt_openssl_methods_done_linux_x86_64");
    emitter.instruction("mov rsp, rbp");                                        // release packed buffer and parser state
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return array handle in rax
}
