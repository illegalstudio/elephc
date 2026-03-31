use std::collections::{HashMap, HashSet};

use crate::names::{mangle_fqn, method_symbol, static_method_symbol};
use crate::types::{ClassInfo, InterfaceInfo, PhpType};

use super::system;

pub(crate) fn emit_runtime_data(
    global_var_names: &HashSet<String>,
    static_vars: &HashMap<(String, String), PhpType>,
    interfaces: &HashMap<String, InterfaceInfo>,
    classes: &HashMap<String, ClassInfo>,
    heap_size: usize,
) -> String {
    let mut out = String::new();
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
    out.push_str(&format!("_heap_max:\n    .quad {}\n", heap_size));
    out.push_str("_heap_err_msg:\n    .ascii \"Fatal error: heap memory exhausted\\n\"\n");
    out.push_str("_heap_dbg_bad_refcount_msg:\n    .ascii \"Fatal error: heap debug detected bad refcount\\n\"\n");
    out.push_str("_heap_dbg_double_free_msg:\n    .ascii \"Fatal error: heap debug detected double free\\n\"\n");
    out.push_str("_heap_dbg_free_list_msg:\n    .ascii \"Fatal error: heap debug detected free-list corruption\\n\"\n");
    out.push_str("_arr_cap_err_msg:\n    .ascii \"Fatal error: array capacity exceeded\\n\"\n");
    out.push_str("_ptr_null_err_msg:\n    .ascii \"Fatal error: null pointer dereference\\n\"\n");
    out.push_str("_uncaught_exc_msg:\n    .ascii \"Fatal error: uncaught exception\\n\"\n");
    out.push_str(".comm _gc_allocs, 8, 3\n");
    out.push_str(".comm _gc_frees, 8, 3\n");
    out.push_str(".comm _gc_live, 8, 3\n");
    out.push_str(".comm _gc_peak, 8, 3\n");
    out.push_str(".comm _cstr_buf, 4096, 3\n");
    out.push_str(".comm _cstr_buf2, 4096, 3\n");
    out.push_str(".comm _eof_flags, 256, 3\n");
    out.push_str("_heap_dbg_stats_prefix:\n    .ascii \"HEAP DEBUG: allocs=\"\n");
    out.push_str("_heap_dbg_frees_label:\n    .ascii \" frees=\"\n");
    out.push_str("_heap_dbg_live_blocks_label:\n    .ascii \" live_blocks=\"\n");
    out.push_str("_heap_dbg_live_bytes_label:\n    .ascii \" live_bytes=\"\n");
    out.push_str("_heap_dbg_peak_label:\n    .ascii \" peak_live_bytes=\"\n");
    out.push_str("_heap_dbg_leak_prefix:\n    .ascii \"HEAP DEBUG: leak summary: \"\n");
    out.push_str("_heap_dbg_live_blocks_short_label:\n    .ascii \"live_blocks=\"\n");
    out.push_str("_heap_dbg_clean_label:\n    .ascii \"clean\\n\"\n");
    out.push_str("_heap_dbg_newline:\n    .ascii \"\\n\"\n");
    out.push_str("_fmt_g:\n    .asciz \"%.14G\"\n");
    out.push_str("_b64_encode_tbl:\n    .ascii \"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/\"\n");
    out.push_str("_b64_decode_tbl:\n");

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

    out.push_str("_pcre_space:\n    .ascii \"[[:space:]]\"\n");
    out.push_str("_pcre_digit:\n    .ascii \"[[:digit:]]\"\n");
    out.push_str("_pcre_word:\n    .ascii \"[[:alnum:]_]\"\n");
    out.push_str("_pcre_nspace:\n    .ascii \"[^[:space:]]\"\n");
    out.push_str("_pcre_ndigit:\n    .ascii \"[^[:digit:]]\"\n");
    out.push_str("_pcre_nword:\n    .ascii \"[^[:alnum:]_]\"\n");
    out.push_str(&system::emit_json_data());
    out.push_str(&system::emit_date_data());

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

    let mut sorted_interfaces: Vec<(&String, &InterfaceInfo)> = interfaces.iter().collect();
    sorted_interfaces.sort_by_key(|(_, interface_info)| interface_info.interface_id);
    let mut sorted_classes: Vec<(&String, &ClassInfo)> = classes.iter().collect();
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);
    let class_id_by_name: HashMap<String, u64> = sorted_classes
        .iter()
        .map(|(name, class_info)| ((*name).clone(), class_info.class_id))
        .collect();

    out.push_str(".data\n");
    out.push_str(".p2align 3\n");
    out.push_str("_interface_count:\n");
    out.push_str(&format!("    .quad {}\n", sorted_interfaces.len()));
    out.push_str("_interface_method_ptrs:\n");
    for (_, interface_info) in &sorted_interfaces {
        out.push_str(&format!(
            "    .quad _interface_methods_{}\n",
            interface_info.interface_id
        ));
    }

    out.push_str("_class_interface_ptrs:\n");
    for (_, class_info) in &sorted_classes {
        out.push_str(&format!(
            "    .quad _class_interfaces_{}\n",
            class_info.class_id
        ));
    }

    out.push_str("_class_parent_ids:\n");
    for (_, class_info) in &sorted_classes {
        let parent_id = class_info
            .parent
            .as_ref()
            .and_then(|parent_name| class_id_by_name.get(parent_name))
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-1".to_string());
        out.push_str(&format!("    .quad {}\n", parent_id));
    }

    out.push_str("_class_gc_desc_count:\n");
    out.push_str(&format!("    .quad {}\n", sorted_classes.len()));
    out.push_str("_class_gc_desc_ptrs:\n");
    for (_, class_info) in &sorted_classes {
        out.push_str(&format!("    .quad _class_gc_desc_{}\n", class_info.class_id));
    }

    out.push_str("_class_vtable_ptrs:\n");
    for (_, class_info) in &sorted_classes {
        out.push_str(&format!("    .quad _class_vtable_{}\n", class_info.class_id));
    }

    out.push_str("_class_static_vtable_ptrs:\n");
    for (_, class_info) in &sorted_classes {
        out.push_str(&format!(
            "    .quad _class_static_vtable_{}\n",
            class_info.class_id
        ));
    }

    for (_, interface_info) in &sorted_interfaces {
        out.push_str(&format!(
            "_interface_methods_{}:\n",
            interface_info.interface_id
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
        out.push_str(&format!("_class_interfaces_{}:\n", class_info.class_id));
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
                "_class_interface_impl_{}_{}:\n",
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

        out.push_str(&format!("_class_gc_desc_{}:\n", class_info.class_id));
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
                    PhpType::Callable | PhpType::Pointer(_) | PhpType::Void => 0,
                };
                out.push_str(&tag.to_string());
            }
            out.push('\n');
        }

        out.push_str("    .p2align 3\n");
        out.push_str(&format!("_class_vtable_{}:\n", class_info.class_id));
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
        out.push_str(&format!("_class_static_vtable_{}:\n", class_info.class_id));
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
