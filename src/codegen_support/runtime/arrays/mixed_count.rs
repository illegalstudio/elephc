//! Purpose:
//! Emits the `__rt_mixed_count` runtime helper for `count()` on a boxed Mixed receiver.
//! Provides quiet container-aware counting for JSON-decoded mixed values.
//!
//! Called from:
//! - `crate::codegen_support::runtime::arrays::emit_mixed_count()`.
//!
//! Key details:
//! - Boxed indexed arrays and hashes read the entry count from their payload header.
//! - Non-countable tags return zero instead of modeling PHP's warning surface.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::emit_branch_if_null_container;

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
    emitter.instruction("b.eq __rt_mixed_count_payload");                       // share the payload-header read with the indexed path
    emitter.instruction("cmp x9, #6");                                          // tag = 6 (object)?
    emitter.instruction("b.eq __rt_mixed_count_object");                        // runtime-managed Countable objects need object dispatch
    emitter.instruction("b __rt_mixed_count_zero");                             // any other tag → 0 (quiet PHP "not countable")

    emitter.label("__rt_mixed_count_payload");
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the boxed payload pointer (array or hash)
    emit_branch_if_null_container(emitter, "x9", "x10", "__rt_mixed_count_zero");
    emitter.instruction("ldr x0, [x9]");                                        // count lives at offset 0 of both array and hash headers
    emitter.instruction("ret");                                                 // return count in x0

    emitter.label("__rt_mixed_count_object");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load object payload from the Mixed cell
    emit_branch_if_null_container(emitter, "x10", "x11", "__rt_mixed_count_zero");
    emitter.instruction("ldr x11, [x10]");                                      // load object class id
    abi::emit_symbol_address(emitter, "x12", "_spl_fixed_array_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // load SplFixedArray class id
    emitter.instruction("cmp x11, x12");                                        // is this a SplFixedArray object?
    emitter.instruction("b.eq __rt_mixed_count_spl_fixed");                     // count fixed-array storage through the SPL helper
    abi::emit_symbol_address(emitter, "x12", "_spl_dll_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // load SplDoublyLinkedList class id
    emitter.instruction("cmp x11, x12");                                        // is this a SplDoublyLinkedList object?
    emitter.instruction("b.eq __rt_mixed_count_spl_dll");                       // count list storage through the shared list helper
    abi::emit_symbol_address(emitter, "x12", "_spl_stack_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // load SplStack class id
    emitter.instruction("cmp x11, x12");                                        // is this a SplStack object?
    emitter.instruction("b.eq __rt_mixed_count_spl_dll");                       // SplStack shares doubly-linked-list storage
    abi::emit_symbol_address(emitter, "x12", "_spl_queue_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // load SplQueue class id
    emitter.instruction("cmp x11, x12");                                        // is this a SplQueue object?
    emitter.instruction("b.eq __rt_mixed_count_spl_dll");                       // SplQueue shares doubly-linked-list storage
    emitter.instruction("b __rt_mixed_count_zero");                             // unsupported object payloads are not counted here
    emitter.label("__rt_mixed_count_spl_fixed");
    emitter.instruction("mov x0, x10");                                         // pass the unboxed SplFixedArray receiver
    emitter.instruction("b __rt_spl_fixed_count");                              // tail-call the fixed-array counter
    emitter.label("__rt_mixed_count_spl_dll");
    emitter.instruction("mov x0, x10");                                         // pass the unboxed SPL list receiver
    emitter.instruction("b __rt_spl_dll_count");                                // tail-call the list counter

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
    emitter.instruction("je __rt_mixed_count_payload");                         // branch on the shared array/hash payload path
    emitter.instruction("cmp r10, 6");                                          // tag = 6 (object)?
    emitter.instruction("je __rt_mixed_count_object");                          // runtime-managed Countable objects need object dispatch
    emitter.instruction("jmp __rt_mixed_count_zero");                           // any other tag → 0

    emitter.label("__rt_mixed_count_payload");
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the boxed payload pointer
    emit_branch_if_null_container(emitter, "r10", "r11", "__rt_mixed_count_zero");
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // count lives at offset 0 of both array and hash headers
    emitter.instruction("ret");                                                 // return count in rax

    emitter.label("__rt_mixed_count_object");
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load object payload from the Mixed cell
    emit_branch_if_null_container(emitter, "r10", "r11", "__rt_mixed_count_zero");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load object class id
    abi::emit_load_symbol_to_reg(emitter, "r12", "_spl_fixed_array_class_id", 0);
    emitter.instruction("cmp r11, r12");                                        // is this a SplFixedArray object?
    emitter.instruction("je __rt_mixed_count_spl_fixed");                       // count fixed-array storage through the SPL helper
    abi::emit_load_symbol_to_reg(emitter, "r12", "_spl_dll_class_id", 0);
    emitter.instruction("cmp r11, r12");                                        // is this a SplDoublyLinkedList object?
    emitter.instruction("je __rt_mixed_count_spl_dll");                         // count list storage through the shared list helper
    abi::emit_load_symbol_to_reg(emitter, "r12", "_spl_stack_class_id", 0);
    emitter.instruction("cmp r11, r12");                                        // is this a SplStack object?
    emitter.instruction("je __rt_mixed_count_spl_dll");                         // SplStack shares doubly-linked-list storage
    abi::emit_load_symbol_to_reg(emitter, "r12", "_spl_queue_class_id", 0);
    emitter.instruction("cmp r11, r12");                                        // is this a SplQueue object?
    emitter.instruction("je __rt_mixed_count_spl_dll");                         // SplQueue shares doubly-linked-list storage
    emitter.instruction("jmp __rt_mixed_count_zero");                           // unsupported object payloads are not counted here
    emitter.label("__rt_mixed_count_spl_fixed");
    emitter.instruction("mov rdi, r10");                                        // pass the unboxed SplFixedArray receiver
    emitter.instruction("jmp __rt_spl_fixed_count");                            // tail-call the fixed-array counter
    emitter.label("__rt_mixed_count_spl_dll");
    emitter.instruction("mov rdi, r10");                                        // pass the unboxed SPL list receiver
    emitter.instruction("jmp __rt_spl_dll_count");                              // tail-call the list counter

    emitter.label("__rt_mixed_count_zero");
    emitter.instruction("xor rax, rax");                                        // not a container → return 0
    emitter.instruction("ret");                                                 // return 0 in rax
}
