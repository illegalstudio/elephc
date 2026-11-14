//! Purpose:
//! Emits `__rt_array_payload_to_mixed_indexed`, which normalizes one owned raw PHP array
//! payload into a packed indexed array whose slots are boxed Mixed cells.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via the array runtime.
//! - `crate::codegen::lower_inst::lower_mixed_unbox` for `MixedUnbox` results typed `array<mixed>`.
//! - `crate::codegen::lower_inst::array_access_runtime::lower_mixed_to_mixed_indexed_array`.
//!
//! Key details:
//! - The input is one OWNED raw payload pointer; the helper consumes that owner and returns a
//!   new owned indexed array. A hash payload is first widened to boxed Mixed entries, then
//!   rebuilt in insertion order (the static `array<mixed>` contract has no place for string
//!   keys) and released.
//! - A packed payload is converted through `__rt_array_to_mixed`, which preserves COW by
//!   splitting shared storage before rewriting the element slots.
//! - Null containers, the in-band null sentinel, and non-array payloads pass through unchanged.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::{ARRAY_HEAP_KIND, HASH_HEAP_KIND, NULL_SENTINEL};

/// Normalizes one owned raw array payload into an owned packed Mixed-slot indexed array.
///
/// # ABI
/// - Input: `x0` / `rax` = owned raw indexed-array or hash payload pointer.
/// - Output: `x0` / `rax` = owned packed indexed array with boxed Mixed slots.
pub fn emit_array_payload_to_mixed_indexed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_payload_to_mixed_indexed ---");
    emitter.label_global("__rt_array_payload_to_mixed_indexed");
    emitter.instruction("sub sp, sp, #48");                                     // reserve the source and converted-result slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and continuation
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x19, [sp, #16]");                                  // preserve the callee-saved source register
    emitter.instruction("mov x19, x0");                                         // keep the owned payload across the heap-kind probe
    abi::emit_load_int_immediate(emitter, "x9", NULL_SENTINEL);
    emitter.instruction("cmp x19, x9");                                         // is this the in-band null-container sentinel?
    emitter.instruction("b.eq __rt_array_payload_to_mixed_indexed_done");       // sentinels have no header or slots to convert
    emitter.instruction("cbz x19, __rt_array_payload_to_mixed_indexed_done");   // null payloads pass through unchanged
    emitter.instruction("mov x0, x19");                                         // ask the shared helper which storage layout this payload uses
    abi::emit_call_label(emitter, "__rt_heap_kind");
    emitter.instruction(&format!("cmp x0, #{}", HASH_HEAP_KIND));               // is the payload associative hash storage?
    emitter.instruction("b.eq __rt_array_payload_to_mixed_indexed_hash");       // hashes must be rebuilt as indexed storage
    emitter.instruction(&format!("cmp x0, #{}", ARRAY_HEAP_KIND));              // is the payload ordinary packed array storage?
    emitter.instruction("b.ne __rt_array_payload_to_mixed_indexed_done");       // other heap kinds and raw scalars stay untouched
    emitter.instruction("mov x0, x19");                                         // pass the owned packed array to the Mixed-slot conversion
    emitter.instruction("ldr x1, [x19, #-8]");                                  // load the packed header before recovering its source tag
    emitter.instruction("lsr x1, x1, #8");                                      // move the runtime value_type tag into the low bits
    emitter.instruction("and x1, x1, #0x7f");                                   // isolate the source slot tag, dropping the COW flag
    abi::emit_call_label(emitter, "__rt_array_to_mixed");
    emitter.instruction("b __rt_array_payload_to_mixed_indexed_finish");        // return the converted packed array owner

    emitter.label("__rt_array_payload_to_mixed_indexed_hash");
    emitter.instruction("mov x0, x19");                                         // consume the hash owner while boxing every entry payload
    abi::emit_call_label(emitter, "__rt_hash_to_mixed");
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the Mixed-entry hash for indexed rebuilding
    abi::emit_call_label(emitter, "__rt_hash_to_indexed_array");
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the fresh indexed array while retiring the hash
    emitter.instruction("ldr x0, [sp, #8]");                                    // release the Mixed-entry hash conversion owner
    abi::emit_call_label(emitter, "__rt_decref_hash");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the converted indexed array owner
    emitter.instruction("b __rt_array_payload_to_mixed_indexed_finish");        // share the helper epilogue

    emitter.label("__rt_array_payload_to_mixed_indexed_done");
    emitter.instruction("mov x0, x19");                                         // pass through the unmodified payload pointer
    emitter.label("__rt_array_payload_to_mixed_indexed_finish");
    emitter.instruction("ldr x19, [sp, #16]");                                  // restore the callee-saved source register
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and continuation
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned normalized array
}

