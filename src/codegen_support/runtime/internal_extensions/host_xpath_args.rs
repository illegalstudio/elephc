//! Purpose:
//! Builds PHP argument containers for re-entrant custom XPath callbacks.
//! Converts validated flat null, boolean, number, and byte values into boxed Mixed arguments.
//!
//! Called from:
//! - `super::emit_dom_runtime()` as private helpers for `__rt_dom_host_call`.
//!
//! Key details:
//! - Source argument order is preserved after libxml's reverse operand stack is normalized.
//! - The returned boxed array and raw array keep independent balanced owners during invocation.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific XPath callback argument builder.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_build_args_x86_64(emitter);
        emit_build_nodes_x86_64(emitter);
        return;
    }
    emit_build_args_aarch64(emitter);
    emit_build_nodes_aarch64(emitter);
}

/// Emits the AArch64 XPath callback argument builder.
fn emit_build_args_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host XPath build args ---");
    emitter.label_global("__rt_dom_host_xpath_build_args");
    emitter.instruction("sub sp, sp, #80");                                     // reserve request, count, array, index, and value spills
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame across runtime allocation
    emitter.instruction("add x29, sp, #64");                                    // establish the XPath argument-builder frame
    emitter.instruction("str x0, [sp]");                                        // preserve the validated flat callback request
    emitter.instruction("str x1, [sp, #40]");                                   // preserve the owning DOM context for node wrappers
    emitter.instruction("ldr w9, [x0, #12]");                                   // load the explicit descriptor-plus-root count
    emitter.instruction("and w9, w9, #0x7fffffff");                             // discard the validated root-count marker bit
    emitter.instruction("sub x9, x9, #1");                                      // exclude the callable descriptor from PHP arguments
    emitter.instruction("str x9, [sp, #8]");                                    // preserve the visible callback argument count
    emitter.instruction("ldr x10, [x0, #24]");                                  // load the complete root-plus-descendant value count
    emitter.instruction("mov x14, #24");                                        // each flat ABI value occupies twenty-four bytes
    emitter.instruction("mov x15, #48");                                        // include the padded request header
    emitter.instruction("madd x10, x10, x14, x15");                             // byte values begin after the complete flat value section
    emitter.instruction("str x10, [sp, #16]");                                  // preserve the dynamic byte-section offset
    emitter.instruction("mov x0, x9");                                          // allocate one raw slot per visible callback argument
    emitter.instruction("mov x1, #8");                                          // each indexed slot stores one boxed Mixed pointer
    abi::emit_call_label(emitter, "__rt_array_new");                             // create the raw callback argument array
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the packed indexed-array kind word
    emitter.instruction("mov x12, #0x80ff");                                    // preserve indexed kind and persistent COW metadata
    emitter.instruction("and x10, x10, x12");                                   // discard any stale element-type metadata
    emitter.instruction("mov x11, #7");                                         // runtime array type seven stores boxed Mixed pointers
    emitter.instruction("lsl x11, x11, #8");                                    // move the value type into the kind-word lane
    emitter.instruction("orr x10, x10, x11");                                   // combine indexed storage with Mixed elements
    emitter.instruction("str x10, [x0, #-8]");                                  // publish the exact raw argument array shape
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the raw callback argument array
    emitter.instruction("str xzr, [sp, #32]");                                  // initialize the source-order argument index
    emitter.label("__rt_dom_host_xpath_build_args_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load the next visible argument index
    emitter.instruction("ldr x10, [sp, #8]");                                   // load the complete visible argument count
    emitter.instruction("cmp x9, x10");                                         // have all arguments been boxed?
    emitter.instruction("b.hs __rt_dom_host_xpath_build_args_done");             // finish after the source-order prefix is complete
    emitter.instruction("mov x10, #24");                                        // each request value record occupies twenty-four bytes
    emitter.instruction("mov x14, #72");                                        // skip the request header and descriptor record
    emitter.instruction("madd x10, x9, x10, x14");                              // locate argument index after the descriptor record
    emitter.instruction("ldr x11, [sp]");                                       // reload the flat callback request
    emitter.instruction("add x12, x11, x10");                                   // address the selected validated argument record
    emitter.instruction("ldr w13, [x12]");                                      // load its public ABI value tag
    emitter.instruction("cmp w13, #1");                                         // is this argument a PHP boolean?
    emitter.instruction("b.eq __rt_dom_host_xpath_build_args_bool");             // map the canonical boolean payload
    emitter.instruction("cmp w13, #3");                                         // is this argument an XPath double?
    emitter.instruction("b.eq __rt_dom_host_xpath_build_args_float");            // preserve the exact IEEE-754 payload
    emitter.instruction("cmp w13, #4");                                         // is this argument an XPath string?
    emitter.instruction("b.eq __rt_dom_host_xpath_build_args_bytes");            // persist its bounded request byte range
    emitter.instruction("cmp w13, #5");                                         // is this argument an XPath node-set array?
    emitter.instruction("b.eq __rt_dom_host_xpath_build_args_nodes");           // materialize canonical DOM wrapper members
    emitter.instruction("mov x0, #8");                                          // runtime tag eight boxes PHP null
    emitter.instruction("mov x1, xzr");                                         // null has no low payload
    emitter.instruction("mov x2, xzr");                                         // null has no high payload
    emitter.instruction("b __rt_dom_host_xpath_build_args_box");                 // box the canonical null argument
    emitter.label("__rt_dom_host_xpath_build_args_bool");
    emitter.instruction("mov x0, #3");                                          // runtime tag three boxes a PHP boolean
    emitter.instruction("ldr x1, [x12, #8]");                                   // load the validated zero-or-one boolean payload
    emitter.instruction("mov x2, xzr");                                         // booleans have no high payload
    emitter.instruction("b __rt_dom_host_xpath_build_args_box");                 // box the boolean argument
    emitter.label("__rt_dom_host_xpath_build_args_float");
    emitter.instruction("mov x0, #2");                                          // runtime tag two boxes a PHP float
    emitter.instruction("ldr x1, [x12, #8]");                                   // preserve the exact double bit pattern
    emitter.instruction("mov x2, xzr");                                         // floats use only the low payload word
    emitter.instruction("b __rt_dom_host_xpath_build_args_box");                 // box the floating-point argument
    emitter.label("__rt_dom_host_xpath_build_args_bytes");
    emitter.instruction("ldr x13, [x12, #8]");                                  // load this string's byte-section offset
    emitter.instruction("ldr x2, [x12, #16]");                                  // load this string's exact byte length
    emitter.instruction("ldr x14, [sp, #16]");                                  // reload the dynamic byte-section start
    emitter.instruction("add x1, x11, x14");                                    // address the request byte section
    emitter.instruction("add x1, x1, x13");                                     // address this exact XPath string
    emitter.instruction("mov x0, #1");                                          // runtime tag one boxes a PHP string
    emitter.instruction("b __rt_dom_host_xpath_build_args_box");                // box the persisted XPath string
    emitter.label("__rt_dom_host_xpath_build_args_nodes");
    emitter.instruction("ldr x0, [sp]");                                        // node builder arg0 is the validated flat request
    emitter.instruction("mov x1, x12");                                         // node builder arg1 is this array value record
    emitter.instruction("ldr x2, [sp, #40]");                                   // node builder arg2 is the owning DOM context
    abi::emit_call_label(emitter, "__rt_dom_host_xpath_build_nodes");            // return one owned boxed PHP node array
    emitter.instruction("cbz x0, __rt_dom_host_xpath_build_args_fail");         // reject unavailable wrapper classes without publishing null elements
    emitter.instruction("b __rt_dom_host_xpath_build_args_store");              // transfer the already-boxed array into the argument slot
    emitter.label("__rt_dom_host_xpath_build_args_box");
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // create one owned boxed callback argument
    emitter.label("__rt_dom_host_xpath_build_args_store");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the source-order argument index
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the raw callback argument array
    emitter.instruction("add x11, x10, #24");                                   // address indexed-array element storage
    emitter.instruction("str x0, [x11, x9, lsl #3]");                           // transfer the Mixed owner into its argument slot
    emitter.instruction("add x9, x9, #1");                                      // advance past the initialized argument
    emitter.instruction("str x9, [x10]");                                       // publish the initialized array length for cleanup safety
    emitter.instruction("str x9, [sp, #32]");                                   // preserve the next source-order index
    emitter.instruction("b __rt_dom_host_xpath_build_args_loop");                // box the remaining callback arguments
    emitter.label("__rt_dom_host_xpath_build_args_done");
    emitter.instruction("mov x0, #4");                                          // runtime tag four boxes an indexed array
    emitter.instruction("ldr x1, [sp, #24]");                                   // payload is the raw callback argument array
    emitter.instruction("mov x2, xzr");                                         // indexed arrays have no high payload word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // create the descriptor invoker's argument container
    emitter.instruction("ldr x1, [sp, #24]");                                   // return the independent raw array owner beside the box
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame after construction
    emitter.instruction("add sp, sp, #80");                                     // release the XPath argument-builder frame
    emitter.instruction("ret");                                                 // return boxed args in x0 and raw args in x1
    emitter.label("__rt_dom_host_xpath_build_args_fail");
    emitter.instruction("ldr x0, [sp, #24]");                                   // release the partially initialized raw argument array
    abi::emit_call_label(emitter, "__rt_decref_any");                            // release every already-boxed callback argument transitively
    emitter.instruction("mov x0, xzr");                                         // return a missing boxed argument container
    emitter.instruction("mov x1, xzr");                                         // return a missing independent raw-array owner
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame after failed construction
    emitter.instruction("add sp, sp, #80");                                     // release the XPath argument-builder frame
    emitter.instruction("ret");                                                 // report contained argument materialization failure
}

