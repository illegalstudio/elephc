//! Purpose:
//! Emits the `__rt_array_to_hash_unique` runtime helper backing `array_unique()`.
//! Converts an indexed array into an owned hash that keeps the ORIGINAL integer key of each
//! surviving element, which is what PHP returns and an indexed array cannot represent.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - PHP's `array_unique([1,2,2,3,1])` answers keys `[0, 1, 3]` — SPARSE. The previous helper
//!   pushed survivors into a fresh dense array, so `$r[2]` existed where PHP has no such key
//!   and `json_encode()` produced a JSON list where PHP produces an object.
//! - The duplicate scan looks BACKWARD over the SOURCE rather than forward over the result,
//!   because the result is no longer densely indexed and can no longer be scanned by position.
//!   The comparison is unchanged: the same 8-byte word equality the dense helper used.
//! - Payload handling is `__rt_array_to_hash_reverse`'s, slot for slot: strings are persisted
//!   into independent copies and heap-backed values retained, so the result owns its payloads.
//! - A BOXED (`Mixed`, tag 7) slot holds a CELL POINTER, so word equality compared cell
//!   addresses: `array_unique([1, "b", 1, 4])` kept both `1`s. PHP compares `array_unique`
//!   elements by their STRING rendering (the `SORT_STRING` default), which `__rt_mixed_string_eq`
//!   applies to a pair of cells — the same `__rt_mixed_cast_string` -> `__rt_str_eq` chain
//!   `__rt_assoc_diff_intersect` uses for hash values.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// array_to_hash_unique: build an owned hash of the first occurrence of each value, keyed by
/// its ORIGINAL index.
/// Input:  x0 = indexed array pointer
/// Output: x0 = new owned hash table carrying the preserved integer keys
pub fn emit_array_to_hash_unique(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_to_hash_unique_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_to_hash_unique ---");
    emitter.label_global("__rt_array_to_hash_unique");
    emitter.instruction("sub sp, sp, #96");                                     // allocate the conversion stack frame, incl. the saved scan index
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the indexed array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load the indexed array length
    emitter.instruction("str x9, [sp, #24]");                                   // save the length
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the uniform heap-kind header word
    emitter.instruction("lsr x10, x10, #8");                                    // shift the packed value_type into the low bits
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate the indexed-array value_type (also the Mixed tag)
    emitter.instruction("str x10, [sp, #32]");                                  // save the value_type / runtime tag
    emitter.instruction("ldr x11, [x0, #16]");                                  // load the element size (stride) from the header
    emitter.instruction("str x11, [sp, #40]");                                  // save the element stride
    emitter.instruction("mov x1, x10");                                         // value_type for the new hash header
    emitter.instruction("cmp x9, #8");                                          // is the length below the minimum hash capacity?
    emitter.instruction("b.ge __rt_array_to_hash_uniq_cap_ok");                 // use the length as the capacity hint
    emitter.instruction("mov x9, #8");                                          // clamp the capacity hint to a small minimum
    emitter.label("__rt_array_to_hash_uniq_cap_ok");
    emitter.instruction("mov x0, x9");                                          // capacity hint for the new hash
    emitter.instruction("bl __rt_hash_new");                                    // allocate the result hash, x0 = result
    emitter.instruction("str x0, [sp, #8]");                                    // save the result hash pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // ascending index i = 0
    emitter.label("__rt_array_to_hash_uniq_loop");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the ascending index
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the source length
    emitter.instruction("cmp x10, x11");                                        // have all source elements been visited?
    emitter.instruction("b.ge __rt_array_to_hash_uniq_done");                   // every element has been considered
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the indexed array pointer
    emitter.instruction("add x11, x11, #24");                                   // skip the 24-byte indexed-array header
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the element stride
    emitter.instruction("mul x13, x10, x12");                                   // byte offset of element[i]
    emitter.instruction("add x14, x11, x13");                                   // x14 = address of element[i]
    emitter.instruction("ldr x6, [x14]");                                       // x6 = candidate element low word
    emitter.instruction("str x6, [sp, #48]");                                   // save the candidate low word
    emitter.instruction("str x14, [sp, #56]");                                  // save the candidate slot address for the string path

    // -- a duplicate is an EARLIER equal element in the SOURCE: the result is sparse, so it
    //    can no longer be scanned by position the way the dense helper did --
    emitter.instruction("mov x7, #0");                                          // scan index j = 0
    emitter.label("__rt_array_to_hash_uniq_scan");
    emitter.instruction("cmp x7, x10");                                         // has the scan reached the candidate itself?
    emitter.instruction("b.ge __rt_array_to_hash_uniq_first");                  // no earlier element matched: this is a first occurrence
    emitter.instruction("mul x13, x7, x12");                                    // byte offset of element[j]
    emitter.instruction("add x13, x11, x13");                                   // address of element[j]
    // A string slot holds a POINTER in its low word, so comparing words compares addresses:
    // `array_unique(["a", "a"])` would keep both when the two live at different addresses. PHP
    // compares elements by their string rendering, which for a string array is byte equality.
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the value_type
    emitter.instruction("cmp x9, #1");                                          // is the element a string?
    emitter.instruction("b.eq __rt_array_to_hash_uniq_scan_string");            // strings compare by value
    emitter.instruction("cmp x9, #7");                                          // is the element a boxed Mixed cell?
    emitter.instruction("b.eq __rt_array_to_hash_uniq_scan_mixed");             // boxed cells compare by their PHP string rendering
    emitter.instruction("ldr x8, [x13]");                                       // load the earlier element low word
    emitter.instruction("cmp x8, x6");                                          // is the earlier element equal to the candidate?
    emitter.instruction("b.eq __rt_array_to_hash_uniq_skip");                   // duplicate: PHP keeps only the FIRST occurrence
    emitter.instruction("b __rt_array_to_hash_uniq_scan_next");                 // share the scan advance
    emitter.label("__rt_array_to_hash_uniq_scan_string");
    emitter.instruction("str x7, [sp, #64]");                                   // the comparison call clobbers the scan index
    emitter.instruction("ldr x14, [sp, #56]");                                  // reload the candidate slot address
    emitter.instruction("ldr x1, [x14]");                                       // candidate string pointer
    emitter.instruction("ldr x2, [x14, #8]");                                   // candidate string length
    emitter.instruction("ldr x3, [x13]");                                       // earlier string pointer
    emitter.instruction("ldr x4, [x13, #8]");                                   // earlier string length
    emitter.instruction("bl __rt_str_eq");                                      // byte equality, not address equality
    emitter.instruction("cbnz x0, __rt_array_to_hash_uniq_skip");               // duplicate: PHP keeps only the FIRST occurrence
    // Every loop register is caller-saved, so the scan state is rebuilt from the frame.
    emitter.instruction("ldr x7, [sp, #64]");                                   // scan index j
    emitter.instruction("ldr x10, [sp, #16]");                                  // candidate index i
    emitter.instruction("ldr x11, [sp, #0]");                                   // indexed array pointer
    emitter.instruction("add x11, x11, #24");                                   // skip the 24-byte indexed-array header
    emitter.instruction("ldr x12, [sp, #40]");                                  // element stride
    emitter.instruction("ldr x6, [sp, #48]");                                   // candidate element low word
    emitter.label("__rt_array_to_hash_uniq_scan_next");
    emitter.instruction("add x7, x7, #1");                                      // advance the backward scan
    emitter.instruction("b __rt_array_to_hash_uniq_scan");                      // keep scanning earlier elements

    // A boxed slot holds a CELL POINTER, so word equality compared cell ADDRESSES and
    // `array_unique([1, "b", 1, 4])` kept both `1`s. PHP compares these elements by their string
    // rendering, which is what the shared cell comparator applies.
    emitter.label("__rt_array_to_hash_uniq_scan_mixed");
    emitter.instruction("str x7, [sp, #64]");                                   // the comparison call clobbers the scan index
    emitter.instruction("ldr x0, [sp, #48]");                                   // candidate boxed Mixed cell
    emitter.instruction("ldr x1, [x13]");                                       // earlier boxed Mixed cell
    emitter.instruction("bl __rt_mixed_string_eq");                             // compare the two cells by their PHP string rendering
    emitter.instruction("cbnz x0, __rt_array_to_hash_uniq_skip");               // duplicate: PHP keeps only the FIRST occurrence
    // Every loop register is caller-saved, so the scan state is rebuilt from the frame.
    emitter.instruction("ldr x7, [sp, #64]");                                   // scan index j
    emitter.instruction("ldr x10, [sp, #16]");                                  // candidate index i
    emitter.instruction("ldr x11, [sp, #0]");                                   // indexed array pointer
    emitter.instruction("add x11, x11, #24");                                   // skip the 24-byte indexed-array header
    emitter.instruction("ldr x12, [sp, #40]");                                  // element stride
    emitter.instruction("ldr x6, [sp, #48]");                                   // candidate element low word
    emitter.instruction("b __rt_array_to_hash_uniq_scan_next");                 // share the scan advance

    emitter.label("__rt_array_to_hash_uniq_first");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the value_type
    emitter.instruction("cmp x9, #1");                                          // is the element a string?
    emitter.instruction("b.eq __rt_array_to_hash_uniq_string");                 // strings need persistence
    emitter.instruction("str xzr, [sp, #56]");                                  // non-string elements have no high word
    emitter.instruction("cmp x9, #4");                                          // is the element below the heap-backed tag range?
    emitter.instruction("b.lt __rt_array_to_hash_uniq_set");                    // scalar elements need no retain
    emitter.instruction("cmp x9, #7");                                          // is the element above the heap-backed tag range?
    emitter.instruction("b.gt __rt_array_to_hash_uniq_set");                    // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #48]");                                   // load the heap-backed element pointer
    emitter.instruction("bl __rt_incref");                                      // retain the heap-backed element for the result hash
    emitter.instruction("b __rt_array_to_hash_uniq_set");                       // continue to insertion
    emitter.label("__rt_array_to_hash_uniq_string");
    emitter.instruction("ldr x14, [sp, #56]");                                  // reload the candidate slot address
    emitter.instruction("ldr x2, [x14, #8]");                                   // load the string length from the 16-byte slot
    emitter.instruction("ldr x1, [sp, #48]");                                   // load the string pointer
    emitter.instruction("bl __rt_str_persist");                                 // copy the string into an independent heap block, x1 = new pointer
    emitter.instruction("str x1, [sp, #48]");                                   // save the persisted string pointer
    emitter.instruction("str x2, [sp, #56]");                                   // save the string length

    emitter.label("__rt_array_to_hash_uniq_set");
    emitter.instruction("ldr x0, [sp, #8]");                                    // result hash pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // integer key = the preserved source index i
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("ldr x3, [sp, #48]");                                   // value low word
    emitter.instruction("ldr x4, [sp, #56]");                                   // value high word
    emitter.instruction("ldr x5, [sp, #32]");                                   // value runtime tag (= value_type)
    emitter.instruction("bl __rt_hash_set");                                    // insert element[i] at its preserved integer key
    emitter.instruction("str x0, [sp, #8]");                                    // update the result pointer after possible reallocation

    emitter.label("__rt_array_to_hash_uniq_skip");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the ascending index
    emitter.instruction("add x10, x10, #1");                                    // advance to the next source element
    emitter.instruction("str x10, [sp, #16]");                                  // save the advanced index
    emitter.instruction("b __rt_array_to_hash_uniq_loop");                      // continue converting elements
    emitter.label("__rt_array_to_hash_uniq_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the result hash in x0

    emit_mixed_string_eq_aarch64(emitter);
}

/// mixed_string_eq: PHP's `(string)$a === (string)$b` over two BOXED Mixed cells.
/// Input:  x0 = left boxed cell, x1 = right boxed cell
/// Output: x0 = 1 when the two renderings are byte-equal, 0 otherwise
///
/// Emitted next to its only caller so the reference from `__rt_array_to_hash_unique` is what keeps
/// the atom alive; it needs no emission-list entry of its own.
///
/// Ownership follows `__rt_mixed_cast_string`'s per-tag contract: a string payload comes back as a
/// FRESH heap copy the caller owns, every other scalar tag borrows the shared concat scratch.
/// Both results are handed to `__rt_heap_free`, which by contract ignores anything that is not a
/// live heap block, and the concat cursor is restored to its entry value — without which an
/// `array_unique` of n elements would burn 21 bytes of scratch per rendering across its O(n²)
/// scan.
fn emit_mixed_string_eq_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_string_eq ---");
    emitter.label_global("__rt_mixed_string_eq");
    emitter.instruction("sub sp, sp, #64");                                     // allocate the boxed-cell comparison frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up the new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right cell across the left rendering
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the shared concat scratch cursor
    emitter.instruction("str x10, [sp, #24]");                                  // save the cursor so both renderings are reclaimed on the way out
    emitter.instruction("bl __rt_mixed_cast_string");                           // render the left cell: x1 = pointer, x2 = length
    emitter.instruction("str x1, [sp, #8]");                                    // save the left rendering pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the left rendering length
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right cell
    emitter.instruction("bl __rt_mixed_cast_string");                           // render the right cell: x1 = pointer, x2 = length
    emitter.instruction("str x1, [sp, #40]");                                   // save the right rendering pointer for its release
    emitter.instruction("mov x3, x1");                                          // pass the right rendering pointer to the byte comparator
    emitter.instruction("mov x4, x2");                                          // pass the right rendering length to the byte comparator
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the left rendering pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the left rendering length
    emitter.instruction("bl __rt_str_eq");                                      // byte equality of the two PHP string renderings
    emitter.instruction("str x0, [sp, #32]");                                   // save the answer across the releases
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the left rendering pointer
    emitter.instruction("bl __rt_heap_free");                                   // release a persisted copy; concat scratch is ignored by contract
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the right rendering pointer
    emitter.instruction("bl __rt_heap_free");                                   // release a persisted copy; concat scratch is ignored by contract
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the saved concat scratch cursor
    emitter.instruction("str x10, [x9]");                                       // hand back the scratch both renderings consumed
    emitter.instruction("ldr x0, [sp, #32]");                                   // x0 = 1 when the two renderings are equal
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the string-rendering equality answer
}

