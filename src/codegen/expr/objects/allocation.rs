//! Purpose:
//! Lowers object allocation and constructor-ready initialization.
//! Produces object-related expression results while respecting runtime metadata and ownership rules.
//!
//! Called from:
//! - `crate::codegen::expr::objects`
//!
//! Key details:
//! - Object handles, property storage, and class ids must stay consistent with emitted class tables.

use crate::codegen::builtins::arrays::callback_env;
use crate::codegen::{abi, runtime_value_tag};
use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::calls::args as call_args;
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::names::method_symbol;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, TypeExpr};
use crate::types::{FunctionSig, PhpType};

use super::super::{
    coerce_result_to_type, emit_expr, expr_result_heap_ownership,
    restore_concat_offset_after_nested_call,
    save_concat_offset_before_nested_call,
};
use super::dispatch::emit_dispatch_interface_method;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;
const NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;
const ITERATOR_ITERATOR_DOWNCAST_MESSAGE: &str =
    "Class to downcast to not found or not base class or does not implement Traversable";

/// Emits assembly for new object.
pub(super) fn emit_new_object(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if class_name == "Fiber" {
        return emit_new_fiber(args, emitter, ctx, data);
    }
    if is_spl_doubly_linked_list_family(class_name) {
        return emit_new_spl_doubly_linked_list(class_name, args, emitter, ctx);
    }
    if class_name == "SplFixedArray" {
        return emit_new_spl_fixed_array(args, emitter, ctx, data);
    }
    if matches!(class_name, "ArrayIterator" | "ArrayObject") {
        return emit_new_spl_array_storage_object(class_name, args, emitter, ctx, data);
    }
    if class_name == "IteratorIterator" {
        return emit_new_iterator_iterator(args, emitter, ctx, data);
    }
    if matches!(class_name, "CallbackFilterIterator" | "RecursiveCallbackFilterIterator") {
        return emit_new_callback_filter_iterator(class_name, args, emitter, ctx, data);
    }
    if super::reflection::is_reflection_owner_class(class_name) {
        return super::reflection::emit_new_reflection_owner(
            class_name, args, emitter, ctx, data,
        );
    }
    emit_new_object_core(class_name, args, true, emitter, ctx, data)
}

/// Emits assembly for new object core.
pub(super) fn emit_new_object_core(
    class_name: &str,
    args: &[Expr],
    run_constructor: bool,
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
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        emitter.comment("new stdClass()");
        // stdClass instances do not have static property slots; the
        // dedicated runtime helper allocates the 16-byte payload, stamps
        // the class_id, and seeds the dynamic-property hash. User-supplied
        // arguments (none allowed by PHP for stdClass) are ignored here.
        let _ = args;
        abi::emit_call_label(emitter, "__rt_stdclass_new");                     // allocate a fresh stdClass instance with an empty property hash
        return PhpType::Object(class_name.to_string());
    }
    let num_props = class_info.properties.len();
    // PHP 8.2 #[\AllowDynamicProperties] adds a single 8-byte slot after the
    // declared properties to hold a lazily-allocated hashtable pointer for
    // undeclared property storage. The slot is initialised to 0 (null) and
    // only allocated on the first dynamic property write.
    let dyn_props_slot = if class_info.allow_dynamic_properties {
        8
    } else {
        0
    };
    let obj_size = 8 + num_props * 16 + dyn_props_slot; // 8 for class_id + 16 per property + optional dyn_props ptr

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
        let property_name = &class_info.properties[i].0;
        let starts_uninitialized = class_info.declared_properties.contains(property_name)
            && class_info.defaults.get(i).is_some_and(|default| default.is_none());
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
        if starts_uninitialized {
            let marker_reg = abi::temp_int_reg(emitter.target);
            abi::emit_load_int_immediate(emitter, marker_reg, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldr x9, [sp]");                        // peek object pointer before marking this typed property uninitialized
                }
                Arch::X86_64 => {
                    emitter.instruction("mov r11, QWORD PTR [rsp]");            // peek object pointer before marking this typed property uninitialized
                }
            }
            let object_reg = match emitter.target.arch {
                Arch::AArch64 => "x9",
                Arch::X86_64 => "r11",
            };
            abi::emit_store_to_address(emitter, marker_reg, object_reg, offset + 8);
        }
    }

    // -- allocate the dyn_props hashtable if the class declares
    // #[\AllowDynamicProperties], and store the pointer in the slot --
    if dyn_props_slot != 0 {
        let offset = 8 + num_props * 16;
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #4");                              // initial hashtable capacity for dyn_props
                emitter.instruction("mov x1, #7");                              // value type tag = mixed (heterogeneous)
                emitter.instruction("bl __rt_hash_new");                        // allocate empty hashtable -> x0 = hashtable pointer
                emitter.instruction("ldr x9, [sp]");                            // peek object pointer for dyn_props slot store
                emitter.instruction(&format!("str x0, [x9, #{}]", offset));     // store the hashtable pointer in the dyn_props slot
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, 4");                              // initial hashtable capacity for dyn_props
                emitter.instruction("mov rsi, 7");                              // value type tag = mixed
                emitter.instruction("call __rt_hash_new");                      // allocate empty hashtable -> rax = hashtable pointer
                emitter.instruction("mov r11, QWORD PTR [rsp]");                // peek object pointer for dyn_props slot store
                emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", offset)); // store the hashtable pointer in the dyn_props slot
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
                PhpType::Resource(_) => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    let tag_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, tag_reg, 9);
                    abi::emit_store_to_address(emitter, tag_reg, object_reg, offset + 8);
                }
                PhpType::Mixed | PhpType::Iterable => {
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
                PhpType::Void => {
                    let null_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, null_reg, NULL_SENTINEL);
                    abi::emit_store_to_address(emitter, null_reg, object_reg, offset);
                    abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
                }
                PhpType::Never => {}
            }
        }
    }

    // -- call __construct if it exists --
    if run_constructor && class_info.methods.contains_key("__construct") {
        let sig = class_info.methods.get("__construct").cloned();
        let regular_param_count = call_args::regular_param_count(sig.as_ref(), args.len());
        let emitted_args = call_args::emit_pushed_call_args(
            args,
            sig.as_ref(),
            regular_param_count,
            "constructor ref arg",
            false,
            true,
            emitter,
            ctx,
            data,
        );
        let arg_types = emitted_args.arg_types;

        let assignments = crate::codegen::abi::build_outgoing_arg_assignments_for_target(
            emitter.target,
            &arg_types,
            1,
        );
        let overflow_bytes =
            crate::codegen::abi::materialize_outgoing_args(emitter, &assignments);

        let receiver_offset = overflow_bytes + emitted_args.source_temp_bytes;
        if receiver_offset == 0 {
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
                    emitter.instruction(&format!("ldr x0, [sp, #{}]", receiver_offset)); // skip argument temporaries to reload the saved object pointer as $this on AArch64
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", receiver_offset)); // skip argument temporaries to reload the saved object pointer as $this in the first SysV integer argument register on x86_64
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
        abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes); // drop source-order named-argument temporaries after constructor dispatch
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the allocated object pointer as the expression result for the active target ABI
    PhpType::Object(class_name.to_string())
}

