//! Purpose:
//! Records tokenizer source-buffer coordinates without changing Throwable storage layouts.
//!
//! Called from:
//! - The typed tokenizer location operation and its boxed eval adapter.
//!
//! Key details:
//! - Compact and ordinary Throwable layouts share a flagged creation-line word.
//! - This leaf borrows the receiver and returns the original integer line.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, sentinels::THROWABLE_SCANNED_LOCATION_FLAG};

/// Emits a setter accepting receiver/line in x0/x1 or rax/rcx.
pub(super) fn emit_setter(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_tokenizer_error_location");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- preserve the result and encode the scanned-source marker --
            emitter.instruction("mov x2, x1");                                  // retain the plain result before encoding metadata
            emitter.instruction(&format!("orr x1, x1, #{}", THROWABLE_SCANNED_LOCATION_FLAG)); // distinguish buffer locations from ordinary source spans
            emitter.instruction("cbz x0, __rt_tokenizer_location_done");        // an invalid internal receiver has no writable metadata
            // -- resolve compact or ordinary Throwable metadata storage --
            emitter.instruction("ldurb w9, [x0, #-8]");                         // inspect the physical heap kind
            emitter.instruction("mov x10, #32");                                // compact Throwables reserve the fixed creation-line word
            emitter.instruction("cmp w9, #6");                                  // kind six is the compact Throwable payload
            emitter.instruction("b.eq __rt_tokenizer_location_store");          // preserve the compact storage layout
            emitter.instruction("ldr x9, [x0]");                                // ordinary objects carry a runtime class id
            abi::emit_symbol_address(emitter, "x10", "_class_throwable_line_offsets");
            emitter.instruction("ldr x10, [x10, x9, lsl #3]");                  // resolve the private line word after PHP properties
            emitter.instruction("cbz x10, __rt_tokenizer_location_done");       // non-Throwable objects have no location storage
            // -- write the metadata while keeping the receiver borrowed --
            emitter.label("__rt_tokenizer_location_store");
            emitter.instruction("str x1, [x0, x10]");                           // stamp the borrowed receiver without touching PHP values
            // -- return the original line without its private marker --
            emitter.label("__rt_tokenizer_location_done");
            emitter.instruction("mov x0, x2");                                  // expose the plain integer result
        }
        Arch::X86_64 => {
            // -- encode the scanned-source marker in the private line word --
            emitter.instruction("bts rcx, 63");                                 // mark the line as an explicitly named scanned buffer
            emitter.instruction("test rax, rax");                               // an invalid internal receiver has no writable metadata
            emitter.instruction("jz __rt_tokenizer_location_done");             // skip the metadata write for a missing object
            // -- resolve compact or ordinary Throwable metadata storage --
            emitter.instruction("movzx r9, BYTE PTR [rax - 8]");                // inspect the physical heap kind
            emitter.instruction("mov r10, 32");                                 // compact Throwables reserve the fixed creation-line word
            emitter.instruction("cmp r9, 6");                                   // kind six is the compact Throwable payload
            emitter.instruction("je __rt_tokenizer_location_store");            // preserve the compact storage layout
            emitter.instruction("mov r9, QWORD PTR [rax]");                     // resolve the ordinary runtime class id
            abi::emit_symbol_address(emitter, "r10", "_class_throwable_line_offsets");
            emitter.instruction("mov r10, QWORD PTR [r10 + r9 * 8]");           // locate private metadata after PHP properties
            emitter.instruction("test r10, r10");                               // non-Throwable objects have no location storage
            emitter.instruction("jz __rt_tokenizer_location_done");             // leave unrelated objects untouched
            // -- write the metadata while keeping the receiver borrowed --
            emitter.label("__rt_tokenizer_location_store");
            emitter.instruction("mov QWORD PTR [rax + r10], rcx");              // stamp the borrowed receiver without changing ownership
            // -- return the original line without its private marker --
            emitter.label("__rt_tokenizer_location_done");
            emitter.instruction("btr rcx, 63");                                 // remove private metadata from the returned integer
            emitter.instruction("mov rax, rcx");                                // expose the plain integer result
        }
    }
    emitter.instruction("ret");                                                 // return without allocations or nested calls
}
