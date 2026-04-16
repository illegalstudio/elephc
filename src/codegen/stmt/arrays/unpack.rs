use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub(super) fn emit_list_unpack_stmt(
    vars: &[String],
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment("list unpack");

    let arr_ty = emit_expr(value, emitter, ctx, data);
    let elem_ty = match &arr_ty {
        PhpType::Array(t) => *t.clone(),
        _ => PhpType::Int,
    };

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the source indexed-array pointer while each unpack target local is assigned from its element slot

    for (i, var_name) in vars.iter().enumerate() {
        let var = match ctx.variables.get(var_name) {
            Some(v) => v,
            None => {
                emitter.comment(&format!("WARNING: undefined variable ${}", var_name));
                continue;
            }
        };
        let offset = var.stack_offset;

        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x9, [sp]");                            // peek the preserved indexed-array pointer from the temporary stack slot before loading the requested unpack element
                match &elem_ty {
                    PhpType::Int | PhpType::Bool => {
                        emitter.instruction("add x9, x9, #24");                 // skip the fixed indexed-array header before addressing the scalar payload region
                        emitter.instruction(&format!("ldr x0, [x9, #{}]", i * 8)); // load the requested scalar unpack element from the indexed-array payload region
                        abi::store_at_offset(emitter, "x0", offset);
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!("add x9, x9, #{}", 24 + i * 16)); // advance from the indexed-array base to the selected 16-byte string slot
                        emitter.instruction("ldr x1, [x9]");                    // load the requested unpack string pointer from the selected indexed-array slot
                        emitter.instruction("ldr x2, [x9, #8]");                // load the requested unpack string length from the selected indexed-array slot
                        abi::store_at_offset(emitter, "x1", offset);
                        abi::store_at_offset(emitter, "x2", offset - 8);
                    }
                    PhpType::Float => {
                        emitter.instruction("add x9, x9, #24");                 // skip the fixed indexed-array header before addressing the floating payload region
                        emitter.instruction(&format!("ldr d0, [x9, #{}]", i * 8)); // load the requested floating unpack element from the indexed-array payload region
                        abi::store_at_offset(emitter, "d0", offset);
                    }
                    _ => {
                        emitter.instruction("add x9, x9, #24");                 // skip the fixed indexed-array header before addressing the pointer-like payload region
                        emitter.instruction(&format!("ldr x0, [x9, #{}]", i * 8)); // load the requested pointer-like unpack element from the indexed-array payload region
                        abi::store_at_offset(emitter, "x0", offset);
                    }
                }
            }
            Arch::X86_64 => {
                emitter.instruction("mov r11, QWORD PTR [rsp]");                // peek the preserved indexed-array pointer from the temporary stack slot before loading the requested unpack element
                match &elem_ty {
                    PhpType::Int | PhpType::Bool => {
                        emitter.instruction("add r11, 24");                     // skip the fixed indexed-array header before addressing the scalar payload region
                        emitter.instruction(&format!("mov rax, QWORD PTR [r11 + {}]", i * 8)); // load the requested scalar unpack element from the indexed-array payload region
                        abi::store_at_offset(emitter, "rax", offset);
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!("add r11, {}", 24 + i * 16)); // advance from the indexed-array base to the selected 16-byte string slot
                        emitter.instruction("mov rax, QWORD PTR [r11]");        // load the requested unpack string pointer from the selected indexed-array slot
                        emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");    // load the requested unpack string length from the selected indexed-array slot
                        abi::store_at_offset(emitter, "rax", offset);
                        abi::store_at_offset(emitter, "rdx", offset - 8);
                    }
                    PhpType::Float => {
                        emitter.instruction("add r11, 24");                     // skip the fixed indexed-array header before addressing the floating payload region
                        emitter.instruction(&format!("movsd xmm0, QWORD PTR [r11 + {}]", i * 8)); // load the requested floating unpack element from the indexed-array payload region
                        abi::store_at_offset(emitter, "xmm0", offset);
                    }
                    _ => {
                        emitter.instruction("add r11, 24");                     // skip the fixed indexed-array header before addressing the pointer-like payload region
                        emitter.instruction(&format!("mov rax, QWORD PTR [r11 + {}]", i * 8)); // load the requested pointer-like unpack element from the indexed-array payload region
                        abi::store_at_offset(emitter, "rax", offset);
                    }
                }
            }
        }
        ctx.update_var_type_and_ownership(
            var_name,
            elem_ty.clone(),
            super::super::HeapOwnership::borrowed_alias_for_type(&elem_ty),
        );
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // discard the preserved indexed-array pointer after every list-unpack target local has been assigned
}