/// Returns true when SPL doubly linked list family.
fn is_spl_doubly_linked_list_family(class_name: &str) -> bool {
    matches!(class_name, "SplDoublyLinkedList" | "SplStack" | "SplQueue")
}

/// Emits assembly for new SPL doubly linked list.
fn emit_new_spl_doubly_linked_list(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &Context,
) -> PhpType {
    if !args.is_empty() {
        emitter.comment(&format!(
            "WARNING: {} constructor arguments ignored by runtime-managed SPL list",
            class_name
        ));
    }
    let class_id = ctx
        .classes
        .get(class_name)
        .map(|info| info.class_id)
        .unwrap_or(0);
    emitter.comment(&format!("new {}() — SPL runtime construction", class_name));
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        class_id as i64,
    );                                                                          // pass the concrete SPL class id to the runtime allocator
    abi::emit_call_label(emitter, "__rt_spl_dll_new");                         // allocate a runtime-managed SPL doubly-linked-list payload
    PhpType::Object(class_name.to_string())
}

/// Emits assembly for new SPL fixed array.
fn emit_new_spl_fixed_array(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let class_id = ctx
        .classes
        .get("SplFixedArray")
        .map(|info| info.class_id)
        .unwrap_or(0);
    emitter.comment("new SplFixedArray() — SPL runtime construction");
    if let Some(size_expr) = args.first() {
        let actual_ty = emit_expr(size_expr, emitter, ctx, data);
        coerce_result_to_type(emitter, ctx, data, &actual_ty, &PhpType::Int);
    } else {
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve constructor size while loading class id
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        class_id as i64,
    );                                                                          // pass the concrete SplFixedArray class id to the runtime allocator
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 1));       // pass requested fixed-array size as the second runtime argument
    abi::emit_call_label(emitter, "__rt_spl_fixed_new");                       // allocate a runtime-managed SplFixedArray payload
    PhpType::Object("SplFixedArray".to_string())
}

/// Emits assembly for new SPL array storage object.
fn emit_new_spl_array_storage_object(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("new {}() — SPL array storage construction", class_name));
    let Some(class_info) = ctx.classes.get(class_name).cloned() else {
        emitter.comment(&format!("WARNING: missing {} metadata", class_name));
        return PhpType::Object(class_name.to_string());
    };
    let keys_offset = *class_info.property_offsets.get("keys").unwrap_or(&8);
    let values_offset = *class_info.property_offsets.get("values").unwrap_or(&24);
    let flags_offset = class_info
        .property_offsets
        .get("flags")
        .copied()
        .unwrap_or(if class_name == "ArrayIterator" { 56 } else { 40 });
    let position_offset = class_info.property_offsets.get("position").copied();

    emit_new_object_core(class_name, &[], false, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the allocated SPL storage object while constructor arguments are normalized

    let source_ty = if let Some(source_expr) = args.first() {
        let ty = emit_expr(source_expr, emitter, ctx, data);
        if matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
            ty
        } else {
            emitter.comment("WARNING: ArrayIterator/ArrayObject source was not statically typed as array");
            emit_empty_mixed_array(emitter);
            PhpType::Array(Box::new(PhpType::Mixed))
        }
    } else {
        emit_empty_mixed_array(emitter);
        PhpType::Array(Box::new(PhpType::Mixed))
    };
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the source array for both keys and values extraction

    if let Some(flags_expr) = args.get(1) {
        let flags_ty = emit_expr(flags_expr, emitter, ctx, data);
        coerce_result_to_type(emitter, ctx, data, &flags_ty, &PhpType::Int);
    } else {
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve ArrayIterator/ArrayObject flags until property storage is ready

    load_storage_source_from_stack(emitter);
    let keys_ty = crate::codegen::builtins::arrays::array_keys::emit_loaded_keys(
        &source_ty,
        emitter,
        ctx,
    )
    .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Mixed)));
    emit_convert_loaded_indexed_array_to_mixed(&keys_ty, emitter);
    store_storage_array_property_from_result(emitter, keys_offset, 32);

    load_storage_source_from_stack(emitter);
    let values_ty = crate::codegen::builtins::arrays::array_values::emit_loaded_values(
        &source_ty,
        emitter,
        ctx,
        data,
    )
    .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Mixed)));
    emit_convert_loaded_indexed_array_to_mixed(&values_ty, emitter);
    store_storage_array_property_from_result(emitter, values_offset, 32);

    store_storage_int_property_from_stack(emitter, flags_offset, 0, 32);
    if let Some(position_offset) = position_offset {
        store_storage_zero_property(emitter, position_offset, 32);
    }

    abi::emit_release_temporary_stack(emitter, 32);                             // discard preserved flags and source array after storage initialization
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the initialized SPL storage object as the expression result
    PhpType::Object(class_name.to_string())
}

