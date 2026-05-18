//! Purpose:
//! Shares compile-time reflection metadata helpers across class-method and
//! expression codegen.
//!
//! Called from:
//! - `crate::codegen::class_methods`
//! - `crate::codegen::builtins::system::class_get_attributes`
//! - `crate::codegen::expr::objects::reflection`
//!
//! Key details:
//! - Attribute factory ids are deterministic over the full class metadata
//!   table so `ReflectionAttribute::newInstance()` and metadata materializers
//!   agree without runtime registration state.

use std::collections::{BTreeMap, HashMap};

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::arrays::emit_array_value_type_stamp;
use crate::codegen::expr::objects::emit_new_object;
use crate::codegen::platform::Arch;
use crate::names::php_symbol_key;
use crate::types::{AttrArgValue, ClassInfo, PhpType};

#[derive(Clone)]
pub(crate) struct ReflectionAttributeFactory {
    pub(crate) id: i64,
    pub(crate) class_name: String,
    pub(crate) args: Vec<AttrArgValue>,
}

pub(crate) fn resolve_class_name<'a>(
    classes: &'a HashMap<String, ClassInfo>,
    class_name: &str,
) -> Option<&'a str> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(String::as_str)
}

pub(crate) fn collect_attribute_factories(
    classes: &HashMap<String, ClassInfo>,
) -> Vec<ReflectionAttributeFactory> {
    let mut unique = BTreeMap::new();
    for class_info in classes.values() {
        collect_from_attribute_lists(
            classes,
            &class_info.attribute_names,
            &class_info.attribute_args,
            &mut unique,
        );
        for (member, names) in &class_info.method_attribute_names {
            if let Some(args) = class_info.method_attribute_args.get(member) {
                collect_from_attribute_lists(classes, names, args, &mut unique);
            }
        }
        for (member, names) in &class_info.property_attribute_names {
            if let Some(args) = class_info.property_attribute_args.get(member) {
                collect_from_attribute_lists(classes, names, args, &mut unique);
            }
        }
    }

    unique
        .into_keys()
        .enumerate()
        .map(|(idx, (class_name, args))| ReflectionAttributeFactory {
            id: (idx as i64) + 1,
            class_name,
            args,
        })
        .collect()
}

pub(crate) fn attribute_factory_id(
    classes: &HashMap<String, ClassInfo>,
    attr_name: &str,
    attr_args: &[AttrArgValue],
) -> i64 {
    let Some(resolved_name) = resolve_class_name(classes, attr_name) else {
        return 0;
    };
    collect_attribute_factories(classes)
        .into_iter()
        .find(|factory| factory.class_name == resolved_name && factory.args == attr_args)
        .map(|factory| factory.id)
        .unwrap_or(0)
}

