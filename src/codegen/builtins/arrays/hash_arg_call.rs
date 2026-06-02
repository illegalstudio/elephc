//! Purpose:
//! Shared two-array argument choreography for the hash-based array builtins
//! (`array_replace`, `array_replace_recursive`, `array_diff_assoc`, `array_intersect_assoc`,
//! `array_merge_recursive`). Accepts scalar indexed-array inputs by converting them to hashes.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::{array_replace, assoc_diff_intersect, array_merge_recursive}::emit()`.
//!
//! Key details:
//! - A scalar indexed input is converted to an owned integer-keyed hash via `__rt_array_to_hash`.
//!   Converted temporaries are released with `__rt_decref_hash` after the runtime reads them; the
//!   result (independently owned) is preserved across the frees. Scalar element values carry no
//!   heap children, so freeing the converted temporaries cannot disturb the result.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Returns true if the type is an indexed array that the emitter must convert to a hash.
fn needs_conversion(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Array(_))
}

/// Converts the indexed array currently in the integer result register to an owned hash.
///
/// `__rt_array_to_hash` takes its argument in the first argument register. On AArch64 the result
/// register `x0` already is that register, but on x86_64 the result lives in `rax`, so it must be
/// moved into `rdi` before the call.
fn emit_convert_indexed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the array pointer into the first SysV argument register
    }
    abi::emit_call_label(emitter, "__rt_array_to_hash");
}

/// Emits the two-array argument choreography for a hash-based builtin and calls `runtime_label`.
///
/// Evaluates both arguments in source order. Any indexed-array argument is converted to an owned
/// hash with `__rt_array_to_hash`. When `mode` is `Some`, its value is loaded into the third
/// runtime-argument register (used by `array_diff_assoc` / `array_intersect_assoc`). Converted
/// temporaries are released with `__rt_decref_hash` after the call, preserving the result.
///
/// Leaves the result hash pointer in the integer result register and returns both arguments'
/// static types `(ty0, ty1)` so the caller can derive the builtin's result type (key/value
/// widening across the two inputs).
pub fn emit_two_hash_arg_call(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    runtime_label: &str,
    mode: Option<i64>,
) -> (PhpType, PhpType) {
    let ty0 = emit_expr(&args[0], emitter, ctx, data);
    let conv0 = needs_conversion(&ty0);
    if conv0 {
        emit_convert_indexed(emitter); // convert the indexed first argument to an owned hash
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));

    let ty1 = emit_expr(&args[1], emitter, ctx, data);
    let conv1 = needs_conversion(&ty1);
    if conv1 {
        emit_convert_indexed(emitter); // convert the indexed second argument to an owned hash
    }

    if !conv0 && !conv1 {
        // Fast path: both inputs are already hashes, no temporaries to free.
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x1, x0");                              // move the second hash pointer into the second runtime argument register
                abi::emit_pop_reg(emitter, "x0");
                if let Some(m) = mode {
                    emitter.instruction(&format!("mov x2, #{}", m));            // mode selector into the third runtime argument register
                }
            }
            Arch::X86_64 => {
                emitter.instruction("mov rsi, rax");                            // move the second hash pointer into the second SysV argument register
                abi::emit_pop_reg(emitter, "rdi");
                if let Some(m) = mode {
                    emitter.instruction(&format!("mov edx, {}", m));            // mode selector into the third SysV argument register
                }
            }
        }
        abi::emit_call_label(emitter, runtime_label);
        return (ty0, ty1);
    }

    // Freeing path: at least one input was converted to a temporary hash that must be released.
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // spill the second hash; stack holds [h2, h1]
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp, #16]");                           // load the first hash pointer (kept on the stack for freeing)
            emitter.instruction("ldr x1, [sp]");                                // load the second hash pointer (kept on the stack for freeing)
            if let Some(m) = mode {
                emitter.instruction(&format!("mov x2, #{}", m));                // mode selector into the third runtime argument register
            }
            abi::emit_call_label(emitter, runtime_label); // result hash pointer returned in x0
            emitter.instruction("str x0, [sp, #-16]!");                         // spill the result; stack holds [result, h2, h1]
            if conv1 {
                emitter.instruction("ldr x0, [sp, #16]");                       // reload the converted second hash temporary
                abi::emit_call_label(emitter, "__rt_decref_hash"); // release the converted second hash temporary
            }
            if conv0 {
                emitter.instruction("ldr x0, [sp, #32]");                       // reload the converted first hash temporary
                abi::emit_call_label(emitter, "__rt_decref_hash"); // release the converted first hash temporary
            }
            emitter.instruction("ldr x0, [sp], #16");                           // restore the result hash pointer
            emitter.instruction("add sp, sp, #32");                             // discard the two spilled input hash pointers
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");               // load the first hash pointer (kept on the stack for freeing)
            emitter.instruction("mov rsi, QWORD PTR [rsp]");                    // load the second hash pointer (kept on the stack for freeing)
            if let Some(m) = mode {
                emitter.instruction(&format!("mov edx, {}", m));                // mode selector into the third SysV argument register
            }
            abi::emit_call_label(emitter, runtime_label); // result hash pointer returned in rax
            emitter.instruction("sub rsp, 16");                                 // reserve a slot for the result
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // spill the result; stack holds [result, h2, h1]
            if conv1 {
                emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // reload the converted second hash temporary
                abi::emit_call_label(emitter, "__rt_decref_hash"); // release the converted second hash temporary
            }
            if conv0 {
                emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");           // reload the converted first hash temporary
                abi::emit_call_label(emitter, "__rt_decref_hash"); // release the converted first hash temporary
            }
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // restore the result hash pointer
            emitter.instruction("add rsp, 48");                                 // discard the result slot and the two spilled inputs
        }
    }
    (ty0, ty1)
}