/// Emits assembly for new iterator iterator.
fn emit_new_iterator_iterator(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("new IteratorIterator() — Traversable normalization");
    let Some(class_info) = ctx.classes.get("IteratorIterator").cloned() else {
        emitter.comment("WARNING: missing IteratorIterator metadata");
        return PhpType::Object("IteratorIterator".to_string());
    };
    let inner_offset = class_info.property_offsets.get("inner").copied().unwrap_or(8);
    let normalized_args =
        normalize_iterator_iterator_constructor_args(&class_info, args, emitter, ctx, data);

    emit_new_object_core("IteratorIterator", &[], false, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the allocated IteratorIterator while normalizing the constructor source

    if let Some(iterator_expr) = normalized_args.first() {
        let source_ty = emit_expr(iterator_expr, emitter, ctx, data);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the Traversable candidate while evaluating the optional downcast class
        emit_iterator_iterator_downcast_arg_status(normalized_args.get(1), emitter, ctx, data);
        emit_normalize_saved_traversable_to_iterator(iterator_expr, &source_ty, emitter, ctx);
    } else {
        emitter.comment("WARNING: IteratorIterator constructor missing Traversable source");
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    }

    store_iterator_inner_property_from_result(emitter, inner_offset);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the initialized IteratorIterator as the expression result
    PhpType::Object("IteratorIterator".to_string())
}

/// Emits assembly for new callback filter iterator.
fn emit_new_callback_filter_iterator(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("new {}() — callback filter construction", class_name));
    let Some(class_info) = ctx.classes.get(class_name).cloned() else {
        emitter.comment(&format!("WARNING: missing {} metadata", class_name));
        return PhpType::Object(class_name.to_string());
    };
    let inner_offset = class_info.property_offsets.get("inner").copied().unwrap_or(8);
    let callback_offset = class_info
        .property_offsets
        .get("callback")
        .copied()
        .unwrap_or(24);
    let callback_env_offset = class_info
        .property_offsets
        .get("callbackEnv")
        .copied()
        .unwrap_or(40);
    let normalized_args = normalize_constructor_args(&class_info, args, emitter, ctx, data);

    emit_new_object_core(class_name, &[], false, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the allocated callback-filter object while constructor arguments are stored

    if let Some(iterator_expr) = normalized_args.first() {
        let actual_ty = emit_expr(iterator_expr, emitter, ctx, data);
        coerce_result_to_type(
            emitter,
            ctx,
            data,
            &actual_ty,
            &PhpType::Object("Iterator".to_string()),
        );
    } else {
        emitter.comment(&format!("WARNING: {} constructor missing Iterator source", class_name));
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    }
    store_iterator_inner_property_from_result(emitter, inner_offset);

    if let Some(callback_expr) = normalized_args.get(1) {
        let (_callback_ty, captures, target_visible_arg_types) =
            emit_callback_filter_callable_arg(callback_expr, emitter, ctx, data);
        if captures.is_empty() {
            store_callable_property_from_result(emitter, callback_offset);
            store_pointer_property_zero(emitter, callback_env_offset);
        } else {
            let wrapper_label = callback_env::emit_persistent_callback_env_from_result(
                &captures,
                callback_filter_visible_arg_types(),
                target_visible_arg_types,
                emitter,
                ctx,
            );
            store_pointer_property_from_result(emitter, callback_env_offset);
            crate::codegen::callable_descriptor::emit_load_descriptor_address(
                emitter,
                data,
                abi::int_result_reg(emitter),
                &wrapper_label,
                None,
                crate::codegen::callable_descriptor::CALLABLE_DESC_KIND_CALLBACK_ADAPTER,
            );
            store_callable_property_from_result(emitter, callback_offset);
        }
    } else {
        emitter.comment(&format!("WARNING: {} constructor missing callback", class_name));
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        store_pointer_property_zero(emitter, callback_env_offset);
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        store_callable_property_from_result(emitter, callback_offset);
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the initialized callback-filter object as the expression result
    PhpType::Object(class_name.to_string())
}

/// Normalizes iterator iterator constructor args into the representation expected by later lowering.
fn normalize_iterator_iterator_constructor_args(
    class_info: &crate::types::ClassInfo,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<Expr> {
    normalize_constructor_args(class_info, args, emitter, ctx, data)
}

/// Normalizes constructor args into the representation expected by later lowering.
fn normalize_constructor_args(
    class_info: &crate::types::ClassInfo,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<Expr> {
    let Some(sig) = class_info.methods.get("__construct") else {
        return args.to_vec();
    };
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let regular_param_count = call_args::regular_param_count(Some(sig), args.len());
    call_args::preevaluate_named_call_args_to_temps(
        sig,
        args,
        call_span,
        regular_param_count,
        false,
        emitter,
        ctx,
        data,
    )
    .args
}

/// Computes the callable signature metadata for callback filter callable.
fn callback_filter_callable_sig() -> FunctionSig {
    FunctionSig {
        params: vec![
            ("current".to_string(), PhpType::Mixed),
            ("key".to_string(), PhpType::Mixed),
            ("iterator".to_string(), PhpType::Object("Iterator".to_string())),
        ],
        defaults: vec![None, None, None],
        return_type: PhpType::Bool,
        declared_return: false,
        ref_params: vec![false, false, false],
        declared_params: vec![false, false, false],
        variadic: None,
        deprecation: None,
    }
}

/// Provides the Callback filter visible arg types helper used by the allocation module.
fn callback_filter_visible_arg_types() -> Vec<PhpType> {
    vec![
        PhpType::Mixed,
        PhpType::Mixed,
        PhpType::Object("Iterator".to_string()),
    ]
}

/// Emits assembly for callback filter callable arg.
fn emit_callback_filter_callable_arg(
    callback_expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> (PhpType, Vec<(String, PhpType, bool)>, Vec<PhpType>) {
    let previous_sig = ctx
        .expected_first_class_callable_sig
        .replace(callback_filter_callable_sig());
    let (callback_ty, capture_source) = if let ExprKind::Variable(name) = &callback_expr.kind {
        if let Some(CallableTarget::Function(function_name)) =
            ctx.first_class_callable_targets.get(name).cloned()
        {
            let synthetic = Expr::new(
                ExprKind::FirstClassCallable(CallableTarget::Function(function_name)),
                callback_expr.span,
            );
            (emit_expr(&synthetic, emitter, ctx, data), synthetic)
        } else {
            (emit_expr(callback_expr, emitter, ctx, data), callback_expr.clone())
        }
    } else {
        (emit_expr(callback_expr, emitter, ctx, data), callback_expr.clone())
    };
    let captures = crate::codegen::callables::callable_captures(&capture_source, ctx);
    let target_visible_arg_types = callback_filter_target_arg_types(&capture_source, ctx);
    ctx.expected_first_class_callable_sig = previous_sig;
    (callback_ty, captures, target_visible_arg_types)
}

/// Provides the Callback filter target arg types helper used by the allocation module.
fn callback_filter_target_arg_types(callback_expr: &Expr, ctx: &Context) -> Vec<PhpType> {
    let sig = match &callback_expr.kind {
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) => {
            ctx.deferred_closures.last().map(|closure| closure.sig.clone())
        }
        _ => crate::codegen::callables::callable_sig(callback_expr, ctx),
    };
    sig.map(|sig| {
        sig.params
            .into_iter()
            .take(3)
            .map(|(_, ty)| ty)
            .collect::<Vec<_>>()
    })
    .filter(|types| types.len() == 3)
    .unwrap_or_else(callback_filter_visible_arg_types)
}

/// Emits assembly for iterator iterator downcast arg status.
fn emit_iterator_iterator_downcast_arg_status(
    class_expr: Option<&Expr>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let Some(class_expr) = class_expr else {
        emit_push_iterator_iterator_downcast_status(emitter, 0, 0);
        return;
    };

    let class_ty = emit_expr(class_expr, emitter, ctx, data).codegen_repr();
    match class_ty {
        PhpType::Str => emit_push_iterator_iterator_downcast_status_from_string(emitter, ctx),
        PhpType::Void | PhpType::Never => {
            emit_push_iterator_iterator_downcast_status(emitter, 0, 0);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_push_iterator_iterator_downcast_status_from_mixed(emitter, ctx);
        }
        _ => emit_push_iterator_iterator_downcast_status(emitter, 2, 0),
    }
}

/// Emits assembly for push iterator iterator downcast status from string.
fn emit_push_iterator_iterator_downcast_status_from_string(
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    abi::emit_call_label(emitter, "__rt_instanceof_lookup");                    // resolve the optional downcast class-string argument
    emit_push_iterator_iterator_downcast_status_from_lookup(emitter, ctx);
}

/// Emits assembly for push iterator iterator downcast status from mixed.
fn emit_push_iterator_iterator_downcast_status_from_mixed(
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let string_case = ctx.next_label("iterator_iterator_downcast_string");
    let null_case = ctx.next_label("iterator_iterator_downcast_null");
    let invalid_case = ctx.next_label("iterator_iterator_downcast_invalid");
    let done = ctx.next_label("iterator_iterator_downcast_done");

    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // inspect nullable mixed downcast values at runtime
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #1");                                  // runtime tag 1 means the downcast argument is a string
            emitter.instruction(&format!("b.eq {}", string_case));              // resolve string downcast targets through class metadata
            emitter.instruction("cmp x0, #8");                                  // runtime tag 8 means the downcast argument is null
            emitter.instruction(&format!("b.eq {}", null_case));                // null behaves like the omitted second constructor argument
            emitter.instruction(&format!("b {}", invalid_case));                // non-string, non-null mixed payloads are invalid downcast targets
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 1");                                  // runtime tag 1 means the downcast argument is a string
            emitter.instruction(&format!("je {}", string_case));                // resolve string downcast targets through class metadata
            emitter.instruction("cmp rax, 8");                                  // runtime tag 8 means the downcast argument is null
            emitter.instruction(&format!("je {}", null_case));                  // null behaves like the omitted second constructor argument
            emitter.instruction(&format!("jmp {}", invalid_case));              // non-string, non-null mixed payloads are invalid downcast targets
        }
    }

    emitter.label(&string_case);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, rdi");                                    // move the unboxed string pointer into the lookup input register
    }
    emit_push_iterator_iterator_downcast_status_from_string(emitter, ctx);
    abi::emit_jump(emitter, &done);                                             // converge after pushing the resolved downcast metadata

    emitter.label(&null_case);
    emit_push_iterator_iterator_downcast_status(emitter, 0, 0);
    abi::emit_jump(emitter, &done);                                             // converge after pushing the omitted/null downcast marker

    emitter.label(&invalid_case);
    emit_push_iterator_iterator_downcast_status(emitter, 2, 0);

    emitter.label(&done);
}

/// Emits assembly for push iterator iterator downcast status from lookup.
fn emit_push_iterator_iterator_downcast_status_from_lookup(
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let invalid_case = ctx.next_label("iterator_iterator_downcast_lookup_invalid");
    let done = ctx.next_label("iterator_iterator_downcast_lookup_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // did the class-string lookup resolve to a declared target?
            emitter.instruction(&format!("b.eq {}", invalid_case));             // unknown downcast class names fail when the source is an aggregate
            emitter.instruction("cmp x2, #0");                                  // only concrete class targets are valid downcast classes
            emitter.instruction(&format!("b.ne {}", invalid_case));             // interface names are not valid IteratorIterator downcast classes
            emitter.instruction("mov x0, #1");                                  // status 1 means a concrete downcast class id follows
            emitter.instruction(&format!("b {}", done));                        // keep the resolved class id in x1

            emitter.label(&invalid_case);
            emitter.instruction("mov x0, #2");                                  // status 2 means the class argument must throw for aggregates
            emitter.instruction("mov x1, #0");                                  // invalid targets have no usable class id
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // did the class-string lookup resolve to a declared target?
            emitter.instruction(&format!("je {}", invalid_case));               // unknown downcast class names fail when the source is an aggregate
            emitter.instruction("test rdx, rdx");                               // only concrete class targets are valid downcast classes
            emitter.instruction(&format!("jne {}", invalid_case));              // interface names are not valid IteratorIterator downcast classes
            emitter.instruction("mov rax, 1");                                  // status 1 means a concrete downcast class id follows
            emitter.instruction(&format!("jmp {}", done));                      // keep the resolved class id in rdi

            emitter.label(&invalid_case);
            emitter.instruction("mov rax, 2");                                  // status 2 means the class argument must throw for aggregates
            emitter.instruction("xor edi, edi");                                // invalid targets have no usable class id
        }
    }
    emitter.label(&done);
    match emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg_pair(emitter, "x0", "x1"),
        Arch::X86_64 => abi::emit_push_reg_pair(emitter, "rax", "rdi"),
    }
}

