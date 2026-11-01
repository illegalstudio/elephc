//! Purpose:
//! Emits the `__rt_strcasecmp`, `__rt_strcasecmp_loop` runtime helper assembly for strcasecmp.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_strcasecmp` runtime helper for case-insensitive string comparison.
///
/// dispatches to the x86_64 Linux variant or emits the ARM64 fallback depending on
/// `emitter.target.arch`. The helper is called with pointer/length pairs:
///
/// - ARM64: x1=ptr_a, x2=len_a, x3=ptr_b, x4=len_b → x0 = signed result
/// - x86_64 Linux: rdi=ptr_a, rsi=len_a, rdx=ptr_b, rcx=len_b → rax = signed result
///
/// Returns a signed integer: < 0 if a < b, 0 if a == b, > 0 if a > b.
/// Comparison is lexicographic over the shared prefix, then by length when equal.
/// ASCII bytes 'A'-'Z' are lowercased before comparison; all other bytes are compared as-is.
pub fn emit_strcasecmp(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strcasecmp_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strcasecmp ---");
    emitter.label_global("__rt_strcasecmp");

    // -- determine minimum length for comparison --
    emitter.instruction("cmp x2, x4");                                          // compare lengths of both strings
    emitter.instruction("csel x5, x2, x4, lt");                                 // x5 = min(len_a, len_b)
    emitter.instruction("mov x6, #0");                                          // initialize byte index to 0

    // -- compare bytes (case-insensitive) --
    emitter.label("__rt_strcasecmp_loop");
    emitter.instruction("cmp x6, x5");                                          // check if we've compared all min-length bytes
    emitter.instruction("b.ge __rt_strcasecmp_len");                            // if done, compare by string lengths
    emitter.instruction("ldrb w7, [x1, x6]");                                   // load byte from string A at index
    emitter.instruction("ldrb w8, [x3, x6]");                                   // load byte from string B at index

    // -- convert byte A to lowercase if uppercase --
    emitter.instruction("cmp w7, #65");                                         // compare with 'A'
    emitter.instruction("b.lt __rt_strcasecmp_b");                              // if below 'A', skip conversion
    emitter.instruction("cmp w7, #90");                                         // compare with 'Z'
    emitter.instruction("b.gt __rt_strcasecmp_b");                              // if above 'Z', skip conversion
    emitter.instruction("add w7, w7, #32");                                     // convert A-Z to a-z

    // -- convert byte B to lowercase if uppercase --
    emitter.label("__rt_strcasecmp_b");
    emitter.instruction("cmp w8, #65");                                         // compare with 'A'
    emitter.instruction("b.lt __rt_strcasecmp_cmp");                            // if below 'A', skip conversion
    emitter.instruction("cmp w8, #90");                                         // compare with 'Z'
    emitter.instruction("b.gt __rt_strcasecmp_cmp");                            // if above 'Z', skip conversion
    emitter.instruction("add w8, w8, #32");                                     // convert A-Z to a-z

    // -- compare lowered bytes --
    emitter.label("__rt_strcasecmp_cmp");
    emitter.instruction("cmp w7, w8");                                          // compare the two lowered bytes
    emitter.instruction("b.ne __rt_strcasecmp_diff");                           // if different, return their difference
    emitter.instruction("add x6, x6, #1");                                      // advance to next byte
    emitter.instruction("b __rt_strcasecmp_loop");                              // continue comparing

    // -- bytes differ: return difference --
    emitter.label("__rt_strcasecmp_diff");
    emitter.instruction("sub x0, x7, x8");                                      // return lowered_a - lowered_b
    emitter.instruction("ret");                                                 // return to caller

    // -- all shared bytes equal: compare by length --
    emitter.label("__rt_strcasecmp_len");
    emitter.instruction("sub x0, x2, x4");                                      // return len_a - len_b as tiebreaker
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_name_list_find_ci`, the shared case-insensitive lookup of a runtime name in a
/// packed name list.
///
/// The list is one byte string of entries `[u16 little-endian length][name bytes]`, baked by the
/// caller as content-addressed data, so every site that asks the same question (a dynamic
/// `class_exists($name)` against the program's classes) shares one copy of the names and one
/// copy of this loop instead of unrolling a `__rt_strcasecmp` call per candidate.
///
/// - ARM64: x1=name ptr, x2=name len, x3=list ptr, x4=list byte length → x0 = entry index or -1
/// - x86_64: rdi=name ptr, rsi=name len, rdx=list ptr, rcx=list byte length → rax = same
///
/// One leading `\` of the name is dropped first, as php resolves `'\Foo'` as `Foo`. Leaf routine:
/// it clobbers only caller-saved scratch and never touches the stack.
pub fn emit_name_list_find_ci(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_name_list_find_ci_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: name_list_find_ci ---");
    emitter.label_global("__rt_name_list_find_ci");
    emitter.instruction("cbz x2, __rt_name_list_find_ci_start");                // an empty name has no separator to drop
    emitter.instruction("ldrb w9, [x1]");                                       // read the first byte of the name
    emitter.instruction("cmp w9, #92");                                         // is it the namespace separator?
    emitter.instruction("b.ne __rt_name_list_find_ci_start");                   // a bare name is used as is
    emitter.instruction("add x1, x1, #1");                                      // skip the leading separator
    emitter.instruction("sub x2, x2, #1");                                      // and shorten the name by one byte
    emitter.label("__rt_name_list_find_ci_start");
    emitter.instruction("add x4, x3, x4");                                      // x4 = end of the packed list
    emitter.instruction("mov x5, #0");                                          // x5 = index of the current entry

    emitter.label("__rt_name_list_find_ci_entry");
    emitter.instruction("cmp x3, x4");                                          // every entry visited?
    emitter.instruction("b.hs __rt_name_list_find_ci_missing");                 // then the name is not in the list
    emitter.instruction("ldrh w6, [x3]");                                       // x6 = byte length of this entry
    emitter.instruction("add x3, x3, #2");                                      // x3 = first byte of this entry's name
    emitter.instruction("cmp x6, x2");                                          // only an entry of the same length can match
    emitter.instruction("b.ne __rt_name_list_find_ci_next");                    // skip it otherwise
    emitter.instruction("mov x7, #0");                                          // x7 = byte index inside the name

    emitter.label("__rt_name_list_find_ci_byte");
    emitter.instruction("cmp x7, x2");                                          // every byte equal?
    emitter.instruction("b.hs __rt_name_list_find_ci_found");                   // then this entry is the name
    emitter.instruction("ldrb w8, [x3, x7]");                                   // byte of the entry
    emitter.instruction("ldrb w9, [x1, x7]");                                   // byte of the runtime name
    emitter.instruction("sub w10, w8, #65");                                    // offset of the entry byte from 'A'
    emitter.instruction("cmp w10, #26");                                        // is the entry byte an uppercase ASCII letter?
    emitter.instruction("add w10, w8, #32");                                    // its lowercase form
    emitter.instruction("csel w8, w10, w8, lo");                                // lowercase the entry byte when it was uppercase
    emitter.instruction("sub w10, w9, #65");                                    // offset of the name byte from 'A'
    emitter.instruction("cmp w10, #26");                                        // is the name byte an uppercase ASCII letter?
    emitter.instruction("add w10, w9, #32");                                    // its lowercase form
    emitter.instruction("csel w9, w10, w9, lo");                                // lowercase the name byte when it was uppercase
    emitter.instruction("cmp w8, w9");                                          // do the lowered bytes agree?
    emitter.instruction("b.ne __rt_name_list_find_ci_next");                    // no: try the next entry
    emitter.instruction("add x7, x7, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_name_list_find_ci_byte");                       // and compare it

    emitter.label("__rt_name_list_find_ci_next");
    emitter.instruction("add x3, x3, x6");                                      // step over this entry's name bytes
    emitter.instruction("add x5, x5, #1");                                      // count the entry
    emitter.instruction("b __rt_name_list_find_ci_entry");                      // and visit the next one

    emitter.label("__rt_name_list_find_ci_found");
    emitter.instruction("mov x0, x5");                                          // return the index of the matching entry
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_name_list_find_ci_missing");
    emitter.instruction("mov x0, #-1");                                         // no entry matched
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits `__rt_name_list_entry`, which returns entry `index` of a packed name list (the layout
/// [`emit_name_list_find_ci`] walks) as a string pair. It pairs with the lookup to map a found
/// name to a value baked in a parallel list (a class name to its parent's name).
///
/// - ARM64: x0=index, x3=list ptr → x1=entry ptr, x2=entry len
/// - x86_64: rdi=index, rsi=list ptr → rax=entry ptr, rdx=entry len
///
/// The index must be in range. Leaf routine: it clobbers only caller-saved scratch.
pub fn emit_name_list_entry(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: name_list_entry ---");
    emitter.label_global("__rt_name_list_entry");
    if emitter.target.arch == Arch::X86_64 {
        emitter.label("__rt_name_list_entry_loop_x86_64");
        emitter.instruction("movzx rdx, WORD PTR [rsi]");                       // rdx = byte length of the current entry
        emitter.instruction("lea rax, [rsi + 2]");                              // rax = first byte of the current entry
        emitter.instruction("test rdi, rdi");                                   // is this the requested entry?
        emitter.instruction("jz __rt_name_list_entry_done_x86_64");             // then return it
        emitter.instruction("lea rsi, [rax + rdx]");                            // step over the current entry
        emitter.instruction("sub rdi, 1");                                      // one entry closer to the requested one
        emitter.instruction("jmp __rt_name_list_entry_loop_x86_64");            // and visit the next entry
        emitter.label("__rt_name_list_entry_done_x86_64");
        emitter.instruction("ret");                                             // return the entry's string pair
        return;
    }
    emitter.label("__rt_name_list_entry_loop");
    emitter.instruction("ldrh w2, [x3]");                                       // x2 = byte length of the current entry
    emitter.instruction("add x1, x3, #2");                                      // x1 = first byte of the current entry
    emitter.instruction("cbz x0, __rt_name_list_entry_done");                   // the requested entry: return it
    emitter.instruction("add x3, x1, x2");                                      // step over the current entry
    emitter.instruction("sub x0, x0, #1");                                      // one entry closer to the requested one
    emitter.instruction("b __rt_name_list_entry_loop");                         // and visit the next entry
    emitter.label("__rt_name_list_entry_done");
    emitter.instruction("ret");                                                 // return the entry's string pair
}

/// Emits the x86_64 variant of [`emit_name_list_find_ci`].
fn emit_name_list_find_ci_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: name_list_find_ci ---");
    emitter.label_global("__rt_name_list_find_ci");
    emitter.instruction("test rsi, rsi");                                       // an empty name has no separator to drop
    emitter.instruction("jz __rt_name_list_find_ci_start_x86_64");
    emitter.instruction("cmp BYTE PTR [rdi], 92");                              // is the first byte the namespace separator?
    emitter.instruction("jne __rt_name_list_find_ci_start_x86_64");             // a bare name is used as is
    emitter.instruction("add rdi, 1");                                          // skip the leading separator
    emitter.instruction("sub rsi, 1");                                          // and shorten the name by one byte
    emitter.label("__rt_name_list_find_ci_start_x86_64");
    emitter.instruction("add rcx, rdx");                                        // rcx = end of the packed list
    emitter.instruction("xor r8d, r8d");                                        // r8 = index of the current entry

    emitter.label("__rt_name_list_find_ci_entry_x86_64");
    emitter.instruction("cmp rdx, rcx");                                        // every entry visited?
    emitter.instruction("jae __rt_name_list_find_ci_missing_x86_64");           // then the name is not in the list
    emitter.instruction("movzx r9, WORD PTR [rdx]");                            // r9 = byte length of this entry
    emitter.instruction("add rdx, 2");                                          // rdx = first byte of this entry's name
    emitter.instruction("cmp r9, rsi");                                         // only an entry of the same length can match
    emitter.instruction("jne __rt_name_list_find_ci_next_x86_64");              // skip it otherwise
    emitter.instruction("xor r10d, r10d");                                      // r10 = byte index inside the name

    emitter.label("__rt_name_list_find_ci_byte_x86_64");
    emitter.instruction("cmp r10, rsi");                                        // every byte equal?
    emitter.instruction("jae __rt_name_list_find_ci_found_x86_64");             // then this entry is the name
    emitter.instruction("movzx eax, BYTE PTR [rdx + r10]");                     // byte of the entry
    emitter.instruction("movzx r11d, BYTE PTR [rdi + r10]");                    // byte of the runtime name
    emitter.instruction("cmp al, 65");                                          // is the entry byte at or above 'A'?
    emitter.instruction("jb __rt_name_list_find_ci_lower_x86_64");              // no: keep it
    emitter.instruction("cmp al, 90");                                          // is it at or below 'Z'?
    emitter.instruction("ja __rt_name_list_find_ci_lower_x86_64");              // no: keep it
    emitter.instruction("add al, 32");                                          // lowercase the uppercase entry byte
    emitter.label("__rt_name_list_find_ci_lower_x86_64");
    emitter.instruction("cmp r11b, 65");                                        // is the name byte at or above 'A'?
    emitter.instruction("jb __rt_name_list_find_ci_cmp_x86_64");                // no: keep it
    emitter.instruction("cmp r11b, 90");                                        // is it at or below 'Z'?
    emitter.instruction("ja __rt_name_list_find_ci_cmp_x86_64");                // no: keep it
    emitter.instruction("add r11b, 32");                                        // lowercase the uppercase name byte
    emitter.label("__rt_name_list_find_ci_cmp_x86_64");
    emitter.instruction("cmp eax, r11d");                                       // do the lowered bytes agree?
    emitter.instruction("jne __rt_name_list_find_ci_next_x86_64");              // no: try the next entry
    emitter.instruction("add r10, 1");                                          // advance to the next byte
    emitter.instruction("jmp __rt_name_list_find_ci_byte_x86_64");              // and compare it

    emitter.label("__rt_name_list_find_ci_next_x86_64");
    emitter.instruction("add rdx, r9");                                         // step over this entry's name bytes
    emitter.instruction("add r8, 1");                                           // count the entry
    emitter.instruction("jmp __rt_name_list_find_ci_entry_x86_64");             // and visit the next one

    emitter.label("__rt_name_list_find_ci_found_x86_64");
    emitter.instruction("mov rax, r8");                                         // return the index of the matching entry
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_name_list_find_ci_missing_x86_64");
    emitter.instruction("mov rax, -1");                                         // no entry matched
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the x86_64 Linux variant of `__rt_strcasecmp`.
///
/// Registers (System V AMD64 ABI):
/// - rdi = ptr_a, rsi = len_a, rdx = ptr_b, rcx = len_b → rax = signed result
///
/// Behavior is identical to the ARM64 fallback: case-insensitive lexicographic
/// comparison over the shared prefix, then by length. ASCII 'A'-'Z' are lowercased.
fn emit_strcasecmp_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcasecmp ---");
    emitter.label_global("__rt_strcasecmp");

    emitter.instruction("mov r8, rsi");                                         // seed the shared-length comparison bound from the first string length
    emitter.instruction("cmp rsi, rcx");                                        // compare both string lengths to determine the shorter shared prefix
    emitter.instruction("cmovg r8, rcx");                                       // clamp the shared-length comparison bound to the shorter string length
    emitter.instruction("xor r9d, r9d");                                        // start comparing bytes from offset zero

    emitter.label("__rt_strcasecmp_loop_linux_x86_64");
    emitter.instruction("cmp r9, r8");                                          // have we compared every byte in the shared prefix?
    emitter.instruction("jge __rt_strcasecmp_len_linux_x86_64");                // fall back to comparing string lengths once the shared prefix is exhausted
    emitter.instruction("movzx rax, BYTE PTR [rdi + r9]");                      // load the current byte from the first string
    emitter.instruction("movzx r10, BYTE PTR [rdx + r9]");                      // load the current byte from the second string
    emitter.instruction("cmp al, 65");                                          // is the first string byte an uppercase ASCII letter at or above 'A'?
    emitter.instruction("jb __rt_strcasecmp_second_linux_x86_64");              // skip lowercasing when the first string byte is below 'A'
    emitter.instruction("cmp al, 90");                                          // is the first string byte above the uppercase ASCII range?
    emitter.instruction("ja __rt_strcasecmp_second_linux_x86_64");              // skip lowercasing when the first string byte is above 'Z'
    emitter.instruction("add al, 32");                                          // lowercase the uppercase ASCII byte from the first string

    emitter.label("__rt_strcasecmp_second_linux_x86_64");
    emitter.instruction("cmp r10b, 65");                                        // is the second string byte an uppercase ASCII letter at or above 'A'?
    emitter.instruction("jb __rt_strcasecmp_cmp_linux_x86_64");                 // skip lowercasing when the second string byte is below 'A'
    emitter.instruction("cmp r10b, 90");                                        // is the second string byte above the uppercase ASCII range?
    emitter.instruction("ja __rt_strcasecmp_cmp_linux_x86_64");                 // skip lowercasing when the second string byte is above 'Z'
    emitter.instruction("add r10b, 32");                                        // lowercase the uppercase ASCII byte from the second string

    emitter.label("__rt_strcasecmp_cmp_linux_x86_64");
    emitter.instruction("cmp rax, r10");                                        // compare the lowercased bytes from both strings
    emitter.instruction("jne __rt_strcasecmp_diff_linux_x86_64");               // return the byte difference on the first mismatch
    emitter.instruction("add r9, 1");                                           // advance to the next shared-prefix byte after an equal comparison
    emitter.instruction("jmp __rt_strcasecmp_loop_linux_x86_64");               // continue comparing the remaining shared-prefix bytes

    emitter.label("__rt_strcasecmp_diff_linux_x86_64");
    emitter.instruction("sub rax, r10");                                        // return the signed lowercased-byte difference for the first mismatching character pair
    emitter.instruction("ret");                                                 // return the byte-difference result to the caller

    emitter.label("__rt_strcasecmp_len_linux_x86_64");
    emitter.instruction("mov rax, rsi");                                        // seed the length-tiebreak result from the first string length
    emitter.instruction("sub rax, rcx");                                        // return the signed length difference once the shared prefix is equal
    emitter.instruction("ret");                                                 // return the length-difference result to the caller
}
