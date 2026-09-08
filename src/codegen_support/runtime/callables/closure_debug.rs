//! Purpose:
//! Emits Closure-specific debug walkers for callable descriptors.
//! Keeps descriptor-backed Closures out of ordinary object-header walkers.
//!
//! Called from:
//! - `runtime::callables` and tag-10 debug dispatchers.
//!
//! Key details:
//! - The descriptor itself owns the PHP object handle and must remain the recursion identity.
//! - Debug records live behind the invocation side record and never move stable descriptor offsets.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the Closure-specific `var_dump` walker for callable descriptor tag 10.
pub(crate) fn emit_closure_debug(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_closure ---");
    emitter.label_global("__rt_var_dump_closure");
    match emitter.target.arch {
        Arch::AArch64 => emit_var_dump_closure_aarch64(emitter),
        Arch::X86_64 => emit_var_dump_closure_x86_64(emitter),
    }
    emit_print_r_closure(emitter);
    emit_var_dump_open_counted_array(emitter);
    emit_var_dump_closure_parameters(emitter);
    emit_closure_debug_static_count(emitter);
    emit_var_dump_closure_static(emitter);
    emit_closure_debug_this_present(emitter);
    emit_var_dump_closure_this(emitter);
    emit_print_r_closure_static(emitter);
    emit_print_r_closure_parameters(emitter);
    emit_print_r_closure_this(emitter);
}

/// Emits the php-src Closure `$this` property through the `print_r` walker.
fn emit_print_r_closure_this(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r_closure_this ---");
    emitter.label_global("__rt_print_r_closure_this");
    match emitter.target.arch {
        Arch::AArch64 => emit_print_r_closure_this_aarch64(emitter),
        Arch::X86_64 => emit_print_r_closure_this_x86_64(emitter),
    }
}

/// Emits the AArch64 `print_r` bound Closure receiver from its live capture slot.
fn emit_print_r_closure_this_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #64");                                     // allocate descriptor, receiver, and frame slots
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the Closure receiver print_r frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // preserve the Closure descriptor and caller base indent
    emitter.instruction("bl __rt_closure_debug_this_present");                  // reject unbound top-level Closure receivers
    emitter.instruction("cbz x0, __rt_pr_closure_this_done");                   // omit `$this` when no object is bound
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the Closure descriptor identity
    emitter.instruction("ldr x9, [x9, #48]");                                   // load the invocation side record pointer
    emitter.instruction("ldr x9, [x9, #56]");                                   // load the true-Closure debug record pointer
    emitter.instruction("ldr x10, [x9, #56]");                                  // load the capture binding-table pointer
    emitter.instruction("ldr x11, [x9, #64]");                                  // load the capture binding count
    emitter.instruction("mov x12, #0");                                         // initialize the capture binding cursor
    emitter.label("__rt_pr_closure_this_scan");
    emitter.instruction("cmp x12, x11");                                        // consumed every capture binding?
    emitter.instruction("b.ge __rt_pr_closure_this_done");                      // presence helper makes this defensive path unreachable
    emitter.instruction("add x13, x10, x12, lsl #5");                           // select this 32-byte capture binding record
    emitter.instruction("ldr x14, [x13, #8]");                                  // load the capture name length
    emitter.instruction("cmp x14, #4");                                         // only `$this` has the reserved four-byte name
    emitter.instruction("b.ne __rt_pr_closure_this_next");                      // another capture cannot provide the Closure receiver
    emitter.instruction("ldr x14, [x13]");                                      // load the capture name pointer
    emitter.instruction("ldr w14, [x14]");                                      // load the first four bytes of the capture name
    emitter.instruction("movz w15, #0x6874");                                   // low half of `this` in little-endian order
    emitter.instruction("movk w15, #0x7369, lsl #16");                          // high half of `this` in little-endian order
    emitter.instruction("cmp w14, w15");                                        // is this the reserved `$this` binding?
    emitter.instruction("b.ne __rt_pr_closure_this_next");                      // another four-byte capture is not `$this`
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the Closure descriptor identity
    emitter.instruction("add x12, x12, #4");                                    // skip the 64-byte descriptor header in capture units
    emitter.instruction("add x9, x9, x12, lsl #4");                             // resolve the `$this` capture slot address
    emitter.instruction("ldr x0, [x13, #16]");                                  // load the `$this` capture runtime type tag
    emitter.instruction("ldr x1, [x9]");                                        // load the `$this` capture low payload word
    emitter.instruction("ldr x2, [x9, #8]");                                    // load the `$this` capture high payload word
    emitter.instruction("cmp x0, #7");                                          // top-level Closures capture boxed Mixed receivers
    emitter.instruction("b.ne __rt_pr_closure_this_store");                     // method-defined Closures capture raw objects directly
    emitter.instruction("mov x0, x1");                                          // pass the boxed Mixed receiver to the unbox helper
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the bound receiver into tag and payload words
    emitter.label("__rt_pr_closure_this_store");
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the live `$this` runtime tag through key rendering
    emitter.instruction("str x1, [sp, #24]");                                   // preserve the live `$this` low payload through key rendering
    emitter.instruction("str x2, [sp, #32]");                                   // preserve the live `$this` high payload through key rendering
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the Closure base indent for the `$this` property row
    emitter.instruction("add x0, x0, #4");                                      // property rows are four spaces beneath the Closure block
    emitter.instruction("bl __rt_print_r_spaces");                              // indent the `$this` property row
    abi::emit_symbol_address(emitter, "x1", "_closure_print_r_key_this");
    emitter.instruction("mov x2, #10");                                         // len("[this] => ") = 10
    emitter.instruction("bl __rt_pr_write");                                    // write the `$this` property prefix
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the live `$this` runtime tag
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the live `$this` low payload
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the live `$this` high payload
    emitter.instruction("ldr x3, [sp, #8]");                                    // reload the Closure base indent for nested receiver containers
    emitter.instruction("add x3, x3, #8");                                      // nested receiver containers begin beneath the `$this` property row
    emitter.instruction("bl __rt_print_r_value");                               // render the bound receiver object
    abi::emit_symbol_address(emitter, "x1", "_pr_nl");
    emitter.instruction("mov x2, #1");                                          // len("\\n") = 1
    emitter.instruction("bl __rt_pr_write");                                    // terminate the `$this` property row
    emitter.instruction("b __rt_pr_closure_this_done");                         // finish after rendering the unique `$this` binding
    emitter.label("__rt_pr_closure_this_next");
    emitter.instruction("add x12, x12, #1");                                    // advance to the next capture binding
    emitter.instruction("b __rt_pr_closure_this_scan");                         // continue scanning for `$this`
    emitter.label("__rt_pr_closure_this_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the Closure receiver print_r frame
    emitter.instruction("ret");                                                 // return to the Closure projection
}

/// Emits the x86_64 `print_r` bound Closure receiver from its live capture slot.
fn emit_print_r_closure_this_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the Closure receiver print_r frame
    emitter.instruction("sub rsp, 64");                                         // allocate descriptor, receiver, and alignment slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the Closure descriptor identity
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the caller base indent
    emitter.instruction("call __rt_closure_debug_this_present");                // reject unbound top-level Closure receivers
    emitter.instruction("test rax, rax");                                       // is an object actually bound as `$this`?
    emitter.instruction("jz __rt_pr_closure_this_done_x86");                    // omit `$this` when no object is bound
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the Closure descriptor identity
    emitter.instruction("mov r9, QWORD PTR [r9 + 48]");                         // load the invocation side record pointer
    emitter.instruction("mov r9, QWORD PTR [r9 + 56]");                         // load the true-Closure debug record pointer
    emitter.instruction("mov r10, QWORD PTR [r9 + 56]");                        // load the capture binding-table pointer
    emitter.instruction("mov r11, QWORD PTR [r9 + 64]");                        // load the capture binding count
    emitter.instruction("xor ecx, ecx");                                        // initialize the capture binding cursor
    emitter.label("__rt_pr_closure_this_scan_x86");
    emitter.instruction("cmp rcx, r11");                                        // consumed every capture binding?
    emitter.instruction("jge __rt_pr_closure_this_done_x86");                   // presence helper makes this defensive path unreachable
    emitter.instruction("mov r8, rcx");                                         // copy the cursor before scaling the binding offset
    emitter.instruction("shl r8, 5");                                           // scale the cursor by the 32-byte binding stride
    emitter.instruction("add r8, r10");                                         // select this capture binding record
    emitter.instruction("cmp QWORD PTR [r8 + 8], 4");                           // only `$this` has the reserved four-byte name
    emitter.instruction("jne __rt_pr_closure_this_next_x86");                   // another capture cannot provide the Closure receiver
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the capture name pointer
    emitter.instruction("cmp DWORD PTR [r9], 0x73696874");                      // is this the reserved `$this` binding?
    emitter.instruction("jne __rt_pr_closure_this_next_x86");                   // another four-byte capture is not `$this`
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor identity
    emitter.instruction("add rcx, 4");                                          // skip the 64-byte descriptor header in capture units
    emitter.instruction("shl rcx, 4");                                          // scale the `$this` capture slot offset by sixteen bytes
    emitter.instruction("add rax, rcx");                                        // resolve the `$this` capture slot address
    emitter.instruction("mov rdi, QWORD PTR [r8 + 16]");                        // load the `$this` capture runtime type tag
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // load the `$this` capture low payload word
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // load the `$this` capture high payload word
    emitter.instruction("cmp rdi, 7");                                          // top-level Closures capture boxed Mixed receivers
    emitter.instruction("jne __rt_pr_closure_this_store_x86");                  // method-defined Closures capture raw objects directly
    emitter.instruction("mov rax, rsi");                                        // pass the boxed Mixed receiver to the unbox helper
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the bound receiver into tag and payload words
    emitter.instruction("mov rsi, rdi");                                        // unboxed payload becomes the print_r low word
    emitter.instruction("mov rdi, rax");                                        // unboxed tag becomes the print_r type argument
    emitter.label("__rt_pr_closure_this_store_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the live `$this` runtime tag through key rendering
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // preserve the live `$this` low payload through key rendering
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // preserve the live `$this` high payload through key rendering
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the `$this` property row
    emitter.instruction("add rdi, 4");                                          // property rows are four spaces beneath the Closure block
    emitter.instruction("call __rt_print_r_spaces");                            // indent the `$this` property row
    abi::emit_symbol_address(emitter, "rsi", "_closure_print_r_key_this");
    emitter.instruction("mov edx, 10");                                         // len("[this] => ") = 10
    emitter.instruction("call __rt_pr_write");                                  // write the `$this` property prefix
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the live `$this` runtime tag
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the live `$this` low payload
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the live `$this` high payload
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for nested receiver containers
    emitter.instruction("add rcx, 8");                                          // nested receiver containers begin beneath the `$this` property row
    emitter.instruction("call __rt_print_r_value");                             // render the bound receiver object
    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");
    emitter.instruction("mov edx, 1");                                          // len("\\n") = 1
    emitter.instruction("call __rt_pr_write");                                  // terminate the `$this` property row
    emitter.instruction("jmp __rt_pr_closure_this_done_x86");                   // finish after rendering the unique `$this` binding
    emitter.label("__rt_pr_closure_this_next_x86");
    emitter.instruction("add rcx, 1");                                          // advance to the next capture binding
    emitter.instruction("jmp __rt_pr_closure_this_scan_x86");                   // continue scanning for `$this`
    emitter.label("__rt_pr_closure_this_done_x86");
    emitter.instruction("add rsp, 64");                                         // release the Closure receiver print_r frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the Closure projection
}

/// Emits the php-src Closure `parameter` property through the `print_r` walker.
fn emit_print_r_closure_parameters(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r_closure_parameters ---");
    emitter.label_global("__rt_print_r_closure_parameters");
    match emitter.target.arch {
        Arch::AArch64 => emit_print_r_closure_parameters_aarch64(emitter),
        Arch::X86_64 => emit_print_r_closure_parameters_x86_64(emitter),
    }
}

/// Emits the AArch64 `print_r` Closure parameter array from signature metadata.
fn emit_print_r_closure_parameters_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #80");                                     // allocate descriptor, signature, counters, and frame slots
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the Closure parameter print_r frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // preserve the Closure descriptor and caller base indent
    emitter.instruction("ldr x9, [x0, #32]");                                   // load the optional signature side record
    emitter.instruction("cbz x9, __rt_pr_closure_params_done");                 // descriptors without signatures expose no parameter property
    emitter.instruction("ldr x10, [x9]");                                       // load the visible parameter count
    emitter.instruction("cbz x10, __rt_pr_closure_params_done");                // zero visible parameters omit the parameter property
    emitter.instruction("ldr x11, [x9, #96]");                                  // load the debug `$name` and `&$name` key table
    emitter.instruction("cbz x11, __rt_pr_closure_params_done");                // generated debug names are required for php-src keys
    emitter.instruction("str x10, [sp, #24]");                                  // preserve the visible parameter count
    emitter.instruction("ldr x10, [x9, #8]");                                   // load the required parameter count
    emitter.instruction("str x10, [sp, #32]");                                  // preserve the required/optional split point
    emitter.instruction("str x11, [sp, #40]");                                  // preserve the generated parameter-key table pointer
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the Closure base indent for the parameter property row
    emitter.instruction("add x0, x0, #4");                                      // property rows are four spaces beneath the Closure block
    emitter.instruction("bl __rt_print_r_spaces");                              // indent the parameter property row
    abi::emit_symbol_address(emitter, "x1", "_closure_print_r_key_parameter");
    emitter.instruction("mov x2, #15");                                         // len("[parameter] => ") = 15
    emitter.instruction("bl __rt_pr_write");                                    // write the parameter property prefix
    abi::emit_symbol_address(emitter, "x1", "_pr_array_hdr");
    emitter.instruction("mov x2, #6");                                          // len("Array\\n") = 6
    emitter.instruction("bl __rt_pr_write");                                    // write the parameter array header
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the Closure base indent for the parameter array
    emitter.instruction("add x0, x0, #4");                                      // the parameter array opens beneath its property row
    emitter.instruction("bl __rt_print_r_open");                                // write the parameter array opening parenthesis
    emitter.instruction("str xzr, [sp, #48]");                                  // initialize the parameter index
    emitter.label("__rt_pr_closure_params_loop");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the parameter index
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the visible parameter count
    emitter.instruction("cmp x9, x10");                                         // rendered every visible parameter?
    emitter.instruction("b.ge __rt_pr_closure_params_finish");                  // close the parameter array when complete
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload the generated parameter-key table pointer
    emitter.instruction("add x11, x11, x9, lsl #4");                            // select this 16-byte parameter key record
    emitter.instruction("ldr x0, [x11]");                                       // load the php-src parameter key pointer
    emitter.instruction("ldr x1, [x11, #8]");                                   // load the php-src parameter key length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the Closure base indent for the parameter row
    emitter.instruction("add x2, x2, #8");                                      // parameter-array entries are eight spaces beneath the Closure block
    emitter.instruction("bl __rt_print_r_str_key");                             // write the parameter key row
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the parameter index after key rendering
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the required/optional split point
    emitter.instruction("cmp x9, x10");                                         // is this parameter required?
    emitter.instruction("b.ge __rt_pr_closure_param_optional");                 // optional and variadic parameters use the optional marker
    abi::emit_symbol_address(emitter, "x1", "_closure_debug_required");
    emitter.instruction("b __rt_pr_closure_param_value");                       // share the required/optional marker writer
    emitter.label("__rt_pr_closure_param_optional");
    abi::emit_symbol_address(emitter, "x1", "_closure_debug_optional");
    emitter.label("__rt_pr_closure_param_value");
    emitter.instruction("mov x2, #10");                                         // both php-src parameter markers are ten bytes
    emitter.instruction("bl __rt_pr_write");                                    // write the required or optional marker
    abi::emit_symbol_address(emitter, "x1", "_pr_nl");
    emitter.instruction("mov x2, #1");                                          // len("\\n") = 1
    emitter.instruction("bl __rt_pr_write");                                    // terminate the parameter entry row
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the parameter index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next visible parameter
    emitter.instruction("str x9, [sp, #48]");                                   // persist the advanced parameter index
    emitter.instruction("b __rt_pr_closure_params_loop");                       // continue rendering parameter metadata
    emitter.label("__rt_pr_closure_params_finish");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the Closure base indent for the parameter array close
    emitter.instruction("add x0, x0, #4");                                      // close the parameter array at its parent indentation
    emitter.instruction("bl __rt_print_r_close");                               // write the parameter array closing parenthesis
    emitter.label("__rt_pr_closure_params_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the Closure parameter print_r frame
    emitter.instruction("ret");                                                 // return to the Closure projection
}