/// Emits assembly for push iterator iterator downcast status.
fn emit_push_iterator_iterator_downcast_status(
    emitter: &mut Emitter,
    status: i64,
    class_id: i64,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(emitter, "x0", status);
            abi::emit_load_int_immediate(emitter, "x1", class_id);
            abi::emit_push_reg_pair(emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(emitter, "rax", status);
            abi::emit_load_int_immediate(emitter, "rdi", class_id);
            abi::emit_push_reg_pair(emitter, "rax", "rdi");
        }
    }
}

/// Emits assembly for normalize saved traversable to iterator.
fn emit_normalize_saved_traversable_to_iterator(
    source_expr: &Expr,
    source_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let iterator_id = ctx
        .interfaces
        .get("Iterator")
        .expect("codegen bug: missing builtin Iterator interface")
        .interface_id;
    let aggregate_id = ctx
        .interfaces
        .get("IteratorAggregate")
        .expect("codegen bug: missing builtin IteratorAggregate interface")
        .interface_id;
    let direct_case = ctx.next_label("iterator_iterator_source_iterator");
    let aggregate_case = ctx.next_label("iterator_iterator_source_aggregate");
    let done = ctx.next_label("iterator_iterator_source_done");
    let source_is_borrowed = expr_result_heap_ownership(source_expr) != HeapOwnership::Owned;

    emit_branch_if_saved_traversable_implements(iterator_id, 16, &direct_case, emitter);
    emit_branch_if_saved_traversable_implements(aggregate_id, 16, &aggregate_case, emitter);
    abi::emit_release_temporary_stack(emitter, 32);                             // discard downcast metadata and unsupported Traversable candidate
    abi::emit_call_label(emitter, "__rt_iterable_unsupported_kind");            // invalid Traversable metadata aborts defensively

    emitter.label(&direct_case);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard ignored downcast metadata for direct Iterator inputs
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the direct Iterator object pointer
    if source_is_borrowed {
        abi::emit_incref_if_refcounted(emitter, &source_ty.codegen_repr());
    }
    abi::emit_jump(emitter, &done);                                             // direct Iterator inputs are already normalized

    emitter.label(&aggregate_case);
    emit_validate_iterator_iterator_aggregate_downcast(aggregate_id, emitter, ctx);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard validated downcast metadata before dispatching getIterator()
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the IteratorAggregate object pointer before getIterator()
    move_loaded_result_to_receiver_arg(emitter);
    emit_dispatch_interface_method("IteratorAggregate", "getiterator", emitter, ctx);

    emitter.label(&done);
}

