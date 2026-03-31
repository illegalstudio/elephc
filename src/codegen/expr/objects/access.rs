use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::super::emit_expr;

pub(super) fn emit_property_access(
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let obj_ty = emit_expr(object, emitter, ctx, data);
    let (class_name, prop_ty, offset, needs_deref) = match &obj_ty {
        PhpType::Object(class_name) => {
            let class_info = match ctx.classes.get(class_name).cloned() {
                Some(c) => c,
                None => {
                    emitter.comment(&format!("WARNING: undefined class {}", class_name));
                    return PhpType::Int;
                }
            };

            let prop_ty = match class_info
                .properties
                .iter()
                .find(|(n, _)| n == property)
                .map(|(_, t)| t.clone())
            {
                Some(v) => v,
                None => {
                    if class_info.methods.contains_key("__get") {
                        emitter.comment(&format!("magic __get('{}')", property));
                        super::push_magic_property_name_arg(property, emitter, data);
                        emitter.instruction("str x0, [sp, #-16]!");             // push $this pointer for __get dispatch
                        return super::emit_method_call_with_pushed_args(
                            class_name,
                            "__get",
                            &[PhpType::Str],
                            emitter,
                            ctx,
                        );
                    }
                    emitter.comment(&format!("WARNING: undefined property {}", property));
                    return PhpType::Int;
                }
            };
            let offset = match class_info.property_offsets.get(property) {
                Some(offset) => *offset,
                None => {
                    emitter.comment(&format!("WARNING: missing property offset {}", property));
                    return PhpType::Int;
                }
            };

            (class_name.clone(), prop_ty, offset, false)
        }
        PhpType::Pointer(Some(class_name)) if ctx.extern_classes.contains_key(class_name) => {
            let class_info = match ctx.extern_classes.get(class_name).cloned() {
                Some(c) => c,
                None => {
                    emitter.comment(&format!("WARNING: undefined extern class {}", class_name));
                    return PhpType::Int;
                }
            };

            let field = match class_info
                .fields
                .iter()
                .find(|field| field.name == property)
            {
                Some(field) => field.clone(),
                None => {
                    emitter.comment(&format!("WARNING: undefined extern field {}", property));
                    return PhpType::Int;
                }
            };

            (class_name.clone(), field.php_type, field.offset, true)
        }
        PhpType::Pointer(Some(class_name)) if ctx.packed_classes.contains_key(class_name) => {
            let class_info = match ctx.packed_classes.get(class_name).cloned() {
                Some(c) => c,
                None => {
                    emitter.comment(&format!("WARNING: undefined packed class {}", class_name));
                    return PhpType::Int;
                }
            };

            let field = match class_info
                .fields
                .iter()
                .find(|field| field.name == property)
            {
                Some(field) => field.clone(),
                None => {
                    emitter.comment(&format!("WARNING: undefined packed field {}", property));
                    return PhpType::Int;
                }
            };

            (class_name.clone(), field.php_type, field.offset, true)
        }
        _ => {
            emitter.comment("WARNING: property access on non-object");
            return PhpType::Int;
        }
    };

    if needs_deref {
        emitter.instruction("bl __rt_ptr_check_nonnull");                       // abort with fatal error on null pointer dereference
        emitter.comment(&format!(
            "->{} via ptr<{}> (offset {})",
            property, class_name, offset
        ));
    } else {
        emitter.comment(&format!("->{}  (offset {})", property, offset));
    }

    match &prop_ty {
        PhpType::Str => {
            emitter.instruction(&format!("ldr x1, [x0, #{}]", offset));         // load string pointer from property
            emitter.instruction(&format!("ldr x2, [x0, #{}]", offset + 8));     // load string length from property
        }
        PhpType::Float => {
            emitter.instruction(&format!("ldr d0, [x0, #{}]", offset));         // load float from property
        }
        PhpType::Bool | PhpType::Int | PhpType::Void => {
            emitter.instruction(&format!("ldr x0, [x0, #{}]", offset));         // load int/bool from property
        }
        PhpType::Mixed
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            emitter.instruction(&format!("ldr x0, [x0, #{}]", offset));         // load heap pointer from property
        }
    }

    prop_ty
}
