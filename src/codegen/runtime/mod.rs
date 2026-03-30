mod arrays;
mod io;
mod pointers;
mod strings;
mod system;

use super::emit::Emitter;

pub(crate) fn emit_runtime(emitter: &mut Emitter) {
    // String runtime functions
    strings::emit_itoa(emitter);
    strings::emit_ftoa(emitter);
    strings::emit_concat(emitter);
    strings::emit_atoi(emitter);
    strings::emit_str_eq(emitter);
    strings::emit_number_format(emitter);
    strings::emit_strcopy(emitter);
    strings::emit_str_persist(emitter);
    strings::emit_strtolower(emitter);
    strings::emit_strtoupper(emitter);
    strings::emit_trim(emitter);
    strings::emit_ltrim(emitter);
    strings::emit_rtrim(emitter);
    strings::emit_strpos(emitter);
    strings::emit_strrpos(emitter);
    strings::emit_str_repeat(emitter);
    strings::emit_strrev(emitter);
    strings::emit_chr(emitter);
    strings::emit_strcmp(emitter);
    strings::emit_strcasecmp(emitter);
    strings::emit_str_starts_with(emitter);
    strings::emit_str_ends_with(emitter);
    strings::emit_str_replace(emitter);
    strings::emit_explode(emitter);
    strings::emit_implode(emitter);
    strings::emit_implode_int(emitter);
    strings::emit_ucwords(emitter);
    strings::emit_str_ireplace(emitter);
    strings::emit_substr_replace(emitter);
    strings::emit_str_pad(emitter);
    strings::emit_str_split(emitter);
    strings::emit_addslashes(emitter);
    strings::emit_stripslashes(emitter);
    strings::emit_nl2br(emitter);
    strings::emit_wordwrap(emitter);
    strings::emit_bin2hex(emitter);
    strings::emit_hex2bin(emitter);
    strings::emit_htmlspecialchars(emitter);
    strings::emit_html_entity_decode(emitter);
    strings::emit_urlencode(emitter);
    strings::emit_urldecode(emitter);
    strings::emit_rawurlencode(emitter);
    strings::emit_base64_encode(emitter);
    strings::emit_base64_decode(emitter);
    strings::emit_sprintf(emitter);
    strings::emit_md5(emitter);
    strings::emit_sha1(emitter);
    strings::emit_hash(emitter);
    strings::emit_sscanf(emitter);
    strings::emit_rtrim_mask(emitter);
    strings::emit_ltrim_mask(emitter);
    strings::emit_trim_mask(emitter);

    // System runtime functions
    system::emit_build_argv(emitter);
    system::emit_time(emitter);
    system::emit_microtime(emitter);
    system::emit_getenv(emitter);
    system::emit_shell_exec(emitter);
    system::emit_date(emitter);
    system::emit_mktime(emitter);
    system::emit_strtotime(emitter);
    system::emit_json_encode_bool(emitter);
    system::emit_json_encode_null(emitter);
    system::emit_json_encode_str(emitter);
    system::emit_json_encode_mixed(emitter);
    system::emit_json_encode_array_int(emitter);
    system::emit_json_encode_array_str(emitter);
    system::emit_json_encode_assoc(emitter);
    system::emit_json_decode(emitter);
    system::emit_preg_strip(emitter);
    system::emit_pcre_to_posix(emitter);
    system::emit_preg_match(emitter);
    system::emit_preg_match_all(emitter);
    system::emit_preg_replace(emitter);
    system::emit_preg_split(emitter);

    // Array runtime functions
    arrays::emit_heap_alloc(emitter);
    arrays::emit_heap_debug_fail(emitter);
    arrays::emit_heap_debug_check_live(emitter);
    arrays::emit_heap_debug_validate_free_list(emitter);
    arrays::emit_heap_debug_report(emitter);
    arrays::emit_heap_kind(emitter);
    arrays::emit_heap_free(emitter);
    arrays::emit_array_free_deep(emitter);
    arrays::emit_array_clone_shallow(emitter);
    arrays::emit_array_ensure_unique(emitter);
    arrays::emit_array_grow(emitter);
    arrays::emit_array_new(emitter);
    arrays::emit_array_push_int(emitter);
    arrays::emit_array_push_refcounted(emitter);
    arrays::emit_array_push_str(emitter);
    arrays::emit_sort_int(emitter, false);
    arrays::emit_sort_int(emitter, true);
    arrays::emit_hash_fnv1a(emitter);
    arrays::emit_hash_clone_shallow(emitter);
    arrays::emit_hash_ensure_unique(emitter);
    arrays::emit_hash_new(emitter);
    arrays::emit_hash_grow(emitter);
    arrays::emit_hash_may_have_cyclic_values(emitter);
    arrays::emit_hash_set(emitter);
    arrays::emit_hash_insert_owned(emitter);
    arrays::emit_hash_get(emitter);
    arrays::emit_hash_iter(emitter);
    arrays::emit_hash_count(emitter);
    arrays::emit_hash_free_deep(emitter);
    arrays::emit_array_key_exists(emitter);
    arrays::emit_array_search(emitter);
    arrays::emit_array_reverse(emitter);
    arrays::emit_array_reverse_refcounted(emitter);
    arrays::emit_array_sum(emitter);
    arrays::emit_array_product(emitter);
    arrays::emit_array_shift(emitter);
    arrays::emit_array_unshift(emitter);
    arrays::emit_array_merge(emitter);
    arrays::emit_array_merge_refcounted(emitter);
    arrays::emit_array_slice(emitter);
    arrays::emit_array_slice_refcounted(emitter);
    arrays::emit_range(emitter);
    arrays::emit_shuffle(emitter);
    arrays::emit_array_unique(emitter);
    arrays::emit_array_unique_refcounted(emitter);
    arrays::emit_array_rand(emitter);
    arrays::emit_array_fill(emitter);
    arrays::emit_array_fill_refcounted(emitter);
    arrays::emit_array_pad(emitter);
    arrays::emit_array_pad_refcounted(emitter);
    arrays::emit_array_diff(emitter);
    arrays::emit_array_diff_refcounted(emitter);
    arrays::emit_array_intersect(emitter);
    arrays::emit_array_intersect_refcounted(emitter);
    arrays::emit_array_flip(emitter);
    arrays::emit_array_combine(emitter);
    arrays::emit_array_combine_refcounted(emitter);
    arrays::emit_array_fill_keys(emitter);
    arrays::emit_array_fill_keys_refcounted(emitter);
    arrays::emit_array_chunk(emitter);
    arrays::emit_array_chunk_refcounted(emitter);
    arrays::emit_array_column(emitter);
    arrays::emit_array_column_ref(emitter);
    arrays::emit_array_column_str(emitter);
    arrays::emit_array_splice(emitter);
    arrays::emit_array_splice_refcounted(emitter);
    arrays::emit_array_diff_key(emitter);
    arrays::emit_array_intersect_key(emitter);
    arrays::emit_asort(emitter);
    arrays::emit_ksort(emitter);
    arrays::emit_natsort(emitter);
    arrays::emit_array_map(emitter);
    arrays::emit_array_map_str(emitter);
    arrays::emit_array_filter(emitter);
    arrays::emit_array_filter_refcounted(emitter);
    arrays::emit_array_reduce(emitter);
    arrays::emit_array_walk(emitter);
    arrays::emit_usort(emitter);
    arrays::emit_array_merge_into(emitter);
    arrays::emit_array_merge_into_refcounted(emitter);
    arrays::emit_decref_any(emitter);
    arrays::emit_decref_mixed(emitter);
    arrays::emit_gc_note_child_ref(emitter);
    arrays::emit_gc_mark_reachable(emitter);
    arrays::emit_gc_collect_cycles(emitter);
    arrays::emit_mixed_from_value(emitter);
    arrays::emit_mixed_free_deep(emitter);
    arrays::emit_mixed_is_empty(emitter);
    arrays::emit_mixed_write_stdout(emitter);
    arrays::emit_object_free_deep(emitter);
    arrays::emit_refcount(emitter);

    // I/O runtime functions
    io::emit_cstr(emitter);
    io::emit_fopen(emitter);
    io::emit_fgets(emitter);
    io::emit_feof(emitter);
    io::emit_fread(emitter);
    io::emit_file_get_contents(emitter);
    io::emit_file_put_contents(emitter);
    io::emit_file(emitter);
    io::emit_stat(emitter);
    io::emit_fs(emitter);
    io::emit_getcwd(emitter);
    io::emit_scandir(emitter);
    io::emit_glob(emitter);
    io::emit_tempnam(emitter);
    io::emit_fgetcsv(emitter);
    io::emit_fputcsv(emitter);

    // Pointer runtime functions
    pointers::emit_ptoa(emitter);
    pointers::emit_ptr_check_nonnull(emitter);
    pointers::emit_str_to_cstr(emitter);
    pointers::emit_cstr_to_str(emitter);
}