/// Emits assembly for branch if saved traversable implements.
fn emit_branch_if_saved_traversable_implements(
    interface_id: u64,
    candidate_stack_offset: usize,
    target_label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [sp, #{}]", candidate_stack_offset)); // load the saved Traversable candidate as matcher argument 1
            abi::emit_load_int_immediate(emitter, "x1", interface_id as i64);
            abi::emit_load_int_immediate(emitter, "x2", 1);
            abi::emit_call_label(emitter, "__rt_exception_matches");            // test whether the candidate implements the requested Traversable interface
            emitter.instruction("cmp x0, #0");                                  // did the runtime interface matcher succeed?
            emitter.instruction(&format!("b.ne {}", target_label));             // branch to the matching normalization path
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", candidate_stack_offset)); // load the saved Traversable candidate as matcher argument 1
            abi::emit_load_int_immediate(emitter, "rsi", interface_id as i64);
            abi::emit_load_int_immediate(emitter, "rdx", 1);
            abi::emit_call_label(emitter, "__rt_exception_matches");            // test whether the candidate implements the requested Traversable interface
            emitter.instruction("test rax, rax");                               // did the runtime interface matcher succeed?
            emitter.instruction(&format!("jne {}", target_label));              // branch to the matching normalization path
        }
    }
}

/// Emits assembly for validate iterator iterator aggregate downcast.
fn emit_validate_iterator_iterator_aggregate_downcast(
    aggregate_interface_id: u64,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let skip = ctx.next_label("iterator_iterator_downcast_skip");
    let throw = ctx.next_label("iterator_iterator_downcast_throw");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // load downcast status: 0 omitted/null, 1 class id, 2 invalid
            emitter.instruction(&format!("cbz x9, {}", skip));                  // omitted/null class arguments do not constrain IteratorAggregate inputs
            emitter.instruction("cmp x9, #1");                                  // only status 1 carries a valid concrete class id
            emitter.instruction(&format!("b.ne {}", throw));                    // invalid class names and interfaces throw LogicException for aggregates
            emitter.instruction("ldr x0, [sp, #16]");                           // pass the saved IteratorAggregate object to the class matcher
            emitter.instruction("ldr x1, [sp, #8]");                            // pass the requested downcast class id to the class matcher
            abi::emit_load_int_immediate(emitter, "x2", 0);
            abi::emit_call_label(emitter, "__rt_exception_matches");            // require the aggregate object to be an instance of the requested class
            emitter.instruction("cmp x0, #0");                                  // did the aggregate object match the requested class?
            emitter.instruction(&format!("b.eq {}", throw));                    // non-base downcast classes are rejected like PHP
            emitter.instruction("ldr x0, [sp, #8]");                            // pass the requested class id to the metadata-only interface checker
            abi::emit_load_int_immediate(emitter, "x1", aggregate_interface_id as i64);
            abi::emit_call_label(emitter, "__rt_class_implements_interface");   // require the downcast class itself to implement IteratorAggregate
            emitter.instruction("cmp x0, #0");                                  // did the downcast class implement IteratorAggregate?
            emitter.instruction(&format!("b.eq {}", throw));                    // non-Traversable base classes are rejected like PHP
            emitter.instruction(&format!("b {}", skip));                        // the aggregate downcast class is valid
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // load downcast status: 0 omitted/null, 1 class id, 2 invalid
            emitter.instruction("test r10, r10");                               // is there an explicit downcast class to validate?
            emitter.instruction(&format!("je {}", skip));                       // omitted/null class arguments do not constrain IteratorAggregate inputs
            emitter.instruction("cmp r10, 1");                                  // only status 1 carries a valid concrete class id
            emitter.instruction(&format!("jne {}", throw));                     // invalid class names and interfaces throw LogicException for aggregates
            emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");               // pass the saved IteratorAggregate object to the class matcher
            emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                // pass the requested downcast class id to the class matcher
            abi::emit_load_int_immediate(emitter, "rdx", 0);
            abi::emit_call_label(emitter, "__rt_exception_matches");            // require the aggregate object to be an instance of the requested class
            emitter.instruction("test rax, rax");                               // did the aggregate object match the requested class?
            emitter.instruction(&format!("je {}", throw));                      // non-base downcast classes are rejected like PHP
            emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // pass the requested class id to the metadata-only interface checker
            abi::emit_load_int_immediate(emitter, "rsi", aggregate_interface_id as i64);
            abi::emit_call_label(emitter, "__rt_class_implements_interface");   // require the downcast class itself to implement IteratorAggregate
            emitter.instruction("test rax, rax");                               // did the downcast class implement IteratorAggregate?
            emitter.instruction(&format!("je {}", throw));                      // non-Traversable base classes are rejected like PHP
            emitter.instruction(&format!("jmp {}", skip));                      // the aggregate downcast class is valid
        }
    }

    emitter.label(&throw);
    emit_throw_iterator_iterator_downcast_logic_exception(emitter);
    emitter.label(&skip);
}

