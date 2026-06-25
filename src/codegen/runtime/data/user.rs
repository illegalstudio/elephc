//! Purpose:
//! Builds user-program runtime metadata as assembly text.
//! This owns class, interface, vtable, enum, static-property, and source-location tables generated from analysis.
//!
//! Called from:
//! - `crate::codegen::runtime::data::emit_runtime_data_user()`.
//!
//! Key details:
//! - User data is program-specific and must match class ids, static property slots, and generated call sites.

use std::collections::{HashMap, HashSet};

use crate::names::{
    enum_case_symbol, function_variant_active_symbol, interface_method_wrapper_symbol, mangle_fqn,
    method_symbol, php_symbol_key, static_method_symbol, static_property_symbol,
};
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, EnumInfo, FunctionSig, InterfaceInfo, PhpType};

use super::instanceof::{escaped_ascii, escaped_bytes};

const EVAL_REFLECTION_CLASS_FLAG_FINAL: u64 = 1;
const EVAL_REFLECTION_CLASS_FLAG_ABSTRACT: u64 = 2;
const EVAL_REFLECTION_CLASS_FLAG_READONLY: u64 = 32;
const EVAL_REFLECTION_PROPERTY_FLAG_STATIC: u64 = 1;
const EVAL_REFLECTION_PROPERTY_FLAG_PUBLIC: u64 = 2;
const EVAL_REFLECTION_PROPERTY_FLAG_PROTECTED: u64 = 4;
const EVAL_REFLECTION_PROPERTY_FLAG_PRIVATE: u64 = 8;
const EVAL_REFLECTION_PROPERTY_FLAG_FINAL: u64 = 16;
const EVAL_REFLECTION_PROPERTY_FLAG_ABSTRACT: u64 = 32;
const EVAL_REFLECTION_PROPERTY_FLAG_READONLY: u64 = 64;
const EVAL_REFLECTION_PROPERTY_FLAG_HAS_DEFAULT_VALUE: u64 = 256;
const EVAL_REFLECTION_PROPERTY_FLAG_PROMOTED: u64 = 512;
const EVAL_REFLECTION_PROPERTY_FLAG_PROTECTED_SET: u64 = 2048;
const EVAL_REFLECTION_PROPERTY_FLAG_PRIVATE_SET: u64 = 4096;
const EVAL_REFLECTION_METHOD_FLAG_STATIC: u64 = 1;
const EVAL_REFLECTION_METHOD_FLAG_PUBLIC: u64 = 2;
const EVAL_REFLECTION_METHOD_FLAG_PROTECTED: u64 = 4;
const EVAL_REFLECTION_METHOD_FLAG_PRIVATE: u64 = 8;
const EVAL_REFLECTION_METHOD_FLAG_FINAL: u64 = 16;
const EVAL_REFLECTION_METHOD_FLAG_ABSTRACT: u64 = 32;