/// Emits the Linux x86_64 XPath callback argument builder.
fn emit_build_args_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host XPath build args ---");
    emitter.label_global("__rt_dom_host_xpath_build_args");
    emitter.instruction("push rbp");                                            // preserve the caller frame across runtime allocation
    emitter.instruction("mov rbp, rsp");                                        // establish the XPath argument-builder frame
    emitter.instruction("sub rsp, 48");                                         // reserve request, count, array, index, and byte-offset spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the validated flat callback request
    emitter.instruction("mov QWORD PTR [rbp - 48], rsi");                       // preserve the owning DOM context for node wrappers
    emitter.instruction("mov r9d, DWORD PTR [rdi + 12]");                       // load the explicit descriptor-plus-root count
    emitter.instruction("and r9d, 0x7fffffff");                                 // discard the validated root-count marker bit
    emitter.instruction("sub r9, 1");                                           // exclude the callable descriptor from PHP arguments
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // preserve the visible callback argument count
    emitter.instruction("mov r10, QWORD PTR [rdi + 24]");                       // load the complete root-plus-descendant value count
    emitter.instruction("imul r10, r10, 24");                                   // compute the complete flat value-section bytes
    emitter.instruction("add r10, 48");                                         // include the padded request header
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // preserve the dynamic byte-section offset
    emitter.instruction("mov rdi, r9");                                         // allocate one raw slot per visible callback argument
    emitter.instruction("mov esi, 8");                                          // each indexed slot stores one boxed Mixed pointer
    abi::emit_call_label(emitter, "__rt_array_new");                             // create the raw callback argument array
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve heap marker, indexed kind, and COW metadata
    emitter.instruction("and r10, r11");                                        // discard any stale element-type metadata
    emitter.instruction("mov r11, 7");                                          // runtime array type seven stores boxed Mixed pointers
    emitter.instruction("shl r11, 8");                                          // move the value type into the kind-word lane
    emitter.instruction("or r10, r11");                                         // combine indexed storage with Mixed elements
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // publish the exact raw argument array shape
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the raw callback argument array
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the source-order argument index
    emitter.label("__rt_dom_host_xpath_build_args_loop");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // load the next visible argument index
    emitter.instruction("cmp r9, QWORD PTR [rbp - 16]");                        // have all arguments been boxed?
    emitter.instruction("jae __rt_dom_host_xpath_build_args_done");             // finish after the source-order prefix is complete
    emitter.instruction("imul r10, r9, 24");                                    // compute this argument's value-record displacement
    emitter.instruction("add r10, 72");                                         // skip the request header and descriptor record
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the flat callback request
    emitter.instruction("add r10, r11");                                        // address the selected validated argument record
    emitter.instruction("mov eax, DWORD PTR [r10]");                            // load its public ABI value tag
    emitter.instruction("cmp eax, 1");                                          // is this argument a PHP boolean?
    emitter.instruction("je __rt_dom_host_xpath_build_args_bool");              // map the canonical boolean payload
    emitter.instruction("cmp eax, 3");                                          // is this argument an XPath double?
    emitter.instruction("je __rt_dom_host_xpath_build_args_float");             // preserve the exact IEEE-754 payload
    emitter.instruction("cmp eax, 4");                                          // is this argument an XPath string?
    emitter.instruction("je __rt_dom_host_xpath_build_args_bytes");             // persist its bounded request byte range
    emitter.instruction("cmp eax, 5");                                          // is this argument an XPath node-set array?
    emitter.instruction("je __rt_dom_host_xpath_build_args_nodes");             // materialize canonical DOM wrapper members
    emitter.instruction("mov eax, 8");                                          // runtime tag eight boxes PHP null
    emitter.instruction("xor edi, edi");                                        // null has no low payload
    emitter.instruction("xor esi, esi");                                        // null has no high payload
    emitter.instruction("jmp __rt_dom_host_xpath_build_args_box");              // box the canonical null argument
    emitter.label("__rt_dom_host_xpath_build_args_bool");
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // load the validated zero-or-one boolean payload
    emitter.instruction("xor esi, esi");                                        // booleans have no high payload
    emitter.instruction("mov eax, 3");                                          // runtime tag three boxes a PHP boolean
    emitter.instruction("jmp __rt_dom_host_xpath_build_args_box");              // box the boolean argument
    emitter.label("__rt_dom_host_xpath_build_args_float");
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // preserve the exact double bit pattern
    emitter.instruction("xor esi, esi");                                        // floats use only the low payload word
    emitter.instruction("mov eax, 2");                                          // runtime tag two boxes a PHP float
    emitter.instruction("jmp __rt_dom_host_xpath_build_args_box");              // box the floating-point argument
    emitter.label("__rt_dom_host_xpath_build_args_bytes");
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // load this string's byte-section offset
    emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");                       // load this string's exact byte length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the flat callback request
    emitter.instruction("add rdi, QWORD PTR [rbp - 24]");                       // address the request byte section
    emitter.instruction("add rdi, rax");                                        // address this exact XPath string
    emitter.instruction("mov eax, 1");                                          // runtime tag one boxes a PHP string
    emitter.instruction("jmp __rt_dom_host_xpath_build_args_box");              // box the persisted XPath string
    emitter.label("__rt_dom_host_xpath_build_args_nodes");
    emitter.instruction("mov rsi, r10");                                        // node builder arg1 is this array value record
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // node builder arg0 is the validated flat request
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // node builder arg2 is the owning DOM context
    abi::emit_call_label(emitter, "__rt_dom_host_xpath_build_nodes");            // return one owned boxed PHP node array
    emitter.instruction("test rax, rax");                                       // did every native node resolve to a PHP wrapper class?
    emitter.instruction("jz __rt_dom_host_xpath_build_args_fail_x86");          // reject unavailable wrappers without publishing null elements
    emitter.instruction("jmp __rt_dom_host_xpath_build_args_store");            // transfer the already-boxed array into its argument slot
    emitter.label("__rt_dom_host_xpath_build_args_box");
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // create one owned boxed callback argument
    emitter.label("__rt_dom_host_xpath_build_args_store");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the source-order argument index
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the raw callback argument array
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8 + 24], rax");              // transfer the Mixed owner into its argument slot
    emitter.instruction("add r9, 1");                                           // advance past the initialized argument
    emitter.instruction("mov QWORD PTR [r10], r9");                             // publish the initialized array length for cleanup safety
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // preserve the next source-order index
    emitter.instruction("jmp __rt_dom_host_xpath_build_args_loop");             // box the remaining callback arguments
    emitter.label("__rt_dom_host_xpath_build_args_done");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // payload is the raw callback argument array
    emitter.instruction("xor esi, esi");                                        // indexed arrays have no high payload word
    emitter.instruction("mov eax, 4");                                          // runtime tag four boxes an indexed array
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // create the descriptor invoker's argument container
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // return the independent raw array owner beside the box
    emitter.instruction("add rsp, 48");                                         // release the XPath argument-builder spills
    emitter.instruction("pop rbp");                                             // restore the caller frame after construction
    emitter.instruction("ret");                                                 // return boxed args in rax and raw args in rdi
    emitter.label("__rt_dom_host_xpath_build_args_fail_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // release the partially initialized raw argument array
    abi::emit_call_label(emitter, "__rt_decref_any");                            // release every already-boxed callback argument transitively
    emitter.instruction("xor eax, eax");                                        // return a missing boxed argument container
    emitter.instruction("xor edi, edi");                                        // return a missing independent raw-array owner
    emitter.instruction("add rsp, 48");                                         // release the XPath argument-builder spills
    emitter.instruction("pop rbp");                                             // restore the caller frame after failed construction
    emitter.instruction("ret");                                                 // report contained argument materialization failure
}

