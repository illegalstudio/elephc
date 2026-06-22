//! Purpose:
//! Emits PHP `isset` checks without evaluating to ordinary truthiness.
//! Owns null/unset sentinel handling for variables and array element probes.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Must distinguish PHP null/unset semantics from false, zero, and empty string values.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::codegen::NULL_SENTINEL;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// The null sentinel value used to represent PHP `null` in scalar runtime representations.
///
/// This value is distinct from all valid PHP scalar values (integers, booleans, floats)
/// and is used by the runtime to distinguish a loaded null from false, zero, or empty.

/// Emits PHP `isset(...)` for one or more arguments.
///
/// Returns `PhpType::Int` to indicate the result is always treated as integer (0 or 1).
/// When multiple arguments are given, all must be set for the result to be true.
///
/// # Arguments
/// * `_name` - Unused; included for parity with the builtin call signature dispatcher.
/// * `args` - The PHP expressions to check for set-ness.
/// * `emitter` - The assembly emitter.
/// * `ctx` - Codegen context (labels, scope).
/// * `data` - Data section for constants and runtime symbols.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("isset()");
    if args.is_empty() {
        emit_bool_result(false, emitter);
        return Some(PhpType::Int);
    }

    let false_label = ctx.next_label("isset_false");
    let done_label = ctx.next_label("isset_done");
    for (idx, arg) in args.iter().enumerate() {
        emit_isset_arg(arg, emitter, ctx, data);
        if idx + 1 < args.len() {
            abi::emit_branch_if_int_result_zero(emitter, &false_label);
        }
    }

    if args.len() > 1 {
        abi::emit_jump(emitter, &done_label);
        emitter.label(&false_label);
        emit_bool_result(false, emitter);
        emitter.label(&done_label);
    }

    Some(PhpType::Int)
}

/// Emits `isset` checks for a single argument expression.
///
/// Dispatches to the appropriate specialized emitter based on the expression kind:
/// - `ArrayAccess` on array/object types → object offset or array element check
/// - `ArrayAccess` on strings → string offset bounds check
/// - Other expressions → null-sentinel or type-based check on the loaded value
fn emit_isset_arg(
    arg: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if let ExprKind::ArrayAccess { array, index } = &arg.kind {
        let array_ty = crate::codegen::functions::infer_contextual_type(array, ctx);
        if crate::codegen::expr::arrays::type_is_array_access_object(&array_ty, ctx) {
            crate::codegen::expr::arrays::emit_array_access_offset_exists(
                array, index, emitter, ctx, data,
            );
            return;
        }

        match &array_ty {
            PhpType::Str => {
                emit_expr(arg, emitter, ctx, data);
                emit_string_offset_isset_result(emitter);
                return;
            }
            PhpType::Array(elem_ty) => {
                emit_indexed_array_isset(array, index, elem_ty, emitter, ctx, data);
                return;
            }
            PhpType::AssocArray { value, .. } => {
                emit_assoc_array_isset(array, index, value, emitter, ctx, data);
                return;
            }
            PhpType::Mixed => {
                emit_expr(arg, emitter, ctx, data);
                emit_mixed_result_not_null(emitter);
                return;
            }
            _ => {}
        }
    }

    let ty = emit_expr(arg, emitter, ctx, data);
    emit_loaded_result_isset(&ty, emitter);
}

/// Emits an `isset` check on a value whose type is already known.
///
/// Uses the type's codegen representation to determine null-ness:
/// - `Void`/`Never` → false (these types cannot hold values)
/// - `Mixed` → runtime unbox and null tag check
/// - `Int`/`Bool` → compare against the null sentinel
/// - All other types → true (e.g., arrays, objects, resources always exist)
fn emit_loaded_result_isset(ty: &PhpType, emitter: &mut Emitter) {
    match ty.codegen_repr() {
        PhpType::Void | PhpType::Never => emit_bool_result(false, emitter),
        PhpType::Mixed => emit_mixed_result_not_null(emitter),
        PhpType::TaggedScalar => emit_tagged_scalar_result_not_null(emitter),
        PhpType::Int | PhpType::Bool if crate::codegen::sentinels::null_repr_is_tagged() => {
            emit_bool_result(true, emitter)
        }
        PhpType::Int | PhpType::Bool => emit_scalar_result_not_null(emitter),
        _ => emit_bool_result(true, emitter),
    }
}

