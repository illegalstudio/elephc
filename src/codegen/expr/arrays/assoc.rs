use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::{abi, platform::Arch};
use super::super::{emit_expr, retain_borrowed_heap_arg, Expr, ExprKind, PhpType};

pub(crate) fn emit_assoc_array_literal(
    pairs: &[(Expr, Expr)],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("assoc array literal");
    let result_reg = abi::int_result_reg(emitter);
    let stack_reg = match emitter.target.arch {
        Arch::AArch64 => "sp",
        Arch::X86_64 => "rsp",
    };
    let hash_capacity_reg = abi::int_arg_reg_name(emitter.target, 0);
    let key_ptr_reg = abi::int_arg_reg_name(emitter.target, 1);
    let key_len_reg = abi::int_arg_reg_name(emitter.target, 2);
    let value_lo_reg = abi::int_arg_reg_name(emitter.target, 3);
    let value_hi_reg = abi::int_arg_reg_name(emitter.target, 4);
    let value_tag_reg = abi::int_arg_reg_name(emitter.target, 5);
    let tag_reg = if emitter.target.arch == Arch::AArch64 {
        abi::int_arg_reg_name(emitter.target, 1)
    } else {
        abi::temp_int_reg(emitter.target)
    };
    let float_bits_reg = abi::temp_int_reg(emitter.target);
    let zero_reg = match emitter.target.arch {
        Arch::AArch64 => "xzr",
        Arch::X86_64 => "0",
    };
    let (string_ptr_reg, string_len_reg) = abi::string_result_regs(emitter);

    let first_value_ty = super::super::super::functions::infer_contextual_type(&pairs[0].1, ctx);
    let value_type_tag = super::super::super::runtime_value_tag(&first_value_ty);

    abi::emit_load_int_immediate(
        emitter,
        hash_capacity_reg,
        std::cmp::max(pairs.len() * 2, 16) as i64,
    );
    abi::emit_load_int_immediate(emitter, tag_reg, value_type_tag as i64);
    abi::emit_call_label(emitter, "__rt_hash_new");
    abi::emit_push_reg(emitter, result_reg);                                    // save the hash table pointer while key/value pairs are inserted

    let mut val_ty = PhpType::Int;
    for (i, pair) in pairs.iter().enumerate() {
        emit_expr(&pair.0, emitter, ctx, data);
        abi::emit_push_reg_pair(emitter, string_ptr_reg, string_len_reg);        // save the assoc-array key payload while the value expression is emitted
        let ty = emit_expr(&pair.1, emitter, ctx, data);
        retain_borrowed_heap_arg(emitter, &pair.1, &ty);
        if i == 0 {
            val_ty = ty.clone();
        } else if ty != val_ty {
            val_ty = PhpType::Mixed;
        }
        let (val_lo, val_hi) = match &ty {
            PhpType::Int | PhpType::Bool => (result_reg, zero_reg),
            PhpType::Str => {
                abi::emit_call_label(emitter, "__rt_str_persist");              // copy the borrowed string result into owned heap storage
                (string_ptr_reg, string_len_reg)
            }
            PhpType::Float => {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction(&format!("fmov {}, {}", float_bits_reg, abi::float_result_reg(emitter))); // move the float bits into an integer scratch register for hash insertion
                    }
                    Arch::X86_64 => {
                        emitter.instruction(&format!("movq {}, {}", float_bits_reg, abi::float_result_reg(emitter))); // move the float bits into an integer scratch register for hash insertion
                    }
                }
                (float_bits_reg, zero_reg)
            }
            _ => (result_reg, zero_reg),
        };
        emitter.instruction(&format!("mov {}, {}", value_lo_reg, val_lo));      // move the low payload word into the hash-set value register
        emitter.instruction(&format!("mov {}, {}", value_hi_reg, val_hi));      // move the high payload word into the hash-set value register
        abi::emit_load_int_immediate(
            emitter,
            value_tag_reg,
            super::super::super::runtime_value_tag(&ty) as i64,
        );
        abi::emit_pop_reg_pair(emitter, key_ptr_reg, key_len_reg);              // restore the assoc-array key payload into the hash-set argument registers
        abi::emit_load_from_address(emitter, hash_capacity_reg, stack_reg, 0);  // reload the current hash table pointer before insertion
        abi::emit_call_label(emitter, "__rt_hash_set");
        abi::emit_store_to_address(emitter, result_reg, stack_reg, 0);          // persist the updated hash table pointer after possible growth
    }

    abi::emit_pop_reg(emitter, result_reg);                                     // restore the completed hash table pointer as the expression result

    let key_ty = match &pairs[0].0.kind {
        ExprKind::IntLiteral(_) => PhpType::Int,
        _ => PhpType::Str,
    };

    PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(val_ty),
    }
}
