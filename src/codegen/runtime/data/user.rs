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
    enum_case_symbol, interface_method_wrapper_symbol, mangle_fqn, method_symbol, php_symbol_key,
    static_method_symbol, static_property_symbol,
};
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, EnumInfo, InterfaceInfo, PhpType};

use super::instanceof::escaped_ascii;

/// Emit the user-dependent data section — globals, statics, class metadata.
/// This changes per program and cannot be cached.
pub(crate) fn emit_runtime_data_user(
    global_var_names: &HashSet<String>,
    static_vars: &HashMap<(String, String), PhpType>,
    interfaces: &HashMap<String, InterfaceInfo>,
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
    for enum_name in sorted_enum_names {
        let Some(enum_info) = enums.get(enum_name) else {
            continue;
        };
        for case in &enum_info.cases {
            out.push_str(&format!(
                ".comm {}, 8, 3\n",
                enum_case_symbol(enum_name, &case.name)
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
    let max_class_id = sorted_classes.iter().map(|(_, class_info)| class_info.class_id).max();

    out.push_str(".data\n");
    out.push_str(".p2align 3\n");
    super::instanceof::emit_instanceof_target_lookup_data(&mut out, &sorted_interfaces, &sorted_classes);

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

    // -- attribute metadata (PHP 8 attributes, future ReflectionClass) --
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
    //   tag 3 = bool  (lo = 0 or 1,            hi = 0)
    //   tag 8 = null  (lo = 0,                 hi = 0)
    //
    // Unsupported args are represented as absent metadata by
    // `collect_attribute_args`; reflection helpers reject queries that would
    // need those payloads before codegen reaches this table. Float and other
    // mixed-cell payloads are reserved for future iterations.
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
                        match arg {
                            crate::types::AttrArgValue::Str(value) => {
                                let label = format!("_attr_arg_str_{}", arg_str_id);
                                arg_str_id += 1;
                                out.push_str(&format!(".globl {0}\n{0}:\n", label));
                                out.push_str(&format!(
                                    "    .ascii \"{}\"\n",
                                    escaped_ascii(value)
                                ));
                                arg_rows.push((1u64, label, value.len() as u64));
                            }
                            crate::types::AttrArgValue::Int(value) => {
                                arg_rows.push((0u64, format!("{}", *value as u64), 0u64));
                            }
                            crate::types::AttrArgValue::Bool(value) => {
                                arg_rows.push((3u64, format!("{}", *value as u64), 0u64));
                            }
                            crate::types::AttrArgValue::Null => {
                                arg_rows.push((8u64, "0".to_string(), 0u64));
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
            let tag = if class_info.reference_properties.contains(prop_name) {
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
                let tag = if class_info.reference_properties.contains(prop_name) {
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
            continue;
        }
        for method_name in &class_info.static_vtable_methods {
            if let Some(impl_class) = class_info.static_method_impl_classes.get(method_name) {
                out.push_str(&format!("    .quad {}\n", static_method_symbol(impl_class, method_name)));
            } else {
                out.push_str("    .quad 0\n");
            }
        }
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
            attribute_names: Vec::new(),
            attribute_args: Vec::new(),
            properties: Vec::new(),
            property_offsets: HashMap::new(),
            property_declaring_classes: HashMap::new(),
            defaults: Vec::new(),
            property_visibilities: HashMap::new(),
            declared_properties: HashSet::new(),
            final_properties: HashSet::new(),
            readonly_properties: HashSet::new(),
            reference_properties: HashSet::new(),
            static_properties: Vec::new(),
            static_defaults: Vec::new(),
            static_property_declaring_classes: HashMap::new(),
            static_property_visibilities: HashMap::new(),
            declared_static_properties: HashSet::new(),
            final_static_properties: HashSet::new(),
            method_decls: Vec::new(),
            methods: HashMap::new(),
            static_methods: HashMap::new(),
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
            &classes,
            &HashMap::new(),
            Some(&allowed_class_names),
        );

        assert!(asm.contains("_class_vtable_1"));
        assert!(asm.contains("_method_Exception_run"));
        assert!(!asm.contains("_class_vtable_0"));
        assert!(!asm.contains("_method_Exception__construct"));
    }

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
