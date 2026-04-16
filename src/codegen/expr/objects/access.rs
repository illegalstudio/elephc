use crate::codegen::abi;
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
                        let object_reg = abi::symbol_scratch_reg(emitter);
                        emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); // preserve $this while the magic-property name setup clobbers normal result registers
                        super::push_magic_property_name_arg(property, emitter, data);
                        abi::emit_push_reg(emitter, object_reg);                  // push $this pointer for __get dispatch using the preserved object register
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
        abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");               // abort with fatal error on null pointer dereference
        emitter.comment(&format!(
            "->{} via ptr<{}> (offset {})",
            property, class_name, offset
        ));
    } else {
        emitter.comment(&format!("->{}  (offset {})", property, offset));
    }

    let object_reg = abi::int_result_reg(emitter);

    match &prop_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            let base_reg = abi::symbol_scratch_reg(emitter);
            emitter.instruction(&format!("mov {}, {}", base_reg, object_reg));  // preserve the object base pointer while loading the two-word string property payload
            abi::emit_load_from_address(emitter, ptr_reg, base_reg, offset);
            abi::emit_load_from_address(emitter, len_reg, base_reg, offset + 8);
        }
        PhpType::Float => {
            abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), object_reg, offset);
        }
        PhpType::Bool | PhpType::Int | PhpType::Void => {
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
        }
        PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
        }
    }

    prop_ty
}
