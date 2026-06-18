//! Purpose:
//! Emits the `__rt_array_push_str`, `__rt_array_ensure_unique` runtime helper assembly for array push str.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_array_push_str` runtime helper for appending a string element to a PHP array.
/// Input:  x0 = array pointer, x1 = string ptr, x2 = string len (ARM64 convention).
/// Output: x0 = array pointer (may differ if the array was reallocated during growth).
///
/// Algorithm:
/// 1. Calls `__rt_array_ensure_unique` to split the array if it is shared (COW).
/// 2. On first append to an empty array, stamps the array header with elem_size=16 and
///    value_type=string so future growth correctly copies 16-byte string slots. An empty
///    `array<never>` literal is allocated with 8-byte slots, so the same step also rescales
///    the capacity field (`old_capacity * old_elem_size / 16`) to count the existing backing
///    store in 16-byte slots — otherwise the 16-byte first writes would overflow a buffer
///    sized for 8-byte slots before the capacity check ever triggers a grow.
/// 3. Persists the incoming string to heap via `__rt_str_persist` (safety: the caller's
///    string may point into the volatile concat_buf).
/// 4. Checks capacity; if full, calls `__rt_array_grow` to double capacity and retry.
/// 5. Stores the (ptr, len) pair into the next 16-byte slot, advances length, returns.
pub fn emit_array_push_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_push_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_push_str ---");
    emitter.label_global("__rt_array_push_str");

    // -- set up stack frame (needed for str_persist and potential growth) --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the incoming string ptr/len across ensure_unique
    emitter.instruction("bl __rt_array_ensure_unique");                         // split shared arrays before persisting/appending a new string slot
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique array pointer

    // -- specialize freshly empty arrays to 16-byte string slots --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length before first-write specialization
    emitter.instruction("cbnz x9, __rt_array_push_str_shape_ready");            // existing arrays already have their element shape fixed
    emitter.instruction("ldr x10, [x0, #16]");                                  // x10 = old elem_size (8 for empty array<never> buffers, 16 for string buffers)
    emitter.instruction("ldr x11, [x0, #8]");                                   // x11 = old capacity counted in old-elem_size slots
    emitter.instruction("mul x11, x11, x10");                                   // x11 = backing-store data bytes already reserved by __rt_array_new
    emitter.instruction("lsr x11, x11, #4");                                    // reinterpret the same bytes as 16-byte string slots so capacity matches the buffer
    emitter.instruction("str x11, [x0, #8]");                                   // publish slot-accurate capacity so an 8-byte-sized buffer grows before a 16-byte slot overflows it
    emitter.instruction("mov x10, #16");                                        // string append slots carry pointer and length
    emitter.instruction("str x10, [x0, #16]");                                  // elem_size = 16 before any future grow copies live string slots
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load packed array metadata for value_type stamping
    emitter.instruction("mov x11, #0x80ff");                                    // keep indexed-array kind and persistent COW metadata only
    emitter.instruction("and x10, x10, x11");                                   // clear any stale first-write value_type tag
    emitter.instruction("mov x11, #1");                                         // value_type 1 = string payload slots
    emitter.instruction("lsl x11, x11, #8");                                    // move string value_type into the packed kind-word byte lane
    emitter.instruction("orr x10, x10, x11");                                   // combine stable metadata with the string value_type tag
    emitter.instruction("str x10, [x0, #-8]");                                  // publish string metadata before the first append
    emitter.label("__rt_array_push_str_shape_ready");

    // -- persist string to heap before pushing --
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // restore the incoming string ptr/len after ensure_unique
    emitter.instruction("bl __rt_str_persist");                                 // copy string to heap, x1=heap_ptr, x2=len
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save persisted string ptr and len

    // -- check capacity before pushing --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = array capacity
    emitter.instruction("cmp x9, x10");                                         // is the array full?
    emitter.instruction("b.ge __rt_array_push_str_grow");                       // grow array if at capacity

    // -- push directly --
    emitter.label("__rt_array_push_str_push");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x9, [x0]");                                        // reload length
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // reload persisted string ptr and len
    emitter.instruction("lsl x10, x9, #4");                                     // x10 = length * 16 (byte offset)
    emitter.instruction("add x10, x0, x10");                                    // x10 = array base + byte offset
    emitter.instruction("add x10, x10, #24");                                   // x10 = skip header to data region
    emitter.instruction("str x1, [x10]");                                       // store string pointer at slot[0..8]
    emitter.instruction("str x2, [x10, #8]");                                   // store string length at slot[8..16]
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // write updated length back to header

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return (x0 = array pointer, unchanged)

    // -- slow path: grow array then push --
    emitter.label("__rt_array_push_str_grow");
    emitter.instruction("bl __rt_array_grow");                                  // double array capacity → x0 = new array
    emitter.instruction("str x0, [sp, #0]");                                    // update saved array pointer
    emitter.instruction("b __rt_array_push_str_push");                          // go push into the grown array
}

