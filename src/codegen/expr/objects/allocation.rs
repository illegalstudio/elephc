use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::names::method_symbol;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::super::{
    emit_expr, restore_concat_offset_after_nested_call, retain_borrowed_heap_arg,
    save_concat_offset_before_nested_call,
};

pub(super) fn emit_new_object(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let class_info = match ctx.classes.get(class_name).cloned() {
        Some(c) => c,
        None => {
            emitter.comment(&format!("WARNING: undefined class {}", class_name));
            return PhpType::Int;
        }
    };
    let num_props = class_info.properties.len();
    let obj_size = 8 + num_props * 16; // 8 for class_id + 16 per property

    emitter.comment(&format!("new {}()", class_name));

    // -- allocate object on heap --
    emitter.instruction(&format!("mov x0, #{}", obj_size));                     // object size in bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate object -> x0 = pointer
    emitter.instruction("mov x9, #4");                                          // heap kind 4 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // store object kind in the uniform heap header
    emitter.instruction(&format!("mov x10, #{}", class_info.class_id));         // load compile-time class id
    emitter.instruction("str x10, [x0]");                                       // store class id at object header
    emitter.instruction("str x0, [sp, #-16]!");                                 // save object pointer on stack

    // -- zero-initialize all property slots --
    for i in 0..num_props {
        let offset = 8 + i * 16;
        emitter.instruction("ldr x9, [sp]");                                    // peek object pointer
        emitter.instruction(&format!("str xzr, [x9, #{}]", offset));            // zero-init property lo
        emitter.instruction(&format!("str xzr, [x9, #{}]", offset + 8));        // zero-init property hi
    }

    // -- set default property values --
    for i in 0..num_props {
        if let Some(default_expr) = &class_info.defaults[i] {
            let default_expr = default_expr.clone();
            let offset = 8 + i * 16;
            let prop_ty = emit_expr(&default_expr, emitter, ctx, data);
            emitter.instruction("ldr x9, [sp]");                                // peek object pointer
            match &prop_ty {
                PhpType::Int
                | PhpType::Bool
                | PhpType::Callable
                | PhpType::Pointer(_)
                | PhpType::Buffer(_)
                | PhpType::Packed(_) => {
                    emitter.instruction(&format!("str x0, [x9, #{}]", offset)); // store default value
                    emitter.instruction(&format!("str xzr, [x9, #{}]", offset + 8)); //clear runtime property metadata slot
                }
                PhpType::Mixed => {
                    emitter.instruction(&format!("str x0, [x9, #{}]", offset)); // store boxed mixed value
                    emitter.instruction("mov x10, #7");                         // runtime property tag 7 = mixed
                    emitter.instruction(&format!("str x10, [x9, #{}]", offset + 8)); //store runtime property metadata tag
                }
                PhpType::Union(_) => {
                    emitter.instruction(&format!("str x0, [x9, #{}]", offset)); // store boxed union value using mixed runtime layout
                    emitter.instruction("mov x10, #7");                         // runtime property tag 7 = mixed/union boxed payload
                    emitter.instruction(&format!("str x10, [x9, #{}]", offset + 8)); //store runtime property metadata tag
                }
                PhpType::Array(_) => {
                    emitter.instruction(&format!("str x0, [x9, #{}]", offset)); // store default value
                    emitter.instruction("mov x10, #4");                         // runtime property tag 4 = indexed array
                    emitter.instruction(&format!("str x10, [x9, #{}]", offset + 8)); //store runtime property metadata tag
                }
                PhpType::AssocArray { .. } => {
                    emitter.instruction(&format!("str x0, [x9, #{}]", offset)); // store default value
                    emitter.instruction("mov x10, #5");                         // runtime property tag 5 = associative array
                    emitter.instruction(&format!("str x10, [x9, #{}]", offset + 8)); //store runtime property metadata tag
                }
                PhpType::Object(_) => {
                    emitter.instruction(&format!("str x0, [x9, #{}]", offset)); // store default value
                    emitter.instruction("mov x10, #6");                         // runtime property tag 6 = object
                    emitter.instruction(&format!("str x10, [x9, #{}]", offset + 8)); //store runtime property metadata tag
                }
                PhpType::Float => {
                    emitter.instruction(&format!("str d0, [x9, #{}]", offset)); // store float default
                    emitter.instruction(&format!("str xzr, [x9, #{}]", offset + 8)); //clear runtime property metadata slot
                }
                PhpType::Str => {
                    emitter.instruction(&format!("str x1, [x9, #{}]", offset)); // store string pointer
                    emitter.instruction(&format!("str x2, [x9, #{}]", offset + 8)); //store string length
                }
                PhpType::Void => {}
            }
        }
    }

    // -- call __construct if it exists --
    if class_info.methods.contains_key("__construct") {
        let normalized_args = class_info
            .methods
            .get("__construct")
            .map(|sig| {
                let regular_param_count = if sig.variadic.is_some() {
                    sig.params.len().saturating_sub(1)
                } else {
                    sig.params.len()
                };
                crate::codegen::expr::calls::args::normalize_named_call_args(sig, args, regular_param_count)
            })
            .unwrap_or_else(|| args.to_vec());
        let mut arg_types = Vec::new();
        for arg in &normalized_args {
            let ty = emit_expr(arg, emitter, ctx, data);
            retain_borrowed_heap_arg(emitter, arg, &ty);
            match &ty {
                PhpType::Bool
                | PhpType::Int
                | PhpType::Mixed
                | PhpType::Union(_)
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Buffer(_)
                | PhpType::Callable
                | PhpType::Object(_)
                | PhpType::Packed(_)
                | PhpType::Pointer(_) => {
                    emitter.instruction("str x0, [sp, #-16]!");                 // push int/object arg onto stack
                }
                PhpType::Float => {
                    emitter.instruction("str d0, [sp, #-16]!");                 // push float arg onto stack
                }
                PhpType::Str => {
                    emitter.instruction("stp x1, x2, [sp, #-16]!");             // push string ptr+len onto stack
                }
                PhpType::Void => {}
            }
            arg_types.push(ty);
        }

        let total_args = arg_types.len();
        let mut int_reg_idx = 1usize;
        let mut float_reg_idx = 0usize;
        let mut assignments: Vec<(PhpType, usize, bool)> = Vec::new();
        for ty in &arg_types {
            if ty.is_float_reg() {
                assignments.push((ty.clone(), float_reg_idx, true));
                float_reg_idx += 1;
            } else {
                assignments.push((ty.clone(), int_reg_idx, false));
                int_reg_idx += ty.register_count();
            }
        }

        for i in (0..total_args).rev() {
            let (ty, start_reg, _is_float) = &assignments[i];
            match ty {
                PhpType::Bool
                | PhpType::Int
                | PhpType::Mixed
                | PhpType::Union(_)
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Buffer(_)
                | PhpType::Callable
                | PhpType::Object(_)
                | PhpType::Packed(_)
                | PhpType::Pointer(_) => {
                    emitter.instruction(&format!("ldr x{}, [sp], #16", start_reg)); //pop arg into register
                }
                PhpType::Float => {
                    emitter.instruction(&format!("ldr d{}, [sp], #16", start_reg)); //pop float arg
                }
                PhpType::Str => {
                    emitter.instruction(&format!(                               // pop string constructor arg into consecutive registers
                        "ldp x{}, x{}, [sp], #16",
                        start_reg,
                        start_reg + 1
                    ));
                }
                PhpType::Void => {}
            }
        }

        emitter.instruction("ldr x0, [sp]");                                    // load $this pointer for constructor
        save_concat_offset_before_nested_call(emitter);
        let constructor_impl = class_info
            .method_impl_classes
            .get("__construct")
            .map(String::as_str)
            .unwrap_or(class_name);
        emitter.instruction(&format!("bl {}", method_symbol(constructor_impl, "__construct"))); // call constructor
        restore_concat_offset_after_nested_call(emitter, &PhpType::Void);
    }

    emitter.instruction("ldr x0, [sp], #16");                                   // pop object pointer into x0
    PhpType::Object(class_name.to_string())
}
