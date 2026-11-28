//! Purpose:
//! Publishes the packed array element layout used by Mixed owners.
//!
//! Called from:
//! - Runtime adapters constructing native packed boxed arrays.
//!
//! Key details:
//! - Callers publish the element layout before storing child cells.
//! - Existing flags in an array header can be preserved independently of its initial stamp.

use crate::{Arch, AssemblyEmitter, TargetAbi};

const MIXED_ELEMENT_BITS: u32 = 7 << 8;
const FRESH_MIXED_KIND: u32 = crate::layout::ARRAY_HEAP_KIND | 0x8000 | MIXED_ELEMENT_BITS;

/// Marks a freshly allocated packed array in the result register as containing Mixed cells.
pub fn emit_enable(emitter: &mut (impl AssemblyEmitter + TargetAbi)) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0, #-8]");                           // preserve the array's existing header flags
            emitter.instruction(&format!("orr x9, x9, #{MIXED_ELEMENT_BITS:#x}")); // select Mixed element ownership
            emitter.instruction("str x9, [x0, #-8]");                           // publish the updated word
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("or QWORD PTR [rax - 8], {MIXED_ELEMENT_BITS:#x}")); // publish Mixed ownership without altering flags
        }
    }
}

/// Sets the initial packed Mixed word, including COW and the target's heap ownership marker.
pub fn emit_stamp_fresh(emitter: &mut (impl AssemblyEmitter + TargetAbi)) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x9, #{FRESH_MIXED_KIND:#x}"));    // materialize the indexed Mixed layout
            emitter.instruction("str x9, [x0, #-8]");                           // publish the complete header word
        }
        Arch::X86_64 => {
            let word = crate::layout::x86_64_heap_kind_word(FRESH_MIXED_KIND);
            emitter.instruction(&format!("mov r10, {word:#x}"));                // include the x86 ownership marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // publish the complete header word
        }
    }
}
