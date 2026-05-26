//! Purpose:
//! Emits PHP `array_key_exists` builtin calls over associative or key-aware array data.
//! Owns key/value payload setup and runtime hash-helper invocation for array results or lookups.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Array key typing and Mixed payload tags must match the runtime hash-table representation.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `array_key_exists($key, $array)` builtin call.
///
/// Dispatches to a different runtime helper based on the array's PHP type:
/// - `AssocArray`: pushes the hash-table pointer, emits the key as a normalized string,
///   then restores both into ABI registers and calls `__rt_hash_get` to check key presence.
/// - Indexed array: pushes the array pointer, evaluates the integer key, then restores both
///   into helper registers and calls `__rt_array_key_exists` to check bounds.
///
/// Preserves evaluation order by using the stack to save the first argument while computing
/// the second, then materializes all arguments into ABI registers before the call.
/// Returns `PhpType::Bool` in the integer result register on both paths.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_key_exists()");

    // -- evaluate the array (second arg) first to get its type --
    let arr_ty = emit_expr(&args[1], emitter, ctx, data);

    if matches!(arr_ty, PhpType::AssocArray { .. }) {
        // -- associative array: use hash_get to check if key exists --
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the hash table pointer while evaluating the associative-array key expression
        crate::codegen::emit_normalized_hash_key(&args[0], emitter, ctx, data);
        let (key_ptr_reg, key_len_reg) = abi::string_result_regs(emitter);
        abi::emit_push_reg_pair(emitter, key_ptr_reg, key_len_reg);             // preserve the computed associative-array key while restoring the hash-table pointer
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_pop_reg_pair(emitter, "x1", "x2");                    // restore the associative-array key pointer and length into the hash-get helper registers
                abi::emit_pop_reg(emitter, "x0");                               // restore the associative-array hash-table pointer into the first hash-get helper register
            }
            Arch::X86_64 => {
                abi::emit_pop_reg_pair(emitter, "rsi", "rdx");                  // restore the associative-array key pointer and length into the SysV hash-get helper registers
                abi::emit_pop_reg(emitter, "rdi");                              // restore the associative-array hash-table pointer into the first SysV hash-get helper register
            }
        }
        abi::emit_call_label(emitter, "__rt_hash_get");                         // lookup the associative-array key and leave the found flag in the integer result register
    } else {
        // -- indexed array: check if integer key is in bounds --
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the indexed-array pointer while evaluating the integer key expression
        emit_expr(&args[0], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x1, x0");                              // move the integer key into the indexed-array key-exists helper argument register
                abi::emit_pop_reg(emitter, "x0");                               // restore the indexed-array pointer into the first helper argument register
            }
            Arch::X86_64 => {
                emitter.instruction("mov rsi, rax");                            // move the integer key into the second SysV helper argument register
                abi::emit_pop_reg(emitter, "rdi");                              // restore the indexed-array pointer into the first SysV helper argument register
            }
        }
        abi::emit_call_label(emitter, "__rt_array_key_exists");                 // check whether the integer key lies within the indexed-array bounds
    }

    Some(PhpType::Bool)
}
