//! Purpose:
//! Emits the `__rt_array_is_list` runtime helper assembly for array_is_list.
//! Determines whether a PHP array value has sequential integer keys 0..n-1 in insertion order.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Indexed arrays are always lists; hashes are walked through the insertion-order chain; boxed mixed cells are unwrapped once.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_is_list: report whether a container is a PHP list (keys 0..n-1 in order).
/// Input:  x0 = container pointer (indexed array, hash, or boxed mixed cell)
/// Output: x0 = 1 when the value is a list, 0 otherwise
///
/// Indexed arrays are always lists. Hash tables are walked along the insertion-order
/// chain, requiring every key to be the integer matching its position. Boxed mixed
/// cells holding an array payload are unwrapped once and re-dispatched.
pub fn emit_array_is_list(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_is_list_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_is_list ---");
    emitter.label_global("__rt_array_is_list");
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the uniform heap-kind header word
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low-byte heap kind
    emitter.instruction("cmp x9, #2");                                          // is the value an indexed array?
    emitter.instruction("b.eq __rt_array_is_list_one");                         // indexed arrays are always lists
    emitter.instruction("cmp x9, #5");                                          // is the value a boxed mixed cell?
    emitter.instruction("b.eq __rt_array_is_list_mixed");                       // mixed cells must be unwrapped before inspection
    emitter.instruction("cmp x9, #3");                                          // is the value an associative hash?
    emitter.instruction("b.ne __rt_array_is_list_zero");                        // any other kind is not a PHP array
    emitter.comment("-- walk the hash insertion-order chain checking keys 0,1,2,... --");
    emitter.instruction("mov x10, #0");                                         // expected next integer key starts at 0
    emitter.instruction("ldr x11, [x0, #24]");                                  // x11 = head slot index from the hash header
    emitter.label("__rt_array_is_list_loop");
    emitter.instruction("cmn x11, #1");                                         // has the insertion chain reached its end (slot == -1)?
    emitter.instruction("b.eq __rt_array_is_list_one");                         // all keys matched 0..n-1 in order, including the empty hash
    emitter.instruction("mov x12, #64");                                        // hash entry stride in bytes
    emitter.instruction("mul x12, x11, x12");                                   // byte offset of the current slot
    emitter.instruction("add x12, x0, x12");                                    // advance from the hash base to the slot
    emitter.instruction("add x12, x12, #40");                                   // skip the 40-byte hash header
    emitter.instruction("ldr x13, [x12, #16]");                                 // x13 = key_len (-1 marks an integer key)
    emitter.instruction("cmn x13, #1");                                         // is this entry keyed by an integer?
    emitter.instruction("b.ne __rt_array_is_list_zero");                        // a string key cannot appear in a list
    emitter.instruction("ldr x14, [x12, #8]");                                  // x14 = integer key payload
    emitter.instruction("cmp x14, x10");                                        // does the key equal the expected position?
    emitter.instruction("b.ne __rt_array_is_list_zero");                        // a gap or reorder breaks list shape
    emitter.instruction("add x10, x10, #1");                                    // advance the expected position
    emitter.instruction("ldr x11, [x12, #56]");                                 // x11 = next slot index in insertion order
    emitter.instruction("b __rt_array_is_list_loop");                           // continue checking the next entry
    emitter.label("__rt_array_is_list_mixed");
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed mixed value tag
    emitter.instruction("cmp x9, #4");                                          // does the cell box an indexed array?
    emitter.instruction("b.eq __rt_array_is_list_unwrap");                      // unwrap indexed array payloads
    emitter.instruction("cmp x9, #5");                                          // does the cell box an associative array?
    emitter.instruction("b.eq __rt_array_is_list_unwrap");                      // unwrap associative array payloads
    emitter.instruction("b __rt_array_is_list_zero");                           // non-array mixed payloads are not lists
    emitter.label("__rt_array_is_list_unwrap");
    emitter.instruction("ldr x0, [x0, #8]");                                    // unbox the container pointer from mixed[8]
    emitter.instruction("b __rt_array_is_list");                                // re-dispatch on the unboxed container
    emitter.label("__rt_array_is_list_one");
    emitter.instruction("mov x0, #1");                                          // result: the value is a list
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_array_is_list_zero");
    emitter.instruction("mov x0, #0");                                          // result: the value is not a list
    emitter.instruction("ret");                                                 // return to caller
}

