//! Purpose:
//! Emits the `__rt_array_to_pointer_words` runtime helper, which turns an indexed array that
//! reached a pointer-array slot with boxed Mixed slots back into raw `?ptr<T>` words.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - `crate::codegen::lower_inst::arrays::lower_array_to_pointer_words` for `Op::ArrayToPointerWords`.
//! - The Mixed-to-array unbox in `crate::codegen::lower_inst::array_access_runtime` when the
//!   contract is a pointer-element array (`ReflectionProperty::setValue()` and similar).
//! - Eval static-property stores in `crate::codegen::eval_static_property_helpers`.
//!
//! Key details:
//! - Runtime bridges (the callable descriptor invoker, eval argument binding and writebacks,
//!   reflection setters) hand pointer-array storage a PHP array whose slots are boxed Mixed
//!   cells. That storage must hold raw words, because typed reads, `isset()` and writes treat
//!   each slot as the pointer itself.
//! - Only arrays whose header value type is 7 (Mixed) are converted. Pointer arrays (13), empty
//!   arrays and null containers return unchanged.
//! - The helper consumes one owned reference and returns an owned array: a shared array is split
//!   by `__rt_array_ensure_unique` first, so the caller's array keeps its boxed slots.
//! - Each cell is read through `__rt_mixed_unbox`, so a reference element (`[&$addr]`) yields
//!   the referenced value: null becomes the null pointer word 0 and an integer its address. The
//!   slot's own cell is released once its word is stored. The header is then stamped `POINTER_ARRAY_VALUE_TYPE`.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::NULL_SENTINEL;
use crate::types::PhpType;

