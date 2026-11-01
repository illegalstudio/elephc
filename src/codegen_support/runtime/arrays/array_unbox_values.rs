//! Purpose:
//! Emits the `__rt_array_unbox_values` runtime helper, which narrows the boxed-Mixed element
//! cells of an indexed array or hash to the concrete element representation a typed
//! destination (`array<Foo>`) reads.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - The EIR `runtime_call` conversion from a boxed `mixed` value to a typed array, right after
//!   the outer Mixed cell has been unboxed.
//!
//! Key details:
//! - A Mixed cell can carry an array whose elements are boxed Mixed cells (value_type 7) while the
//!   checker proved the elements are, say, objects: `array_map()` over an untyped property keeps
//!   a Mixed result slot and widens its entries. Reading such an array as `array<Foo>` treats each
//!   box pointer as the object, so the payload must be moved out of the box first.
//! - Only containers stamped value_type 7 are rewritten, after a copy-on-write split. Each box is
//!   released after its container payload (tags 4..6) gained its own reference. An indexed array
//!   is rewritten in place only for pointer-sized destinations: string slots are 16 bytes wide.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_array_unbox_values` for the active target.
///
/// A Mixed destination (tag 7) keeps boxed entries boxed and boxes concrete ones while packing,
/// so the call boundary can hand a list-shaped hash to an `array<mixed>` parameter as a list.
///
/// Input:  x0 / rdi = owned indexed-array or hash pointer, x1 / rsi = destination value tag,
///         x2 / rdx = 1 when the destination is a list (`array<T>`), 0 for an associative one.
/// Output: x0 / rax = owned container whose Mixed cells now hold the concrete payloads; a
///         list-shaped hash headed for a list destination comes back packed.
pub fn emit_array_unbox_values(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_unbox_values_x86_64(emitter);
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: array_unbox_values ---");
    emitter.label_global("__rt_array_unbox_values");
    emitter.instruction("sub sp, sp, #96");                                     // reserve container, tag, length, cursor, entry and list slots
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the narrowing frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the owned container pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the destination value tag
    emitter.instruction("str x2, [sp, #40]");                                   // save whether the destination is a list
    emitter.instruction("bl __rt_heap_kind");                                   // read the container's heap kind
    emitter.instruction("cmp x0, #3");                                          // heap kind 3 = hash
    emitter.instruction("b.eq __rt_array_unbox_values_hash");                   // hashes carry a per-entry tag
    emitter.instruction("cmp x0, #2");                                          // heap kind 2 = indexed array
    emitter.instruction("b.ne __rt_array_unbox_values_done");                   // anything else has no element cells to narrow
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the destination value tag
    emitter.instruction("cmp x9, #1");                                          // string slots are 16 bytes wide
    emitter.instruction("b.eq __rt_array_unbox_values_done");                   // they cannot be rewritten in place
    emitter.instruction("cmp x9, #7");                                          // a Mixed destination reads any packed list
    emitter.instruction("b.eq __rt_array_unbox_values_done");                   // nothing to narrow
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the indexed array
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the packed indexed-array kind word
    emitter.instruction("lsr x10, x10, #8");                                    // shift the runtime value_type into the low byte
    emitter.instruction("and x10, x10, #0x7f");                                 // drop the copy-on-write bit
    emitter.instruction("cmp x10, #7");                                         // are the slots boxed Mixed cells?
    emitter.instruction("b.ne __rt_array_unbox_values_done");                   // concrete slots already match the destination
    emitter.instruction("bl __rt_array_ensure_unique");                         // split a shared array before rewriting its slots
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load the logical length
    emitter.instruction("str x9, [sp, #16]");                                   // keep it across the per-slot calls
    emitter.instruction("str xzr, [sp, #24]");                                  // start at slot zero

    emitter.label("__rt_array_unbox_values_list_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the slot index
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the logical length
    emitter.instruction("cmp x9, x10");                                         // every slot narrowed?
    emitter.instruction("b.ge __rt_array_unbox_values_list_stamp");             // then restamp the value_type
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the unique array pointer
    emitter.instruction("add x11, x11, #24");                                   // skip the indexed-array header
    emitter.instruction("ldr x0, [x11, x9, lsl #3]");                           // load the slot's Mixed cell
    emitter.instruction("bl __rt_array_unbox_values_cell");                     // move its payload out and release the cell
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the slot index
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the unique array pointer
    emitter.instruction("add x11, x11, #24");                                   // skip the indexed-array header
    emitter.instruction("str x0, [x11, x9, lsl #3]");                           // store the concrete payload in the slot
    emitter.instruction("add x9, x9, #1");                                      // advance to the next slot
    emitter.instruction("str x9, [sp, #24]");                                   // persist the slot index
    emitter.instruction("b __rt_array_unbox_values_list_loop");                 // continue narrowing slots

    emitter.label("__rt_array_unbox_values_list_stamp");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the unique array pointer
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the packed kind word
    emitter.instruction("mov x11, #0x80ff");                                    // keep heap kind and persistent copy-on-write flag
    emitter.instruction("and x10, x10, x11");                                   // clear the Mixed value_type bits
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the destination value tag
    emitter.instruction("lsl x11, x11, #8");                                    // move it into the value_type byte
    emitter.instruction("orr x10, x10, x11");                                   // combine it with the stable metadata
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the concrete value_type
    emitter.instruction("b __rt_array_unbox_values_done");                      // return the narrowed array

    emitter.label("__rt_array_unbox_values_hash");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the destination value tag
    emitter.instruction("cmp x9, #7");                                          // a Mixed destination keeps its boxed entries
    emitter.instruction("b.eq __rt_array_unbox_values_hash_list");              // it only needs the list check
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash
    emitter.instruction("ldr x9, [x0, #16]");                                   // load the hash header value_type
    emitter.instruction("cmp x9, #7");                                          // are its entries boxed Mixed cells?
    emitter.instruction("b.ne __rt_array_unbox_values_hash_list");              // concrete entries only need the list check
    emitter.instruction("bl __rt_hash_ensure_unique");                          // split a shared hash before rewriting its entries
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique hash pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // start the insertion-order cursor

    emitter.label("__rt_array_unbox_values_hash_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the unique hash pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next entry and its value address
    emitter.instruction("cmn x0, #1");                                          // end of the entries?
    emitter.instruction("b.eq __rt_array_unbox_values_hash_stamp");             // then restamp the header
    emitter.instruction("str x0, [sp, #24]");                                   // save the next cursor
    emitter.instruction("str x6, [sp, #32]");                                   // save the entry value address
    emitter.instruction("cmp x5, #7");                                          // is this entry a boxed Mixed cell?
    emitter.instruction("b.ne __rt_array_unbox_values_hash_loop");              // concrete entries stay as they are
    emitter.instruction("mov x0, x3");                                          // pass the entry's Mixed cell
    emitter.instruction("bl __rt_array_unbox_values_cell");                     // move its payload out and release the cell
    emitter.instruction("ldr x6, [sp, #32]");                                   // reload the entry value address
    emitter.instruction("stp x0, x1, [x6]");                                    // store the payload words in the entry
    emitter.instruction("str x2, [x6, #16]");                                   // store the payload's own runtime tag
    emitter.instruction("b __rt_array_unbox_values_hash_loop");                 // continue with the next entry

    emitter.label("__rt_array_unbox_values_hash_stamp");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the unique hash pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the destination value tag
    emitter.instruction("str x9, [x0, #16]");                                   // stamp the header with the concrete value_type

    // A list destination walks packed slots. A hash whose keys are 0..n-1 in order is the same
    // PHP value, so rebuild it packed; any other hash keeps its keys (and its hash storage).
    emitter.label("__rt_array_unbox_values_hash_list");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the list-destination flag
    emitter.instruction("cbz x9, __rt_array_unbox_values_done");                // an associative destination keeps the hash
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash
    emitter.instruction("bl __rt_array_is_list");                               // are its keys 0..n-1 in insertion order?
    emitter.instruction("cbz x0, __rt_array_unbox_values_done");                // keyed hashes stay hashes
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash
    emitter.instruction("ldr x0, [x0]");                                        // load its entry count
    emitter.instruction("str x0, [sp, #16]");                                   // keep the count
    emitter.instruction("mov x1, #8");                                          // pointer-sized slots
    emitter.instruction("bl __rt_array_new");                                   // allocate the packed array
    emitter.instruction("str x0, [sp, #48]");                                   // keep the packed array
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the entry count
    emitter.instruction("str x9, [x0]");                                        // every slot is filled below
    emitter.instruction("str xzr, [sp, #24]");                                  // start the insertion-order cursor
    emitter.instruction("str xzr, [sp, #56]");                                  // start at packed slot zero

    emitter.label("__rt_array_unbox_values_pack_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next entry
    emitter.instruction("cmn x0, #1");                                          // end of the entries?
    emitter.instruction("b.eq __rt_array_unbox_values_pack_done");              // then release the hash
    emitter.instruction("str x0, [sp, #24]");                                   // save the next cursor
    emitter.instruction("str x3, [sp, #32]");                                   // keep the entry payload
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the destination value tag
    emitter.instruction("cmp x9, #7");                                          // does the destination hold Mixed cells?
    emitter.instruction("b.ne __rt_array_unbox_values_pack_concrete");          // concrete destinations store the payload itself
    emitter.instruction("cmp x5, #7");                                          // is the entry already a Mixed cell?
    emitter.instruction("b.eq __rt_array_unbox_values_pack_retain_box");        // then the slot shares that cell
    emitter.instruction("mov x0, x5");                                          // pass the entry's runtime tag
    emitter.instruction("mov x1, x3");                                          // pass the entry's low payload word
    emitter.instruction("mov x2, x4");                                          // pass the entry's high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box a retained copy of the entry
    emitter.instruction("str x0, [sp, #32]");                                   // the fresh cell is the slot's payload
    emitter.instruction("b __rt_array_unbox_values_pack_store");                // store it
    emitter.label("__rt_array_unbox_values_pack_retain_box");
    emitter.instruction("mov x0, x3");                                          // pass the entry's Mixed cell
    emitter.instruction("bl __rt_incref");                                      // the slot shares the cell with the hash until the hash goes
    emitter.instruction("b __rt_array_unbox_values_pack_store");                // store it
    emitter.label("__rt_array_unbox_values_pack_concrete");
    emitter.instruction("cmp x5, #4");                                          // containers and objects occupy tags 4 through 6
    emitter.instruction("b.lt __rt_array_unbox_values_pack_store");             // scalars carry no reference
    emitter.instruction("cmp x5, #6");                                          // above 6 is not a container payload
    emitter.instruction("b.gt __rt_array_unbox_values_pack_store");             // nothing to retain
    emitter.instruction("mov x0, x3");                                          // pass the container payload
    emitter.instruction("bl __rt_incref");                                      // the packed slot shares it with the hash until the hash goes
    emitter.label("__rt_array_unbox_values_pack_store");
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the packed slot index
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the packed array
    emitter.instruction("add x11, x11, #24");                                   // skip the indexed-array header
    emitter.instruction("ldr x3, [sp, #32]");                                   // reload the entry payload
    emitter.instruction("str x3, [x11, x9, lsl #3]");                           // store it in the packed slot
    emitter.instruction("add x9, x9, #1");                                      // advance to the next slot
    emitter.instruction("str x9, [sp, #56]");                                   // persist the slot index
    emitter.instruction("b __rt_array_unbox_values_pack_loop");                 // continue with the next entry

    emitter.label("__rt_array_unbox_values_pack_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash
    emitter.instruction("bl __rt_decref_hash");                                 // the packed array replaces this reference
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the packed array
    emitter.instruction("str x0, [sp, #0]");                                    // it is the container returned from here on
    emitter.instruction("b __rt_array_unbox_values_list_stamp");                // stamp its concrete value_type

    emitter.label("__rt_array_unbox_values_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the owned container
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the narrowing frame
    emitter.instruction("ret");                                                 // return to the conversion site

    // x0 = Mixed cell -> x0/x1 = payload words (container payloads gain a reference), x2 = tag.
    emitter.label("__rt_array_unbox_values_cell");
    emitter.instruction("cbz x0, __rt_array_unbox_values_cell_null");           // a missing cell reads as null
    emitter.instruction("sub sp, sp, #48");                                     // reserve cell, tag and payload slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the cell frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the Mixed cell
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed runtime tag
    emitter.instruction("str x9, [sp, #8]");                                    // keep the tag
    emitter.instruction("ldp x10, x11, [x0, #8]");                              // load the boxed payload words
    emitter.instruction("stp x10, x11, [sp, #16]");                             // keep the payload words
    emitter.instruction("cmp x9, #4");                                          // containers and objects occupy tags 4 through 6
    emitter.instruction("b.lt __rt_array_unbox_values_cell_release");           // scalars carry no reference
    emitter.instruction("cmp x9, #6");                                          // above 6 is not a container payload
    emitter.instruction("b.gt __rt_array_unbox_values_cell_release");           // nothing to retain
    emitter.instruction("mov x0, x10");                                         // pass the container payload
    emitter.instruction("bl __rt_incref");                                      // the slot takes its own reference before the cell goes
    emitter.label("__rt_array_unbox_values_cell_release");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Mixed cell
    emitter.instruction("bl __rt_decref_mixed");                                // the slot no longer owns the cell
    emitter.instruction("ldp x0, x1, [sp, #16]");                               // return the payload words
    emitter.instruction("ldr x2, [sp, #8]");                                    // return the payload tag
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the cell frame
    emitter.instruction("ret");                                                 // return to the narrowing loop
    emitter.label("__rt_array_unbox_values_cell_null");
    emitter.instruction("mov x1, #0");                                          // null has no high payload word
    emitter.instruction("mov x2, #8");                                          // runtime tag 8 = null
    emitter.instruction("ret");                                                 // x0 already holds the null payload
}

/// Emits the x86_64 variant of `__rt_array_unbox_values`.
///
/// Input: rdi = owned container, rsi = destination value tag, rdx = list-destination flag.
/// Output: rax = owned container.
fn emit_array_unbox_values_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_unbox_values ---");
    emitter.label_global("__rt_array_unbox_values");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the narrowing frame
    emitter.instruction("sub rsp, 80");                                         // reserve container, tag, length, cursor, entry and list slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the owned container pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the destination value tag
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save whether the destination is a list
    emitter.instruction("mov rax, rdi");                                        // pass the container to the heap-kind probe
    emitter.instruction("call __rt_heap_kind");                                 // read the container's heap kind
    emitter.instruction("cmp eax, 3");                                          // heap kind 3 = hash
    emitter.instruction("je __rt_array_unbox_values_x86_hash");                 // hashes carry a per-entry tag
    emitter.instruction("cmp eax, 2");                                          // heap kind 2 = indexed array
    emitter.instruction("jne __rt_array_unbox_values_x86_done");                // anything else has no element cells to narrow
    emitter.instruction("cmp QWORD PTR [rbp - 16], 1");                         // string slots are 16 bytes wide
    emitter.instruction("je __rt_array_unbox_values_x86_done");                 // they cannot be rewritten in place
    emitter.instruction("cmp QWORD PTR [rbp - 16], 7");                         // a Mixed destination reads any packed list
    emitter.instruction("je __rt_array_unbox_values_x86_done");                 // nothing to narrow
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the indexed array
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("shr r10, 8");                                          // shift the runtime value_type into the low byte
    emitter.instruction("and r10, 0x7f");                                       // drop the copy-on-write bit
    emitter.instruction("cmp r10, 7");                                          // are the slots boxed Mixed cells?
    emitter.instruction("jne __rt_array_unbox_values_x86_done");                // concrete slots already match the destination
    emitter.instruction("call __rt_array_ensure_unique");                       // split a shared array before rewriting its slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the unique array pointer
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the logical length
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // keep it across the per-slot calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start at slot zero

    emitter.label("__rt_array_unbox_values_x86_list_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the slot index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // every slot narrowed?
    emitter.instruction("jge __rt_array_unbox_values_x86_list_stamp");          // then restamp the value_type
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the unique array pointer
    emitter.instruction("mov rax, QWORD PTR [r11 + 24 + r10 * 8]");             // load the slot's Mixed cell
    emitter.instruction("call __rt_array_unbox_values_x86_cell");               // move its payload out and release the cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the slot index
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the unique array pointer
    emitter.instruction("mov QWORD PTR [r11 + 24 + r10 * 8], rax");             // store the concrete payload in the slot
    emitter.instruction("add r10, 1");                                          // advance to the next slot
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the slot index
    emitter.instruction("jmp __rt_array_unbox_values_x86_list_loop");           // continue narrowing slots

    emitter.label("__rt_array_unbox_values_x86_list_stamp");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the unique array pointer
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed kind word
    emitter.instruction("mov r11, r10");                                        // copy it so the heap marker survives
    emitter.instruction("and r10, 0x80ff");                                     // keep heap kind and persistent copy-on-write flag
    emitter.instruction("and r11, -65536");                                     // keep the x86_64 heap marker bits
    emitter.instruction("or r10, r11");                                         // combine the stable metadata
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the destination value tag
    emitter.instruction("shl r11, 8");                                          // move it into the value_type byte
    emitter.instruction("or r10, r11");                                         // stamp the concrete value_type
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // persist the packed kind word
    emitter.instruction("jmp __rt_array_unbox_values_x86_done");                // return the narrowed array

    emitter.label("__rt_array_unbox_values_x86_hash");
    emitter.instruction("cmp QWORD PTR [rbp - 16], 7");                         // a Mixed destination keeps its boxed entries
    emitter.instruction("je __rt_array_unbox_values_x86_hash_list");            // it only needs the list check
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash
    emitter.instruction("cmp QWORD PTR [rdi + 16], 7");                         // are its entries boxed Mixed cells?
    emitter.instruction("jne __rt_array_unbox_values_x86_hash_list");           // concrete entries only need the list check
    emitter.instruction("call __rt_hash_ensure_unique");                        // split a shared hash before rewriting its entries
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the unique hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the insertion-order cursor

    emitter.label("__rt_array_unbox_values_x86_hash_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the unique hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next entry and its value address
    emitter.instruction("cmp rax, -1");                                         // end of the entries?
    emitter.instruction("je __rt_array_unbox_values_x86_hash_stamp");           // then restamp the header
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the next cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the entry value address
    emitter.instruction("cmp r9, 7");                                           // is this entry a boxed Mixed cell?
    emitter.instruction("jne __rt_array_unbox_values_x86_hash_loop");           // concrete entries stay as they are
    emitter.instruction("mov rax, rcx");                                        // pass the entry's Mixed cell
    emitter.instruction("call __rt_array_unbox_values_x86_cell");               // move its payload out and release the cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the entry value address
    emitter.instruction("mov QWORD PTR [r10], rax");                            // store the low payload word
    emitter.instruction("mov QWORD PTR [r10 + 8], rdx");                        // store the high payload word
    emitter.instruction("mov QWORD PTR [r10 + 16], rcx");                       // store the payload's own runtime tag
    emitter.instruction("jmp __rt_array_unbox_values_x86_hash_loop");           // continue with the next entry

    emitter.label("__rt_array_unbox_values_x86_hash_stamp");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the unique hash pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the destination value tag
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // stamp the header with the concrete value_type

    emitter.label("__rt_array_unbox_values_x86_hash_list");
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // is the destination a list?
    emitter.instruction("je __rt_array_unbox_values_x86_done");                 // an associative destination keeps the hash
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash
    emitter.instruction("call __rt_array_is_list");                             // are its keys 0..n-1 in insertion order?
    emitter.instruction("test rax, rax");                                       // list-shaped?
    emitter.instruction("jz __rt_array_unbox_values_x86_done");                 // keyed hashes stay hashes
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // load its entry count
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // keep the count
    emitter.instruction("mov rsi, 8");                                          // pointer-sized slots
    emitter.instruction("call __rt_array_new");                                 // allocate the packed array
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // keep the packed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the entry count
    emitter.instruction("mov QWORD PTR [rax], r10");                            // every slot is filled below
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the insertion-order cursor
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // start at packed slot zero

    emitter.label("__rt_array_unbox_values_x86_pack_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next entry
    emitter.instruction("cmp rax, -1");                                         // end of the entries?
    emitter.instruction("je __rt_array_unbox_values_x86_pack_done");            // then release the hash
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the next cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // keep the entry payload
    emitter.instruction("cmp QWORD PTR [rbp - 16], 7");                         // does the destination hold Mixed cells?
    emitter.instruction("jne __rt_array_unbox_values_x86_pack_concrete");       // concrete destinations store the payload itself
    emitter.instruction("cmp r9, 7");                                           // is the entry already a Mixed cell?
    emitter.instruction("je __rt_array_unbox_values_x86_pack_retain_box");      // then the slot shares that cell
    emitter.instruction("mov rax, r9");                                         // pass the entry's runtime tag
    emitter.instruction("mov rdi, rcx");                                        // pass the entry's low payload word
    emitter.instruction("mov rsi, r8");                                         // pass the entry's high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box a retained copy of the entry
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // the fresh cell is the slot's payload
    emitter.instruction("jmp __rt_array_unbox_values_x86_pack_store");          // store it
    emitter.label("__rt_array_unbox_values_x86_pack_retain_box");
    emitter.instruction("mov rax, rcx");                                        // pass the entry's Mixed cell
    emitter.instruction("call __rt_incref");                                    // the slot shares the cell with the hash until the hash goes
    emitter.instruction("jmp __rt_array_unbox_values_x86_pack_store");          // store it
    emitter.label("__rt_array_unbox_values_x86_pack_concrete");
    emitter.instruction("cmp r9, 4");                                           // containers and objects occupy tags 4 through 6
    emitter.instruction("jl __rt_array_unbox_values_x86_pack_store");           // scalars carry no reference
    emitter.instruction("cmp r9, 6");                                           // above 6 is not a container payload
    emitter.instruction("jg __rt_array_unbox_values_x86_pack_store");           // nothing to retain
    emitter.instruction("mov rax, rcx");                                        // pass the container payload
    emitter.instruction("call __rt_incref");                                    // the packed slot shares it with the hash until the hash goes
    emitter.label("__rt_array_unbox_values_x86_pack_store");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // reload the packed slot index
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the packed array
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the entry payload
    emitter.instruction("mov QWORD PTR [r11 + 24 + r10 * 8], rcx");             // store it in the packed slot
    emitter.instruction("add r10, 1");                                          // advance to the next slot
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // persist the slot index
    emitter.instruction("jmp __rt_array_unbox_values_x86_pack_loop");           // continue with the next entry

    emitter.label("__rt_array_unbox_values_x86_pack_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the hash
    emitter.instruction("call __rt_decref_hash");                               // the packed array replaces this reference
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the packed array
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // it is the container returned from here on
    emitter.instruction("jmp __rt_array_unbox_values_x86_list_stamp");          // stamp its concrete value_type

    emitter.label("__rt_array_unbox_values_x86_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the owned container
    emitter.instruction("add rsp, 80");                                         // release the narrowing frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the conversion site

    // rax = Mixed cell -> rax/rdx = payload words (container payloads gain a reference), rcx = tag.
    emitter.label("__rt_array_unbox_values_x86_cell");
    emitter.instruction("test rax, rax");                                       // a missing cell reads as null
    emitter.instruction("jz __rt_array_unbox_values_x86_cell_null");            // return the null payload
    emitter.instruction("push rbp");                                            // preserve the narrowing frame
    emitter.instruction("mov rbp, rsp");                                        // establish the cell frame
    emitter.instruction("sub rsp, 32");                                         // reserve cell, tag and payload slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the boxed runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // keep the tag
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // load the low payload word
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // keep it
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // load the high payload word
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // keep it
    emitter.instruction("cmp r10, 4");                                          // containers and objects occupy tags 4 through 6
    emitter.instruction("jl __rt_array_unbox_values_x86_cell_release");         // scalars carry no reference
    emitter.instruction("cmp r10, 6");                                          // above 6 is not a container payload
    emitter.instruction("jg __rt_array_unbox_values_x86_cell_release");         // nothing to retain
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // pass the container payload
    emitter.instruction("call __rt_incref");                                    // the slot takes its own reference before the cell goes
    emitter.label("__rt_array_unbox_values_x86_cell_release");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the Mixed cell
    emitter.instruction("call __rt_decref_mixed");                              // the slot no longer owns the cell
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // return the high payload word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // return the payload tag
    emitter.instruction("add rsp, 32");                                         // release the cell frame
    emitter.instruction("pop rbp");                                             // restore the narrowing frame
    emitter.instruction("ret");                                                 // return to the narrowing loop
    emitter.label("__rt_array_unbox_values_x86_cell_null");
    emitter.instruction("xor edx, edx");                                        // null has no high payload word
    emitter.instruction("mov ecx, 8");                                          // runtime tag 8 = null
    emitter.instruction("ret");                                                 // rax already holds the null payload
}
