//! Purpose:
//! Shared array metadata emitters used by EIR codegen, runtime helpers, and legacy support.
//! Keeps packed array header stamping outside AST expression lowering.
//!
//! Called from:
//! - `crate::codegen` EIR lowerers and `crate::codegen_support` helper emitters.
//!
//! Key details:
//! - The packed kind word layout must match runtime array allocation and COW metadata.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::types::PhpType;

/// Writes the runtime value_type tag into the array header's packed kind word.
pub(crate) fn emit_array_value_type_stamp(
    emitter: &mut Emitter,
    array_reg: &str,
    elem_ty: &PhpType,
) {
    let value_type_tag = match elem_ty {
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Str => 1,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        PhpType::Void => 8,
        PhpType::Resource(_) => 9,
        _ => return,
    };
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x10, [{}, #-8]", array_reg));     // load the packed array kind word from the heap header
            emitter.instruction("mov x12, #0x80ff");                            // preserve the indexed-array kind and persistent COW flag
            emitter.instruction("and x10, x10, x12");                           // keep only the persistent indexed-array metadata bits
            emitter.instruction(&format!("mov x11, #{}", value_type_tag));      // materialize the runtime array value_type tag
            emitter.instruction("lsl x11, x11, #8");                            // move the value_type tag into the packed kind-word byte lane
            emitter.instruction("orr x10, x10, x11");                           // combine the heap kind with the array value_type tag
            emitter.instruction(&format!("str x10, [{}, #-8]", array_reg));     // persist the packed array kind word in the heap header
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "r12");                                 // preserve the x86_64 nested-call scratch register before reusing it as a temporary array-stamp helper
            emitter.instruction(&format!("mov r10, QWORD PTR [{} - 8]", array_reg)); // load the packed array kind word from the heap header
            emitter.instruction("mov r12, 0xffffffff000080ff");                 // materialize the x86_64 heap-kind preservation mask without clobbering the array base register
            emitter.instruction("and r10, r12");                                // preserve the x86_64 heap magic marker plus the indexed-array kind and persistent COW flag
            emitter.instruction(&format!("mov r12, {}", value_type_tag));       // materialize the runtime array value_type tag in a scratch register that does not alias the array base register
            emitter.instruction("shl r12, 8");                                  // move the value_type tag into the packed kind-word byte lane
            emitter.instruction("or r10, r12");                                 // combine the preserved heap kind with the stamped array value_type tag
            emitter.instruction(&format!("mov QWORD PTR [{} - 8], r10", array_reg)); // persist the packed array kind word in the heap header
            abi::emit_pop_reg(emitter, "r12");                                  // restore the x86_64 nested-call scratch register after the array value-type stamp is complete
        }
    }
}

/// Copies one array's runtime `value_type` tag into another array's packed kind word.
///
/// The run-time twin of `emit_array_value_type_stamp`: that one bakes a tag known at EMIT
/// time, which a SHARED runtime helper cannot do — a helper like `__rt_array_chunk` sees the
/// element type only in the source header. Leaving the destination unstamped is silent: the
/// container is well formed and the right SIZE, but every reader treats its slots as raw
/// words, so a heterogeneous source produced chunks whose elements printed as ADDRESSES.
///
/// `dest_reg` holds the destination array pointer; `source_slot` is an assembler memory
/// operand holding the source array pointer (a stack slot in the calling helper's frame).
pub(crate) fn emit_array_value_type_inherit(
    emitter: &mut Emitter,
    dest_reg: &str,
    source_slot: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, {}", source_slot));           // reload the source array pointer
            emitter.instruction("ldr x9, [x9, #-8]");                           // load the source packed heap-kind word
            emitter.instruction("lsr x9, x9, #8");                              // shift the source value_type into the low bits
            emitter.instruction("and x9, x9, #0x7f");                           // isolate the value_type, dropping the COW bit
            emitter.instruction("lsl x9, x9, #8");                              // move it back into the packed byte lane
            emitter.instruction(&format!("ldr x10, [{}, #-8]", dest_reg));      // load the destination packed heap-kind word
            emitter.instruction("mov x11, #0x80ff");                            // preserve the indexed-array kind and persistent COW flag
            emitter.instruction("and x10, x10, x11");                           // keep only the destination's own metadata bits
            emitter.instruction("orr x10, x10, x9");                            // combine them with the inherited value_type
            emitter.instruction(&format!("str x10, [{}, #-8]", dest_reg));      // persist the destination packed kind word
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "r12");                                 // preserve the nested-call scratch register before reusing it
            emitter.instruction(&format!("mov r10, {}", source_slot));          // reload the source array pointer
            emitter.instruction("mov r10, QWORD PTR [r10 - 8]");                // load the source packed heap-kind word
            emitter.instruction("shr r10, 8");                                  // shift the source value_type into the low bits
            emitter.instruction("and r10, 127");                                // isolate the value_type, dropping the COW bit
            emitter.instruction("shl r10, 8");                                  // move it back into the packed byte lane
            emitter.instruction(&format!("mov r11, QWORD PTR [{} - 8]", dest_reg)); // load the destination packed heap-kind word
            emitter.instruction("mov r12, 0xffffffff000080ff");                 // preserve the heap magic marker, kind and persistent COW flag
            emitter.instruction("and r11, r12");                                // keep only the destination's own metadata bits
            emitter.instruction("or r11, r10");                                 // combine them with the inherited value_type
            emitter.instruction(&format!("mov QWORD PTR [{} - 8], r11", dest_reg)); // persist the destination packed kind word
            abi::emit_pop_reg(emitter, "r12");                                  // restore the nested-call scratch register
        }
    }
}
