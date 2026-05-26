//! Purpose:
//! Emits the `__rt_mixed_count` runtime helper for `count()` on a boxed Mixed receiver.
//! Provides quiet container-aware counting for JSON-decoded mixed values.
//!
//! Called from:
//! - `crate::codegen::runtime::arrays::emit_mixed_count()`.
//!
//! Key details:
//! - Boxed indexed arrays and hashes read the entry count from their payload header.
//! - Non-countable tags return zero instead of modeling PHP's warning surface.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_mixed_count` runtime helper for `count()` on a boxed Mixed receiver.
/// Dispatches to the target-specific implementation.
pub fn emit_mixed_count(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_count_x86_64(emitter);
        return;
    }
    emit_mixed_count_aarch64(emitter);
}

/// Emits `__rt_mixed_count` for ARM64.
///
/// Input: `x0` = pointer to boxed Mixed.
/// Output: `x0` = count (int), or 0 if the Mixed is not a countable container.
///
/// Behavior:
/// - Tag 4 (indexed array) or tag 5 (associative array): reads the count from the
///   payload header at offset 0 and returns it in `x0`.
/// - Any other tag (including null): returns 0 silently, matching PHP's quiet
///   "not countable" semantics.
fn emit_mixed_count_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_count ---");
    emitter.label_global("__rt_mixed_count");

    // x0 = Mixed* receiver. Output: x0 = count.
    emitter.instruction("cbz x0, __rt_mixed_count_zero");                       // null Mixed → 0
    emitter.instruction("ldr x9, [x0]");                                        // load tag from mixed[0]
    emitter.instruction("cmp x9, #4");                                          // tag = 4 (indexed array)?
    emitter.instruction("b.eq __rt_mixed_count_payload");                       // share the payload-header read with the assoc path
    emitter.instruction("cmp x9, #5");                                          // tag = 5 (associative array)?
    emitter.instruction("b.ne __rt_mixed_count_zero");                          // any other tag → 0 (quiet PHP "not countable")

    emitter.label("__rt_mixed_count_payload");
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the boxed payload pointer (array or hash)
    emitter.instruction("cbz x9, __rt_mixed_count_zero");                       // defensive null guard for the payload pointer
    emitter.instruction("ldr x0, [x9]");                                        // count lives at offset 0 of both array and hash headers
    emitter.instruction("ret");                                                 // return count in x0

    emitter.label("__rt_mixed_count_zero");
    emitter.instruction("mov x0, #0");                                          // not a container → return 0
    emitter.instruction("ret");                                                 // return 0 in x0
}

/// Emits `__rt_mixed_count` for x86_64.
///
/// Input: `rax` = pointer to boxed Mixed (single-arg int-result ABI).
/// Output: `rax` = count (int), or 0 if the Mixed is not a countable container.
///
/// Behavior:
/// - Tag 4 (indexed array) or tag 5 (associative array): reads the count from the
///   payload header at offset 0 and returns it in `rax`.
/// - Any other tag (including null): returns 0 silently, matching PHP's quiet
///   "not countable" semantics.
fn emit_mixed_count_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_count ---");
    emitter.label_global("__rt_mixed_count");

    // rax = Mixed* receiver (single-arg int-result ABI). Output: rax = count.
    emitter.instruction("test rax, rax");                                       // null Mixed → 0
    emitter.instruction("je __rt_mixed_count_zero");                            // branch on the current mixed count helper condition
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load tag from mixed[0]
    emitter.instruction("cmp r10, 4");                                          // tag = 4 (indexed array)?
    emitter.instruction("je __rt_mixed_count_payload");                         // branch on the current mixed count helper condition
    emitter.instruction("cmp r10, 5");                                          // tag = 5 (associative array)?
    emitter.instruction("jne __rt_mixed_count_zero");                           // any other tag → 0

    emitter.label("__rt_mixed_count_payload");
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the boxed payload pointer
    emitter.instruction("test r10, r10");                                       // defensive null guard
    emitter.instruction("je __rt_mixed_count_zero");                            // branch on the current mixed count helper condition
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // count lives at offset 0 of both array and hash headers
    emitter.instruction("ret");                                                 // return count in rax

    emitter.label("__rt_mixed_count_zero");
    emitter.instruction("xor rax, rax");                                        // not a container → return 0
    emitter.instruction("ret");                                                 // return 0 in rax
}

/// Suppresses the unused-import warning for the `abi` module on architectures that
/// don't yet need its helpers in this file. The import keeps the module structurally
/// consistent with sibling runtime emitters.
const _: fn(&Emitter) = |_| {
    let _ = abi::int_result_reg;
};