/// Emits the AArch64 XPath node-set-to-PHP-array materializer.
fn emit_build_nodes_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host XPath build node array ---");
    emitter.label_global("__rt_dom_host_xpath_build_nodes");
    emitter.instruction("sub sp, sp, #96");                                     // reserve request, range, array, index, context, object, and box spills
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame across wrapper and ownership helpers
    emitter.instruction("add x29, sp, #80");                                    // establish the XPath node-array builder frame
    emitter.instruction("str x0, [sp]");                                        // preserve the validated flat callback request
    emitter.instruction("ldr x9, [x1, #8]");                                    // load the first descendant value index
    emitter.instruction("str x9, [sp, #8]");                                    // preserve the descendant range start
    emitter.instruction("ldr x9, [x1, #16]");                                   // load the exact node count
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the node count across allocation
    emitter.instruction("str x2, [sp, #40]");                                   // preserve the owning DOM context
    emitter.instruction("mov x0, x9");                                          // allocate one raw slot per node-set member
    emitter.instruction("mov x1, #8");                                          // each indexed slot stores one boxed Mixed pointer
    abi::emit_call_label(emitter, "__rt_array_new");                             // create the raw PHP callback node array
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the packed indexed-array kind word
    emitter.instruction("mov x12, #0x80ff");                                    // preserve indexed kind and persistent COW metadata
    emitter.instruction("and x10, x10, x12");                                   // discard stale element-type metadata
    emitter.instruction("mov x11, #7");                                         // runtime array type seven stores boxed Mixed pointers
    emitter.instruction("lsl x11, x11, #8");                                    // move the value type into the kind-word lane
    emitter.instruction("orr x10, x10, x11");                                   // combine indexed storage with Mixed elements
    emitter.instruction("str x10, [x0, #-8]");                                  // publish the exact raw node-array shape
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the raw PHP node array
    emitter.instruction("str xzr, [sp, #32]");                                  // initialize the node-set member index
    emitter.label("__rt_dom_host_xpath_build_nodes_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load the next node-set member index
    emitter.instruction("ldr x10, [sp, #16]");                                  // load the exact node count
    emitter.instruction("cmp x9, x10");                                         // have all native nodes been wrapped?
    emitter.instruction("b.hs __rt_dom_host_xpath_build_nodes_done");           // finish after the complete node set is materialized
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the first descendant value index
    emitter.instruction("add x10, x10, x9");                                    // derive this member's flat value index
    emitter.instruction("mov x11, #24");                                        // each descendant occupies one public value record
    emitter.instruction("mov x12, #48");                                        // the value section follows the padded request header
    emitter.instruction("madd x10, x10, x11, x12");                             // compute this bridge-handle record's displacement
    emitter.instruction("ldr x11, [sp]");                                       // reload the validated flat callback request
    emitter.instruction("add x11, x11, x10");                                   // address this canonical bridge-handle record
    emitter.instruction("ldr x0, [sp, #40]");                                   // wrapper arg0 is the owning DOM context
    emitter.instruction("ldr x1, [x11, #8]");                                   // wrapper arg1 is the generation-checked native handle
    emitter.instruction("ldr x2, [x11, #16]");                                  // wrapper arg2 is the stable concrete wrapper kind
    abi::emit_call_label(emitter, "__rt_dom_host_xpath_wrapper_from_kind");      // materialize one owned canonical PHP DOM wrapper
    emitter.instruction("cbz x0, __rt_dom_host_xpath_build_nodes_fail");        // fail atomically when the wrapper class is unavailable
    emitter.instruction("str x0, [sp, #48]");                                   // preserve the owned object while creating its Mixed cell
    emitter.instruction("mov x1, x0");                                          // Mixed payload is the canonical wrapper object
    emitter.instruction("mov x0, #6");                                          // runtime tag six boxes a PHP object
    emitter.instruction("mov x2, xzr");                                         // objects use no high payload word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // retain the wrapper for one owned Mixed element
    emitter.instruction("str x0, [sp, #56]");                                   // preserve the boxed element while dropping the extra object owner
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the independently owned wrapper reference
    abi::emit_call_label(emitter, "__rt_decref_any");                            // leave the Mixed element as sole wrapper owner
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the owned boxed node element
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the node-set member index
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the raw PHP node array
    emitter.instruction("add x11, x10, #24");                                   // address indexed-array element storage
    emitter.instruction("str x0, [x11, x9, lsl #3]");                           // transfer the Mixed element owner into the array
    emitter.instruction("add x9, x9, #1");                                      // advance past the initialized node element
    emitter.instruction("str x9, [x10]");                                       // publish length for ownership-safe partial cleanup
    emitter.instruction("str x9, [sp, #32]");                                   // preserve the next node-set member index
    emitter.instruction("b __rt_dom_host_xpath_build_nodes_loop");              // materialize the remaining node wrappers
    emitter.label("__rt_dom_host_xpath_build_nodes_done");
    emitter.instruction("mov x0, #4");                                          // runtime tag four boxes an indexed array
    emitter.instruction("ldr x1, [sp, #24]");                                   // payload is the raw PHP node array
    emitter.instruction("mov x2, xzr");                                         // indexed arrays have no high payload word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // retain the raw node array for its returned Mixed owner
    emitter.instruction("str x0, [sp, #56]");                                   // preserve the returned box while dropping the extra raw owner
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the independently owned raw node array
    abi::emit_call_label(emitter, "__rt_decref_any");                            // leave the returned Mixed box as sole array owner
    emitter.instruction("ldr x0, [sp, #56]");                                   // restore the owned boxed PHP node array
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame after successful materialization
    emitter.instruction("add sp, sp, #96");                                     // release the node-array builder frame
    emitter.instruction("ret");                                                 // return one owned boxed PHP node array
    emitter.label("__rt_dom_host_xpath_build_nodes_fail");
    emitter.instruction("ldr x0, [sp, #24]");                                   // release the partially initialized PHP node array
    abi::emit_call_label(emitter, "__rt_decref_any");                            // release every already-boxed wrapper transitively
    emitter.instruction("mov x0, xzr");                                         // return a contained materialization failure sentinel
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame after failed materialization
    emitter.instruction("add sp, sp, #96");                                     // release the node-array builder frame
    emitter.instruction("ret");                                                 // let the host callback reject the malformed exchange
}