/// x86_64 Linux implementation of `__rt_array_is_list`.
/// Input:  rdi = container pointer
/// Output: rax = 1 when the value is a list, 0 otherwise
fn emit_array_is_list_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_is_list ---");
    emitter.label_global("__rt_array_is_list");
    emitter.instruction("movzx eax, BYTE PTR [rdi - 8]");                       // load the low-byte heap kind from the uniform header
    emitter.instruction("cmp eax, 2");                                          // is the value an indexed array?
    emitter.instruction("je __rt_array_is_list_one");                           // indexed arrays are always lists
    emitter.instruction("cmp eax, 5");                                          // is the value a boxed mixed cell?
    emitter.instruction("je __rt_array_is_list_mixed");                         // mixed cells must be unwrapped before inspection
    emitter.instruction("cmp eax, 3");                                          // is the value an associative hash?
    emitter.instruction("jne __rt_array_is_list_zero");                         // any other kind is not a PHP array
    emitter.comment("-- walk the hash insertion-order chain checking keys 0,1,2,... --");
    emitter.instruction("xor r10, r10");                                        // expected next integer key starts at 0
    emitter.instruction("mov r11, QWORD PTR [rdi + 24]");                       // r11 = head slot index from the hash header
    emitter.label("__rt_array_is_list_loop");
    emitter.instruction("cmp r11, -1");                                         // has the insertion chain reached its end?
    emitter.instruction("je __rt_array_is_list_one");                           // all keys matched 0..n-1 in order, including the empty hash
    emitter.instruction("mov rcx, r11");                                        // copy the slot index before scaling it
    emitter.instruction("shl rcx, 6");                                          // convert the slot index into a 64-byte entry offset
    emitter.instruction("add rcx, rdi");                                        // advance from the hash base to the slot
    emitter.instruction("add rcx, 40");                                         // skip the 40-byte hash header
    emitter.instruction("mov r8, QWORD PTR [rcx + 16]");                        // r8 = key_len (-1 marks an integer key)
    emitter.instruction("cmp r8, -1");                                          // is this entry keyed by an integer?
    emitter.instruction("jne __rt_array_is_list_zero");                         // a string key cannot appear in a list
    emitter.instruction("mov r9, QWORD PTR [rcx + 8]");                         // r9 = integer key payload
    emitter.instruction("cmp r9, r10");                                         // does the key equal the expected position?
    emitter.instruction("jne __rt_array_is_list_zero");                         // a gap or reorder breaks list shape
    emitter.instruction("add r10, 1");                                          // advance the expected position
    emitter.instruction("mov r11, QWORD PTR [rcx + 56]");                       // r11 = next slot index in insertion order
    emitter.instruction("jmp __rt_array_is_list_loop");                         // continue checking the next entry
    emitter.label("__rt_array_is_list_mixed");
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the boxed mixed value tag
    emitter.instruction("cmp rax, 4");                                          // does the cell box an indexed array?
    emitter.instruction("je __rt_array_is_list_unwrap");                        // unwrap indexed array payloads
    emitter.instruction("cmp rax, 5");                                          // does the cell box an associative array?
    emitter.instruction("je __rt_array_is_list_unwrap");                        // unwrap associative array payloads
    emitter.instruction("jmp __rt_array_is_list_zero");                         // non-array mixed payloads are not lists
    emitter.label("__rt_array_is_list_unwrap");
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // unbox the container pointer from mixed[8]
    emitter.instruction("jmp __rt_array_is_list");                              // re-dispatch on the unboxed container
    emitter.label("__rt_array_is_list_one");
    emitter.instruction("mov rax, 1");                                          // result: the value is a list
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_array_is_list_zero");
    emitter.instruction("xor rax, rax");                                        // result: the value is not a list
    emitter.instruction("ret");                                                 // return to caller
}