/// Emits assembly for throw iterator iterator downcast logic exception.
fn emit_throw_iterator_iterator_downcast_logic_exception(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #32");                                 // request Throwable payload storage
            emitter.instruction("bl __rt_heap_alloc");                          // allocate the LogicException object payload
            emitter.instruction("mov x9, #6");                                  // heap kind 6 = object instance
            emitter.instruction("str x9, [x0, #-8]");                           // stamp allocation as a runtime object
            abi::emit_symbol_address(emitter, "x9", "_spl_logic_exception_class_id");
            emitter.instruction("ldr x9, [x9]");                                // load LogicException's runtime class id for this program
            emitter.instruction("str x9, [x0]");                                // store class id at object header
            abi::emit_symbol_address(emitter, "x9", "_iterator_iterator_downcast_msg");
            emitter.instruction("str x9, [x0, #8]");                            // store static exception message pointer
            emitter.instruction(&format!("mov x9, #{}", ITERATOR_ITERATOR_DOWNCAST_MESSAGE.len())); // load static exception message length
            emitter.instruction("str x9, [x0, #16]");                           // store exception message length
            emitter.instruction("str xzr, [x0, #24]");                          // exception code defaults to zero
            abi::emit_symbol_address(emitter, "x9", "_exc_value");
            emitter.instruction("str x0, [x9]");                                // publish the active exception object
            emitter.instruction("b __rt_throw_current");                        // enter the standard exception unwinder
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve caller frame pointer for exception allocation
            emitter.instruction("mov rbp, rsp");                                // establish aligned helper frame
            emitter.instruction("sub rsp, 16");                                 // keep the nested heap allocation call 16-byte aligned
            emitter.instruction("mov rax, 32");                                 // request Throwable payload storage
            emitter.instruction("call __rt_heap_alloc");                        // allocate the LogicException object payload
            emitter.instruction("mov r10, 0x4548504c00000006");                 // x86_64 heap-kind word: HE LP magic + kind 6 object
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp allocation as a runtime object
            emitter.instruction("mov r10, QWORD PTR [rip + _spl_logic_exception_class_id]"); // load LogicException's runtime class id for this program
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store class id at object header
            emitter.instruction("lea r10, [rip + _iterator_iterator_downcast_msg]"); // materialize static exception message pointer
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store static exception message pointer
            emitter.instruction(&format!("mov QWORD PTR [rax + 16], {}", ITERATOR_ITERATOR_DOWNCAST_MESSAGE.len())); // store static exception message length
            emitter.instruction("mov QWORD PTR [rax + 24], 0");                 // exception code defaults to zero
            emitter.instruction("mov QWORD PTR [rip + _exc_value], rax");       // publish the active exception object
            emitter.instruction("mov rsp, rbp");                                // release helper frame before throwing
            emitter.instruction("pop rbp");                                     // restore caller frame pointer before throwing
            emitter.instruction("jmp __rt_throw_current");                      // enter the standard exception unwinder
        }
    }
}

/// Moves loaded result to receiver arg into the register or storage slot expected by the next operation.
fn move_loaded_result_to_receiver_arg(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the object result into the SysV receiver argument register
    }
}

/// Stores iterator inner property from result into runtime storage or stack state.
fn store_iterator_inner_property_from_result(emitter: &mut Emitter, inner_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // reload the IteratorIterator object pointer
            emitter.instruction(&format!("str x0, [x9, #{}]", inner_offset));   // store the normalized inner Iterator object
            emitter.instruction("mov x10, #6");                                 // runtime property tag 6 = object
            emitter.instruction(&format!("str x10, [x9, #{}]", inner_offset + 8)); // stamp the inner property as an object
        }
        Arch::X86_64 => {
            emitter.instruction("mov r11, QWORD PTR [rsp]");                    // reload the IteratorIterator object pointer
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", inner_offset)); // store the normalized inner Iterator object
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 6", inner_offset + 8)); // stamp the inner property as an object
        }
    }
}

/// Stores callable property from result into runtime storage or stack state.
fn store_callable_property_from_result(emitter: &mut Emitter, property_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // reload the object pointer that owns the callable property
            emitter.instruction(&format!("str x0, [x9, #{}]", property_offset)); // store the callable descriptor pointer
            emitter.instruction(&format!("str xzr, [x9, #{}]", property_offset + 8)); // clear the unused inline property metadata slot for callable descriptors
        }
        Arch::X86_64 => {
            emitter.instruction("mov r11, QWORD PTR [rsp]");                    // reload the object pointer that owns the callable property
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", property_offset)); // store the callable descriptor pointer
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", property_offset + 8)); // clear the unused inline property metadata slot for callable descriptors
        }
    }
}

/// Stores pointer property from result into runtime storage or stack state.
fn store_pointer_property_from_result(emitter: &mut Emitter, property_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // reload the object pointer that owns the raw pointer property
            emitter.instruction(&format!("str x0, [x9, #{}]", property_offset)); // store the raw pointer payload
            emitter.instruction(&format!("str xzr, [x9, #{}]", property_offset + 8)); // clear pointer property metadata because it is not PHP-owned
        }
        Arch::X86_64 => {
            emitter.instruction("mov r11, QWORD PTR [rsp]");                    // reload the object pointer that owns the raw pointer property
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", property_offset)); // store the raw pointer payload
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", property_offset + 8)); // clear pointer property metadata because it is not PHP-owned
        }
    }
}

/// Stores pointer property zero into runtime storage or stack state.
fn store_pointer_property_zero(emitter: &mut Emitter, property_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // reload the object pointer that owns the raw pointer property
            emitter.instruction(&format!("str xzr, [x9, #{}]", property_offset)); // initialize the raw pointer payload as null
            emitter.instruction(&format!("str xzr, [x9, #{}]", property_offset + 8)); // clear pointer property metadata because it is not PHP-owned
        }
        Arch::X86_64 => {
            emitter.instruction("mov r11, QWORD PTR [rsp]");                    // reload the object pointer that owns the raw pointer property
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", property_offset)); // initialize the raw pointer payload as null
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", property_offset + 8)); // clear pointer property metadata because it is not PHP-owned
        }
    }
}

/// Emits assembly for empty mixed array.
fn emit_empty_mixed_array(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #4");                                  // allocate a small empty storage array for SPL keys/values
            emitter.instruction("mov x1, #8");                                  // Mixed storage uses pointer-sized slots
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, 4");                                  // allocate a small empty storage array for SPL keys/values
            emitter.instruction("mov rsi, 8");                                  // Mixed storage uses pointer-sized slots
        }
    }
    abi::emit_call_label(emitter, "__rt_array_new");                           // allocate empty indexed storage
    emit_convert_loaded_indexed_array_to_mixed(&PhpType::Array(Box::new(PhpType::Int)), emitter);
}

/// Loads storage source from stack from runtime storage or stack state.
fn load_storage_source_from_stack(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the preserved constructor source array
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the preserved constructor source array
        }
    }
}

/// Emits assembly for convert loaded indexed array to mixed.
fn emit_convert_loaded_indexed_array_to_mixed(array_ty: &PhpType, emitter: &mut Emitter) {
    let elem_ty = match array_ty {
        PhpType::Array(elem_ty) => elem_ty.as_ref(),
        _ => &PhpType::Mixed,
    };
    let tag = runtime_value_tag(&elem_ty.codegen_repr()) as i64;
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x1, #{}", tag));                  // pass the current indexed-array value_type tag to the Mixed converter
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // pass the loaded indexed-array pointer to the Mixed converter
            emitter.instruction(&format!("mov rsi, {}", tag));                  // pass the current indexed-array value_type tag to the Mixed converter
        }
    }
    abi::emit_call_label(emitter, "__rt_array_to_mixed");                      // normalize SPL storage arrays to boxed Mixed slots
}

