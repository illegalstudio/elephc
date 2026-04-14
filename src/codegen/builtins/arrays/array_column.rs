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
    emitter.comment("array_column()");
    // -- evaluate array of assoc arrays --
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let val_ty = match &arr_ty {
        PhpType::Array(inner) => match inner.as_ref() {
            PhpType::AssocArray { value, .. } => *value.clone(),
            _ => PhpType::Str,
        },
        _ => PhpType::Str,
    };
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the outer indexed-array pointer while evaluating the requested column key

    // -- evaluate column key (string) --
    emit_expr(&args[1], emitter, ctx, data);
    let (key_ptr_reg, key_len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, key_ptr_reg, key_len_reg);                 // preserve the requested column key string while restoring the outer indexed-array pointer

    // -- call runtime --
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg_pair(emitter, "x1", "x2");                        // restore the requested column key into the runtime string-argument registers
            abi::emit_pop_reg(emitter, "x0");                                   // restore the outer indexed-array pointer into the runtime array-argument register
        }
        Arch::X86_64 => {
            abi::emit_pop_reg_pair(emitter, "rsi", "rdx");                      // restore the requested column key into the SysV string-argument registers
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the outer indexed-array pointer into the SysV first integer argument register
        }
    }
    if val_ty == PhpType::Str {
        abi::emit_call_label(emitter, "__rt_array_column_str");                 // extract string column values into a new indexed array whose slots own persisted strings
    } else if val_ty.is_refcounted() {
        abi::emit_call_label(emitter, "__rt_array_column_ref");                 // extract retained heap/object/array column values into a new indexed array
    } else {
        abi::emit_call_label(emitter, "__rt_array_column");                     // extract scalar column values into a new indexed array
    }

    Some(PhpType::Array(Box::new(val_ty)))
}
