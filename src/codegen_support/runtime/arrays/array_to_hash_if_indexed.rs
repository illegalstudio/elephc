//! Purpose:
//! Emits the `__rt_array_to_hash_if_indexed` runtime helper: promotes an operand to hash storage
//! only when it is not already hash storage.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The two-hash builtin lowering (`array_diff_key`, `array_intersect_key`, …) decides from the
//!   STATIC type whether an operand needs promoting. `array<mixed>` — what a declared `array`
//!   parameter carries — was read as "indexed, needs promoting", so a value that was already a
//!   hash got walked as if it were indexed. `array_diff_key($a, ['x'=>1])` on `['x'=>1,'y'=>2]`
//!   answered `2:0,1:7,1024`: invented keys AND invented values.
//! - The pass-through path RETAINS the operand. The caller releases whatever this returns as a
//!   converted temporary, so handing back the original without a retain would over-release the
//!   caller's own array. One `__rt_incref` keeps that bookkeeping exactly balanced.
//! - A NULL operand keeps today's behaviour and goes to `__rt_array_to_hash` unchanged.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Uniform heap kind tag for associative (hash) array storage.
const HASH_STORAGE_KIND: u32 = 3;

/// Emits the `__rt_array_to_hash_if_indexed` runtime helper.
///
/// ABI (AArch64): `x0` = operand; returns the hash in `x0`.
/// ABI (x86_64):  `rdi` = operand; returns the hash in `rax`.
///
/// Both mirror `__rt_array_to_hash`, which this tail-branches to, so the two are interchangeable
/// at every call site.
pub fn emit_array_to_hash_if_indexed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_to_hash_if_indexed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_to_hash_if_indexed ---");
    emitter.label_global("__rt_array_to_hash_if_indexed");

    emitter.instruction("cbz x0, __rt_array_to_hash_if_indexed_convert");       // a null operand keeps today's conversion behaviour
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load packed kind metadata from the operand header
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low byte (kind tag)
    emitter.instruction(&format!("cmp x9, #{HASH_STORAGE_KIND}"));              // is the operand already hash storage?
    emitter.instruction("b.ne __rt_array_to_hash_if_indexed_convert");          // indexed storage still needs promoting
    emitter.instruction("b __rt_incref");                                       // already a hash: retain it and hand the same pointer back

    emitter.label("__rt_array_to_hash_if_indexed_convert");
    emitter.instruction("b __rt_array_to_hash");                                // promote indexed storage exactly as before
}

/// x86_64 Linux implementation of `__rt_array_to_hash_if_indexed`.
fn emit_array_to_hash_if_indexed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_to_hash_if_indexed ---");
    emitter.label_global("__rt_array_to_hash_if_indexed");

    emitter.instruction("test rdi, rdi");                                       // is the operand a null pointer?
    emitter.instruction("jz __rt_array_to_hash_if_indexed_x86_convert");        // a null operand keeps today's conversion behaviour
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // load packed kind metadata from the operand header
    emitter.instruction("and r9, 0xff");                                        // isolate the low byte (kind tag)
    emitter.instruction(&format!("cmp r9, {HASH_STORAGE_KIND}"));               // is the operand already hash storage?
    emitter.instruction("jne __rt_array_to_hash_if_indexed_x86_convert");       // indexed storage still needs promoting
    emitter.instruction("mov rax, rdi");                                        // already a hash: move it into the result register
    emitter.instruction("jmp __rt_incref");                                     // retain it; `__rt_incref` reads and preserves rax

    emitter.label("__rt_array_to_hash_if_indexed_x86_convert");
    emitter.instruction("jmp __rt_array_to_hash");                              // promote indexed storage exactly as before
}
