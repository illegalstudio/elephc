//! Purpose:
//! Emits the `__rt_hash_get`, `__rt_hash_key_hash` runtime helper assembly for hash get.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Hash helpers must normalize PHP keys and preserve bucket layout, ownership, and iteration conventions.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_hash_get` runtime helper using linear probing with typed key comparison.
/// Uses `__rt_hash_key_hash` to compute the initial slot and `__rt_hash_key_eq` for equality checks.
/// Falls through to `__rt_hash_get_not_found` when the table is null, empty, or the key is absent.
/// Input:  x0=hash_table_ptr, x1=key_lo, x2=key_hi (key_hi=-1 means integer key, otherwise string key with key_lo=ptr, key_hi=len)
/// Output: x0=found (1 or 0), x1=value_lo, x2=value_hi, x3=value_tag (PhpType tag; null tag on miss)
pub fn emit_hash_get(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_get_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_get ---");
    emitter.label_global("__rt_hash_get");

    // -- set up stack frame, save inputs --
    // Stack layout:
    //   [sp, #0]  = hash_table_ptr
    //   [sp, #8]  = key_ptr
    //   [sp, #16] = key_len
    //   [sp, #24] = current probe index
    //   [sp, #32] = probe count
    //   [sp, #40] = saved x29
    //   [sp, #48] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save hash_table_ptr
    emitter.instruction("str x1, [sp, #8]");                                    // save key_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save key_len
    emitter.instruction("cbz x0, __rt_hash_get_not_found");                     // null tables cannot contain the requested key
    emitter.instruction("ldr x5, [x0, #8]");                                    // load capacity before hashing to avoid division by zero on empty tables
    emitter.instruction("cbz x5, __rt_hash_get_not_found");                     // zero-capacity tables cannot contain the requested key

    // -- hash the key --
    emitter.instruction("bl __rt_hash_key_hash");                               // compute the typed key hash, result in x0

    // -- compute slot index: hash % capacity --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x6, [x5, #8]");                                    // x6 = capacity from header
    emitter.instruction("udiv x7, x0, x6");                                     // x7 = hash / capacity
    emitter.instruction("msub x8, x7, x6, x0");                                 // x8 = hash % capacity
    emitter.instruction("str x8, [sp, #24]");                                   // save initial probe index
    emitter.instruction("str xzr, [sp, #32]");                                  // probe count = 0

    // -- linear probe loop --
    emitter.label("__rt_hash_get_probe");
    emitter.instruction("ldr x10, [sp, #32]");                                  // load probe count
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x6, [x5, #8]");                                    // reload capacity
    emitter.instruction("cmp x10, x6");                                         // check if we've probed all slots
    emitter.instruction("b.ge __rt_hash_get_not_found");                        // if probed all, key not found

    // -- compute entry address: base + 40 + index * 64 --
    emitter.instruction("ldr x9, [sp, #24]");                                   // load current probe index
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // x12 = index * 64
    emitter.instruction("add x12, x5, x12");                                    // x12 = table_ptr + index * 64
    emitter.instruction("add x12, x12, #40");                                   // x12 = entry address (skip header)

    // -- check occupied field --
    emitter.instruction("ldr x13, [x12]");                                      // x13 = occupied flag
    emitter.instruction("cbz x13, __rt_hash_get_not_found");                    // if empty (0), key not in table
    emitter.instruction("cmp x13, #2");                                         // check for tombstone
    emitter.instruction("b.eq __rt_hash_get_next");                             // if tombstone, skip and continue probing

    // -- slot is occupied: compare keys --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = our key_ptr
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = our key_len
    emitter.instruction("ldr x3, [x12, #8]");                                   // x3 = entry's key_ptr
    emitter.instruction("ldr x4, [x12, #16]");                                  // x4 = entry's key_len
    emitter.instruction("bl __rt_hash_key_eq");                                 // compare normalized typed keys, x0=1 if equal
    emitter.instruction("cbnz x0, __rt_hash_get_found");                        // if keys match, we found it

    // -- advance to next slot --
    emitter.label("__rt_hash_get_next");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload current probe index
    emitter.instruction("add x9, x9, #1");                                      // index += 1
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x6, [x5, #8]");                                    // reload capacity
    emitter.instruction("udiv x7, x9, x6");                                     // x7 = index / capacity
    emitter.instruction("msub x9, x7, x6, x9");                                 // x9 = index % capacity (wrap around)
    emitter.instruction("str x9, [sp, #24]");                                   // save updated probe index
    emitter.instruction("ldr x10, [sp, #32]");                                  // load probe count
    emitter.instruction("add x10, x10, #1");                                    // probe count += 1
    emitter.instruction("str x10, [sp, #32]");                                  // save updated probe count
    emitter.instruction("b __rt_hash_get_probe");                               // try next slot

    // -- key found: return entry's value --
    emitter.label("__rt_hash_get_found");

    // -- recompute entry address (registers were clobbered by str_eq) --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload probe index
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // x12 = index * 64
    emitter.instruction("add x12, x5, x12");                                    // x12 = table_ptr + index * 64
    emitter.instruction("add x12, x12, #40");                                   // x12 = entry address

    emitter.instruction("mov x0, #1");                                          // found = 1
    emitter.instruction("ldr x1, [x12, #24]");                                  // x1 = value_lo
    emitter.instruction("ldr x2, [x12, #32]");                                  // x2 = value_hi
    emitter.instruction("ldr x3, [x12, #40]");                                  // x3 = value_tag
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- key not found --
    emitter.label("__rt_hash_get_not_found");
    emitter.instruction("mov x0, #0");                                          // found = 0
    emitter.instruction("mov x1, #0");                                          // value_lo = 0
    emitter.instruction("mov x2, #0");                                          // value_hi = 0
    emitter.instruction("mov x3, #8");                                          // value_tag = null when lookup misses
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_hash_get`.
/// Uses SysV ABI: rdi=hash_table_ptr, rsi=key_lo, rdx=key_hi (key_hi=-1 means integer key, otherwise string key with rsi=ptr, rdx=len).
/// Returns: rax=found (1 or 0), rdi=value_lo, rsi=value_hi, rcx=value_tag.
fn emit_hash_get_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_get ---");
    emitter.label_global("__rt_hash_get");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving lookup spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved hash-table pointer and key payload
    emitter.instruction("sub rsp, 48");                                         // reserve local slots for hash pointer, key payload, probe index, and probe count
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash-table pointer across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the key pointer across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the key length across helper calls and probe iterations
    emitter.instruction("test rdi, rdi");                                       // null tables cannot contain the requested key
    emitter.instruction("jz __rt_hash_get_not_found");                          // return a miss before reading a null table header
    emitter.instruction("mov r11, QWORD PTR [rdi + 8]");                        // load capacity before hashing to avoid division by zero on empty tables
    emitter.instruction("test r11, r11");                                       // zero capacity means there are no live entries to probe
    emitter.instruction("jz __rt_hash_get_not_found");                          // return a miss for empty hash tables
    emitter.instruction("mov rdi, rsi");                                        // pass the lookup key low word to the typed hash helper
    emitter.instruction("mov rsi, rdx");                                        // pass the lookup key high word to the typed hash helper
    emitter.instruction("call __rt_hash_key_hash");                             // compute the 64-bit hash for the normalized lookup key
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after the hash helper returns
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the table capacity for the modulo operation and linear-probe loop
    emitter.instruction("test r11, r11");                                       // empty / uninitialised hashes have capacity 0 — `div r11` would SIGFPE
    emitter.instruction("jz __rt_hash_get_not_found");                          // treat capacity-0 hashes as misses without probing
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before dividing the 64-bit hash by the capacity
    emitter.instruction("div r11");                                             // compute hash % capacity using the SysV integer divide remainder register
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the initial probe index so the loop can survive helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the linear-probe count for full-table miss detection

    emitter.label("__rt_hash_get_probe");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer at the top of every probe iteration
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // reload capacity so a full-table miss can terminate the lookup
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // load how many slots have already been inspected
    emitter.instruction("cmp rdx, r11");                                        // check whether the probe has inspected the whole hash table
    emitter.instruction("jae __rt_hash_get_not_found");                         // stop lookup when a full table does not contain the requested key
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the current probe index before deriving the slot address
    emitter.instruction("mov r8, r11");                                         // copy the probe index before scaling it into a byte offset
    emitter.instruction("shl r8, 6");                                           // convert the probe index into a 64-byte entry offset
    emitter.instruction("add r8, r10");                                         // advance from the hash-table base pointer to the selected entry block
    emitter.instruction("add r8, 40");                                          // skip the fixed 40-byte hash header to land on the selected entry
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the occupied marker for the probed hash-entry slot
    emitter.instruction("test r9, r9");                                         // detect an empty slot that terminates the failed lookup path immediately
    emitter.instruction("jz __rt_hash_get_not_found");                          // empty slots mean the requested key does not exist in the hash table
    emitter.instruction("cmp r9, 2");                                           // check whether the current slot is a tombstone rather than a live entry
    emitter.instruction("je __rt_hash_get_next");                               // tombstones do not terminate the probe but also cannot satisfy the lookup
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the lookup key pointer to the x86_64 string-equality helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the lookup key length to the x86_64 string-equality helper
    emitter.instruction("mov rdx, QWORD PTR [r8 + 8]");                         // pass the stored entry key pointer to the x86_64 string-equality helper
    emitter.instruction("mov rcx, QWORD PTR [r8 + 16]");                        // pass the stored entry key length to the x86_64 string-equality helper
    emitter.instruction("call __rt_hash_key_eq");                               // compare the requested normalized key against the current live hash entry key
    emitter.instruction("test rax, rax");                                       // check whether the string-equality helper reported a key match
    emitter.instruction("jne __rt_hash_get_found");                             // stop probing as soon as the requested key matches the current entry

    emitter.label("__rt_hash_get_next");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after the key-compare helper clobbered caller-saved registers
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // reload the table capacity before advancing the linear-probe cursor
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the current probe index before incrementing it
    emitter.instruction("add rdx, 1");                                          // advance to the next linear-probe slot after a tombstone or key mismatch
    emitter.instruction("cmp rdx, r11");                                        // detect wraparound once the probe index reaches the table capacity
    emitter.instruction("jb __rt_hash_get_store_probe");                        // keep the incremented probe index when the cursor remains in bounds
    emitter.instruction("xor edx, edx");                                        // wrap the probe cursor back to slot zero once the end of the table is reached

    emitter.label("__rt_hash_get_store_probe");
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // persist the updated probe index before the next loop iteration
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the count of slots already inspected by this lookup
    emitter.instruction("add rdx, 1");                                          // account for the non-matching slot before probing the next one
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // persist the updated probe count for full-table miss detection
    emitter.instruction("jmp __rt_hash_get_probe");                             // continue probing until a matching or empty slot is reached

    emitter.label("__rt_hash_get_found");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer because the string-equality helper clobbered caller-saved registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the matching probe index before rebuilding the entry address
    emitter.instruction("mov r8, r11");                                         // copy the matching probe index before scaling it into a byte offset
    emitter.instruction("shl r8, 6");                                           // convert the matching probe index into a 64-byte entry offset
    emitter.instruction("add r8, r10");                                         // advance from the hash-table base pointer to the matching entry block
    emitter.instruction("add r8, 40");                                          // skip the fixed 40-byte hash header to land on the matching entry
    emitter.instruction("mov rdi, QWORD PTR [r8 + 24]");                        // return the low payload word in the first borrowed-value result register
    emitter.instruction("mov rsi, QWORD PTR [r8 + 32]");                        // return the high payload word in the second borrowed-value result register
    emitter.instruction("mov rcx, QWORD PTR [r8 + 40]");                        // return the runtime value tag in the borrowed-value tag result register
    emitter.instruction("mov rax, 1");                                          // return found = 1 in the standard integer result register
    emitter.instruction("add rsp, 48");                                         // release the lookup spill slots before returning the borrowed payload
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the successful lookup result to generated code

    emitter.label("__rt_hash_get_not_found");
    emitter.instruction("xor eax, eax");                                        // return found = 0 in the standard integer result register when the key is absent
    emitter.instruction("xor edi, edi");                                        // clear the low payload word for the failed lookup path
    emitter.instruction("xor esi, esi");                                        // clear the high payload word for the failed lookup path
    emitter.instruction("mov ecx, 8");                                          // return runtime value tag 8 = null for failed hash lookups
    emitter.instruction("add rsp, 48");                                         // release the lookup spill slots before returning the failed lookup result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the failed lookup result to generated code
}
