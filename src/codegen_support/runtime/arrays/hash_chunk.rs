//! Purpose:
//! Emits the `__rt_hash_chunk` runtime helper backing `array_chunk()` over an ASSOCIATIVE
//! receiver. Walks the source hash once in insertion order and fills one owned hash per chunk,
//! collecting them into an outer indexed array of pointers.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - `preserve_keys` is a runtime argument here rather than two helpers, because the flag changes
//!   only which key each entry is inserted under: the source key when true, the entry's position
//!   WITHIN ITS OWN CHUNK when false. PHP restarts that numbering at 0 in every chunk, which is
//!   why the counter is reset when a chunk closes rather than running across the whole source.
//! - `preserve_keys: false` drops STRING keys too, which is where chunk's rule parts company with
//!   `array_slice()`'s: slice renumbers integer keys and leaves string keys alone.
//! - Both forms build hash chunks. A dense indexed array could hold the renumbered form, but only
//!   by deciding the element width first, which is the 16-byte `{pointer, length}` slot problem
//!   that makes a string-valued source unrepresentable (issue #675). A hash keyed 0,1,2,… reads
//!   and prints identically and sidesteps it.
//! - Each chunk OWNS its payloads, so values are taken the way `__rt_array_slice_to_hash` takes
//!   them: strings are persisted into independent heap copies, heap-backed values (tags 4..=7)
//!   are retained, and scalars are copied by value. `__rt_hash_set` persists string KEYS itself.
//! - The inner hashes inherit the source's `value_type` header word, so a chunk is stamped the
//!   same way the container its values came from is.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// hash_chunk: split an associative array into an indexed array of owned hash chunks.
/// Input:  x0 = source hash pointer, x1 = chunk size (must be >= 1),
///         x2 = preserve_keys (0 renumbers each chunk from 0, 1 keeps the source keys)
/// Output: x0 = outer indexed array whose elements are owned hash pointers
///
/// Backs `array_chunk($assoc, $length, $preserve_keys)`.
pub fn emit_hash_chunk(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_chunk_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_chunk ---");
    emitter.label_global("__rt_hash_chunk");

    // Stack frame (112 bytes):
    //   [sp, #0]  source hash      [sp, #8]  chunk size     [sp, #16] preserve_keys
    //   [sp, #24] outer array      [sp, #32] open chunk     [sp, #40] entries in open chunk
    //   [sp, #48] iteration cursor [sp, #56] key low word    [sp, #64] key high word
    //   [sp, #72] value low word   [sp, #80] value high word [sp, #88] value runtime tag
    //   [sp, #96] saved x29/x30
    emitter.instruction("sub sp, sp, #112");                                    // allocate the chunking stack frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source hash pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested chunk size
    emitter.instruction("str x2, [sp, #16]");                                   // save the preserve_keys flag

    emitter.instruction("mov x0, #0");                                          // the outer array grows through push; start header-only
    emitter.instruction("mov x1, #8");                                          // outer slots hold pointer-sized hash payloads
    emitter.instruction("bl __rt_array_new");                                   // allocate the outer indexed array, x0 = outer
    emitter.instruction("str x0, [sp, #24]");                                   // save the outer indexed array pointer
    emitter.instruction("str xzr, [sp, #32]");                                  // no chunk is open yet
    emitter.instruction("str xzr, [sp, #40]");                                  // the open chunk holds no entries yet
    emitter.instruction("str xzr, [sp, #48]");                                  // cursor 0 starts a fresh insertion-order walk

    emitter.label("__rt_hash_chunk_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source hash pointer
    emitter.instruction("ldr x1, [sp, #48]");                                   // reload the insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // x0 = next cursor, x1/x2 = key, x3/x4 = value, x5 = tag
    emitter.instruction("cmn x0, #1");                                          // did the iterator signal end-of-walk?
    emitter.instruction("b.eq __rt_hash_chunk_flush");                          // the source is exhausted; close any partial chunk
    emitter.instruction("str x0, [sp, #48]");                                   // save the next insertion-order cursor
    emitter.instruction("str x1, [sp, #56]");                                   // save this entry's key low word
    emitter.instruction("str x2, [sp, #64]");                                   // save this entry's key high word (-1 marks an integer key)
    emitter.instruction("str x3, [sp, #72]");                                   // save this entry's value low word
    emitter.instruction("str x4, [sp, #80]");                                   // save this entry's value high word
    emitter.instruction("str x5, [sp, #88]");                                   // save this entry's value runtime tag

    // -- open a fresh chunk when the previous one closed (or this is the first entry) --
    emitter.instruction("ldr x9, [sp, #32]");                                   // is a chunk already open?
    emitter.instruction("cbnz x9, __rt_hash_chunk_have_chunk");                 // yes: keep filling it
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the source hash to read its header
    emitter.instruction("ldr x1, [x9, #16]");                                   // inherit the source hash's value_type word
    emitter.instruction("ldr x0, [sp, #8]");                                    // the requested chunk size is only an UPPER bound
    emitter.instruction("ldr x12, [x9, #0]");                                   // entries the source actually holds
    emitter.instruction("cmp x0, x12");                                         // is the requested size larger than the whole source?
    emitter.instruction("csel x0, x0, x12, ls");                                // size the chunk for what can really enter it
    emitter.instruction("bl __rt_hash_new");                                    // allocate this chunk's hash, x0 = chunk
    emitter.instruction("str x0, [sp, #32]");                                   // record the newly opened chunk
    emitter.instruction("str xzr, [sp, #40]");                                  // a fresh chunk holds no entries yet

    emitter.label("__rt_hash_chunk_have_chunk");
    // -- the chunk becomes a new owner of this value, so take a reference for it --
    emitter.instruction("ldr x9, [sp, #88]");                                   // reload the value runtime tag
    emitter.instruction("cmp x9, #1");                                          // runtime tag 1 = string
    emitter.instruction("b.eq __rt_hash_chunk_string");                         // strings need an independent heap copy
    emitter.instruction("cmp x9, #4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("b.lt __rt_hash_chunk_insert");                         // scalar values need no retain
    emitter.instruction("cmp x9, #7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("b.gt __rt_hash_chunk_insert");                         // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #72]");                                   // load the heap-backed value pointer
    emitter.instruction("bl __rt_incref");                                      // retain the heap-backed value for this chunk
    emitter.instruction("b __rt_hash_chunk_insert");                            // continue to insertion

    emitter.label("__rt_hash_chunk_string");
    emitter.instruction("ldr x1, [sp, #72]");                                   // load the string pointer
    emitter.instruction("ldr x2, [sp, #80]");                                   // load the string length
    emitter.instruction("bl __rt_str_persist");                                 // copy the string into an independent heap block, x1 = new pointer
    emitter.instruction("str x1, [sp, #72]");                                   // save the persisted string pointer
    emitter.instruction("str x2, [sp, #80]");                                   // save the string length

    emitter.label("__rt_hash_chunk_insert");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the preserve_keys flag
    emitter.instruction("cbnz x9, __rt_hash_chunk_source_key");                 // keep the source key when the flag is set
    emitter.instruction("ldr x1, [sp, #40]");                                   // renumbered key = this entry's position within its own chunk
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("b __rt_hash_chunk_set");                               // insert under the renumbered key
    emitter.label("__rt_hash_chunk_source_key");
    emitter.instruction("ldr x1, [sp, #56]");                                   // reload the source key low word
    emitter.instruction("ldr x2, [sp, #64]");                                   // reload the source key high word
    emitter.label("__rt_hash_chunk_set");
    emitter.instruction("ldr x0, [sp, #32]");                                   // the open chunk receives this entry
    emitter.instruction("ldr x3, [sp, #72]");                                   // value low word
    emitter.instruction("ldr x4, [sp, #80]");                                   // value high word
    emitter.instruction("ldr x5, [sp, #88]");                                   // value runtime tag
    emitter.instruction("bl __rt_hash_set");                                    // insert the owned value into the open chunk
    emitter.instruction("str x0, [sp, #32]");                                   // publish the possibly-reallocated chunk pointer

    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the open chunk's entry count
    emitter.instruction("add x9, x9, #1");                                      // count this entry
    emitter.instruction("str x9, [sp, #40]");                                   // save the updated count
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the requested chunk size
    emitter.instruction("cmp x9, x10");                                         // is the chunk full?
    emitter.instruction("b.lt __rt_hash_chunk_loop");                           // not yet: take the next source entry
    emitter.instruction("ldr x1, [sp, #32]");                                   // the finished chunk is the value appended to the outer array
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the outer indexed array pointer
    emitter.instruction("bl __rt_array_push_int");                              // append the finished chunk to the outer array
    emitter.instruction("str x0, [sp, #24]");                                   // publish the possibly-grown outer array pointer
    emitter.instruction("str xzr, [sp, #32]");                                  // close the chunk so the next entry opens a new one
    emitter.instruction("str xzr, [sp, #40]");                                  // and restart PHP's per-chunk key numbering at 0
    emitter.instruction("b __rt_hash_chunk_loop");                              // continue with the next source entry

    emitter.label("__rt_hash_chunk_flush");
    emitter.instruction("ldr x9, [sp, #32]");                                   // is a partial chunk still open?
    emitter.instruction("cbz x9, __rt_hash_chunk_done");                        // no: the source length was a multiple of the chunk size
    emitter.instruction("mov x1, x9");                                          // append the short final chunk
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the outer indexed array pointer
    emitter.instruction("bl __rt_array_push_int");                              // append the short final chunk to the outer array
    emitter.instruction("str x0, [sp, #24]");                                   // publish the possibly-grown outer array pointer

    emitter.label("__rt_hash_chunk_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = outer indexed array pointer
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the outer array in x0
}

/// x86_64 Linux implementation of `__rt_hash_chunk`.
/// Input:  rdi = source hash pointer, rsi = chunk size (must be >= 1),
///         rdx = preserve_keys (0 renumbers each chunk from 0, 1 keeps the source keys)
/// Output: rax = outer indexed array whose elements are owned hash pointers
fn emit_hash_chunk_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_chunk ---");
    emitter.label_global("__rt_hash_chunk");

    // Stack frame (96 bytes below rbp), mirroring the AArch64 slot assignment:
    //   [rbp-8]  source hash     [rbp-16] chunk size      [rbp-24] preserve_keys
    //   [rbp-32] outer array     [rbp-40] open chunk      [rbp-48] entries in open chunk
    //   [rbp-56] cursor          [rbp-64] key low word    [rbp-72] key high word
    //   [rbp-80] value low word  [rbp-88] value high word [rbp-96] value runtime tag
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 96");                                         // reserve local slots for the chunking loop state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested chunk size
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the preserve_keys flag

    emitter.instruction("xor edi, edi");                                        // the outer array grows through push; start header-only
    emitter.instruction("mov rsi, 8");                                          // outer slots hold pointer-sized hash payloads
    emitter.instruction("call __rt_array_new");                                 // allocate the outer indexed array, rax = outer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the outer indexed array pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // no chunk is open yet
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // the open chunk holds no entries yet
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // cursor 0 starts a fresh insertion-order walk

    emitter.label("__rt_hash_chunk_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next");                            // rax = next cursor, rdi/rdx = key, rcx/r8 = value, r9 = tag
    emitter.instruction("cmp rax, -1");                                         // did the iterator signal end-of-walk?
    emitter.instruction("je __rt_hash_chunk_flush");                            // the source is exhausted; close any partial chunk
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the next insertion-order cursor
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // save this entry's key low word
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save this entry's key high word (-1 marks an integer key)
    emitter.instruction("mov QWORD PTR [rbp - 80], rcx");                       // save this entry's value low word
    emitter.instruction("mov QWORD PTR [rbp - 88], r8");                        // save this entry's value high word
    emitter.instruction("mov QWORD PTR [rbp - 96], r9");                        // save this entry's value runtime tag

    // -- open a fresh chunk when the previous one closed (or this is the first entry) --
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // is a chunk already open?
    emitter.instruction("jne __rt_hash_chunk_have_chunk");                      // yes: keep filling it
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source hash to read its header
    emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");                       // inherit the source hash's value_type word
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the requested chunk size is only an UPPER bound
    emitter.instruction("mov r11, QWORD PTR [r10 + 0]");                        // entries the source actually holds
    emitter.instruction("cmp rdi, r11");                                        // is the requested size larger than the whole source?
    emitter.instruction("cmova rdi, r11");                                      // size the chunk for what can really enter it
    emitter.instruction("call __rt_hash_new");                                  // allocate this chunk's hash, rax = chunk
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // record the newly opened chunk
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // a fresh chunk holds no entries yet

    emitter.label("__rt_hash_chunk_have_chunk");
    // -- the chunk becomes a new owner of this value, so take a reference for it --
    emitter.instruction("mov r10, QWORD PTR [rbp - 96]");                       // reload the value runtime tag
    emitter.instruction("cmp r10, 1");                                          // runtime tag 1 = string
    emitter.instruction("je __rt_hash_chunk_string");                           // strings need an independent heap copy
    emitter.instruction("cmp r10, 4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("jl __rt_hash_chunk_insert");                           // scalar values need no retain
    emitter.instruction("cmp r10, 7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("jg __rt_hash_chunk_insert");                           // non-heap tags need no retain
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // load the heap-backed value pointer
    emitter.instruction("call __rt_incref");                                    // retain the heap-backed value for this chunk
    emitter.instruction("jmp __rt_hash_chunk_insert");                          // continue to insertion

    emitter.label("__rt_hash_chunk_string");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // load the string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // load the string length
    emitter.instruction("call __rt_str_persist");                               // copy the string into an independent heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the persisted string pointer
    emitter.instruction("mov QWORD PTR [rbp - 88], rdx");                       // save the string length

    emitter.label("__rt_hash_chunk_insert");
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // reload the preserve_keys flag
    emitter.instruction("jne __rt_hash_chunk_source_key");                      // keep the source key when the flag is set
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // renumbered key = this entry's position within its own chunk
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("jmp __rt_hash_chunk_set");                             // insert under the renumbered key
    emitter.label("__rt_hash_chunk_source_key");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // reload the source key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // reload the source key high word
    emitter.label("__rt_hash_chunk_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // the open chunk receives this entry
    emitter.instruction("mov rcx, QWORD PTR [rbp - 80]");                       // value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 88]");                        // value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 96]");                        // value runtime tag
    emitter.instruction("call __rt_hash_set");                                  // insert the owned value into the open chunk
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // publish the possibly-reallocated chunk pointer

    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the open chunk's entry count
    emitter.instruction("add r10, 1");                                          // count this entry
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the updated count
    emitter.instruction("cmp r10, QWORD PTR [rbp - 16]");                       // is the chunk full?
    emitter.instruction("jl __rt_hash_chunk_loop");                             // not yet: take the next source entry
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // the finished chunk is the value appended to the outer array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the outer indexed array pointer
    emitter.instruction("call __rt_array_push_int");                            // append the finished chunk to the outer array
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // publish the possibly-grown outer array pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // close the chunk so the next entry opens a new one
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // and restart PHP's per-chunk key numbering at 0
    emitter.instruction("jmp __rt_hash_chunk_loop");                            // continue with the next source entry

    emitter.label("__rt_hash_chunk_flush");
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // is a partial chunk still open?
    emitter.instruction("je __rt_hash_chunk_done");                             // no: the source length was a multiple of the chunk size
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // append the short final chunk
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the outer indexed array pointer
    emitter.instruction("call __rt_array_push_int");                            // append the short final chunk to the outer array
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // publish the possibly-grown outer array pointer

    emitter.label("__rt_hash_chunk_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // rax = outer indexed array pointer
    emitter.instruction("add rsp, 96");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the outer array in rax
}
