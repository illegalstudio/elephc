//! Purpose:
//! Copies native callable descriptor headers and initializes fresh capture storage.
//!
//! Called from:
//! - Compiler callable construction and closure binding.
//!
//! Key details:
//! - The caller supplies the descriptor metadata and manages capture values and handles.
//! - Fresh captures must be zeroed before the heap header publishes the descriptor as callable.

use crate::layout::{CALLABLE_HEAP_KIND, x86_64_heap_kind_word};
use crate::{abi, Arch, Emitter};

/// Bytes in the fixed header before the first runtime capture.
pub const HEADER_BYTES: usize = 64;

/// Copies only the fixed header, preserving any runtime capture slots.
pub fn emit_copy_runtime_header(
    emitter: &mut impl Emitter,
    dest: &str,
    source: &str,
    scratch: &str,
) {
    for offset in (0..HEADER_BYTES).step_by(8) {
        abi::emit_load_from_address(emitter, scratch, source, offset);
        abi::emit_store_to_address(emitter, scratch, dest, offset);
    }
}

/// Copies a static descriptor's header and initializes the capture storage of a fresh copy.
pub fn emit_copy_static_descriptor_to_runtime(
    emitter: &mut impl Emitter,
    dest: &str,
    descriptor_label: &str,
) {
    let source = emitter.symbol_scratch_reg();
    abi::emit_symbol_address(emitter, source, descriptor_label);
    emit_copy_descriptor_header_to_runtime(emitter, dest, source);
}

/// Copies a descriptor at a known address and initializes the fresh callable allocation.
/// The source register must differ from the secondary scratch register used for the copy.
pub fn emit_copy_descriptor_header_to_runtime(emitter: &mut impl Emitter, dest: &str, source: &str) {
    let scratch = emitter.secondary_scratch_reg();
    emit_copy_runtime_header(emitter, dest, source, scratch);
    emit_initialize_runtime_descriptor(emitter, dest);
}

/// Clears capture storage before publishing a fresh allocation as a managed callable node.
pub fn emit_initialize_runtime_descriptor(emitter: &mut impl Emitter, dest: &str) {
    // -- recycled captures must be null before any later allocation or ownership cleanup --
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr w9, [{dest}, #-16]"));            // bound initialization by the allocator's actual payload capacity
            emitter.instruction(&format!("mov x10, #{HEADER_BYTES}"));          // preserve the copied fixed descriptor header
            emitter.label("1");
            emitter.instruction("cmp x10, x9");                                 // stop before crossing the allocated descriptor payload
            emitter.instruction("b.hs 2f");                                     // descriptors without captures need no slot initialization
            emitter.instruction(&format!("str xzr, [{dest}, x10]"));            // leave every not-yet-written capture null for nested cleanup
            emitter.instruction("add x10, x10, #8");                            // initialize the next capture word, including metadata halves
            emitter.instruction("b 1b");                                        // finish initialization before publishing the graph kind
            emitter.label("2");
            emitter.instruction(&format!("mov x9, #{CALLABLE_HEAP_KIND}"));     // register callable ownership independently from raw helper buffers
            emitter.instruction(&format!("str x9, [{dest}, #-8]"));             // publish the canonical managed callable kind
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r11d, DWORD PTR [{dest} - 16]")); // bound initialization by the allocator's actual payload capacity
            emitter.instruction(&format!("mov r10d, {HEADER_BYTES}"));          // preserve the copied fixed descriptor header
            emitter.label("1");
            emitter.instruction("cmp r10, r11");                                // stop before crossing the allocated descriptor payload
            emitter.instruction("jae 2f");                                      // descriptors without captures need no slot initialization
            emitter.instruction(&format!("mov QWORD PTR [{dest} + r10], 0"));   // leave every not-yet-written capture null for nested cleanup
            emitter.instruction("add r10, 8");                                  // initialize the next capture word, including metadata halves
            emitter.instruction("jmp 1b");                                      // finish initialization before publishing the graph kind
            emitter.label("2");
            emitter.instruction(&format!("mov r10, 0x{:x}", x86_64_heap_kind_word(CALLABLE_HEAP_KIND))); // preserve canonical heap magic while registering callable ownership
            emitter.instruction(&format!("mov QWORD PTR [{dest} - 8], r10"));   // publish the canonical managed callable kind
        }
    }
}