/// Emits the x86_64 `print_r` Closure parameter array from signature metadata.
fn emit_print_r_closure_parameters_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the Closure parameter print_r frame
    emitter.instruction("sub rsp, 80");                                         // allocate descriptor, signature, counters, and alignment slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the Closure descriptor identity
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the caller base indent
    emitter.instruction("mov r9, QWORD PTR [rdi + 32]");                        // load the optional signature side record
    emitter.instruction("test r9, r9");                                         // is a signature record present?
    emitter.instruction("jz __rt_pr_closure_params_done_x86");                  // descriptors without signatures expose no parameter property
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the visible parameter count
    emitter.instruction("test r10, r10");                                       // are there visible parameters?
    emitter.instruction("jz __rt_pr_closure_params_done_x86");                  // zero visible parameters omit the parameter property
    emitter.instruction("mov r11, QWORD PTR [r9 + 96]");                        // load the debug `$name` and `&$name` key table
    emitter.instruction("test r11, r11");                                       // generated debug names must exist for php-src keys
    emitter.instruction("jz __rt_pr_closure_params_done_x86");                  // missing names cannot form the parameter property
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // preserve the visible parameter count
    emitter.instruction("mov r10, QWORD PTR [r9 + 8]");                         // load the required parameter count
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the required/optional split point
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // preserve the generated parameter-key table pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the parameter property row
    emitter.instruction("add rdi, 4");                                          // property rows are four spaces beneath the Closure block
    emitter.instruction("call __rt_print_r_spaces");                            // indent the parameter property row
    abi::emit_symbol_address(emitter, "rsi", "_closure_print_r_key_parameter");
    emitter.instruction("mov edx, 15");                                         // len("[parameter] => ") = 15
    emitter.instruction("call __rt_pr_write");                                  // write the parameter property prefix
    abi::emit_symbol_address(emitter, "rsi", "_pr_array_hdr");
    emitter.instruction("mov edx, 6");                                          // len("Array\\n") = 6
    emitter.instruction("call __rt_pr_write");                                  // write the parameter array header
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the parameter array
    emitter.instruction("add rdi, 4");                                          // the parameter array opens beneath its property row
    emitter.instruction("call __rt_print_r_open");                              // write the parameter array opening parenthesis
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the parameter index
    emitter.label("__rt_pr_closure_params_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the parameter index
    emitter.instruction("cmp r9, QWORD PTR [rbp - 24]");                        // rendered every visible parameter?
    emitter.instruction("jge __rt_pr_closure_params_finish_x86");               // close the parameter array when complete
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the generated parameter-key table pointer
    emitter.instruction("mov r8, r9");                                          // copy the parameter index before scaling its record offset
    emitter.instruction("shl r8, 4");                                           // scale the parameter index by the 16-byte key-record stride
    emitter.instruction("add r11, r8");                                         // select this parameter key record
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load the php-src parameter key pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // load the php-src parameter key length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the parameter row
    emitter.instruction("add rdx, 8");                                          // parameter-array entries are eight spaces beneath the Closure block
    emitter.instruction("call __rt_print_r_str_key");                           // write the parameter key row
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the parameter index after key rendering
    emitter.instruction("cmp r9, QWORD PTR [rbp - 32]");                        // is this parameter required?
    emitter.instruction("jge __rt_pr_closure_param_optional_x86");              // optional and variadic parameters use the optional marker
    abi::emit_symbol_address(emitter, "rsi", "_closure_debug_required");
    emitter.instruction("jmp __rt_pr_closure_param_value_x86");                 // share the required/optional marker writer
    emitter.label("__rt_pr_closure_param_optional_x86");
    abi::emit_symbol_address(emitter, "rsi", "_closure_debug_optional");
    emitter.label("__rt_pr_closure_param_value_x86");
    emitter.instruction("mov edx, 10");                                         // both php-src parameter markers are ten bytes
    emitter.instruction("call __rt_pr_write");                                  // write the required or optional marker
    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");
    emitter.instruction("mov edx, 1");                                          // len("\\n") = 1
    emitter.instruction("call __rt_pr_write");                                  // terminate the parameter entry row
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // advance to the next visible parameter
    emitter.instruction("jmp __rt_pr_closure_params_loop_x86");                 // continue rendering parameter metadata
    emitter.label("__rt_pr_closure_params_finish_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the parameter array close
    emitter.instruction("add rdi, 4");                                          // close the parameter array at its parent indentation
    emitter.instruction("call __rt_print_r_close");                             // write the parameter array closing parenthesis
    emitter.label("__rt_pr_closure_params_done_x86");
    emitter.instruction("add rsp, 80");                                         // release the Closure parameter print_r frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the Closure projection
}

/// Emits the php-src Closure `static` property through the `print_r` walker.
fn emit_print_r_closure_static(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r_closure_static ---");
    emitter.label_global("__rt_print_r_closure_static");
    match emitter.target.arch {
        Arch::AArch64 => emit_print_r_closure_static_aarch64(emitter),
        Arch::X86_64 => emit_print_r_closure_static_x86_64(emitter),
    }
}

/// Emits the AArch64 `print_r` Closure static array from captures and static locals.
fn emit_print_r_closure_static_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #128");                                    // allocate descriptor, tables, cursors, and frame slots
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the Closure static print_r frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // preserve the Closure descriptor and caller base indent
    emitter.instruction("bl __rt_closure_debug_static_count");                  // count captures and persistent static locals
    emitter.instruction("cbz x0, __rt_pr_closure_static_done");                 // omit an empty php-src static property
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the Closure descriptor identity
    emitter.instruction("ldr x9, [x9, #48]");                                   // load the invocation side record pointer
    emitter.instruction("ldr x9, [x9, #56]");                                   // load the true-Closure debug record pointer
    emitter.instruction("ldr x10, [x9, #56]");                                  // load the capture binding-table pointer
    emitter.instruction("ldr x11, [x9, #64]");                                  // load the capture binding count
    emitter.instruction("ldr x12, [x9, #72]");                                  // load the symbol-backed static-local table pointer
    emitter.instruction("ldr x13, [x9, #80]");                                  // load the symbol-backed static-local count
    emitter.instruction("str x10, [sp, #16]");                                  // preserve the capture binding-table pointer
    emitter.instruction("str x11, [sp, #24]");                                  // preserve the capture binding count
    emitter.instruction("str x12, [sp, #48]");                                  // preserve the static-local table pointer
    emitter.instruction("str x13, [sp, #56]");                                  // preserve the static-local count
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the Closure base indent for the static property row
    emitter.instruction("add x0, x0, #4");                                      // property rows are four spaces beneath the Closure block
    emitter.instruction("bl __rt_print_r_spaces");                              // indent the static property row
    abi::emit_symbol_address(emitter, "x1", "_closure_print_r_key_static");
    emitter.instruction("mov x2, #12");                                         // len("[static] => ") = 12
    emitter.instruction("bl __rt_pr_write");                                    // write the static property prefix
    abi::emit_symbol_address(emitter, "x1", "_pr_array_hdr");
    emitter.instruction("mov x2, #6");                                          // len("Array\\n") = 6
    emitter.instruction("bl __rt_pr_write");                                    // write the static array header
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the Closure base indent for the static array
    emitter.instruction("add x0, x0, #4");                                      // the static array opens beneath its property row
    emitter.instruction("bl __rt_print_r_open");                                // write the static array opening parenthesis
    emitter.instruction("str xzr, [sp, #32]");                                  // initialize the capture binding cursor
    emitter.label("__rt_pr_closure_static_capture_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the capture binding cursor
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the capture binding count
    emitter.instruction("cmp x9, x10");                                         // consumed every capture binding?
    emitter.instruction("b.ge __rt_pr_closure_static_locals_begin");            // continue with symbol-backed static locals after captures
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the capture binding-table pointer
    emitter.instruction("add x11, x11, x9, lsl #5");                            // select this 32-byte capture binding record
    emitter.instruction("str x11, [sp, #64]");                                  // preserve the capture metadata through writers
    emitter.instruction("ldr x12, [x11, #8]");                                  // load the capture name length
    emitter.instruction("cmp x12, #4");                                         // only `$this` has the reserved four-byte name
    emitter.instruction("b.ne __rt_pr_closure_static_capture_render");          // ordinary captures belong to the static array
    emitter.instruction("ldr x12, [x11]");                                      // load the candidate capture name pointer
    emitter.instruction("ldr w12, [x12]");                                      // load the candidate name's first four bytes
    emitter.instruction("movz w13, #0x6874");                                   // low half of `this` in little-endian order
    emitter.instruction("movk w13, #0x7369, lsl #16");                          // high half of `this` in little-endian order
    emitter.instruction("cmp w12, w13");                                        // is this the reserved `$this` binding?
    emitter.instruction("b.eq __rt_pr_closure_static_capture_next");            // `$this` renders as its own Closure property
    emitter.label("__rt_pr_closure_static_capture_render");
    emitter.instruction("ldr x11, [sp, #64]");                                  // reload the capture metadata after the `$this` check
    emitter.instruction("ldr x0, [x11]");                                       // load the capture name pointer
    emitter.instruction("ldr x1, [x11, #8]");                                   // load the capture name length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the Closure base indent for the capture row
    emitter.instruction("add x2, x2, #8");                                      // static-array entries are eight spaces beneath the Closure block
    emitter.instruction("bl __rt_print_r_str_key");                             // write the captured-variable key row
    emitter.instruction("ldr x11, [sp, #64]");                                  // reload the capture metadata after key rendering
    emitter.instruction("ldr x0, [x11, #16]");                                  // load the capture runtime type tag
    emitter.instruction("ldr x12, [x11, #24]");                                 // load the by-reference capture flag
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the capture binding cursor for descriptor storage lookup
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the Closure descriptor identity
    emitter.instruction("add x9, x9, #4");                                      // skip the 64-byte descriptor header in capture units
    emitter.instruction("add x13, x10, x9, lsl #4");                            // resolve the current 16-byte capture slot address
    emitter.instruction("ldr x1, [x13]");                                       // load the capture low payload word
    emitter.instruction("ldr x2, [x13, #8]");                                   // load the capture high payload word
    emitter.instruction("cbz x12, __rt_pr_closure_static_capture_value");       // by-value captures occupy their descriptor slot directly
    emitter.instruction("mov x10, x1");                                         // reference captures store a heap cell pointer in the low word
    emitter.instruction("ldr x1, [x10]");                                       // load the live low payload from the reference cell
    emitter.instruction("cmp x0, #1");                                          // does the live reference hold a string pair?
    emitter.instruction("b.ne __rt_pr_closure_static_capture_ref_nonstring");   // non-string reference cells have one meaningful payload word
    emitter.instruction("ldr x2, [x10, #8]");                                   // load the live string length from the reference cell
    emitter.instruction("b __rt_pr_closure_static_capture_value");              // render the live reference string value
    emitter.label("__rt_pr_closure_static_capture_ref_nonstring");
    emitter.instruction("mov x2, xzr");                                         // non-string reference captures have no high payload word
    emitter.label("__rt_pr_closure_static_capture_value");
    emitter.instruction("ldr x3, [sp, #8]");                                    // reload the Closure base indent for nested capture values
    emitter.instruction("add x3, x3, #12");                                     // nested capture containers begin below their entry indentation
    emitter.instruction("bl __rt_print_r_value");                               // render the live capture value
    abi::emit_symbol_address(emitter, "x1", "_pr_nl");
    emitter.instruction("mov x2, #1");                                          // len("\\n") = 1
    emitter.instruction("bl __rt_pr_write");                                    // terminate the capture entry row
    emitter.label("__rt_pr_closure_static_capture_next");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the capture binding cursor
    emitter.instruction("add x9, x9, #1");                                      // advance to the next capture binding
    emitter.instruction("str x9, [sp, #32]");                                   // persist the advanced capture cursor
    emitter.instruction("b __rt_pr_closure_static_capture_loop");               // continue rendering captured static values
    emitter.label("__rt_pr_closure_static_locals_begin");
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize the symbol-backed static-local cursor
    emitter.label("__rt_pr_closure_static_locals_loop");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the symbol-backed static-local cursor
    emitter.instruction("ldr x10, [sp, #56]");                                  // reload the symbol-backed static-local count
    emitter.instruction("cmp x9, x10");                                         // rendered every persistent static local?
    emitter.instruction("b.ge __rt_pr_closure_static_finish");                  // close the static array after every static local was rendered
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the symbol-backed static-local table pointer
    emitter.instruction("add x11, x11, x9, lsl #5");                            // select this 32-byte static-local metadata record
    emitter.instruction("str x11, [sp, #64]");                                  // preserve the static-local metadata through writers
    emitter.instruction("ldr x0, [x11]");                                       // load the static-local name pointer
    emitter.instruction("ldr x1, [x11, #8]");                                   // load the static-local name length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the Closure base indent for the static-local row
    emitter.instruction("add x2, x2, #8");                                      // static-array entries are eight spaces beneath the Closure block
    emitter.instruction("bl __rt_print_r_str_key");                             // write the persistent static-local key row
    emitter.instruction("ldr x11, [sp, #64]");                                  // reload the static-local metadata after key rendering
    emitter.instruction("ldr x0, [x11, #16]");                                  // load the static-local runtime type tag
    emitter.instruction("ldr x10, [x11, #24]");                                 // load the static-local storage symbol address
    emitter.instruction("ldr x1, [x10]");                                       // load the static-local low payload word
    emitter.instruction("ldr x2, [x10, #8]");                                   // load the static-local high payload word
    emitter.instruction("ldr x3, [sp, #8]");                                    // reload the Closure base indent for nested static-local values
    emitter.instruction("add x3, x3, #12");                                     // nested static-local containers begin below their entry indentation
    emitter.instruction("bl __rt_print_r_value");                               // render the live persistent static-local value
    abi::emit_symbol_address(emitter, "x1", "_pr_nl");
    emitter.instruction("mov x2, #1");                                          // len("\\n") = 1
    emitter.instruction("bl __rt_pr_write");                                    // terminate the static-local entry row
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the symbol-backed static-local cursor
    emitter.instruction("add x9, x9, #1");                                      // advance to the next static-local metadata record
    emitter.instruction("str x9, [sp, #40]");                                   // persist the advanced static-local cursor
    emitter.instruction("b __rt_pr_closure_static_locals_loop");                // continue rendering persistent static-local values
    emitter.label("__rt_pr_closure_static_finish");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the Closure base indent for the static array close
    emitter.instruction("add x0, x0, #4");                                      // close the static array at its parent indentation
    emitter.instruction("bl __rt_print_r_close");                               // write the static array closing parenthesis
    emitter.label("__rt_pr_closure_static_done");
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the Closure static print_r frame
    emitter.instruction("ret");                                                 // return to the Closure projection
}

/// Emits the x86_64 `print_r` Closure static array from captures and static locals.
fn emit_print_r_closure_static_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the Closure static print_r frame
    emitter.instruction("sub rsp, 128");                                        // allocate descriptor, tables, cursors, and alignment slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the Closure descriptor identity
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the caller base indent
    emitter.instruction("call __rt_closure_debug_static_count");                // count captures and persistent static locals
    emitter.instruction("test rax, rax");                                       // does php-src expose a non-empty static property?
    emitter.instruction("jz __rt_pr_closure_static_done_x86");                  // omit an empty static property
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the Closure descriptor identity
    emitter.instruction("mov r9, QWORD PTR [r9 + 48]");                         // load the invocation side record pointer
    emitter.instruction("mov r9, QWORD PTR [r9 + 56]");                         // load the true-Closure debug record pointer
    emitter.instruction("mov r10, QWORD PTR [r9 + 56]");                        // load the capture binding-table pointer
    emitter.instruction("mov r11, QWORD PTR [r9 + 64]");                        // load the capture binding count
    emitter.instruction("mov r8, QWORD PTR [r9 + 72]");                         // load the symbol-backed static-local table pointer
    emitter.instruction("mov r9, QWORD PTR [r9 + 80]");                         // load the symbol-backed static-local count
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // preserve the capture binding-table pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // preserve the capture binding count
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // preserve the static-local table pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // preserve the static-local count
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the static property row
    emitter.instruction("add rdi, 4");                                          // property rows are four spaces beneath the Closure block
    emitter.instruction("call __rt_print_r_spaces");                            // indent the static property row
    abi::emit_symbol_address(emitter, "rsi", "_closure_print_r_key_static");
    emitter.instruction("mov edx, 12");                                         // len("[static] => ") = 12
    emitter.instruction("call __rt_pr_write");                                  // write the static property prefix
    abi::emit_symbol_address(emitter, "rsi", "_pr_array_hdr");
    emitter.instruction("mov edx, 6");                                          // len("Array\\n") = 6
    emitter.instruction("call __rt_pr_write");                                  // write the static array header
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the static array
    emitter.instruction("add rdi, 4");                                          // the static array opens beneath its property row
    emitter.instruction("call __rt_print_r_open");                              // write the static array opening parenthesis
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the capture binding cursor
    emitter.label("__rt_pr_closure_static_capture_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the capture binding cursor
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // consumed every capture binding?
    emitter.instruction("jge __rt_pr_closure_static_locals_begin_x86");         // continue with symbol-backed static locals after captures
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the capture binding-table pointer
    emitter.instruction("mov r8, rcx");                                         // copy the capture cursor before scaling its record offset
    emitter.instruction("shl r8, 5");                                           // scale the capture cursor by the 32-byte metadata stride
    emitter.instruction("add r11, r8");                                         // select this capture binding record
    emitter.instruction("mov QWORD PTR [rbp - 72], r11");                       // preserve the capture metadata through writers
    emitter.instruction("cmp QWORD PTR [r11 + 8], 4");                          // only `$this` has the reserved four-byte name
    emitter.instruction("jne __rt_pr_closure_static_capture_render_x86");       // ordinary captures belong to the static array
    emitter.instruction("mov r9, QWORD PTR [r11]");                             // load the candidate capture name pointer
    emitter.instruction("cmp DWORD PTR [r9], 0x73696874");                      // is this the reserved `$this` binding?
    emitter.instruction("je __rt_pr_closure_static_capture_next_x86");          // `$this` renders as its own Closure property
    emitter.label("__rt_pr_closure_static_capture_render_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 72]");                       // reload the capture metadata after the `$this` check
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load the capture name pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // load the capture name length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the capture row
    emitter.instruction("add rdx, 8");                                          // static-array entries are eight spaces beneath the Closure block
    emitter.instruction("call __rt_print_r_str_key");                           // write the captured-variable key row
    emitter.instruction("mov r11, QWORD PTR [rbp - 72]");                       // reload the capture metadata after key rendering
    emitter.instruction("mov rdi, QWORD PTR [r11 + 16]");                       // load the capture runtime type tag
    emitter.instruction("mov r10, QWORD PTR [r11 + 24]");                       // load the by-reference capture flag
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the capture cursor for descriptor storage lookup
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor identity
    emitter.instruction("add rcx, 4");                                          // skip the 64-byte descriptor header in capture units
    emitter.instruction("shl rcx, 4");                                          // scale the capture index by the 16-byte slot stride
    emitter.instruction("add rax, rcx");                                        // resolve the current 16-byte capture slot address
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // load the capture low payload word
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // load the capture high payload word
    emitter.instruction("test r10, r10");                                       // does this capture use a live reference cell?
    emitter.instruction("jz __rt_pr_closure_static_capture_value_x86");         // by-value captures occupy their descriptor slot directly
    emitter.instruction("mov rax, rsi");                                        // reference captures store a heap cell pointer in the low word
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // load the live low payload from the reference cell
    emitter.instruction("cmp rdi, 1");                                          // does the live reference hold a string pair?
    emitter.instruction("jne __rt_pr_closure_static_capture_ref_nonstring_x86"); // non-string reference cells have one meaningful payload word
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // load the live string length from the reference cell
    emitter.instruction("jmp __rt_pr_closure_static_capture_value_x86");        // render the live reference string value
    emitter.label("__rt_pr_closure_static_capture_ref_nonstring_x86");
    emitter.instruction("xor edx, edx");                                        // non-string reference captures have no high payload word
    emitter.label("__rt_pr_closure_static_capture_value_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for nested capture values
    emitter.instruction("add rcx, 12");                                         // nested capture containers begin below their entry indentation
    emitter.instruction("call __rt_print_r_value");                             // render the live capture value
    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");
    emitter.instruction("mov edx, 1");                                          // len("\\n") = 1
    emitter.instruction("call __rt_pr_write");                                  // terminate the capture entry row
    emitter.label("__rt_pr_closure_static_capture_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // advance to the next capture binding
    emitter.instruction("jmp __rt_pr_closure_static_capture_loop_x86");         // continue rendering captured static values
    emitter.label("__rt_pr_closure_static_locals_begin_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the symbol-backed static-local cursor
    emitter.label("__rt_pr_closure_static_locals_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the symbol-backed static-local cursor
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 64]");                       // rendered every persistent static local?
    emitter.instruction("jge __rt_pr_closure_static_finish_x86");               // close the static array after every static local was rendered
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the symbol-backed static-local table pointer
    emitter.instruction("mov r8, rcx");                                         // copy the static-local cursor before scaling its record offset
    emitter.instruction("shl r8, 5");                                           // scale the static-local cursor by the 32-byte metadata stride
    emitter.instruction("add r11, r8");                                         // select this static-local metadata record
    emitter.instruction("mov QWORD PTR [rbp - 72], r11");                       // preserve the static-local metadata through writers
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load the static-local name pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // load the static-local name length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the static-local row
    emitter.instruction("add rdx, 8");                                          // static-array entries are eight spaces beneath the Closure block
    emitter.instruction("call __rt_print_r_str_key");                           // write the persistent static-local key row
    emitter.instruction("mov r11, QWORD PTR [rbp - 72]");                       // reload the static-local metadata after key rendering
    emitter.instruction("mov rdi, QWORD PTR [r11 + 16]");                       // load the static-local runtime type tag
    emitter.instruction("mov r10, QWORD PTR [r11 + 24]");                       // load the static-local storage symbol address
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the static-local low payload word
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the static-local high payload word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for nested static-local values
    emitter.instruction("add rcx, 12");                                         // nested static-local containers begin below their entry indentation
    emitter.instruction("call __rt_print_r_value");                             // render the live persistent static-local value
    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");
    emitter.instruction("mov edx, 1");                                          // len("\\n") = 1
    emitter.instruction("call __rt_pr_write");                                  // terminate the static-local entry row
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // advance to the next static-local metadata record
    emitter.instruction("jmp __rt_pr_closure_static_locals_loop_x86");          // continue rendering persistent static-local values
    emitter.label("__rt_pr_closure_static_finish_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the static array close
    emitter.instruction("add rdi, 4");                                          // close the static array at its parent indentation
    emitter.instruction("call __rt_print_r_close");                             // write the static array closing parenthesis
    emitter.label("__rt_pr_closure_static_done_x86");
    emitter.instruction("add rsp, 128");                                        // release the Closure static print_r frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the Closure projection
}

/// Emits the Closure `$this` debug field from its live descriptor capture slot.
fn emit_var_dump_closure_this(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_closure_this ---");
    emitter.label_global("__rt_var_dump_closure_this");
    match emitter.target.arch {
        Arch::AArch64 => emit_var_dump_closure_this_aarch64(emitter),
        Arch::X86_64 => emit_var_dump_closure_this_x86_64(emitter),
    }
}

/// Emits the AArch64 live `$this` Closure debug field.
fn emit_var_dump_closure_this_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #64");                                     // allocate descriptor, payload, tag, and frame slots
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the Closure `$this` frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the Closure descriptor identity
    emitter.instruction("bl __rt_closure_debug_this_present");                  // reject unbound top-level Closure receivers
    emitter.instruction("cbz x0, __rt_vd_closure_this_done");                   // omit `$this` when no object is bound
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the Closure descriptor identity
    emitter.instruction("ldr x9, [x9, #48]");                                   // load the invocation side record pointer
    emitter.instruction("ldr x9, [x9, #56]");                                   // load the true-Closure debug record pointer
    emitter.instruction("ldr x10, [x9, #56]");                                  // load the capture binding-table pointer
    emitter.instruction("ldr x11, [x9, #64]");                                  // load the capture binding count
    emitter.instruction("mov x12, #0");                                         // initialize the capture binding cursor
    emitter.label("__rt_vd_closure_this_scan");
    emitter.instruction("cmp x12, x11");                                        // consumed every capture binding?
    emitter.instruction("b.ge __rt_vd_closure_this_done");                      // presence helper guarantees this path is defensive only
    emitter.instruction("add x13, x10, x12, lsl #5");                           // select this 32-byte capture binding record
    emitter.instruction("ldr x14, [x13, #8]");                                  // load the capture name length
    emitter.instruction("cmp x14, #4");                                         // only `$this` has the reserved four-byte name
    emitter.instruction("b.ne __rt_vd_closure_this_next");                      // another capture cannot provide the Closure receiver
    emitter.instruction("ldr x14, [x13]");                                      // load the capture name pointer
    emitter.instruction("ldr w14, [x14]");                                      // load the first four bytes of the capture name
    emitter.instruction("movz w15, #0x6874");                                   // low half of `this` in little-endian order
    emitter.instruction("movk w15, #0x7369, lsl #16");                          // high half of `this` in little-endian order
    emitter.instruction("cmp w14, w15");                                        // is this the reserved `$this` binding?
    emitter.instruction("b.ne __rt_vd_closure_this_next");                      // another four-byte capture is not `$this`
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the Closure descriptor identity
    emitter.instruction("add x12, x12, #4");                                    // skip the 64-byte descriptor header in capture units
    emitter.instruction("add x9, x9, x12, lsl #4");                             // resolve the `$this` capture slot address
    emitter.instruction("ldr x0, [x13, #16]");                                  // load the `$this` capture runtime type tag
    emitter.instruction("ldr x1, [x9]");                                        // load the `$this` capture low payload word
    emitter.instruction("ldr x2, [x9, #8]");                                    // load the `$this` capture high payload word
    emitter.instruction("cmp x0, #7");                                          // top-level Closures capture boxed Mixed receivers
    emitter.instruction("b.ne __rt_vd_closure_this_store");                     // method-defined Closures capture raw objects directly
    emitter.instruction("mov x0, x1");                                          // pass the boxed Mixed receiver to the unbox helper
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=tag x1=payload after unboxing `$this`
    emitter.instruction("mov x2, xzr");                                         // unboxed object payload has no high word
    emitter.label("__rt_vd_closure_this_store");
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the live `$this` runtime tag through key rendering
    emitter.instruction("str x1, [sp, #24]");                                   // preserve the live `$this` low payload through key rendering
    emitter.instruction("str x2, [sp, #32]");                                   // preserve the live `$this` high payload through key rendering
    abi::emit_symbol_address(emitter, "x1", "_closure_debug_key_this");
    emitter.instruction("mov x2, #8");                                          // len("[\"this\"]") = 8
    emitter.instruction("bl __rt_var_dump_emit_object_key");                    // write the `$this` property key line
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the live `$this` runtime tag
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the live `$this` low payload
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the live `$this` high payload
    emitter.instruction("bl __rt_var_dump_value");                              // render the bound `$this` object value
    emitter.instruction("b __rt_vd_closure_this_done");                         // finish after rendering the unique `$this` binding
    emitter.label("__rt_vd_closure_this_next");
    emitter.instruction("add x12, x12, #1");                                    // advance to the next capture binding
    emitter.instruction("b __rt_vd_closure_this_scan");                         // continue scanning for `$this`
    emitter.label("__rt_vd_closure_this_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the Closure `$this` frame
    emitter.instruction("ret");                                                 // return to the Closure object projection
}

/// Emits the x86_64 live `$this` Closure debug field.
fn emit_var_dump_closure_this_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the Closure `$this` frame
    emitter.instruction("sub rsp, 64");                                         // allocate descriptor, payload, tag, and alignment slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the Closure descriptor identity
    emitter.instruction("call __rt_closure_debug_this_present");                // reject unbound top-level Closure receivers
    emitter.instruction("test rax, rax");                                       // is an object actually bound as `$this`?
    emitter.instruction("jz __rt_vd_closure_this_done");                        // omit `$this` when no object is bound
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the Closure descriptor identity
    emitter.instruction("mov r9, QWORD PTR [r9 + 48]");                         // load the invocation side record pointer
    emitter.instruction("mov r9, QWORD PTR [r9 + 56]");                         // load the true-Closure debug record pointer
    emitter.instruction("mov r10, QWORD PTR [r9 + 56]");                        // load the capture binding-table pointer
    emitter.instruction("mov r11, QWORD PTR [r9 + 64]");                        // load the capture binding count
    emitter.instruction("xor ecx, ecx");                                        // initialize the capture binding cursor
    emitter.label("__rt_vd_closure_this_scan");
    emitter.instruction("cmp rcx, r11");                                        // consumed every capture binding?
    emitter.instruction("jge __rt_vd_closure_this_done");                       // presence helper guarantees this path is defensive only
    emitter.instruction("mov r8, rcx");                                         // copy the cursor before scaling the binding offset
    emitter.instruction("shl r8, 5");                                           // scale the cursor by the 32-byte binding stride
    emitter.instruction("add r8, r10");                                         // select this capture binding record
    emitter.instruction("cmp QWORD PTR [r8 + 8], 4");                           // only `$this` has the reserved four-byte name
    emitter.instruction("jne __rt_vd_closure_this_next");                       // another capture cannot provide the Closure receiver
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the capture name pointer
    emitter.instruction("cmp DWORD PTR [r9], 0x73696874");                      // is this the reserved `$this` binding?
    emitter.instruction("jne __rt_vd_closure_this_next");                       // another four-byte capture is not `$this`
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor identity
    emitter.instruction("add rcx, 4");                                          // skip the 64-byte descriptor header in capture units
    emitter.instruction("shl rcx, 4");                                          // scale the `$this` capture slot offset by sixteen bytes
    emitter.instruction("add rax, rcx");                                        // resolve the `$this` capture slot address
    emitter.instruction("mov rdi, QWORD PTR [r8 + 16]");                        // load the `$this` capture runtime type tag
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // load the `$this` capture low payload word
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // load the `$this` capture high payload word
    emitter.instruction("cmp rdi, 7");                                          // top-level Closures capture boxed Mixed receivers
    emitter.instruction("jne __rt_vd_closure_this_store");                      // method-defined Closures capture raw objects directly
    emitter.instruction("mov rax, rsi");                                        // pass the boxed Mixed receiver to the unbox helper
    emitter.instruction("call __rt_mixed_unbox");                               // rax=tag rdi=payload after unboxing `$this`
    emitter.instruction("mov rsi, rdi");                                        // unboxed object payload becomes the var_dump low word
    emitter.instruction("mov rdi, rax");                                        // unboxed runtime tag becomes the var_dump type argument
    emitter.instruction("xor edx, edx");                                        // unboxed object payload has no high word
    emitter.label("__rt_vd_closure_this_store");
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // preserve the live `$this` runtime tag through key rendering
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // preserve the live `$this` low payload through key rendering
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // preserve the live `$this` high payload through key rendering
    abi::emit_symbol_address(emitter, "rdi", "_closure_debug_key_this");
    emitter.instruction("mov esi, 8");                                          // len("[\"this\"]") = 8
    emitter.instruction("call __rt_var_dump_emit_object_key");                  // write the `$this` property key line
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the live `$this` runtime tag
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the live `$this` low payload
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the live `$this` high payload
    emitter.instruction("call __rt_var_dump_value");                            // render the bound `$this` object value
    emitter.instruction("jmp __rt_vd_closure_this_done");                       // finish after rendering the unique `$this` binding
    emitter.label("__rt_vd_closure_this_next");
    emitter.instruction("add rcx, 1");                                          // advance to the next capture binding
    emitter.instruction("jmp __rt_vd_closure_this_scan");                       // continue scanning for `$this`
    emitter.label("__rt_vd_closure_this_done");
    emitter.instruction("add rsp, 64");                                         // release the Closure `$this` frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the Closure object projection
}

/// Emits whether a Closure descriptor currently carries a non-null bound `$this` object.
fn emit_closure_debug_this_present(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: closure_debug_this_present ---");
    emitter.label_global("__rt_closure_debug_this_present");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // preserve the descriptor while Mixed `$this` values are unboxed
            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address
            emitter.instruction("mov x29, sp");                                 // establish the `$this` presence frame
            emitter.instruction("str x0, [sp, #0]");                            // preserve the Closure descriptor identity
            emitter.instruction("ldr x9, [x0, #48]");                           // load the invocation side record pointer
            emitter.instruction("cbz x9, __rt_closure_this_absent");            // fake descriptors carry no true-Closure `$this` binding
            emitter.instruction("ldr x9, [x9, #56]");                           // load the optional debug record pointer
            emitter.instruction("cbz x9, __rt_closure_this_absent");            // non-Closure descriptors expose no `$this` binding
            emitter.instruction("ldr x10, [x9, #56]");                          // load the capture binding-table pointer
            emitter.instruction("ldr x11, [x9, #64]");                          // load the capture binding count
            emitter.instruction("mov x12, #0");                                 // initialize the capture binding cursor
            emitter.label("__rt_closure_this_scan");
            emitter.instruction("cmp x12, x11");                                // consumed every capture binding?
            emitter.instruction("b.ge __rt_closure_this_absent");               // no `$this` binding means no bound receiver
            emitter.instruction("add x13, x10, x12, lsl #5");                   // select this 32-byte capture binding record
            emitter.instruction("ldr x14, [x13, #8]");                          // load the capture name length
            emitter.instruction("cmp x14, #4");                                 // only `$this` has the reserved four-byte name
            emitter.instruction("b.ne __rt_closure_this_next");                 // another capture cannot provide the Closure receiver
            emitter.instruction("ldr x14, [x13]");                              // load the capture name pointer
            emitter.instruction("ldr w14, [x14]");                              // load the first four bytes of the capture name
            emitter.instruction("movz w15, #0x6874");                           // low half of `this` in little-endian order
            emitter.instruction("movk w15, #0x7369, lsl #16");                  // high half of `this` in little-endian order
            emitter.instruction("cmp w14, w15");                                // is this the reserved `$this` binding?
            emitter.instruction("b.ne __rt_closure_this_next");                 // another four-byte capture is not `$this`
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the Closure descriptor identity
            emitter.instruction("add x12, x12, #4");                            // skip the 64-byte descriptor header in capture units
            emitter.instruction("add x0, x0, x12, lsl #4");                     // resolve the current `$this` capture slot address
            emitter.instruction("ldr x1, [x0]");                                // load the `$this` capture payload
            emitter.instruction("ldr x0, [x13, #16]");                          // load the `$this` capture runtime type tag
            emitter.instruction("cmp x0, #6");                                  // method-defined Closures capture raw objects
            emitter.instruction("b.eq __rt_closure_this_object");               // test the raw object pointer directly
            emitter.instruction("cmp x0, #7");                                  // top-level Closures capture boxed Mixed receivers
            emitter.instruction("b.ne __rt_closure_this_absent");               // unsupported receiver representation is unbound
            emitter.instruction("mov x0, x1");                                  // pass the boxed Mixed receiver to the unbox helper
            emitter.instruction("bl __rt_mixed_unbox");                         // x0=tag x1=payload after unboxing `$this`
            emitter.instruction("cmp x0, #6");                                  // is the boxed receiver an object?
            emitter.instruction("b.ne __rt_closure_this_absent");               // null/non-object Mixed receivers are unbound
            emitter.instruction("cbz x1, __rt_closure_this_absent");            // a null object payload is unbound
            emitter.instruction("mov x0, #1");                                  // report a bound `$this` object
            emitter.instruction("b __rt_closure_this_return");                  // return the positive presence result
            emitter.label("__rt_closure_this_object");
            emitter.instruction("cbz x1, __rt_closure_this_absent");            // a null raw object payload is unbound
            emitter.instruction("mov x0, #1");                                  // report a bound raw-object `$this`
            emitter.instruction("b __rt_closure_this_return");                  // return the positive presence result
            emitter.label("__rt_closure_this_next");
            emitter.instruction("add x12, x12, #1");                            // advance to the next capture binding
            emitter.instruction("b __rt_closure_this_scan");                    // continue scanning for `$this`
            emitter.label("__rt_closure_this_absent");
            emitter.instruction("mov x0, #0");                                  // report that the Closure has no bound `$this`
            emitter.label("__rt_closure_this_return");
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #32");                             // release the `$this` presence frame
            emitter.instruction("ret");                                         // return the `$this` presence boolean
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // save caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the `$this` presence frame
            emitter.instruction("sub rsp, 32");                                 // preserve the descriptor while Mixed `$this` values are unboxed
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // preserve the Closure descriptor identity
            emitter.instruction("mov r9, QWORD PTR [rdi + 48]");                // load the invocation side record pointer
            emitter.instruction("test r9, r9");                                 // is invocation metadata available?
            emitter.instruction("jz __rt_closure_this_absent");                 // fake descriptors carry no true-Closure `$this` binding
            emitter.instruction("mov r9, QWORD PTR [r9 + 56]");                 // load the optional debug record pointer
            emitter.instruction("test r9, r9");                                 // is this a true Closure descriptor?
            emitter.instruction("jz __rt_closure_this_absent");                 // non-Closure descriptors expose no `$this` binding
            emitter.instruction("mov r10, QWORD PTR [r9 + 56]");                // load the capture binding-table pointer
            emitter.instruction("mov r11, QWORD PTR [r9 + 64]");                // load the capture binding count
            emitter.instruction("xor ecx, ecx");                                // initialize the capture binding cursor
            emitter.label("__rt_closure_this_scan");
            emitter.instruction("cmp rcx, r11");                                // consumed every capture binding?
            emitter.instruction("jge __rt_closure_this_absent");                // no `$this` binding means no bound receiver
            emitter.instruction("mov r8, rcx");                                 // copy the cursor before scaling the binding offset
            emitter.instruction("shl r8, 5");                                   // scale the cursor by the 32-byte binding stride
            emitter.instruction("add r8, r10");                                 // select this capture binding record
            emitter.instruction("cmp QWORD PTR [r8 + 8], 4");                   // only `$this` has the reserved four-byte name
            emitter.instruction("jne __rt_closure_this_next");                  // another capture cannot provide the Closure receiver
            emitter.instruction("mov r9, QWORD PTR [r8]");                      // load the capture name pointer
            emitter.instruction("cmp DWORD PTR [r9], 0x73696874");              // is this the reserved `$this` binding?
            emitter.instruction("jne __rt_closure_this_next");                  // another four-byte capture is not `$this`
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // reload the Closure descriptor identity
            emitter.instruction("add rcx, 4");                                  // skip the 64-byte descriptor header in capture units
            emitter.instruction("shl rcx, 4");                                  // scale the capture slot offset by sixteen bytes
            emitter.instruction("add rax, rcx");                                // resolve the current `$this` capture slot address
            emitter.instruction("mov rsi, QWORD PTR [rax]");                    // load the `$this` capture payload
            emitter.instruction("mov rdi, QWORD PTR [r8 + 16]");                // load the `$this` capture runtime type tag
            emitter.instruction("cmp rdi, 6");                                  // method-defined Closures capture raw objects
            emitter.instruction("je __rt_closure_this_object");                 // test the raw object pointer directly
            emitter.instruction("cmp rdi, 7");                                  // top-level Closures capture boxed Mixed receivers
            emitter.instruction("jne __rt_closure_this_absent");                // unsupported receiver representation is unbound
            emitter.instruction("mov rax, rsi");                                // pass the boxed Mixed receiver to the unbox helper
            emitter.instruction("call __rt_mixed_unbox");                       // rax=tag rdi=payload after unboxing `$this`
            emitter.instruction("cmp rax, 6");                                  // is the boxed receiver an object?
            emitter.instruction("jne __rt_closure_this_absent");                // null/non-object Mixed receivers are unbound
            emitter.instruction("test rdi, rdi");                               // does the boxed object payload exist?
            emitter.instruction("jz __rt_closure_this_absent");                 // a null object payload is unbound
            emitter.instruction("mov eax, 1");                                  // report a bound `$this` object
            emitter.instruction("jmp __rt_closure_this_return");                // return the positive presence result
            emitter.label("__rt_closure_this_object");
            emitter.instruction("test rsi, rsi");                               // does the raw object payload exist?
            emitter.instruction("jz __rt_closure_this_absent");                 // a null raw object payload is unbound
            emitter.instruction("mov eax, 1");                                  // report a bound raw-object `$this`
            emitter.instruction("jmp __rt_closure_this_return");                // return the positive presence result
            emitter.label("__rt_closure_this_next");
            emitter.instruction("add rcx, 1");                                  // advance to the next capture binding
            emitter.instruction("jmp __rt_closure_this_scan");                  // continue scanning for `$this`
            emitter.label("__rt_closure_this_absent");
            emitter.instruction("xor eax, eax");                                // report that the Closure has no bound `$this`
            emitter.label("__rt_closure_this_return");
            emitter.instruction("add rsp, 32");                                 // release the `$this` presence frame
            emitter.instruction("pop rbp");                                     // restore caller frame pointer
            emitter.instruction("ret");                                         // return the `$this` presence boolean
        }
    }
}

/// Emits the Closure `static` debug array from the live runtime capture slots.
fn emit_var_dump_closure_static(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_closure_static ---");
    emitter.label_global("__rt_var_dump_closure_static");
    match emitter.target.arch {
        Arch::AArch64 => emit_var_dump_closure_static_aarch64(emitter),
        Arch::X86_64 => emit_var_dump_closure_static_x86_64(emitter),
    }
}

/// Emits the AArch64 Closure `static` capture projection.
fn emit_var_dump_closure_static_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #96");                                     // allocate descriptor, metadata, cursor, and frame slots
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the Closure static-capture frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the Closure descriptor identity
    emitter.instruction("bl __rt_closure_debug_static_count");                  // count non-`$this` live capture bindings
    emitter.instruction("cbz x0, __rt_vd_closure_static_done");                 // omit an empty php-src static array
    emitter.instruction("str x0, [sp, #40]");                                   // preserve the visible static-capture count
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the Closure descriptor identity
    emitter.instruction("ldr x9, [x9, #48]");                                   // load the invocation side record pointer
    emitter.instruction("ldr x9, [x9, #56]");                                   // load the true-Closure debug record pointer
    emitter.instruction("ldr x10, [x9, #56]");                                  // load the capture binding-table pointer
    emitter.instruction("ldr x11, [x9, #64]");                                  // load the capture binding count
    emitter.instruction("ldr x12, [x9, #72]");                                  // load the symbol-backed static-local table pointer
    emitter.instruction("ldr x13, [x9, #80]");                                  // load the symbol-backed static-local count
    emitter.instruction("str x10, [sp, #8]");                                   // preserve the binding table pointer
    emitter.instruction("str x11, [sp, #16]");                                  // preserve the binding-table count
    emitter.instruction("str x12, [sp, #56]");                                  // preserve the static-local table pointer
    emitter.instruction("str x13, [sp, #64]");                                  // preserve the static-local count
    abi::emit_symbol_address(emitter, "x1", "_closure_debug_key_static");
    emitter.instruction("mov x2, #10");                                         // len("[\"static\"]") = 10
    emitter.instruction("bl __rt_var_dump_emit_object_key");                    // write the static property key line
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the visible static-capture count
    emitter.instruction("bl __rt_var_dump_open_counted_array");                 // write the static array header
    emitter.instruction("bl __rt_vd_indent_push");                              // capture rows are nested one level deeper
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the binding-table cursor
    emitter.label("__rt_vd_closure_static_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the binding-table cursor
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the binding-table count
    emitter.instruction("cmp x9, x10");                                         // consumed every capture binding?
    emitter.instruction("b.ge __rt_vd_closure_static_locals_begin");            // continue with symbol-backed static locals after captures
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the binding-table pointer
    emitter.instruction("add x11, x11, x9, lsl #5");                            // select this 32-byte capture binding record
    emitter.instruction("str x11, [sp, #48]");                                  // preserve the current binding record through key rendering
    emitter.instruction("ldr x12, [x11, #8]");                                  // load the capture name length
    emitter.instruction("cmp x12, #4");                                         // only `$this` has the reserved four-byte name
    emitter.instruction("b.ne __rt_vd_closure_static_render");                  // non-`$this` capture belongs to the static array
    emitter.instruction("ldr x12, [x11]");                                      // load the capture name pointer
    emitter.instruction("ldr w12, [x12]");                                      // load the first four bytes of the capture name
    emitter.instruction("movz w13, #0x6874");                                   // low half of `this` in little-endian order
    emitter.instruction("movk w13, #0x7369, lsl #16");                          // high half of `this` in little-endian order
    emitter.instruction("cmp w12, w13");                                        // is this the reserved `$this` binding?
    emitter.instruction("b.eq __rt_vd_closure_static_next");                    // `$this` is rendered as its own Closure debug field
    emitter.label("__rt_vd_closure_static_render");
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the current capture binding record
    emitter.instruction("ldr x1, [x11]");                                       // load the capture name pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // load the capture name length
    emitter.instruction("bl __rt_var_dump_emit_string_key");                    // write the captured-variable key line
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the current capture binding record
    emitter.instruction("ldr x0, [x11, #16]");                                  // load the capture runtime type tag
    emitter.instruction("ldr x12, [x11, #24]");                                 // load the by-reference capture flag
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the binding-table cursor for capture storage lookup
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the Closure descriptor identity
    emitter.instruction("add x9, x9, #4");                                      // skip the 64-byte descriptor header in 16-byte capture units
    emitter.instruction("add x13, x10, x9, lsl #4");                            // resolve the current 16-byte capture slot address
    emitter.instruction("ldr x1, [x13]");                                       // load the capture low payload word
    emitter.instruction("ldr x2, [x13, #8]");                                   // load the capture high payload word
    emitter.instruction("cbz x12, __rt_vd_closure_static_value");               // by-value captures already occupy their descriptor slot directly
    emitter.instruction("mov x10, x1");                                         // reference captures store a heap cell pointer in the low word
    emitter.instruction("ldr x1, [x10]");                                       // load the live low payload from the reference cell
    emitter.instruction("cmp x0, #1");                                          // does the live reference hold a string pair?
    emitter.instruction("b.ne __rt_vd_closure_static_ref_nonstring");           // non-string reference cells have one meaningful payload word
    emitter.instruction("ldr x2, [x10, #8]");                                   // load the live string length from the reference cell
    emitter.instruction("b __rt_vd_closure_static_value");                      // render the live reference string value
    emitter.label("__rt_vd_closure_static_ref_nonstring");
    emitter.instruction("mov x2, xzr");                                         // non-string reference captures have no high payload word
    emitter.label("__rt_vd_closure_static_value");
    emitter.instruction("bl __rt_var_dump_value");                              // render the live captured value with its declared runtime tag
    emitter.label("__rt_vd_closure_static_next");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the binding-table cursor
    emitter.instruction("add x9, x9, #1");                                      // advance to the next capture binding
    emitter.instruction("str x9, [sp, #24]");                                   // persist the advanced binding-table cursor
    emitter.instruction("b __rt_vd_closure_static_loop");                       // continue the live capture projection
    emitter.label("__rt_vd_closure_static_locals_begin");
    emitter.instruction("str xzr, [sp, #72]");                                  // initialize the symbol-backed static-local cursor
    emitter.label("__rt_vd_closure_static_locals_loop");
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the symbol-backed static-local cursor
    emitter.instruction("ldr x10, [sp, #64]");                                  // reload the symbol-backed static-local count
    emitter.instruction("cmp x9, x10");                                         // rendered every symbol-backed static local?
    emitter.instruction("b.ge __rt_vd_closure_static_finish");                  // close the static array after every static local was rendered
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload the symbol-backed static-local table pointer
    emitter.instruction("add x11, x11, x9, lsl #5");                            // select this 32-byte static-local metadata record
    emitter.instruction("str x11, [sp, #48]");                                  // preserve the static-local metadata through key rendering
    emitter.instruction("ldr x1, [x11]");                                       // load the static-local name pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // load the static-local name length
    emitter.instruction("bl __rt_var_dump_emit_string_key");                    // write the static-local array key line
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the static-local metadata after the key writer call
    emitter.instruction("ldr x0, [x11, #16]");                                  // load the static-local runtime type tag
    emitter.instruction("ldr x10, [x11, #24]");                                 // load the static-local storage symbol address
    emitter.instruction("ldr x1, [x10]");                                       // load the static-local low payload word
    emitter.instruction("ldr x2, [x10, #8]");                                   // load the static-local high payload word
    emitter.instruction("bl __rt_var_dump_value");                              // render the live persistent static-local value
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the symbol-backed static-local cursor
    emitter.instruction("add x9, x9, #1");                                      // advance to the next static-local metadata record
    emitter.instruction("str x9, [sp, #72]");                                   // persist the advanced static-local cursor
    emitter.instruction("b __rt_vd_closure_static_locals_loop");                // continue rendering symbol-backed static locals
    emitter.label("__rt_vd_closure_static_finish");
    emitter.instruction("bl __rt_vd_indent_pop");                               // restore the Closure object field indentation
    emitter.instruction("bl __rt_var_dump_close_container");                    // write the static array closing brace
    emitter.label("__rt_vd_closure_static_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the Closure static-capture frame
    emitter.instruction("ret");                                                 // return to the Closure object projection
}

/// Emits the x86_64 Closure `static` capture projection.
fn emit_var_dump_closure_static_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the Closure static-capture frame
    emitter.instruction("sub rsp, 96");                                         // allocate descriptor, metadata, cursor, and alignment slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the Closure descriptor identity
    emitter.instruction("call __rt_closure_debug_static_count");                // count non-`$this` live capture bindings
    emitter.instruction("test rax, rax");                                       // are there visible static captures?
    emitter.instruction("jz __rt_vd_closure_static_done");                      // omit an empty php-src static array
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the visible static-capture count
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the Closure descriptor identity
    emitter.instruction("mov r9, QWORD PTR [r9 + 48]");                         // load the invocation side record pointer
    emitter.instruction("mov r9, QWORD PTR [r9 + 56]");                         // load the true-Closure debug record pointer
    emitter.instruction("mov r10, QWORD PTR [r9 + 56]");                        // load the capture binding-table pointer
    emitter.instruction("mov r11, QWORD PTR [r9 + 64]");                        // load the capture binding count
    emitter.instruction("mov r8, QWORD PTR [r9 + 72]");                         // load the symbol-backed static-local table pointer
    emitter.instruction("mov r9, QWORD PTR [r9 + 80]");                         // load the symbol-backed static-local count
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the binding table pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // preserve the binding-table count
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // preserve the static-local table pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // preserve the static-local count
    abi::emit_symbol_address(emitter, "rdi", "_closure_debug_key_static");
    emitter.instruction("mov esi, 10");                                         // len("[\"static\"]") = 10
    emitter.instruction("call __rt_var_dump_emit_object_key");                  // write the static property key line
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the visible static-capture count
    emitter.instruction("call __rt_var_dump_open_counted_array");               // write the static array header
    emitter.instruction("call __rt_vd_indent_push");                            // capture rows are nested one level deeper
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the binding-table cursor
    emitter.label("__rt_vd_closure_static_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the binding-table cursor
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // consumed every capture binding?
    emitter.instruction("jge __rt_vd_closure_static_locals_begin");             // continue with symbol-backed static locals after captures
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the binding-table pointer
    emitter.instruction("mov r8, rcx");                                         // copy the cursor before scaling the binding offset
    emitter.instruction("shl r8, 5");                                           // scale the cursor by the 32-byte binding stride
    emitter.instruction("add r11, r8");                                         // select this capture binding record
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // preserve the current binding record through key rendering
    emitter.instruction("cmp QWORD PTR [r11 + 8], 4");                          // only `$this` has the reserved four-byte name
    emitter.instruction("jne __rt_vd_closure_static_render");                   // non-`$this` capture belongs to the static array
    emitter.instruction("mov r9, QWORD PTR [r11]");                             // load the capture name pointer
    emitter.instruction("cmp DWORD PTR [r9], 0x73696874");                      // is this the reserved `$this` binding?
    emitter.instruction("je __rt_vd_closure_static_next");                      // `$this` is rendered as its own Closure debug field
    emitter.label("__rt_vd_closure_static_render");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current capture binding record
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load the capture name pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // load the capture name length
    emitter.instruction("call __rt_var_dump_emit_string_key");                  // write the captured-variable key line
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current capture binding record
    emitter.instruction("mov rdi, QWORD PTR [r11 + 16]");                       // load the capture runtime type tag
    emitter.instruction("mov r10, QWORD PTR [r11 + 24]");                       // load the by-reference capture flag
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the binding-table cursor for capture storage lookup
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor identity
    emitter.instruction("add rcx, 4");                                          // skip the 64-byte descriptor header in 16-byte capture units
    emitter.instruction("shl rcx, 4");                                          // scale the capture index by its 16-byte slot stride
    emitter.instruction("add rax, rcx");                                        // resolve the current capture slot address
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // load the capture low payload word
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // load the capture high payload word
    emitter.instruction("test r10, r10");                                       // does this capture use a live reference cell?
    emitter.instruction("jz __rt_vd_closure_static_value");                     // by-value captures already occupy their descriptor slot directly
    emitter.instruction("mov rax, rsi");                                        // reference captures store a heap cell pointer in the low word
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // load the live low payload from the reference cell
    emitter.instruction("cmp rdi, 1");                                          // does the live reference hold a string pair?
    emitter.instruction("jne __rt_vd_closure_static_ref_nonstring");            // non-string reference cells have one meaningful payload word
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // load the live string length from the reference cell
    emitter.instruction("jmp __rt_vd_closure_static_value");                    // render the live reference string value
    emitter.label("__rt_vd_closure_static_ref_nonstring");
    emitter.instruction("xor edx, edx");                                        // non-string reference captures have no high payload word
    emitter.label("__rt_vd_closure_static_value");
    emitter.instruction("call __rt_var_dump_value");                            // render the live captured value with its declared runtime tag
    emitter.label("__rt_vd_closure_static_next");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next capture binding
    emitter.instruction("jmp __rt_vd_closure_static_loop");                     // continue the live capture projection
    emitter.label("__rt_vd_closure_static_locals_begin");
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // initialize the symbol-backed static-local cursor
    emitter.label("__rt_vd_closure_static_locals_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 80]");                       // reload the symbol-backed static-local cursor
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 72]");                       // rendered every symbol-backed static local?
    emitter.instruction("jge __rt_vd_closure_static_finish");                   // close the static array after every static local was rendered
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                       // reload the symbol-backed static-local table pointer
    emitter.instruction("mov r8, rcx");                                         // copy the static-local cursor before scaling its record offset
    emitter.instruction("shl r8, 5");                                           // scale the static-local cursor by the 32-byte metadata stride
    emitter.instruction("add r11, r8");                                         // select this static-local metadata record
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // preserve the static-local metadata through key rendering
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load the static-local name pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // load the static-local name length
    emitter.instruction("call __rt_var_dump_emit_string_key");                  // write the static-local array key line
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the static-local metadata after the key writer call
    emitter.instruction("mov rdi, QWORD PTR [r11 + 16]");                       // load the static-local runtime type tag
    emitter.instruction("mov r10, QWORD PTR [r11 + 24]");                       // load the static-local storage symbol address
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the static-local low payload word
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the static-local high payload word
    emitter.instruction("call __rt_var_dump_value");                            // render the live persistent static-local value
    emitter.instruction("add QWORD PTR [rbp - 80], 1");                         // advance to the next static-local metadata record
    emitter.instruction("jmp __rt_vd_closure_static_locals_loop");              // continue rendering symbol-backed static locals
    emitter.label("__rt_vd_closure_static_finish");
    emitter.instruction("call __rt_vd_indent_pop");                             // restore the Closure object field indentation
    emitter.instruction("call __rt_var_dump_close_container");                  // write the static array closing brace
    emitter.label("__rt_vd_closure_static_done");
    emitter.instruction("add rsp, 96");                                         // release the Closure static-capture frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the Closure object projection
}

/// Emits the count of non-`$this` capture bindings visible through Closure `static` debug data.
fn emit_closure_debug_static_count(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: closure_debug_static_count ---");
    emitter.label_global("__rt_closure_debug_static_count");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0, #48]");                           // load the invocation side record pointer
            emitter.instruction("cbz x9, __rt_closure_static_count_zero");      // fake descriptors have no true-Closure debug record
            emitter.instruction("ldr x9, [x9, #56]");                           // load the optional debug record pointer
            emitter.instruction("cbz x9, __rt_closure_static_count_zero");      // non-Closure descriptors expose no static capture metadata
            emitter.instruction("ldr x10, [x9, #56]");                          // load the capture binding-table pointer
            emitter.instruction("ldr x11, [x9, #64]");                          // load the capture binding count
            emitter.instruction("ldr x0, [x9, #80]");                           // initialize with symbol-backed static-local count
            emitter.instruction("mov x12, #0");                                 // initialize the binding-table cursor
            emitter.label("__rt_closure_static_count_loop");
            emitter.instruction("cmp x12, x11");                                // consumed every capture binding?
            emitter.instruction("b.ge __rt_closure_static_count_done");         // return the completed visible static-capture count
            emitter.instruction("add x13, x10, x12, lsl #5");                   // select this 32-byte capture binding record
            emitter.instruction("ldr x14, [x13, #8]");                          // load the capture name length
            emitter.instruction("cmp x14, #4");                                 // only `$this` has the reserved four-byte name
            emitter.instruction("b.ne __rt_closure_static_count_include");      // non-`$this` capture belongs to the static array
            emitter.instruction("ldr x14, [x13]");                              // load the capture name pointer
            emitter.instruction("ldr w14, [x14]");                              // load the first four bytes of the capture name
            emitter.instruction("movz w15, #0x6874");                           // low half of `this` in little-endian order
            emitter.instruction("movk w15, #0x7369, lsl #16");                  // high half of `this` in little-endian order
            emitter.instruction("cmp w14, w15");                                // is this the reserved `$this` binding?
            emitter.instruction("b.eq __rt_closure_static_count_next");         // `$this` is exposed separately, never inside `static`
            emitter.label("__rt_closure_static_count_include");
            emitter.instruction("add x0, x0, #1");                              // count one visible static capture
            emitter.label("__rt_closure_static_count_next");
            emitter.instruction("add x12, x12, #1");                            // advance to the next capture binding
            emitter.instruction("b __rt_closure_static_count_loop");            // continue counting visible static captures
            emitter.label("__rt_closure_static_count_done");
            emitter.instruction("ret");                                         // return the visible static-capture count
            emitter.label("__rt_closure_static_count_zero");
            emitter.instruction("mov x0, #0");                                  // no debug record means no visible static captures
            emitter.instruction("ret");                                         // return zero static captures
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, QWORD PTR [rdi + 48]");                // load the invocation side record pointer
            emitter.instruction("test r9, r9");                                 // is invocation metadata available?
            emitter.instruction("jz __rt_closure_static_count_zero");           // fake descriptors have no true-Closure debug record
            emitter.instruction("mov r9, QWORD PTR [r9 + 56]");                 // load the optional debug record pointer
            emitter.instruction("test r9, r9");                                 // is this a true Closure descriptor?
            emitter.instruction("jz __rt_closure_static_count_zero");           // non-Closure descriptors expose no static capture metadata
            emitter.instruction("mov r10, QWORD PTR [r9 + 56]");                // load the capture binding-table pointer
            emitter.instruction("mov r11, QWORD PTR [r9 + 64]");                // load the capture binding count
            emitter.instruction("mov rax, QWORD PTR [r9 + 80]");                // initialize with symbol-backed static-local count
            emitter.instruction("xor ecx, ecx");                                // initialize the binding-table cursor
            emitter.label("__rt_closure_static_count_loop");
            emitter.instruction("cmp rcx, r11");                                // consumed every capture binding?
            emitter.instruction("jge __rt_closure_static_count_done");          // return the completed visible static-capture count
            emitter.instruction("mov r8, rcx");                                 // copy the capture index before scaling its binding offset
            emitter.instruction("shl r8, 5");                                   // scale the capture index by the 32-byte binding stride
            emitter.instruction("add r8, r10");                                 // select this capture binding record
            emitter.instruction("cmp QWORD PTR [r8 + 8], 4");                   // only `$this` has the reserved four-byte name
            emitter.instruction("jne __rt_closure_static_count_include");       // non-`$this` capture belongs to the static array
            emitter.instruction("mov r9, QWORD PTR [r8]");                      // load the capture name pointer
            emitter.instruction("cmp DWORD PTR [r9], 0x73696874");              // is this the reserved `$this` binding?
            emitter.instruction("je __rt_closure_static_count_next");           // `$this` is exposed separately, never inside `static`
            emitter.label("__rt_closure_static_count_include");
            emitter.instruction("add rax, 1");                                  // count one visible static capture
            emitter.label("__rt_closure_static_count_next");
            emitter.instruction("add rcx, 1");                                  // advance to the next capture binding
            emitter.instruction("jmp __rt_closure_static_count_loop");          // continue counting visible static captures
            emitter.label("__rt_closure_static_count_done");
            emitter.instruction("ret");                                         // return the visible static-capture count
            emitter.label("__rt_closure_static_count_zero");
            emitter.instruction("xor eax, eax");                                // no debug record means no visible static captures
            emitter.instruction("ret");                                         // return zero static captures
        }
    }
}

