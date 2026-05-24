//! Purpose:
//! Emits the `_fn_<f>` wrapper symbol for a generator function: allocates a
//! fresh `GeneratorFrame` on the heap, stamps it with the Generator class id
//! and the resume function address, copies integer parameters into their
//! frame slots, zeroes locals, and returns the frame pointer.
//!
//! Called from:
//!  - `crate::codegen::functions::generator::emit_generator_function()` via
//!    the parent module's `emit_wrapper` re-export.
//!
//! Key details:
//!  - Wrapper layout: 16 bytes for `x29`/`x30` save plus a 16-byte-aligned
//!    parameter stash; the heap frame itself is `aligned_frame_size_with_slots`.
//!  - All fixed-header slots (`class_id`, `resume_fn`, `state_idx`/`flags`,
//!    `auto_key_counter`, key/value/return/sent pointers, `delegated_iter`,
//!    `layout_id`) are initialised here so the resume function and runtime
//!    helpers can rely on the invariants on first entry.

use super::{aligned_frame_size_with_slots, slot_offset};
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime::generators::frame as gen_frame;

/// Emits the `_fn_<f>` wrapper symbol for a generator function (ARM64 path).
///
/// Allocates a fresh `GeneratorFrame` on the heap via `__rt_heap_alloc`, stamps it
/// with `class_id` and the `resume_label` address, copies integer parameters from
/// the stash slots into their frame slots, zero-initialises all local slots, and
/// returns the frame pointer in `x0`.
///
/// # Arguments
/// - `emitter` — assembly emitter
/// - `label` — global symbol name of the wrapper (e.g. `_fn_foo`)
/// - `resume_label` — global symbol name of the generator's resume function
/// - `class_id` — compile-time Generator class id written to the frame header
/// - `int_param_count` — number of integer parameters to copy into the frame
/// - `int_local_count` — number of local slots to zero-initialise
///
/// # Stack layout
/// Prologue reserves `16 + param_save_bytes` bytes; `param_save_bytes` is
/// `int_param_count * 8` rounded up to a 16-byte boundary. Parameters are parked
/// in the stash before the heap allocation so the frame can be built before the
/// caller frame is unwound.
///
/// # Frame header initialisation
/// All fixed-header slots (class_id, resume_fn, state_idx/flags, auto_key_counter,
/// last_key, last_value, return_value, sent_value, delegated_iter, layout_id) are
/// written here so the resume function and runtime helpers can rely on the
/// invariants on first entry.
pub(in crate::codegen::functions::generator) fn emit_wrapper(
    emitter: &mut Emitter,
    label: &str,
    resume_label: &str,
    class_id: u64,
    int_param_count: usize,
    int_local_count: usize,
) {
    if emitter.target.arch == Arch::X86_64 {
        emit_wrapper_x86_64(
            emitter,
            label,
            resume_label,
            class_id,
            int_param_count,
            int_local_count,
        );
        return;
    }

    let total_slots = int_param_count + int_local_count;
    let frame_size = aligned_frame_size_with_slots(total_slots);

    emitter.blank();
    emitter.comment(&format!("--- generator wrapper {} ---", label));
    emitter.label_global(label);

    let param_save_bytes = if int_param_count > 0 {
        (int_param_count * 8 + 15) & !15
    } else {
        0
    };
    let prologue_bytes = 16 + param_save_bytes;
    emitter.instruction(&format!("sub sp, sp, #{}", prologue_bytes));           // reserve the wrapper's stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", param_save_bytes)); // save frame pointer and return address above the param stash
    emitter.instruction(&format!("add x29, sp, #{}", param_save_bytes));        // establish the wrapper's frame pointer

    for i in 0..int_param_count {
        emitter.instruction(&format!("str x{}, [sp, #{}]", i, i * 8));          // park parameter i in its stash slot
    }

    emitter.instruction(&format!("mov x0, #{}", frame_size));                   // total frame size including parameter and local slots
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = pointer to fresh GeneratorFrame

    emitter.instruction(&format!("mov x9, #{}", gen_frame::HEAP_KIND_GENERATOR)); // heap kind = object instance for Generator frames
    emitter.instruction("str x9, [x0, #-8]");                                   // write kind into the uniform heap header

    emitter.instruction(&format!("mov x9, #{}", class_id));                     // load Generator's compile-time class id
    emitter.instruction(&format!("str x9, [x0, #{}]", gen_frame::OFF_CLASS_ID)); // class_id at OFF_CLASS_ID

    emitter.instruction(&format!("adr x9, {}", resume_label));                  // load address of the resume function symbol
    emitter.instruction(&format!("str x9, [x0, #{}]", gen_frame::OFF_RESUME_FN)); // resume_fn at OFF_RESUME_FN

    emitter.instruction(&format!("str xzr, [x0, #{}]", gen_frame::OFF_STATE_IDX));        // state_idx + flags
    emitter.instruction(&format!("str xzr, [x0, #{}]", gen_frame::OFF_AUTO_KEY_COUNTER)); // auto_key_counter
    emitter.instruction(&format!("str xzr, [x0, #{}]", gen_frame::OFF_LAST_KEY));         // last_key (Mixed pointer, NULL until first yield)
    emitter.instruction(&format!("str xzr, [x0, #{}]", gen_frame::OFF_LAST_VALUE));       // last_value (Mixed pointer)
    emitter.instruction(&format!("str xzr, [x0, #{}]", gen_frame::OFF_RETURN_VALUE));     // return_value (Mixed pointer)
    emitter.instruction(&format!("str xzr, [x0, #{}]", gen_frame::OFF_SENT_VALUE));       // sent_value (Mixed pointer)
    emitter.instruction(&format!("str xzr, [x0, #{}]", gen_frame::OFF_DELEGATED_ITER));   // delegated_iter
    emitter.instruction(&format!("str xzr, [x0, #{}]", gen_frame::OFF_LAYOUT_ID));        // layout_id

    for i in 0..int_param_count {
        let frame_off = slot_offset(i);
        emitter.instruction(&format!("ldr x9, [sp, #{}]", i * 8));              // reload saved parameter i from the stash
        emitter.instruction(&format!("str x9, [x0, #{}]", frame_off));          // store parameter i in its frame slot
    }

    for i in 0..int_local_count {
        let frame_off = slot_offset(int_param_count + i);
        emitter.instruction(&format!("str xzr, [x0, #{}]", frame_off));         // zero-initialize local i's frame slot
    }

    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", param_save_bytes)); // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", prologue_bytes));           // release the wrapper's stack frame
    emitter.instruction("ret");                                                 // return the frame pointer
}