pub(crate) fn emit_reflection_attribute_array(
    attr_names: &[String],
    attr_args: &[Option<Vec<AttrArgValue>>],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let result_reg = abi::int_result_reg(emitter);
    let scratch = abi::symbol_scratch_reg(emitter);

    // -- allocate the result indexed array (one heap-pointer slot per attr) --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", attr_names.len().max(1))); // initial capacity (>=1 to avoid grow on first push)
            emitter.instruction("mov x1, #8");                                  // element stride: one heap pointer per slot (object handle)
            emitter.instruction("bl __rt_array_new");                           // x0 = freshly allocated array pointer
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", attr_names.len().max(1))); // initial capacity (>=1)
            emitter.instruction("mov rdx, 8");                                  // element stride: one heap pointer per slot
            emitter.instruction("call __rt_array_new");                         // rax = array pointer
        }
    }
    emit_array_value_type_stamp(
        emitter,
        result_reg,
        &PhpType::Object("ReflectionAttribute".to_string()),
    );

    for (idx, attr_name) in attr_names.iter().enumerate() {
        let empty_args = Vec::new();
        let attr_arg_list = attr_args
            .get(idx)
            .and_then(Option::as_ref)
            .unwrap_or(&empty_args);
        let factory_id = attribute_factory_id(&ctx.classes, attr_name, attr_arg_list);

        // -- save the result array pointer below later temporaries --
        abi::emit_push_reg(emitter, result_reg);

        // -- allocate a fresh ReflectionAttribute via the normal new path --
        // emit_new_object walks the registered class and runs its private
        // synthetic zero-arg constructor; this internal emitter is the only
        // code path that can populate ReflectionAttribute metadata slots.
        emit_new_object("ReflectionAttribute", &[], emitter, ctx, data);

        // The new object pointer is now in the result reg. Save it below
        // both the array pointer and the spilled per-property scratch
        // values that follow.
        abi::emit_push_reg(emitter, result_reg);

        // -- overwrite `$__name` (offset 8 = lo, 16 = hi) --
        emit_set_name_property(emitter, data, attr_name, scratch);

        // -- build the mixed args array and overwrite `$__args` --
        emit_set_args_property(emitter, data, attr_arg_list, scratch);

        // -- store the newInstance factory id in `$__factory` --
        emit_set_factory_property(emitter, factory_id, scratch);

        // -- push the populated object pointer into the result array --
        // After emit_set_args_property, the spilled object pointer is still
        // on the stack one slot below the result array. Pop both back, push.
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x1, [sp], #16");                       // pop the populated ReflectionAttribute pointer into the value-arg register
                emitter.instruction("ldr x0, [sp], #16");                       // pop the result array pointer into the array-arg register
                emitter.instruction("bl __rt_array_push_int");                  // append the object handle to the result array
            }
            Arch::X86_64 => {
                emitter.instruction("pop rsi");                                 // pop the populated ReflectionAttribute pointer into the value-arg register
                emitter.instruction("pop rax");                                 // pop the result array pointer into the array-arg register
                emitter.instruction("call __rt_array_push_int");                // append the object handle to the result array
            }
        }
    }

    PhpType::Array(Box::new(PhpType::Object("ReflectionAttribute".to_string())))
}

/// Overwrite the freshly-allocated ReflectionAttribute's `$__name` slot
/// with a heap-persisted copy of `attr_name`. The object pointer is
/// expected at the top of the temporary stack; the helper leaves it there.
fn emit_set_name_property(
    emitter: &mut Emitter,
    data: &mut DataSection,
    attr_name: &str,
    obj_ptr_scratch: &str,
) {
    let (sym, len) = data.add_string(attr_name.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // peek the obj pointer from the temporary stack
            emitter.instruction("ldr x0, [x9, #8]");                            // load the old __name.lo (heap-resident default copy)
            emitter.instruction("bl __rt_heap_free_safe");                      // release the previous owned name string
            abi::emit_symbol_address(emitter, "x1", &sym);                      // x1 = source string address
            emitter.instruction(&format!("mov x2, #{}", len));                  // x2 = source string length
            emitter.instruction("bl __rt_str_persist");                         // x1 = heap-resident pointer, x2 = length
            emitter.instruction(&format!("ldr {}, [sp]", obj_ptr_scratch));     // peek the obj pointer back
            emitter.instruction(&format!("str x1, [{}, #8]", obj_ptr_scratch)); // commit __name.lo (heap pointer)
            emitter.instruction(&format!("str x2, [{}, #16]", obj_ptr_scratch)); // commit __name.hi (length)
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // peek the obj pointer
            emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                // load old __name.lo for the free helper
            emitter.instruction("call __rt_heap_free_safe");                    // release the previous owned name string
            abi::emit_symbol_address(emitter, "rax", &sym);                     // rax = source string address
            emitter.instruction(&format!("mov rdx, {}", len));                  // rdx = source string length
            emitter.instruction("call __rt_str_persist");                       // rax = heap-resident pointer, rdx = length
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", obj_ptr_scratch)); // peek the obj pointer back
            emitter.instruction(&format!("mov QWORD PTR [{} + 8], rax", obj_ptr_scratch)); // commit __name.lo
            emitter.instruction(&format!("mov QWORD PTR [{} + 16], rdx", obj_ptr_scratch)); // commit __name.hi (length)
        }
    }
}