/// Emits the Closure `parameter` debug array from immutable signature metadata.
fn emit_var_dump_closure_parameters(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_closure_parameters ---");
    emitter.label_global("__rt_var_dump_closure_parameters");
    match emitter.target.arch {
        Arch::AArch64 => emit_var_dump_closure_parameters_aarch64(emitter),
        Arch::X86_64 => emit_var_dump_closure_parameters_x86_64(emitter),
    }
}

/// Emits the AArch64 `parameter` array from one Closure descriptor's signature record.
fn emit_var_dump_closure_parameters_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #96");                                     // allocate descriptor, signature, table, counters, and frame slots
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the Closure parameter frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the Closure descriptor identity
    emitter.instruction("ldr x9, [x0, #32]");                                   // load the signature side record pointer
    emitter.instruction("cbz x9, __rt_vd_closure_params_done");                 // descriptors without signatures expose no parameter field
    emitter.instruction("ldr x10, [x9]");                                       // load the visible parameter count
    emitter.instruction("cbz x10, __rt_vd_closure_params_done");                // zero visible parameters omit the parameter field
    emitter.instruction("ldr x11, [x9, #96]");                                  // load the debug `$name` / `&$name` key table
    emitter.instruction("cbz x11, __rt_vd_closure_params_done");                // missing generated names cannot form a PHP parameter array
    emitter.instruction("str x9, [sp, #8]");                                    // preserve the signature record through output calls
    emitter.instruction("str x10, [sp, #16]");                                  // preserve the visible parameter count
    emitter.instruction("ldr x10, [x9, #8]");                                   // load the required parameter count
    emitter.instruction("str x10, [sp, #24]");                                  // preserve the required-count split point
    emitter.instruction("str x11, [sp, #32]");                                  // preserve the generated debug-key table pointer
    abi::emit_symbol_address(emitter, "x1", "_closure_debug_key_parameter");
    emitter.instruction("mov x2, #13");                                         // len("[\"parameter\"]") = 13
    emitter.instruction("bl __rt_var_dump_emit_object_key");                    // write the parameter property key line
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the visible parameter count
    emitter.instruction("bl __rt_var_dump_open_counted_array");                 // write the parameter array header
    emitter.instruction("bl __rt_vd_indent_push");                              // parameter entries are nested one level deeper
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize the parameter index
    emitter.label("__rt_vd_closure_params_loop");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the parameter index
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the visible parameter count
    emitter.instruction("cmp x9, x10");                                         // rendered every visible parameter?
    emitter.instruction("b.ge __rt_vd_closure_params_finish");                  // leave the parameter loop when complete
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the generated debug-key table pointer
    emitter.instruction("add x11, x11, x9, lsl #4");                            // select the 16-byte key record for this parameter
    emitter.instruction("ldr x1, [x11]");                                       // load the PHP parameter key pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // load the PHP parameter key length
    emitter.instruction("bl __rt_var_dump_emit_string_key");                    // write `[\"$name\"]=>` or `[\"&$name\"]=>`
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the parameter index after the key writer call
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the required-count split point
    emitter.instruction("cmp x9, x10");                                         // is this parameter required?
    emitter.instruction("b.ge __rt_vd_closure_param_optional");                 // optional and variadic parameters use the optional marker
    abi::emit_symbol_address(emitter, "x1", "_closure_debug_required");
    emitter.instruction("b __rt_vd_closure_param_value");                       // share the string value renderer
    emitter.label("__rt_vd_closure_param_optional");
    abi::emit_symbol_address(emitter, "x1", "_closure_debug_optional");
    emitter.label("__rt_vd_closure_param_value");
    emitter.instruction("mov x0, #1");                                          // tag 1 = string
    emitter.instruction("mov x2, #10");                                         // both `<required>` and `<optional>` are ten bytes
    emitter.instruction("bl __rt_var_dump_value");                              // render the required/optional marker string
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the parameter index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next visible parameter
    emitter.instruction("str x9, [sp, #40]");                                   // persist the advanced parameter index
    emitter.instruction("b __rt_vd_closure_params_loop");                       // continue the parameter projection
    emitter.label("__rt_vd_closure_params_finish");
    emitter.instruction("bl __rt_vd_indent_pop");                               // restore the Closure object field indentation
    emitter.instruction("bl __rt_var_dump_close_container");                    // write the parameter array closing brace
    emitter.label("__rt_vd_closure_params_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the Closure parameter frame
    emitter.instruction("ret");                                                 // return to the Closure object projection
}