/// Emit the user-dependent data section — globals, statics, class metadata.
/// This changes per program and cannot be cached.
pub(crate) fn emit_runtime_data_user(
    global_var_names: &HashSet<String>,
    static_vars: &HashMap<(String, String), PhpType>,
    functions: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
    interfaces: &HashMap<String, InterfaceInfo>,
    interface_names: &[String],
    trait_names: &[String],
    classes: &HashMap<String, ClassInfo>,
    enums: &HashMap<String, EnumInfo>,
    allowed_class_names: Option<&HashSet<String>>,
) -> String {
    let mut out = String::new();

    let mut sorted_globals: Vec<&String> = global_var_names.iter().collect();
    sorted_globals.sort();
    for name in sorted_globals {
        out.push_str(&format!(".comm _gvar_{}, 16, 3\n", name));
    }

    let mut sorted_statics: Vec<&(String, String)> = static_vars.keys().collect();
    sorted_statics.sort();
    for (func_name, var_name) in sorted_statics {
        out.push_str(&format!(
            ".comm _static_{}_{}, 16, 3\n",
            mangle_fqn(func_name),
            var_name
        ));
        out.push_str(&format!(
            ".comm _static_{}_{}_init, 8, 3\n",
            mangle_fqn(func_name),
            var_name
        ));
    }

    let mut static_property_symbols = HashSet::new();
    for (class_name, class_info) in classes {
        if allowed_class_names.is_some_and(|allowed| !allowed.contains(class_name)) {
            continue;
        }
        for (property_name, _) in &class_info.static_properties {
            let declaring_class = class_info
                .static_property_declaring_classes
                .get(property_name)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            static_property_symbols.insert(static_property_symbol(declaring_class, property_name));
        }
    }
    let mut static_property_symbols: Vec<String> = static_property_symbols.into_iter().collect();
    static_property_symbols.sort();
    for symbol in static_property_symbols {
        out.push_str(&format!(".comm {}, 16, 3\n", symbol));
    }

    let mut sorted_enum_names: Vec<&String> = enums.keys().collect();
    sorted_enum_names.sort();
    for enum_name in &sorted_enum_names {
        let Some(enum_info) = enums.get(*enum_name) else {
            continue;
        };
        for case in &enum_info.cases {
            out.push_str(&format!(
                ".comm {}, 8, 3\n",
                enum_case_symbol(*enum_name, &case.name)
            ));
        }
    }

    let mut sorted_interfaces: Vec<(&String, &InterfaceInfo)> = interfaces.iter().collect();
    sorted_interfaces.sort_by_key(|(_, interface_info)| interface_info.interface_id);
    let all_class_id_by_name: HashMap<String, u64> = classes
        .iter()
        .map(|(name, class_info)| (name.clone(), class_info.class_id))
        .collect();
    let mut sorted_classes: Vec<(&String, &ClassInfo)> = classes.iter().collect();
    if let Some(allowed_class_names) = allowed_class_names {
        sorted_classes.retain(|(class_name, _)| allowed_class_names.contains(*class_name));
    }
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);
    let class_id_by_name: HashMap<String, u64> = sorted_classes
        .iter()
        .map(|(name, class_info)| ((*name).clone(), class_info.class_id))
        .collect();
    let class_info_by_id: HashMap<u64, &ClassInfo> = sorted_classes
        .iter()
        .map(|(_, class_info)| (class_info.class_id, *class_info))
        .collect();
    let class_name_by_id: HashMap<u64, &String> = sorted_classes
        .iter()
        .map(|(name, class_info)| (class_info.class_id, *name))
        .collect();
    let max_class_id = sorted_classes.iter().map(|(_, class_info)| class_info.class_id).max();

    out.push_str(".data\n");
    out.push_str(".p2align 3\n");
    emit_callable_function_data(&mut out, functions, function_variant_groups);
    out.push_str(".p2align 3\n");
    super::instanceof::emit_instanceof_target_lookup_data(&mut out, &sorted_interfaces, &sorted_classes);
    emit_class_name_lookup_data(&mut out, max_class_id, &class_name_by_id);
    emit_name_lookup_data(
        &mut out,
        "_interface_names_count",
        "_interface_names",
        "_interface_name",
        interface_names,
    );
    emit_name_lookup_data(
        &mut out,
        "_trait_names_count",
        "_trait_names",
        "_trait_name",
        trait_names,
    );
    let enum_names: Vec<String> = sorted_enum_names.iter().map(|name| (*name).clone()).collect();
    emit_name_lookup_data(
        &mut out,
        "_enum_names_count",
        "_enum_names",
        "_enum_name",
        &enum_names,
    );

    // Per-program class id of the built-in `Fiber` class. The fiber runtime
    // checks this against the receiver's class_id in __rt_object_free_deep so
    // that a Fiber being garbage-collected releases its 256 KB stack instead
    // of leaking it. Defaults to u64::MAX when Fiber is not in scope (which
    // never happens in practice — Fiber is always injected as a built-in).
    let fiber_class_id = all_class_id_by_name
        .get("Fiber")
        .copied()
        .unwrap_or(u64::MAX);
    out.push_str(".globl _fiber_class_id\n_fiber_class_id:\n");
    out.push_str(&format!("    .quad {}\n", fiber_class_id));

    let fiber_error_class_id = all_class_id_by_name
        .get("FiberError")
        .copied()
        .unwrap_or(u64::MAX);
    out.push_str(".globl _fiber_error_class_id\n_fiber_error_class_id:\n");
    out.push_str(&format!("    .quad {}\n", fiber_error_class_id));

    let generator_class_id = all_class_id_by_name
        .get("Generator")
        .copied()
        .unwrap_or(u64::MAX);
    out.push_str(".globl _generator_class_id\n_generator_class_id:\n");
    out.push_str(&format!("    .quad {}\n", generator_class_id));

    for (symbol, class_name) in [
        ("_spl_dll_class_id", "SplDoublyLinkedList"),
        ("_spl_stack_class_id", "SplStack"),
        ("_spl_queue_class_id", "SplQueue"),
        ("_spl_fixed_array_class_id", "SplFixedArray"),
        ("_spl_logic_exception_class_id", "LogicException"),
        ("_spl_runtime_exception_class_id", "RuntimeException"),
        ("_spl_out_of_range_exception_class_id", "OutOfRangeException"),
        ("_spl_out_of_bounds_exception_class_id", "OutOfBoundsException"),
        ("_spl_invalid_argument_exception_class_id", "InvalidArgumentException"),
        ("_spl_type_error_class_id", "TypeError"),
        ("_spl_value_error_class_id", "ValueError"),
    ] {
        let class_id = all_class_id_by_name
            .get(class_name)
            .copied()
            .unwrap_or(u64::MAX);
        out.push_str(&format!(".globl {}\n{}:\n", symbol, symbol));
        out.push_str(&format!("    .quad {}\n", class_id));
    }

    out.push_str(".globl _interface_count\n_interface_count:\n");
    out.push_str(&format!("    .quad {}\n", sorted_interfaces.len()));
    out.push_str(".globl _interface_method_ptrs\n_interface_method_ptrs:\n");
    for (_, interface_info) in &sorted_interfaces {
        out.push_str(&format!(
            "    .quad _interface_methods_{}\n",
            interface_info.interface_id
        ));
    }

    out.push_str(".globl _class_interface_ptrs\n_class_interface_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_interfaces_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_interfaces_missing\n");
            }
        }
    }

    // Per-class JSON descriptor pointer table — used by __rt_json_encode_object
    // to walk public properties and dispatch JsonSerializable when present.
    out.push_str(".globl _class_json_desc_ptrs\n_class_json_desc_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_json_desc_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_json_desc_missing\n");
            }
        }
    }

    // JsonException's class_id is consulted by __rt_json_throw_error when
    // JSON_THROW_ON_ERROR is set — it allocates an instance of this class
    // and routes it through the normal exception machinery.
    let json_exception_class_id = classes
        .get("JsonException")
        .map(|info| info.class_id as i64)
        .unwrap_or(-1);
    out.push_str(&format!(
        ".globl _json_exception_class_id\n_json_exception_class_id:\n    .quad {}\n",
        json_exception_class_id,
    ));

    out.push_str(".globl _class_parent_ids\n_class_parent_ids:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let parent_id = class_info_by_id
                .get(&class_id)
                .and_then(|class_info| class_info.parent.as_ref())
                .and_then(|parent_name| class_id_by_name.get(parent_name))
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-1".to_string());
            out.push_str(&format!("    .quad {}\n", parent_id));
        }
    }

    out.push_str(".globl _class_gc_desc_count\n_class_gc_desc_count:\n");
    out.push_str(&format!(
        "    .quad {}\n",
        max_class_id.map_or(0, |class_id| class_id + 1)
    ));
    out.push_str(".globl _class_gc_desc_ptrs\n_class_gc_desc_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_gc_desc_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_gc_desc_missing\n");
            }
        }
    }

    out.push_str(".globl _class_vtable_ptrs\n_class_vtable_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_vtable_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_vtable_missing\n");
            }
        }
    }

    // Per-class destructor symbol table — consulted by __rt_call_object_destructor
    // (invoked at the top of __rt_object_free_deep) to run a class's PHP
    // __destruct before its storage is freed. Each entry resolves through the
    // implementing class so an inherited destructor dispatches to the ancestor's
    // emitted method symbol; `0` means the class and its ancestors declare no
    // __destruct, so no destructor call is made.
    out.push_str(".globl _class_destruct_count\n_class_destruct_count:\n");
    out.push_str(&format!(
        "    .quad {}\n",
        max_class_id.map_or(0, |class_id| class_id + 1)
    ));
    out.push_str(".globl _class_destruct_ptrs\n_class_destruct_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        let destruct_key = php_symbol_key("__destruct");
        for class_id in 0..=max_class_id {
            let entry = class_info_by_id
                .get(&class_id)
                .and_then(|class_info| class_info.method_impl_classes.get(&destruct_key))
                .map(|impl_class| method_symbol(impl_class, &destruct_key))
                .unwrap_or_else(|| "0".to_string());
            out.push_str(&format!("    .quad {}\n", entry));
        }
    }

    // _class_propinit_ptrs: dense class_id-indexed table of property-default
    // init thunks. Entry = _class_propinit_<id> when the class has any property
    // default, else 0 (null = nothing to init). __rt_new_by_name indexes this
    // by class_id and calls the thunk (when non-zero) after zeroing the object.
    // The has-default predicate MUST match property_init_thunks::class_needs_property_init.
    out.push_str(".globl _class_propinit_ptrs\n_class_propinit_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            match class_info_by_id.get(&class_id) {
                Some(class_info) if class_info.defaults.iter().any(|d| d.is_some()) => {
                    out.push_str(&format!("    .quad _class_propinit_{}\n", class_id));
                }
                _ => out.push_str("    .quad 0\n"),
            }
        }
    }

    out.push_str(".globl _class_static_vtable_ptrs\n_class_static_vtable_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_static_vtable_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_static_vtable_missing\n");
            }
        }
    }

    out.push_str(".globl _class_callable_method_ptrs\n_class_callable_method_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if class_info_by_id.contains_key(&class_id) {
                out.push_str(&format!("    .quad _class_callable_methods_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_callable_methods_missing\n");
            }
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _user_wrapper_vtable_ptrs\n_user_wrapper_vtable_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let class_publishes_wrapper_method = class_info_by_id
                .get(&class_id)
                .is_some_and(|class_info| class_has_user_wrapper_method(class_info));
            if class_publishes_wrapper_method {
                out.push_str(&format!("    .quad _user_wrapper_vtable_{}\n", class_id));
            } else {
                out.push_str("    .quad _user_wrapper_vtable_missing\n");
            }
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _user_filter_vtable_ptrs\n_user_filter_vtable_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let class_publishes_filter_method = class_info_by_id
                .get(&class_id)
                .is_some_and(|class_info| class_has_user_filter_method(class_info));
            if class_publishes_filter_method {
                out.push_str(&format!("    .quad _user_filter_vtable_{}\n", class_id));
            } else {
                out.push_str("    .quad _user_filter_vtable_missing\n");
            }
        }
    }

    out.push_str(".globl _class_interfaces_missing\n_class_interfaces_missing:\n");
    out.push_str("    .quad 0\n");
    out.push_str(".globl _class_gc_desc_missing\n_class_gc_desc_missing:\n");
    out.push_str("    .byte 0\n");
    // _class_json_desc_missing: zero flags, zero properties, no jsonSerialize.
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _class_json_desc_missing\n_class_json_desc_missing:\n");
    out.push_str("    .quad 0\n"); // flags
    out.push_str("    .quad 0\n"); // jsonSerialize target
    out.push_str("    .quad 0\n"); // public property count
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _class_vtable_missing\n_class_vtable_missing:\n");
    out.push_str("    .quad 0\n");
    out.push_str("    .p2align 3\n");
    out.push_str(
        ".globl _class_static_vtable_missing\n_class_static_vtable_missing:\n",
    );
    out.push_str("    .quad 0\n");
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _class_callable_methods_missing\n_class_callable_methods_missing:\n");
    out.push_str("    .quad 0\n");
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _user_wrapper_vtable_missing\n_user_wrapper_vtable_missing:\n");
    for _ in 0..USER_WRAPPER_VTABLE_SLOTS {
        out.push_str("    .quad 0\n");
    }
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _user_filter_vtable_missing\n_user_filter_vtable_missing:\n");
    for _ in 0..USER_FILTER_VTABLE_SLOTS {
        out.push_str("    .quad 0\n");
    }
    out.push_str(".p2align 3\n");
    emit_static_callable_method_data(&mut out, &sorted_classes);
    out.push_str(".p2align 3\n");
    emit_classes_by_name_table(&mut out, &sorted_classes);
    out.push_str(".p2align 3\n");
    emit_eval_reflection_method_lookup_data(&mut out, &sorted_classes);
    out.push_str(".p2align 3\n");
    emit_eval_reflection_property_lookup_data(&mut out, &sorted_classes);
    out.push_str(".p2align 3\n");
    emit_eval_reflection_class_lookup_data(&mut out, &sorted_classes);
    out.push_str(".p2align 3\n");
    emit_eval_reflection_class_interface_lookup_data(&mut out, &sorted_classes, interfaces);

    // -- class-level PHP 8 attribute metadata table --
    // Per-class layout: count followed by (name_ptr, name_len) pairs.
    // Top-level pointer table indexes by class_id.
    out.push_str(".p2align 3\n");
    out.push_str(".globl _class_attribute_count\n_class_attribute_count:\n");
    out.push_str(&format!(
        "    .quad {}\n",
        max_class_id.map_or(0, |class_id| class_id + 1)
    ));
    out.push_str(".globl _class_attribute_ptrs\n_class_attribute_ptrs:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let has_attrs = class_info_by_id
                .get(&class_id)
                .is_some_and(|info| !info.attribute_names.is_empty());
            if has_attrs {
                out.push_str(&format!("    .quad _class_attributes_{}\n", class_id));
            } else {
                out.push_str("    .quad _class_attributes_missing\n");
            }
        }
    }
    out.push_str(".globl _class_attributes_missing\n_class_attributes_missing:\n");
    out.push_str("    .quad 0\n"); // count = 0

    // Per-class attribute payloads. The per-class table holds 32-byte
    // entries: `(name_ptr, name_len, args_count, args_ptr)`. The args_ptr
    // points to a block of 24-byte tagged-arg entries — one per literal
    // argument captured at parse time. Each entry is laid out as
    // `(tag, lo, hi)` matching the runtime mixed-cell ABI:
    //
    //   tag 0 = int   (lo = i64 value,         hi = 0)
    //   tag 1 = str   (lo = .ascii label addr, hi = byte length)
    //   tag 2 = float (lo = f64 bits,          hi = 0)
    //   tag 3 = bool  (lo = 0 or 1,            hi = 0)
    //   tag 8 = null  (lo = 0,                 hi = 0)
    //
    // Unsupported args are represented as absent metadata by
    // `collect_attribute_args`; reflection helpers reject queries that would
    // need those payloads before codegen reaches this table. Other mixed-cell
    // payloads are reserved for future iterations.
    if let Some(max_class_id) = max_class_id {
        let mut name_id = 0u64;
        let mut arg_str_id = 0u64;
        let mut args_block_id = 0u64;
        for class_id in 0..=max_class_id {
            let Some(info) = class_info_by_id.get(&class_id) else {
                continue;
            };
            if info.attribute_names.is_empty() {
                continue;
            }
            let mut entries = Vec::with_capacity(info.attribute_names.len());
            for (idx, name) in info.attribute_names.iter().enumerate() {
                let name_label = format!("_attr_name_{}", name_id);
                name_id += 1;
                out.push_str(&format!(".globl {0}\n{0}:\n", name_label));
                out.push_str(&format!("    .ascii \"{}\"\n", escaped_ascii(name)));

                let empty_fallback = Vec::new();
                let args = info
                    .attribute_args
                    .get(idx)
                    .and_then(Option::as_ref)
                    .unwrap_or(&empty_fallback);
                let args_label = if args.is_empty() {
                    None
                } else {
                    // Intern any string-arg payload first so the per-arg
                    // table can reference it by label, then emit the tagged
                    // (tag, lo, hi) rows in source order.
                    let mut arg_rows = Vec::with_capacity(args.len());
                    for arg in args {
                        match arg.value() {
                            crate::types::AttrArgValue::Str(value) => {
                                let label = format!("_attr_arg_str_{}", arg_str_id);
                                arg_str_id += 1;
                                let bytes = crate::string_bytes::literal_bytes(value);
                                out.push_str(&format!(".globl {0}\n{0}:\n", label));
                                out.push_str(&format!(
                                    "    .ascii \"{}\"\n",
                                    escaped_bytes(&bytes)
                                ));
                                arg_rows.push((1u64, label, bytes.len() as u64));
                            }
                            crate::types::AttrArgValue::Int(value) => {
                                arg_rows.push((0u64, format!("{}", *value as u64), 0u64));
                            }
                            crate::types::AttrArgValue::Float(bits) => {
                                arg_rows.push((2u64, format!("{}", bits), 0u64));
                            }
                            crate::types::AttrArgValue::Bool(value) => {
                                arg_rows.push((3u64, format!("{}", *value as u64), 0u64));
                            }
                            crate::types::AttrArgValue::Null => {
                                arg_rows.push((8u64, "0".to_string(), 0u64));
                            }
                            crate::types::AttrArgValue::Named { .. } => {
                                unreachable!("named attribute arguments are unwrapped before runtime data emission")
                            }
                        }
                    }
                    out.push_str("    .p2align 3\n");
                    let block_label = format!("_attr_args_{}", args_block_id);
                    args_block_id += 1;
                    out.push_str(&format!(".globl {0}\n{0}:\n", block_label));
                    for (tag, lo, hi) in arg_rows {
                        out.push_str(&format!("    .quad {}\n", tag));
                        out.push_str(&format!("    .quad {}\n", lo));
                        out.push_str(&format!("    .quad {}\n", hi));
                    }
                    Some(block_label)
                };
                entries.push((name_label, name.len(), args.len(), args_label));
            }
            out.push_str("    .p2align 3\n");
            out.push_str(&format!(
                ".globl _class_attributes_{0}\n_class_attributes_{0}:\n",
                class_id
            ));
            out.push_str(&format!("    .quad {}\n", info.attribute_names.len()));
            for (name_label, name_len, args_count, args_label) in entries {
                out.push_str(&format!("    .quad {}\n", name_label));
                out.push_str(&format!("    .quad {}\n", name_len));
                out.push_str(&format!("    .quad {}\n", args_count));
                out.push_str(&format!(
                    "    .quad {}\n",
                    args_label.as_deref().unwrap_or("0")
                ));
            }
        }
    }

    for (_, interface_info) in &sorted_interfaces {
        out.push_str(&format!(
            ".globl _interface_methods_{}\n_interface_methods_{}:\n",
            interface_info.interface_id, interface_info.interface_id
        ));
        out.push_str(&format!("    .quad {}\n", interface_info.method_order.len()));
        for method_name in &interface_info.method_order {
            let slot = interface_info
                .method_slots
                .get(method_name)
                .expect("codegen bug: missing interface method slot");
            out.push_str(&format!("    .quad {}\n", slot));
        }
    }

    for (_, class_info) in sorted_classes {
        out.push_str(&format!(".globl _class_interfaces_{}\n_class_interfaces_{}:\n", class_info.class_id, class_info.class_id));
        out.push_str(&format!("    .quad {}\n", class_info.interfaces.len()));
        for interface_name in &class_info.interfaces {
            let interface_info = interfaces
                .get(interface_name)
                .expect("codegen bug: missing interface metadata for class");
            out.push_str(&format!("    .quad {}\n", interface_info.interface_id));
            out.push_str(&format!(
                "    .quad _class_interface_impl_{}_{}\n",
                class_info.class_id, interface_info.interface_id
            ));
        }

        for interface_name in &class_info.interfaces {
            let interface_info = interfaces
                .get(interface_name)
                .expect("codegen bug: missing interface metadata for class");
            out.push_str(&format!(
                ".globl _class_interface_impl_{}_{}\n_class_interface_impl_{}_{}:\n",
                class_info.class_id, interface_info.interface_id,
                class_info.class_id, interface_info.interface_id
            ));
            if interface_info.method_order.is_empty() {
                out.push_str("    .quad 0\n");
                continue;
            }
            for method_name in &interface_info.method_order {
                if let Some(impl_class) = class_info.method_impl_classes.get(method_name) {
                    let symbol = interface_method_table_symbol(
                        class_info,
                        interface_info,
                        method_name,
                        impl_class,
                        classes,
                    );
                    out.push_str(&format!("    .quad {}\n", symbol));
                } else {
                    out.push_str("    .quad 0\n");
                }
            }
        }

        // Per-property name strings used by the JSON descriptor below. We
        // emit them as labels before the descriptor so the descriptor
        // table holds plain (ptr, len) pairs.
        let public_props: Vec<(usize, &(String, PhpType))> = class_info
            .properties
            .iter()
            .enumerate()
            .filter(|(prop_index, (name, _))| {
                class_info
                    .visible_property_index(name)
                    .is_some_and(|visible_index| visible_index == *prop_index)
            })
            .filter(|(_, (name, _))| {
                class_info
                    .property_visibilities
                    .get(name)
                    .map_or(true, |v| matches!(v, Visibility::Public))
            })
            .collect();
        for (prop_index, (prop_name, _)) in &public_props {
            out.push_str(&format!(
                ".globl _class_json_pname_{}_{}\n_class_json_pname_{}_{}:\n    .ascii {:?}\n",
                class_info.class_id, prop_index, class_info.class_id, prop_index, prop_name,
            ));
        }
        out.push_str("    .p2align 3\n");
        out.push_str(&format!(
            ".globl _class_json_desc_{}\n_class_json_desc_{}:\n",
            class_info.class_id, class_info.class_id,
        ));
        let implements_jsonserializable = class_info
            .interfaces
            .iter()
            .any(|i| i == "JsonSerializable");
        let flags: u64 = if implements_jsonserializable { 1 } else { 0 };
        out.push_str(&format!("    .quad {}\n", flags));
        if implements_jsonserializable {
            let key = php_symbol_key("jsonSerialize");
            if let Some(impl_class) = class_info.method_impl_classes.get(&key) {
                out.push_str(&format!(
                    "    .quad {}\n",
                    method_symbol(impl_class, &key),
                ));
            } else {
                out.push_str("    .quad 0\n");
            }
        } else {
            out.push_str("    .quad 0\n");
        }
        out.push_str(&format!("    .quad {}\n", public_props.len()));
        for (prop_index, (prop_name, prop_ty)) in &public_props {
            let tag = if class_info.property_slot_is_reference(*prop_index, prop_name) {
                0
            } else {
                match prop_ty {
                    PhpType::Int => 0,
                    PhpType::Str => 1,
                    PhpType::Float => 2,
                    PhpType::Bool => 3,
                    PhpType::Array(_) => 4,
                    PhpType::AssocArray { .. } => 5,
                    PhpType::Object(_) => 6,
                    PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => 7,
                    PhpType::Resource(_) => 9,
                    PhpType::TaggedScalar => {
                        unreachable!("nullable scalar properties use the boxed Mixed representation")
                    }
                    PhpType::Callable
                    | PhpType::Pointer(_)
                    | PhpType::Buffer(_)
                    | PhpType::Packed(_)
                    | PhpType::Never
                    | PhpType::Void => 0,
                }
            };
            out.push_str(&format!(
                "    .quad _class_json_pname_{}_{}\n",
                class_info.class_id, prop_index,
            ));
            out.push_str(&format!("    .quad {}\n", prop_name.len()));
            out.push_str(&format!("    .quad {}\n", prop_index));
            out.push_str(&format!("    .quad {}\n", tag));
        }

        out.push_str("    .p2align 3\n");
        out.push_str(&format!(".globl _class_gc_desc_{}\n_class_gc_desc_{}:\n", class_info.class_id, class_info.class_id));
        if class_info.properties.is_empty() {
            out.push_str("    .byte 0\n");
        } else {
            out.push_str("    .byte ");
            for (i, (_, prop_ty)) in class_info.properties.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let prop_name = &class_info.properties[i].0;
                let tag = if class_info.property_slot_is_reference(i, prop_name) {
                    0
                } else {
                    match prop_ty {
                        PhpType::Int => 0,
                        PhpType::Str => 1,
                        PhpType::Float => 2,
                        PhpType::Bool => 3,
                        PhpType::Array(_) => 4,
                        PhpType::AssocArray { .. } => 5,
                        PhpType::Object(_) => 6,
                        PhpType::Mixed => 7,
                        PhpType::Union(_) => 7,
                        PhpType::Iterable => 7,
                        PhpType::Resource(_) => 9,
                        PhpType::TaggedScalar => {
                            unreachable!("nullable scalar properties use the boxed Mixed representation")
                        }
                        PhpType::Callable
                        | PhpType::Pointer(_)
                        | PhpType::Buffer(_)
                        | PhpType::Packed(_)
                        | PhpType::Never
                        | PhpType::Void => 0,
                    }
                };
                out.push_str(&tag.to_string());
            }
            out.push('\n');
        }

        out.push_str("    .p2align 3\n");
        out.push_str(&format!(".globl _class_vtable_{}\n_class_vtable_{}:\n", class_info.class_id, class_info.class_id));
        if class_info.vtable_methods.is_empty() {
            out.push_str("    .quad 0\n");
        } else {
            for method_name in &class_info.vtable_methods {
                if let Some(impl_class) = class_info.method_impl_classes.get(method_name) {
                    out.push_str(&format!("    .quad {}\n", method_symbol(impl_class, method_name)));
                } else {
                    out.push_str("    .quad 0\n");
                }
            }
        }

        out.push_str("    .p2align 3\n");
        out.push_str(&format!(".globl _class_static_vtable_{}\n_class_static_vtable_{}:\n", class_info.class_id, class_info.class_id));
        if class_info.static_vtable_methods.is_empty() {
            out.push_str("    .quad 0\n");
        } else {
            for method_name in &class_info.static_vtable_methods {
                if let Some(impl_class) = class_info.static_method_impl_classes.get(method_name) {
                    out.push_str(&format!("    .quad {}\n", static_method_symbol(impl_class, method_name)));
                } else {
                    out.push_str("    .quad 0\n");
                }
            }
        }

        emit_class_callable_methods(&mut out, class_info);
        emit_user_wrapper_vtable(&mut out, class_info);
        emit_user_filter_vtable(&mut out, class_info);
    }

    let stdclass_id = classes
        .get("stdClass")
        .map(|class_info| class_info.class_id as i64)
        .unwrap_or(-1);
    out.push_str(".p2align 3\n");
    out.push_str(".globl _stdclass_class_id\n_stdclass_class_id:\n");
    out.push_str(&format!("    .quad {}\n", stdclass_id));

    out
}

