//! Purpose:
//! Emits the `__rt_mixed_strict_eq`, `__rt_mixed_unbox` runtime helper assembly for mixed strict eq.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Mixed helpers use boxed tag/payload cells; tag constants and ownership rules are shared with type checking and codegen.
//! - Indexed arrays compare structurally in key order; typed slots are boxed temporarily so
//!   recursive Mixed comparison applies the same strict type/value rules at every depth.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Compares two boxed mixed values for strict equality using runtime tag and payload dispatch.
///
/// dispatches to `emit_mixed_strict_eq_linux_x86_64` on x86_64, otherwise uses ARM64 SysV ABI.
/// Saves both operand pointers and calls `__rt_mixed_unbox` on each to extract runtime tags.
/// If tags match, dispatches on the shared tag: scalar/pointer payloads compare word-for-word,
/// strings delegate to `__rt_str_eq`, and indexed arrays compare length and ordered elements.
/// Array elements are read as owned temporary Mixed cells, compared recursively, then released.
/// Returns 1 in `x0` (ARM64) or `rax` (x86_64) if strictly equal, 0 otherwise.
/// Clobbers: x0–x12, lr. Preserves: x29 (frame pointer).
pub fn emit_mixed_strict_eq(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_strict_eq_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_strict_eq ---");
    emitter.label_global("__rt_mixed_strict_eq");

    // -- save both mixed operands across helper calls --
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack space for both operands, payloads, and saved frame state
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper stack frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save the incoming left/right mixed pointers

    // -- unbox the left payload --
    emitter.instruction("bl __rt_mixed_unbox");                                 // left mixed pointer -> x0=tag, x1=value_lo, x2=value_hi
    emitter.instruction("str x0, [sp, #16]");                                   // save the left runtime tag
    emitter.instruction("stp x1, x2, [sp, #24]");                               // save the left payload words

    // -- unbox the right payload --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right mixed pointer into the helper argument register
    emitter.instruction("bl __rt_mixed_unbox");                                 // right mixed pointer -> x0=tag, x1=value_lo, x2=value_hi
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the saved left runtime tag

    // -- array/hash payloads (tags 4 and 5) compare by deep structure, not pointer identity, and
    //    a packed indexed array (tag 4) may be structurally equal to a hash (tag 5) --
    emitter.instruction("sub x10, x9, #4");                                     // is the left tag an array-ish tag (4 or 5)?
    emitter.instruction("cmp x10, #1");                                         // fold tags 4 and 5 into the range 0..1
    emitter.instruction("b.hi __rt_mixed_strict_eq_tag_gate");                  // non-array left payloads take the strict-tag path
    emitter.instruction("sub x11, x0, #4");                                     // is the right tag also array-ish (4 or 5)?
    emitter.instruction("cmp x11, #1");                                         // fold the right tag into the range 0..1
    emitter.instruction("b.hi __rt_mixed_strict_eq_false");                     // an array is never strictly equal to a non-array
    emitter.instruction("ldr x0, [sp, #24]");                                   // left array/hash pointer (x1 already holds the right one)
    emitter.instruction("bl __rt_array_strict_eq");                             // deep structural comparison of the two arrays
    emitter.instruction("b __rt_mixed_strict_eq_done");                         // return the structural-equality result

    // -- dispatch on the shared concrete runtime tag --
    emitter.label("__rt_mixed_strict_eq_tag_gate");
    emitter.instruction("cmp x9, x0");                                          // strict equality first requires matching runtime tags
    emitter.instruction("b.ne __rt_mixed_strict_eq_false");                     // different payload tags are never strictly equal
    emitter.instruction("cmp x0, #8");                                          // do both payloads represent PHP null?
    emitter.instruction("b.eq __rt_mixed_strict_eq_true");                      // null identity depends only on the matching runtime tag
    emitter.instruction("cmp x0, #1");                                          // do both payloads hold strings?
    emitter.instruction("b.eq __rt_mixed_strict_eq_string");                    // strings need byte-by-byte comparison
    emitter.instruction("cmp x0, #4");                                          // do both payloads hold indexed arrays?
    emitter.instruction("b.eq __rt_mixed_strict_eq_array");                     // indexed arrays compare ordered elements structurally
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the left payload low word
    emitter.instruction("cmp x10, x1");                                         // compare low payload words for scalar/pointer tags
    emitter.instruction("b.ne __rt_mixed_strict_eq_false");                     // mismatched payload low words are not equal
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the left payload high word
    emitter.instruction("cmp x11, x2");                                         // compare high payload words for string/null padding
    emitter.instruction("b.ne __rt_mixed_strict_eq_false");                     // mismatched payload high words are not equal

    emitter.label("__rt_mixed_strict_eq_true");
    emitter.instruction("mov x0, #1");                                          // report strict equality after null or payload identity matched
    emitter.instruction("b __rt_mixed_strict_eq_done");                         // return true after the matching-tag comparison path

    // -- strings compare by bytes, not by pointer identity --
    emitter.label("__rt_mixed_strict_eq_string");
    emitter.instruction("mov x3, x1");                                          // move right string pointer into the third string-equality argument slot
    emitter.instruction("mov x4, x2");                                          // move right string length into the fourth string-equality argument slot
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // reload the left string pointer/length into the first two argument slots
    emitter.instruction("bl __rt_str_eq");                                      // compare the two string payloads byte-for-byte
    emitter.instruction("b __rt_mixed_strict_eq_done");                         // return the string comparison result

    // -- indexed arrays compare ordered values recursively --
    emitter.label("__rt_mixed_strict_eq_array");
    emitter.instruction("str x1, [sp, #32]");                                   // save the right indexed-array pointer over the unused left high word
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left indexed-array pointer
    emitter.instruction("cmp x9, x1");                                          // identical containers are necessarily structurally identical
    emitter.instruction("b.eq __rt_mixed_strict_eq_true");                      // avoid allocating temporary element boxes for shared storage
    emitter.instruction("ldr x10, [x9]");                                       // load the left indexed-array logical length
    emitter.instruction("ldr x11, [x1]");                                       // load the right indexed-array logical length
    emitter.instruction("cmp x10, x11");                                        // strict array identity requires the same number of ordered entries
    emitter.instruction("b.ne __rt_mixed_strict_eq_false");                     // unequal lengths cannot represent identical indexed arrays
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize the ordered element cursor

    emitter.label("__rt_mixed_strict_eq_array_loop");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the current ordered element index
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the left indexed-array pointer across recursive calls
    emitter.instruction("ldr x11, [x10]");                                      // reload the common logical length
    emitter.instruction("cmp x9, x11");                                         // have all ordered elements compared strictly equal?
    emitter.instruction("b.ge __rt_mixed_strict_eq_true");                      // equal lengths and elements make the arrays strictly identical
    emitter.instruction("mov x0, x10");                                         // pass the left indexed array to the uniform Mixed element reader
    emitter.instruction("mov x1, x9");                                          // pass the current integer key
    emitter.instruction("mov x2, #-1");                                         // normalized integer keys use high-word sentinel -1
    emitter.instruction("mov x3, #0");                                          // valid internal reads must not emit missing-key warnings
    emitter.instruction("bl __rt_array_get_mixed_key");                         // return an owned boxed Mixed cell for the left element
    emitter.instruction("str x0, [sp, #0]");                                    // save the owned left element across the right read
    emitter.instruction("ldr x0, [sp, #32]");                                   // pass the right indexed array to the same element reader
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the current integer key
    emitter.instruction("mov x2, #-1");                                         // normalized integer keys use high-word sentinel -1
    emitter.instruction("mov x3, #0");                                          // valid internal reads must not emit missing-key warnings
    emitter.instruction("bl __rt_array_get_mixed_key");                         // return an owned boxed Mixed cell for the right element
    emitter.instruction("str x0, [sp, #8]");                                    // save the owned right element for comparison and cleanup
    emitter.instruction("ldr x0, [sp, #0]");                                    // load the left boxed element as recursive operand one
    emitter.instruction("ldr x1, [sp, #8]");                                    // load the right boxed element as recursive operand two
    emitter.instruction("bl __rt_mixed_strict_eq");                             // recursively compare element type, value, and nested indexed arrays
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the recursive result across temporary releases
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the owned left temporary Mixed cell
    emitter.instruction("bl __rt_decref_mixed");                                // release the left temporary and any payload it owns
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the owned right temporary Mixed cell
    emitter.instruction("bl __rt_decref_mixed");                                // release the right temporary and any payload it owns
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the recursive strict-equality result
    emitter.instruction("cbz x9, __rt_mixed_strict_eq_false");                  // stop at the first value or type mismatch
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the ordered element cursor after helper calls
    emitter.instruction("add x9, x9, #1");                                      // advance to the next integer key
    emitter.instruction("str x9, [sp, #40]");                                   // preserve the next cursor across recursive comparison
    emitter.instruction("b __rt_mixed_strict_eq_array_loop");                   // continue until every ordered element matches

    emitter.label("__rt_mixed_strict_eq_false");
    emitter.instruction("mov x0, #0");                                          // report that the mixed payloads are not strictly equal

    emitter.label("__rt_mixed_strict_eq_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the strict-equality boolean in x0
}

/// x86_64 Linux implementation of mixed strict equality comparison.
///
/// Uses System V AMD64 ABI: left mixed pointer in `rdi`, right in `rsi`.
/// Saves both operands on the stack, calls `__rt_mixed_unbox` on each, then compares
/// tags and payloads. String payloads delegate to `__rt_str_eq`; indexed arrays use ordered,
/// recursive element comparison through temporary Mixed cells. Returns boolean in `rax`.
/// Clobbers: rax, rcx, rdx, rdi, rsi, r10, r11. Preserves: rbx, rbp, r12–r15.
fn emit_mixed_strict_eq_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_strict_eq ---");
    emitter.label_global("__rt_mixed_strict_eq");

    emitter.instruction("push rbp");                                            // preserve rbp and realign rsp so every helper call is 16-byte aligned
    emitter.instruction("sub rsp, 64");                                         // allocate stack space for both operands, payloads, and the saved comparison state
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // save the incoming left mixed pointer for the later comparison and cleanup path
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // save the incoming right mixed pointer for the later comparison and cleanup path

    emitter.instruction("mov rax, rdi");                                        // move the left mixed pointer into the x86_64 mixed-unbox input register
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // left mixed pointer -> rax=tag, rdi=value_lo, rdx=value_hi
    emitter.instruction("mov QWORD PTR [rsp + 16], rax");                       // save the left runtime tag
    emitter.instruction("mov QWORD PTR [rsp + 24], rdi");                       // save the left payload low word
    emitter.instruction("mov QWORD PTR [rsp + 32], rdx");                       // save the left payload high word

    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // reload the right mixed pointer into the x86_64 mixed-unbox input register
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // right mixed pointer -> rax=tag, rdi=value_lo, rdx=value_hi
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // reload the saved left runtime tag

    // -- array/hash payloads (tags 4 and 5) compare by deep structure, not pointer identity, and
    //    a packed indexed array (tag 4) may be structurally equal to a hash (tag 5) --
    emitter.instruction("mov r11, r10");                                        // copy the left tag to test the array-ish range
    emitter.instruction("sub r11, 4");                                          // is the left tag an array-ish tag (4 or 5)?
    emitter.instruction("cmp r11, 1");                                          // fold tags 4 and 5 into the range 0..1
    emitter.instruction("ja __rt_mixed_strict_eq_tag_gate");                    // non-array left payloads take the strict-tag path
    emitter.instruction("mov r11, rax");                                        // copy the right tag to test the array-ish range
    emitter.instruction("sub r11, 4");                                          // is the right tag also array-ish (4 or 5)?
    emitter.instruction("cmp r11, 1");                                          // fold the right tag into the range 0..1
    emitter.instruction("ja __rt_mixed_strict_eq_false");                       // an array is never strictly equal to a non-array
    emitter.instruction("mov rsi, rdi");                                        // right array/hash pointer into the second argument
    emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");                       // left array/hash pointer into the first argument
    emitter.instruction("call __rt_array_strict_eq");                           // deep structural comparison of the two arrays
    emitter.instruction("jmp __rt_mixed_strict_eq_done");                       // return the structural-equality result

    emitter.label("__rt_mixed_strict_eq_tag_gate");
    emitter.instruction("cmp r10, rax");                                        // strict equality first requires matching runtime tags
    emitter.instruction("jne __rt_mixed_strict_eq_false");                      // different payload tags are never strictly equal
    emitter.instruction("cmp rax, 8");                                          // do both payloads represent PHP null?
    emitter.instruction("je __rt_mixed_strict_eq_true");                        // null identity depends only on the matching runtime tag
    emitter.instruction("cmp rax, 1");                                          // do both payloads hold strings?
    emitter.instruction("je __rt_mixed_strict_eq_string");                      // strings need byte-by-byte comparison
    emitter.instruction("cmp rax, 4");                                          // do both payloads hold indexed arrays?
    emitter.instruction("je __rt_mixed_strict_eq_array");                       // indexed arrays compare ordered elements structurally
    emitter.instruction("cmp QWORD PTR [rsp + 24], rdi");                       // compare low payload words for scalar or pointer tags
    emitter.instruction("jne __rt_mixed_strict_eq_false");                      // mismatched payload low words are not equal
    emitter.instruction("cmp QWORD PTR [rsp + 32], rdx");                       // compare high payload words for string/null padding
    emitter.instruction("jne __rt_mixed_strict_eq_false");                      // mismatched payload high words are not equal

    emitter.label("__rt_mixed_strict_eq_true");
    emitter.instruction("mov rax, 1");                                          // report strict equality after null or payload identity matched
    emitter.instruction("jmp __rt_mixed_strict_eq_done");                       // return true after the matching-tag comparison path

    emitter.label("__rt_mixed_strict_eq_string");
    emitter.instruction("mov rcx, rdx");                                        // move the right string length into the fourth SysV integer argument register
    emitter.instruction("mov rdx, rdi");                                        // move the right string pointer into the third SysV integer argument register
    emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");                       // reload the left string pointer into the first SysV integer argument register
    emitter.instruction("mov rsi, QWORD PTR [rsp + 32]");                       // reload the left string length into the second SysV integer argument register
    abi::emit_call_label(emitter, "__rt_str_eq");                               // compare the two string payloads byte-by-byte
    emitter.instruction("jmp __rt_mixed_strict_eq_done");                       // return the string comparison result

    emitter.label("__rt_mixed_strict_eq_array");
    emitter.instruction("mov QWORD PTR [rsp + 40], rdi");                       // save the right indexed-array pointer over unused comparison storage
    emitter.instruction("mov r10, QWORD PTR [rsp + 24]");                       // reload the left indexed-array pointer
    emitter.instruction("cmp r10, rdi");                                        // identical containers are necessarily structurally identical
    emitter.instruction("je __rt_mixed_strict_eq_true");                        // avoid allocating temporary element boxes for shared storage
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the left indexed-array logical length
    emitter.instruction("cmp r11, QWORD PTR [rdi]");                            // strict array identity requires the same number of ordered entries
    emitter.instruction("jne __rt_mixed_strict_eq_false");                      // unequal lengths cannot represent identical indexed arrays
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // initialize the ordered element cursor

    emitter.label("__rt_mixed_strict_eq_array_loop");
    emitter.instruction("mov r10, QWORD PTR [rsp + 24]");                       // reload the left indexed-array pointer across recursive calls
    emitter.instruction("mov r11, QWORD PTR [rsp + 48]");                       // reload the current ordered element index
    emitter.instruction("cmp r11, QWORD PTR [r10]");                            // have all ordered elements compared strictly equal?
    emitter.instruction("jge __rt_mixed_strict_eq_true");                       // equal lengths and elements make the arrays strictly identical
    emitter.instruction("mov rdi, r10");                                        // pass the left indexed array to the uniform Mixed element reader
    emitter.instruction("mov rsi, r11");                                        // pass the current integer key
    emitter.instruction("mov rdx, -1");                                         // normalized integer keys use high-word sentinel -1
    emitter.instruction("xor ecx, ecx");                                        // valid internal reads must not emit missing-key warnings
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");                  // return an owned boxed Mixed cell for the left element
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // save the owned left element across the right read
    emitter.instruction("mov rdi, QWORD PTR [rsp + 40]");                       // pass the right indexed array to the same element reader
    emitter.instruction("mov rsi, QWORD PTR [rsp + 48]");                       // reload the current integer key
    emitter.instruction("mov rdx, -1");                                         // normalized integer keys use high-word sentinel -1
    emitter.instruction("xor ecx, ecx");                                        // valid internal reads must not emit missing-key warnings
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");                  // return an owned boxed Mixed cell for the right element
    emitter.instruction("mov QWORD PTR [rsp + 8], rax");                        // save the owned right element for comparison and cleanup
    emitter.instruction("mov rdi, QWORD PTR [rsp]");                            // load the left boxed element as recursive operand one
    emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                        // load the right boxed element as recursive operand two
    abi::emit_call_label(emitter, "__rt_mixed_strict_eq");                      // recursively compare element type, value, and nested indexed arrays
    emitter.instruction("mov QWORD PTR [rsp + 16], rax");                       // preserve the recursive result across temporary releases
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // reload the owned left temporary Mixed cell
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the left temporary and any payload it owns
    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // reload the owned right temporary Mixed cell
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the right temporary and any payload it owns
    emitter.instruction("cmp QWORD PTR [rsp + 16], 0");                         // did the recursive comparison find an exact type/value match?
    emitter.instruction("je __rt_mixed_strict_eq_false");                       // stop at the first value or type mismatch
    emitter.instruction("add QWORD PTR [rsp + 48], 1");                         // advance to the next ordered integer key
    emitter.instruction("jmp __rt_mixed_strict_eq_array_loop");                 // continue until every ordered element matches

    emitter.label("__rt_mixed_strict_eq_false");
    emitter.instruction("xor rax, rax");                                        // report that the mixed payloads are not strictly equal

    emitter.label("__rt_mixed_strict_eq_done");
    emitter.instruction("add rsp, 64");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer saved for stack alignment
    emitter.instruction("ret");                                                 // return the strict-equality boolean in rax
}