/// Emits the x86_64 `parameter` array from one Closure descriptor's signature record.
fn emit_var_dump_closure_parameters_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the Closure parameter frame
    emitter.instruction("sub rsp, 64");                                         // allocate descriptor, signature, table, counters, and alignment slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the Closure descriptor identity
    emitter.instruction("mov r9, QWORD PTR [rdi + 32]");                        // load the signature side record pointer
    emitter.instruction("test r9, r9");                                         // is a signature record present?
    emitter.instruction("jz __rt_vd_closure_params_done");                      // descriptors without signatures expose no parameter field
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the visible parameter count
    emitter.instruction("test r10, r10");                                       // are there visible parameters?
    emitter.instruction("jz __rt_vd_closure_params_done");                      // zero visible parameters omit the parameter field
    emitter.instruction("mov r11, QWORD PTR [r9 + 96]");                        // load the debug `$name` / `&$name` key table
    emitter.instruction("test r11, r11");                                       // were generated parameter keys emitted?
    emitter.instruction("jz __rt_vd_closure_params_done");                      // missing generated names cannot form a PHP parameter array
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the visible parameter count
    emitter.instruction("mov r10, QWORD PTR [r9 + 8]");                         // load the required parameter count
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // preserve the required-count split point
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // preserve the generated debug-key table pointer
    abi::emit_symbol_address(emitter, "rdi", "_closure_debug_key_parameter");
    emitter.instruction("mov esi, 13");                                         // len("[\"parameter\"]") = 13
    emitter.instruction("call __rt_var_dump_emit_object_key");                  // write the parameter property key line
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the visible parameter count
    emitter.instruction("call __rt_var_dump_open_counted_array");               // write the parameter array header
    emitter.instruction("call __rt_vd_indent_push");                            // parameter entries are nested one level deeper
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the parameter index
    emitter.label("__rt_vd_closure_params_loop");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the parameter index
    emitter.instruction("cmp r9, QWORD PTR [rbp - 16]");                        // rendered every visible parameter?
    emitter.instruction("jge __rt_vd_closure_params_finish");                   // leave the parameter loop when complete
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the generated debug-key table pointer
    emitter.instruction("shl r9, 4");                                           // scale the parameter index by the 16-byte key-record stride
    emitter.instruction("add r11, r9");                                         // select this parameter's key record
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load the PHP parameter key pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // load the PHP parameter key length
    emitter.instruction("call __rt_var_dump_emit_string_key");                  // write `[\"$name\"]=>` or `[\"&$name\"]=>`
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the parameter index after the key writer call
    emitter.instruction("cmp r9, QWORD PTR [rbp - 24]");                        // is this parameter required?
    emitter.instruction("jge __rt_vd_closure_param_optional");                  // optional and variadic parameters use the optional marker
    abi::emit_symbol_address(emitter, "rsi", "_closure_debug_required");
    emitter.instruction("jmp __rt_vd_closure_param_value");                     // share the string value renderer
    emitter.label("__rt_vd_closure_param_optional");
    abi::emit_symbol_address(emitter, "rsi", "_closure_debug_optional");
    emitter.label("__rt_vd_closure_param_value");
    emitter.instruction("mov rdi, 1");                                          // tag 1 = string
    emitter.instruction("mov rdx, 10");                                         // both `<required>` and `<optional>` are ten bytes
    emitter.instruction("call __rt_var_dump_value");                            // render the required/optional marker string
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // advance to the next visible parameter
    emitter.instruction("jmp __rt_vd_closure_params_loop");                     // continue the parameter projection
    emitter.label("__rt_vd_closure_params_finish");
    emitter.instruction("call __rt_vd_indent_pop");                             // restore the Closure object field indentation
    emitter.instruction("call __rt_var_dump_close_container");                  // write the parameter array closing brace
    emitter.label("__rt_vd_closure_params_done");
    emitter.instruction("add rsp, 64");                                         // release the Closure parameter frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the Closure object projection
}

