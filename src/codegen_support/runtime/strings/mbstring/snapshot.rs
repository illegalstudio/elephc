//! Purpose:
//! Emits the nonmutating host array reader consumed by the shared mbstring snapshot ABI.
//!
//! Called from:
//! - Optional mbstring runtime emission and AOT/eval graph argument adapters.
//!
//! Key details:
//! - The C callback reads indexed slots or the existing insertion-order hash iterator.
//! - Concrete tag/low/high descriptors borrow strings and preserve array payload identity.
//! - Mixed wrappers and tagged scalar slots are normalized without allocating or retaining values.
//! - Every supported target uses the same reader contract and returns explicit failure for bad shapes.

use super::*;
use crate::codegen_support::sentinels::{emit_branch_if_null_container, TAGGED_SCALAR_ARRAY_VALUE_TYPE};

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

/// Emits the target's C-ABI reader without performing PHP calls or changing input ownership.
pub(super) fn emit(emitter: &mut Emitter) {
    match emitter.target.arch { Arch::AArch64 => aarch64(emitter), Arch::X86_64 => x86_64(emitter) }
}

/// Borrows one ordered entry on AArch64 and returns the callback status.
fn aarch64(emitter: &mut Emitter) {
    emit_entry_points(emitter);
    emitter.instruction("sub sp, sp, #80");                                     // reserve reader state and aligned caller linkage
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the C caller frame and return address
    emitter.instruction("str x5, [sp, #56]");                                   // retain whether the caller needs the original concrete value tag
    emitter.instruction("add x29, sp, #64");                                    // establish the borrowed-reader frame
    emitter.instruction("stp x2, x3, [sp, #8]");                                // save cursor and key output pointers
    emitter.instruction("str x4, [sp, #24]");                                   // save the value output pointer
    emitter.instruction("ldr x0, [x1, #8]");                                    // load the opaque native array payload
    emitter.instruction("str x0, [sp]");                                        // retain the borrowed payload across helper calls
    emitter.instruction("ldr x10, [x1]");                                       // read the concrete array representation tag
    emit_branch_if_null_container(emitter, "x0", "x9", "__rt_mbstring_array_next_error");
    emitter.instruction("cmp x10, #4");                                         // recognize an indexed array descriptor
    emitter.instruction("b.eq __rt_mbstring_array_next_indexed");               // read one typed indexed slot
    emitter.instruction("cmp x10, #5");                                         // recognize an associative array descriptor
    emitter.instruction("b.ne __rt_mbstring_array_next_error");                 // reject non-array roots without reading their payload
    emitter.instruction("ldr x1, [x2]");                                        // resume the insertion-order hash cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // borrow the next key and per-entry typed value
    emitter.instruction("cmn x0, #1");                                          // recognize the hash iterator end marker
    emitter.instruction("b.eq __rt_mbstring_array_next_end");                   // publish no entry when the hash walk ends
    emitter.instruction("ldr x9, [sp, #8]");                                    // recover the caller-owned cursor pointer
    emitter.instruction("str x0, [x9]");                                        // publish the next opaque hash cursor
    emitter.instruction("str x5, [sp, #32]");                                   // save the entry tag while writing its key
    emitter.instruction("stp x3, x4, [sp, #40]");                               // save the borrowed value words
    emitter.instruction("mov x9, #1");                                          // default to a string key descriptor
    emitter.instruction("cmn x2, #1");                                          // integer keys use a minus-one length sentinel
    emitter.instruction("b.ne __rt_mbstring_array_next_hash_key");              // preserve string keys without numeric coercion
    emitter.instruction("mov x9, xzr");                                         // select an integer key descriptor
    emitter.instruction("mov x2, xzr");                                         // integer keys have no high payload word
    emitter.label("__rt_mbstring_array_next_hash_key");
    emitter.instruction("ldr x10, [sp, #16]");                                  // recover the key output descriptor
    emitter.instruction("stp x9, x1, [x10]");                                   // publish the exact key tag and low word
    emitter.instruction("str x2, [x10, #16]");                                  // publish the binary key length
    emitter.instruction("ldr x0, [sp, #32]");                                   // restore the concrete or Mixed value tag
    emitter.instruction("ldp x1, x2, [sp, #40]");                               // restore the borrowed value payload
    emitter.instruction("b __rt_mbstring_array_next_value");                    // normalize the value before returning it
    emitter.label("__rt_mbstring_array_next_indexed");
    emitter.instruction("ldr x9, [x2]");                                        // read the next logical indexed position
    emitter.instruction("ldr x10, [x0]");                                       // read the indexed array length
    emitter.instruction("cmp x9, x10");                                         // check the live bounds before loading a slot
    emitter.instruction("b.hs __rt_mbstring_array_next_end");                   // finish without reading beyond the indexed payload
    emitter.instruction("ldr x12, [x0, #-8]");                                  // read the indexed element type from heap metadata
    emitter.instruction("lsr x12, x12, #8");                                    // move the element type into the low bits
    emitter.instruction("and x12, x12, #0x7f");                                 // remove the persistent COW flag
    emitter.instruction("mov x13, #8");                                         // ordinary scalar and pointer slots occupy one word
    emitter.instruction("cmp x12, #1");                                         // strings occupy a pointer and length pair
    emitter.instruction("b.eq __rt_mbstring_array_next_wide");                  // select a two-word string slot
    emitter.instruction(&format!("cmp x12, #{}", TAGGED_SCALAR_ARRAY_VALUE_TYPE)); // nullable scalar slots contain a payload and runtime tag
    emitter.instruction("b.ne __rt_mbstring_array_next_stride");                // retain one-word storage for other types
    emitter.label("__rt_mbstring_array_next_wide");
    emitter.instruction("mov x13, #16");                                        // select the two-word physical slot size
    emitter.label("__rt_mbstring_array_next_stride");
    emitter.instruction("ldr x10, [x0, #16]");                                  // read the authoritative physical slot width
    emitter.instruction("cmp x10, x13");                                        // require type metadata and slot width to agree
    emitter.instruction("b.ne __rt_mbstring_array_next_error");                 // reject malformed storage before reading an element
    emitter.instruction("ldr x11, [sp, #16]");                                  // recover the key output descriptor
    emitter.instruction("stp xzr, x9, [x11]");                                  // publish the integer index without allocating a key cell
    emitter.instruction("str xzr, [x11, #16]");                                 // clear the unused key high word
    emitter.instruction("add x11, x0, #24");                                    // address the indexed payload after its header
    emitter.instruction("madd x11, x9, x13, x11");                              // address the selected typed slot
    emitter.instruction("add x9, x9, #1");                                      // advance to the next logical position
    emitter.instruction("str x9, [x2]");                                        // publish the next indexed cursor
    emitter.instruction("mov x0, x12");                                         // load the declared slot value type
    emitter.instruction("ldr x1, [x11]");                                       // borrow the low payload word
    emitter.instruction("mov x2, xzr");                                         // clear high data for ordinary one-word slots
    emitter.instruction("cmp x0, #1");                                          // check for a string pointer and length pair
    emitter.instruction("b.eq __rt_mbstring_array_next_string_slot");           // load the complete binary string descriptor
    emitter.instruction(&format!("cmp x0, #{}", TAGGED_SCALAR_ARRAY_VALUE_TYPE)); // check for a nullable scalar tag/payload pair
    emitter.instruction("b.ne __rt_mbstring_array_next_value");                 // normalize the ordinary slot representation
    emitter.instruction("ldr x0, [x11, #8]");                                   // use the per-slot scalar or null tag
    emitter.instruction("b __rt_mbstring_array_next_value");                    // preserve scalar payload bits without evaluating them
    emitter.label("__rt_mbstring_array_next_string_slot");
    emitter.instruction("ldr x2, [x11, #8]");                                   // borrow the exact string byte length
    emitter.label("__rt_mbstring_array_next_value");
    emitter.instruction("ldr x9, [sp, #56]");                                   // retain original Mixed cells when an owned copy needs resource or reference identity
    emitter.instruction("cbnz x9, __rt_mbstring_array_next_store");             // defer boxed-value copying to the resource-aware clone helper
    emitter.instruction("cmp x0, #7");                                          // Mixed slots point to a boxed runtime cell
    emitter.instruction("b.ne __rt_mbstring_array_next_concrete");              // already-concrete slots need no unboxing
    emitter.instruction("mov x0, x1");                                          // pass the borrowed Mixed cell to the shared unboxer
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel nested wrappers and normalize container nulls
    emitter.label("__rt_mbstring_array_next_concrete");
    emitter.instruction("cmp x0, #4");                                          // only container tags can hold the null sentinel
    emitter.instruction("b.lo __rt_mbstring_array_next_store");                 // preserve integers, strings, floats, and booleans
    emitter.instruction("cmp x0, #6");                                          // arrays and objects occupy the nullable container tag range
    emitter.instruction("b.hi __rt_mbstring_array_next_other");                 // classify null and unsupported non-container tags
    emit_branch_if_null_container(emitter, "x1", "x9", "__rt_mbstring_array_next_null");
    emitter.instruction("cmp x0, #6");                                          // objects are unsupported by recursive mbstring operations
    emitter.instruction("b.lo __rt_mbstring_array_next_store");                 // preserve indexed and associative array identity
    emitter.label("__rt_mbstring_array_next_other");
    emitter.instruction("cmp x0, #8");                                          // retain canonical PHP null values
    emitter.instruction("b.eq __rt_mbstring_array_next_null");                  // clear unused null payload words
    emitter.instruction("mov x0, #6");                                          // represent objects, resources, and other unsupported host values
    emitter.instruction("mov x1, xzr");                                         // do not export unsupported host payload ownership
    emitter.instruction("mov x2, xzr");                                         // clear unused unsupported high data
    emitter.instruction("b __rt_mbstring_array_next_store");                    // publish the explicit unsupported shape
    emitter.label("__rt_mbstring_array_next_null");
    emitter.instruction("mov x0, #8");                                          // select the canonical null descriptor
    emitter.instruction("mov x1, xzr");                                         // clear the null low word
    emitter.instruction("mov x2, xzr");                                         // clear the null high word
    emitter.label("__rt_mbstring_array_next_store");
    emitter.instruction("ldr x9, [sp, #24]");                                   // recover the value output descriptor
    emitter.instruction("stp x0, x1, [x9]");                                    // publish its concrete tag and low payload
    emitter.instruction("str x2, [x9, #16]");                                   // publish its borrowed string length or high word
    emitter.instruction("mov x0, #1");                                          // report one complete borrowed entry
    emitter.instruction("b __rt_mbstring_array_next_return");                   // restore the C caller after success
    emitter.label("__rt_mbstring_array_next_end");
    emitter.instruction("mov x0, #0");                                          // report the end of the ordered array walk
    emitter.instruction("b __rt_mbstring_array_next_return");                   // restore the C caller without publishing values
    emitter.label("__rt_mbstring_array_next_error");
    emitter.instruction("mov x0, #2");                                          // report malformed host metadata without a PHP value
    emitter.label("__rt_mbstring_array_next_return");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the C caller frame and return address
    emitter.instruction("add sp, sp, #80");                                     // release reader state without changing source ownership
    emitter.instruction("ret");                                                 // return the reader status across the C ABI
}

