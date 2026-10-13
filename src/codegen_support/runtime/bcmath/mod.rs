//! Purpose:
//! Emits shared BCMath bridge trampolines, ABI marshalling helpers, and throwable construction.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` for every generated runtime.
//!
//! Key details:
//! - Public `__rt_bc*` labels load late-bound bridge slots before tail-entering shape-specific code.
//! - Raw target bodies contain decimal-free marshalling only; arithmetic remains in elephc-bcmath.
//! - Dynamic crate messages are persisted before `ValueError` or `DivisionByZeroError` is thrown.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits every public BCMath runtime entry and its target-specific shared implementation.
pub(crate) fn emit_bcmath(emitter: &mut Emitter) {
    for (label, slot, common) in [
        ("__rt_bcadd", "_elephc_bcmath_add_fn", "__rt_bcmath_binary"),
        ("__rt_bcsub", "_elephc_bcmath_sub_fn", "__rt_bcmath_binary"),
        ("__rt_bcmul", "_elephc_bcmath_mul_fn", "__rt_bcmath_binary"),
        ("__rt_bcdiv", "_elephc_bcmath_div_fn", "__rt_bcmath_binary"),
        ("__rt_bcmod", "_elephc_bcmath_mod_fn", "__rt_bcmath_binary"),
        ("__rt_bcpow", "_elephc_bcmath_pow_fn", "__rt_bcmath_binary"),
        ("__rt_bcsqrt", "_elephc_bcmath_sqrt_fn", "__rt_bcmath_unary_scaled"),
        ("__rt_bcceil", "_elephc_bcmath_ceil_fn", "__rt_bcmath_unary"),
        ("__rt_bcfloor", "_elephc_bcmath_floor_fn", "__rt_bcmath_unary"),
        ("__rt_bcround", "_elephc_bcmath_round_fn", "__rt_bcmath_round"),
        ("__rt_bccomp", "_elephc_bcmath_comp_fn", "__rt_bcmath_comp"),
        (
            "__rt_bcdivmod",
            "_elephc_bcmath_divmod_fn",
            "__rt_bcmath_divmod",
        ),
        (
            "__rt_bcpowmod",
            "_elephc_bcmath_powmod_fn",
            "__rt_bcmath_powmod",
        ),
        (
            "__rt_bcscale_get",
            "_elephc_bcmath_get_scale_fn",
            "__rt_bcmath_scale_get",
        ),
        (
            "__rt_bcscale_set",
            "_elephc_bcmath_set_scale_fn",
            "__rt_bcmath_scale_set",
        ),
    ] {
        emit_bridge_entry(emitter, label, slot, common);
    }
    emit_bridge_tail_call(
        emitter,
        "__rt_bcmath_call_free",
        "_elephc_bcmath_free_fn",
    );
    emit_bridge_tail_call(
        emitter,
        "__rt_bcmath_call_last_error",
        "_elephc_bcmath_last_error_fn",
    );

    emitter.blank();
    emitter.comment("--- runtime: shared bcmath ABI helpers ---");
    match emitter.target.arch {
        Arch::AArch64 => emitter.raw(include_str!("aarch64.s")),
        Arch::X86_64 => emitter.raw(include_str!("x86_64.s")),
    }
    emit_throw_dynamic_bcmath_error(emitter);
}

/// Emits one public bridge-backed helper entry that passes its C function pointer to common code.
fn emit_bridge_entry(emitter: &mut Emitter, label: &str, slot: &str, common: &str) {
    emitter.blank();
    emitter.label_global(label);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", slot, 0);
            emitter.instruction(&format!("b {}", common));                      // tail-enter the shared BCMath argument-shape helper
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "rcx", slot, 0);             // load the late-bound BCMath bridge entry pointer
            emitter.instruction(&format!("jmp {}", common));                    // tail-enter the shared BCMath argument-shape helper
        }
    }
}

/// Emits a late-bound tail-call trampoline for bridge utilities shared by several helpers.
fn emit_bridge_tail_call(emitter: &mut Emitter, label: &str, slot: &str) {
    emitter.blank();
    emitter.label_global(label);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", slot, 0);
            emitter.instruction("br x9");                                       // tail-call the published BCMath utility entry
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "rcx", slot, 0);             // load the published BCMath utility entry pointer
            emitter.instruction("jmp rcx");                                     // tail-call the published BCMath utility entry
        }
    }
}

/// Emits the target-specific dynamic BCMath throwable constructor.
fn emit_throw_dynamic_bcmath_error(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_bcmath_throw");
    match emitter.target.arch {
        Arch::AArch64 => emit_throw_dynamic_bcmath_error_aarch64(emitter),
        Arch::X86_64 => emit_throw_dynamic_bcmath_error_x86_64(emitter),
    }
}