/// Emits the `_fn_<f>` wrapper symbol for a generator function (x86_64 path).
///
/// Allocates a fresh `GeneratorFrame` on the heap via `__rt_heap_alloc`, stamps it
/// with `class_id` and the `resume_label` address, copies integer parameters from
/// the spill slots into their frame slots, zero-initialises all local slots, and
/// returns the frame pointer in `rax`.
///
/// # Arguments
/// - `emitter` — assembly emitter
/// - `label` — global symbol name of the wrapper (e.g. `_fn_foo`)
/// - `resume_label` — global symbol name of the generator's resume function
/// - `class_id` — compile-time Generator class id written to the frame header
/// - `int_param_count` — number of integer parameters to copy into the frame
/// - `int_local_count` — number of local slots to zero-initialise
///
/// # Stack layout
/// A spill area of `param_save_bytes` (`int_param_count * 8` rounded up to 16 bytes)
/// is reserved below `rbp`. Incoming register parameters (rdi, rsi, rdx, rcx, r8,
/// r9) are spilled to the spill area; stack-passed parameters are loaded from the
/// caller frame and also spilled.
///
/// # Frame header initialisation
/// All fixed-header slots (class_id, resume_fn, state_idx/flags, auto_key_counter,
/// last_key, last_value, return_value, sent_value, delegated_iter, layout_id) are
/// written here so the resume function and runtime helpers can rely on the
/// invariants on first entry.
///
/// # ABI note
/// The x86_64 heap magic word embeds `X86_64_HEAP_MAGIC_HI32` in the upper 32 bits
/// to distinguish generator frames from other heap objects during debugging.
fn emit_wrapper_x86_64(
    emitter: &mut Emitter,
    label: &str,
    resume_label: &str,
    class_id: u64,
    int_param_count: usize,
    int_local_count: usize,
) {
    let total_slots = int_param_count + int_local_count;
    let frame_size = aligned_frame_size_with_slots(total_slots);
    let param_save_bytes = if int_param_count > 0 {
        (int_param_count * 8 + 15) & !15
    } else {
        0
    };
    let arg_regs = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];

    emitter.blank();
    emitter.comment(&format!("--- generator wrapper {} ---", label));
    emitter.label_global(label);

    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the wrapper frame pointer
    if param_save_bytes > 0 {
        emitter.instruction(&format!("sub rsp, {}", param_save_bytes));         // reserve a spill area for incoming generator parameters
    }

    for i in 0..int_param_count {
        if i < arg_regs.len() {
            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], {}", (i + 1) * 8, arg_regs[i])); // park register parameter i in its spill slot
        } else {
            let caller_off = 16 + (i - arg_regs.len()) * 8;
            emitter.instruction(&format!("mov r10, QWORD PTR [rbp + {}]", caller_off)); // load stack-passed parameter i from the caller frame
            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", (i + 1) * 8)); // park stack-passed parameter i in its spill slot
        }
    }

    emitter.instruction(&format!("mov rax, {}", frame_size));                   // total frame size including parameter and local slots
    emitter.instruction("call __rt_heap_alloc");                                // rax = pointer to fresh GeneratorFrame

    emitter.instruction(&format!("mov r10, 0x{:x}", (super::X86_64_HEAP_MAGIC_HI32 << 32) | u64::from(gen_frame::HEAP_KIND_GENERATOR))); // heap kind = object instance with x86 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // write kind into the uniform heap header

    emitter.instruction(&format!("mov r10, {}", class_id));                     // load Generator's compile-time class id
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], r10", gen_frame::OFF_CLASS_ID)); // class_id at OFF_CLASS_ID

    crate::codegen::abi::emit_symbol_address(emitter, "r10", resume_label);
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], r10", gen_frame::OFF_RESUME_FN)); // resume_fn at OFF_RESUME_FN

    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", gen_frame::OFF_STATE_IDX)); // state_idx + flags
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", gen_frame::OFF_AUTO_KEY_COUNTER)); // auto_key_counter
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", gen_frame::OFF_LAST_KEY)); // last_key (Mixed pointer, NULL until first yield)
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", gen_frame::OFF_LAST_VALUE)); // last_value (Mixed pointer)
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", gen_frame::OFF_RETURN_VALUE)); // return_value (Mixed pointer)
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", gen_frame::OFF_SENT_VALUE)); // sent_value (Mixed pointer)
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", gen_frame::OFF_DELEGATED_ITER)); // delegated_iter
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", gen_frame::OFF_LAYOUT_ID)); // layout_id

    for i in 0..int_param_count {
        let frame_off = slot_offset(i);
        emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", (i + 1) * 8)); // reload saved parameter i from the spill slot
        emitter.instruction(&format!("mov QWORD PTR [rax + {}], r10", frame_off)); // store parameter i in its frame slot
    }

    for i in 0..int_local_count {
        let frame_off = slot_offset(int_param_count + i);
        emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", frame_off)); // zero-initialize local i's frame slot
    }

    if param_save_bytes > 0 {
        emitter.instruction(&format!("add rsp, {}", param_save_bytes));         // release the parameter spill area
    }
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the frame pointer in rax
}