/// Borrows one ordered entry through the x86_64 System V C ABI.
fn x86_64(emitter: &mut Emitter) {
    emit_entry_points(emitter);
    emitter.instruction("push rbp");                                            // preserve the C caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned borrowed-reader frame
    emitter.instruction("sub rsp, 64");                                         // reserve cursor and output descriptors plus borrowed value words
    emitter.instruction("mov QWORD PTR [rsp + 56], r9");                        // retain the raw-value reader mode independently of caller context
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // save the cursor pointer
    emitter.instruction("mov QWORD PTR [rsp + 16], rcx");                       // save the key output pointer
    emitter.instruction("mov QWORD PTR [rsp + 24], r8");                        // save the value output pointer
    emitter.instruction("mov rax, QWORD PTR [rsi]");                            // read the concrete array representation tag
    emitter.instruction("mov rdi, QWORD PTR [rsi + 8]");                        // load the opaque native array payload
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // retain the borrowed payload across helper calls
    emit_branch_if_null_container(emitter, "rdi", "r11", "__rt_mbstring_array_next_error");
    emitter.instruction("cmp rax, 4");                                          // recognize an indexed array descriptor
    emitter.instruction("je __rt_mbstring_array_next_indexed");                 // read one typed indexed slot
    emitter.instruction("cmp rax, 5");                                          // recognize an associative array descriptor
    emitter.instruction("jne __rt_mbstring_array_next_error");                  // reject non-array roots without reading their payload
    emitter.instruction("mov rsi, QWORD PTR [rdx]");                            // resume the insertion-order hash cursor
    emitter.instruction("call __rt_hash_iter_next");                            // borrow the next key and per-entry typed value
    emitter.instruction("cmp rax, -1");                                         // recognize the hash iterator end marker
    emitter.instruction("je __rt_mbstring_array_next_end");                     // publish no entry when the hash walk ends
    emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                        // recover the caller-owned cursor pointer
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the next opaque hash cursor
    emitter.instruction("mov QWORD PTR [rsp + 32], r9");                        // save the entry tag while writing its key
    emitter.instruction("mov QWORD PTR [rsp + 40], rcx");                       // save the borrowed value low word
    emitter.instruction("mov QWORD PTR [rsp + 48], r8");                        // save the borrowed value high word
    emitter.instruction("mov eax, 1");                                          // default to a string key descriptor
    emitter.instruction("cmp rdx, -1");                                         // integer keys use a minus-one length sentinel
    emitter.instruction("jne __rt_mbstring_array_next_hash_key");               // preserve string keys without numeric coercion
    emitter.instruction("xor eax, eax");                                        // select an integer key descriptor
    emitter.instruction("xor edx, edx");                                        // integer keys have no high payload word
    emitter.label("__rt_mbstring_array_next_hash_key");
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // recover the key output descriptor
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the exact key tag
    emitter.instruction("mov QWORD PTR [r10 + 8], rdi");                        // publish the integer value or borrowed key bytes
    emitter.instruction("mov QWORD PTR [r10 + 16], rdx");                       // publish the binary key length
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // restore the concrete or Mixed value tag
    emitter.instruction("mov rdi, QWORD PTR [rsp + 40]");                       // restore the borrowed value low word
    emitter.instruction("mov rdx, QWORD PTR [rsp + 48]");                       // restore the borrowed value high word
    emitter.instruction("jmp __rt_mbstring_array_next_value");                  // normalize the value before returning it
    emitter.label("__rt_mbstring_array_next_indexed");
    emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                        // recover the cursor pointer
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // read the next logical indexed position
    emitter.instruction("cmp r11, QWORD PTR [rdi]");                            // check the live bounds before loading a slot
    emitter.instruction("jae __rt_mbstring_array_next_end");                    // finish without reading beyond the indexed payload
    emitter.instruction("mov rax, QWORD PTR [rdi - 8]");                        // read the indexed element type from heap metadata
    emitter.instruction("shr rax, 8");                                          // move the element type into the low bits
    emitter.instruction("and rax, 0x7f");                                       // remove the persistent COW flag
    emitter.instruction("mov r8, 8");                                           // ordinary scalar and pointer slots occupy one word
    emitter.instruction("cmp rax, 1");                                          // strings occupy a pointer and length pair
    emitter.instruction("je __rt_mbstring_array_next_wide");                    // select a two-word string slot
    emitter.instruction(&format!("cmp rax, {}", TAGGED_SCALAR_ARRAY_VALUE_TYPE)); // nullable scalar slots contain a payload and runtime tag
    emitter.instruction("jne __rt_mbstring_array_next_stride");                 // retain one-word storage for other types
    emitter.label("__rt_mbstring_array_next_wide");
    emitter.instruction("mov r8, 16");                                          // select the two-word physical slot size
    emitter.label("__rt_mbstring_array_next_stride");
    emitter.instruction("cmp r8, QWORD PTR [rdi + 16]");                        // require type metadata and slot width to agree
    emitter.instruction("jne __rt_mbstring_array_next_error");                  // reject malformed storage before reading an element
    emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");                       // recover the key output descriptor
    emitter.instruction("mov QWORD PTR [rcx], 0");                              // publish the integer key tag
    emitter.instruction("mov QWORD PTR [rcx + 8], r11");                        // publish the logical index without allocating a key cell
    emitter.instruction("mov QWORD PTR [rcx + 16], 0");                         // clear the unused key high word
    emitter.instruction("imul r8, r11");                                        // scale the index by the validated physical slot width
    emitter.instruction("lea r8, [rdi + r8 + 24]");                             // address the indexed slot after its header
    emitter.instruction("inc r11");                                             // advance to the next logical position
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the next indexed cursor
    emitter.instruction("mov rdi, QWORD PTR [r8]");                             // borrow the low payload word
    emitter.instruction("xor edx, edx");                                        // clear high data for ordinary one-word slots
    emitter.instruction("cmp rax, 1");                                          // check for a string pointer and length pair
    emitter.instruction("je __rt_mbstring_array_next_string_slot");             // load the complete binary string descriptor
    emitter.instruction(&format!("cmp rax, {}", TAGGED_SCALAR_ARRAY_VALUE_TYPE)); // check for a nullable scalar tag/payload pair
    emitter.instruction("jne __rt_mbstring_array_next_value");                  // normalize the ordinary slot representation
    emitter.instruction("mov rax, QWORD PTR [r8 + 8]");                         // use the per-slot scalar or null tag
    emitter.instruction("jmp __rt_mbstring_array_next_value");                  // preserve scalar payload bits without evaluating them
    emitter.label("__rt_mbstring_array_next_string_slot");
    emitter.instruction("mov rdx, QWORD PTR [r8 + 8]");                         // borrow the exact string byte length
    emitter.label("__rt_mbstring_array_next_value");
    emitter.instruction("cmp QWORD PTR [rsp + 56], 0");                         // preserve the original boxed cell for owned entry copies
    emitter.instruction("jne __rt_mbstring_array_next_store");                  // resource and reference identities require the shared value-copy helper
    emitter.instruction("cmp rax, 7");                                          // Mixed slots point to a boxed runtime cell
    emitter.instruction("jne __rt_mbstring_array_next_concrete");               // already-concrete slots need no unboxing
    emitter.instruction("mov rax, rdi");                                        // pass the borrowed Mixed cell to the shared unboxer
    emitter.instruction("call __rt_mixed_unbox");                               // peel nested wrappers and normalize container nulls
    emitter.label("__rt_mbstring_array_next_concrete");
    emitter.instruction("cmp rax, 4");                                          // only container tags can hold the null sentinel
    emitter.instruction("jb __rt_mbstring_array_next_store");                   // preserve integers, strings, floats, and booleans
    emitter.instruction("cmp rax, 6");                                          // arrays and objects occupy the nullable container tag range
    emitter.instruction("ja __rt_mbstring_array_next_other");                   // classify null and unsupported non-container tags
    emit_branch_if_null_container(emitter, "rdi", "r11", "__rt_mbstring_array_next_null");
    emitter.instruction("cmp rax, 6");                                          // objects are unsupported by recursive mbstring operations
    emitter.instruction("jb __rt_mbstring_array_next_store");                   // preserve indexed and associative array identity
    emitter.label("__rt_mbstring_array_next_other");
    emitter.instruction("cmp rax, 8");                                          // retain canonical PHP null values
    emitter.instruction("je __rt_mbstring_array_next_null");                    // clear unused null payload words
    emitter.instruction("mov eax, 6");                                          // represent objects, resources, and other unsupported host values
    emitter.instruction("xor edi, edi");                                        // do not export unsupported host payload ownership
    emitter.instruction("xor edx, edx");                                        // clear unused unsupported high data
    emitter.instruction("jmp __rt_mbstring_array_next_store");                  // publish the explicit unsupported shape
    emitter.label("__rt_mbstring_array_next_null");
    emitter.instruction("mov eax, 8");                                          // select the canonical null descriptor
    emitter.instruction("xor edi, edi");                                        // clear the null low word
    emitter.instruction("xor edx, edx");                                        // clear the null high word
    emitter.label("__rt_mbstring_array_next_store");
    emitter.instruction("mov r10, QWORD PTR [rsp + 24]");                       // recover the value output descriptor
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the concrete value tag
    emitter.instruction("mov QWORD PTR [r10 + 8], rdi");                        // publish its low payload word
    emitter.instruction("mov QWORD PTR [r10 + 16], rdx");                       // publish its borrowed string length or high word
    emitter.instruction("mov eax, 1");                                          // report one complete borrowed entry
    emitter.instruction("jmp __rt_mbstring_array_next_return");                 // restore the C caller after success
    emitter.label("__rt_mbstring_array_next_end");
    emitter.instruction("xor eax, eax");                                        // report the end of the ordered array walk
    emitter.instruction("jmp __rt_mbstring_array_next_return");                 // restore the C caller without publishing values
    emitter.label("__rt_mbstring_array_next_error");
    emitter.instruction("mov eax, 2");                                          // report malformed host metadata without a PHP value
    emitter.label("__rt_mbstring_array_next_return");
    emitter.instruction("mov rsp, rbp");                                        // release reader state without changing source ownership
    emitter.instruction("pop rbp");                                             // restore the C caller frame pointer
    emitter.instruction("ret");                                                 // return the reader status across the C ABI
}

/// Shares one reader implementation between graph snapshots and owned element coercion.
fn emit_entry_points(emitter: &mut Emitter) {
    for (name, raw) in [("__rt_mbstring_array_next", 0), ("__rt_mbstring_array_next_raw", 1)] {
        emitter.label_global(name);
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_load_int_immediate(emitter, "x5", raw);
                emitter.instruction("b __rt_mbstring_array_next_impl");         // enter the common reader with the explicit normalization mode
            },
            Arch::X86_64 => {
                abi::emit_load_int_immediate(emitter, "r9", raw);
                emitter.instruction("jmp __rt_mbstring_array_next_impl");       // share all storage and cursor logic with the snapshot reader
            },
        }
    }
    emitter.label_global("__rt_mbstring_array_next_impl");
}