/// Stores storage array property from result into runtime storage or stack state.
fn store_storage_array_property_from_result(
    emitter: &mut Emitter,
    property_offset: usize,
    object_stack_offset: usize,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [sp, #{}]", object_stack_offset)); // reload the SPL storage object pointer
            emitter.instruction(&format!("str x0, [x9, #{}]", property_offset)); // store the initialized storage array pointer
            emitter.instruction("mov x10, #4");                                 // runtime property tag 4 = indexed array
            emitter.instruction(&format!("str x10, [x9, #{}]", property_offset + 8)); // stamp the property as an indexed array
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", object_stack_offset)); // reload the SPL storage object pointer
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", property_offset)); // store the initialized storage array pointer
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 4", property_offset + 8)); // stamp the property as an indexed array
        }
    }
}

/// Stores storage integer property from stack into runtime storage or stack state.
fn store_storage_int_property_from_stack(
    emitter: &mut Emitter,
    property_offset: usize,
    value_stack_offset: usize,
    object_stack_offset: usize,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [sp, #{}]", object_stack_offset)); // reload the SPL storage object pointer
            emitter.instruction(&format!("ldr x10, [sp, #{}]", value_stack_offset)); // reload the preserved integer property value
            emitter.instruction(&format!("str x10, [x9, #{}]", property_offset)); // store the integer property value
            emitter.instruction(&format!("str xzr, [x9, #{}]", property_offset + 8)); // clear scalar property metadata
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", object_stack_offset)); // reload the SPL storage object pointer
            emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", value_stack_offset)); // reload the preserved integer property value
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], r10", property_offset)); // store the integer property value
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", property_offset + 8)); // clear scalar property metadata
        }
    }
}

/// Stores storage zero property into runtime storage or stack state.
fn store_storage_zero_property(
    emitter: &mut Emitter,
    property_offset: usize,
    object_stack_offset: usize,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [sp, #{}]", object_stack_offset)); // reload the SPL storage object pointer
            emitter.instruction(&format!("str xzr, [x9, #{}]", property_offset)); // initialize the integer property to zero
            emitter.instruction(&format!("str xzr, [x9, #{}]", property_offset + 8)); // clear scalar property metadata
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", object_stack_offset)); // reload the SPL storage object pointer
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", property_offset)); // initialize the integer property to zero
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", property_offset + 8)); // clear scalar property metadata
        }
    }
}

/// Codegen interception for `new Fiber($callable)`.
///
/// The standard `emit_new_object` path would size the object as `8 + num_props * 16`,
/// which for Fiber (zero declared properties) yields only the object header and
/// not enough room for the runtime-managed Fiber payload. We instead delegate the
/// entire allocation, stack setup, and field initialisation to `__rt_fiber_construct`,
/// passing the captured closure plus the runtime class id so `instanceof Fiber` keeps
/// working.
fn emit_new_fiber(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let class_id = ctx
        .classes
        .get("Fiber")
        .map(|info| info.class_id)
        .unwrap_or(0);

    emitter.comment("new Fiber() — runtime construction");

    let wrapper_label = if let Some(callable_expr) = args.first() {
        emit_expr(callable_expr, emitter, ctx, data);
        super::fiber_wrapper::prepare_fiber_wrapper(callable_expr, ctx)
    } else {
        emitter.comment("WARNING: Fiber constructor missing $callback argument");
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        None
    };

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the closure pointer across constructor-argument setup
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 1),
        class_id as i64,
    );                                                                          // load the runtime class id of Fiber into the second integer argument register
    if let Some(label) = wrapper_label {
        abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 2), &label);
    } else {
        abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 2), 0);
    }
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 0));       // pop the closure pointer into the first integer argument register for the active target ABI
    abi::emit_call_label(emitter, "__rt_fiber_construct");                      // delegate allocation, stack setup, and field initialisation to the runtime helper

    // -- closure capture pre-load: when `new Fiber(function(...) use(...))` is
    //    used, evaluate each captured variable in the surrounding scope NOW
    //    (it is no longer reachable from inside the fiber's stack) and stash
    //    the boxed Mixed value in the trailing start_args slots. The
    //    user_arg_max field is lowered to the closure's user-param count so a
    //    later $f->start(...) only writes to the leading slots and leaves the
    //    captured values intact.
    let capture_info = args.first().and_then(|callable_expr| match &callable_expr.kind {
        ExprKind::Closure {
            params,
            captures,
            capture_refs,
            ..
        } if !captures.is_empty() => {
            // Compute the closure's user-param register footprint so capture
            // cursors start past whatever the closure's own parameters consume.
            // Floats land in d-regs, strings consume two int regs, everything
            // else consumes one int reg.
            let mut user_int_regs = 0usize;
            let mut user_float_regs = 0usize;
            for (_, type_expr, _, _) in params {
                let mut ty = type_expr.as_ref();
                while let Some(TypeExpr::Nullable(inner)) = ty {
                    ty = Some(inner.as_ref());
                }
                match ty {
                    Some(TypeExpr::Float) => user_float_regs += 1,
                    Some(TypeExpr::Str) => user_int_regs += 2,
                    _ => user_int_regs += 1,
                }
            }
            // user_arg_max for the start() spill path is the *count of user
            // params* (not their reg footprint). String params take 2 int
            // regs but are still one user param from the user's perspective,
            // and start() is type-erased Mixed-only so the spill cap mirrors
            // the param count, not the reg footprint.
            let user_param_count = params.len();
            Some((
                callable_expr.span,
                user_param_count,
                user_int_regs,
                user_float_regs,
                captures.clone(),
                capture_refs.clone(),
            ))
        }
        _ => None,
    });

    if let Some((
        span,
        user_param_count,
        user_int_regs,
        user_float_regs,
        captures,
        capture_refs,
    )) = capture_info
    {
        emit_fiber_capture_preload(
            emitter,
            ctx,
            data,
            user_param_count,
            user_int_regs,
            user_float_regs,
            &captures,
            &capture_refs,
            span,
        );
    }

    PhpType::Object("Fiber".to_string())
}

