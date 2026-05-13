//! Runtime helper for `count()` on a Mixed receiver.
//!
//! PHP's `count()` on a non-countable produces a warning and returns 1 in
//! older PHP and 0 with a warning in PHP 8+. elephc collapses both edges to
//! the quiet zero — the most common idiom is `count(json_decode($json, true))`
//! and a defensive zero matches what user code typically expects.
//!
//! For boxed indexed arrays (tag 4) and associative hashes (tag 5) we read
//! the entry count from the payload header (offset 0). Objects (tag 6) only
//! support count when they implement Countable in PHP — elephc does not yet
//! model Countable, so objects fall through to zero. All other tags
//! (scalars, null, boxed mixed, …) also return zero.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emit `__rt_mixed_count(mixed_ptr) → int`.
pub fn emit_mixed_count(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_count_x86_64(emitter);
        return;
    }
    emit_mixed_count_aarch64(emitter);
}

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

// Suppress an unused-import warning for the abi module on architectures
// that don't yet need its helpers in this small file. The import keeps the
// module structurally consistent with sibling runtime emitters.
const _: fn(&Emitter) = |_| {
    let _ = abi::int_result_reg;
};