/// Emits a var_dump array header for a caller-provided item count.
fn emit_var_dump_open_counted_array(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_open_counted_array ---");
    emitter.label_global("__rt_var_dump_open_counted_array");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // allocate the counted-array header frame
            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address
            emitter.instruction("mov x29, sp");                                 // establish the counted-array header frame
            emitter.instruction("str x0, [sp, #0]");                            // save the caller-provided item count
            emitter.instruction("bl __rt_vd_pad");                              // indent the array value line
            abi::emit_symbol_address(emitter, "x1", "_vd_array_prefix");
            emitter.instruction("mov x2, #6");                                  // len("array(") = 6
            emitter.instruction("bl __rt_vd_write");                            // write the array prefix
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the caller-provided item count
            emitter.instruction("bl __rt_itoa");                                // format the item count as decimal text
            emitter.instruction("bl __rt_vd_write");                            // write the item count digits
            abi::emit_symbol_address(emitter, "x1", "_vd_brace_open");
            emitter.instruction("mov x2, #4");                                  // len(") {\n") = 4
            emitter.instruction("bl __rt_vd_write");                            // complete the counted-array header line
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #32");                             // release the counted-array header frame
            emitter.instruction("ret");                                         // return to the Closure projection
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // save caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the counted-array header frame
            emitter.instruction("sub rsp, 16");                                 // allocate the counted-array header frame
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // save the caller-provided item count
            emitter.instruction("call __rt_vd_pad");                            // indent the array value line
            abi::emit_symbol_address(emitter, "rsi", "_vd_array_prefix");
            emitter.instruction("mov edx, 6");                                  // len("array(") = 6
            emitter.instruction("call __rt_vd_write");                          // write the array prefix
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // reload the caller-provided item count
            emitter.instruction("call __rt_itoa");                              // format the item count as decimal text
            emitter.instruction("mov rsi, rax");                                // decimal count pointer → writer buffer
            emitter.instruction("call __rt_vd_write");                          // write the item count digits
            abi::emit_symbol_address(emitter, "rsi", "_vd_brace_open");
            emitter.instruction("mov edx, 4");                                  // len(") {\n") = 4
            emitter.instruction("call __rt_vd_write");                          // complete the counted-array header line
            emitter.instruction("add rsp, 16");                                 // release the counted-array header frame
            emitter.instruction("pop rbp");                                     // restore caller frame pointer
            emitter.instruction("ret");                                         // return to the Closure projection
        }
    }
}