/// Emits a dense class-id to class-name lookup table for runtime `get_class()`.
///
/// Each `_class_name_entries` row is two words: `(name_ptr, name_len)`. Missing
/// class ids point at `_class_name_missing` with length zero so runtime lookups
/// can fail to an empty string without branching into undefined labels.
fn emit_class_name_lookup_data(
    out: &mut String,
    max_class_id: Option<u64>,
    class_name_by_id: &HashMap<u64, &String>,
) {
    out.push_str(".p2align 3\n");
    out.push_str(".globl _class_name_count\n_class_name_count:\n");
    out.push_str(&format!(
        "    .quad {}\n",
        max_class_id.map_or(0, |class_id| class_id + 1)
    ));
    out.push_str(".globl _class_name_entries\n_class_name_entries:\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            if let Some(class_name) = class_name_by_id.get(&class_id) {
                out.push_str(&format!("    .quad _class_name_{}\n", class_id));
                out.push_str(&format!("    .quad {}\n", class_name.len()));
            } else {
                out.push_str("    .quad _class_name_missing\n");
                out.push_str("    .quad 0\n");
            }
        }
    }
    out.push_str(".globl _class_name_missing\n_class_name_missing:\n");
    out.push_str("    .byte 0\n");
    if let Some(max_class_id) = max_class_id {
        for class_id in 0..=max_class_id {
            let Some(class_name) = class_name_by_id.get(&class_id) else {
                continue;
            };
            out.push_str(&format!(
                ".globl _class_name_{0}\n_class_name_{0}:\n",
                class_id
            ));
            out.push_str(&format!("    .ascii \"{}\"\n", escaped_ascii(class_name)));
        }
    }
    out.push_str("    .p2align 3\n");
}

