//! Purpose:
//! Materializes canonical PHP DOM wrappers for XPath node-set callback arguments.
//! Uses generated wrapper-kind and class-layout tables without target-specific class assumptions.
//!
//! Called from:
//! - `super::emit_dom_runtime()` for the private XPath host callback path.
//!
//! Key details:
//! - Cache hits and fresh allocations both return one owned PHP object reference.
//! - Hidden context/handle metadata exactly matches ordinary internal-extension wrappers.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific wrapper materialization helper.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        emit_result_handle_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
        emit_result_handle_aarch64(emitter);
    }
}

/// Emits the AArch64 wrapper materialization helper.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host XPath wrapper materialization ---");
    emitter.label_global("__rt_dom_host_xpath_wrapper_from_kind");
    emitter.instruction("sub sp, sp, #80");                                     // reserve context, handle, kind, class, size, object, and frame spills
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame across allocation and cache helpers
    emitter.instruction("add x29, sp, #64");                                    // establish the wrapper materialization frame
    emitter.instruction("stp x0, x1, [sp]");                                    // preserve the DOM context and native handle
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the stable native wrapper kind
    abi::emit_call_label(emitter, "__rt_dom_wrapper_cache_get");                 // return the retained canonical wrapper when already materialized
    emitter.instruction("cbnz x0, __rt_dom_host_xpath_wrapper_done");           // a cache hit already owns its result reference
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the requested stable wrapper kind
    abi::emit_symbol_address(emitter, "x10", "_dom_wrapper_kind_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the generated dense kind-table bound
    emitter.instruction("cmp x9, x10");                                         // can the stable kind index the generated table?
    emitter.instruction("b.hs __rt_dom_host_xpath_wrapper_fail");               // reject unknown or forged wrapper kinds
    abi::emit_symbol_address(emitter, "x10", "_dom_wrapper_kind_class_ids");
    emitter.instruction("ldr x11, [x10, x9, lsl #3]");                          // resolve the program-specific PHP class ID
    emitter.instruction("cmn x11, #1");                                         // is this wrapper class absent from the compiled program?
    emitter.instruction("b.eq __rt_dom_host_xpath_wrapper_fail");               // never allocate an unavailable PHP class
    emitter.instruction("str x11, [sp, #24]");                                  // preserve the class ID across heap allocation
    abi::emit_symbol_address(emitter, "x10", "_class_internal_extension_hidden_offsets");
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // load the compiler-hidden wrapper metadata offset
    emitter.instruction("cbz x12, __rt_dom_host_xpath_wrapper_fail");           // only native wrapper classes carry this metadata tail
    emitter.instruction("str x12, [sp, #48]");                                  // preserve the hidden metadata base
    abi::emit_symbol_address(emitter, "x10", "_class_object_payload_sizes");
    emitter.instruction("ldr x13, [x10, x11, lsl #3]");                         // load the complete object payload size
    emitter.instruction("cbz x13, __rt_dom_host_xpath_wrapper_fail");           // reject a class without allocatable runtime storage
    emitter.instruction("str x13, [sp, #32]");                                  // preserve the payload size across allocation
    emitter.instruction("mov x0, x13");                                         // request exactly the generated object payload bytes
    abi::emit_call_label(emitter, "__rt_heap_alloc");                            // allocate one ordinary runtime object payload
    emitter.instruction("str x0, [sp, #40]");                                   // preserve the fresh object across initialization helpers
    emitter.instruction("mov x9, #4");                                          // heap kind four identifies an object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the uniform object heap header
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the concrete runtime class ID
    emitter.instruction("str x11, [x0]");                                       // publish the class ID at the object header
    emitter.instruction("mov x9, #8");                                          // begin zeroing after the class ID word
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the complete object payload size
    emitter.label("__rt_dom_host_xpath_wrapper_zero");
    emitter.instruction("cmp x9, x10");                                         // has every property and hidden slot been initialized?
    emitter.instruction("b.hs __rt_dom_host_xpath_wrapper_zero_done");          // continue once the entire payload tail is canonical zero
    emitter.instruction("str xzr, [x0, x9]");                                   // clear one eight-byte payload word
    emitter.instruction("add x9, x9, #8");                                      // advance to the next payload word
    emitter.instruction("b __rt_dom_host_xpath_wrapper_zero");                  // clear the remaining object storage
    emitter.label("__rt_dom_host_xpath_wrapper_zero_done");
    abi::emit_symbol_address(emitter, "x10", "_class_propinit_ptrs");
    emitter.instruction("ldr x10, [x10, x11, lsl #3]");                         // load an optional generated property-default thunk
    emitter.instruction("cbz x10, __rt_dom_host_xpath_wrapper_metadata");       // wrappers without defaults need no PHP property initialization
    emitter.instruction("blr x10");                                             // initialize declared PHP property defaults
    emitter.instruction("ldr x0, [sp, #40]");                                   // restore the wrapper after the property thunk
    emitter.label("__rt_dom_host_xpath_wrapper_metadata");
    emitter.instruction("ldr x12, [sp, #48]");                                  // reload the hidden wrapper metadata base
    emitter.instruction("add x10, x0, x12");                                    // address the compiler-hidden metadata tail
    emitter.instruction("mov x9, #1");                                          // mark the native wrapper as fully initialized
    emitter.instruction("str x9, [x10]");                                       // publish the initialized marker
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the runtime class ID
    emitter.instruction("str x11, [x10, #8]");                                  // store the concrete wrapper class ID
    emitter.instruction("ldr x9, [sp]");                                        // reload the DOM context ID
    emitter.instruction("str x9, [x10, #16]");                                  // store the owning DOM context
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the native bridge handle
    emitter.instruction("str x9, [x10, #32]");                                  // store the generation-checked native handle
    emitter.instruction("str x11, [x10, #48]");                                 // store the finalizer's concrete class discriminator
    emitter.instruction("ldr x0, [sp]");                                        // cache insertion arg0 is the DOM context ID
    emitter.instruction("ldr x1, [sp, #8]");                                    // cache insertion arg1 is the native handle
    emitter.instruction("ldr x2, [sp, #40]");                                   // cache insertion arg2 is the borrowed new wrapper
    abi::emit_call_label(emitter, "__rt_dom_wrapper_cache_put");                 // publish canonical wrapper identity and return the object
    emitter.instruction("b __rt_dom_host_xpath_wrapper_done");                  // return the fresh owned object reference
    emitter.label("__rt_dom_host_xpath_wrapper_fail");
    emitter.instruction("mov x0, xzr");                                         // malformed kind/class metadata returns a null sentinel
    emitter.label("__rt_dom_host_xpath_wrapper_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame
    emitter.instruction("add sp, sp, #80");                                     // release wrapper materialization storage
    emitter.instruction("ret");                                                 // return one owned wrapper object or null
}

/// Emits the Linux x86_64 wrapper materialization helper.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host XPath wrapper materialization ---");
    emitter.label_global("__rt_dom_host_xpath_wrapper_from_kind");
    emitter.instruction("push rbp");                                            // preserve the caller frame across runtime helpers
    emitter.instruction("mov rbp, rsp");                                        // establish the wrapper materialization frame
    emitter.instruction("sub rsp, 64");                                         // reserve context, handle, kind, class, size, object, and metadata spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the DOM context ID
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the generation-checked native handle
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the stable wrapper kind
    abi::emit_call_label(emitter, "__rt_dom_wrapper_cache_get");                 // return a retained canonical wrapper when present
    emitter.instruction("test rax, rax");                                       // did the weak cache resolve this native identity?
    emitter.instruction("jnz __rt_dom_host_xpath_wrapper_done_x86");            // a cache hit already owns its result reference
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the requested stable wrapper kind
    abi::emit_symbol_address(emitter, "r10", "_dom_wrapper_kind_count");
    emitter.instruction("cmp rax, QWORD PTR [r10]");                            // can this kind index the generated dense table?
    emitter.instruction("jae __rt_dom_host_xpath_wrapper_fail_x86");            // reject unknown or forged wrapper kinds
    abi::emit_symbol_address(emitter, "r10", "_dom_wrapper_kind_class_ids");
    emitter.instruction("mov r11, QWORD PTR [r10 + rax * 8]");                  // resolve the program-specific PHP class ID
    emitter.instruction("cmp r11, -1");                                         // is this wrapper class absent from the program?
    emitter.instruction("je __rt_dom_host_xpath_wrapper_fail_x86");             // never allocate an unavailable PHP class
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // preserve the class ID across heap allocation
    abi::emit_symbol_address(emitter, "r10", "_class_internal_extension_hidden_offsets");
    emitter.instruction("mov rcx, QWORD PTR [r10 + r11 * 8]");                  // load the compiler-hidden wrapper metadata offset
    emitter.instruction("test rcx, rcx");                                       // does this class own native-wrapper hidden metadata?
    emitter.instruction("jz __rt_dom_host_xpath_wrapper_fail_x86");             // reject ordinary PHP objects
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // preserve the hidden metadata base
    abi::emit_symbol_address(emitter, "r10", "_class_object_payload_sizes");
    emitter.instruction("mov rax, QWORD PTR [r10 + r11 * 8]");                  // load the complete object payload size
    emitter.instruction("test rax, rax");                                       // is allocatable storage available?
    emitter.instruction("jz __rt_dom_host_xpath_wrapper_fail_x86");             // reject missing class layout metadata
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the payload size
    abi::emit_call_label(emitter, "__rt_heap_alloc");                            // allocate one ordinary runtime object payload
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the fresh wrapper object
    emitter.instruction("mov QWORD PTR [rax - 8], 4");                          // heap kind four identifies an object instance
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the concrete runtime class ID
    emitter.instruction("mov QWORD PTR [rax], r11");                            // publish the class ID at the object header
    emitter.instruction("mov rcx, 8");                                          // begin zeroing after the class ID word
    emitter.label("__rt_dom_host_xpath_wrapper_zero_x86");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 40]");                       // has the complete payload tail been initialized?
    emitter.instruction("jae __rt_dom_host_xpath_wrapper_zero_done_x86");       // continue once all properties and hidden slots are zero
    emitter.instruction("mov QWORD PTR [rax + rcx], 0");                        // clear one eight-byte payload word
    emitter.instruction("add rcx, 8");                                          // advance to the next payload word
    emitter.instruction("jmp __rt_dom_host_xpath_wrapper_zero_x86");            // clear the remaining object storage
    emitter.label("__rt_dom_host_xpath_wrapper_zero_done_x86");
    abi::emit_symbol_address(emitter, "r10", "_class_propinit_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r10 + r11 * 8]");                  // load an optional generated property-default thunk
    emitter.instruction("test r10, r10");                                       // does this wrapper declare property defaults?
    emitter.instruction("jz __rt_dom_host_xpath_wrapper_metadata_x86");         // skip absent property initialization
    emitter.instruction("call r10");                                            // initialize declared PHP property defaults
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // restore the wrapper after the property thunk
    emitter.label("__rt_dom_host_xpath_wrapper_metadata_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the hidden wrapper metadata base
    emitter.instruction("lea r10, [rax + rcx]");                                // address the compiler-hidden metadata tail
    emitter.instruction("mov QWORD PTR [r10], 1");                              // mark the native wrapper as fully initialized
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the concrete runtime class ID
    emitter.instruction("mov QWORD PTR [r10 + 8], r11");                        // store the concrete wrapper class ID
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the owning DOM context
    emitter.instruction("mov QWORD PTR [r10 + 16], rcx");                       // store the context in hidden metadata
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the native bridge handle
    emitter.instruction("mov QWORD PTR [r10 + 32], rcx");                       // store the generation-checked handle
    emitter.instruction("mov QWORD PTR [r10 + 48], r11");                       // store the finalizer's class discriminator
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // cache insertion arg0 is the DOM context ID
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // cache insertion arg1 is the native handle
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // cache insertion arg2 is the borrowed new wrapper
    abi::emit_call_label(emitter, "__rt_dom_wrapper_cache_put");                 // publish canonical identity and return the object
    emitter.instruction("jmp __rt_dom_host_xpath_wrapper_done_x86");            // return the fresh owned wrapper
    emitter.label("__rt_dom_host_xpath_wrapper_fail_x86");
    emitter.instruction("xor eax, eax");                                        // malformed kind/class metadata returns null
    emitter.label("__rt_dom_host_xpath_wrapper_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // release wrapper materialization storage
    emitter.instruction("pop rbp");                                             // restore the caller frame
    emitter.instruction("ret");                                                 // return one owned wrapper object or null
}

/// Emits the AArch64 callback-result wrapper validator.
fn emit_result_handle_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host XPath result wrapper validation ---");
    emitter.label_global("__rt_dom_host_xpath_result_handle");
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the runtime object payload from the boxed callback result
    emitter.instruction("cbz x9, __rt_dom_host_xpath_result_handle_fail");      // reject a null object payload
    emitter.instruction("ldr x10, [x9]");                                       // load the program-specific runtime class ID
    abi::emit_symbol_address(
        emitter,
        "x11",
        "_class_internal_extension_hidden_offsets_count",
    );
    emitter.instruction("ldr x11, [x11]");                                      // load the generated hidden-offset table bound
    emitter.instruction("cmp x10, x11");                                        // can the object class index native-wrapper metadata?
    emitter.instruction("b.hs __rt_dom_host_xpath_result_handle_fail");         // reject forged or unavailable class IDs
    abi::emit_symbol_address(
        emitter,
        "x11",
        "_class_internal_extension_hidden_offsets",
    );
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the compiler-hidden wrapper metadata offset
    emitter.instruction("cbz x12, __rt_dom_host_xpath_result_handle_fail");     // ordinary PHP objects have no native DOM metadata
    emitter.instruction("add x9, x9, x12");                                     // address the wrapper's hidden metadata tail
    emitter.instruction("ldr x11, [x9]");                                       // load the native-wrapper initialized marker
    emitter.instruction("cmp x11, #1");                                         // is this a fully initialized native wrapper?
    emitter.instruction("b.ne __rt_dom_host_xpath_result_handle_fail");         // reject partially constructed or forged objects
    emitter.instruction("ldr x11, [x9, #8]");                                   // load the stored concrete wrapper class ID
    emitter.instruction("cmp x11, x10");                                        // must metadata agree with the object header?
    emitter.instruction("b.ne __rt_dom_host_xpath_result_handle_fail");         // reject mismatched wrapper metadata
    emitter.instruction("ldr x11, [x9, #16]");                                  // load the owning DOM context ID
    emitter.instruction("cmp x11, x1");                                         // does the wrapper belong to this bridge context?
    emitter.instruction("b.ne __rt_dom_host_xpath_result_handle_fail");         // reject cross-context or stale wrappers
    emitter.instruction("ldr x0, [x9, #32]");                                   // return the generation-checked native bridge handle
    emitter.instruction("cbz x0, __rt_dom_host_xpath_result_handle_fail");      // initialized wrappers always carry a nonzero handle
    emitter.instruction("ret");                                                 // return the validated bridge handle
    emitter.label("__rt_dom_host_xpath_result_handle_fail");
    emitter.instruction("mov x0, xzr");                                         // ordinary or malformed objects return a null sentinel
    emitter.instruction("ret");                                                 // let the caller raise PHP's exact TypeError
}