/// Emits AArch64 construction of a BCMath ValueError or DivisionByZeroError.
fn emit_throw_dynamic_bcmath_error_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #80");                                     // reserve status, message, object, and saved-register slots
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame before bridge/error helpers
    emitter.instruction("add x29, sp, #64");                                    // establish the dynamic-error helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the stable BCMath status code
    emitter.instruction("add x0, sp, #8");                                      // C arg0 writes the thread-local error pointer
    emitter.instruction("add x1, sp, #16");                                     // C arg1 writes the thread-local error length
    emitter.instruction("bl __rt_bcmath_call_last_error");                      // borrow the crate-owned PHP-compatible error message
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // load the borrowed message in PHP string registers
    emitter.instruction("bl __rt_str_persist");                                 // copy the message into refcounted PHP storage
    emitter.instruction("stp x1, x2, [sp, #24]");                               // preserve the owned message across object allocation
    emitter.instruction("mov x0, #56");                                         // request the standard Throwable payload size
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the BCMath throwable object
    emitter.instruction("mov x9, #6");                                          // heap kind 6 identifies object payloads
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the throwable allocation as an object
    emitter.instruction("bl __rt_object_handle_acquire");                       // assign the throwable its PHP-visible object handle
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the BCMath status for class selection
    emitter.instruction("cmp x10, #3");                                         // status 3 is the DivisionByZeroError family
    emitter.instruction("b.eq __rt_bcmath_throw_div_zero_class");               // select DivisionByZeroError for division-like failures
    abi::emit_symbol_address(emitter, "x9", "_spl_value_error_class_id");
    emitter.instruction("b __rt_bcmath_throw_class_ready");                     // keep every other BCMath failure as ValueError
    emitter.label("__rt_bcmath_throw_div_zero_class");
    abi::emit_symbol_address(
        emitter,
        "x9",
        "_spl_division_by_zero_error_class_id",
    );
    emitter.label("__rt_bcmath_throw_class_ready");
    emitter.instruction("ldr x9, [x9]");                                        // load the selected program-local exception class id
    emitter.instruction("str x9, [x0]");                                        // install the selected throwable class id
    emitter.instruction("ldp x10, x11, [sp, #24]");                             // reload the owned dynamic message pair
    emitter.instruction("stp x10, x11, [x0, #8]");                              // install the throwable message pointer and length
    emitter.instruction("str xzr, [x0, #24]");                                  // exception code defaults to zero
    crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "x0");
    emitter.instruction("str xzr, [x0, #40]");                                  // previous exception defaults to null
    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);              // publish the active BCMath throwable
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame before unwinding
    emitter.instruction("add sp, sp, #80");                                     // discard the dynamic-error helper frame
    emitter.instruction("b __rt_throw_current");                                // enter the standard catchable exception path
}

/// Emits x86_64 construction of a BCMath ValueError or DivisionByZeroError.
fn emit_throw_dynamic_bcmath_error_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned dynamic-error frame
    emitter.instruction("sub rsp, 64");                                         // reserve status, message, and object spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the stable BCMath status code
    emitter.instruction("lea rdi, [rbp - 16]");                                 // C arg0 writes the thread-local error pointer
    emitter.instruction("lea rsi, [rbp - 24]");                                 // C arg1 writes the thread-local error length
    emitter.instruction("call __rt_bcmath_call_last_error");                    // borrow the crate-owned PHP-compatible error message
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the borrowed message pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // load the borrowed message length
    emitter.instruction("call __rt_str_persist");                               // copy the message into refcounted PHP storage
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the owned message pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // preserve the owned message length
    emitter.instruction("mov rax, 56");                                         // request the standard Throwable payload size
    emitter.instruction("call __rt_heap_alloc");                                // allocate the BCMath throwable object
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))); // materialize the canonical throwable heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocation as an object payload
    emitter.instruction("call __rt_object_handle_acquire");                     // assign the throwable its PHP-visible object handle
    emitter.instruction("cmp QWORD PTR [rbp - 8], 3");                          // status 3 is the DivisionByZeroError family
    emitter.instruction("je __rt_bcmath_throw_div_zero_class_x86_64");          // select DivisionByZeroError for division-like failures
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_value_error_class_id", 0);
    emitter.instruction("jmp __rt_bcmath_throw_class_ready_x86_64");            // keep every other BCMath failure as ValueError
    emitter.label("__rt_bcmath_throw_div_zero_class_x86_64");
    abi::emit_load_symbol_to_reg(
        emitter,
        "r10",
        "_spl_division_by_zero_error_class_id",
        0,
    );
    emitter.label("__rt_bcmath_throw_class_ready_x86_64");
    emitter.instruction("mov QWORD PTR [rax], r10");                            // install the selected throwable class id
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the owned dynamic message pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // install the throwable message pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the owned dynamic message length
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // install the throwable message length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // exception code defaults to zero
    crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "rax");
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // previous exception defaults to null
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);          // publish the active BCMath throwable
    emitter.instruction("mov rsp, rbp");                                        // release the dynamic-error frame before unwinding
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_throw_current");                              // enter the standard catchable exception path
}
