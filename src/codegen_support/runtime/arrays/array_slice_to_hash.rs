//! Purpose:
//! Emits the `__rt_array_slice_to_hash` runtime helper backing `array_slice($a, $o, $l, true)`.
//! Copies the PHP slice window out of an indexed array into an owned hash that keeps each
//! element's ORIGINAL integer key instead of renumbering it from zero.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The `$offset`/`$length` window is normalized by the shared `emit_slice_bounds` prologue, the
//!   single source of truth for PHP's slice arithmetic, so the key-preserving form cannot drift
//!   from `__rt_array_slice` / `__rt_array_slice_refcounted` — negative offsets, negative lengths
//!   and out-of-range clamps behave identically and the copied window always stays inside the
//!   source payload.
//! - The element extraction mirrors `__rt_array_to_hash` slot for slot: string elements
//!   (16-byte slots) are persisted into independent heap copies, heap-backed elements are
//!   retained, and scalar elements are copied by value, so the result owns its payloads.
//! - PHP's `preserve_keys` result is key-identical to the sliced source region, which is exactly
//!   what a hash records and elephc's dense indexed array cannot represent.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::arrays::slice_bounds::emit_slice_bounds;

/// array_slice_to_hash: build an owned hash {start: e(start), …} from an indexed array window.
/// Input:  x0 = indexed array pointer, x1 = raw `$offset`, x2 = raw `$length`,
///         x3 = 1 when the caller passed a `$length` and 0 when it was omitted or `null`
/// Output: x0 = new owned hash carrying the source integer keys of the selected window
///
/// Backs `array_slice($array, $offset, $length, preserve_keys: true)`.
pub fn emit_array_slice_to_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_slice_to_hash_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_slice_to_hash ---");
    emitter.label_global("__rt_array_slice_to_hash");
    emitter.instruction("sub sp, sp, #80");                                     // allocate the conversion stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up the new frame pointer
    emit_slice_bounds(emitter, "__rt_array_slice_to_hash");
    emitter.instruction("str x0, [sp, #0]");                                    // save the indexed array pointer
    emitter.instruction("str x1, [sp, #16]");                                   // cursor i = normalized window start
    emitter.instruction("add x9, x1, x2");                                      // end = start + clamped window length
    emitter.instruction("str x9, [sp, #24]");                                   // save the exclusive window end
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the uniform heap-kind header word
    emitter.instruction("lsr x10, x10, #8");                                    // shift the packed value_type into the low bits
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate the indexed-array value_type (also the Mixed tag)
    emitter.instruction("str x10, [sp, #32]");                                  // save the value_type / runtime tag
    emitter.instruction("ldr x11, [x0, #16]");                                  // load the element size (stride) from the header
    emitter.instruction("str x11, [sp, #40]");                                  // save the element stride
    emitter.instruction("mov x1, x10");                                         // value_type for the new hash header
    emitter.instruction("cmp x2, #8");                                          // is the window below the minimum hash capacity?
    emitter.instruction("b.ge __rt_array_slice_to_hash_cap_ok");                // use the window length as the capacity hint
    emitter.instruction("mov x2, #8");                                          // clamp the capacity hint to a small minimum
    emitter.label("__rt_array_slice_to_hash_cap_ok");
    emitter.instruction("mov x0, x2");                                          // capacity hint for the new hash
    emitter.instruction("bl __rt_hash_new");                                    // allocate the result hash, x0 = result
    emitter.instruction("str x0, [sp, #8]");                                    // save the result hash pointer
    emitter.label("__rt_array_slice_to_hash_loop");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the window cursor
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the exclusive window end
    emitter.instruction("cmp x10, x9");                                         // has the cursor reached the end of the window?
    emitter.instruction("b.ge __rt_array_slice_to_hash_done");                  // the whole window has been copied
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the indexed array pointer
    emitter.instruction("add x11, x11, #24");                                   // skip the 24-byte indexed-array header
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the element stride
    emitter.instruction("mul x13, x10, x12");                                   // byte offset of element[i]
    emitter.instruction("add x11, x11, x13");                                   // x11 = address of element[i]
    emitter.instruction("ldr x3, [x11]");                                       // load the element low word
    emitter.instruction("str x3, [sp, #48]");                                   // save the element low word
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the value_type
    emitter.instruction("cmp x9, #1");                                          // is the element a string?
    emitter.instruction("b.eq __rt_array_slice_to_hash_string");                // strings need persistence
    emitter.instruction("mov x9, #0");                                          // non-string elements have no high word
    emitter.instruction("str x9, [sp, #56]");                                   // save a zero high word
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the value_type
    emitter.instruction("cmp x9, #4");                                          // is the element below the heap-backed tag range?
    emitter.instruction("b.lt __rt_array_slice_to_hash_set");                   // scalar elements need no retain
    emitter.instruction("cmp x9, #7");                                          // is the element above the heap-backed tag range?
    emitter.instruction("b.gt __rt_array_slice_to_hash_set");                   // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #48]");                                   // load the heap-backed element pointer
    emitter.instruction("bl __rt_incref");                                      // retain the heap-backed element for the result hash
    emitter.instruction("b __rt_array_slice_to_hash_set");                      // continue to insertion
    emitter.label("__rt_array_slice_to_hash_string");
    emitter.instruction("ldr x2, [x11, #8]");                                   // load the string length from the 16-byte slot
    emitter.instruction("ldr x1, [sp, #48]");                                   // load the string pointer
    emitter.instruction("bl __rt_str_persist");                                 // copy the string into an independent heap block, x1 = new pointer
    emitter.instruction("str x1, [sp, #48]");                                   // save the persisted string pointer
    emitter.instruction("str x2, [sp, #56]");                                   // save the string length
    emitter.label("__rt_array_slice_to_hash_set");
    emitter.instruction("ldr x0, [sp, #8]");                                    // result hash pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // integer key = the preserved source index i
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("ldr x3, [sp, #48]");                                   // value low word
    emitter.instruction("ldr x4, [sp, #56]");                                   // value high word
    emitter.instruction("ldr x5, [sp, #32]");                                   // value runtime tag (= value_type)
    emitter.instruction("bl __rt_hash_set");                                    // insert element[i] at its preserved integer key
    emitter.instruction("str x0, [sp, #8]");                                    // update the result pointer after possible reallocation
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the window cursor
    emitter.instruction("add x10, x10, #1");                                    // advance to the next element of the window
    emitter.instruction("str x10, [sp, #16]");                                  // save the advanced cursor
    emitter.instruction("b __rt_array_slice_to_hash_loop");                     // continue copying the window
    emitter.label("__rt_array_slice_to_hash_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the result hash in x0
}

