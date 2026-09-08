//! Purpose:
//! Reverses a boxed PHP array without narrowing its packed or associative storage.
//!
//! Called from:
//! - The typed ArrayReverse codegen path for boxed PHP array declarations.
//!
//! Key details:
//! - Borrows the source and returns an owned hash of Mixed cells, or zero for a non-array.
//! - String keys survive reversal; integer keys are renumbered only when preservation is false.
//! - Each result value owns a retained cell or a freshly boxed payload. Source links never change.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits boxed reversal for both native ABIs, preserving the source's keys, cursor, and owners.
pub fn emit_array_reverse_boxed(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_array_reverse_boxed");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Borrows a box in x0 and a preservation flag in x1, returning an owned Mixed-value hash in x0.
fn emit_aarch64(emitter: &mut Emitter) {
    // -- validate the box before reading either container layout --
    emitter.instruction("sub sp, sp, #80");                                     // reserve eight state slots and the saved frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller across allocation helpers
    emitter.instruction("add x29, sp, #64");                                    // establish an aligned helper frame
    emitter.instruction("str x1, [sp]");                                        // preserve the key policy across unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // borrow the concrete tag and source payload
    emitter.instruction("sub x9, x0, #4");                                      // map packed and hash tags onto zero and one
    emitter.instruction("cmp x9, #1");                                          // only the two PHP array layouts are valid
    emitter.instruction("b.hi __rt_array_reverse_boxed_invalid");               // reject without allocating a partial result
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the borrowed source throughout result construction
    emitter.instruction("str x0, [sp, #16]");                                   // retain the layout discriminator
    emitter.instruction("ldr x9, [x1]");                                        // packed arrays and hashes both begin with their live count
    emitter.instruction("cmp x0, #4");                                          // choose an index or a hash insertion-order cursor
    emitter.instruction("b.eq __rt_array_reverse_boxed_cursor");                // packed iteration begins one past its last slot
    emitter.instruction("ldr x9, [x1, #32]");                                   // hashes begin at the last live entry, not the final bucket
    emitter.label("__rt_array_reverse_boxed_cursor");
    emitter.instruction("str x9, [sp, #24]");                                   // initialize the descending source cursor
    emitter.instruction("str xzr, [sp, #40]");                                  // result integer keys start at zero
    emitter.instruction("ldr x0, [x1]");                                        // size the initial result from the source count
    emitter.instruction("mov x9, #8");                                          // avoid a zero-capacity result for empty arrays
    emitter.instruction("cmp x0, x9");                                          // choose at least the minimum hash capacity
    emitter.instruction("csel x0, x0, x9, ge");                                 // retain larger capacity hints
    emitter.instruction("mov x1, #7");                                          // every result entry owns a boxed Mixed value
    emitter.instruction("bl __rt_hash_new");                                    // allocate independent result storage
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the owner across entry boxing and insertion

    // -- traverse packed slots or hash links in reverse insertion order --
    emitter.label("__rt_array_reverse_boxed_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // recover the source layout
    emitter.instruction("ldr x10, [sp, #24]");                                  // recover the next source position
    emitter.instruction("ldr x11, [sp, #8]");                                   // borrow the unchanged source storage
    emitter.instruction("cmp x9, #4");                                          // packed arrays use a descending numeric index
    emitter.instruction("b.ne __rt_array_reverse_boxed_hash");                  // hashes follow their previous-entry links
    emitter.instruction("cbz x10, __rt_array_reverse_boxed_done");              // no packed elements remain
    emitter.instruction("sub x10, x10, #1");                                    // visit the preceding packed slot
    emitter.instruction("str x10, [sp, #24]");                                  // advance before calling any helper
    emitter.instruction("str x10, [sp, #48]");                                  // preserve the original integer key
    emitter.instruction("mov x9, #-1");                                         // integer keys use the negative high-word marker
    emitter.instruction("str x9, [sp, #56]");                                   // record the source key kind
    emitter.instruction("ldr x0, [x11, #-8]");                                  // read the packed element metadata
    emitter.instruction("ubfx x0, x0, #8, #7");                                 // isolate the runtime value tag
    emitter.instruction("ldr x12, [x11, #16]");                                 // packed strings require a sixteen-byte stride
    emitter.instruction("madd x11, x10, x12, x11");                             // address this slot relative to the source header
    emitter.instruction("ldr x1, [x11, #24]");                                  // read the low payload word after the array header
    emitter.instruction("mov x2, #0");                                          // scalar and pointer slots have no second word
    emitter.instruction("cmp x12, #16");                                        // read a high word only when storage actually contains it
    emitter.instruction("b.ne __rt_array_reverse_boxed_value");                 // avoid reading past an eight-byte final slot
    emitter.instruction("ldr x2, [x11, #32]");                                  // preserve the string length or other paired payload
    emitter.instruction("b __rt_array_reverse_boxed_value");                    // normalize this value into an owned cell

    emitter.label("__rt_array_reverse_boxed_hash");
    emitter.instruction("tbnz x10, #63, __rt_array_reverse_boxed_done");        // minus one terminates the insertion-order chain
    emitter.instruction("add x11, x11, x10, lsl #6");                           // locate the live sixty-four-byte bucket
    emitter.instruction("add x11, x11, #40");                                   // skip the hash header
    emitter.instruction("ldr x9, [x11, #48]");                                  // follow the previous live entry, skipping tombstones
    emitter.instruction("str x9, [sp, #24]");                                   // preserve traversal across allocations
    emitter.instruction("ldp x9, x10, [x11, #8]");                              // borrow the normalized integer or string key
    emitter.instruction("stp x9, x10, [sp, #48]");                              // keep both key words while boxing the value
    emitter.instruction("ldr x0, [x11, #40]");                                  // each hash entry is authoritative for its value tag
    emitter.instruction("ldp x1, x2, [x11, #24]");                              // preserve both source payload words

    // -- acquire one value owner and transfer it into a new hash entry --
    emitter.label("__rt_array_reverse_boxed_value");
    emitter.instruction("cmp x0, #7");                                          // an existing Mixed cell can be shared directly
    emitter.instruction("b.ne __rt_array_reverse_boxed_box");                   // raw values need a new cell with the actual runtime tag
    emitter.instruction("mov x0, x1");                                          // borrow the source's existing Mixed pointer
    emitter.instruction("bl __rt_incref");                                      // acquire the result's independent owner without nesting boxes
    emitter.instruction("b __rt_array_reverse_boxed_key");                      // insert the retained cell
    emitter.label("__rt_array_reverse_boxed_box");
    emitter.instruction("bl __rt_mixed_from_value");                            // persist strings and retain heap-backed raw payloads
    emitter.label("__rt_array_reverse_boxed_key");
    emitter.instruction("mov x3, x0");                                          // transfer the owned Mixed pointer as the entry value
    emitter.instruction("ldp x1, x2, [sp, #48]");                               // reload the source key
    emitter.instruction("ldr x9, [sp]");                                        // recover the preservation flag
    emitter.instruction("cbnz x9, __rt_array_reverse_boxed_insert");            // keep every original key when requested
    emitter.instruction("cmn x2, #1");                                          // only integer keys are renumbered
    emitter.instruction("b.ne __rt_array_reverse_boxed_insert");                // PHP always keeps string keys
    emitter.instruction("ldr x1, [sp, #40]");                                   // use the next result integer key
    emitter.instruction("add x9, x1, #1");                                      // reserve the following integer key
    emitter.instruction("str x9, [sp, #40]");                                   // string keys never advance this counter
    emitter.label("__rt_array_reverse_boxed_insert");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the unique result hash
    emitter.instruction("mov x4, #0");                                          // a Mixed pointer occupies one payload word
    emitter.instruction("mov x5, #7");                                          // the hash owns this boxed cell after insertion
    emitter.instruction("bl __rt_hash_set");                                    // persist borrowed string keys and consume the value owner
    emitter.instruction("str x0, [sp, #32]");                                   // publish any result relocation before advancing
    emitter.instruction("b __rt_array_reverse_boxed_loop");                     // continue with the unchanged source

    // -- return the owned result, or report an invalid input without an allocation --
    emitter.label("__rt_array_reverse_boxed_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // transfer the completed hash owner to the caller
    emitter.instruction("b __rt_array_reverse_boxed_return");                   // share the balanced helper epilogue
    emitter.label("__rt_array_reverse_boxed_invalid");
    emitter.instruction("mov x0, #0");                                          // let the codegen caller raise a catchable TypeError
    emitter.label("__rt_array_reverse_boxed_return");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller's frame and continuation
    emitter.instruction("add sp, sp, #80");                                     // release all temporary traversal state
    emitter.instruction("ret");                                                 // return without consuming the source array
}

/// Borrows a box in rdi and a preservation flag in rsi, returning an owned hash in rax.
fn emit_x86_64(emitter: &mut Emitter) {
    // -- validate the box before reading either container layout --
    emitter.instruction("push rbp");                                            // preserve the caller's frame pointer
    emitter.instruction("mov rbp, rsp");                                        // keep a stable base across native helper calls
    emitter.instruction("sub rsp, 64");                                         // reserve eight state slots with SysV call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the key policy across unboxing
    emitter.instruction("mov rax, rdi");                                        // use the Mixed helper's result-register input ABI
    emitter.instruction("call __rt_mixed_unbox");                               // borrow the concrete tag and source payload
    emitter.instruction("lea r10, [rax - 4]");                                  // map packed and hash tags onto zero and one
    emitter.instruction("cmp r10, 1");                                          // only the two PHP array layouts are valid
    emitter.instruction("ja __rt_array_reverse_boxed_invalid");                 // reject before allocating a partial result
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // preserve the borrowed source throughout construction
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // retain the layout discriminator
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // both source layouts begin with their live count
    emitter.instruction("cmp rax, 4");                                          // choose a packed index or an insertion-order cursor
    emitter.instruction("je __rt_array_reverse_boxed_cursor");                  // packed iteration begins one past its final slot
    emitter.instruction("mov r10, QWORD PTR [rdi + 32]");                       // hashes begin at the last live entry
    emitter.label("__rt_array_reverse_boxed_cursor");
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // initialize the descending source cursor
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // result integer keys begin at zero
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // size the initial result from the live count
    emitter.instruction("mov r10, 8");                                          // empty results still need usable hash capacity
    emitter.instruction("cmp rdi, r10");                                        // choose at least the minimum capacity
    emitter.instruction("cmovl rdi, r10");                                      // retain larger capacity hints
    emitter.instruction("mov rsi, 7");                                          // result entries own Mixed cells
    emitter.instruction("call __rt_hash_new");                                  // allocate independent result storage
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the owner across boxing and insertion

    // -- traverse packed slots or hash links in reverse insertion order --
    emitter.label("__rt_array_reverse_boxed_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // recover the next source position
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // borrow the unchanged source storage
    emitter.instruction("cmp QWORD PTR [rbp - 24], 4");                         // packed sources use a descending numeric index
    emitter.instruction("jne __rt_array_reverse_boxed_hash");                   // hashes follow their previous-entry links
    emitter.instruction("test r10, r10");                                       // check whether any packed elements remain
    emitter.instruction("jz __rt_array_reverse_boxed_done");                    // stop before reading outside the packed payload
    emitter.instruction("sub r10, 1");                                          // visit the preceding packed slot
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // advance before calling any helper
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // preserve the original integer key
    emitter.instruction("mov QWORD PTR [rbp - 64], -1");                        // mark this key as an integer
    emitter.instruction("mov rax, QWORD PTR [r11 - 8]");                        // read the packed element metadata
    emitter.instruction("shr rax, 8");                                          // move the runtime value tag into the low bits
    emitter.instruction("and rax, 127");                                        // exclude heap and copy-on-write flags
    emitter.instruction("mov rcx, QWORD PTR [r11 + 16]");                       // read the actual packed element stride
    emitter.instruction("imul r10, rcx");                                       // locate this slot within the packed payload
    emitter.instruction("add r11, r10");                                        // keep the source header offset separate
    emitter.instruction("mov rdi, QWORD PTR [r11 + 24]");                       // borrow the low payload word after the array header
    emitter.instruction("xor esi, esi");                                        // scalar and pointer slots have no second word
    emitter.instruction("cmp rcx, 16");                                         // read a high word only when storage contains it
    emitter.instruction("jne __rt_array_reverse_boxed_value");                  // avoid reading past an eight-byte final slot
    emitter.instruction("mov rsi, QWORD PTR [r11 + 32]");                       // preserve string lengths and other paired payloads
    emitter.instruction("jmp __rt_array_reverse_boxed_value");                  // normalize the source value into an owned cell

    emitter.label("__rt_array_reverse_boxed_hash");
    emitter.instruction("test r10, r10");                                       // minus one terminates the insertion-order chain
    emitter.instruction("js __rt_array_reverse_boxed_done");                    // no live source entries remain
    emitter.instruction("shl r10, 6");                                          // each bucket occupies sixty-four bytes
    emitter.instruction("lea r11, [r11 + r10 + 40]");                           // locate the bucket after the hash header
    emitter.instruction("mov r10, QWORD PTR [r11 + 48]");                       // follow the previous live entry, skipping tombstones
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve traversal across allocations
    emitter.instruction("mov r10, QWORD PTR [r11 + 8]");                        // borrow the key's integer value or string pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // preserve the low key word across boxing
    emitter.instruction("mov r10, QWORD PTR [r11 + 16]");                       // read the integer marker or string length
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // preserve the source key kind
    emitter.instruction("mov rax, QWORD PTR [r11 + 40]");                       // each entry supplies its actual runtime value tag
    emitter.instruction("mov rdi, QWORD PTR [r11 + 24]");                       // borrow the low payload word
    emitter.instruction("mov rsi, QWORD PTR [r11 + 32]");                       // preserve the high payload word

    // -- acquire one value owner and transfer it into a new hash entry --
    emitter.label("__rt_array_reverse_boxed_value");
    emitter.instruction("cmp rax, 7");                                          // an existing Mixed cell can be shared directly
    emitter.instruction("jne __rt_array_reverse_boxed_box");                    // raw values need a new cell with the actual tag
    emitter.instruction("mov rax, rdi");                                        // use the retain helper's result-register input ABI
    emitter.instruction("call __rt_incref");                                    // acquire a result owner without nesting Mixed cells
    emitter.instruction("jmp __rt_array_reverse_boxed_key");                    // insert the retained cell
    emitter.label("__rt_array_reverse_boxed_box");
    emitter.instruction("call __rt_mixed_from_value");                          // persist strings and retain heap-backed raw payloads
    emitter.label("__rt_array_reverse_boxed_key");
    emitter.instruction("mov rcx, rax");                                        // transfer the owned Mixed pointer as the entry value
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the source key's low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // reload its integer marker or string length
    emitter.instruction("cmp QWORD PTR [rbp - 8], 0");                          // inspect the requested key policy
    emitter.instruction("jne __rt_array_reverse_boxed_insert");                 // keep every original key when requested
    emitter.instruction("cmp rdx, -1");                                         // only integer keys are renumbered
    emitter.instruction("jne __rt_array_reverse_boxed_insert");                 // PHP always keeps string keys
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // use the next result integer key
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // string keys never advance this counter
    emitter.label("__rt_array_reverse_boxed_insert");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the unique result hash
    emitter.instruction("xor r8d, r8d");                                        // a Mixed pointer occupies one payload word
    emitter.instruction("mov r9, 7");                                           // the hash owns this boxed cell after insertion
    emitter.instruction("call __rt_hash_set");                                  // persist borrowed keys and consume the value owner
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // publish any result relocation before advancing
    emitter.instruction("jmp __rt_array_reverse_boxed_loop");                   // continue with the unchanged source

    // -- return the owned result, or reject without allocating --
    emitter.label("__rt_array_reverse_boxed_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // transfer the completed result owner
    emitter.instruction("jmp __rt_array_reverse_boxed_return");                 // share the balanced helper epilogue
    emitter.label("__rt_array_reverse_boxed_invalid");
    emitter.instruction("xor eax, eax");                                        // let the codegen caller raise a catchable TypeError
    emitter.label("__rt_array_reverse_boxed_return");
    emitter.instruction("mov rsp, rbp");                                        // discard traversal state
    emitter.instruction("pop rbp");                                             // restore the caller's frame pointer
    emitter.instruction("ret");                                                 // return without consuming the source
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All supported targets guard both layouts and retain boxed entries before transferring them.
    #[test]
    fn boxed_reverse_guards_layouts_and_owns_entries_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_array_reverse_boxed(&mut emitter);
            let asm = emitter.output();
            for symbol in ["__rt_mixed_unbox", "__rt_hash_new", "__rt_incref", "__rt_mixed_from_value", "__rt_hash_set"] {
                assert!(asm.contains(symbol), "{name}: {symbol}");
            }
            assert!(asm.find("__rt_array_reverse_boxed_invalid").unwrap()
                < asm.find("__rt_hash_new").unwrap(), "{name}");
            assert!(asm.contains("__rt_array_reverse_boxed_hash:"), "{name}");
            assert!(asm.contains("__rt_array_reverse_boxed_key:"), "{name}");
            assert!(!asm.contains("decref"), "{name}: reversal only borrows its source");
        }
    }
}
