//! Purpose:
//! Describes borrowed native values for the shared mbstring parameter planner.
//!
//! Called from:
//! - Native/eval argument adapters before elephc_mbstring_prepare_v1.
//!
//! Key details:
//! - C inputs are eval context, borrowed boxed value, MbCoercionInputV1 output, and owner output.
//! - Scalars retain their actual kinds; arrays are not traversed and strings are not cast.
//! - Only dynamic class-name metadata may acquire an owner, released by the native GC helper.
//! - Metadata lookup invokes no PHP callbacks and never borrows Rust mbstring request state.

use super::*;

mod objects;

/// Emits concrete input classification with optional eval-backed class introspection.
pub(super) fn emit(emitter: &mut Emitter, eval_bridge: bool) {
    match emitter.target.arch {
        Arch::AArch64 => aarch64(emitter, eval_bridge),
        Arch::X86_64 => x86_64(emitter, eval_bridge),
    }
}

/// Emits AArch64 classification and ownership cleanup for the borrowed input descriptor.
fn aarch64(emitter: &mut Emitter, eval_bridge: bool) {
    emitter.label_global("__rt_mbstring_input");
    emitter.instruction("cbz x2, __rt_mbstring_input_invalid");                 // require a writable concrete-value descriptor
    emitter.instruction("cbz x3, __rt_mbstring_input_invalid");                 // require a writable metadata-owner slot
    emitter.instruction("str xzr, [x3]");                                       // ordinary inputs acquire no additional native owner
    emitter.instruction("stp xzr, xzr, [x2]");                                  // clear the initial kind and numeric payload
    emitter.instruction("stp xzr, xzr, [x2, #16]");                             // clear borrowed string bytes and length
    emitter.instruction("str xzr, [x2, #32]");                                  // clear Stringable/resource capability flags
    emitter.instruction("sub sp, sp, #144");                                    // reserve common metadata, eval result, and member-name storage
    emitter.instruction("stp x29, x30, [sp, #128]");                            // preserve the C caller across introspection helpers
    emitter.instruction("add x29, sp, #128");                                   // establish an aligned input-description frame
    emitter.instruction("stp x0, x1, [sp]");                                    // retain optional eval context and the borrowed source box
    emitter.instruction("stp x2, x3, [sp, #16]");                               // retain descriptor and metadata-owner output pointers
    emitter.instruction("stp xzr, xzr, [sp, #48]");                             // initialize optional eval result kind and value ownership
    emitter.instruction("str xzr, [sp, #64]");                                  // no eval throwable output exists before metadata lookup
    emitter.instruction("mov x0, x1");                                          // inspect the concrete value without creating a PHP cast
    emitter.instruction("bl __rt_mixed_unbox");                                 // preserve null sentinels and unwrap nested Mixed cells
    emitter.instruction("ldr x10, [sp, #16]");                                  // recover the caller-owned descriptor
    emitter.instruction("str x0, [x10]");                                       // retain the actual concrete runtime kind
    emitter.instruction("cmp x0, #1");                                          // identify strings before the numeric/pointer payload branch
    emitter.instruction("b.eq __rt_mbstring_input_string");                     // borrow the complete original binary string
    emitter.instruction("cmp x0, #8");                                          // normalize explicit and pointer-encoded null
    emitter.instruction("b.eq __rt_mbstring_input_done");                       // null keeps every payload word zero
    emitter.instruction("str x1, [x10, #8]");                                   // retain numeric bits or opaque container identity
    emitter.instruction("cmp x0, #6");                                          // objects additionally need their real class and Stringable capability
    emitter.instruction("b.eq __rt_mbstring_input_object");                     // inspect object metadata without executing its methods
    emitter.instruction("cmp x0, #10");                                         // callable descriptors are PHP Closure objects
    emitter.instruction("b.eq __rt_mbstring_input_closure");                    // preserve Closure in TypeError diagnostics
    emitter.instruction("cmp x0, #4");                                          // indexed arrays may carry the request's cached catalog identity
    emitter.instruction("b.eq __rt_mbstring_input_array");                      // inspect identity without traversing or comparing values
    emitter.instruction("cmp x0, #5");                                          // accept the remaining scalar and array tags zero through five
    emitter.instruction("b.ls __rt_mbstring_input_done");                       // retain scalars and arrays without allocating
    emitter.instruction("cmp x0, #9");                                          // resources retain an opaque identity without string conversion
    emitter.instruction("b.eq __rt_mbstring_input_done");                       // PHP TypeErrors use resource for open and closed handles
    emitter.instruction("b __rt_mbstring_input_fatal");                         // reject internal markers that are not concrete PHP values
    emitter.label("__rt_mbstring_input_string");
    emitter.instruction("stp x1, x2, [x10, #16]");                              // preserve bytes and length while leaving scalar payload zero
    emitter.instruction("b __rt_mbstring_input_done");                          // borrowed strings require no metadata owner
    emitter.label("__rt_mbstring_input_array");
    abi::emit_load_symbol_to_reg(emitter, "x9", "_mbstring_catalog_array", 0);
    emitter.instruction("cbz x9, __rt_mbstring_input_done");                    // no array qualifies before catalog initialization
    emitter.instruction("cmp x1, x9");                                          // compare the retained payload with the request's catalog
    emitter.instruction("cset x9, eq");                                         // encode the kind-specific catalog identity capability
    emitter.instruction("str x9, [x10, #32]");                                  // publish the flag without retaining another owner
    emitter.instruction("b __rt_mbstring_input_done");                          // array description is complete without PHP callbacks
    emitter.label("__rt_mbstring_input_closure");
    emitter.instruction("mov x9, #6");                                          // expose a concrete PHP object to the shared planner
    emitter.instruction("str x9, [x10]");                                       // replace the internal callable kind with the object kind
    abi::emit_symbol_address(emitter, "x1", "_sprintf_closure_class_name");
    emitter.instruction("mov x2, #7");                                          // preserve the complete Closure class spelling
    emitter.instruction("stp x1, x2, [x10, #16]");                              // borrow immutable class-name metadata
    emitter.instruction("b __rt_mbstring_input_done");                          // Closure has no Stringable conversion handler
    objects::aarch64(emitter, eval_bridge);
    emitter.label("__rt_mbstring_input_fatal");
    emitter.instruction("ldr x10, [sp, #24]");                                  // locate any dynamic class-name owner acquired before failure
    emitter.instruction("ldr x0, [x10]");                                       // take ownership of the failed metadata result
    emitter.instruction("str xzr, [x10]");                                      // prevent caller cleanup from releasing the same owner twice
    emitter.instruction("bl __rt_decref_any");                                  // release only optional metadata, never the borrowed argument
    emitter.instruction("ldr x0, [sp, #64]");                                   // take any exceptional eval metadata output
    emitter.instruction("bl __rt_decref_any");                                  // release exceptional metadata before returning internal failure
    emitter.instruction("ldr x10, [sp, #16]");                                  // clear stale borrowed class bytes after releasing their owner
    emitter.instruction("stp xzr, xzr, [x10]");                                 // failure publishes no concrete descriptor kind or payload
    emitter.instruction("stp xzr, xzr, [x10, #16]");                            // failure publishes no dangling byte range
    emitter.instruction("str xzr, [x10, #32]");                                 // failure publishes no Stringable capability
    emitter.instruction("mov w0, #1");                                          // return RuntimeFatal for malformed host metadata
    emitter.instruction("b __rt_mbstring_input_return");                        // restore the same frame on every failure
    emitter.label("__rt_mbstring_input_done");
    emitter.instruction("mov w0, #0");                                          // report a complete borrowed descriptor
    emitter.label("__rt_mbstring_input_return");
    emitter.instruction("ldp x29, x30, [sp, #128]");                            // restore C caller linkage after all metadata helpers return
    emitter.instruction("add sp, sp, #144");                                    // release local metadata and eval result storage
    emitter.instruction("ret");                                                 // return Success or RuntimeFatal without a PHP callback
    emitter.label("__rt_mbstring_input_invalid");
    emitter.instruction("mov w0, #1");                                          // invalid output pointers acquire no ownership
    emitter.instruction("ret");                                                 // leave caller memory untouched when the ABI contract is invalid
}

