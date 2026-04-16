use std::collections::{HashMap, HashSet};

use crate::names::{enum_case_symbol, mangle_fqn, method_symbol, static_method_symbol};
use crate::types::{ClassInfo, EnumInfo, InterfaceInfo, PhpType};

use super::system;

/// Emit the fixed runtime data section — cacheable across compilations.
/// Contains heap buffers, error messages, lookup tables, and other
/// data that does not depend on the user's program.
pub(crate) fn emit_runtime_data_fixed(heap_size: usize) -> String {
    let mut out = String::new();
    out.push_str(".data\n");
    out.push_str(".comm _concat_buf, 65536, 3\n");
    out.push_str(".comm _concat_off, 8, 3\n");
    out.push_str(".comm _global_argc, 8, 3\n");
    out.push_str(".comm _global_argv, 8, 3\n");
    out.push_str(".comm _exc_handler_top, 8, 3\n");
    out.push_str(".comm _exc_call_frame_top, 8, 3\n");
    out.push_str(".comm _exc_value, 8, 3\n");
    out.push_str(&format!(".comm _heap_buf, {}, 3\n", heap_size));
    out.push_str(".comm _heap_off, 8, 3\n");
    out.push_str(".comm _heap_free_list, 8, 3\n");
    out.push_str(".comm _heap_small_bins, 32, 3\n");
    out.push_str(".comm _heap_debug_enabled, 8, 3\n");
    out.push_str(".comm _gc_collecting, 8, 3\n");
    out.push_str(".comm _gc_release_suppressed, 8, 3\n");
    out.push_str(&format!(".globl _heap_max\n_heap_max:\n    .quad {}\n", heap_size));
    out.push_str(".globl _heap_err_msg\n_heap_err_msg:\n    .ascii \"Fatal error: heap memory exhausted\\n\"\n");
    out.push_str(".globl _heap_dbg_bad_refcount_msg\n_heap_dbg_bad_refcount_msg:\n    .ascii \"Fatal error: heap debug detected bad refcount\\n\"\n");
    out.push_str(".globl _heap_dbg_double_free_msg\n_heap_dbg_double_free_msg:\n    .ascii \"Fatal error: heap debug detected double free\\n\"\n");
    out.push_str(".globl _heap_dbg_free_list_msg\n_heap_dbg_free_list_msg:\n    .ascii \"Fatal error: heap debug detected free-list corruption\\n\"\n");
    out.push_str(".globl _arr_cap_err_msg\n_arr_cap_err_msg:\n    .ascii \"Fatal error: array capacity exceeded\\n\"\n");
    out.push_str(".globl _buffer_bounds_msg\n_buffer_bounds_msg:\n    .ascii \"Fatal error: buffer index out of bounds\\n\"\n");
    out.push_str(".globl _buffer_uaf_msg\n_buffer_uaf_msg:\n    .ascii \"Fatal error: use of buffer after buffer_free()\\n\"\n");
    out.push_str(".globl _match_unhandled_msg\n_match_unhandled_msg:\n    .ascii \"Fatal error: unhandled match case\\n\"\n");
    out.push_str(".globl _enum_from_msg\n_enum_from_msg:\n    .ascii \"Fatal error: enum case not found\\n\"\n");
    out.push_str(".globl _ptr_null_err_msg\n_ptr_null_err_msg:\n    .ascii \"Fatal error: null pointer dereference\\n\"\n");
    out.push_str(".globl _uncaught_exc_msg\n_uncaught_exc_msg:\n    .ascii \"Fatal error: uncaught exception\\n\"\n");
    out.push_str(".comm _gc_allocs, 8, 3\n");
    out.push_str(".comm _gc_frees, 8, 3\n");
    out.push_str(".comm _gc_live, 8, 3\n");
    out.push_str(".comm _gc_peak, 8, 3\n");
    out.push_str(".comm _cstr_buf, 4096, 3\n");
    out.push_str(".comm _cstr_buf2, 4096, 3\n");
    out.push_str(".comm _eof_flags, 256, 3\n");
    out.push_str(".globl _heap_dbg_stats_prefix\n_heap_dbg_stats_prefix:\n    .ascii \"HEAP DEBUG: allocs=\"\n");
    out.push_str(".globl _heap_dbg_frees_label\n_heap_dbg_frees_label:\n    .ascii \" frees=\"\n");
    out.push_str(".globl _heap_dbg_live_blocks_label\n_heap_dbg_live_blocks_label:\n    .ascii \" live_blocks=\"\n");
    out.push_str(".globl _heap_dbg_live_bytes_label\n_heap_dbg_live_bytes_label:\n    .ascii \" live_bytes=\"\n");
    out.push_str(".globl _heap_dbg_peak_label\n_heap_dbg_peak_label:\n    .ascii \" peak_live_bytes=\"\n");
    out.push_str(".globl _heap_dbg_leak_prefix\n_heap_dbg_leak_prefix:\n    .ascii \"HEAP DEBUG: leak summary: \"\n");
    out.push_str(".globl _heap_dbg_live_blocks_short_label\n_heap_dbg_live_blocks_short_label:\n    .ascii \"live_blocks=\"\n");
    out.push_str(".globl _heap_dbg_clean_label\n_heap_dbg_clean_label:\n    .ascii \"clean\\n\"\n");
    out.push_str(".globl _heap_dbg_newline\n_heap_dbg_newline:\n    .ascii \"\\n\"\n");
    out.push_str(".globl _fmt_g\n_fmt_g:\n    .asciz \"%.14G\"\n");
    out.push_str(".globl _b64_encode_tbl\n_b64_encode_tbl:\n    .ascii \"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/\"\n");
    out.push_str(".globl _b64_decode_tbl\n_b64_decode_tbl:\n");

    let mut decode_tbl = vec![0u8; 256];
    for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        decode_tbl[c as usize] = i as u8;
    }

    out.push_str("    .byte ");
    for (i, val) in decode_tbl.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&val.to_string());
    }
    out.push('\n');

    out.push_str(".globl _pcre_space\n_pcre_space:\n    .ascii \"[[:space:]]\"\n");
    out.push_str(".globl _pcre_digit\n_pcre_digit:\n    .ascii \"[[:digit:]]\"\n");
    out.push_str(".globl _pcre_word\n_pcre_word:\n    .ascii \"[[:alnum:]_]\"\n");
    out.push_str(".globl _pcre_nspace\n_pcre_nspace:\n    .ascii \"[^[:space:]]\"\n");
    out.push_str(".globl _pcre_ndigit\n_pcre_ndigit:\n    .ascii \"[^[:digit:]]\"\n");
    out.push_str(".globl _pcre_nword\n_pcre_nword:\n    .ascii \"[^[:alnum:]_]\"\n");
    out.push_str(&system::emit_json_data());
    out.push_str(&system::emit_date_data());

    out
}

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
    out.push_str("    .p2align 3\n");
    out.push_str(".globl _class_vtable_missing\n_class_vtable_missing:\n");
    out.push_str("    .quad 0\n");
    out.push_str("    .p2align 3\n");
    out.push_str(
        ".globl _class_static_vtable_missing\n_class_static_vtable_missing:\n",
    );
    out.push_str("    .quad 0\n");

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
                    out.push_str(&format!("    .quad {}\n", method_symbol(impl_class, method_name)));
                } else {
                    out.push_str("    .quad 0\n");
                }
            }
        }

        out.push_str(&format!(".globl _class_gc_desc_{}\n_class_gc_desc_{}:\n", class_info.class_id, class_info.class_id));
        if class_info.properties.is_empty() {
            out.push_str("    .byte 0\n");
        } else {
            out.push_str("    .byte ");
            for (i, (_, prop_ty)) in class_info.properties.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let tag = match prop_ty {
                    PhpType::Int => 0,
                    PhpType::Str => 1,
                    PhpType::Float => 2,
                    PhpType::Bool => 3,
                    PhpType::Array(_) => 4,
                    PhpType::AssocArray { .. } => 5,
                    PhpType::Object(_) => 6,
                    PhpType::Mixed => 7,
                    PhpType::Union(_) => 7,
                    PhpType::Callable
                    | PhpType::Pointer(_)
                    | PhpType::Buffer(_)
                    | PhpType::Packed(_)
                    | PhpType::Void => 0,
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

    out
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
            is_readonly_class: false,
            properties: Vec::new(),
            property_offsets: HashMap::new(),
            property_declaring_classes: HashMap::new(),
            defaults: Vec::new(),
            property_visibilities: HashMap::new(),
            readonly_properties: HashSet::new(),
            method_decls: Vec::new(),
            methods: HashMap::new(),
            static_methods: HashMap::new(),
            method_visibilities: HashMap::<String, Visibility>::new(),
            method_declaring_classes: HashMap::new(),
            method_impl_classes,
            vtable_methods: vec![method_name.to_string()],
            vtable_slots,
            static_method_visibilities: HashMap::new(),
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