/// Emits a compact `(name_ptr, name_len)` table for runtime class-like name probes.
fn emit_name_lookup_data(
    out: &mut String,
    count_symbol: &str,
    table_symbol: &str,
    label_prefix: &str,
    names: &[String],
) {
    let mut sorted_names: Vec<&String> = names.iter().collect();
    sorted_names.sort();
    for (idx, name) in sorted_names.iter().enumerate() {
        out.push_str(&format!(
            ".globl {0}_{1}\n{0}_{1}:\n    .ascii \"{2}\"\n",
            label_prefix,
            idx,
            escaped_ascii(name)
        ));
    }
    out.push_str(".p2align 3\n");
    out.push_str(&format!(".globl {0}\n{0}:\n", count_symbol));
    out.push_str(&format!("    .quad {}\n", sorted_names.len()));
    out.push_str(&format!(".globl {0}\n{0}:\n", table_symbol));
    for (idx, name) in sorted_names.iter().enumerate() {
        out.push_str(&format!("    .quad {}_{}\n", label_prefix, idx));
        out.push_str(&format!("    .quad {}\n", name.len()));
    }
}

/// Emits AOT method flag rows consumed by eval ReflectionMethod metadata probes.
fn emit_eval_reflection_method_lookup_data(
    out: &mut String,
    sorted_classes: &[(&String, &ClassInfo)],
) {
    let mut entries = Vec::new();
    let mut index = 0usize;
    for (class_name, class_info) in sorted_classes {
        let mut methods = class_info.methods.keys().collect::<Vec<_>>();
        methods.sort();
        for method_name in methods {
            let flags = eval_reflection_instance_method_flags(class_info, method_name);
            let class_label = format!("_eval_reflection_method_class_{}", index);
            let method_label = format!("_eval_reflection_method_name_{}", index);
            let declaring_class = eval_reflection_instance_method_declaring_class(
                class_name,
                class_info,
                method_name,
            );
            let declaring_label = format!("_eval_reflection_method_declaring_class_{}", index);
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                class_label,
                escaped_ascii(class_name)
            ));
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                method_label,
                escaped_ascii(method_name)
            ));
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                declaring_label,
                escaped_ascii(declaring_class)
            ));
            entries.push((
                class_label,
                class_name.len(),
                method_label,
                method_name.len(),
                flags,
                declaring_label,
                declaring_class.len(),
            ));
            index += 1;
        }

        let mut static_methods = class_info.static_methods.keys().collect::<Vec<_>>();
        static_methods.sort();
        for method_name in static_methods {
            let flags = eval_reflection_static_method_flags(class_info, method_name);
            let class_label = format!("_eval_reflection_method_class_{}", index);
            let method_label = format!("_eval_reflection_method_name_{}", index);
            let declaring_class =
                eval_reflection_static_method_declaring_class(class_name, class_info, method_name);
            let declaring_label = format!("_eval_reflection_method_declaring_class_{}", index);
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                class_label,
                escaped_ascii(class_name)
            ));
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                method_label,
                escaped_ascii(method_name)
            ));
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                declaring_label,
                escaped_ascii(declaring_class)
            ));
            entries.push((
                class_label,
                class_name.len(),
                method_label,
                method_name.len(),
                flags,
                declaring_label,
                declaring_class.len(),
            ));
            index += 1;
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _eval_reflection_method_count\n_eval_reflection_method_count:\n");
    out.push_str(&format!("    .quad {}\n", entries.len()));
    out.push_str(".globl _eval_reflection_methods\n_eval_reflection_methods:\n");
    for (class_label, class_len, method_label, method_len, flags, declaring_label, declaring_len) in
        entries
    {
        out.push_str(&format!("    .quad {}\n", class_label));
        out.push_str(&format!("    .quad {}\n", class_len));
        out.push_str(&format!("    .quad {}\n", method_label));
        out.push_str(&format!("    .quad {}\n", method_len));
        out.push_str(&format!("    .quad {}\n", flags));
        out.push_str(&format!("    .quad {}\n", declaring_label));
        out.push_str(&format!("    .quad {}\n", declaring_len));
    }
}

