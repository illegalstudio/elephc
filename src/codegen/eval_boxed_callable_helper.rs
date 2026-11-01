//! Purpose:
//! Emits the shared helper that turns a boxed eval-owned Closure into a callable descriptor.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when the eval bridge is linked.
//! - `crate::codegen::runtime_callable_invoker`, which calls it for a `callable` parameter fed
//!   a boxed value by the interpreter.
//!
//! Key details:
//! - A `callable` ABI slot holds a descriptor pointer. A Closure the interpreter created is an
//!   object whose payload is not one: the compiled callee read its "invoke" slot out of the
//!   object's bytes and jumped to garbage (`run(fn ($x) => ...)` from `eval()` into a compiled
//!   `function run($cb) { return $cb(5); }` whose other callers passed compiled closures).
//! - The descriptor is the same callback-adapter descriptor the eval-to-native bridges build: it
//!   captures the owning eval context (recovered from the Closure's identity, because a plain
//!   function call carries none) and the boxed callback, and dispatches back through magician.
//! - The helper returns 0 for anything it cannot adapt, and the caller then keeps the previous
//!   behavior for that value.

use crate::codegen::abi;
use crate::codegen::callable_descriptor;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Global label of the shared helper: boxed Mixed in the first argument register, descriptor
/// pointer (or 0) in the integer result register.
pub(crate) const EVAL_BOXED_CALLABLE_DESCRIPTOR_LABEL: &str = "__elephc_eval_boxed_callable_descriptor";

const CONTEXT_OFFSET: usize = 0;
const CALLBACK_OFFSET: usize = 16;
const SCRATCH_BYTES: usize = 32;
const CAPTURE_BYTES: usize = 32;
const CONTEXT_CAPTURE: usize = 0;
const CALLBACK_CAPTURE: usize = 1;

/// Emits the helper body once per program.
pub(super) fn emit_eval_boxed_callable_descriptor_helper(emitter: &mut Emitter, data: &mut DataSection) {
    let descriptor_label = crate::codegen::eval_callable_helpers::eval_dynamic_callable_descriptor(data);
    let capture_offset = callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET;
    let total_bytes = capture_offset + CAPTURE_BYTES;
    let result_reg = abi::int_result_reg(emitter).to_string();
    let descriptor_reg = abi::nested_call_reg(emitter).to_string();
    let arg0 = abi::int_arg_reg_name(emitter.target, 0).to_string();
    let arg1 = abi::int_arg_reg_name(emitter.target, 1).to_string();
    let failed_label = "__elephc_eval_boxed_callable_descriptor_failed";
    let done_label = "__elephc_eval_boxed_callable_descriptor_done";

    emitter.blank();
    emitter.comment("--- eval bridge: boxed eval Closure to callable descriptor ---");
    emitter.raw(".align 2");
    emitter.label_global(EVAL_BOXED_CALLABLE_DESCRIPTOR_LABEL);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // save frame pointer and return address
            emitter.instruction("mov x29, sp");                                 // establish the helper frame pointer
            emitter.instruction(&format!("str {}, [sp, #-16]!", descriptor_reg)); // preserve the caller's nested-call register
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame pointer
            emitter.instruction(&format!("push {}", descriptor_reg));           // preserve the caller's nested-call register
            emitter.instruction("sub rsp, 8");                                  // restore 16-byte alignment before nested calls
        }
    }
    abi::emit_reserve_temporary_stack(emitter, SCRATCH_BYTES);
    abi::emit_store_to_sp(emitter, &arg0, CALLBACK_OFFSET);
    let owner_symbol = emitter.target.extern_symbol("__elephc_eval_callable_owner_context");
    abi::emit_call_label(emitter, &owner_symbol);
    abi::emit_store_to_sp(emitter, &result_reg, CONTEXT_OFFSET);
    abi::emit_branch_if_int_result_zero(emitter, failed_label);

    abi::emit_load_temporary_stack_slot(emitter, &arg0, CONTEXT_OFFSET);
    abi::emit_load_temporary_stack_slot(emitter, &arg1, CALLBACK_OFFSET);
    let is_callable_symbol = emitter.target.extern_symbol("__elephc_eval_is_callable");
    abi::emit_call_label(emitter, &is_callable_symbol);
    abi::emit_branch_if_int_result_zero(emitter, failed_label);

    // The descriptor takes an owner on the callback cell; the caller's cell is borrowed.
    abi::emit_load_temporary_stack_slot(emitter, &result_reg, CALLBACK_OFFSET);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_load_int_immediate(emitter, &result_reg, total_bytes as i64);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    abi::emit_reg_move(emitter, &descriptor_reg, &result_reg);
    callable_descriptor::emit_copy_static_descriptor_to_runtime(emitter, &descriptor_reg, &descriptor_label);
    let scratch = match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    };
    abi::emit_load_temporary_stack_slot(emitter, scratch, CONTEXT_OFFSET);
    abi::emit_store_to_address(emitter, scratch, &descriptor_reg, capture_offset + CONTEXT_CAPTURE * 16);
    abi::emit_load_temporary_stack_slot(emitter, scratch, CALLBACK_OFFSET);
    abi::emit_store_to_address(emitter, scratch, &descriptor_reg, capture_offset + CALLBACK_CAPTURE * 16);
    abi::emit_reg_move(emitter, &result_reg, &descriptor_reg);
    abi::emit_jump(emitter, done_label);

    emitter.label(failed_label);
    abi::emit_load_int_immediate(emitter, &result_reg, 0);
    emitter.label(done_label);
    abi::emit_release_temporary_stack(emitter, SCRATCH_BYTES);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [sp], #16", descriptor_reg)); // restore the caller's nested-call register
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore frame pointer and return address
            emitter.instruction("ret");                                         // return the descriptor, or 0
        }
        Arch::X86_64 => {
            emitter.instruction("add rsp, 8");                                  // release the alignment padding
            emitter.instruction(&format!("pop {}", descriptor_reg));            // restore the caller's nested-call register
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the descriptor, or 0
        }
    }
}