/// Emits the AArch64 Closure debug projection for name, file, and line metadata.
fn emit_var_dump_closure_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #48");                                     // allocate descriptor, debug-record, count, and frame slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the Closure debug frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the Closure descriptor identity
    emitter.instruction("ldr x10, [x0, #32]");                                  // load the optional signature side record
    emitter.instruction("str x10, [sp, #24]");                                  // preserve the signature record through output calls
    emitter.instruction("ldr x9, [x0, #48]");                                   // load the invocation side record pointer
    emitter.instruction("cbz x9, __rt_vd_closure_no_debug");                    // fake descriptors have no true-Closure debug record
    emitter.instruction("ldr x9, [x9, #56]");                                   // load the optional debug record pointer
    emitter.instruction("cbz x9, __rt_vd_closure_no_debug");                    // non-Closure descriptors expose no Closure fields
    emitter.instruction("str x9, [sp, #8]");                                    // retain the debug record through nested value renderers
    emitter.instruction("ldr x10, [x9, #8]");                                   // load the debug-record kind flags
    emitter.instruction("tst x10, #2");                                         // is this a first-class fake Closure?
    emitter.instruction("b.ne __rt_vd_closure_fake_debug");                     // fake Closures expose function/static/this/parameter fields
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Closure descriptor for static-capture counting
    emitter.instruction("bl __rt_closure_debug_static_count");                  // count non-`$this` live capture bindings
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the static-capture count through header rendering
    emitter.instruction("mov x9, #3");                                          // name, file, and line are always present for user Closures
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the visible static-capture count
    emitter.instruction("cbz x10, __rt_vd_closure_no_static_field");            // omit an empty php-src static array
    emitter.instruction("add x9, x9, #1");                                      // account for the static debug array
    emitter.label("__rt_vd_closure_no_static_field");
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the field count across the caller-clobbering receiver probe
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Closure descriptor for bound-receiver detection
    emitter.instruction("bl __rt_closure_debug_this_present");                  // test whether this Closure carries a live `$this` object
    emitter.instruction("ldr x9, [sp, #16]");                                   // restore the field count after the receiver probe clobbers temporary registers
    emitter.instruction("cbz x0, __rt_vd_closure_no_this_field");               // omit `$this` for unbound top-level Closures
    emitter.instruction("add x9, x9, #1");                                      // account for the bound `$this` debug field
    emitter.label("__rt_vd_closure_no_this_field");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the Closure descriptor for signature lookup
    emitter.instruction("ldr x10, [x10, #32]");                                 // load the optional signature side record
    emitter.instruction("cbz x10, __rt_vd_closure_header");                     // no signature means no parameter field
    emitter.instruction("ldr x10, [x10]");                                      // load the visible parameter count
    emitter.instruction("cbz x10, __rt_vd_closure_header");                     // zero visible parameters omit the parameter field
    emitter.instruction("add x9, x9, #1");                                      // account for the parameter debug array
    emitter.instruction("b __rt_vd_closure_header");                            // render the Closure object header with the field count
    emitter.label("__rt_vd_closure_fake_debug");
    emitter.instruction("mov x9, #1");                                          // fake Closures always expose their function field
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the fake Closure descriptor for static-field counting
    emitter.instruction("bl __rt_closure_debug_static_count");                  // count shared method statics
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the fake-Closure static count
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the visible fake-Closure static count
    emitter.instruction("cbz x10, __rt_vd_closure_fake_no_static_field");       // omit an empty php-src static array
    emitter.instruction("add x9, x9, #1");                                      // account for the fake-Closure static debug array
    emitter.label("__rt_vd_closure_fake_no_static_field");
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the fake field count across the receiver probe
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the fake Closure descriptor for bound-receiver detection
    emitter.instruction("bl __rt_closure_debug_this_present");                  // detect the instance-method receiver exposed by php-src as `$this`
    emitter.instruction("ldr x9, [sp, #16]");                                   // restore the fake field count after the receiver probe
    emitter.instruction("cbz x0, __rt_vd_closure_fake_no_this_field");          // static/free first-class callables have no `$this` field
    emitter.instruction("add x9, x9, #1");                                      // account for the bound fake-Closure `$this` field
    emitter.label("__rt_vd_closure_fake_no_this_field");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the fake Closure descriptor for signature lookup
    emitter.instruction("ldr x10, [x10, #32]");                                 // load the optional signature side record
    emitter.instruction("cbz x10, __rt_vd_closure_header");                     // no signature means no parameter field
    emitter.instruction("ldr x10, [x10]");                                      // load the visible parameter count
    emitter.instruction("cbz x10, __rt_vd_closure_header");                     // zero visible parameters omit the parameter field
    emitter.instruction("add x9, x9, #1");                                      // account for fake-Closure parameter metadata
    emitter.instruction("b __rt_vd_closure_header");                            // render the fake Closure object header with the field count
    emitter.label("__rt_vd_closure_no_debug");
    emitter.instruction("str xzr, [sp, #8]");                                   // mark the missing debug record for the fake-Closure projection
    emitter.instruction("mov x9, #1");                                          // php-src fake Closures always expose their function field
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the Closure descriptor for signature lookup
    emitter.instruction("ldr x10, [x10, #32]");                                 // load the optional signature side record
    emitter.instruction("cbz x10, __rt_vd_closure_header");                     // no signature means the function field is the whole projection
    emitter.instruction("ldr x10, [x10]");                                      // load the visible parameter count
    emitter.instruction("cbz x10, __rt_vd_closure_header");                     // zero visible parameters omit the parameter field
    emitter.instruction("add x9, x9, #1");                                      // account for php-src fake-Closure parameter metadata
    emitter.label("__rt_vd_closure_header");
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the visible field count across output calls
    emitter.instruction("bl __rt_vd_pad");                                      // indent the Closure object header
    abi::emit_symbol_address(emitter, "x1", "_vd_object_prefix");
    emitter.instruction("mov x2, #7");                                          // len("object(") = 7
    emitter.instruction("bl __rt_vd_write");                                    // write the object prefix
    abi::emit_symbol_address(emitter, "x1", "_sprintf_closure_class_name");
    emitter.instruction("mov x2, #7");                                          // len("Closure") = 7
    emitter.instruction("bl __rt_vd_write");                                    // write the Closure class name
    abi::emit_symbol_address(emitter, "x1", "_vd_object_mid");
    emitter.instruction("mov x2, #2");                                          // len(")#") = 2
    emitter.instruction("bl __rt_vd_write");                                    // write the object handle separator
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the descriptor used as the Closure object identity
    emitter.instruction("bl __rt_object_handle_of");                            // resolve the Closure's PHP object handle
    emitter.instruction("bl __rt_itoa");                                        // format the object handle as decimal text
    emitter.instruction("bl __rt_vd_write");                                    // write the object handle digits
    abi::emit_symbol_address(emitter, "x1", "_vd_object_count_open");
    emitter.instruction("mov x2, #2");                                          // len(" (") = 2
    emitter.instruction("bl __rt_vd_write");                                    // write the debug-field count opener
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the Closure debug-field count
    emitter.instruction("bl __rt_itoa");                                        // format the field count as decimal text
    emitter.instruction("bl __rt_vd_write");                                    // write the field count digits
    abi::emit_symbol_address(emitter, "x1", "_vd_brace_open");
    emitter.instruction("mov x2, #4");                                          // len(") {\n") = 4
    emitter.instruction("bl __rt_vd_write");                                    // complete the Closure header line
    emitter.instruction("bl __rt_vd_indent_push");                              // Closure debug fields are nested one indent level
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the true-Closure debug record pointer
    emitter.instruction("cbz x9, __rt_vd_closure_fake_fields");                 // fake descriptors render php-src function and parameter fields
    emitter.instruction("ldr x10, [x9, #8]");                                   // load the debug-record kind flags
    emitter.instruction("tst x10, #2");                                         // is this a first-class fake Closure?
    emitter.instruction("b.ne __rt_vd_closure_fake_fields");                    // fake Closures do not expose name/file/line
    abi::emit_symbol_address(emitter, "x1", "_closure_debug_key_name");
    emitter.instruction("mov x2, #8");                                          // len("[\"name\"]") = 8
    emitter.instruction("bl __rt_var_dump_emit_object_key");                    // write the name property key line
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the debug record after the key writer call
    emitter.instruction("mov x0, #1");                                          // tag 1 = string
    emitter.instruction("ldr x1, [x9, #16]");                                   // load the Closure display-name pointer
    emitter.instruction("ldr x2, [x9, #24]");                                   // load the Closure display-name length
    emitter.instruction("bl __rt_var_dump_value");                              // render the Closure display-name string
    abi::emit_symbol_address(emitter, "x1", "_closure_debug_key_file");
    emitter.instruction("mov x2, #8");                                          // len("[\"file\"]") = 8
    emitter.instruction("bl __rt_var_dump_emit_object_key");                    // write the file property key line
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the debug record after the key writer call
    emitter.instruction("mov x0, #1");                                          // tag 1 = string
    emitter.instruction("ldr x1, [x9, #32]");                                   // load the Closure source-file pointer
    emitter.instruction("ldr x2, [x9, #40]");                                   // load the Closure source-file length
    emitter.instruction("bl __rt_var_dump_value");                              // render the Closure source-file string
    abi::emit_symbol_address(emitter, "x1", "_closure_debug_key_line");
    emitter.instruction("mov x2, #8");                                          // len("[\"line\"]") = 8
    emitter.instruction("bl __rt_var_dump_emit_object_key");                    // write the line property key line
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the debug record after the key writer call
    emitter.instruction("mov x0, #0");                                          // tag 0 = integer
    emitter.instruction("ldr x1, [x9, #48]");                                   // load the Closure source line number
    emitter.instruction("mov x2, xzr");                                         // integers have no high payload word
    emitter.instruction("bl __rt_var_dump_value");                              // render the Closure source-line integer
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Closure descriptor for the static-capture projection
    emitter.instruction("bl __rt_var_dump_closure_static");                     // render the optional php-src static debug array
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Closure descriptor for the bound-receiver projection
    emitter.instruction("bl __rt_var_dump_closure_this");                       // render `$this` only when the Closure currently binds an object
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Closure descriptor for the parameter projection
    emitter.instruction("bl __rt_var_dump_closure_parameters");                 // render the optional php-src parameter debug array
    emitter.instruction("b __rt_vd_closure_finish");                            // true Closure projection is complete
    emitter.label("__rt_vd_closure_fake_fields");
    abi::emit_symbol_address(emitter, "x1", "_closure_debug_key_function");
    emitter.instruction("mov x2, #12");                                         // len("[\"function\"]") = 12
    emitter.instruction("bl __rt_var_dump_emit_object_key");                    // write the php-src fake-Closure function key line
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the fake Closure descriptor identity
    emitter.instruction("mov x0, #1");                                          // tag 1 = string
    emitter.instruction("ldr x1, [x9, #16]");                                   // load the PHP callable display-name pointer
    emitter.instruction("ldr x2, [x9, #24]");                                   // load the PHP callable display-name length
    emitter.instruction("bl __rt_var_dump_value");                              // render the php-src fake-Closure function name
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the fake Closure descriptor for the static projection
    emitter.instruction("bl __rt_var_dump_closure_static");                     // render shared method static locals when present
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the fake Closure descriptor for the receiver projection
    emitter.instruction("bl __rt_var_dump_closure_this");                       // render php-src `$this` for instance-method first-class callables
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the fake Closure descriptor for the parameter projection
    emitter.instruction("bl __rt_var_dump_closure_parameters");                 // render optional fake-Closure parameter metadata
    emitter.label("__rt_vd_closure_finish");
    emitter.instruction("bl __rt_vd_indent_pop");                               // restore the caller's value indentation
    emitter.instruction("bl __rt_var_dump_close_container");                    // write the Closure object closing brace
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the Closure debug frame
    emitter.instruction("ret");                                                 // return to the tag-10 value dispatcher
}

