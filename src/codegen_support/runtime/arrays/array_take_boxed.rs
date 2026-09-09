//! Purpose:
//! Removes an edge value from a boxed PHP array while preserving COW and key semantics.
//!
//! Called from:
//! - Runtime emission and the typed ArrayPop/ArrayShift codegen paths.
//!
//! Key details:
//! - Cell separation consumes one old owner; codegen publishes the returned cell before mutation.
//! - Pop preserves remaining keys. Shift rebuilds numeric keys while retaining string keys.
//! - The removed value is retained before unlinking, so removal cannot destroy its payload.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits boxed cell COW and the shared pop/shift operation for the active target.
pub fn emit_array_take_boxed(emitter: &mut Emitter) {
    emit_cell_ensure_unique(emitter);
    emit_take(emitter);
}

/// Accepts an array cell in the first C argument and consumes its old owner only when splitting.
fn emit_cell_ensure_unique(emitter: &mut Emitter) {
    let arg = abi::int_arg_reg_name(emitter.target, 0);
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_array_cell_ensure_unique");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, arg, 8);
    abi::emit_reg_move(emitter, result, arg);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x0, __rt_array_cell_invalid");             // null is not a boxed PHP array
            emitter.instruction("ldr x9, [x0]");                                // validate the cell tag before reading allocation metadata
            emitter.instruction("sub x9, x9, #4");                              // array and hash tags become zero and one
            emitter.instruction("cmp x9, #1");                                  // only the two PHP array layouts can be mutated
            emitter.instruction("b.hi __rt_array_cell_invalid");                // reject scalar, object and resource cells
            emitter.instruction("ldr w9, [x0, #-12]");                          // inspect the outer cell's owner count
            emitter.instruction("cmp w9, #1");                                  // unique cells can retain their address
            emitter.instruction("b.ls __rt_array_cell_unique");                 // skip allocation when no value alias shares this cell
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // null is not a boxed PHP array
            emitter.instruction("jz __rt_array_cell_invalid");                  // reject null before dereferencing its tag
            emitter.instruction("mov r10, QWORD PTR [rax]");                    // validate the cell tag before reading allocation metadata
            emitter.instruction("sub r10, 4");                                  // array and hash tags become zero and one
            emitter.instruction("cmp r10, 1");                                  // only the two PHP array layouts can be mutated
            emitter.instruction("ja __rt_array_cell_invalid");                  // reject scalar, object and resource cells
            emitter.instruction("cmp DWORD PTR [rax - 12], 1");                 // inspect the outer cell's owner count
            emitter.instruction("jbe __rt_array_cell_unique");                  // skip allocation when no value alias shares this cell
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_clone");
    // The old cell had at least two owners. Dropping this one cannot run a
    // destructor, and the new cell already retained the underlying array.
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x9", 8);
            emitter.instruction("ldr w10, [x9, #-12]");                         // reload the shared cell's count after cloning
            emitter.instruction("sub w10, w10, #1");                            // transfer this receiver away from the old cell
            emitter.instruction("str w10, [x9, #-12]");                         // preserve all other value aliases
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r10", 8);
            emitter.instruction("sub DWORD PTR [r10 - 12], 1");                 // transfer this receiver away from the shared old cell
        }
    }
    abi::emit_jump(emitter, "__rt_array_cell_unique");
    emitter.label("__rt_array_cell_invalid");
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label("__rt_array_cell_unique");
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Mutates a published unique cell in argument zero; argument one selects shift rather than pop.
fn emit_take(emitter: &mut Emitter) {
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let arg2 = abi::int_arg_reg_name(emitter.target, 2);
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_array_take_boxed");
    abi::emit_frame_prologue(emitter, 64);
    // FP-relative slots: cell 8, hash 16, removed owner 24, key words 32/40, mode 48.
    abi::store_at_offset(emitter, arg0, 8);
    abi::store_at_offset(emitter, arg1, 48);
    abi::emit_call_label(emitter, "__rt_mixed_cell_promote_to_hash");
    abi::store_at_offset(emitter, result, 16);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0]");                                // inspect the live entry count after payload separation
            emitter.instruction("cbz x9, __rt_array_take_empty");               // an empty array returns an owned null cell
            abi::load_at_offset(emitter, "x9", 48);
            emitter.instruction("ldr x10, [x0, #32]");                          // pop selects the insertion-order tail
            emitter.instruction("cbz x9, __rt_array_take_entry");               // pop does not need to inspect the head
            emitter.instruction("ldr x10, [x0, #24]");                          // shift selects the insertion-order head
            emitter.label("__rt_array_take_entry");
            emitter.instruction("add x10, x0, x10, lsl #6");                    // each hash bucket occupies sixty-four bytes
            emitter.instruction("add x10, x10, #40");                           // skip the fixed hash header
            emitter.instruction("ldp x1, x2, [x10, #8]");                       // preserve the selected key before helpers can relocate registers
            abi::store_at_offset(emitter, "x1", 32);
            abi::store_at_offset(emitter, "x2", 40);
            emitter.instruction("ldr x0, [x10, #40]");                          // per-entry tags describe heterogeneous payloads
            emitter.instruction("ldp x1, x2, [x10, #24]");                      // borrow the selected value payload words
            emitter.instruction("cmp x0, #7");                                  // an existing Mixed value needs a retain rather than nested boxing
            emitter.instruction("b.ne __rt_array_take_box_value");              // concrete payloads need a new owned cell
            emitter.instruction("mov x0, x1");                                  // pass the existing Mixed pointer to incref
        }
        Arch::X86_64 => {
            emitter.instruction("cmp QWORD PTR [rax], 0");                      // inspect the live entry count after payload separation
            emitter.instruction("je __rt_array_take_empty");                    // an empty array returns an owned null cell
            abi::load_at_offset(emitter, "r10", 48);
            emitter.instruction("mov r11, QWORD PTR [rax + 32]");               // pop selects the insertion-order tail
            emitter.instruction("test r10, r10");                               // inspect the requested removal mode
            emitter.instruction("jz __rt_array_take_entry");                    // pop does not need to inspect the head
            emitter.instruction("mov r11, QWORD PTR [rax + 24]");               // shift selects the insertion-order head
            emitter.label("__rt_array_take_entry");
            emitter.instruction("shl r11, 6");                                  // each hash bucket occupies sixty-four bytes
            emitter.instruction("lea r11, [rax + r11 + 40]");                   // skip the fixed hash header
            emitter.instruction("mov rdi, QWORD PTR [r11 + 8]");                // borrow the selected key's low word
            emitter.instruction("mov rsi, QWORD PTR [r11 + 16]");               // borrow its length or integer sentinel
            abi::store_at_offset(emitter, "rdi", 32);
            abi::store_at_offset(emitter, "rsi", 40);
            emitter.instruction("mov rax, QWORD PTR [r11 + 40]");               // per-entry tags describe heterogeneous payloads
            emitter.instruction("mov rdi, QWORD PTR [r11 + 24]");               // borrow the selected value's low word
            emitter.instruction("mov rsi, QWORD PTR [r11 + 32]");               // borrow its paired high word when present
            emitter.instruction("cmp rax, 7");                                  // an existing Mixed value needs a retain rather than nested boxing
            emitter.instruction("jne __rt_array_take_box_value");               // concrete payloads need a new owned cell
            emitter.instruction("mov rax, rdi");                                // pass the existing Mixed pointer to incref
        }
    }
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_jump(emitter, "__rt_array_take_unlink");
    emitter.label("__rt_array_take_box_value");
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.label("__rt_array_take_unlink");
    abi::store_at_offset(emitter, result, 24);
    abi::load_at_offset(emitter, arg0, 16);
    abi::load_at_offset(emitter, arg1, 32);
    abi::load_at_offset(emitter, arg2, 40);
    abi::emit_call_label(emitter, "__rt_hash_unset");
    abi::load_at_offset(emitter, result, 48);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("cbz x0, __rt_array_take_done"),   // pop leaves every surviving key unchanged
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // inspect whether surviving numeric keys need rebuilding
            emitter.instruction("jz __rt_array_take_done");                     // pop leaves every surviving key unchanged
        }
    }
    // Hash spread into an empty destination preserves string keys and numbers
    // the surviving integer keys from zero in insertion order.
    abi::load_at_offset(emitter, arg0, 16);
    abi::emit_load_from_address(emitter, arg1, arg0, 16);
    abi::emit_load_from_address(emitter, arg0, arg0, 8);
    abi::emit_call_label(emitter, "__rt_hash_new");
    abi::emit_reg_move(emitter, arg0, result);
    abi::load_at_offset(emitter, arg1, 16);
    abi::emit_call_label(emitter, "__rt_hash_spread");
    let cell = if emitter.target.arch == Arch::AArch64 { "x9" } else { "r10" };
    abi::load_at_offset(emitter, cell, 8);
    abi::emit_store_to_address(emitter, result, cell, 8);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_decref_hash");
    abi::emit_jump(emitter, "__rt_array_take_done");
    emitter.label("__rt_array_take_empty");
    // mixed_from_value intentionally uses the legacy tag/result register on x86_64.
    abi::emit_load_int_immediate(emitter, result, 8);
    if emitter.target.arch == Arch::AArch64 {
        abi::emit_load_int_immediate(emitter, "x1", 0);
        abi::emit_load_int_immediate(emitter, "x2", 0);
    } else {
        abi::emit_load_int_immediate(emitter, "rdi", 0);
        abi::emit_load_int_immediate(emitter, "rsi", 0);
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, result, 24);
    emitter.label("__rt_array_take_done");
    abi::load_at_offset(emitter, result, 24);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every ABI retains the removed result before unlinking and releases the replaced hash, not its replacement.
    #[test]
    fn boxed_take_retains_results_before_mutation_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_array_take_boxed(&mut emitter);
            let asm = emitter.output();
            let take = &asm[asm.find("__rt_array_take_boxed:").unwrap()..];
            let unlink = take.find("__rt_hash_unset").unwrap();
            assert!(take.find("__rt_incref").unwrap() < unlink, "{name}");
            assert!(take.find("__rt_mixed_from_value").unwrap() < unlink, "{name}");
            assert!(take.find("__rt_hash_spread").unwrap() > unlink, "{name}");
            if name == "linux-x86_64" {
                let release = take.find("call __rt_decref_hash").unwrap();
                let reload = take[..release].rfind("mov rax, QWORD PTR [rbp - 16]").unwrap();
                assert!(take[reload..release].lines().count() <= 2, "release must receive the old hash in rax");
            }
        }
    }
}
