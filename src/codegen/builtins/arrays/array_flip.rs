//! Purpose:
//! Emits PHP `array_flip` builtin calls over associative or key-aware array data.
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
use crate::types::{array_key_type_from_value_type, PhpType};

/// Emits code for the PHP `array_flip` builtin, which exchanges array keys and values.
///
/// # Arguments
/// - `_name`: Unused; matches the dispatcher signature (builtin name is resolved via catalog).
/// - `args`: Must contain exactly one expression producing an array.
/// - `emitter`: Target-aware instruction emitter.
/// - `ctx`: Codegen context (types, locals, class metadata).
/// - `data`: Data section for relocations and static data.
///
/// # Returns
/// `Some(PhpType)` describing the flipped array type:
/// - `Array<Str>` → `AssocArray<Int, Int>` (string keys flipped to integer values)
/// - `AssocArray<K, V>` → `AssocArray<V, K>` (swaps key and value types)
/// - Other arrays → `AssocArray<Int, Int>` (homogeneous fallback)
///
/// # Runtime helpers
/// - `__rt_array_flip_string`: Used when flipping an `Array<Str>` (all string keys).
/// - `__rt_array_flip`: Used for all other array types.
///
/// # ABI notes
/// - ARM64: passes array pointer in `x0`, result returned in `x0` via `bl helper`.
/// - x86_64: moves array pointer to `rdi` before calling, result in `rax`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_flip()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let result_ty = match &arr_ty {
        PhpType::Array(value) => PhpType::AssocArray {
            key: Box::new(array_key_type_from_value_type(*value.clone())),
            value: Box::new(PhpType::Int),
        },
        PhpType::AssocArray { key, value } => PhpType::AssocArray {
            key: Box::new(array_key_type_from_value_type(*value.clone())),
            value: key.clone(),
        },
        _ => PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Int),
        },
    };
    let helper = match &arr_ty {
        PhpType::Array(value) if matches!(value.as_ref(), PhpType::Str) => {
            "__rt_array_flip_string"
        }
        _ => "__rt_array_flip",
    };

    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the source indexed array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, helper);                                  // flip the indexed array into an associative array through the selected runtime helper
        return Some(result_ty);
    }

    // -- call runtime to swap keys and values --
    emitter.instruction(&format!("bl {}", helper));                             // call runtime: flip array → x0=new assoc array

    Some(result_ty)
}
