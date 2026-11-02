//! Purpose:
//! Emits the `__rt_instanceof_lookup`, `__rt_instanceof_lookup_no` runtime helper assembly for dynamic instanceof.
//! Keeps exception object matching, unwinding state, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::exceptions`.
//!
//! Key details:
//! - Exception matching and unwinding must keep handler-stack, call-frame cleanup, and class metadata invariants aligned.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;
use crate::codegen_support::runtime::data::instanceof::{
    INSTANCEOF_HASH_BASIS, INSTANCEOF_HASH_PRIME,
};

/// Emits the `__rt_instanceof_lookup`, `__rt_instanceof_lookup_no`, and
/// `__rt_instanceof_invalid_target` runtime helpers for dynamic instanceof.
/// Dispatches to the target-specific implementation based on `emitter.target.arch`.
/// On x86_64, calls the Linux x86_64 variant; on ARM64, emits the generic implementation.
///
/// Input registers (ARM64): x0 = result (1=success, 0=failure), x1 = string pointer, x2 = string length
/// Output registers (ARM64): x0 = 1 success / 0 failure, x1 = target id, x2 = 0 class / 1 interface
/// Input registers (x86_64 Linux): rdi = string pointer, rsi = string length (System V ABI)
/// Output registers (x86_64 Linux): rax = 1 success / 0 failure, rdi = target id, rdx = 0 class / 1 interface
///
/// The lookup hashes the requested name (FNV-1a over ASCII-lowercased bytes, the same hash the
/// compiler used to build `_instanceof_target_hash`), probes that open-addressing index, and
/// compares each candidate entry of `_instanceof_target_entries` (name ptr, name len, target
/// id, target kind) case-insensitively. It returns the first matching target's metadata, in
/// table order, or signals failure at the first empty slot.
pub fn emit_dynamic_instanceof(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_dynamic_instanceof_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: dynamic_instanceof ---");
    emitter.label_global("__rt_instanceof_lookup");

    // -- hash the requested name exactly as the compiler hashed the table's names --
    emitter.instruction("cbz x1, __rt_instanceof_lookup_no");                   // null string pointers cannot name a class or interface
    abi::emit_load_int_immediate(emitter, "x9", INSTANCEOF_HASH_BASIS as i64);  // x9 = FNV-1a offset basis
    abi::emit_load_int_immediate(emitter, "x10", INSTANCEOF_HASH_PRIME as i64); // x10 = FNV-1a prime
    emitter.instruction("mov x11, #0");                                         // x11 = byte index within the requested name
    emitter.label("__rt_instanceof_lookup_hash_loop");
    emitter.instruction("cmp x11, x2");                                         // has every byte of the requested name been hashed?
    emitter.instruction("b.hs __rt_instanceof_lookup_hashed");                  // yes — probe the index
    emitter.instruction("ldrb w12, [x1, x11]");                                 // load the next byte of the requested name
    emitter.instruction("sub w13, w12, #65");                                   // w13 = byte - 'A'
    emitter.instruction("cmp w13, #25");                                        // is the byte an uppercase ASCII letter?
    emitter.instruction("b.hi __rt_instanceof_lookup_hash_byte");               // no — hash it unchanged
    emitter.instruction("add w12, w12, #32");                                   // fold the uppercase letter to lowercase, as PHP class names compare
    emitter.label("__rt_instanceof_lookup_hash_byte");
    emitter.instruction("eor x9, x9, x12");                                     // mix the folded byte into the hash
    emitter.instruction("mul x9, x9, x10");                                     // multiply by the FNV prime
    emitter.instruction("add x11, x11, #1");                                    // advance to the next byte
    emitter.instruction("b __rt_instanceof_lookup_hash_loop");                  // keep hashing

    // -- probe the open-addressing index; an empty slot means no such class-like --
    // x9 = slot, x13 = mask, x14 = index base, x15 = entry table base, x10 = candidate entry
    emitter.label("__rt_instanceof_lookup_hashed");
    abi::emit_symbol_address(emitter, "x13", "_instanceof_target_hash_mask");
    emitter.instruction("ldr x13, [x13]");                                      // x13 = index size - 1
    emitter.instruction("and x9, x9, x13");                                     // x9 = the name's home slot
    abi::emit_symbol_address(emitter, "x14", "_instanceof_target_hash");
    abi::emit_symbol_address(emitter, "x15", "_instanceof_target_entries");
    emitter.label("__rt_instanceof_lookup_probe");
    emitter.instruction("ldr w10, [x14, x9, lsl #2]");                          // w10 = entry index + 1 in this slot, or 0 when it is empty
    emitter.instruction("cbz w10, __rt_instanceof_lookup_no");                  // an empty slot ends the probe: the name is not a known target
    emitter.instruction("sub x10, x10, #1");                                    // x10 = entry index
    emitter.instruction("add x10, x15, x10, lsl #5");                           // x10 = address of this four-word entry
    emitter.instruction("ldr x16, [x10, #8]");                                  // x16 = candidate name length
    emitter.instruction("cmp x2, x16");                                         // compare the requested length with the candidate's
    emitter.instruction("b.ne __rt_instanceof_lookup_next");                    // names with different lengths cannot match
    emitter.instruction("ldr x12, [x10]");                                      // x12 = candidate name pointer
    emitter.instruction("mov x11, #0");                                         // x11 = byte index within the candidate name

    emitter.label("__rt_instanceof_lookup_byte_loop");
    emitter.instruction("cmp x11, x2");                                         // have all bytes in this equal-length name been checked?
    emitter.instruction("b.hs __rt_instanceof_lookup_match");                   // every byte matched case-insensitively
    emitter.instruction("ldrb w16, [x1, x11]");                                 // load a byte from the requested name
    emitter.instruction("sub w17, w16, #65");                                   // w17 = byte - 'A'
    emitter.instruction("cmp w17, #25");                                        // is the requested byte an uppercase ASCII letter?
    emitter.instruction("b.hi __rt_instanceof_lookup_rhs");                     // no — compare it unchanged
    emitter.instruction("add w16, w16, #32");                                   // lowercase the requested byte for PHP class-name lookup
    emitter.label("__rt_instanceof_lookup_rhs");
    emitter.instruction("ldrb w17, [x12, x11]");                                // load the corresponding candidate-name byte
    emitter.instruction("sub w13, w17, #65");                                   // w13 = candidate byte - 'A'
    emitter.instruction("cmp w13, #25");                                        // is the candidate byte an uppercase ASCII letter?
    emitter.instruction("b.hi __rt_instanceof_lookup_cmp");                     // no — compare it unchanged
    emitter.instruction("add w17, w17, #32");                                   // lowercase the candidate byte for case-insensitive comparison
    emitter.label("__rt_instanceof_lookup_cmp");
    emitter.instruction("cmp w16, w17");                                        // compare the lowercased requested and candidate bytes
    emitter.instruction("b.ne __rt_instanceof_lookup_next");                    // this candidate is not the requested target
    emitter.instruction("add x11, x11, #1");                                    // advance to the next byte
    emitter.instruction("b __rt_instanceof_lookup_byte_loop");                  // continue comparing this candidate

    emitter.label("__rt_instanceof_lookup_match");
    emitter.instruction("ldr x1, [x10, #16]");                                  // return the matched class/interface id
    emitter.instruction("ldr x2, [x10, #24]");                                  // return 0 for class targets or 1 for interface targets
    emitter.instruction("mov x0, #1");                                          // signal that the dynamic target string resolved successfully
    emitter.instruction("ret");                                                 // return lookup success plus target metadata

    emitter.label("__rt_instanceof_lookup_next");
    abi::emit_symbol_address(emitter, "x13", "_instanceof_target_hash_mask");
    emitter.instruction("ldr x13, [x13]");                                      // x13 = index size - 1 (the byte loop reused the register)
    emitter.instruction("add x9, x9, #1");                                      // step to the next slot on the probe path
    emitter.instruction("and x9, x9, x13");                                     // wrap around the end of the index
    emitter.instruction("b __rt_instanceof_lookup_probe");                      // probe the next slot

    emitter.label("__rt_instanceof_lookup_no");
    emitter.instruction("mov x0, #0");                                          // signal that no class/interface target matched the string
    emitter.instruction("mov x1, #0");                                          // clear the target id result on failed lookup
    emitter.instruction("mov x2, #0");                                          // clear the target kind result on failed lookup
    emitter.instruction("ret");                                                 // return lookup failure

    emitter.blank();
    emitter.label_global("__rt_instanceof_invalid_target");
    if emitter.phpext_output { abi::emit_cdylib_exit_escape(emitter); }
    abi::emit_symbol_address(emitter, "x1", "_instanceof_target_type_msg");     // load the page containing the dynamic-target TypeError message
    emitter.instruction("mov x2, #59");                                         // pass the dynamic-target TypeError message length to write()
    emitter.instruction("mov x0, #2");                                          // fd = stderr for the dynamic instanceof TypeError
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit status 1 indicates abnormal termination
    emitter.syscall(1);
}

