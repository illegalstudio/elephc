//! Purpose:
//! Authenticates native cells whose physical payload is a persistent boxed PHP reference.
//!
//! Called from:
//! - Managed reference runtime emission and native reference ABI adaptation.
//!
//! Key details:
//! - Raw frames and interior addresses are rejected before any header read.
//! - Descriptor seven alone is insufficient because raw Iterable cells share that tag.
//! - The result is borrowed and must be recomputed after PHP re-entry.

use super::*;

/// Returns a live marker-one Mixed wrapper, or zero for every raw or uninitialized layout.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let none = "__rt_reference_cell_canonical_none";
    let done = "__rt_reference_cell_canonical_done";
    emitter.blank();
    emitter.label_global("__rt_reference_cell_canonical");
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_call_label(emitter, "__rt_reference_cell_owner");
    abi::emit_branch_if_int_result_zero(emitter, done);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0, #-8]");                           // inspect only an authenticated managed cell header
            emitter.instruction("ubfx x9, x9, #8, #7");                         // isolate the physical payload descriptor
            emitter.instruction("cmp x9, #7");                                  // a canonical cell owns a boxed Mixed payload
            emitter.instruction(&format!("b.ne {none}"));                       // raw scalar and string words cannot be wrapper pointers
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rax - 8]");                // inspect only an authenticated managed cell header
            emitter.instruction("shr r10, 8");                                  // position the physical payload descriptor
            emitter.instruction("and r10d, 0x7f");                              // discard collector flags and the heap marker
            emitter.instruction("cmp r10, 7");                                  // a canonical cell owns a boxed Mixed payload
            emitter.instruction(&format!("jne {none}"));                        // raw scalar and string words cannot be wrapper pointers
        }
    }
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_load_int_immediate(emitter, scratch, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {result}, {scratch}"));           // unset cells retain the low-word initialization sentinel
            emitter.instruction(&format!("b.eq {none}"));                       // never inspect a sentinel as a heap payload
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {result}, {scratch}"));           // unset cells retain the low-word initialization sentinel
            emitter.instruction(&format!("je {none}"));                         // never inspect a sentinel as a heap payload
        }
    }
    abi::emit_call_label(emitter, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(emitter, done);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldrb w9, [x0, #-8]");                          // descriptor seven also permits raw iterable containers
            emitter.instruction("cmp w9, #5");                                  // only managed Mixed allocations can hold a PHPRef wrapper
            emitter.instruction(&format!("b.ne {none}"));                       // reject arrays, objects and other owned payload kinds
            emitter.instruction("ldr x9, [x0]");                                // inspect the logical Mixed tag after heap authentication
            emitter.instruction("cmp x9, #7");                                  // persistent aliases own a nested boxed child
            emitter.instruction(&format!("b.ne {none}"));                       // ordinary concrete boxes keep their raw cell representation
            emitter.instruction("ldr x9, [x0, #16]");                           // distinguish aliases from ordinary nested Mixed values
            emitter.instruction("cmp x9, #1");                                  // marker one is the persistent PHP reference ABI
            emitter.instruction(&format!("b.ne {none}"));                       // never promote an ordinary nested value into an alias
        }
        Arch::X86_64 => {
            emitter.instruction("cmp BYTE PTR [rax - 8], 5");                   // only managed Mixed allocations can hold a PHPRef wrapper
            emitter.instruction(&format!("jne {none}"));                        // raw iterable containers share descriptor seven
            emitter.instruction("cmp QWORD PTR [rax], 7");                      // persistent aliases own a nested boxed child
            emitter.instruction(&format!("jne {none}"));                        // ordinary concrete boxes keep their raw cell representation
            emitter.instruction("cmp QWORD PTR [rax + 16], 1");                 // marker one distinguishes aliases from ordinary nested values
            emitter.instruction(&format!("jne {none}"));                        // never promote an ordinary nested value into an alias
        }
    }
    abi::emit_jump(emitter, done);
    emitter.label(none);
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every target authenticates the cell and wrapper before inspecting their physical layouts.
    #[test]
    fn canonical_reference_shape_authentication_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(crate::codegen_support::platform::Target::parse(target).unwrap());
            emit(&mut emitter);
            let assembly = emitter.output();
            let owner = assembly.find("__rt_reference_cell_owner").unwrap();
            let payload = assembly.find("__rt_gc_candidate_live").unwrap();
            assert!(owner < payload, "{target}: raw cell addresses must be rejected first");
            assert_eq!(assembly.matches("__rt_reference_cell_owner").count(), 1,
                "{target}: shape lookup must use the allocator identity index once");
            assert!(!assembly.contains("__rt_incref"), "{target}: the probe returns a borrowed wrapper");
        }
    }
}
