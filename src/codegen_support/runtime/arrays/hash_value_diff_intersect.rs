//! Purpose:
//! Emits the `__rt_hash_value_diff_intersect` runtime helper behind `array_diff()` and
//! `array_intersect()`: keeps each entry of hash1, under its ORIGINAL key, whose value is absent
//! from (diff) or present in (intersect) the values of hash2.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - PHP keeps the surviving keys (`array_diff([1, 2, 3], [2])` is `[0 => 1, 2 => 3]`), so the
//!   result is a hash. An indexed operand is converted to a hash keyed `0..n-1` by the lowering
//!   before the call (`__rt_array_to_hash`), and the result keeps hash1's value_type summary.
//! - Values compare as PHP does, by string cast: `(string) $a === (string) $b`. Two ints, two
//!   bools, or two strings compare directly; every other pair is boxed and cast. A string cast
//!   hands back an owned copy while scalar casts format into the shared concat scratch, so both
//!   results go to `__rt_heap_free` (which ignores scratch) and `_concat_off` is restored after
//!   each comparison: the scan is n*m casts and must not grow the scratch.
//! - Kept values are owned by the result: strings are persisted, heap-backed values retained.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_hash_value_diff_intersect` for the current target.
///
/// Input: hash1 (`x0` / `rdi`), hash2 (`x1` / `rsi`), mode (`x2` / `rdx`: 0 = diff, 1 =
/// intersect). Output: a new owned hash (`x0` / `rax`). Both inputs are borrowed.
pub fn emit_hash_value_diff_intersect(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_value_diff_intersect_linux_x86_64(emitter);
        return;
    }

    // Frame: [0] hash1, [8] hash2, [16] result, [24] mode, [32] cursor1, [40] key lo, [48] key hi,
    // [56] value lo, [64] value hi, [72] value tag, [80] str1 ptr, [88] str1 len, [104] cursor2,
    // [112] found, [120] concat snapshot, [128] box1, [136] value2 lo, [144] value2 hi,
    // [152] value2 tag, [160] str2 ptr, [168] str2 len, [176] box2, [184] equal, [192] fp/lr.
    emitter.blank();
    emitter.comment("--- runtime: hash_value_diff_intersect ---");
    emitter.label_global("__rt_hash_value_diff_intersect");
    emitter.instruction("sub sp, sp, #208");                                    // allocate the value diff/intersect stack frame
    emitter.instruction("stp x29, x30, [sp, #192]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #192");                                   // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save hash1 pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save hash2 pointer
    emitter.instruction("str x2, [sp, #24]");                                   // save mode (0 = diff, 1 = intersect)
    emitter.instruction("ldr x0, [x0, #8]");                                    // x0 = hash1 capacity for the result hash
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload hash1 pointer
    emitter.instruction("ldr x1, [x9, #16]");                                   // x1 = hash1 value_type summary
    emitter.instruction("bl __rt_hash_new");                                    // create the result hash table, x0 = result
    emitter.instruction("str x0, [sp, #16]");                                   // save the result hash pointer
    emitter.instruction("str xzr, [sp, #32]");                                  // hash1 cursor = 0

    emitter.label("__rt_hash_value_diff_intersect_outer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = hash1 pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // x1 = hash1 cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // next hash1 entry: x0=cursor,x1=klo,x2=khi,x3=vlo,x4=vhi,x5=vtag
    emitter.instruction("cmn x0, #1");                                          // has hash1 iteration reached the end?
    emitter.instruction("b.eq __rt_hash_value_diff_intersect_done");            // stop once every hash1 entry has been visited
    emitter.instruction("str x0, [sp, #32]");                                   // save the next hash1 cursor
    emitter.instruction("str x1, [sp, #40]");                                   // save the key low word
    emitter.instruction("str x2, [sp, #48]");                                   // save the key high word
    emitter.instruction("str x3, [sp, #56]");                                   // save the hash1 value low word
    emitter.instruction("str x4, [sp, #64]");                                   // save the hash1 value high word
    emitter.instruction("str x5, [sp, #72]");                                   // save the hash1 value runtime tag
    emitter.instruction("str xzr, [sp, #112]");                                 // found = 0 until a hash2 value matches
    emitter.instruction("str xzr, [sp, #104]");                                 // hash2 cursor = 0
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // snapshot the concat cursor before any cast of this entry
    emitter.instruction("str x10, [sp, #120]");                                 // keep it to rewind the scratch after each comparison

    emitter.label("__rt_hash_value_diff_intersect_inner");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = hash2 pointer
    emitter.instruction("ldr x1, [sp, #104]");                                  // x1 = hash2 cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // next hash2 entry: x0=cursor,x3=vlo,x4=vhi,x5=vtag
    emitter.instruction("cmn x0, #1");                                          // has hash2 iteration reached the end?
    emitter.instruction("b.eq __rt_hash_value_diff_intersect_decide");          // no hash2 value matched this entry
    emitter.instruction("str x0, [sp, #104]");                                  // save the next hash2 cursor
    emitter.instruction("str x3, [sp, #136]");                                  // save the hash2 value low word
    emitter.instruction("str x4, [sp, #144]");                                  // save the hash2 value high word
    emitter.instruction("str x5, [sp, #152]");                                  // save the hash2 value runtime tag
    emitter.instruction("ldr x9, [sp, #72]");                                   // hash1 value tag
    emitter.instruction("cmp x9, x5");                                          // do both values carry the same runtime tag?
    emitter.instruction("b.ne __rt_hash_value_diff_intersect_cast");            // different kinds compare through PHP's string cast
    emitter.instruction("cmp x9, #1");                                          // two strings?
    emitter.instruction("b.eq __rt_hash_value_diff_intersect_strings");         // compare their bytes directly
    emitter.instruction("cmp x9, #0");                                          // two ints?
    emitter.instruction("b.eq __rt_hash_value_diff_intersect_words");           // equal ints have equal string casts
    emitter.instruction("cmp x9, #3");                                          // two bools?
    emitter.instruction("b.eq __rt_hash_value_diff_intersect_words");           // equal bools have equal string casts
    emitter.instruction("b __rt_hash_value_diff_intersect_cast");               // floats and the rest go through the cast

    emitter.label("__rt_hash_value_diff_intersect_words");
    emitter.instruction("ldr x10, [sp, #56]");                                  // hash1 value word
    emitter.instruction("cmp x10, x3");                                         // compare with the hash2 value word
    emitter.instruction("cset x0, eq");                                         // x0 = 1 when the values are equal
    emitter.instruction("b __rt_hash_value_diff_intersect_matched");            // record the comparison result

    emitter.label("__rt_hash_value_diff_intersect_strings");
    emitter.instruction("ldr x1, [sp, #56]");                                   // hash1 string pointer
    emitter.instruction("ldr x2, [sp, #64]");                                   // hash1 string length
    emitter.instruction("ldr x3, [sp, #136]");                                  // hash2 string pointer
    emitter.instruction("ldr x4, [sp, #144]");                                  // hash2 string length
    emitter.instruction("bl __rt_str_eq");                                      // compare the two strings byte for byte, x0 = equal
    emitter.instruction("b __rt_hash_value_diff_intersect_matched");            // record the comparison result

    emitter.label("__rt_hash_value_diff_intersect_cast");
    emitter.instruction("ldr x0, [sp, #72]");                                   // hash1 value tag
    emitter.instruction("ldr x1, [sp, #56]");                                   // hash1 value low word
    emitter.instruction("ldr x2, [sp, #64]");                                   // hash1 value high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the hash1 value, x0 = box1
    emitter.instruction("str x0, [sp, #128]");                                  // keep box1 for its release
    emitter.instruction("bl __rt_mixed_cast_string");                           // cast box1 to string: x1=ptr, x2=len
    emitter.instruction("str x1, [sp, #80]");                                   // save the hash1 string pointer
    emitter.instruction("str x2, [sp, #88]");                                   // save the hash1 string length
    emitter.instruction("ldr x0, [sp, #152]");                                  // hash2 value tag
    emitter.instruction("ldr x1, [sp, #136]");                                  // hash2 value low word
    emitter.instruction("ldr x2, [sp, #144]");                                  // hash2 value high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the hash2 value, x0 = box2
    emitter.instruction("str x0, [sp, #176]");                                  // keep box2 for its release
    emitter.instruction("bl __rt_mixed_cast_string");                           // cast box2 to string: x1=ptr, x2=len
    emitter.instruction("str x1, [sp, #160]");                                  // save the hash2 string pointer
    emitter.instruction("str x2, [sp, #168]");                                  // save the hash2 string length
    emitter.instruction("mov x3, x1");                                          // hash2 string pointer as the right operand
    emitter.instruction("mov x4, x2");                                          // hash2 string length as the right operand
    emitter.instruction("ldr x1, [sp, #80]");                                   // hash1 string pointer as the left operand
    emitter.instruction("ldr x2, [sp, #88]");                                   // hash1 string length as the left operand
    emitter.instruction("bl __rt_str_eq");                                      // compare the two cast strings, x0 = equal
    emitter.instruction("str x0, [sp, #184]");                                  // keep the result across the releases
    emitter.instruction("ldr x0, [sp, #128]");                                  // reload box1
    emitter.instruction("bl __rt_decref_mixed");                                // release the temporary hash1 box
    emitter.instruction("ldr x0, [sp, #176]");                                  // reload box2
    emitter.instruction("bl __rt_decref_mixed");                                // release the temporary hash2 box
    emitter.instruction("ldr x0, [sp, #80]");                                   // hash1 cast string (owned copy or borrowed scratch)
    emitter.instruction("bl __rt_heap_free");                                   // free an owned copy; scratch and null are ignored
    emitter.instruction("ldr x0, [sp, #160]");                                  // hash2 cast string (owned copy or borrowed scratch)
    emitter.instruction("bl __rt_heap_free");                                   // free an owned copy; scratch and null are ignored
    emitter.instruction("ldr x10, [sp, #120]");                                 // reload the entry's concat cursor snapshot
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x10, [x9]");                                       // rewind the scratch the two casts consumed
    emitter.instruction("ldr x0, [sp, #184]");                                  // x0 = the cast strings were equal

    emitter.label("__rt_hash_value_diff_intersect_matched");
    emitter.instruction("cbz x0, __rt_hash_value_diff_intersect_inner");        // keep scanning hash2 while nothing matched
    emitter.instruction("mov x9, #1");                                          // a hash2 value equals this entry's value
    emitter.instruction("str x9, [sp, #112]");                                  // found = 1

    emitter.label("__rt_hash_value_diff_intersect_decide");
    emitter.instruction("ldr x0, [sp, #112]");                                  // x0 = found
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the mode selector
    emitter.instruction("cbz x9, __rt_hash_value_diff_intersect_diff");         // mode 0 selects difference semantics
    emitter.instruction("cbz x0, __rt_hash_value_diff_intersect_outer");        // intersect drops values absent from hash2
    emitter.instruction("b __rt_hash_value_diff_intersect_keep");               // intersect keeps values present in hash2
    emitter.label("__rt_hash_value_diff_intersect_diff");
    emitter.instruction("cbnz x0, __rt_hash_value_diff_intersect_outer");       // diff drops values present in hash2

    emitter.label("__rt_hash_value_diff_intersect_keep");
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the kept value tag
    emitter.instruction("cmp x9, #1");                                          // is the kept value a string?
    emitter.instruction("b.eq __rt_hash_value_diff_intersect_persist");         // strings are persisted as an independent copy
    emitter.instruction("cmp x9, #4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("b.lt __rt_hash_value_diff_intersect_insert");          // scalar values need no retain
    emitter.instruction("cmp x9, #7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("b.gt __rt_hash_value_diff_intersect_insert");          // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #56]");                                   // load the kept heap-backed value
    emitter.instruction("bl __rt_incref");                                      // retain it for the result owner
    emitter.instruction("b __rt_hash_value_diff_intersect_insert");             // continue to the insertion
    emitter.label("__rt_hash_value_diff_intersect_persist");
    emitter.instruction("ldr x1, [sp, #56]");                                   // string pointer to persist
    emitter.instruction("ldr x2, [sp, #64]");                                   // string length to persist
    emitter.instruction("bl __rt_str_persist");                                 // copy the string into an owned heap block, x1 = new pointer
    emitter.instruction("str x1, [sp, #56]");                                   // store the persisted string pointer
    emitter.instruction("str x2, [sp, #64]");                                   // store the persisted string length
    emitter.label("__rt_hash_value_diff_intersect_insert");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result hash pointer
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the key low word
    emitter.instruction("ldr x2, [sp, #48]");                                   // reload the key high word
    emitter.instruction("ldr x3, [sp, #56]");                                   // reload the value low word
    emitter.instruction("ldr x4, [sp, #64]");                                   // reload the value high word
    emitter.instruction("ldr x5, [sp, #72]");                                   // reload the value runtime tag
    emitter.instruction("bl __rt_hash_set");                                    // insert the kept entry under its original key
    emitter.instruction("str x0, [sp, #16]");                                   // update the result pointer after possible reallocation
    emitter.instruction("b __rt_hash_value_diff_intersect_outer");              // continue with the next hash1 entry

    emitter.label("__rt_hash_value_diff_intersect_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #192]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #208");                                    // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the result hash in x0
}

/// Emits the x86_64 Linux variant of `__rt_hash_value_diff_intersect`.
///
/// Input: `rdi` = hash1, `rsi` = hash2, `rdx` = mode. Output: `rax` = new owned hash. The frame
/// mirrors the AArch64 layout at `[rbp - 8]`..`[rbp - 184]`; `__rt_decref_mixed`,
/// `__rt_heap_free`, `__rt_incref` and `__rt_str_persist` take their operand in `rax`.
fn emit_hash_value_diff_intersect_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_value_diff_intersect ---");
    emitter.label_global("__rt_hash_value_diff_intersect");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 208");                                        // reserve the spill slots, keeping calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save hash1 pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save hash2 pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save mode (0 = diff, 1 = intersect)
    emitter.instruction("mov rsi, QWORD PTR [rdi + 16]");                       // rsi = hash1 value_type summary
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // rdi = hash1 capacity for the result hash
    emitter.instruction("call __rt_hash_new");                                  // create the result hash table, rax = result
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // hash1 cursor = 0

    emitter.label("__rt_hash_value_diff_intersect_outer_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // rdi = hash1 pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // rsi = hash1 cursor
    emitter.instruction("call __rt_hash_iter_next_value");                      // next hash1 entry: rax=cursor,rdi=klo,rdx=khi,rcx=vlo,r8=vhi,r9=vtag
    emitter.instruction("cmp rax, -1");                                         // has hash1 iteration reached the end?
    emitter.instruction("je __rt_hash_value_diff_intersect_done_x86");          // stop once every hash1 entry has been visited
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the next hash1 cursor
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save the key low word
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save the key high word
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // save the hash1 value low word
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // save the hash1 value high word
    emitter.instruction("mov QWORD PTR [rbp - 80], r9");                        // save the hash1 value runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // found = 0 until a hash2 value matches
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                        // hash2 cursor = 0
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);
    emitter.instruction("mov QWORD PTR [rbp - 120], r10");                      // snapshot the concat cursor to rewind after each comparison

    emitter.label("__rt_hash_value_diff_intersect_inner_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // rdi = hash2 pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 104]");                      // rsi = hash2 cursor
    emitter.instruction("call __rt_hash_iter_next_value");                      // next hash2 entry: rax=cursor,rcx=vlo,r8=vhi,r9=vtag
    emitter.instruction("cmp rax, -1");                                         // has hash2 iteration reached the end?
    emitter.instruction("je __rt_hash_value_diff_intersect_decide_x86");        // no hash2 value matched this entry
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // save the next hash2 cursor
    emitter.instruction("mov QWORD PTR [rbp - 136], rcx");                      // save the hash2 value low word
    emitter.instruction("mov QWORD PTR [rbp - 144], r8");                       // save the hash2 value high word
    emitter.instruction("mov QWORD PTR [rbp - 152], r9");                       // save the hash2 value runtime tag
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // hash1 value tag
    emitter.instruction("cmp r10, r9");                                         // do both values carry the same runtime tag?
    emitter.instruction("jne __rt_hash_value_diff_intersect_cast_x86");         // different kinds compare through PHP's string cast
    emitter.instruction("cmp r10, 1");                                          // two strings?
    emitter.instruction("je __rt_hash_value_diff_intersect_strings_x86");       // compare their bytes directly
    emitter.instruction("cmp r10, 0");                                          // two ints?
    emitter.instruction("je __rt_hash_value_diff_intersect_words_x86");         // equal ints have equal string casts
    emitter.instruction("cmp r10, 3");                                          // two bools?
    emitter.instruction("je __rt_hash_value_diff_intersect_words_x86");         // equal bools have equal string casts
    emitter.instruction("jmp __rt_hash_value_diff_intersect_cast_x86");         // floats and the rest go through the cast

    emitter.label("__rt_hash_value_diff_intersect_words_x86");
    emitter.instruction("xor eax, eax");                                        // assume the values differ
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // hash1 value word
    emitter.instruction("cmp r10, rcx");                                        // compare with the hash2 value word
    emitter.instruction("sete al");                                             // rax = 1 when the values are equal
    emitter.instruction("jmp __rt_hash_value_diff_intersect_matched_x86");      // record the comparison result

    emitter.label("__rt_hash_value_diff_intersect_strings_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // hash1 string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // hash1 string length
    emitter.instruction("mov rdx, rcx");                                        // hash2 string pointer
    emitter.instruction("mov rcx, r8");                                         // hash2 string length
    emitter.instruction("call __rt_str_eq");                                    // compare the two strings byte for byte, rax = equal
    emitter.instruction("jmp __rt_hash_value_diff_intersect_matched_x86");      // record the comparison result

    emitter.label("__rt_hash_value_diff_intersect_cast_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // hash1 value tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // hash1 value low word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // hash1 value high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the hash1 value, rax = box1
    emitter.instruction("mov QWORD PTR [rbp - 128], rax");                      // keep box1 for its release
    emitter.instruction("mov rdi, rax");                                        // pass box1 to the string cast
    emitter.instruction("call __rt_mixed_cast_string");                         // cast box1 to string: rax=ptr, rdx=len
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // save the hash1 string pointer
    emitter.instruction("mov QWORD PTR [rbp - 96], rdx");                       // save the hash1 string length
    emitter.instruction("mov rax, QWORD PTR [rbp - 152]");                      // hash2 value tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 136]");                      // hash2 value low word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 144]");                      // hash2 value high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the hash2 value, rax = box2
    emitter.instruction("mov QWORD PTR [rbp - 176], rax");                      // keep box2 for its release
    emitter.instruction("mov rdi, rax");                                        // pass box2 to the string cast
    emitter.instruction("call __rt_mixed_cast_string");                         // cast box2 to string: rax=ptr, rdx=len
    emitter.instruction("mov QWORD PTR [rbp - 160], rax");                      // save the hash2 string pointer
    emitter.instruction("mov QWORD PTR [rbp - 168], rdx");                      // save the hash2 string length
    emitter.instruction("mov rcx, rdx");                                        // hash2 string length as the right operand
    emitter.instruction("mov rdx, rax");                                        // hash2 string pointer as the right operand
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // hash1 string pointer as the left operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 96]");                       // hash1 string length as the left operand
    emitter.instruction("call __rt_str_eq");                                    // compare the two cast strings, rax = equal
    emitter.instruction("mov QWORD PTR [rbp - 184], rax");                      // keep the result across the releases
    emitter.instruction("mov rax, QWORD PTR [rbp - 128]");                      // reload box1
    emitter.instruction("call __rt_decref_mixed");                              // release the temporary hash1 box
    emitter.instruction("mov rax, QWORD PTR [rbp - 176]");                      // reload box2
    emitter.instruction("call __rt_decref_mixed");                              // release the temporary hash2 box
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // hash1 cast string (owned copy or borrowed scratch)
    emitter.instruction("call __rt_heap_free");                                 // free an owned copy; scratch and null are ignored
    emitter.instruction("mov rax, QWORD PTR [rbp - 160]");                      // hash2 cast string (owned copy or borrowed scratch)
    emitter.instruction("call __rt_heap_free");                                 // free an owned copy; scratch and null are ignored
    emitter.instruction("mov r10, QWORD PTR [rbp - 120]");                      // reload the entry's concat cursor snapshot
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);
    emitter.instruction("mov rax, QWORD PTR [rbp - 184]");                      // rax = the cast strings were equal

    emitter.label("__rt_hash_value_diff_intersect_matched_x86");
    emitter.instruction("test rax, rax");                                       // did this hash2 value match?
    emitter.instruction("je __rt_hash_value_diff_intersect_inner_x86");         // keep scanning hash2 while nothing matched
    emitter.instruction("mov QWORD PTR [rbp - 112], 1");                        // found = 1

    emitter.label("__rt_hash_value_diff_intersect_decide_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 112]");                      // rax = found
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the mode selector
    emitter.instruction("test r10, r10");                                       // is the mode difference (0)?
    emitter.instruction("je __rt_hash_value_diff_intersect_diff_x86");          // mode 0 selects difference semantics
    emitter.instruction("test rax, rax");                                       // was the value present in hash2?
    emitter.instruction("je __rt_hash_value_diff_intersect_outer_x86");         // intersect drops values absent from hash2
    emitter.instruction("jmp __rt_hash_value_diff_intersect_keep_x86");         // intersect keeps values present in hash2
    emitter.label("__rt_hash_value_diff_intersect_diff_x86");
    emitter.instruction("test rax, rax");                                       // was the value present in hash2?
    emitter.instruction("jne __rt_hash_value_diff_intersect_outer_x86");        // diff drops values present in hash2

    emitter.label("__rt_hash_value_diff_intersect_keep_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the kept value tag
    emitter.instruction("cmp r10, 1");                                          // is the kept value a string?
    emitter.instruction("je __rt_hash_value_diff_intersect_persist_x86");       // strings are persisted as an independent copy
    emitter.instruction("cmp r10, 4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("jl __rt_hash_value_diff_intersect_insert_x86");        // scalar values need no retain
    emitter.instruction("cmp r10, 7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("jg __rt_hash_value_diff_intersect_insert_x86");        // non-heap tags need no retain
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the kept heap-backed value
    emitter.instruction("call __rt_incref");                                    // retain it for the result owner
    emitter.instruction("jmp __rt_hash_value_diff_intersect_insert_x86");       // continue to the insertion
    emitter.label("__rt_hash_value_diff_intersect_persist_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // string pointer to persist
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // string length to persist
    emitter.instruction("call __rt_str_persist");                               // copy the string into an owned heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // store the persisted string pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // store the persisted string length
    emitter.label("__rt_hash_value_diff_intersect_insert_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // rdi = result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload the key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // reload the key high word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload the value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 72]");                        // reload the value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");                        // reload the value runtime tag
    emitter.instruction("call __rt_hash_set");                                  // insert the kept entry under its original key
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // update the result pointer after possible reallocation
    emitter.instruction("jmp __rt_hash_value_diff_intersect_outer_x86");        // continue with the next hash1 entry

    emitter.label("__rt_hash_value_diff_intersect_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // rax = result hash pointer
    emitter.instruction("add rsp, 208");                                        // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result hash in rax
}
