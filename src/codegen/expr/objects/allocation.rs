use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::names::method_symbol;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::super::{
    emit_expr, restore_concat_offset_after_nested_call, retain_borrowed_heap_arg,
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
            let prop_ty = emit_expr(&default_expr, emitter, ctx, data);
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
            if !matches!(ty, PhpType::Void) {
                abi::emit_push_result_value(emitter, &ty);
            }
            arg_types.push(ty);
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
        save_concat_offset_before_nested_call(emitter);
        let constructor_impl = class_info
            .method_impl_classes
            .get("__construct")
            .map(String::as_str)
            .unwrap_or(class_name);
        abi::emit_call_label(emitter, &method_symbol(constructor_impl, "__construct")); // call the resolved constructor implementation for the active target ABI
        restore_concat_offset_after_nested_call(emitter, &PhpType::Void);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);             // drop spilled constructor arguments after the nested call returns
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the allocated object pointer as the expression result for the active target ABI
    PhpType::Object(class_name.to_string())
}
