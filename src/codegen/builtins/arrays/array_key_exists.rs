use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

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
        emit_expr(&args[0], emitter, ctx, data);
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