/// Emits SysV classification while preserving scalar bits and optional dynamic metadata ownership.
fn x86_64(emitter: &mut Emitter, eval_bridge: bool) {
    emitter.label_global("__rt_mbstring_input");
    emitter.instruction("test rdx, rdx");                                       // require a concrete-value output descriptor
    emitter.instruction("jz __rt_mbstring_input_invalid");                      // reject null descriptor storage
    emitter.instruction("test rcx, rcx");                                       // require a separate metadata-owner output slot
    emitter.instruction("jz __rt_mbstring_input_invalid");                      // reject null owner storage before acquiring anything
    emitter.instruction("mov QWORD PTR [rcx], 0");                              // ordinary values acquire no extra native owner
    for offset in [0, 8, 16, 24, 32] {
        emitter.instruction(&format!("mov QWORD PTR [rdx + {offset}], 0"));     // initialize payload ranges and capability flags
    }
    emitter.instruction("push rbp");                                            // align the stack for C metadata helpers
    emitter.instruction("mov rbp, rsp");                                        // preserve a stable caller frame
    emitter.instruction("sub rsp, 128");                                        // reserve common spills, eval output, and a borrowed method-name cell
    for (offset, register) in [(0, "rdi"), (8, "rsi"), (16, "rdx"), (24, "rcx")] {
        emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], {register}")); // preserve each C input across unboxing and metadata calls
    }
    for offset in [48, 56, 64] {
        emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], 0"));     // no eval result or throwable ownership exists before metadata lookup
    }
    emitter.instruction("mov rax, rsi");                                        // inspect the original borrowed boxed argument
    emitter.instruction("call __rt_mixed_unbox");                               // unwrap nested cells and normalize null containers
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // recover the concrete-value descriptor
    emitter.instruction("mov QWORD PTR [r10], rax");                            // preserve the actual runtime kind
    emitter.instruction("cmp rax, 1");                                          // handle binary strings without a scalar cast
    emitter.instruction("je __rt_mbstring_input_string");                       // retain the original byte range
    emitter.instruction("cmp rax, 8");                                          // null has no numeric or pointer payload
    emitter.instruction("je __rt_mbstring_input_done");                         // preserve zeroed null fields
    emitter.instruction("mov QWORD PTR [r10 + 8], rdi");                        // retain raw numeric bits or opaque array/object/resource identity
    emitter.instruction("cmp rax, 6");                                          // inspect object class metadata before deciding Stringable behavior
    emitter.instruction("je __rt_mbstring_input_object");                       // do not execute an object conversion during description
    emitter.instruction("cmp rax, 10");                                         // native callable descriptors represent PHP Closure objects
    emitter.instruction("je __rt_mbstring_input_closure");                      // retain Closure in PHP parameter errors
    emitter.instruction("cmp rax, 4");                                          // indexed arrays may share the cached catalog payload
    emitter.instruction("je __rt_mbstring_input_array");                        // test identity independently of candidate names
    emitter.instruction("cmp rax, 5");                                          // accept the remaining scalar and array kinds
    emitter.instruction("jbe __rt_mbstring_input_done");                        // scalars and arrays borrow storage without allocating
    emitter.instruction("cmp rax, 9");                                          // recognize concrete open or closed resource handles
    emitter.instruction("je __rt_mbstring_input_done");                         // preserve resource identity without using its display string
    emitter.instruction("jmp __rt_mbstring_input_fatal");                       // reject internal non-value markers
    emitter.label("__rt_mbstring_input_string");
    emitter.instruction("mov QWORD PTR [r10 + 16], rdi");                       // retain the borrowed binary string pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], rdx");                       // retain its complete byte length
    emitter.instruction("jmp __rt_mbstring_input_done");                        // strings require no additional metadata owner
    emitter.label("__rt_mbstring_input_array");
    abi::emit_load_symbol_to_reg(emitter, "r11", "_mbstring_catalog_array", 0);
    emitter.instruction("test r11, r11");                                       // reject catalog identity while the request cache is empty
    emitter.instruction("jz __rt_mbstring_input_done");                         // preserve the zero capability flags
    emitter.instruction("cmp rdi, r11");                                        // compare exact native array payload identities
    emitter.instruction("sete r11b");                                           // mark only the retained cached array as order-independent
    emitter.instruction("movzx r11, r11b");                                     // clear unused capability bits
    emitter.instruction("mov QWORD PTR [r10 + 32], r11");                       // publish the array-specific capability
    emitter.instruction("jmp __rt_mbstring_input_done");                        // complete the nonmutating array description
    emitter.label("__rt_mbstring_input_closure");
    emitter.instruction("mov QWORD PTR [r10], 6");                              // expose the PHP object kind for internal callable descriptors
    abi::emit_symbol_address(emitter, "rdi", "_sprintf_closure_class_name");
    emitter.instruction("mov QWORD PTR [r10 + 16], rdi");                       // borrow the immutable Closure class spelling
    emitter.instruction("mov QWORD PTR [r10 + 24], 7");                         // preserve its exact byte length
    emitter.instruction("jmp __rt_mbstring_input_done");                        // Closure has no Stringable conversion capability
    objects::x86_64(emitter, eval_bridge);
    emitter.label("__rt_mbstring_input_fatal");
    emitter.instruction("mov r10, QWORD PTR [rsp + 24]");                       // recover optional class-name ownership acquired before failure
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // take the failed metadata owner
    emitter.instruction("mov QWORD PTR [r10], 0");                              // prevent duplicate owner cleanup by the caller
    emitter.instruction("call __rt_decref_any");                                // release metadata without touching the borrowed source argument
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // take any exceptional metadata output from eval
    emitter.instruction("call __rt_decref_any");                                // release exceptional metadata before returning internal failure
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // clear dangling class bytes after metadata cleanup
    for offset in [0, 8, 16, 24, 32] {
        emitter.instruction(&format!("mov QWORD PTR [r10 + {offset}], 0"));     // failure publishes no partially initialized descriptor
    }
    emitter.instruction("mov eax, 1");                                          // report RuntimeFatal for invalid host metadata
    emitter.instruction("jmp __rt_mbstring_input_return");                      // restore the same frame on every failure
    emitter.label("__rt_mbstring_input_done");
    emitter.instruction("xor eax, eax");                                        // report a complete borrowed descriptor
    emitter.label("__rt_mbstring_input_return");
    emitter.instruction("leave");                                               // discard metadata spills and restore caller linkage
    emitter.instruction("ret");                                                 // return Success or RuntimeFatal without executing PHP
    emitter.label("__rt_mbstring_input_invalid");
    emitter.instruction("mov eax, 1");                                          // invalid output pointers acquire no ownership
    emitter.instruction("ret");                                                 // leave caller memory untouched when the ABI contract is invalid
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