/// Emits the x86_64 variant using the System V integer calling convention.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_payload_to_mixed_indexed ---");
    emitter.label_global("__rt_array_payload_to_mixed_indexed");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame across helper calls
    emitter.instruction("sub rsp, 16");                                         // reserve the source and converted-result slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the owned payload across the heap-kind probe
    abi::emit_load_int_immediate(emitter, "r10", NULL_SENTINEL);
    emitter.instruction("cmp rax, r10");                                        // is this the in-band null-container sentinel?
    emitter.instruction("je __rt_array_payload_to_mixed_indexed_done_x86");     // sentinels have no header or slots to convert
    emitter.instruction("test rax, rax");                                       // null payloads have no heap kind
    emitter.instruction("jz __rt_array_payload_to_mixed_indexed_done_x86");     // pass null through unchanged
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // ask the shared helper which storage layout this payload uses
    abi::emit_call_label(emitter, "__rt_heap_kind");
    emitter.instruction(&format!("cmp eax, {}", HASH_HEAP_KIND));               // is the payload associative hash storage?
    emitter.instruction("je __rt_array_payload_to_mixed_indexed_hash_x86");     // hashes must be rebuilt as indexed storage
    emitter.instruction(&format!("cmp eax, {}", ARRAY_HEAP_KIND));              // is the payload ordinary packed array storage?
    emitter.instruction("jne __rt_array_payload_to_mixed_indexed_done_x86");    // other heap kinds and raw scalars stay untouched
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the owned packed array to the Mixed-slot conversion
    emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");                        // load the packed header before recovering its source tag
    emitter.instruction("shr rsi, 8");                                          // move the runtime value_type tag into the low bits
    emitter.instruction("and rsi, 0x7f");                                       // isolate the source slot tag, dropping the COW flag
    abi::emit_call_label(emitter, "__rt_array_to_mixed");
    emitter.instruction("jmp __rt_array_payload_to_mixed_indexed_finish_x86");  // return the converted packed array owner

    emitter.label("__rt_array_payload_to_mixed_indexed_hash_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // consume the hash owner while boxing every entry payload
    abi::emit_call_label(emitter, "__rt_hash_to_mixed");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the Mixed-entry hash for indexed rebuilding
    emitter.instruction("mov rdi, rax");                                        // pass the converted hash to the indexed rebuild helper
    abi::emit_call_label(emitter, "__rt_hash_to_indexed_array");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the fresh indexed array while retiring the hash
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // release the Mixed-entry hash conversion owner
    abi::emit_call_label(emitter, "__rt_decref_hash");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the converted indexed array owner
    emitter.instruction("jmp __rt_array_payload_to_mixed_indexed_finish_x86");  // share the helper epilogue

    emitter.label("__rt_array_payload_to_mixed_indexed_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // pass through the unmodified payload pointer
    emitter.label("__rt_array_payload_to_mixed_indexed_finish_x86");
    emitter.instruction("add rsp, 16");                                         // release the helper frame slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned normalized array
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target probes the runtime layout, rebuilds hashes, and converts packed slots.
    #[test]
    fn array_payload_to_mixed_indexed_dispatches_both_layouts() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_payload_to_mixed_indexed(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_heap_kind"), "{name}");
            assert!(asm.contains("__rt_hash_to_indexed_array"), "{name}");
            assert!(asm.contains("__rt_array_to_mixed"), "{name}");
            assert!(asm.contains("__rt_decref_hash"), "{name}");
            assert!(asm.contains("__rt_array_payload_to_mixed_indexed_hash"), "{name}");
        }
    }
}