/// Overwrite the freshly-allocated ReflectionAttribute's `$__args` slot
/// with a fresh `array<mixed>` built from `attr_arg_list`. The object
/// pointer is expected at the top of the temporary stack and is left there.
fn emit_set_args_property(
    emitter: &mut Emitter,
    data: &mut DataSection,
    attr_arg_list: &[AttrArgValue],
    obj_ptr_scratch: &str,
) {
    let result_reg = abi::int_result_reg(emitter);

    // -- decref the previous default `[]` value before overwriting --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // peek the obj pointer
            emitter.instruction("ldr x0, [x9, #24]");                           // load old __args.lo (heap array pointer)
            emitter.instruction("bl __rt_decref_array");                        // release the previous default empty array
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // peek the obj pointer
            emitter.instruction("mov rax, QWORD PTR [r10 + 24]");               // load old __args.lo
            emitter.instruction("call __rt_decref_array");                      // release the previous default empty array
        }
    }

    // -- allocate a fresh mixed-cell pointer array for the literal args --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", attr_arg_list.len().max(1))); // initial capacity (>=1)
            emitter.instruction("mov x1, #8");                                  // element stride: one boxed mixed-cell pointer per slot
            emitter.instruction("bl __rt_array_new");                           // x0 = freshly allocated args array
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", attr_arg_list.len().max(1))); // initial capacity (>=1)
            emitter.instruction("mov rdx, 8");                                  // element stride: one boxed mixed-cell pointer per slot
            emitter.instruction("call __rt_array_new");                         // rax = freshly allocated args array
        }
    }
    emit_array_value_type_stamp(emitter, result_reg, &PhpType::Mixed);

    // -- box and push each literal arg --
    for arg in attr_arg_list {
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_push_reg(emitter, result_reg);                        // save the args array pointer across the boxing helper call
                emit_box_arg_aarch64(arg, emitter, data);                       // x0 = boxed mixed-cell pointer for this arg
                emitter.instruction("mov x1, x0");                              // x1 = mixed-cell pointer (push helper's value arg)
                emitter.instruction("ldr x0, [sp]");                            // x0 = args array pointer
                emitter.instruction("bl __rt_array_push_int");                  // x0 = (possibly realloc'd) args array pointer
                abi::emit_release_temporary_stack(emitter, 16);                 // drop the saved slot now that the helper returned the up-to-date array pointer
            }
            Arch::X86_64 => {
                abi::emit_push_reg(emitter, result_reg);                        // save the args array pointer
                emit_box_arg_x86_64(arg, emitter, data);                        // rax = boxed mixed-cell pointer
                emitter.instruction("mov rsi, rax");                            // rsi = mixed-cell pointer
                emitter.instruction("mov rax, QWORD PTR [rsp]");                // rax = args array pointer
                emitter.instruction("call __rt_array_push_int");                // rax = updated args array pointer
                abi::emit_release_temporary_stack(emitter, 16);                 // drop the saved args-array slot
            }
        }
    }

    // -- store the args array pointer + array kind tag in __args --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [sp]", obj_ptr_scratch));     // peek the obj pointer
            emitter.instruction(&format!("str {}, [{}, #24]", result_reg, obj_ptr_scratch)); // commit __args.lo (array pointer)
            emitter.instruction("mov x10, #4");                                 // runtime kind tag 4 = indexed array
            emitter.instruction(&format!("str x10, [{}, #32]", obj_ptr_scratch)); // commit __args.hi (kind tag)
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", obj_ptr_scratch)); // peek the obj pointer
            emitter.instruction(&format!("mov QWORD PTR [{} + 24], {}", obj_ptr_scratch, result_reg)); // commit __args.lo (array pointer)
            emitter.instruction(&format!("mov QWORD PTR [{} + 32], 4", obj_ptr_scratch)); // commit __args.hi (kind tag = 4 = indexed array)
        }
    }
}