/// x86_64 Linux implementation of `__rt_array_to_hash_unique`.
/// Input:  rdi = indexed array pointer
/// Output: rax = new owned hash carrying the preserved integer keys
fn emit_array_to_hash_unique_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_to_hash_unique ---");
    emitter.label_global("__rt_array_to_hash_unique");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 80");                                         // reserve local slots for the conversion loop state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the indexed array pointer
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the indexed array length
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the length
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the uniform heap-kind header word
    emitter.instruction("shr r10, 8");                                          // shift the packed value_type into the low bits
    emitter.instruction("and r10, 127");                                        // isolate the indexed-array value_type (also the Mixed tag)
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the value_type / runtime tag
    emitter.instruction("mov r11, QWORD PTR [rdi + 16]");                       // load the element size (stride) from the header
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the element stride
    emitter.instruction("mov rsi, r10");                                        // value_type for the new hash header
    emitter.instruction("mov rdi, rax");                                        // capacity hint = length
    emitter.instruction("cmp rdi, 8");                                          // is the length below the minimum hash capacity?
    emitter.instruction("jge __rt_array_to_hash_uniq_cap_ok");                  // use the length as the capacity hint
    emitter.instruction("mov rdi, 8");                                          // clamp the capacity hint to a small minimum
    emitter.label("__rt_array_to_hash_uniq_cap_ok");
    emitter.instruction("call __rt_hash_new");                                  // allocate the result hash, rax = result
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the result hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // ascending index i = 0
    emitter.label("__rt_array_to_hash_uniq_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the ascending index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // have all source elements been visited?
    emitter.instruction("jge __rt_array_to_hash_uniq_done");                    // every element has been considered
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the indexed array pointer
    emitter.instruction("add r10, 24");                                         // skip the 24-byte indexed-array header
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the element stride
    emitter.instruction("imul r11, rax");                                       // byte offset of element[i]
    emitter.instruction("add r11, r10");                                        // r11 = address of element[i]
    emitter.instruction("mov rcx, QWORD PTR [r11]");                            // load the candidate element low word
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save the candidate low word
    emitter.instruction("mov QWORD PTR [rbp - 72], r11");                       // save the candidate slot address for the string path

    // -- a duplicate is an EARLIER equal element in the SOURCE --
    emitter.instruction("mov r9, 0");                                           // scan index j = 0
    emitter.label("__rt_array_to_hash_uniq_scan");
    emitter.instruction("cmp r9, rax");                                         // has the scan reached the candidate itself?
    emitter.instruction("jge __rt_array_to_hash_uniq_first");                   // no earlier element matched: first occurrence
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the element stride
    emitter.instruction("imul r11, r9");                                        // byte offset of element[j]
    emitter.instruction("add r11, r10");                                        // address of element[j]
    // See the ARM64 path: a string slot's low word is a POINTER, so word equality would keep two
    // equal strings that live at different addresses.
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the value_type
    emitter.instruction("cmp rdx, 1");                                          // is the element a string?
    emitter.instruction("je __rt_array_to_hash_uniq_scan_string");              // strings compare by value
    emitter.instruction("cmp rdx, 7");                                          // is the element a boxed Mixed cell?
    emitter.instruction("je __rt_array_to_hash_uniq_scan_mixed");               // boxed cells compare by their PHP string rendering
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // load the earlier element low word
    emitter.instruction("cmp rdx, rcx");                                        // is the earlier element equal to the candidate?
    emitter.instruction("je __rt_array_to_hash_uniq_skip");                     // duplicate: PHP keeps only the FIRST occurrence
    emitter.instruction("jmp __rt_array_to_hash_uniq_scan_next");               // share the scan advance
    emitter.label("__rt_array_to_hash_uniq_scan_string");
    emitter.instruction("mov QWORD PTR [rbp - 80], r9");                        // the comparison call clobbers the scan index
    emitter.instruction("mov r8, QWORD PTR [rbp - 72]");                        // reload the candidate slot address
    emitter.instruction("mov rdi, QWORD PTR [r8]");                             // candidate string pointer
    emitter.instruction("mov rsi, QWORD PTR [r8 + 8]");                         // candidate string length
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // earlier string pointer
    emitter.instruction("mov rcx, QWORD PTR [r11 + 8]");                        // earlier string length
    emitter.instruction("call __rt_str_eq");                                    // byte equality, not address equality
    emitter.instruction("test rax, rax");                                       // did the two strings match?
    emitter.instruction("jne __rt_array_to_hash_uniq_skip");                    // duplicate: PHP keeps only the FIRST occurrence
    // Every loop register is caller-saved, so the scan state is rebuilt from the frame.
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");                        // scan index j
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // candidate index i
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // indexed array pointer
    emitter.instruction("add r10, 24");                                         // skip the 24-byte indexed-array header
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // candidate element low word
    emitter.label("__rt_array_to_hash_uniq_scan_next");
    emitter.instruction("add r9, 1");                                           // advance the backward scan
    emitter.instruction("jmp __rt_array_to_hash_uniq_scan");                    // keep scanning earlier elements

    // See the ARM64 path: a boxed slot's low word is a CELL POINTER, so word equality compared
    // cell addresses instead of the values PHP renders and compares.
    emitter.label("__rt_array_to_hash_uniq_scan_mixed");
    emitter.instruction("mov QWORD PTR [rbp - 80], r9");                        // the comparison call clobbers the scan index
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // earlier boxed Mixed cell
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // candidate boxed Mixed cell
    emitter.instruction("call __rt_mixed_string_eq");                           // compare the two cells by their PHP string rendering
    emitter.instruction("test rax, rax");                                       // did the two renderings match?
    emitter.instruction("jne __rt_array_to_hash_uniq_skip");                    // duplicate: PHP keeps only the FIRST occurrence
    // Every loop register is caller-saved, so the scan state is rebuilt from the frame.
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");                        // scan index j
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // candidate index i
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // indexed array pointer
    emitter.instruction("add r10, 24");                                         // skip the 24-byte indexed-array header
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // candidate element low word
    emitter.instruction("jmp __rt_array_to_hash_uniq_scan_next");               // share the scan advance

    emitter.label("__rt_array_to_hash_uniq_first");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the value_type
    emitter.instruction("cmp r9, 1");                                           // is the element a string?
    emitter.instruction("je __rt_array_to_hash_uniq_string");                   // strings need persistence
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // non-string elements have no high word
    emitter.instruction("cmp r9, 4");                                           // is the element below the heap-backed tag range?
    emitter.instruction("jl __rt_array_to_hash_uniq_set");                      // scalar elements need no retain
    emitter.instruction("cmp r9, 7");                                           // is the element above the heap-backed tag range?
    emitter.instruction("jg __rt_array_to_hash_uniq_set");                      // non-heap tags need no retain
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // load the heap-backed element pointer into the incref input register
    emitter.instruction("call __rt_incref");                                    // retain the heap-backed element for the result hash
    emitter.instruction("jmp __rt_array_to_hash_uniq_set");                     // continue to insertion
    emitter.label("__rt_array_to_hash_uniq_string");
    emitter.instruction("mov r11, QWORD PTR [rbp - 72]");                       // reload the candidate slot address
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                        // load the string length from the 16-byte slot
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // load the string pointer
    emitter.instruction("call __rt_str_persist");                               // copy the string into an independent heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the persisted string pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the string length

    emitter.label("__rt_array_to_hash_uniq_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // integer key = the preserved source index i
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // value runtime tag (= value_type)
    emitter.instruction("call __rt_hash_set");                                  // insert element[i] at its preserved integer key
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // update the result pointer after possible reallocation

    emitter.label("__rt_array_to_hash_uniq_skip");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the ascending index
    emitter.instruction("add rax, 1");                                          // advance to the next source element
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the advanced index
    emitter.instruction("jmp __rt_array_to_hash_uniq_loop");                    // continue converting elements
    emitter.label("__rt_array_to_hash_uniq_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // rax = result hash pointer
    emitter.instruction("add rsp, 80");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result hash in rax

    emit_mixed_string_eq_x86_64(emitter);
}

/// x86_64 variant of `__rt_mixed_string_eq`.
/// Input:  rax = left boxed cell, rdi = right boxed cell
/// Output: rax = 1 when the two PHP string renderings are byte-equal, 0 otherwise
///
/// `__rt_mixed_cast_string` reads its argument from RAX on this target (it hands the register
/// straight to `__rt_mixed_unbox`), and `__rt_heap_free` reads RAX too, which is why neither takes
/// RDI here. Ownership and the concat-cursor restore match the ARM64 emitter exactly.
fn emit_mixed_string_eq_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_string_eq ---");
    emitter.label_global("__rt_mixed_string_eq");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve the boxed-cell comparison slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the right cell across the left rendering
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the shared concat scratch cursor
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // save the cursor so both renderings are reclaimed on the way out
    emitter.instruction("call __rt_mixed_cast_string");                         // render the left cell: rax = pointer, rdx = length
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the left rendering pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the left rendering length
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right cell into the cast helper's input register
    emitter.instruction("call __rt_mixed_cast_string");                         // render the right cell: rax = pointer, rdx = length
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the right rendering pointer for its release
    emitter.instruction("mov rcx, rdx");                                        // pass the right rendering length to the byte comparator
    emitter.instruction("mov rdx, rax");                                        // pass the right rendering pointer to the byte comparator
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the left rendering pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the left rendering length
    emitter.instruction("call __rt_str_eq");                                    // byte equality of the two PHP string renderings
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the answer across the releases
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the left rendering pointer
    emitter.instruction("call __rt_heap_free");                                 // release a persisted copy; concat scratch is ignored by contract
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the right rendering pointer
    emitter.instruction("call __rt_heap_free");                                 // release a persisted copy; concat scratch is ignored by contract
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the saved concat scratch cursor
    emitter.instruction("mov QWORD PTR [r8], r9");                              // hand back the scratch both renderings consumed
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // rax = 1 when the two renderings are equal
    emitter.instruction("add rsp, 64");                                         // release the comparison slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the string-rendering equality answer
}