/// Emits the Linux x86_64 XPath node-set-to-PHP-array materializer.
fn emit_build_nodes_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host XPath build node array ---");
    emitter.label_global("__rt_dom_host_xpath_build_nodes");
    emitter.instruction("push rbp");                                            // preserve the caller frame across wrapper and ownership helpers
    emitter.instruction("mov rbp, rsp");                                        // establish the XPath node-array builder frame
    emitter.instruction("sub rsp, 64");                                         // reserve request, range, array, index, context, object, and box spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the validated flat callback request
    emitter.instruction("mov rax, QWORD PTR [rsi + 8]");                        // load the first descendant value index
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the descendant range start
    emitter.instruction("mov rax, QWORD PTR [rsi + 16]");                       // load the exact node count
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the node count across allocation
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // preserve the owning DOM context
    emitter.instruction("mov rdi, rax");                                        // allocate one raw slot per node-set member
    emitter.instruction("mov esi, 8");                                          // each indexed slot stores one boxed Mixed pointer
    abi::emit_call_label(emitter, "__rt_array_new");                             // create the raw PHP callback node array
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve heap marker, indexed kind, and COW metadata
    emitter.instruction("and r10, r11");                                        // discard stale element-type metadata
    emitter.instruction("mov r11, 7");                                          // runtime array type seven stores boxed Mixed pointers
    emitter.instruction("shl r11, 8");                                          // move the value type into the kind-word lane
    emitter.instruction("or r10, r11");                                         // combine indexed storage with Mixed elements
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // publish the exact raw node-array shape
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the raw PHP node array
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the node-set member index
    emitter.label("__rt_dom_host_xpath_build_nodes_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // load the next node-set member index
    emitter.instruction("cmp r9, QWORD PTR [rbp - 24]");                        // have all native nodes been wrapped?
    emitter.instruction("jae __rt_dom_host_xpath_build_nodes_done_x86");        // finish after the complete node set is materialized
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the first descendant value index
    emitter.instruction("add r10, r9");                                         // derive this member's flat value index
    emitter.instruction("imul r10, r10, 24");                                   // compute this bridge-handle record's value displacement
    emitter.instruction("add r10, 48");                                         // include the padded request header
    emitter.instruction("add r10, QWORD PTR [rbp - 8]");                        // address this canonical bridge-handle record
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // wrapper arg0 is the owning DOM context
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // wrapper arg1 is the generation-checked native handle
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // wrapper arg2 is the stable concrete wrapper kind
    abi::emit_call_label(emitter, "__rt_dom_host_xpath_wrapper_from_kind");      // materialize one owned canonical PHP DOM wrapper
    emitter.instruction("test rax, rax");                                       // did this native node resolve to a PHP wrapper class?
    emitter.instruction("jz __rt_dom_host_xpath_build_nodes_fail_x86");         // fail atomically when the wrapper class is unavailable
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the owned object while creating its Mixed cell
    emitter.instruction("mov rdi, rax");                                        // Mixed payload is the canonical wrapper object
    emitter.instruction("xor esi, esi");                                        // objects use no high payload word
    emitter.instruction("mov eax, 6");                                          // runtime tag six boxes a PHP object
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // retain the wrapper for one owned Mixed element
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // preserve the boxed element while dropping the extra object owner
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the independently owned wrapper reference
    abi::emit_call_label(emitter, "__rt_decref_any");                            // leave the Mixed element as sole wrapper owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the owned boxed node element
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the node-set member index
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the raw PHP node array
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8 + 24], rax");              // transfer the Mixed element owner into the array
    emitter.instruction("add r9, 1");                                           // advance past the initialized node element
    emitter.instruction("mov QWORD PTR [r10], r9");                             // publish length for ownership-safe partial cleanup
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // preserve the next node-set member index
    emitter.instruction("jmp __rt_dom_host_xpath_build_nodes_loop_x86");        // materialize the remaining node wrappers
    emitter.label("__rt_dom_host_xpath_build_nodes_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // payload is the raw PHP node array
    emitter.instruction("xor esi, esi");                                        // indexed arrays have no high payload word
    emitter.instruction("mov eax, 4");                                          // runtime tag four boxes an indexed array
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // retain the raw node array for its returned Mixed owner
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // preserve the returned box while dropping the extra raw owner
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the independently owned raw node array
    abi::emit_call_label(emitter, "__rt_decref_any");                            // leave the returned Mixed box as sole array owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // restore the owned boxed PHP node array
    emitter.instruction("mov rsp, rbp");                                        // release the node-array builder frame
    emitter.instruction("pop rbp");                                             // restore the caller frame after successful materialization
    emitter.instruction("ret");                                                 // return one owned boxed PHP node array
    emitter.label("__rt_dom_host_xpath_build_nodes_fail_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // release the partially initialized PHP node array
    abi::emit_call_label(emitter, "__rt_decref_any");                            // release every already-boxed wrapper transitively
    emitter.instruction("xor eax, eax");                                        // return a contained materialization failure sentinel
    emitter.instruction("mov rsp, rbp");                                        // release the node-array builder frame
    emitter.instruction("pop rbp");                                             // restore the caller frame after failed materialization
    emitter.instruction("ret");                                                 // let the host callback reject the malformed exchange
}
