//! Purpose:
//! Emits the `__rt_array_union_gradual` runtime helper: PHP's `+` operator on two values whose
//! STATIC type is the gradual `array<mixed>` but whose runtime storage may be indexed or hash.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - `array<mixed>` is the representation a declared `array` parameter gets, and a declared
//!   `array` holds string keys perfectly happily. Picking a union helper from the static type
//!   alone therefore ran `__rt_array_union` — the INDEXED walk — over hash storage, which
//!   silently produced a short array and left the result malformed enough that iterating it
//!   afterwards exhausted the heap. `count()` and `array_keys()` never had this problem because
//!   they already dispatch on the runtime kind; this helper gives `+` the same treatment.
//! - Dispatch is a tail branch to one of the four existing helpers, all of which share the ABI
//!   (`x0`/`rdi` = left, `x1`/`rsi` = right, result in `x0`/`rax`), so no frame is needed here.
//!   Cross-symbol jumps are unconditional `b`/`jmp` only; conditional branches stay local, which
//!   is what keeps the jump valid once the linker starts inserting branch islands.
//! - A NULL operand keeps today's behaviour rather than inventing new semantics: an empty array
//!   can legitimately be a null pointer, so a null reads as kind 0 and takes the indexed path
//!   exactly as it did before this helper existed.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Uniform heap kind tag for associative (hash) array storage.
const HASH_STORAGE_KIND: u32 = 3;

/// Emits the `__rt_array_union_gradual` runtime helper.
///
/// ABI (AArch64):   `x0` = left, `x1` = right; result in `x0`.
/// ABI (x86_64):    `rdi` = left, `rsi` = right; result in `rax`.
///
/// The result is owned by the caller, exactly like the helper it dispatches to.
pub fn emit_array_union_gradual(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_union_gradual_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_union_gradual ---");
    emitter.label_global("__rt_array_union_gradual");

    // -- read both storage kinds without disturbing the operand registers --
    emitter.instruction("mov x9, #0");                                          // a null left operand reports kind 0 and keeps the indexed path
    emitter.instruction("cbz x0, __rt_array_union_gradual_left_ready");         // skip the header read for a null left operand
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load packed kind metadata from the left array header
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low byte (kind tag)
    emitter.label("__rt_array_union_gradual_left_ready");

    emitter.instruction("mov x10, #0");                                         // a null right operand reports kind 0 and keeps the indexed path
    emitter.instruction("cbz x1, __rt_array_union_gradual_right_ready");        // skip the header read for a null right operand
    emitter.instruction("ldr x10, [x1, #-8]");                                  // load packed kind metadata from the right array header
    emitter.instruction("and x10, x10, #0xff");                                 // isolate the low byte (kind tag)
    emitter.label("__rt_array_union_gradual_right_ready");

    // -- four-way dispatch; every cross-symbol jump is unconditional --
    emitter.instruction(&format!("cmp x9, #{HASH_STORAGE_KIND}"));              // is the left operand hash storage?
    emitter.instruction("b.eq __rt_array_union_gradual_left_hash");             // route hash-left through the hash-first helpers
    emitter.instruction(&format!("cmp x10, #{HASH_STORAGE_KIND}"));             // left is indexed: is the right operand hash storage?
    emitter.instruction("b.eq __rt_array_union_gradual_indexed_hash");          // indexed + hash needs the promoting helper
    emitter.instruction("b __rt_array_union");                                  // indexed + indexed is the dense suffix merge

    emitter.label("__rt_array_union_gradual_indexed_hash");
    emitter.instruction("b __rt_array_hash_union");                             // indexed left, hash right

    emitter.label("__rt_array_union_gradual_left_hash");
    emitter.instruction(&format!("cmp x10, #{HASH_STORAGE_KIND}"));             // left is hash: is the right operand hash storage too?
    emitter.instruction("b.eq __rt_array_union_gradual_hash_hash");             // hash + hash is the plain hash union
    emitter.instruction("b __rt_hash_array_union");                             // hash left, indexed right

    emitter.label("__rt_array_union_gradual_hash_hash");
    emitter.instruction("b __rt_hash_union");                                   // hash + hash
}

/// x86_64 Linux implementation of `__rt_array_union_gradual`.
fn emit_array_union_gradual_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_union_gradual ---");
    emitter.label_global("__rt_array_union_gradual");

    // -- read both storage kinds without disturbing the operand registers --
    emitter.instruction("xor r9d, r9d");                                        // a null left operand reports kind 0 and keeps the indexed path
    emitter.instruction("test rdi, rdi");                                       // is the left operand a null pointer?
    emitter.instruction("jz __rt_array_union_gradual_x86_left_ready");          // skip the header read for a null left operand
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // load packed kind metadata from the left array header
    emitter.instruction("and r9, 0xff");                                        // isolate the low byte (kind tag)
    emitter.label("__rt_array_union_gradual_x86_left_ready");

    emitter.instruction("xor r10d, r10d");                                      // a null right operand reports kind 0 and keeps the indexed path
    emitter.instruction("test rsi, rsi");                                       // is the right operand a null pointer?
    emitter.instruction("jz __rt_array_union_gradual_x86_right_ready");         // skip the header read for a null right operand
    emitter.instruction("mov r10, QWORD PTR [rsi - 8]");                        // load packed kind metadata from the right array header
    emitter.instruction("and r10, 0xff");                                       // isolate the low byte (kind tag)
    emitter.label("__rt_array_union_gradual_x86_right_ready");

    // -- four-way dispatch; every cross-symbol jump is unconditional --
    emitter.instruction(&format!("cmp r9, {HASH_STORAGE_KIND}"));               // is the left operand hash storage?
    emitter.instruction("je __rt_array_union_gradual_x86_left_hash");           // route hash-left through the hash-first helpers
    emitter.instruction(&format!("cmp r10, {HASH_STORAGE_KIND}"));              // left is indexed: is the right operand hash storage?
    emitter.instruction("je __rt_array_union_gradual_x86_indexed_hash");        // indexed + hash needs the promoting helper
    emitter.instruction("jmp __rt_array_union");                                // indexed + indexed is the dense suffix merge

    emitter.label("__rt_array_union_gradual_x86_indexed_hash");
    emitter.instruction("jmp __rt_array_hash_union");                           // indexed left, hash right

    emitter.label("__rt_array_union_gradual_x86_left_hash");
    emitter.instruction(&format!("cmp r10, {HASH_STORAGE_KIND}"));              // left is hash: is the right operand hash storage too?
    emitter.instruction("je __rt_array_union_gradual_x86_hash_hash");           // hash + hash is the plain hash union
    emitter.instruction("jmp __rt_hash_array_union");                           // hash left, indexed right

    emitter.label("__rt_array_union_gradual_x86_hash_hash");
    emitter.instruction("jmp __rt_hash_union");                                 // hash + hash
}
