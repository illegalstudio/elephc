use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
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
    emitter.instruction("str x0, [sp, #-16]!");                                      // push $this pointer while boxing the value argument

    if *val_ty == PhpType::Void {
        emitter.instruction("mov x0, #8");                                           // runtime tag 8 = null payload for Mixed boxing
        emitter.instruction("mov x1, xzr");                                          // null mixed payloads have no low word
        emitter.instruction("mov x2, xzr");                                          // null mixed payloads have no high word
        emitter.instruction("bl __rt_mixed_from_value");                             // box null into an owned Mixed cell for __set
        emitter.instruction("ldr x10, [sp]");                                        // reload $this after the boxing helper may clobber caller-saved registers
        emitter.instruction("add sp, sp, #16");                                      // drop the temporary $this stack slot
    } else {
        match val_ty {
            PhpType::Float => {
                emitter.instruction("ldr d0, [sp, #16]");                            // reload the saved float value for Mixed boxing
                crate::codegen::emit_box_current_value_as_mixed(emitter, val_ty);
            }
            PhpType::Str => {
                emitter.instruction("ldp x1, x2, [sp, #16]");                        // reload the saved string payload for Mixed boxing
                crate::codegen::emit_box_current_value_as_mixed(emitter, val_ty);
            }
            _ => {
                emitter.instruction("ldr x0, [sp, #16]");                            // reload the saved scalar/heap value for Mixed boxing
                if !matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
                    crate::codegen::emit_box_current_value_as_mixed(emitter, val_ty);
                }
            }
        }
        emitter.instruction("ldr x10, [sp]");                                        // reload $this after the boxing helper may clobber caller-saved registers
        emitter.instruction("add sp, sp, #32");                                      // drop the temporary $this slot and saved original value
    }

    emitter.instruction("mov x11, x0");                                              // keep the boxed Mixed value across property-name setup
    crate::codegen::expr::push_magic_property_name_arg(property, emitter, data);
    emitter.instruction("str x11, [sp, #-16]!");                                     // push the boxed Mixed $value argument
    emitter.instruction("str x10, [sp, #-16]!");                                     // push $this pointer for __set dispatch
    crate::codegen::expr::emit_method_call_with_pushed_args(
        class_name,
        "__set",
        &[PhpType::Str, PhpType::Mixed],
        emitter,
        ctx,
    );
}