/// Emits an `isset` check for an indexed array element access.
///
/// Loads the array pointer and index, validates the index is non-negative and within
/// bounds, then checks the element type to determine null-ness. Uses the null sentinel
/// for `Mixed` elements and unconditionally returns true for other non-void types.
fn emit_indexed_array_isset(
    array: &Expr,
    index: &Expr,
    elem_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emit_expr(array, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the indexed array pointer while evaluating the index expression
    emit_expr(index, emitter, ctx, data);
    let array_reg = abi::symbol_scratch_reg(emitter);
    let len_reg = abi::secondary_scratch_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let false_label = ctx.next_label("isset_array_false");
    let done_label = ctx.next_label("isset_array_done");
    abi::emit_pop_reg(emitter, array_reg);                                      // restore the indexed array pointer for the bounds probe

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #0", result_reg));            // reject negative indexes as missing array elements
            emitter.instruction(&format!("b.lt {}", false_label));              // return false when the requested index is negative
            abi::emit_load_from_address(emitter, len_reg, array_reg, 0);
            emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));   // compare the requested index against the array length
            emitter.instruction(&format!("b.ge {}", false_label));              // return false when the requested index is out of bounds
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, 0", result_reg));             // reject negative indexes as missing array elements
            emitter.instruction(&format!("jl {}", false_label));                // return false when the requested index is negative
            abi::emit_load_from_address(emitter, len_reg, array_reg, 0);
            emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));   // compare the requested index against the array length
            emitter.instruction(&format!("jge {}", false_label));               // return false when the requested index is out of bounds
        }
    }

    match elem_ty.codegen_repr() {
        PhpType::Void | PhpType::Never => emit_bool_result(false, emitter),
        PhpType::Mixed => {
            load_indexed_array_element_pointer(array_reg, result_reg, emitter);
            emit_mixed_result_not_null(emitter);
        }
        _ => emit_bool_result(true, emitter),
    }
    abi::emit_jump(emitter, &done_label);
    emitter.label(&false_label);
    emit_bool_result(false, emitter);
    emitter.label(&done_label);
}

/// Computes the element pointer for an indexed array element on AArch64 or x86_64.
///
/// Adds the indexed array header size (24 bytes) to the array pointer to skip the
/// length field and type tag, then loads the boxed `Mixed` element pointer at
/// `element_base + index * 8`.
///
/// # Arguments
/// * `array_reg` - Register holding the indexed array pointer (modified in place).
/// * `index_reg` - Register holding the element index.
/// * `emitter` - The assembly emitter.
fn load_indexed_array_element_pointer(array_reg: &str, index_reg: &str, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); //skip the indexed array header to reach element storage
            emitter.instruction(&format!("ldr x0, [{}, {}, lsl #3]", array_reg, index_reg)); //load the boxed Mixed element pointer for null inspection
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); //skip the indexed array header to reach element storage
            emitter.instruction(&format!("mov rax, QWORD PTR [{} + {} * 8]", array_reg, index_reg)); //load the boxed Mixed element pointer for null inspection
        }
    }
}

/// Emits an `isset` check for an associative array element access.
///
/// Normalizes the index expression to a string key, calls `__rt_hash_get` to probe
/// the hash table, then checks whether the lookup succeeded and the value is not null.
fn emit_assoc_array_isset(
    array: &Expr,
    index: &Expr,
    _value_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emit_expr(array, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // preserve the hash-table pointer while evaluating the offset expression
    crate::codegen::emit_normalized_hash_key(index, emitter, ctx, data);
    let (key_ptr_reg, key_len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, key_ptr_reg, key_len_reg);                // preserve the normalized key while restoring the hash-table pointer
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg_pair(emitter, "x1", "x2");                       // restore the normalized key into hash-get argument registers
            abi::emit_pop_reg(emitter, "x0");                                  // restore the hash-table pointer into the hash-get receiver argument
        }
        Arch::X86_64 => {
            abi::emit_pop_reg_pair(emitter, "rsi", "rdx");                     // restore the normalized key into hash-get argument registers
            abi::emit_pop_reg(emitter, "rdi");                                 // restore the hash-table pointer into the hash-get receiver argument
        }
    }
    abi::emit_call_label(emitter, "__rt_hash_get");                            // return the hash lookup found flag plus borrowed payload metadata
    emit_hash_found_and_not_null(emitter, ctx);
}