/// Emits the x86_64 Closure debug projection for name, file, and line metadata.
fn emit_var_dump_closure_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the Closure debug frame
    emitter.instruction("sub rsp, 48");                                         // allocate descriptor, debug-record, count, and alignment slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the Closure descriptor identity
    emitter.instruction("mov r10, QWORD PTR [rdi + 32]");                       // load the optional signature side record
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the signature record through output calls
    emitter.instruction("mov r9, QWORD PTR [rdi + 48]");                        // load the invocation side record pointer
    emitter.instruction("test r9, r9");                                         // is invocation metadata available?
    emitter.instruction("jz __rt_vd_closure_no_debug");                         // fake descriptors have no true-Closure debug record
    emitter.instruction("mov r9, QWORD PTR [r9 + 56]");                         // load the optional debug record pointer
    emitter.instruction("test r9, r9");                                         // is this a true Closure descriptor?
    emitter.instruction("jz __rt_vd_closure_no_debug");                         // non-Closure descriptors expose no Closure fields
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // retain the debug record through nested value renderers
    emitter.instruction("test QWORD PTR [r9 + 8], 2");                          // is this a first-class fake Closure?
    emitter.instruction("jnz __rt_vd_closure_fake_debug");                      // fake Closures expose function/static/this/parameter fields
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor for static-capture counting
    emitter.instruction("call __rt_closure_debug_static_count");                // count non-`$this` live capture bindings
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the static-capture count through header rendering
    emitter.instruction("mov r9, 3");                                           // name, file, and line are always present for user Closures
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // does the Closure expose any static captures?
    emitter.instruction("je __rt_vd_closure_no_static_field");                  // omit an empty php-src static array
    emitter.instruction("add r9, 1");                                           // account for the static debug array
    emitter.label("__rt_vd_closure_no_static_field");
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // preserve the field count across the caller-clobbering receiver probe
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor for bound-receiver detection
    emitter.instruction("call __rt_closure_debug_this_present");                // test whether this Closure carries a live `$this` object
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // restore the field count after the receiver probe clobbers temporary registers
    emitter.instruction("test rax, rax");                                       // is a receiver actually bound to the Closure?
    emitter.instruction("jz __rt_vd_closure_no_this_field");                    // omit `$this` for unbound top-level Closures
    emitter.instruction("add r9, 1");                                           // account for the bound `$this` debug field
    emitter.label("__rt_vd_closure_no_this_field");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor for signature lookup
    emitter.instruction("mov r10, QWORD PTR [r10 + 32]");                       // load the optional signature side record
    emitter.instruction("test r10, r10");                                       // is a signature record present?
    emitter.instruction("jz __rt_vd_closure_header");                           // no signature means no parameter field
    emitter.instruction("cmp QWORD PTR [r10], 0");                              // does the signature expose visible parameters?
    emitter.instruction("je __rt_vd_closure_header");                           // zero visible parameters omit the parameter field
    emitter.instruction("add r9, 1");                                           // account for the parameter debug array
    emitter.instruction("jmp __rt_vd_closure_header");                          // render the Closure object header with the field count
    emitter.label("__rt_vd_closure_fake_debug");
    emitter.instruction("mov r9, 1");                                           // fake Closures always expose their function field
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the fake Closure descriptor for static-field counting
    emitter.instruction("call __rt_closure_debug_static_count");                // count shared method statics
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the fake-Closure static count
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // does the fake Closure expose shared statics?
    emitter.instruction("je __rt_vd_closure_fake_no_static_field");             // omit an empty php-src static array
    emitter.instruction("add r9, 1");                                           // account for the fake-Closure static debug array
    emitter.label("__rt_vd_closure_fake_no_static_field");
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // preserve the fake field count across the receiver probe
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the fake Closure descriptor for bound-receiver detection
    emitter.instruction("call __rt_closure_debug_this_present");                // detect the instance-method receiver exposed by php-src as `$this`
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // restore the fake field count after the receiver probe
    emitter.instruction("test rax, rax");                                       // is a receiver actually bound to the fake Closure?
    emitter.instruction("jz __rt_vd_closure_fake_no_this_field");               // static/free first-class callables have no `$this` field
    emitter.instruction("add r9, 1");                                           // account for the bound fake-Closure `$this` field
    emitter.label("__rt_vd_closure_fake_no_this_field");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the fake Closure descriptor for signature lookup
    emitter.instruction("mov r10, QWORD PTR [r10 + 32]");                       // load the optional signature side record
    emitter.instruction("test r10, r10");                                       // is a signature record present?
    emitter.instruction("jz __rt_vd_closure_header");                           // no signature means no parameter field
    emitter.instruction("cmp QWORD PTR [r10], 0");                              // does the signature expose visible parameters?
    emitter.instruction("je __rt_vd_closure_header");                           // zero visible parameters omit the parameter field
    emitter.instruction("add r9, 1");                                           // account for fake-Closure parameter metadata
    emitter.instruction("jmp __rt_vd_closure_header");                          // render the fake Closure object header with the field count
    emitter.label("__rt_vd_closure_no_debug");
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // mark the missing debug record for the fake-Closure projection
    emitter.instruction("mov r9, 1");                                           // php-src fake Closures always expose their function field
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor for signature lookup
    emitter.instruction("mov r10, QWORD PTR [r10 + 32]");                       // load the optional signature side record
    emitter.instruction("test r10, r10");                                       // is a signature record present?
    emitter.instruction("jz __rt_vd_closure_header");                           // no signature means the function field is the whole projection
    emitter.instruction("cmp QWORD PTR [r10], 0");                              // does the signature expose visible parameters?
    emitter.instruction("je __rt_vd_closure_header");                           // zero visible parameters omit the parameter field
    emitter.instruction("add r9, 1");                                           // account for php-src fake-Closure parameter metadata
    emitter.label("__rt_vd_closure_header");
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // preserve the visible field count across output calls
    emitter.instruction("call __rt_vd_pad");                                    // indent the Closure object header
    abi::emit_symbol_address(emitter, "rsi", "_vd_object_prefix");
    emitter.instruction("mov edx, 7");                                          // len("object(") = 7
    emitter.instruction("call __rt_vd_write");                                  // write the object prefix
    abi::emit_symbol_address(emitter, "rsi", "_sprintf_closure_class_name");
    emitter.instruction("mov edx, 7");                                          // len("Closure") = 7
    emitter.instruction("call __rt_vd_write");                                  // write the Closure class name
    abi::emit_symbol_address(emitter, "rsi", "_vd_object_mid");
    emitter.instruction("mov edx, 2");                                          // len(")#") = 2
    emitter.instruction("call __rt_vd_write");                                  // write the object handle separator
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the descriptor used as the Closure object identity
    emitter.instruction("call __rt_object_handle_of");                          // resolve the Closure's PHP object handle
    emitter.instruction("call __rt_itoa");                                      // format the object handle as decimal text
    emitter.instruction("mov rsi, rax");                                        // decimal handle pointer → writer buffer
    emitter.instruction("call __rt_vd_write");                                  // write the object handle digits
    abi::emit_symbol_address(emitter, "rsi", "_vd_object_count_open");
    emitter.instruction("mov edx, 2");                                          // len(" (") = 2
    emitter.instruction("call __rt_vd_write");                                  // write the debug-field count opener
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the Closure debug-field count
    emitter.instruction("call __rt_itoa");                                      // format the field count as decimal text
    emitter.instruction("mov rsi, rax");                                        // decimal count pointer → writer buffer
    emitter.instruction("call __rt_vd_write");                                  // write the field count digits
    abi::emit_symbol_address(emitter, "rsi", "_vd_brace_open");
    emitter.instruction("mov edx, 4");                                          // len(") {\n") = 4
    emitter.instruction("call __rt_vd_write");                                  // complete the Closure header line
    emitter.instruction("call __rt_vd_indent_push");                            // Closure debug fields are nested one indent level
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the true-Closure debug record pointer
    emitter.instruction("test r9, r9");                                         // are true-Closure fields available?
    emitter.instruction("jz __rt_vd_closure_fake_fields");                      // fake descriptors render php-src function and parameter fields
    emitter.instruction("test QWORD PTR [r9 + 8], 2");                          // inspect the fake-Closure kind flag before projecting true-Closure fields
    emitter.instruction("jnz __rt_vd_closure_fake_fields");                     // first-class callables expose function/static/this/parameter, never name/file/line
    abi::emit_symbol_address(emitter, "rdi", "_closure_debug_key_name");
    emitter.instruction("mov esi, 8");                                          // len("[\"name\"]") = 8
    emitter.instruction("call __rt_var_dump_emit_object_key");                  // write the name property key line
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the debug record after the key writer call
    emitter.instruction("mov rdi, 1");                                          // tag 1 = string
    emitter.instruction("mov rsi, QWORD PTR [r9 + 16]");                        // load the Closure display-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r9 + 24]");                        // load the Closure display-name length
    emitter.instruction("call __rt_var_dump_value");                            // render the Closure display-name string
    abi::emit_symbol_address(emitter, "rdi", "_closure_debug_key_file");
    emitter.instruction("mov esi, 8");                                          // len("[\"file\"]") = 8
    emitter.instruction("call __rt_var_dump_emit_object_key");                  // write the file property key line
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the debug record after the key writer call
    emitter.instruction("mov rdi, 1");                                          // tag 1 = string
    emitter.instruction("mov rsi, QWORD PTR [r9 + 32]");                        // load the Closure source-file pointer
    emitter.instruction("mov rdx, QWORD PTR [r9 + 40]");                        // load the Closure source-file length
    emitter.instruction("call __rt_var_dump_value");                            // render the Closure source-file string
    abi::emit_symbol_address(emitter, "rdi", "_closure_debug_key_line");
    emitter.instruction("mov esi, 8");                                          // len("[\"line\"]") = 8
    emitter.instruction("call __rt_var_dump_emit_object_key");                  // write the line property key line
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the debug record after the key writer call
    emitter.instruction("mov rdi, 0");                                          // tag 0 = integer
    emitter.instruction("mov rsi, QWORD PTR [r9 + 48]");                        // load the Closure source line number
    emitter.instruction("xor edx, edx");                                        // integers have no high payload word
    emitter.instruction("call __rt_var_dump_value");                            // render the Closure source-line integer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor for the static-capture projection
    emitter.instruction("call __rt_var_dump_closure_static");                   // render the optional php-src static debug array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor for the bound-receiver projection
    emitter.instruction("call __rt_var_dump_closure_this");                     // render `$this` only when the Closure currently binds an object
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor for the parameter projection
    emitter.instruction("call __rt_var_dump_closure_parameters");               // render the optional php-src parameter debug array
    emitter.instruction("jmp __rt_vd_closure_finish");                          // true Closure projection is complete
    emitter.label("__rt_vd_closure_fake_fields");
    abi::emit_symbol_address(emitter, "rdi", "_closure_debug_key_function");
    emitter.instruction("mov esi, 12");                                         // len("[\"function\"]") = 12
    emitter.instruction("call __rt_var_dump_emit_object_key");                  // write the php-src fake-Closure function key line
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the fake Closure descriptor identity
    emitter.instruction("mov rdi, 1");                                          // tag 1 = string
    emitter.instruction("mov rsi, QWORD PTR [r9 + 16]");                        // load the PHP callable display-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r9 + 24]");                        // load the PHP callable display-name length
    emitter.instruction("call __rt_var_dump_value");                            // render the php-src fake-Closure function name
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the fake Closure descriptor for the static projection
    emitter.instruction("call __rt_var_dump_closure_static");                   // render shared method static locals when present
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the fake Closure descriptor for the receiver projection
    emitter.instruction("call __rt_var_dump_closure_this");                     // render php-src `$this` for instance-method first-class callables
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the fake Closure descriptor for the parameter projection
    emitter.instruction("call __rt_var_dump_closure_parameters");               // render optional fake-Closure parameter metadata
    emitter.label("__rt_vd_closure_finish");
    emitter.instruction("call __rt_vd_indent_pop");                             // restore the caller's value indentation
    emitter.instruction("call __rt_var_dump_close_container");                  // write the Closure object closing brace
    emitter.instruction("add rsp, 48");                                         // release the Closure debug frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the tag-10 value dispatcher
}