fn emit_set_factory_property(
    emitter: &mut Emitter,
    factory_id: i64,
    obj_ptr_scratch: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [sp]", obj_ptr_scratch));     // peek the obj pointer
            abi::emit_load_int_immediate(emitter, "x10", factory_id);
            emitter.instruction(&format!("str x10, [{}, #40]", obj_ptr_scratch)); // commit __factory id for newInstance()
            emitter.instruction(&format!("str xzr, [{}, #48]", obj_ptr_scratch)); // clear the unused high word of the int property slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", obj_ptr_scratch)); // peek the obj pointer
            abi::emit_load_int_immediate(emitter, "r10", factory_id);
            emitter.instruction(&format!("mov QWORD PTR [{} + 40], r10", obj_ptr_scratch)); // commit __factory id for newInstance()
            emitter.instruction(&format!("mov QWORD PTR [{} + 48], 0", obj_ptr_scratch)); // clear the unused high word of the int property slot
        }
    }
}

fn emit_box_arg_aarch64(arg: &AttrArgValue, emitter: &mut Emitter, data: &mut DataSection) {
    match arg {
        AttrArgValue::Null => {
            emitter.instruction("mov x0, #8");                                  // runtime tag 8 = null payload
            emitter.instruction("mov x1, xzr");                                 // null carries no low word
            emitter.instruction("mov x2, xzr");                                 // null carries no high word
        }
        AttrArgValue::Int(value) => {
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = integer payload
            abi::emit_load_int_immediate(emitter, "x1", *value);
            emitter.instruction("mov x2, xzr");                                 // ints do not use the high word
        }
        AttrArgValue::Bool(value) => {
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = boolean payload
            emitter.instruction(&format!("mov x1, #{}", *value as u64));        // x1 = 0 or 1
            emitter.instruction("mov x2, xzr");                                 // bools do not use the high word
        }
        AttrArgValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (sym, len) = data.add_string(&bytes);
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string payload
            abi::emit_symbol_address(emitter, "x1", &sym);                      // x1 = string data address
            emitter.instruction(&format!("mov x2, #{}", len));                  // x2 = string length
        }
    }
    emitter.instruction("bl __rt_mixed_from_value");                            // box the captured payload into an owned mixed cell
}

fn emit_box_arg_x86_64(arg: &AttrArgValue, emitter: &mut Emitter, data: &mut DataSection) {
    match arg {
        AttrArgValue::Null => {
            emitter.instruction("mov rax, 8");                                  // runtime tag 8 = null payload
            emitter.instruction("xor rdi, rdi");                                // null carries no low word
            emitter.instruction("xor rsi, rsi");                                // null carries no high word
        }
        AttrArgValue::Int(value) => {
            emitter.instruction("mov rax, 0");                                  // runtime tag 0 = integer payload
            abi::emit_load_int_immediate(emitter, "rdi", *value);
            emitter.instruction("xor rsi, rsi");                                // ints do not use the high word
        }
        AttrArgValue::Bool(value) => {
            emitter.instruction("mov rax, 3");                                  // runtime tag 3 = boolean payload
            emitter.instruction(&format!("mov rdi, {}", *value as u64));        // rdi = 0 or 1
            emitter.instruction("xor rsi, rsi");                                // bools do not use the high word
        }
        AttrArgValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (sym, len) = data.add_string(&bytes);
            emitter.instruction("mov rax, 1");                                  // runtime tag 1 = string payload
            abi::emit_symbol_address(emitter, "rdi", &sym);                     // rdi = string data address
            emitter.instruction(&format!("mov rsi, {}", len));                  // rsi = string length
        }
    }
    emitter.instruction("call __rt_mixed_from_value");                          // box the captured payload into an owned mixed cell
}

fn collect_from_attribute_lists(
    classes: &HashMap<String, ClassInfo>,
    names: &[String],
    args: &[Option<Vec<AttrArgValue>>],
    unique: &mut BTreeMap<(String, Vec<AttrArgValue>), ()>,
) {
    if names.len() != args.len() {
        return;
    }
    for (idx, attr_name) in names.iter().enumerate() {
        let Some(Some(attr_args)) = args.get(idx) else {
            continue;
        };
        let Some(resolved_name) = resolve_class_name(classes, attr_name) else {
            continue;
        };
        unique.insert((resolved_name.to_string(), attr_args.clone()), ());
    }
}