/// Emits the x86_64 Linux variant of the array push string runtime helper.
/// Uses System V AMD64 ABI: rdi = array ptr, rsi = str ptr, rdx = str len.
/// Returns updated array pointer in rax.
/// Shares the same algorithm as the ARM64 variant: persist string to heap, first-write
/// shape specialization for empty arrays, capacity check, push or grow, then return.
fn emit_array_push_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_str ---");
    emitter.label_global("__rt_array_push_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving string-append spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved array pointer and string payload
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the array pointer plus the incoming string ptr/len pair
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the incoming string pointer across uniqueness and persistence helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the incoming string length across uniqueness and persistence helper calls
    emitter.instruction("call __rt_array_ensure_unique");                       // split shared indexed arrays before appending a new owned string slot
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the unique indexed-array pointer across string persistence and optional growth
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load length before first-write string shape specialization
    emitter.instruction("test r10, r10");                                       // is this the first append into a freshly empty indexed array?
    emitter.instruction("jnz __rt_array_push_str_shape_ready");                 // existing arrays already have their element shape fixed
    emitter.instruction("mov r10, QWORD PTR [rax + 16]");                       // r10 = old elem_size (8 for empty array<never> buffers, 16 for string buffers)
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // r11 = old capacity counted in old-elem_size slots
    emitter.instruction("imul r11, r10");                                       // r11 = backing-store data bytes already reserved by __rt_array_new
    emitter.instruction("shr r11, 4");                                          // reinterpret the same bytes as 16-byte string slots so capacity matches the buffer
    emitter.instruction("mov QWORD PTR [rax + 8], r11");                        // publish slot-accurate capacity so an 8-byte-sized buffer grows before a 16-byte slot overflows it
    emitter.instruction("mov QWORD PTR [rax + 16], 16");                        // elem_size = 16 before any future growth copies live string slots
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load packed indexed-array metadata for value_type stamping
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve heap marker, indexed-array kind, and persistent COW metadata
    emitter.instruction("and r10, r11");                                        // clear any stale first-write value_type tag
    emitter.instruction("or r10, 0x100");                                       // value_type 1 = string payload slots
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // publish string metadata before the first append
    emitter.label("__rt_array_push_str_shape_ready");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // move the incoming string pointer into the x86_64 string-persist input register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // move the incoming string length into the x86_64 string-persist length register
    emitter.instruction("call __rt_str_persist");                               // duplicate the appended string into owned heap storage before storing it in the indexed array
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the owned string pointer returned by the string-persist helper
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the owned string length returned by the string-persist helper
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the unique indexed-array pointer after string persistence
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the indexed-array logical length before checking append capacity
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the indexed-array capacity before deciding between the fast path and growth
    emitter.instruction("cmp r11, rcx");                                        // is the indexed array already full at the current logical length?
    emitter.instruction("jae __rt_array_push_str_grow");                        // grow the indexed array when the appended owned string would exceed the current capacity
    emitter.label("__rt_array_push_str_store");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current indexed-array pointer before storing the owned string slot
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // reload the indexed-array logical length after helper calls clobbered caller-saved registers
    emitter.instruction("mov rcx, r11");                                        // copy the logical length before scaling it into a 16-byte string-slot offset
    emitter.instruction("shl rcx, 4");                                          // convert the logical length into the byte offset of the next 16-byte string slot
    emitter.instruction("lea rcx, [r10 + rcx + 24]");                           // compute the destination address of the next indexed-array string slot
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the owned string pointer that should be stored in the appended slot
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the owned string length that should be stored in the appended slot
    emitter.instruction("mov QWORD PTR [rcx], r8");                             // store the owned string pointer in the appended indexed-array string slot
    emitter.instruction("mov QWORD PTR [rcx + 8], r9");                         // store the owned string length in the appended indexed-array string slot
    emitter.instruction("add r11, 1");                                          // advance the indexed-array logical length after materializing the appended string slot
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the updated indexed-array logical length in the array header
    emitter.instruction("mov rax, r10");                                        // return the updated indexed-array pointer in the x86_64 integer result register
    emitter.instruction("add rsp, 32");                                         // release the string-append spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the updated indexed array
    emitter.instruction("ret");                                                 // return to the caller with rax holding the updated indexed-array pointer
    emitter.label("__rt_array_push_str_grow");
    emitter.instruction("mov rdi, r10");                                        // pass the unique indexed-array pointer to the growth helper before storing the owned string slot
    emitter.instruction("call __rt_array_grow");                                // allocate a larger indexed-array backing store so the string append can proceed
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the grown indexed-array pointer before storing the owned string slot
    emitter.instruction("jmp __rt_array_push_str_store");                       // append the owned string payload into the grown indexed-array storage
}