/// Returns the class name that declares one visible instance method.
fn eval_reflection_instance_method_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    method_name: &str,
) -> &'a str {
    class_info
        .method_impl_classes
        .get(method_name)
        .or_else(|| class_info.method_declaring_classes.get(method_name))
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Returns the class name that declares one visible static method.
fn eval_reflection_static_method_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    method_name: &str,
) -> &'a str {
    class_info
        .static_method_impl_classes
        .get(method_name)
        .or_else(|| class_info.static_method_declaring_classes.get(method_name))
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Returns eval ReflectionMethod bitflags for one instance method entry.
fn eval_reflection_instance_method_flags(class_info: &ClassInfo, method_name: &str) -> u64 {
    let visibility = class_info
        .method_visibilities
        .get(method_name)
        .unwrap_or(&Visibility::Public);
    let mut flags = eval_reflection_method_visibility_flags(visibility);
    if class_info.final_methods.contains(method_name) {
        flags |= EVAL_REFLECTION_METHOD_FLAG_FINAL;
    }
    if !class_info.method_impl_classes.contains_key(method_name) {
        flags |= EVAL_REFLECTION_METHOD_FLAG_ABSTRACT;
    }
    flags
}

/// Returns eval ReflectionMethod bitflags for one static method entry.
fn eval_reflection_static_method_flags(class_info: &ClassInfo, method_name: &str) -> u64 {
    let visibility = class_info
        .static_method_visibilities
        .get(method_name)
        .unwrap_or(&Visibility::Public);
    let mut flags =
        EVAL_REFLECTION_METHOD_FLAG_STATIC | eval_reflection_method_visibility_flags(visibility);
    if class_info.final_static_methods.contains(method_name) {
        flags |= EVAL_REFLECTION_METHOD_FLAG_FINAL;
    }
    if !class_info.static_method_impl_classes.contains_key(method_name) {
        flags |= EVAL_REFLECTION_METHOD_FLAG_ABSTRACT;
    }
    flags
}

/// Converts method visibility metadata into eval ReflectionMethod flag bits.
fn eval_reflection_method_visibility_flags(visibility: &Visibility) -> u64 {
    match visibility {
        Visibility::Public => EVAL_REFLECTION_METHOD_FLAG_PUBLIC,
        Visibility::Protected => EVAL_REFLECTION_METHOD_FLAG_PROTECTED,
        Visibility::Private => EVAL_REFLECTION_METHOD_FLAG_PRIVATE,
    }
}

/// Emits AOT property flag rows consumed by eval ReflectionProperty metadata probes.
fn emit_eval_reflection_property_lookup_data(
    out: &mut String,
    sorted_classes: &[(&String, &ClassInfo)],
) {
    let mut entries = Vec::new();
    let mut index = 0usize;
    for (class_name, class_info) in sorted_classes {
        for (slot, (property_name, _)) in class_info.properties.iter().enumerate() {
            let flags = eval_reflection_instance_property_flags(class_info, slot, property_name);
            let class_label = format!("_eval_reflection_property_class_{}", index);
            let property_label = format!("_eval_reflection_property_name_{}", index);
            let declaring_class = eval_reflection_instance_property_declaring_class(
                class_name,
                class_info,
                property_name,
            );
            let declaring_label = format!("_eval_reflection_property_declaring_class_{}", index);
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                class_label,
                escaped_ascii(class_name)
            ));
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                property_label,
                escaped_ascii(property_name)
            ));
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                declaring_label,
                escaped_ascii(declaring_class)
            ));
            entries.push((
                class_label,
                class_name.len(),
                property_label,
                property_name.len(),
                flags,
                declaring_label,
                declaring_class.len(),
            ));
            index += 1;
        }
        for (slot, (property_name, _)) in class_info.static_properties.iter().enumerate() {
            let flags = eval_reflection_static_property_flags(class_info, slot, property_name);
            let class_label = format!("_eval_reflection_property_class_{}", index);
            let property_label = format!("_eval_reflection_property_name_{}", index);
            let declaring_class = eval_reflection_static_property_declaring_class(
                class_name,
                class_info,
                property_name,
            );
            let declaring_label = format!("_eval_reflection_property_declaring_class_{}", index);
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                class_label,
                escaped_ascii(class_name)
            ));
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                property_label,
                escaped_ascii(property_name)
            ));
            out.push_str(&format!(
                ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
                declaring_label,
                escaped_ascii(declaring_class)
            ));
            entries.push((
                class_label,
                class_name.len(),
                property_label,
                property_name.len(),
                flags,
                declaring_label,
                declaring_class.len(),
            ));
            index += 1;
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _eval_reflection_property_count\n_eval_reflection_property_count:\n");
    out.push_str(&format!("    .quad {}\n", entries.len()));
    out.push_str(".globl _eval_reflection_properties\n_eval_reflection_properties:\n");
    for (class_label, class_len, property_label, property_len, flags, declaring_label, declaring_len) in
        entries
    {
        out.push_str(&format!("    .quad {}\n", class_label));
        out.push_str(&format!("    .quad {}\n", class_len));
        out.push_str(&format!("    .quad {}\n", property_label));
        out.push_str(&format!("    .quad {}\n", property_len));
        out.push_str(&format!("    .quad {}\n", flags));
        out.push_str(&format!("    .quad {}\n", declaring_label));
        out.push_str(&format!("    .quad {}\n", declaring_len));
    }
}

