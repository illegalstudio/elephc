//! Purpose:
//! Borrows normalized eval argument cells for native method and constructor bridges.
//!
//! Called from:
//! - `super::eval_method_helpers` and `super::eval_constructor_helpers`.
//!
//! Key details:
//! - Magician owns a dense indexed array of Mixed cells throughout each native invocation.
//! - Reads allocate nothing and retain nothing; the enclosing argument array roots every borrow.
//! - Shape and bounds guards reject malformed bridge inputs before dereferencing an element.

use crate::codegen::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::sentinels::emit_branch_if_null_container;

/// Loads a borrowed argument into the bridge's fixed frame spill without creating a key or owner.
pub(super) fn emit_borrowed_argument(
    emitter: &mut Emitter,
    index: usize,
    array_frame_offset: usize,
    result_frame_offset: usize,
    fail_label: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [x29, #-{array_frame_offset}]")); // load the caller-owned argument array box
            emitter.instruction(&format!("cbz x0, {fail_label}"));              // reject a missing argument array
            emitter.instruction("ldr x9, [x0]");                                // read the normalized argument container tag
            emitter.instruction("cmp x9, #4");                                  // normalized call arguments use indexed storage
            emitter.instruction(&format!("b.ne {fail_label}"));                 // do not interpret hash or scalar payloads as indexed storage
            emitter.instruction("ldr x0, [x0, #8]");                            // borrow the indexed payload rooted by the argument box
            emit_branch_if_null_container(emitter, "x0", "x9", fail_label);
            emitter.instruction("ldr x9, [x0, #-8]");                           // read the indexed element representation
            emitter.instruction("ubfx x9, x9, #8, #7");                         // isolate the element value tag
            emitter.instruction("cmp x9, #7");                                  // a borrowed argument must already be a boxed Mixed cell
            emitter.instruction(&format!("b.ne {fail_label}"));                 // reject raw scalar element storage
            abi::emit_load_int_immediate(emitter, "x10", index as i64);
            emitter.instruction("ldr x9, [x0]");                                // read the number of bound arguments
            emitter.instruction("cmp x10, x9");                                 // check the requested argument before loading its slot
            emitter.instruction(&format!("b.hs {fail_label}"));                 // reject a missing argument without reading past the array
            emitter.instruction("add x0, x0, #24");                             // address the packed Mixed pointer slots
            emitter.instruction("ldr x0, [x0, x10, lsl #3]");                   // borrow the argument cell without incrementing its refcount
            emitter.instruction(&format!("str x0, [x29, #-{result_frame_offset}]")); // keep the borrow available for coercion and reference writeback
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {array_frame_offset}]")); // load the caller-owned argument array box
            emitter.instruction("test rax, rax");                               // check whether an argument array exists
            emitter.instruction(&format!("jz {fail_label}"));                   // reject a missing argument array
            emitter.instruction("cmp QWORD PTR [rax], 4");                      // normalized call arguments use indexed storage
            emitter.instruction(&format!("jne {fail_label}"));                  // do not interpret hash or scalar payloads as indexed storage
            emitter.instruction("mov rax, QWORD PTR [rax + 8]");                // borrow the indexed payload rooted by the argument box
            emit_branch_if_null_container(emitter, "rax", "r10", fail_label);
            emitter.instruction("mov r10, QWORD PTR [rax - 8]");                // read the indexed element representation
            emitter.instruction("shr r10, 8");                                  // shift the element value tag to the low bits
            emitter.instruction("and r10, 127");                                // discard heap flags outside the value tag
            emitter.instruction("cmp r10, 7");                                  // a borrowed argument must already be a boxed Mixed cell
            emitter.instruction(&format!("jne {fail_label}"));                  // reject raw scalar element storage
            abi::emit_load_int_immediate(emitter, "r11", index as i64);
            emitter.instruction("cmp r11, QWORD PTR [rax]");                    // check the argument index against the logical length
            emitter.instruction(&format!("jae {fail_label}"));                  // reject a missing argument without reading past the array
            emitter.instruction("mov rax, QWORD PTR [rax + r11 * 8 + 24]");     // borrow the argument cell without incrementing its refcount
            emitter.instruction(&format!("mov QWORD PTR [rbp - {result_frame_offset}], rax")); // preserve the borrow for coercion and reference writeback
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Every supported target checks argument storage and bounds without allocating or retaining.
    #[test]
    fn native_argument_borrows_are_guarded_and_allocation_free() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_borrowed_argument(&mut emitter, 3, 32, 40, "invalid_arguments");
            let asm = emitter.output();
            assert!(!asm.contains("__elephc_eval_value_int"), "{name}");
            assert!(!asm.contains("__elephc_eval_value_array_get"), "{name}");
            assert!(!asm.contains("incref"), "{name}");
            if target.arch == Arch::AArch64 {
                assert!(asm.contains("cmp x9, #4"), "{name}");
                assert!(asm.contains("cmp x9, #7"), "{name}");
                assert!(asm.contains("b.hs invalid_arguments"), "{name}");
                assert!(asm.contains("str x0, [x29, #-40]"), "{name}");
            } else {
                assert!(asm.contains("cmp QWORD PTR [rax], 4"), "{name}");
                assert!(asm.contains("cmp r10, 7"), "{name}");
                assert!(asm.contains("jae invalid_arguments"), "{name}");
                assert!(asm.contains("mov QWORD PTR [rbp - 40], rax"), "{name}");
            }
        }
    }
}