/// Emits the `print_r` Closure projection with the true-Closure metadata fields.
fn emit_print_r_closure(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r_closure ---");
    emitter.label_global("__rt_print_r_closure");
    match emitter.target.arch {
        Arch::AArch64 => emit_print_r_closure_aarch64(emitter),
        Arch::X86_64 => emit_print_r_closure_x86_64(emitter),
    }
}

/// Emits the AArch64 Closure `print_r` projection for name, file, and line metadata.
fn emit_print_r_closure_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #48");                                     // allocate descriptor, base-indent, debug-record, and frame slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the Closure print_r frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // preserve descriptor identity and caller base indent
    emitter.instruction("ldr x9, [x0, #48]");                                   // load the invocation side record pointer
    emitter.instruction("cbz x9, __rt_pr_closure_no_debug");                    // fake descriptors have no true-Closure debug record
    emitter.instruction("ldr x9, [x9, #56]");                                   // load the optional debug record pointer
    emitter.instruction("cbz x9, __rt_pr_closure_no_debug");                    // non-Closure descriptors expose no true-Closure fields
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the debug record through output calls
    emitter.instruction("b __rt_pr_closure_header");                            // render the true-Closure header and fields
    emitter.label("__rt_pr_closure_no_debug");
    emitter.instruction("str xzr, [sp, #16]");                                  // mark the missing true-Closure debug record
    emitter.label("__rt_pr_closure_header");
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the caller's base indent
    emitter.instruction("bl __rt_print_r_spaces");                              // indent the Closure class header
    abi::emit_symbol_address(emitter, "x1", "_sprintf_closure_class_name");
    emitter.instruction("mov x2, #7");                                          // len("Closure") = 7
    emitter.instruction("bl __rt_pr_write");                                    // write the Closure class name
    abi::emit_symbol_address(emitter, "x1", "_pr_object_suffix");
    emitter.instruction("mov x2, #8");                                          // len(" Object\n") = 8
    emitter.instruction("bl __rt_pr_write");                                    // write the Closure object header suffix
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the caller's base indent
    emitter.instruction("bl __rt_print_r_open");                                // write the opening parenthesis line
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the true-Closure debug record pointer
    emitter.instruction("cbz x9, __rt_pr_closure_fake_fields");                 // fake descriptors render php-src function and parameter fields
    emitter.instruction("ldr x10, [x9, #8]");                                   // load the debug-record kind flags
    emitter.instruction("tst x10, #2");                                         // is this a first-class fake Closure?
    emitter.instruction("b.ne __rt_pr_closure_fake_fields");                    // fake Closures do not expose name/file/line
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the caller base indent for the name line
    emitter.instruction("add x0, x0, #4");                                      // Closure property rows are indented four spaces deeper
    emitter.instruction("bl __rt_print_r_spaces");                              // indent the name property row
    abi::emit_symbol_address(emitter, "x1", "_closure_print_r_key_name");
    emitter.instruction("mov x2, #10");                                         // len("[name] => ") = 10
    emitter.instruction("bl __rt_pr_write");                                    // write the name property prefix
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the debug record after the writer call
    emitter.instruction("ldr x1, [x9, #16]");                                   // load the Closure display-name pointer
    emitter.instruction("ldr x2, [x9, #24]");                                   // load the Closure display-name length
    emitter.instruction("bl __rt_pr_write");                                    // write the Closure display name
    abi::emit_symbol_address(emitter, "x1", "_pr_nl");
    emitter.instruction("mov x2, #1");                                          // len("\n") = 1
    emitter.instruction("bl __rt_pr_write");                                    // terminate the name property row
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the caller base indent for the file line
    emitter.instruction("add x0, x0, #4");                                      // Closure property rows are indented four spaces deeper
    emitter.instruction("bl __rt_print_r_spaces");                              // indent the file property row
    abi::emit_symbol_address(emitter, "x1", "_closure_print_r_key_file");
    emitter.instruction("mov x2, #10");                                         // len("[file] => ") = 10
    emitter.instruction("bl __rt_pr_write");                                    // write the file property prefix
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the debug record after the writer call
    emitter.instruction("ldr x1, [x9, #32]");                                   // load the Closure source-file pointer
    emitter.instruction("ldr x2, [x9, #40]");                                   // load the Closure source-file length
    emitter.instruction("bl __rt_pr_write");                                    // write the Closure source file
    abi::emit_symbol_address(emitter, "x1", "_pr_nl");
    emitter.instruction("mov x2, #1");                                          // len("\n") = 1
    emitter.instruction("bl __rt_pr_write");                                    // terminate the file property row
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the caller base indent for the line row
    emitter.instruction("add x0, x0, #4");                                      // Closure property rows are indented four spaces deeper
    emitter.instruction("bl __rt_print_r_spaces");                              // indent the line property row
    abi::emit_symbol_address(emitter, "x1", "_closure_print_r_key_line");
    emitter.instruction("mov x2, #10");                                         // len("[line] => ") = 10
    emitter.instruction("bl __rt_pr_write");                                    // write the line property prefix
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the debug record after the writer call
    emitter.instruction("ldr x0, [x9, #48]");                                   // load the Closure source line number
    emitter.instruction("bl __rt_itoa");                                        // format the line number as decimal text
    emitter.instruction("bl __rt_pr_write");                                    // write the Closure source line number
    abi::emit_symbol_address(emitter, "x1", "_pr_nl");
    emitter.instruction("mov x2, #1");                                          // len("\n") = 1
    emitter.instruction("bl __rt_pr_write");                                    // terminate the line property row
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Closure descriptor for the static projection
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the Closure base indent for the static projection
    emitter.instruction("bl __rt_print_r_closure_static");                      // render php-src static captures and static locals
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Closure descriptor for the bound-receiver projection
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the Closure base indent for the bound-receiver projection
    emitter.instruction("bl __rt_print_r_closure_this");                        // render php-src `$this` metadata when an object is bound
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Closure descriptor for the parameter projection
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the Closure base indent for the parameter projection
    emitter.instruction("bl __rt_print_r_closure_parameters");                  // render php-src parameter metadata
    emitter.instruction("b __rt_pr_closure_finish");                            // true Closure projection is complete
    emitter.label("__rt_pr_closure_fake_fields");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the Closure base indent for the function property row
    emitter.instruction("add x0, x0, #4");                                      // property rows are four spaces beneath the Closure block
    emitter.instruction("bl __rt_print_r_spaces");                              // indent the fake-Closure function property row
    abi::emit_symbol_address(emitter, "x1", "_closure_print_r_key_function");
    emitter.instruction("mov x2, #14");                                         // len("[function] => ") = 14
    emitter.instruction("bl __rt_pr_write");                                    // write the fake-Closure function property prefix
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the fake Closure descriptor identity
    emitter.instruction("ldr x1, [x9, #16]");                                   // load the PHP callable display-name pointer
    emitter.instruction("ldr x2, [x9, #24]");                                   // load the PHP callable display-name length
    emitter.instruction("bl __rt_pr_write");                                    // write the php-src fake-Closure function name
    abi::emit_symbol_address(emitter, "x1", "_pr_nl");
    emitter.instruction("mov x2, #1");                                          // len("\\n") = 1
    emitter.instruction("bl __rt_pr_write");                                    // terminate the fake-Closure function property row
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the fake Closure descriptor for the static projection
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the Closure base indent for the static projection
    emitter.instruction("bl __rt_print_r_closure_static");                      // render shared method static locals when present
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the fake Closure descriptor for the receiver projection
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the Closure base indent for the receiver projection
    emitter.instruction("bl __rt_print_r_closure_this");                        // render php-src `$this` for instance-method first-class callables
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the fake Closure descriptor for the parameter projection
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the Closure base indent for the parameter projection
    emitter.instruction("bl __rt_print_r_closure_parameters");                  // render optional fake-Closure parameter metadata
    emitter.label("__rt_pr_closure_finish");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the caller's base indent
    emitter.instruction("bl __rt_print_r_close");                               // write the closing parenthesis line
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the Closure print_r frame
    emitter.instruction("ret");                                                 // return to the tag-10 value dispatcher
}

/// Emits the x86_64 Closure `print_r` projection for name, file, and line metadata.
fn emit_print_r_closure_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the Closure print_r frame
    emitter.instruction("sub rsp, 48");                                         // allocate descriptor, base-indent, debug-record, and alignment slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the Closure descriptor identity
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the caller base indent
    emitter.instruction("mov r9, QWORD PTR [rdi + 48]");                        // load the invocation side record pointer
    emitter.instruction("test r9, r9");                                         // is invocation metadata available?
    emitter.instruction("jz __rt_pr_closure_no_debug");                         // fake descriptors have no true-Closure debug record
    emitter.instruction("mov r9, QWORD PTR [r9 + 56]");                         // load the optional debug record pointer
    emitter.instruction("test r9, r9");                                         // is this a true Closure descriptor?
    emitter.instruction("jz __rt_pr_closure_no_debug");                         // non-Closure descriptors expose no true-Closure fields
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // preserve the debug record through output calls
    emitter.instruction("jmp __rt_pr_closure_header");                          // render the true-Closure header and fields
    emitter.label("__rt_pr_closure_no_debug");
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // mark the missing true-Closure debug record
    emitter.label("__rt_pr_closure_header");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // load the caller's base indent
    emitter.instruction("call __rt_print_r_spaces");                            // indent the Closure class header
    abi::emit_symbol_address(emitter, "rsi", "_sprintf_closure_class_name");
    emitter.instruction("mov edx, 7");                                          // len("Closure") = 7
    emitter.instruction("call __rt_pr_write");                                  // write the Closure class name
    abi::emit_symbol_address(emitter, "rsi", "_pr_object_suffix");
    emitter.instruction("mov edx, 8");                                          // len(" Object\n") = 8
    emitter.instruction("call __rt_pr_write");                                  // write the Closure object header suffix
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the caller's base indent
    emitter.instruction("call __rt_print_r_open");                              // write the opening parenthesis line
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the true-Closure debug record pointer
    emitter.instruction("test r9, r9");                                         // are true-Closure fields available?
    emitter.instruction("jz __rt_pr_closure_fake_fields");                      // fake descriptors render php-src function and parameter fields
    emitter.instruction("test QWORD PTR [r9 + 8], 2");                          // is this a first-class fake Closure?
    emitter.instruction("jnz __rt_pr_closure_fake_fields");                     // fake Closures do not expose name/file/line
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // load the caller base indent for the name line
    emitter.instruction("add rdi, 4");                                          // Closure property rows are indented four spaces deeper
    emitter.instruction("call __rt_print_r_spaces");                            // indent the name property row
    abi::emit_symbol_address(emitter, "rsi", "_closure_print_r_key_name");
    emitter.instruction("mov edx, 10");                                         // len("[name] => ") = 10
    emitter.instruction("call __rt_pr_write");                                  // write the name property prefix
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the debug record after the writer call
    emitter.instruction("mov rsi, QWORD PTR [r9 + 16]");                        // load the Closure display-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r9 + 24]");                        // load the Closure display-name length
    emitter.instruction("call __rt_pr_write");                                  // write the Closure display name
    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");
    emitter.instruction("mov edx, 1");                                          // len("\n") = 1
    emitter.instruction("call __rt_pr_write");                                  // terminate the name property row
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // load the caller base indent for the file line
    emitter.instruction("add rdi, 4");                                          // Closure property rows are indented four spaces deeper
    emitter.instruction("call __rt_print_r_spaces");                            // indent the file property row
    abi::emit_symbol_address(emitter, "rsi", "_closure_print_r_key_file");
    emitter.instruction("mov edx, 10");                                         // len("[file] => ") = 10
    emitter.instruction("call __rt_pr_write");                                  // write the file property prefix
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the debug record after the writer call
    emitter.instruction("mov rsi, QWORD PTR [r9 + 32]");                        // load the Closure source-file pointer
    emitter.instruction("mov rdx, QWORD PTR [r9 + 40]");                        // load the Closure source-file length
    emitter.instruction("call __rt_pr_write");                                  // write the Closure source file
    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");
    emitter.instruction("mov edx, 1");                                          // len("\n") = 1
    emitter.instruction("call __rt_pr_write");                                  // terminate the file property row
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // load the caller base indent for the line row
    emitter.instruction("add rdi, 4");                                          // Closure property rows are indented four spaces deeper
    emitter.instruction("call __rt_print_r_spaces");                            // indent the line property row
    abi::emit_symbol_address(emitter, "rsi", "_closure_print_r_key_line");
    emitter.instruction("mov edx, 10");                                         // len("[line] => ") = 10
    emitter.instruction("call __rt_pr_write");                                  // write the line property prefix
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the debug record after the writer call
    emitter.instruction("mov rax, QWORD PTR [r9 + 48]");                        // load the Closure source line number
    emitter.instruction("call __rt_itoa");                                      // format the line number as decimal text
    emitter.instruction("mov rsi, rax");                                        // decimal line pointer → writer buffer
    emitter.instruction("call __rt_pr_write");                                  // write the Closure source line number
    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");
    emitter.instruction("mov edx, 1");                                          // len("\n") = 1
    emitter.instruction("call __rt_pr_write");                                  // terminate the line property row
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor for the static projection
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the static projection
    emitter.instruction("call __rt_print_r_closure_static");                    // render php-src static captures and static locals
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor for the bound-receiver projection
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the bound-receiver projection
    emitter.instruction("call __rt_print_r_closure_this");                      // render php-src `$this` metadata when an object is bound
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the Closure descriptor for the parameter projection
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the parameter projection
    emitter.instruction("call __rt_print_r_closure_parameters");                // render php-src parameter metadata
    emitter.instruction("jmp __rt_pr_closure_finish");                          // true Closure projection is complete
    emitter.label("__rt_pr_closure_fake_fields");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the function property row
    emitter.instruction("add rdi, 4");                                          // property rows are four spaces beneath the Closure block
    emitter.instruction("call __rt_print_r_spaces");                            // indent the fake-Closure function property row
    abi::emit_symbol_address(emitter, "rsi", "_closure_print_r_key_function");
    emitter.instruction("mov edx, 14");                                         // len("[function] => ") = 14
    emitter.instruction("call __rt_pr_write");                                  // write the fake-Closure function property prefix
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the fake Closure descriptor identity
    emitter.instruction("mov rsi, QWORD PTR [r9 + 16]");                        // load the PHP callable display-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r9 + 24]");                        // load the PHP callable display-name length
    emitter.instruction("call __rt_pr_write");                                  // write the php-src fake-Closure function name
    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");
    emitter.instruction("mov edx, 1");                                          // len("\\n") = 1
    emitter.instruction("call __rt_pr_write");                                  // terminate the fake-Closure function property row
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the fake Closure descriptor for the static projection
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the static projection
    emitter.instruction("call __rt_print_r_closure_static");                    // render shared method static locals when present
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the fake Closure descriptor for the receiver projection
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the receiver projection
    emitter.instruction("call __rt_print_r_closure_this");                      // render php-src `$this` for instance-method first-class callables
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the fake Closure descriptor for the parameter projection
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the Closure base indent for the parameter projection
    emitter.instruction("call __rt_print_r_closure_parameters");                // render optional fake-Closure parameter metadata
    emitter.label("__rt_pr_closure_finish");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the caller's base indent
    emitter.instruction("call __rt_print_r_close");                             // write the closing parenthesis line
    emitter.instruction("add rsp, 48");                                         // release the Closure print_r frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the tag-10 value dispatcher
}