/// Emits the Linux x86_64 callback-result wrapper validator.
fn emit_result_handle_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host XPath result wrapper validation ---");
    emitter.label_global("__rt_dom_host_xpath_result_handle");
    emitter.instruction("mov r9, QWORD PTR [rdi + 8]");                         // load the runtime object payload from the boxed callback result
    emitter.instruction("test r9, r9");                                         // is the callback object payload non-null?
    emitter.instruction("jz __rt_dom_host_xpath_result_handle_fail_x86");       // reject a null object payload
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the program-specific runtime class ID
    abi::emit_load_symbol_to_reg(
        emitter,
        "r11",
        "_class_internal_extension_hidden_offsets_count",
        0,
    );
    emitter.instruction("cmp r10, r11");                                        // can the object class index native-wrapper metadata?
    emitter.instruction("jae __rt_dom_host_xpath_result_handle_fail_x86");      // reject forged or unavailable class IDs
    abi::emit_symbol_address(
        emitter,
        "r11",
        "_class_internal_extension_hidden_offsets",
    );
    emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8]");                  // load the compiler-hidden wrapper metadata offset
    emitter.instruction("test rax, rax");                                       // does this class own native DOM wrapper metadata?
    emitter.instruction("jz __rt_dom_host_xpath_result_handle_fail_x86");       // ordinary PHP objects cannot become XPath nodes
    emitter.instruction("add r9, rax");                                         // address the wrapper's hidden metadata tail
    emitter.instruction("cmp QWORD PTR [r9], 1");                               // is this a fully initialized native wrapper?
    emitter.instruction("jne __rt_dom_host_xpath_result_handle_fail_x86");      // reject partially constructed or forged objects
    emitter.instruction("cmp QWORD PTR [r9 + 8], r10");                         // must metadata agree with the object header?
    emitter.instruction("jne __rt_dom_host_xpath_result_handle_fail_x86");      // reject mismatched wrapper metadata
    emitter.instruction("cmp QWORD PTR [r9 + 16], rsi");                        // does the wrapper belong to this bridge context?
    emitter.instruction("jne __rt_dom_host_xpath_result_handle_fail_x86");      // reject cross-context or stale wrappers
    emitter.instruction("mov rax, QWORD PTR [r9 + 32]");                        // return the generation-checked native bridge handle
    emitter.instruction("test rax, rax");                                       // initialized wrappers always carry a nonzero handle
    emitter.instruction("jz __rt_dom_host_xpath_result_handle_fail_x86");       // reject a cleared or forged handle
    emitter.instruction("ret");                                                 // return the validated bridge handle
    emitter.label("__rt_dom_host_xpath_result_handle_fail_x86");
    emitter.instruction("xor eax, eax");                                        // ordinary or malformed objects return a null sentinel
    emitter.instruction("ret");                                                 // let the caller raise PHP's exact TypeError
}