/// Emits the `__rt_instanceof_lookup` and `__rt_instanceof_invalid_target` helpers
/// for Linux x86_64 (System V ABI). Performs case-insensitive string comparison against
/// `_instanceof_target_entries` metadata table.
///
/// Input registers: rax = string pointer, rdx = string length
/// Output registers: rax = 1 success / 0 failure, rdi = target id, rdx = 0 class / 1 interface
///
/// Each table entry is 32 bytes: name ptr (8), name len (8), target id (8), target kind (8).
/// The name is found through the `_instanceof_target_hash` index, as on AArch64.
fn emit_dynamic_instanceof_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: dynamic_instanceof ---");
    emitter.label_global("__rt_instanceof_lookup");

    // -- hash the requested name exactly as the compiler hashed the table's names --
    emitter.instruction("test rax, rax");                                       // null string pointers cannot name a class or interface
    emitter.instruction("je __rt_instanceof_lookup_no");                        // report lookup failure for null dynamic target strings
    emitter.instruction("mov r8, rax");                                         // preserve the dynamic target string pointer
    emitter.instruction("mov r9, rdx");                                         // preserve the dynamic target string length
    emitter.instruction(&format!("mov r10, {:#x}", INSTANCEOF_HASH_BASIS));     // r10 = FNV-1a offset basis
    emitter.instruction(&format!("mov rsi, {:#x}", INSTANCEOF_HASH_PRIME));     // rsi = FNV-1a prime
    emitter.instruction("xor ecx, ecx");                                        // rcx = byte index within the requested name
    emitter.label("__rt_instanceof_lookup_hash_loop");
    emitter.instruction("cmp rcx, r9");                                         // has every byte of the requested name been hashed?
    emitter.instruction("jae __rt_instanceof_lookup_hashed");                   // yes — probe the index
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load the next byte of the requested name
    emitter.instruction("lea edx, [rax - 65]");                                 // edx = byte - 'A'
    emitter.instruction("cmp edx, 25");                                         // is the byte an uppercase ASCII letter?
    emitter.instruction("ja __rt_instanceof_lookup_hash_byte");                 // no — hash it unchanged
    emitter.instruction("add eax, 32");                                         // fold the uppercase letter to lowercase, as PHP class names compare
    emitter.label("__rt_instanceof_lookup_hash_byte");
    emitter.instruction("xor r10, rax");                                        // mix the folded byte into the hash
    emitter.instruction("imul r10, rsi");                                       // multiply by the FNV prime
    emitter.instruction("add rcx, 1");                                          // advance to the next byte
    emitter.instruction("jmp __rt_instanceof_lookup_hash_loop");                // keep hashing

    // -- probe the open-addressing index; an empty slot means no such class-like --
    // r10 = slot, rsi = index base, rdi = candidate entry, r11 = scratch
    emitter.label("__rt_instanceof_lookup_hashed");
    abi::emit_load_symbol_to_reg(emitter, "r11", "_instanceof_target_hash_mask", 0); // r11 = index size - 1
    emitter.instruction("and r10, r11");                                        // r10 = the name's home slot
    abi::emit_symbol_address(emitter, "rsi", "_instanceof_target_hash");
    emitter.label("__rt_instanceof_lookup_probe");
    emitter.instruction("mov eax, DWORD PTR [rsi + r10*4]");                    // eax = entry index + 1 in this slot, or 0 when it is empty
    emitter.instruction("test eax, eax");                                       // is the slot empty?
    emitter.instruction("je __rt_instanceof_lookup_no");                        // yes — the name is not a known target
    emitter.instruction("sub eax, 1");                                          // rax = entry index
    emitter.instruction("shl rax, 5");                                          // rax = byte offset of this four-word entry
    abi::emit_symbol_address(emitter, "rdi", "_instanceof_target_entries");
    emitter.instruction("add rdi, rax");                                        // rdi = address of the candidate entry
    emitter.instruction("cmp r9, QWORD PTR [rdi + 8]");                         // compare the requested length with the candidate's
    emitter.instruction("jne __rt_instanceof_lookup_next");                     // names with different lengths cannot match
    emitter.instruction("xor ecx, ecx");                                        // rcx = byte index within the candidate name

    emitter.label("__rt_instanceof_lookup_byte_loop");
    emitter.instruction("cmp rcx, r9");                                         // have all bytes in this equal-length name been checked?
    emitter.instruction("jae __rt_instanceof_lookup_match");                    // every byte matched case-insensitively
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load a byte from the requested name
    emitter.instruction("lea edx, [rax - 65]");                                 // edx = requested byte - 'A'
    emitter.instruction("cmp edx, 25");                                         // is the requested byte an uppercase ASCII letter?
    emitter.instruction("ja __rt_instanceof_lookup_rhs");                       // no — compare it unchanged
    emitter.instruction("add eax, 32");                                         // lowercase the requested byte for PHP class-name lookup
    emitter.label("__rt_instanceof_lookup_rhs");
    emitter.instruction("mov rdx, QWORD PTR [rdi]");                            // rdx = candidate name pointer
    emitter.instruction("movzx edx, BYTE PTR [rdx + rcx]");                     // load the corresponding candidate-name byte
    emitter.instruction("lea r11d, [rdx - 65]");                                // r11d = candidate byte - 'A'
    emitter.instruction("cmp r11d, 25");                                        // is the candidate byte an uppercase ASCII letter?
    emitter.instruction("ja __rt_instanceof_lookup_cmp");                       // no — compare it unchanged
    emitter.instruction("add edx, 32");                                         // lowercase the candidate byte for case-insensitive comparison
    emitter.label("__rt_instanceof_lookup_cmp");
    emitter.instruction("cmp eax, edx");                                        // compare the lowercased requested and candidate bytes
    emitter.instruction("jne __rt_instanceof_lookup_next");                     // this candidate is not the requested target
    emitter.instruction("add rcx, 1");                                          // advance to the next byte
    emitter.instruction("jmp __rt_instanceof_lookup_byte_loop");                // continue comparing this candidate

    emitter.label("__rt_instanceof_lookup_match");
    emitter.instruction("mov rdx, QWORD PTR [rdi + 24]");                       // return 0 for class targets or 1 for interface targets
    emitter.instruction("mov rdi, QWORD PTR [rdi + 16]");                       // return the matched class/interface id
    emitter.instruction("mov eax, 1");                                          // signal that the dynamic target string resolved successfully
    emitter.instruction("ret");                                                 // return lookup success plus target metadata

    emitter.label("__rt_instanceof_lookup_next");
    abi::emit_load_symbol_to_reg(emitter, "r11", "_instanceof_target_hash_mask", 0); // r11 = index size - 1 (the byte loop reused the register)
    emitter.instruction("add r10, 1");                                          // step to the next slot on the probe path
    emitter.instruction("and r10, r11");                                        // wrap around the end of the index
    emitter.instruction("jmp __rt_instanceof_lookup_probe");                    // probe the next slot

    emitter.label("__rt_instanceof_lookup_no");
    emitter.instruction("xor eax, eax");                                        // signal that no class/interface target matched the string
    emitter.instruction("xor edi, edi");                                        // clear the target id result on failed lookup
    emitter.instruction("xor edx, edx");                                        // clear the target kind result on failed lookup
    emitter.instruction("ret");                                                 // return lookup failure

    emitter.blank();
    emitter.label_global("__rt_instanceof_invalid_target");
    if emitter.phpext_output { abi::emit_cdylib_exit_escape(emitter); }
    abi::emit_symbol_address(emitter, "rsi", "_instanceof_target_type_msg");    // point write() at the dynamic-target TypeError message
    emitter.instruction("mov edx, 59");                                         // pass the dynamic-target TypeError message length to write()
    emitter.instruction("mov edi, 2");                                          // fd = stderr for the dynamic instanceof TypeError
    emitter.instruction("mov eax, 1");                                          // Linux syscall 1 = write
    emitter.instruction("syscall");                                             // emit the dynamic instanceof TypeError diagnostic
    emitter.instruction("mov edi, 1");                                          // exit status 1 indicates abnormal termination
    emitter.instruction("mov eax, 231");                                        // Linux syscall 231 = exit_group
    emitter.instruction("syscall");                                             // terminate after the dynamic instanceof TypeError
}