/// Push the freshly-built fiber pointer, evaluate each capture in the
/// surrounding scope, box it into a Mixed cell, and store it into the trailing
/// `start_args` slot the closure expects to find its capture in. Finally,
/// lower `user_arg_max` to the closure's user-param count so a future
/// `$f->start(...)` does not overwrite the pre-loaded captures.
fn emit_fiber_capture_preload(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    user_param_count: usize,
    user_int_regs: usize,
    user_float_regs: usize,
    captures: &[String],
    capture_refs: &[String],
    span: crate::span::Span,
) {
    let max_int_slots = crate::codegen::runtime::FIBER_START_ARGS_MAX as usize;
    let max_float_slots = crate::codegen::runtime::FIBER_FLOAT_ARGS_MAX as usize;
    let start_args_offset = crate::codegen::runtime::FIBER_START_ARGS_OFFSET;
    let float_args_offset = crate::codegen::runtime::FIBER_FLOAT_ARGS_OFFSET;
    let umax_off = crate::codegen::runtime::FIBER_USER_ARG_MAX_OFFSET;

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // park the freshly built Fiber pointer on the stack while we evaluate captures
    let mut int_slot = user_int_regs;
    let mut float_slot = user_float_regs;
    for capture_name in captures {
        let by_ref = capture_refs.iter().any(|name| name == capture_name);
        // The closure's body reads each capture through a local variable slot
        // whose declared type matches whatever PhpType the variable held in the
        // surrounding scope. Passing the raw register footprint (no Mixed
        // boxing) lines up with what the closure expects:
        //   * floats land in float_args[float_slot] and are loaded into d-regs
        //   * single-register int-like types (int, bool, object, callable,
        //     mixed) ride in start_args[int_slot]
        //   * strings ride as ptr+len across start_args[int_slot..int_slot+2]
        let actual_ty = if by_ref {
            if !crate::codegen::expr::calls::args::emit_ref_arg_variable_address(
                capture_name,
                "fiber capture ref",
                emitter,
                ctx,
            ) {
                emitter.comment(&format!(
                    "WARNING: Fiber capture {} dropped — variable not found",
                    capture_name
                ));
                continue;
            }
            PhpType::Int
        } else {
            let var_expr = Expr::new(ExprKind::Variable(capture_name.clone()), span);
            emit_expr(&var_expr, emitter, ctx, data)
        };
        if matches!(actual_ty, PhpType::Float) {
            if float_slot >= max_float_slots {
                emitter.comment(&format!(
                    "WARNING: Fiber float capture {} dropped — float_args slot budget exhausted",
                    capture_name
                ));
                continue;
            }
            let off = float_args_offset + (float_slot as i32) * 8;
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldr x9, [sp]");                        // peek the saved Fiber pointer from the parking slot
                    emitter.instruction(&format!("str d0, [x9, #{}]", off));    // float_args[float_slot] = float bits (loaded back into d-reg by the trampoline)
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rcx, QWORD PTR [rsp]");            // peek the saved Fiber pointer from the parking slot
                    emitter.instruction(&format!("movsd QWORD PTR [rcx + {}], xmm0", off)); // float_args[float_slot] = float bits
                }
            }
            float_slot += 1;
            continue;                                                            // floats are scalar — no incref needed
        }
        let consumes_two = matches!(actual_ty, PhpType::Str);
        if int_slot >= max_int_slots {
            emitter.comment(&format!(
                "WARNING: Fiber capture {} dropped — start_args slot budget exhausted",
                capture_name
            ));
            break;
        }
        if consumes_two && int_slot + 1 >= max_int_slots {
            emitter.comment(&format!(
                "WARNING: Fiber string capture {} dropped — needs 2 register slots but only 1 remains",
                capture_name
            ));
            break;
        }
        let off_lo = start_args_offset + (int_slot as i32) * 8;
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x9, [sp]");                            // peek the saved Fiber pointer from the parking slot
                if consumes_two {
                    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                    let off_hi = start_args_offset + ((int_slot + 1) as i32) * 8;
                    emitter.instruction(&format!("str {}, [x9, #{}]", ptr_reg, off_lo)); // start_args[int_slot] = string ptr
                    emitter.instruction(&format!("str {}, [x9, #{}]", len_reg, off_hi)); // start_args[int_slot + 1] = string length
                    emitter.instruction(&format!("mov x0, {}", ptr_reg));       // x0 = string ptr for the incref helper (only the ptr half is heap-backed)
                    emitter.instruction("bl __rt_incref");                      // retain the captured string so it survives until the Fiber is freed
                } else {
                    emitter.instruction(&format!("str x0, [x9, #{}]", off_lo)); // start_args[int_slot] = single-register capture value
                    emitter.instruction("bl __rt_incref");                      // retain the captured heap pointer if any (no-op when x0 is not in the heap range)
                }
            }
            Arch::X86_64 => {
                emitter.instruction("mov rcx, QWORD PTR [rsp]");                // peek the saved Fiber pointer from the parking slot
                if consumes_two {
                    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                    let off_hi = start_args_offset + ((int_slot + 1) as i32) * 8;
                    emitter.instruction(&format!("mov QWORD PTR [rcx + {}], {}", off_lo, ptr_reg)); // start_args[int_slot] = string ptr
                    emitter.instruction(&format!("mov QWORD PTR [rcx + {}], {}", off_hi, len_reg)); // start_args[int_slot + 1] = string length
                    // x86_64 incref takes the pointer in rax; ptr_reg is already rax for the string case.
                    emitter.instruction("call __rt_incref");                    // retain the captured string so it survives until the Fiber is freed
                } else {
                    emitter.instruction(&format!("mov QWORD PTR [rcx + {}], rax", off_lo)); // start_args[int_slot] = single-register capture value
                    emitter.instruction("call __rt_incref");                    // retain the captured heap pointer if any (no-op when rax is not in the heap range)
                }
            }
        }
        int_slot += if consumes_two { 2 } else { 1 };
        // Silence dead-code warnings about user_param_count when there are no
        // remaining users below — kept around for future asserts.
        let _ = user_param_count;
    }
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the Fiber pointer as the expression result for the active target ABI

    // user_arg_max = user_param_count so the next $f->start() leaves the
    // pre-loaded captures alone. (We use user_param_count rather than `slot`
    // because user args always go into the first user_param_count slots, and
    // captures occupy the trailing range we just populated.)
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x9, #{}", user_param_count));     // load the closure's user-parameter count
            emitter.instruction(&format!("str x9, [x0, #{}]", umax_off));       // user_arg_max = user_param_count
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rcx, {}", user_param_count));     // load the closure's user-parameter count
            emitter.instruction(&format!("mov QWORD PTR [rax + {}], rcx", umax_off)); // user_arg_max = user_param_count
        }
    }
}
