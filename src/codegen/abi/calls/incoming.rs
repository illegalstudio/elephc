//! Purpose:
//! Stores function entry parameters from ABI registers or caller stack into compiler local slots.
//! Handles scalar, float, string-pair, and aggregate parameter shapes for each target.
//!
//! Called from:
//! - `crate::codegen::functions` during function and wrapper prologue emission
//!
//! Key details:
//! - Incoming cursor state must match outgoing assignment rules or calls will corrupt frame slots.

use crate::codegen::{
    emit::Emitter,
    platform::Arch,
};
use crate::types::PhpType;

use super::super::frame::{load_from_caller_stack, store_at_offset};
use super::super::registers::{
    IncomingArgCursor, float_arg_reg_limit, float_arg_reg_name, int_arg_reg_limit,
    int_arg_reg_name, secondary_scratch_reg, tertiary_scratch_reg,
};

pub fn emit_store_incoming_param(
    emitter: &mut Emitter,
    name: &str,
    ty: &PhpType,
    offset: usize,
    is_ref: bool,
    cursor: &mut IncomingArgCursor,
) {
    let ty = ty.codegen_repr();
    let float_spill_reg = match emitter.target.arch {
        Arch::AArch64 => "d15",
        Arch::X86_64 => "xmm15",
    };
    let int_spill_reg = secondary_scratch_reg(emitter);
    let int_hi_spill_reg = tertiary_scratch_reg(emitter);
    let int_reg_limit = int_arg_reg_limit(emitter.target);
    let float_reg_limit = float_arg_reg_limit(emitter.target);

    if is_ref {
        if !cursor.int_stack_only && cursor.int_reg_idx < int_reg_limit {
            let reg = int_arg_reg_name(emitter.target, cursor.int_reg_idx);
            emitter.comment(&format!("param &${} from {} (ref)", name, reg));
            store_at_offset(emitter, reg, offset);                                     // save the by-reference address from the incoming integer argument register
            cursor.int_reg_idx += 1;
        } else {
            emitter.comment(&format!(
                "param &${} from caller stack +{}",
                name,
                cursor.caller_stack_offset
            ));
            load_from_caller_stack(emitter, int_spill_reg, cursor.caller_stack_offset);
            store_at_offset(emitter, int_spill_reg, offset);                           // save the spilled by-reference address into the local param slot
            cursor.caller_stack_offset += 16;
            cursor.int_stack_only = true;
        }
        return;
    }

    match ty {
        PhpType::Bool | PhpType::Int | PhpType::Resource(_) => {
            if !cursor.int_stack_only && cursor.int_reg_idx < int_reg_limit {
                let reg = int_arg_reg_name(emitter.target, cursor.int_reg_idx);
                emitter.comment(&format!("param ${} from {}", name, reg));
                store_at_offset(emitter, reg, offset);                                 // save the scalar parameter from the incoming integer argument register
                cursor.int_reg_idx += 1;
            } else {
                emitter.comment(&format!(
                    "param ${} from caller stack +{}",
                    name,
                    cursor.caller_stack_offset
                ));
                load_from_caller_stack(emitter, int_spill_reg, cursor.caller_stack_offset);
                store_at_offset(emitter, int_spill_reg, offset);                       // save the spilled scalar parameter into the local param slot
                cursor.caller_stack_offset += 16;
                cursor.int_stack_only = true;
            }
        }
        PhpType::Float => {
            if !cursor.float_stack_only && cursor.float_reg_idx < float_reg_limit {
                let reg = float_arg_reg_name(emitter.target, cursor.float_reg_idx);
                emitter.comment(&format!("param ${} from {}", name, reg));
                store_at_offset(emitter, reg, offset);                                 // save the float parameter from the incoming floating-point argument register
                cursor.float_reg_idx += 1;
            } else {
                emitter.comment(&format!(
                    "param ${} from caller stack +{}",
                    name,
                    cursor.caller_stack_offset
                ));
                load_from_caller_stack(emitter, float_spill_reg, cursor.caller_stack_offset);
                store_at_offset(emitter, float_spill_reg, offset);                     // save the spilled float parameter into the local param slot
                cursor.caller_stack_offset += 16;
                cursor.float_stack_only = true;
            }
        }
        PhpType::Str => {
            if !cursor.int_stack_only && cursor.int_reg_idx + 1 < int_reg_limit {
                let ptr_reg = int_arg_reg_name(emitter.target, cursor.int_reg_idx);
                let len_reg = int_arg_reg_name(emitter.target, cursor.int_reg_idx + 1);
                emitter.comment(&format!(
                    "param ${} from {},{}",
                    name, ptr_reg, len_reg
                ));
                store_at_offset(emitter, ptr_reg, offset);                             // save the string pointer from the incoming integer-register pair
                store_at_offset(emitter, len_reg, offset - 8);                         // save the string length from the incoming integer-register pair
                cursor.int_reg_idx += 2;
            } else {
                emitter.comment(&format!(
                    "param ${} from caller stack +{}",
                    name,
                    cursor.caller_stack_offset
                ));
                load_from_caller_stack(emitter, int_spill_reg, cursor.caller_stack_offset);
                load_from_caller_stack(emitter, int_hi_spill_reg, cursor.caller_stack_offset + 8);
                store_at_offset(emitter, int_spill_reg, offset);                       // save the spilled string pointer into the local param slot
                store_at_offset(emitter, int_hi_spill_reg, offset - 8);                // save the spilled string length into the local param slot
                cursor.caller_stack_offset += 16;
                cursor.int_stack_only = true;
            }
        }
        PhpType::Void | PhpType::Never => {}
        PhpType::Iterable
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            if !cursor.int_stack_only && cursor.int_reg_idx < int_reg_limit {
                let reg = int_arg_reg_name(emitter.target, cursor.int_reg_idx);
                emitter.comment(&format!("param ${} from {}", name, reg));
                store_at_offset(emitter, reg, offset);                                 // save the pointer-like parameter from the incoming integer argument register
                cursor.int_reg_idx += 1;
            } else {
                emitter.comment(&format!(
                    "param ${} from caller stack +{}",
                    name,
                    cursor.caller_stack_offset
                ));
                load_from_caller_stack(emitter, int_spill_reg, cursor.caller_stack_offset);
                store_at_offset(emitter, int_spill_reg, offset);                       // save the spilled pointer-like parameter into the local param slot
                cursor.caller_stack_offset += 16;
                cursor.int_stack_only = true;
            }
        }
    }
}
