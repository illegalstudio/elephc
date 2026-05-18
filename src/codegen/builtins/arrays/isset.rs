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
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

const NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;

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

fn emit_loaded_result_isset(ty: &PhpType, emitter: &mut Emitter) {
    match ty.codegen_repr() {
        PhpType::Void | PhpType::Never => emit_bool_result(false, emitter),
        PhpType::Mixed => emit_mixed_result_not_null(emitter),
        PhpType::Int | PhpType::Bool => emit_scalar_result_not_null(emitter),
        _ => emit_bool_result(true, emitter),
    }
}

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

fn load_indexed_array_element_pointer(array_reg: &str, index_reg: &str, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed array header to reach element storage
            emitter.instruction(&format!("ldr x0, [{}, {}, lsl #3]", array_reg, index_reg)); // load the boxed Mixed element pointer for null inspection
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip the indexed array header to reach element storage
            emitter.instruction(&format!("mov rax, QWORD PTR [{} + {} * 8]", array_reg, index_reg)); // load the boxed Mixed element pointer for null inspection
        }
    }
}

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

fn emit_bool_result(value: bool, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(if value { "mov x0, #1" } else { "mov x0, #0" }); // materialize the isset boolean result on AArch64
        }
        Arch::X86_64 => {
            emitter.instruction(if value { "mov rax, 1" } else { "xor eax, eax" }); // materialize the isset boolean result on x86_64
        }
    }
}