/// Emits post-hash-lookup null check after `__rt_hash_get` returns.
///
/// Consumes the runtime return values from `__rt_hash_get`:
/// - x86_64: `rax` = found flag, `rcx` = value tag
/// - AArch64: `x0` = found flag, `x3` = value tag
///
/// Emits true when the key was found AND the value tag is not 8 (PHP null).
fn emit_hash_found_and_not_null(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("isset_hash_false");
    let done_label = ctx.next_label("isset_hash_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {}", false_label));           // return false when the associative lookup misses
            emitter.instruction("cmp x3, #8");                                  // runtime tag 8 means the stored value is PHP null
            emitter.instruction(&format!("b.eq {}", false_label));              // return false when the stored value is null
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // check whether the associative lookup found a matching key
            emitter.instruction(&format!("je {}", false_label));                // return false when the associative lookup misses
            emitter.instruction("cmp rcx, 8");                                  // runtime tag 8 means the stored value is PHP null
            emitter.instruction(&format!("je {}", false_label));                // return false when the stored value is null
        }
    }
    emit_bool_result(true, emitter);
    abi::emit_jump(emitter, &done_label);
    emitter.label(&false_label);
    emit_bool_result(false, emitter);
    emitter.label(&done_label);
}

/// Emits the result of an `isset` check on a string offset expression.
///
/// After evaluating a string `ArrayAccess` expression (e.g., `$s[0]`), the string
/// result registers contain the character (or null byte) and the length. This
/// function returns true only when the length is non-zero, indicating a valid
/// in-bounds offset was accessed.
fn emit_string_offset_isset_result(emitter: &mut Emitter) {
    let (_, len_reg) = abi::string_result_regs(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #0", len_reg));               // check whether string offset access produced a character
            emitter.instruction("cset x0, ne");                                 // return true only when the string offset is in bounds
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, 0", len_reg));                // check whether string offset access produced a character
            emitter.instruction("setne al");                                    // return true only when the string offset is in bounds
            emitter.instruction("movzx eax, al");                               // widen the boolean byte into the canonical integer result
        }
    }
}

/// Emits the result of an `isset` check on a `Mixed` runtime value.
///
/// Calls `__rt_mixed_unbox` to inspect the boxed `Mixed` payload tag. Returns true
/// only when the tag is not 8 (PHP null).
fn emit_mixed_result_not_null(emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // inspect the boxed Mixed payload tag for PHP null
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #8");                                  // runtime tag 8 means the Mixed payload is PHP null
            emitter.instruction("cset x0, ne");                                 // return true only when the Mixed payload is not null
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 8");                                  // runtime tag 8 means the Mixed payload is PHP null
            emitter.instruction("setne al");                                    // set the low result byte when the Mixed payload is not null
            emitter.instruction("movzx rax, al");                               // widen the Mixed null-check result into the integer result register
        }
    }
}

/// Emits the result of an `isset` check on a tagged scalar runtime value: true unless
/// the runtime tag word marks the value as PHP null.
fn emit_tagged_scalar_result_not_null(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x1, #8");                                  // runtime tag 8 means the tagged scalar is PHP null
            emitter.instruction("cset x0, ne");                                 // return true only when the tagged scalar is not null
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rdx, 8");                                  // runtime tag 8 means the tagged scalar is PHP null
            emitter.instruction("setne al");                                    // set the low result byte when the tagged scalar is not null
            emitter.instruction("movzx rax, al");                               // widen the tagged null-check result into the integer result register
        }
    }
}

/// Emits the result of an `isset` check on a scalar (Int or Bool) runtime value.
///
/// Compares the scalar result register against the null sentinel and returns true
/// only when they differ, indicating the value is not PHP null. On AArch64 uses
/// `x9` as scratch; on x86_64 uses `r10`.
fn emit_scalar_result_not_null(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(emitter, "x9", NULL_SENTINEL);
            emitter.instruction("cmp x0, x9");                                  // compare the scalar result against the shared null sentinel
            emitter.instruction("cset x0, ne");                                 // return true only when the scalar result is not null
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(emitter, "r10", NULL_SENTINEL);
            emitter.instruction("cmp rax, r10");                                // compare the scalar result against the shared null sentinel
            emitter.instruction("setne al");                                    // set the low result byte when the scalar result is not null
            emitter.instruction("movzx rax, al");                               // widen the scalar null-check result into the integer result register
        }
    }
}

/// Emits a constant boolean result for `isset`.
///
/// Materializes `value` as an integer (1 for true, 0 for false) into the canonical
/// integer result register (`x0` on AArch64, `rax` on x86_64).
fn emit_bool_result(value: bool, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(if value { "mov x0, #1" } else { "mov x0, #0" }); //materialize the isset boolean result on AArch64
        }
        Arch::X86_64 => {
            emitter.instruction(if value { "mov rax, 1" } else { "xor eax, eax" }); //materialize the isset boolean result on x86_64
        }
    }
}
