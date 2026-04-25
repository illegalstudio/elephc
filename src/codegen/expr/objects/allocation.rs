use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::calls::args as call_args;
use crate::codegen::platform::Arch;
use crate::names::method_symbol;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::super::{
    coerce_result_to_type, emit_expr, restore_concat_offset_after_nested_call,
    save_concat_offset_before_nested_call,
};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

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
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", obj_size));             // object size in bytes
            emitter.instruction("bl __rt_heap_alloc");                          // allocate object -> x0 = pointer
            emitter.instruction("mov x9, #4");                                  // heap kind 4 = object instance
            emitter.instruction("str x9, [x0, #-8]");                           // store object kind in the uniform heap header
            emitter.instruction(&format!("mov x10, #{}", class_info.class_id)); // load compile-time class id
            emitter.instruction("str x10, [x0]");                               // store class id at object header
            abi::emit_push_reg(emitter, "x0");                                  // save the allocated object pointer while property slots are initialized
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", obj_size));             // object size in bytes
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate object -> rax = pointer
            emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the x86_64 object heap kind word with the uniform heap marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the allocation as an object instance in the x86_64 uniform heap header
            emitter.instruction(&format!("mov r10, {}", class_info.class_id));  // load the compile-time class id for the allocated object instance
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store the class id in the first field of the object payload
            abi::emit_push_reg(emitter, "rax");                                 // save the allocated object pointer while property slots are initialized
        }
    }

    // -- zero-initialize all property slots --
    for i in 0..num_props {
        let offset = 8 + i * 16;
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x9, [sp]");                            // peek object pointer
                emitter.instruction(&format!("str xzr, [x9, #{}]", offset));    // zero-init property lo
                emitter.instruction(&format!("str xzr, [x9, #{}]", offset + 8)); // zero-init property hi
            }
            Arch::X86_64 => {
                emitter.instruction("mov r11, QWORD PTR [rsp]");                // peek the allocated object pointer from the temporary stack slot
                emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", offset)); // zero-initialize the low word of the property storage slot
                emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", offset + 8)); // zero-initialize the high word / runtime metadata slot
            }
        }
    }

    // -- set default property values --
    for i in 0..num_props {
        if let Some(default_expr) = &class_info.defaults[i] {
            let default_expr = default_expr.clone();
            let offset = 8 + i * 16;
            let actual_ty = emit_expr(&default_expr, emitter, ctx, data);
            let prop_name = &class_info.properties[i].0;
            let expected_ty = class_info.properties[i].1.clone();
            let prop_ty = if class_info.declared_properties.contains(prop_name) {
                coerce_result_to_type(emitter, ctx, data, &actual_ty, &expected_ty);
                expected_ty
            } else {
                actual_ty
            };
            let object_reg = abi::symbol_scratch_reg(emitter);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("ldr {}, [sp]", object_reg));  // peek object pointer from the temporary stack slot on AArch64
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", object_reg)); // peek object pointer from the temporary stack slot on x86_64
                }
            }
            match &prop_ty {
                PhpType::Int
                | PhpType::Bool
                | PhpType::Callable
                | PhpType::Pointer(_)
                | PhpType::Buffer(_)
                | PhpType::Packed(_) => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
                }
                PhpType::Mixed => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    let tag_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, tag_reg, 7);
                    abi::emit_store_to_address(emitter, tag_reg, object_reg, offset + 8);
                }
                PhpType::Union(_) => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    let tag_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, tag_reg, 7);
                    abi::emit_store_to_address(emitter, tag_reg, object_reg, offset + 8);
                }
                PhpType::Array(_) => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    let tag_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, tag_reg, 4);
                    abi::emit_store_to_address(emitter, tag_reg, object_reg, offset + 8);
                }
                PhpType::AssocArray { .. } => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    let tag_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, tag_reg, 5);
                    abi::emit_store_to_address(emitter, tag_reg, object_reg, offset + 8);
                }
                PhpType::Object(_) => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    let tag_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, tag_reg, 6);
                    abi::emit_store_to_address(emitter, tag_reg, object_reg, offset + 8);
                }
                PhpType::Float => {
                    abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), object_reg, offset);
                    abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
                }
                PhpType::Str => {
                    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                    abi::emit_store_to_address(emitter, ptr_reg, object_reg, offset);
                    abi::emit_store_to_address(emitter, len_reg, object_reg, offset + 8);
                }
                PhpType::Void => {}
            }
        }
    }

    // -- call __construct if it exists --
    if class_info.methods.contains_key("__construct") {
        let sig = class_info.methods.get("__construct").cloned();
        let regular_param_count = call_args::regular_param_count(sig.as_ref(), args.len());
        let prepared = call_args::prepare_call_args(sig.as_ref(), args, regular_param_count);
        let mut arg_types = call_args::emit_pushed_non_variadic_args(
            &prepared.all_args,
            sig.as_ref(),
            "constructor ref arg",
            false,
            emitter,
            ctx,
            data,
        );

        if prepared.spread_into_named {
            if let Some(spread_expr) = prepared.spread_arg.as_ref() {
                call_args::emit_spread_into_named_params(
                    spread_expr,
                    sig.as_ref(),
                    prepared.spread_at_index,
                    prepared.regular_param_count,
                    "constructor params",
                    emitter,
                    ctx,
                    data,
                    &mut arg_types,
                );
            }
        }

        if prepared.is_variadic {
            if let Some(spread_expr) = prepared.spread_arg.as_ref() {
                let ty = call_args::emit_spread_variadic_array_arg(
                    spread_expr,
                    "spread array as constructor variadic param",
                    emitter,
                    ctx,
                    data,
                );
                arg_types.push(ty);
            } else if prepared.variadic_args.is_empty() {
                arg_types.push(call_args::emit_empty_variadic_array_arg(
                    "empty constructor variadic array",
                    emitter,
                ));
            } else {
                arg_types.push(call_args::emit_variadic_array_arg_from_exprs(
                    &prepared.variadic_args,
                    "build constructor variadic array",
                    true,
                    true,
                    emitter,
                    ctx,
                    data,
                ));
            }
        }

        let assignments = crate::codegen::abi::build_outgoing_arg_assignments_for_target(
            emitter.target,
            &arg_types,
            1,
        );
        let overflow_bytes =
            crate::codegen::abi::materialize_outgoing_args(emitter, &assignments);

        if overflow_bytes == 0 {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldr x0, [sp]");                        // load $this directly from the top of the stack when all args stayed in registers on AArch64
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdi, QWORD PTR [rsp]");            // load $this directly into the first SysV integer argument register when all args stayed in registers on x86_64
                }
            }
        } else {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("ldr x0, [sp, #{}]", overflow_bytes)); // skip spilled stack arguments to reload the saved object pointer as $this on AArch64
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", overflow_bytes)); // skip spilled stack arguments to reload the saved object pointer as $this in the first SysV integer argument register on x86_64
                }
            }
        }
        save_concat_offset_before_nested_call(emitter, ctx);
        let constructor_impl = class_info
            .method_impl_classes
            .get("__construct")
            .map(String::as_str)
            .unwrap_or(class_name);
        abi::emit_call_label(emitter, &method_symbol(constructor_impl, "__construct")); // call the resolved constructor implementation for the active target ABI
        restore_concat_offset_after_nested_call(emitter, ctx, &PhpType::Void);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);             // drop spilled constructor arguments after the nested call returns
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the allocated object pointer as the expression result for the active target ABI
    PhpType::Object(class_name.to_string())
}
