use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub(super) fn resolve_magic_set_target(object: &Expr, property: &str, ctx: &Context) -> Option<String> {
    let obj_ty = crate::codegen::functions::infer_contextual_type(object, ctx);
    let PhpType::Object(class_name) = obj_ty else {
        return None;
    };
    let class_info = ctx.classes.get(&class_name)?;
    if class_info.properties.iter().any(|(name, _)| name == property) {
        return None;
    }
    class_info.methods.contains_key("__set").then_some(class_name)
}

pub(super) fn emit_magic_set_call(
    class_name: &str,
    property: &str,
    val_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment(&format!("magic __set('{}')", property));
    let object_reg = abi::symbol_scratch_reg(emitter);
    let boxed_reg = match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    };
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                        // push $this pointer while boxing the value argument

    if *val_ty == PhpType::Void {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #8");                              // runtime tag 8 = null payload for Mixed boxing
                emitter.instruction("mov x1, xzr");                             // null mixed payloads have no low word
                emitter.instruction("mov x2, xzr");                             // null mixed payloads have no high word
                emitter.instruction("bl __rt_mixed_from_value");                // box null into an owned Mixed cell for __set
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, 9223372036854775806");            // use the runtime null sentinel as the boxed null payload low word
                emitter.instruction("xor rsi, rsi");                            // null mixed payloads have no high word
                emitter.instruction("mov rax, 8");                              // runtime tag 8 = null payload for Mixed boxing
                emitter.instruction("call __rt_mixed_from_value");              // box null into an owned Mixed cell for __set
            }
        }
        abi::emit_pop_reg(emitter, object_reg);                                      // reload $this after the boxing helper may clobber caller-saved registers
    } else {
        match val_ty {
            PhpType::Float => {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("ldr d0, [sp, #16]");               // reload the saved float value for Mixed boxing
                    }
                    Arch::X86_64 => {
                        emitter.instruction("movsd xmm0, QWORD PTR [rsp + 16]"); // reload the saved float value for Mixed boxing
                    }
                }
                crate::codegen::emit_box_current_value_as_mixed(emitter, val_ty);
            }
            PhpType::Str => {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("ldp x1, x2, [sp, #16]");           // reload the saved string payload for Mixed boxing
                    }
                    Arch::X86_64 => {
                        emitter.instruction("mov rax, QWORD PTR [rsp + 16]");   // reload the saved string pointer for Mixed boxing
                        emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");   // reload the saved string length for Mixed boxing
                    }
                }
                crate::codegen::emit_box_current_value_as_mixed(emitter, val_ty);
            }
            _ => {
                abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16); // reload the saved scalar/heap value for Mixed boxing
                if !matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
                    crate::codegen::emit_box_current_value_as_mixed(emitter, val_ty);
                }
            }
        }
        abi::emit_pop_reg(emitter, object_reg);                                      // reload $this after the boxing helper may clobber caller-saved registers
        abi::emit_release_temporary_stack(emitter, 16);                              // drop the saved original value after boxing it into Mixed storage
    }

    emitter.instruction(&format!("mov {}, {}", boxed_reg, abi::int_result_reg(emitter))); // keep the boxed Mixed value across property-name setup
    crate::codegen::expr::push_magic_property_name_arg(property, emitter, data);
    abi::emit_push_reg(emitter, boxed_reg);                                          // push the boxed Mixed $value argument
    abi::emit_push_reg(emitter, object_reg);                                         // push $this pointer for __set dispatch
    crate::codegen::expr::emit_method_call_with_pushed_args(
        class_name,
        "__set",
        &[PhpType::Str, PhpType::Mixed],
        emitter,
        ctx,
    );
}