/// x86_64 Linux implementation of `__rt_array_slice_to_hash`.
/// Input:  rdi = indexed array pointer, rsi = raw `$offset`, rdx = raw `$length`,
///         rcx = 1 when the caller passed a `$length` and 0 when it was omitted or `null`
/// Output: rax = new owned hash carrying the source integer keys of the selected window
fn emit_array_slice_to_hash_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_slice_to_hash ---");
    emitter.label_global("__rt_array_slice_to_hash");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 80");                                         // reserve local slots for the conversion loop state
    emit_slice_bounds(emitter, "__rt_array_slice_to_hash");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the indexed array pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rsi");                       // cursor i = normalized window start
    emitter.instruction("mov rax, rsi");                                        // seed the window-end scratch from the window start
    emitter.instruction("add rax, rdx");                                        // end = start + clamped window length
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the exclusive window end
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the uniform heap-kind header word
    emitter.instruction("shr r10, 8");                                          // shift the packed value_type into the low bits
    emitter.instruction("and r10, 127");                                        // isolate the indexed-array value_type (also the Mixed tag)
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the value_type / runtime tag
    emitter.instruction("mov r11, QWORD PTR [rdi + 16]");                       // load the element size (stride) from the header
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the element stride
    emitter.instruction("mov rsi, r10");                                        // value_type for the new hash header
    emitter.instruction("mov rdi, rdx");                                        // capacity hint = clamped window length
    emitter.instruction("cmp rdi, 8");                                          // is the window below the minimum hash capacity?
    emitter.instruction("jge __rt_array_slice_to_hash_cap_ok");                 // use the window length as the capacity hint
    emitter.instruction("mov rdi, 8");                                          // clamp the capacity hint to a small minimum
    emitter.label("__rt_array_slice_to_hash_cap_ok");
    emitter.instruction("call __rt_hash_new");                                  // allocate the result hash, rax = result
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the result hash pointer
    emitter.label("__rt_array_slice_to_hash_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the window cursor
    emitter.instruction("cmp rax, QWORD PTR [rbp - 72]");                       // has the cursor reached the end of the window?
    emitter.instruction("jge __rt_array_slice_to_hash_done");                   // the whole window has been copied
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the indexed array pointer
    emitter.instruction("add r10, 24");                                         // skip the 24-byte indexed-array header
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the element stride
    emitter.instruction("imul r11, rax");                                       // byte offset of element[i]
    emitter.instruction("add r10, r11");                                        // r10 = address of element[i]
    emitter.instruction("mov rcx, QWORD PTR [r10]");                            // load the element low word
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save the element low word
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the value_type
    emitter.instruction("cmp r9, 1");                                           // is the element a string?
    emitter.instruction("je __rt_array_slice_to_hash_string");                  // strings need persistence
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // non-string elements have no high word
    emitter.instruction("cmp r9, 4");                                           // is the element below the heap-backed tag range?
    emitter.instruction("jl __rt_array_slice_to_hash_set");                     // scalar elements need no retain
    emitter.instruction("cmp r9, 7");                                           // is the element above the heap-backed tag range?
    emitter.instruction("jg __rt_array_slice_to_hash_set");                     // non-heap tags need no retain
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // load the heap-backed element pointer where __rt_incref reads it on x86_64
    emitter.instruction("call __rt_incref");                                    // retain the heap-backed element for the result hash
    emitter.instruction("jmp __rt_array_slice_to_hash_set");                    // continue to insertion
    emitter.label("__rt_array_slice_to_hash_string");
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the string length from the 16-byte slot
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // load the string pointer
    emitter.instruction("call __rt_str_persist");                               // copy the string into an independent heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the persisted string pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the string length
    emitter.label("__rt_array_slice_to_hash_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // integer key = the preserved source index i
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // value runtime tag (= value_type)
    emitter.instruction("call __rt_hash_set");                                  // insert element[i] at its preserved integer key
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // update the result pointer after possible reallocation
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the window cursor
    emitter.instruction("add rax, 1");                                          // advance to the next element of the window
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the advanced cursor
    emitter.instruction("jmp __rt_array_slice_to_hash_loop");                   // continue copying the window
    emitter.label("__rt_array_slice_to_hash_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // rax = result hash pointer
    emitter.instruction("add rsp, 80");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result hash in rax
}