pub(crate) fn emit_runtime_data(
    global_var_names: &std::collections::HashSet<String>,
    static_vars: &std::collections::HashMap<(String, String), crate::types::PhpType>,
    interfaces: &std::collections::HashMap<String, crate::types::InterfaceInfo>,
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
    heap_size: usize,
) -> String {
    let mut out = String::new();
    out.push_str(".comm _concat_buf, 65536, 3\n");
    out.push_str(".comm _concat_off, 8, 3\n");
    out.push_str(".comm _global_argc, 8, 3\n");
    out.push_str(".comm _global_argv, 8, 3\n");
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
    // GC statistics counters
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
    // Base64 encode lookup table (A-Z, a-z, 0-9, +, /)
    out.push_str("_b64_encode_tbl:\n    .ascii \"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/\"\n");
    // Base64 decode lookup table (256 bytes, maps ASCII value to 6-bit value)
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
    // PCRE-to-POSIX shorthand replacement strings
    out.push_str("_pcre_space:\n    .ascii \"[[:space:]]\"\n");
    out.push_str("_pcre_digit:\n    .ascii \"[[:digit:]]\"\n");
    out.push_str("_pcre_word:\n    .ascii \"[[:alnum:]_]\"\n");
    out.push_str("_pcre_nspace:\n    .ascii \"[^[:space:]]\"\n");
    out.push_str("_pcre_ndigit:\n    .ascii \"[^[:digit:]]\"\n");
    out.push_str("_pcre_nword:\n    .ascii \"[^[:alnum:]_]\"\n");
    // JSON string constants
    out.push_str(&system::emit_json_data());
    // Date/time lookup tables (day names, month names)
    out.push_str(&system::emit_date_data());
    // Emit global variable storage for `global $var` keyword
    let mut sorted_globals: Vec<&String> = global_var_names.iter().collect();
    sorted_globals.sort();
    for name in sorted_globals {
        // 16 bytes per global var (enough for string ptr+len or int/float)
        out.push_str(&format!(".comm _gvar_{}, 16, 3\n", name));
    }
    // Emit static variable storage for `static $var = init;`
    let mut sorted_statics: Vec<&(String, String)> = static_vars.keys().collect();
    sorted_statics.sort();
    for (func_name, var_name) in sorted_statics {
        // 16 bytes for the value, 8 bytes for the init flag
        out.push_str(&format!(
            ".comm _static_{}_{}, 16, 3\n",
            func_name, var_name
        ));
        out.push_str(&format!(
            ".comm _static_{}_{}_init, 8, 3\n",
            func_name, var_name
        ));
    }

    let mut sorted_interfaces: Vec<(&String, &crate::types::InterfaceInfo)> =
        interfaces.iter().collect();
    sorted_interfaces.sort_by_key(|(_, interface_info)| interface_info.interface_id);
    let mut sorted_classes: Vec<(&String, &crate::types::ClassInfo)> = classes.iter().collect();
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);
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
                    out.push_str(&format!(
                        "    .quad _method_{}_{}\n",
                        impl_class, method_name
                    ));
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
                    crate::types::PhpType::Int => 0,
                    crate::types::PhpType::Str => 1,
                    crate::types::PhpType::Float => 2,
                    crate::types::PhpType::Bool => 3,
                    crate::types::PhpType::Array(_) => 4,
                    crate::types::PhpType::AssocArray { .. } => 5,
                    crate::types::PhpType::Object(_) => 6,
                    crate::types::PhpType::Mixed => 7,
                    crate::types::PhpType::Callable
                    | crate::types::PhpType::Pointer(_)
                    | crate::types::PhpType::Void => 0,
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
                    out.push_str(&format!(
                        "    .quad _method_{}_{}\n",
                        impl_class, method_name
                    ));
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
                out.push_str(&format!(
                    "    .quad _static_{}_{}\n",
                    impl_class, method_name
                ));
            } else {
                out.push_str("    .quad 0\n");
            }
        }
    }
    out
}