/// Returns the class name that declares one visible instance property.
fn eval_reflection_instance_property_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    property_name: &str,
) -> &'a str {
    class_info
        .property_declaring_classes
        .get(property_name)
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Returns the class name that declares one visible static property.
fn eval_reflection_static_property_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    property_name: &str,
) -> &'a str {
    class_info
        .static_property_declaring_classes
        .get(property_name)
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Emits AOT class flag rows consumed by eval ReflectionClass metadata probes.
fn emit_eval_reflection_class_lookup_data(
    out: &mut String,
    sorted_classes: &[(&String, &ClassInfo)],
) {
    let mut entries = Vec::new();
    let mut index = 0usize;
    for (class_name, class_info) in sorted_classes {
        let flags = eval_reflection_class_flags(class_info);
        if flags == 0 {
            continue;
        }
        let class_label = format!("_eval_reflection_class_name_{}", index);
        out.push_str(&format!(
            ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
            class_label,
            escaped_ascii(class_name)
        ));
        entries.push((class_label, class_name.len(), flags));
        index += 1;
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _eval_reflection_class_count\n_eval_reflection_class_count:\n");
    out.push_str(&format!("    .quad {}\n", entries.len()));
    out.push_str(".globl _eval_reflection_classes\n_eval_reflection_classes:\n");
    for (class_label, class_len, flags) in entries {
        out.push_str(&format!("    .quad {}\n", class_label));
        out.push_str(&format!("    .quad {}\n", class_len));
        out.push_str(&format!("    .quad {}\n", flags));
    }
}

/// Returns eval ReflectionClass flag bits retained for one generated/AOT class.
fn eval_reflection_class_flags(class_info: &ClassInfo) -> u64 {
    let mut flags = 0;
    if class_info.is_final {
        flags |= EVAL_REFLECTION_CLASS_FLAG_FINAL;
    }
    if class_info.is_abstract {
        flags |= EVAL_REFLECTION_CLASS_FLAG_ABSTRACT;
    }
    if class_info.is_readonly_class {
        flags |= EVAL_REFLECTION_CLASS_FLAG_READONLY;
    }
    flags
}

/// Emits class-like/interface-name rows consumed by eval ReflectionClass metadata probes.
fn emit_eval_reflection_class_interface_lookup_data(
    out: &mut String,
    sorted_classes: &[(&String, &ClassInfo)],
    interfaces: &HashMap<String, InterfaceInfo>,
) {
    let mut entries = Vec::new();
    let mut index = 0usize;
    for (class_name, class_info) in sorted_classes {
        for interface_name in &class_info.interfaces {
            push_eval_reflection_class_interface_row(
                out,
                &mut entries,
                &mut index,
                class_name,
                interface_name,
            );
        }
    }

    let mut sorted_interfaces: Vec<&String> = interfaces.keys().collect();
    sorted_interfaces.sort();
    for interface_name in sorted_interfaces {
        for parent_name in eval_reflection_interface_parent_names(interface_name, interfaces) {
            push_eval_reflection_class_interface_row(
                out,
                &mut entries,
                &mut index,
                interface_name,
                &parent_name,
            );
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(
        ".globl _eval_reflection_class_interface_count\n_eval_reflection_class_interface_count:\n",
    );
    out.push_str(&format!("    .quad {}\n", entries.len()));
    out.push_str(".globl _eval_reflection_class_interfaces\n_eval_reflection_class_interfaces:\n");
    for (class_label, class_len, interface_label, interface_len) in entries {
        out.push_str(&format!("    .quad {}\n", class_label));
        out.push_str(&format!("    .quad {}\n", class_len));
        out.push_str(&format!("    .quad {}\n", interface_label));
        out.push_str(&format!("    .quad {}\n", interface_len));
    }
}

/// Adds one class-like/interface-name row and its backing string labels.
fn push_eval_reflection_class_interface_row(
    out: &mut String,
    entries: &mut Vec<(String, usize, String, usize)>,
    index: &mut usize,
    class_name: &str,
    interface_name: &str,
) {
    let class_label = format!("_eval_reflection_class_interface_class_{}", *index);
    let interface_label = format!("_eval_reflection_class_interface_name_{}", *index);
    out.push_str(&format!(
        ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
        class_label,
        escaped_ascii(class_name)
    ));
    out.push_str(&format!(
        ".globl {0}\n{0}:\n    .ascii \"{1}\"\n",
        interface_label,
        escaped_ascii(interface_name)
    ));
    entries.push((
        class_label,
        class_name.len(),
        interface_label,
        interface_name.len(),
    ));
    *index += 1;
}

/// Returns direct and inherited parent interface names for one generated interface.
fn eval_reflection_interface_parent_names(
    interface_name: &str,
    interfaces: &HashMap<String, InterfaceInfo>,
) -> Vec<String> {
    let mut names = Vec::new();
    collect_eval_reflection_interface_parent_names(interface_name, interfaces, &mut names);
    names
}

/// Recursively appends interface parents without duplicating case-insensitive names.
fn collect_eval_reflection_interface_parent_names(
    interface_name: &str,
    interfaces: &HashMap<String, InterfaceInfo>,
    names: &mut Vec<String>,
) {
    let Some((_, interface_info)) = eval_reflection_interface_entry(interface_name, interfaces)
    else {
        return;
    };
    for parent in &interface_info.parents {
        let parent_name = eval_reflection_interface_entry(parent, interfaces)
            .map(|(name, _)| name.to_string())
            .unwrap_or_else(|| parent.clone());
        if names
            .iter()
            .any(|name| php_symbol_key(name) == php_symbol_key(&parent_name))
        {
            continue;
        }
        names.push(parent_name.clone());
        collect_eval_reflection_interface_parent_names(&parent_name, interfaces, names);
    }
}

/// Returns the canonical generated interface entry for a possibly case-varied name.
fn eval_reflection_interface_entry<'a>(
    interface_name: &str,
    interfaces: &'a HashMap<String, InterfaceInfo>,
) -> Option<(&'a str, &'a InterfaceInfo)> {
    if let Some((name, info)) = interfaces.get_key_value(interface_name) {
        return Some((name.as_str(), info));
    }
    interfaces
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(interface_name))
        .map(|(name, info)| (name.as_str(), info))
}

/// Returns eval ReflectionProperty bitflags for one instance property slot.
fn eval_reflection_instance_property_flags(
    class_info: &ClassInfo,
    slot: usize,
    property_name: &str,
) -> u64 {
    let visibility = class_info
        .property_visibilities
        .get(property_name)
        .unwrap_or(&Visibility::Public);
    let mut flags = eval_reflection_visibility_flags(visibility);
    if class_info.final_properties.contains(property_name) {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_FINAL;
    }
    if class_info.abstract_properties.contains(property_name) {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_ABSTRACT;
    }
    if class_info.readonly_properties.contains(property_name) {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_READONLY;
    }
    if class_info.promoted_properties.contains(property_name) {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_PROMOTED;
    }
    match class_info.property_set_visibilities.get(property_name) {
        Some(Visibility::Protected) => flags |= EVAL_REFLECTION_PROPERTY_FLAG_PROTECTED_SET,
        Some(Visibility::Private) => flags |= EVAL_REFLECTION_PROPERTY_FLAG_PRIVATE_SET,
        Some(Visibility::Public) | None => {}
    }
    if class_info.defaults.get(slot).is_some_and(Option::is_some) {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_HAS_DEFAULT_VALUE;
    }
    flags
}

/// Returns eval ReflectionProperty bitflags for one static property slot.
fn eval_reflection_static_property_flags(
    class_info: &ClassInfo,
    slot: usize,
    property_name: &str,
) -> u64 {
    let visibility = class_info
        .static_property_visibilities
        .get(property_name)
        .unwrap_or(&Visibility::Public);
    let mut flags =
        EVAL_REFLECTION_PROPERTY_FLAG_STATIC | eval_reflection_visibility_flags(visibility);
    if class_info.final_static_properties.contains(property_name) {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_FINAL;
    }
    if class_info
        .static_defaults
        .get(slot)
        .is_some_and(Option::is_some)
    {
        flags |= EVAL_REFLECTION_PROPERTY_FLAG_HAS_DEFAULT_VALUE;
    }
    flags
}

/// Converts a property visibility into eval ReflectionProperty bitflags.
fn eval_reflection_visibility_flags(visibility: &Visibility) -> u64 {
    match visibility {
        Visibility::Public => EVAL_REFLECTION_PROPERTY_FLAG_PUBLIC,
        Visibility::Protected => EVAL_REFLECTION_PROPERTY_FLAG_PROTECTED,
        Visibility::Private => EVAL_REFLECTION_PROPERTY_FLAG_PRIVATE,
    }
}

/// Emits the callable-function name table and pointer table for user-defined functions.
/// Each function name is emitted as an ASCII label; the pointer table references
/// either the active variant symbol for polymorphic functions or zero.
fn emit_callable_function_data(
    out: &mut String,
    functions: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
) {
    let mut sorted_functions: Vec<&String> = functions.keys().collect();
    sorted_functions.sort();
    for (idx, name) in sorted_functions.iter().enumerate() {
        out.push_str(&format!(
            ".globl _callable_user_fn_name_{0}\n_callable_user_fn_name_{0}:\n    .ascii \"{1}\"\n",
            idx,
            escaped_ascii(name)
        ));
    }
    out.push_str(".p2align 3\n");
    out.push_str(".globl _callable_user_function_count\n_callable_user_function_count:\n");
    out.push_str(&format!("    .quad {}\n", sorted_functions.len()));
    out.push_str(".globl _callable_user_function_table\n_callable_user_function_table:\n");
    for (idx, name) in sorted_functions.iter().enumerate() {
        out.push_str(&format!("    .quad _callable_user_fn_name_{}\n", idx));
        out.push_str(&format!("    .quad {}\n", name.len()));
        if function_variant_groups.contains(name.as_str()) {
            out.push_str(&format!(
                "    .quad {}\n",
                function_variant_active_symbol(name)
            ));
        } else {
            out.push_str("    .quad 0\n");
        }
    }
}

/// Emits the `_classes_by_name` lookup table used by `__rt_new_by_name`
/// for `new $variable()` dynamic instantiation (Phase 10 user-wrapper
/// dispatch). Each registered class contributes a 32-byte entry:
///
///   [0..8)   name_ptr   — pointer to the class-name ASCII bytes
///   [8..16)  name_len   — count of name bytes
///   [16..24) class_id   — runtime class id (matches the static
///                         `class_info.class_id` stamped by
///                         `__rt_heap_alloc` callers)
///   [24..32) obj_size   — `8 + num_props*16 + dyn_props_slot`, the same
///                         allocation size emit_new_object_core uses
///
/// The accompanying `_classes_by_name_count` symbol holds the entry count
/// so the runtime helper can bound its linear scan.
fn emit_classes_by_name_table(
    out: &mut String,
    sorted_classes: &[(&String, &ClassInfo)],
) {
    for (class_name, class_info) in sorted_classes {
        out.push_str(&format!(
            ".globl _class_by_name_str_{0}\n_class_by_name_str_{0}:\n    .ascii \"{1}\"\n",
            class_info.class_id,
            escaped_ascii(class_name)
        ));
    }
    out.push_str(".p2align 3\n");
    out.push_str(".globl _classes_by_name_count\n_classes_by_name_count:\n");
    out.push_str(&format!("    .quad {}\n", sorted_classes.len()));
    out.push_str(".globl _classes_by_name\n_classes_by_name:\n");
    for (class_name, class_info) in sorted_classes {
        let num_props = class_info.properties.len();
        let dyn_props_slot = if class_info.allow_dynamic_properties {
            8
        } else {
            0
        };
        let obj_size = 8 + num_props * 16 + dyn_props_slot;
        out.push_str(&format!(
            "    .quad _class_by_name_str_{}\n",
            class_info.class_id
        ));
        out.push_str(&format!("    .quad {}\n", class_name.len()));
        out.push_str(&format!("    .quad {}\n", class_info.class_id));
        out.push_str(&format!("    .quad {}\n", obj_size));
    }
}

/// The number of fixed-slot stream-wrapper methods recorded per class in
/// `_user_wrapper_vtable_<class_id>`. Slot order matches the runtime fopen
/// dispatch (Phase 10): 0 stream_open, 1 stream_close, 2 stream_read,
/// 3 stream_write, 4 stream_eof, 5 stream_tell, 6 stream_seek, 7 stream_flush,
/// 8 stream_stat (fd-based `fstat()` on an open wrapper stream), 9 url_stat
/// (path-based `file_exists()`/`is_file()`/`filesize()` on a `scheme://` URL).
/// G1 reserves the full PHP `StreamWrapper` surface so slot indices stay stable
/// as the dispatch is filled in: 10 stream_cast, 11 stream_lock (`flock()`),
/// 12 stream_truncate (`ftruncate()`), 13 stream_set_option, 14 stream_metadata,
/// 15 unlink, 16 rename, 17 mkdir, 18 rmdir, 19 dir_opendir, 20 dir_readdir,
/// 21 dir_closedir, 22 dir_rewinddir. Slots whose dispatch is not yet wired are
/// still emitted (zero when the class does not declare the method); the runtime
/// only reaches a slot when the corresponding builtin routes to it.
/// Each slot is either a method-symbol pointer (when the class declares the
/// method publicly) or zero. The stat methods must be declared WITHOUT a
/// return type (or `: mixed`) so their associative stat array round-trips as a
/// boxed Mixed cell — a `: array` return is integer-keyed and rejects the
/// string keys (`size`, `mode`, ...) PHP stat arrays use.
pub(crate) const USER_WRAPPER_VTABLE_SLOTS: usize = 23;

/// The number of fixed-slot stream-filter methods recorded per class in
/// `_user_filter_vtable_<class_id>` (Phase 10 tier 3). Slot order:
/// 0 filter, 1 onCreate, 2 onClose. Slot 3 is a non-method "arity" flag:
/// 0 = elephc-simplified `filter(string $data): string`, 1 = PHP-canonical
/// `filter($in, $out, &$consumed, $closing): int` with bucket brigades.
/// Slot 4 is a non-method byte offset for `php_user_filter::$params`, or zero
/// when the class has no statically declared params property.
/// The flag is read by the runtime dispatcher to choose which code path
/// to invoke. Adding the flag inline in the vtable lets the dispatcher
/// branch with a single load + cmp.
pub(crate) const USER_FILTER_VTABLE_SLOTS: usize = 5;

const USER_FILTER_METHOD_NAMES: [&str; 3] = [
    "filter",
    "oncreate",
    "onclose",
];

const USER_WRAPPER_METHOD_NAMES: [&str; USER_WRAPPER_VTABLE_SLOTS] = [
    "stream_open",
    "stream_close",
    "stream_read",
    "stream_write",
    "stream_eof",
    "stream_tell",
    "stream_seek",
    "stream_flush",
    "stream_stat",
    "url_stat",
    "stream_cast",
    "stream_lock",
    "stream_truncate",
    "stream_set_option",
    "stream_metadata",
    "unlink",
    "rename",
    "mkdir",
    "rmdir",
    "dir_opendir",
    "dir_readdir",
    "dir_closedir",
    "dir_rewinddir",
];

/// Returns true when a class publishes at least one of the eight
/// stream-wrapper methods publicly — i.e. when it is plausibly a stream
/// wrapper. Classes that miss this filter share `_user_wrapper_vtable_missing`
/// (all zeros) instead of emitting their own all-zero table.
fn class_has_user_wrapper_method(class_info: &ClassInfo) -> bool {
    USER_WRAPPER_METHOD_NAMES.iter().any(|method_name| {
        let is_public = class_info
            .method_visibilities
            .get(*method_name)
            .is_some_and(|visibility| matches!(visibility, Visibility::Public));
        let has_impl = class_info.method_impl_classes.contains_key(*method_name);
        is_public && has_impl
    })
}

/// Returns true when a class publishes at least one of the three
/// stream-filter methods publicly (filter / onCreate / onClose). Classes
/// that miss this filter share `_user_filter_vtable_missing` instead of
/// emitting their own all-zero table.
fn class_has_user_filter_method(class_info: &ClassInfo) -> bool {
    USER_FILTER_METHOD_NAMES.iter().any(|method_name| {
        let is_public = class_info
            .method_visibilities
            .get(*method_name)
            .is_some_and(|visibility| matches!(visibility, Visibility::Public));
        let has_impl = class_info.method_impl_classes.contains_key(*method_name);
        is_public && has_impl
    })
}

/// Emits runtime metadata for user filter vtable.
fn emit_user_filter_vtable(out: &mut String, class_info: &ClassInfo) {
    if !class_has_user_filter_method(class_info) {
        return;
    }
    out.push_str("    .p2align 3\n");
    out.push_str(&format!(
        ".globl _user_filter_vtable_{0}\n_user_filter_vtable_{0}:\n",
        class_info.class_id
    ));
    for method_name in &USER_FILTER_METHOD_NAMES {
        let is_public = class_info
            .method_visibilities
            .get(*method_name)
            .is_some_and(|visibility| matches!(visibility, Visibility::Public));
        let impl_class = class_info.method_impl_classes.get(*method_name);
        if is_public && impl_class.is_some() {
            out.push_str(&format!(
                "    .quad {}\n",
                method_symbol(impl_class.unwrap(), method_name)
            ));
        } else {
            out.push_str("    .quad 0\n");
        }
    }
    // -- slot 3: filter()-arity flag (0 = 1-arg string contract, 1 = 4-arg brigade)
    // The arity is detected by counting the visible parameters of filter() when
    // it lives on this class. 4 params → PHP-canonical
    // filter($in, $out, &$consumed, $closing): int. Anything else → 1-arg.
    let brigade_arity = class_info
        .methods
        .get("filter")
        .map(|sig| sig.params.len() == 4)
        .unwrap_or(false);
    out.push_str(&format!("    .quad {}\n", if brigade_arity { 1 } else { 0 }));
    let params_offset = class_info
        .property_offsets
        .get("params")
        .copied()
        .unwrap_or(0);
    out.push_str(&format!("    .quad {}\n", params_offset));
}

/// Emits runtime metadata for user wrapper vtable.
fn emit_user_wrapper_vtable(out: &mut String, class_info: &ClassInfo) {
    if !class_has_user_wrapper_method(class_info) {
        return;
    }
    out.push_str("    .p2align 3\n");
    out.push_str(&format!(
        ".globl _user_wrapper_vtable_{0}\n_user_wrapper_vtable_{0}:\n",
        class_info.class_id
    ));
    for method_name in &USER_WRAPPER_METHOD_NAMES {
        let is_public = class_info
            .method_visibilities
            .get(*method_name)
            .is_some_and(|visibility| matches!(visibility, Visibility::Public));
        let impl_class = class_info.method_impl_classes.get(*method_name);
        if is_public && impl_class.is_some() {
            out.push_str(&format!(
                "    .quad {}\n",
                method_symbol(impl_class.unwrap(), method_name)
            ));
        } else {
            out.push_str("    .quad 0\n");
        }
    }
}

/// Emits the per-class callable-method name table and count for __invoke support.
/// Only public instance methods are included. Each method name is emitted as an
/// ASCII label; the table is indexed by class_id at runtime.
fn emit_class_callable_methods(out: &mut String, class_info: &ClassInfo) {
    let mut public_methods: Vec<&String> = class_info
        .methods
        .keys()
        .filter(|method_name| {
            class_info
                .method_visibilities
                .get(*method_name)
                .is_some_and(|visibility| matches!(visibility, Visibility::Public))
        })
        .collect();
    public_methods.sort();
    for method_name in &public_methods {
        out.push_str(&format!(
            ".globl _class_callable_method_name_{0}_{1}\n_class_callable_method_name_{0}_{1}:\n    .ascii \"{2}\"\n",
            class_info.class_id,
            mangle_fqn(method_name),
            escaped_ascii(method_name)
        ));
    }
    out.push_str(".p2align 3\n");
    out.push_str(&format!(
        ".globl _class_callable_methods_{0}\n_class_callable_methods_{0}:\n",
        class_info.class_id
    ));
    out.push_str(&format!("    .quad {}\n", public_methods.len()));
    for method_name in public_methods {
        out.push_str(&format!(
            "    .quad _class_callable_method_name_{}_{}\n",
            class_info.class_id,
            mangle_fqn(method_name)
        ));
        out.push_str(&format!("    .quad {}\n", method_name.len()));
    }
}

/// Emits the static-callable method table for ReflectionMethod support on static methods.
/// For each class with public static methods, emits class-name and method-name labels,
/// then builds an entries table of (class_name_ptr, class_name_len, method_name_ptr, method_name_len).
fn emit_static_callable_method_data(out: &mut String, sorted_classes: &[(&String, &ClassInfo)]) {
    let mut entries = Vec::new();
    for (class_name, class_info) in sorted_classes {
        let mut public_static_methods: Vec<&String> = class_info
            .static_methods
            .keys()
            .filter(|method_name| {
                class_info
                    .static_method_visibilities
                    .get(*method_name)
                    .is_some_and(|visibility| matches!(visibility, Visibility::Public))
            })
            .collect();
        public_static_methods.sort();
        if public_static_methods.is_empty() {
            continue;
        }

        out.push_str(&format!(
            ".globl _class_callable_static_class_name_{0}\n_class_callable_static_class_name_{0}:\n    .ascii \"{1}\"\n",
            class_info.class_id,
            escaped_ascii(class_name)
        ));
        for method_name in public_static_methods {
            out.push_str(&format!(
                ".globl _class_callable_static_method_name_{0}_{1}\n_class_callable_static_method_name_{0}_{1}:\n    .ascii \"{2}\"\n",
                class_info.class_id,
                mangle_fqn(method_name),
                escaped_ascii(method_name)
            ));
            entries.push((class_info.class_id, class_name.as_str(), method_name.as_str()));
        }
    }

    out.push_str(".p2align 3\n");
    out.push_str(".globl _class_callable_static_method_count\n_class_callable_static_method_count:\n");
    out.push_str(&format!("    .quad {}\n", entries.len()));
    out.push_str(".globl _class_callable_static_method_table\n_class_callable_static_method_table:\n");
    for (class_id, class_name, method_name) in entries {
        out.push_str(&format!(
            "    .quad _class_callable_static_class_name_{}\n",
            class_id
        ));
        out.push_str(&format!("    .quad {}\n", class_name.len()));
        out.push_str(&format!(
            "    .quad _class_callable_static_method_name_{}_{}\n",
            class_id,
            mangle_fqn(method_name)
        ));
        out.push_str(&format!("    .quad {}\n", method_name.len()));
    }
}

/// Returns the symbol name to use for an interface method table entry.
/// Returns a wrapper symbol when the interface declares a Mixed return type but the
/// implementing class uses a narrower type (the wrapper bridges the type mismatch).
fn interface_method_table_symbol(
    class_info: &ClassInfo,
    interface_info: &InterfaceInfo,
    method_name: &str,
    impl_class: &str,
    classes: &HashMap<String, ClassInfo>,
) -> String {
    if interface_method_needs_return_wrapper(interface_info, method_name, impl_class, classes) {
        interface_method_wrapper_symbol(
            class_info.class_id,
            interface_info.interface_id,
            method_name,
        )
    } else {
        method_symbol(impl_class, method_name)
    }
}

/// Returns true when an interface method requires a return-type wrapper at call sites.
/// A wrapper is needed when the interface declares a Mixed return type but the
/// implementing class uses a narrower type — without the wrapper, a Mixed would be
/// written where a typed value is expected.
fn interface_method_needs_return_wrapper(
    interface_info: &InterfaceInfo,
    method_name: &str,
    impl_class: &str,
    classes: &HashMap<String, ClassInfo>,
) -> bool {
    let Some(interface_sig) = interface_info.methods.get(method_name) else {
        return false;
    };
    let Some(actual_sig) = classes
        .get(impl_class)
        .and_then(|class_info| class_info.methods.get(method_name))
    else {
        return false;
    };

    matches!(interface_sig.return_type.codegen_repr(), PhpType::Mixed)
        && !matches!(actual_sig.return_type.codegen_repr(), PhpType::Mixed)
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::parser::ast::Visibility;
    use crate::types::ClassInfo;

    use super::emit_runtime_data_user;

    /// Provides the Empty class info helper used by the user module.
    fn empty_class_info(class_id: u64, method_name: &str) -> ClassInfo {
        let mut method_impl_classes = HashMap::new();
        method_impl_classes.insert(method_name.to_string(), "Exception".to_string());

        let mut vtable_slots = HashMap::new();
        vtable_slots.insert(method_name.to_string(), 0);

        ClassInfo {
            class_id,
            parent: None,
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            allow_dynamic_properties: false,
            constants: HashMap::new(),
            constant_visibilities: HashMap::new(),
            final_constants: HashSet::new(),
            attribute_names: Vec::new(),
            attribute_args: Vec::new(),
            method_attribute_names: HashMap::new(),
            method_attribute_args: HashMap::new(),
            property_attribute_names: HashMap::new(),
            property_attribute_args: HashMap::new(),
            constant_attribute_names: HashMap::new(),
            constant_attribute_args: HashMap::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
            properties: Vec::new(),
            property_offsets: HashMap::new(),
            property_declaring_classes: HashMap::new(),
            defaults: Vec::new(),
            property_visibilities: HashMap::new(),
            property_set_visibilities: HashMap::new(),
            declared_properties: HashSet::new(),
            property_declared_slots: Vec::new(),
            final_properties: HashSet::new(),
            readonly_properties: HashSet::new(),
            reference_properties: HashSet::new(),
            promoted_properties: HashSet::new(),
            property_reference_slots: Vec::new(),
            abstract_properties: HashSet::new(),
            abstract_property_hooks: HashMap::new(),
            static_properties: Vec::new(),
            static_defaults: Vec::new(),
            static_property_declaring_classes: HashMap::new(),
            static_property_visibilities: HashMap::new(),
            declared_static_properties: HashSet::new(),
            final_static_properties: HashSet::new(),
            method_decls: Vec::new(),
            methods: HashMap::new(),
            static_methods: HashMap::new(),
            callable_method_return_sigs: HashMap::new(),
            callable_array_method_return_sigs: HashMap::new(),
            method_visibilities: HashMap::<String, Visibility>::new(),
            final_methods: HashSet::new(),
            method_declaring_classes: HashMap::new(),
            method_impl_classes,
            vtable_methods: vec![method_name.to_string()],
            vtable_slots,
            static_method_visibilities: HashMap::new(),
            final_static_methods: HashSet::new(),
            static_method_declaring_classes: HashMap::new(),
            static_method_impl_classes: HashMap::new(),
            static_vtable_methods: Vec::new(),
            static_vtable_slots: HashMap::new(),
            interfaces: Vec::new(),
            constructor_param_to_prop: Vec::new(),
        }
    }

    /// Verifies that emit runtime data user can filter built in classes.
    #[test]
    fn test_emit_runtime_data_user_can_filter_built_in_classes() {
        let mut classes = HashMap::new();
        classes.insert(
            "Exception".to_string(),
            empty_class_info(0, "__construct"),
        );
        classes.insert(
            "UserVisible".to_string(),
            empty_class_info(1, "run"),
        );

        let mut allowed_class_names = HashSet::new();
        allowed_class_names.insert("UserVisible".to_string());

        let asm = emit_runtime_data_user(
            &HashSet::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashMap::new(),
            &[],
            &[],
            &classes,
            &HashMap::new(),
            Some(&allowed_class_names),
        );

        assert!(asm.contains("_class_vtable_1"));
        assert!(asm.contains("_method_Exception_run"));
        assert!(!asm.contains("_class_vtable_0"));
        assert!(!asm.contains("_method_Exception__construct"));
    }

    /// Verifies that emit runtime data user keeps dense class tables when ids start at one.
    #[test]
    fn test_emit_runtime_data_user_keeps_dense_class_tables_when_ids_start_at_one() {
        let mut classes = HashMap::new();
        classes.insert("Animal".to_string(), empty_class_info(1, "label"));
        classes.insert("Dog".to_string(), empty_class_info(2, "label"));
        classes.insert("Cat".to_string(), empty_class_info(3, "label"));

        let asm = emit_runtime_data_user(
            &HashSet::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashMap::new(),
            &[],
            &[],
            &classes,
            &HashMap::new(),
            None,
        );

        assert!(asm.contains("_class_gc_desc_count:\n    .quad 4\n"));
        assert!(asm.contains("_class_parent_ids:\n    .quad -1\n    .quad -1\n    .quad -1\n    .quad -1\n"));
        assert!(asm.contains("_class_vtable_ptrs:\n    .quad _class_vtable_missing\n    .quad _class_vtable_1\n    .quad _class_vtable_2\n    .quad _class_vtable_3\n"));
        assert!(asm.contains("_class_static_vtable_ptrs:\n    .quad _class_static_vtable_missing\n    .quad _class_static_vtable_1\n    .quad _class_static_vtable_2\n    .quad _class_static_vtable_3\n"));
    }
}