/// array_to_pointer_words: convert a Mixed-slot indexed array into raw pointer words.
/// Input:  x0 / rdi = owned indexed-array pointer
/// Output: x0 / rax = owned indexed array holding raw pointer words
pub fn emit_array_to_pointer_words(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_to_pointer_words_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_to_pointer_words ---");
    emitter.label_global("__rt_array_to_pointer_words");

    // -- only a real array with boxed Mixed slots needs conversion --
    emitter.instruction("cbz x0, __rt_array_to_pointer_words_passthrough");     // null containers have no header or slots
    abi::emit_load_int_immediate(emitter, "x9", NULL_SENTINEL);
    emitter.instruction("cmp x0, x9");                                          // is this the in-band null-container sentinel?
    emitter.instruction("b.eq __rt_array_to_pointer_words_passthrough");        // missed-read sentinels pass through unconverted
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed kind word from the heap header
    emitter.instruction("lsr x9, x9, #8");                                      // move the runtime value_type into the low bits
    emitter.instruction("and x9, x9, #0x7f");                                   // isolate the value_type, dropping the COW bit
    emitter.instruction("cmp x9, #7");                                          // do the slots hold boxed Mixed cells?
    emitter.instruction("b.ne __rt_array_to_pointer_words_passthrough");        // pointer words and empty arrays are already in shape

    // -- frame: [sp] array, [sp+8] length, [sp+16] index, [sp+24] pending word, [sp+32] cell --
    emitter.instruction("sub sp, sp, #64");                                     // reserve the conversion frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the conversion frame
    emitter.instruction("bl __rt_array_ensure_unique");                         // split a shared array so the caller keeps its boxed slots
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load the logical array length
    emitter.instruction("str x9, [sp, #8]");                                    // save the length across cell releases
    emitter.instruction("str xzr, [sp, #16]");                                  // start at element 0

    emitter.label("__rt_array_to_pointer_words_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the element index
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the logical length
    emitter.instruction("cmp x9, x10");                                         // has every slot been converted?
    emitter.instruction("b.ge __rt_array_to_pointer_words_stamp");              // stamp the header once the slots hold words
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the unique array pointer
    emitter.instruction("add x12, x11, #24");                                   // skip the indexed-array header
    emitter.instruction("ldr x0, [x12, x9, lsl #3]");                           // load the boxed Mixed cell of this slot
    emitter.instruction("mov x13, #0");                                         // default word = the null pointer
    emitter.instruction("cbz x0, __rt_array_to_pointer_words_store");           // an empty slot stays the null pointer
    emitter.instruction("str x0, [sp, #32]");                                   // keep the slot's own cell for the release
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel nested and reference wrappers: x0 = tag, x1 = payload
    emitter.instruction("cmp x0, #0");                                          // is it an integer, the boxed form of a live address?
    emitter.instruction("csel x13, x1, xzr, eq");                               // the integer payload is the word; null and anything else are 0
    emitter.instruction("str x13, [sp, #24]");                                  // keep the word across the cell release
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the slot's own cell
    emitter.instruction("bl __rt_decref_mixed");                                // release the cell the slot owned
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the converted word
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the element index
    emitter.label("__rt_array_to_pointer_words_store");
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the unique array pointer
    emitter.instruction("add x12, x11, #24");                                   // skip the indexed-array header
    emitter.instruction("str x13, [x12, x9, lsl #3]");                          // replace the cell with the raw pointer word
    emitter.instruction("add x9, x9, #1");                                      // advance to the next slot
    emitter.instruction("str x9, [sp, #16]");                                   // persist the index across releases
    emitter.instruction("b __rt_array_to_pointer_words_loop");                  // convert the next slot

    emitter.label("__rt_array_to_pointer_words_stamp");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the converted array
    crate::codegen_support::arrays::emit_array_value_type_stamp(emitter, "x0", &PhpType::Pointer(None));
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the conversion frame
    emitter.instruction("ret");                                                 // x0 = owned array of raw pointer words

    emitter.label("__rt_array_to_pointer_words_passthrough");
    emitter.instruction("ret");                                                 // x0 = the unchanged owned array
}

/// x86_64 Linux implementation of `__rt_array_to_pointer_words`.
/// Input:  rdi = owned indexed-array pointer
/// Output: rax = owned indexed array holding raw pointer words
fn emit_array_to_pointer_words_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_to_pointer_words ---");
    emitter.label_global("__rt_array_to_pointer_words");

    // -- only a real array with boxed Mixed slots needs conversion --
    emitter.instruction("mov rax, rdi");                                        // default to returning the array unchanged
    emitter.instruction("test rdi, rdi");                                       // null containers have no header or slots
    emitter.instruction("jz __rt_array_to_pointer_words_passthrough");          // return the null container as is
    abi::emit_load_int_immediate(emitter, "r10", NULL_SENTINEL);
    emitter.instruction("cmp rdi, r10");                                        // is this the in-band null-container sentinel?
    emitter.instruction("je __rt_array_to_pointer_words_passthrough");          // missed-read sentinels pass through unconverted
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the packed kind word from the heap header
    emitter.instruction("shr r10, 8");                                          // move the runtime value_type into the low bits
    emitter.instruction("and r10, 0x7f");                                       // isolate the value_type, dropping the COW bit
    emitter.instruction("cmp r10, 7");                                          // do the slots hold boxed Mixed cells?
    emitter.instruction("jne __rt_array_to_pointer_words_passthrough");         // pointer words and empty arrays are already in shape

    // -- frame: [rbp-8] array, [rbp-16] length, [rbp-24] index, [rbp-32] pending word, [rbp-40] cell --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the conversion frame
    emitter.instruction("sub rsp, 48");                                         // reserve the frame slots, keeping calls 16-byte aligned
    emitter.instruction("call __rt_array_ensure_unique");                       // split a shared array so the caller keeps its boxed slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the unique array pointer
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the logical array length
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the length across cell releases
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // start at element 0

    emitter.label("__rt_array_to_pointer_words_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the element index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // has every slot been converted?
    emitter.instruction("jge __rt_array_to_pointer_words_stamp");               // stamp the header once the slots hold words
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the unique array pointer
    emitter.instruction("mov rax, QWORD PTR [r11 + rcx * 8 + 24]");             // load the boxed Mixed cell of this slot
    emitter.instruction("xor edx, edx");                                        // default word = the null pointer
    emitter.instruction("test rax, rax");                                       // is the slot empty?
    emitter.instruction("jz __rt_array_to_pointer_words_store");                // an empty slot stays the null pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // keep the slot's own cell for the release
    emitter.instruction("call __rt_mixed_unbox");                               // peel nested and reference wrappers: rax = tag, rdi = payload
    emitter.instruction("xor edx, edx");                                        // default word = the null pointer
    emitter.instruction("test rax, rax");                                       // is it an integer, the boxed form of a live address?
    emitter.instruction("cmovz rdx, rdi");                                      // the integer payload is the word; null and anything else are 0
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // keep the word across the cell release
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the slot's own cell
    emitter.instruction("call __rt_decref_mixed");                              // release the cell the slot owned (cell in rax)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the converted word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the element index
    emitter.label("__rt_array_to_pointer_words_store");
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the unique array pointer
    emitter.instruction("mov QWORD PTR [r11 + rcx * 8 + 24], rdx");             // replace the cell with the raw pointer word
    emitter.instruction("inc rcx");                                             // advance to the next slot
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // persist the index across releases
    emitter.instruction("jmp __rt_array_to_pointer_words_loop");                // convert the next slot

    emitter.label("__rt_array_to_pointer_words_stamp");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the converted array
    crate::codegen_support::arrays::emit_array_value_type_stamp(emitter, "rax", &PhpType::Pointer(None));
    emitter.instruction("leave");                                               // release the conversion frame
    emitter.instruction("ret");                                                 // rax = owned array of raw pointer words

    emitter.label("__rt_array_to_pointer_words_passthrough");
    emitter.instruction("ret");                                                 // rax = the unchanged owned array
}
