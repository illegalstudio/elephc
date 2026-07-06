//! Purpose:
//! Defines runtime callable dispatch metadata shared by indirect callback emitters.
//! Bridges AOT function signatures with runtime-selected callable values or names.
//!
//! Called from:
//! - `crate::codegen::lower_inst::callables` and EIR builtin callback lowerers.
//!
//! Key details:
//! - Cases carry the ABI entry label, optional PHP-visible name, signature metadata, and hidden captures.
//! - String-name dispatch compares against userland callable names before loading the matched descriptor.

use crate::codegen_support::abi;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::types::{callable_wrapper_sig, FunctionSig, PhpType};

#[derive(Clone)]
pub(crate) struct RuntimeCallableCase {
    pub(crate) label: String,
    pub(crate) descriptor_label: String,
    pub(crate) php_name: Option<String>,
    pub(crate) sig: FunctionSig,
    pub(crate) captures: Vec<(String, PhpType, bool)>,
    pub(crate) has_invoker: bool,
    pub(crate) invoker_label: Option<String>,
}

pub(crate) enum RuntimeCallableSelector<'a> {
    Address(&'a str),
    StringNameStack {
        ptr_offset: usize,
        len_offset: usize,
        call_reg: &'a str,
    },
}

#[derive(Clone)]
pub(crate) struct RuntimeStaticMethodCallableCase {
    pub(crate) class_name: String,
    pub(crate) method_name: String,
    pub(crate) case: RuntimeCallableCase,
}

/// Returns true for builtins excluded from generic runtime string-callable dispatch.
///
/// These entries either are internal implementation hooks, require literal/by-ref/resource
/// argument semantics that a generic runtime wrapper cannot preserve, or are variadic callback
/// adapters whose direct and first-class-callable lowering is handled by EIR-specific paths.
pub(crate) fn runtime_builtin_wrapper_excluded(name: &str) -> bool {
    matches!(
        name,
        "call_user_func"
            | "call_user_func_array"
            | "iterator_apply"
            | "preg_replace_callback"
            | "__elephc_mktime_raw"
            | "__elephc_gmmktime_raw"
            | "__elephc_strtotime_raw"
            | "serialize"
            | "unserialize"
            | "array_merge"
            | "array_merge_recursive"
            | "gzcompress"
            | "gzdeflate"
            | "gzinflate"
            | "gzuncompress"
            | "array_diff_assoc"
            | "array_intersect_assoc"
            | "array_is_list"
            | "array_key_first"
            | "array_key_last"
            | "array_multisort"
            | "array_replace"
            | "array_replace_recursive"
            | "array_find"
            | "array_any"
            | "array_all"
            | "array_udiff"
            | "array_uintersect"
            | "array_walk_recursive"
            | "ptr"
            | "ptr_null"
            | "ptr_is_null"
            | "ptr_sizeof"
            | "ptr_offset"
            | "ptr_get"
            | "ptr_set"
            | "ptr_read8"
            | "ptr_read32"
            | "ptr_write8"
            | "ptr_write32"
            | "getenv"
            | "putenv"
            | "http_response_code"
            | "header"
            | "exec"
            | "shell_exec"
            | "system"
            | "passthru"
            | "define"
            | "class_attribute_names"
            | "class_attribute_args"
            | "class_get_attributes"
            | "preg_match"
            | "preg_match_all"
            | "preg_replace"
            | "preg_split"
            | "var_dump"
            | "print_r"
            | "realpath_cache_get"
            | "realpath_cache_size"
            | "disk_free_space"
            | "disk_total_space"
            | "clearstatcache"
            | "fstat"
            | "file_put_contents"
            | "copy"
            | "rename"
            | "unlink"
            | "mkdir"
            | "rmdir"
            | "chdir"
            | "scandir"
            | "glob"
            | "lchown"
            | "lchgrp"
            | "umask"
            | "readfile"
            | "fopen"
            | "fclose"
            | "fread"
            | "fwrite"
            | "fprintf"
            | "vfprintf"
            | "fscanf"
            | "fgets"
            | "feof"
            | "fseek"
            | "ftell"
            | "rewind"
            | "fgetc"
            | "fpassthru"
            | "fgetcsv"
            | "fputcsv"
            | "flock"
            | "tmpfile"
            | "popen"
            | "pclose"
            | "opendir"
            | "readdir"
            | "closedir"
            | "rewinddir"
            | "stream_context_create"
            | "stream_context_get_default"
            | "stream_context_set_default"
            | "stream_context_set_option"
            | "stream_context_set_params"
            | "stream_context_get_options"
            | "stream_context_get_params"
            | "stream_filter_append"
            | "stream_filter_prepend"
            | "stream_filter_remove"
            | "stream_filter_register"
            | "stream_bucket_make_writeable"
            | "stream_bucket_new"
            | "stream_bucket_append"
            | "stream_bucket_prepend"
            | "stream_wrapper_register"
            | "stream_wrapper_unregister"
            | "stream_wrapper_restore"
            | "stream_is_local"
            | "stream_resolve_include_path"
            | "stream_select"
            | "stream_set_chunk_size"
            | "stream_set_read_buffer"
            | "stream_set_write_buffer"
            | "stream_get_filters"
            | "stream_get_transports"
            | "stream_get_wrappers"
            | "stream_isatty"
            | "stream_supports_lock"
            | "stream_set_blocking"
            | "stream_set_timeout"
            | "stream_get_line"
            | "stream_get_meta_data"
            | "stream_get_contents"
            | "stream_copy_to_stream"
            | "stream_socket_server"
            | "stream_socket_client"
            | "stream_socket_accept"
            | "stream_socket_enable_crypto"
            | "stream_socket_sendto"
            | "stream_socket_recvfrom"
            | "stream_socket_get_name"
            | "stream_socket_pair"
            | "stream_socket_shutdown"
            | "fsockopen"
            | "pfsockopen"
            | "gethostname"
            | "gethostbyname"
            | "gethostbyaddr"
            | "getprotobyname"
            | "getprotobynumber"
            | "getservbyname"
            | "getservbyport"
            | "is_array"
            | "is_object"
            | "is_scalar"
            | "is_callable"
            | "is_resource"
            | "get_resource_type"
            | "get_resource_id"
            | "settype"
            | "class_alias"
            | "class_exists"
            | "interface_exists"
            | "trait_exists"
            | "enum_exists"
            | "class_implements"
            | "class_parents"
            | "class_uses"
            | "get_class"
            | "get_parent_class"
            | "is_a"
            | "is_subclass_of"
            | "get_declared_classes"
            | "get_declared_interfaces"
            | "get_declared_traits"
            | "function_exists"
    )
}

/// Builds a static-method runtime wrapper signature that can receive keyed variadic tails.
pub(crate) fn static_method_runtime_wrapper_sig(sig: &FunctionSig) -> FunctionSig {
    let mut wrapper_sig = callable_wrapper_sig(sig);
    if wrapper_sig.variadic.is_some() {
        if let Some((_, ty)) = wrapper_sig.params.last_mut() {
            *ty = PhpType::Iterable;
        }
    }
    wrapper_sig
}

/// Emits assembly for branch if callable case mismatch.
pub(crate) fn emit_branch_if_callable_case_mismatch(
    selector: &RuntimeCallableSelector<'_>,
    case: &RuntimeCallableCase,
    next_case: &str,
    emitter: &mut Emitter,
    matched_label: &str,
    data: &mut DataSection,
) {
    match selector {
        RuntimeCallableSelector::Address(call_reg) => {
            emit_branch_if_address_mismatch(call_reg, &case.label, next_case, emitter);
        }
        RuntimeCallableSelector::StringNameStack {
            ptr_offset,
            len_offset,
            call_reg,
        } => {
            emit_branch_if_string_name_mismatch(
                case,
                *ptr_offset,
                *len_offset,
                call_reg,
                next_case,
                matched_label,
                emitter,
                data,
            );
        }
    }
}

/// Computes the callable signature metadata for specialized runtime case.
pub(crate) fn specialized_runtime_case_sig(
    sig: &FunctionSig,
    source_elem_ty: Option<&PhpType>,
) -> FunctionSig {
    let Some(source_elem_ty) = source_elem_ty else {
        return sig.clone();
    };
    let mut sig = sig.clone();
    let source_ty = source_elem_ty.codegen_repr();
    if matches!(source_ty, PhpType::Void | PhpType::Never) {
        return sig;
    }
    let visible_param_count = sig.params.len();
    let regular_param_count = if sig.variadic.is_some() {
        visible_param_count.saturating_sub(1)
    } else {
        visible_param_count
    };
    for i in 0..regular_param_count {
        if sig.declared_params.get(i).copied().unwrap_or(false)
            || sig.ref_params.get(i).copied().unwrap_or(false)
        {
            continue;
        }
        if let Some((_, param_ty)) = sig.params.get_mut(i) {
            if !matches!(param_ty.codegen_repr(), PhpType::Int) {
                continue;
            }
            *param_ty = source_ty.clone();
        }
    }
    if sig.variadic.is_some() {
        let variadic_idx = visible_param_count.saturating_sub(1);
        if !sig
            .declared_params
            .get(variadic_idx)
            .copied()
            .unwrap_or(false)
        {
            if let Some((_, param_ty)) = sig.params.get_mut(variadic_idx) {
                *param_ty = PhpType::Array(Box::new(source_ty));
            }
        }
    }
    sig
}

/// Emits assembly for branch if address mismatch.
fn emit_branch_if_address_mismatch(
    call_reg: &str,
    candidate_label: &str,
    next_case: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", candidate_label);
            emitter.instruction(&format!("cmp {}, x9", call_reg)); // does the runtime callable entry match this AOT signature case?
            emitter.instruction(&format!("b.ne {}", next_case)); // try the next callable signature case when the pointer differs
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", candidate_label);
            emitter.instruction(&format!("cmp {}, r10", call_reg)); // does the runtime callable entry match this AOT signature case?
            emitter.instruction(&format!("jne {}", next_case)); // try the next callable signature case when the pointer differs
        }
    }
}

/// Emits assembly for branch if string name mismatch.
#[allow(clippy::too_many_arguments)]
fn emit_branch_if_string_name_mismatch(
    case: &RuntimeCallableCase,
    ptr_offset: usize,
    len_offset: usize,
    call_reg: &str,
    next_case: &str,
    matched_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let Some(php_name) = case.php_name.as_ref() else {
        abi::emit_jump(emitter, next_case);
        return;
    };

    let mut candidates = vec![php_name.clone()];
    if !php_name.starts_with('\\') {
        candidates.push(format!("\\{}", php_name));
    }

    for candidate in candidates {
        emit_string_name_compare(
            ptr_offset,
            len_offset,
            candidate.as_bytes(),
            &matched_label,
            emitter,
            data,
        );
    }
    abi::emit_jump(emitter, next_case);

    emitter.label(&matched_label);
    abi::emit_symbol_address(emitter, call_reg, &case.descriptor_label);
}

/// Emits assembly for string name compare.
fn emit_string_name_compare(
    ptr_offset: usize,
    len_offset: usize,
    candidate: &[u8],
    matched_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (candidate_label, candidate_len) = data.add_string(candidate);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x1", ptr_offset);
            abi::emit_load_temporary_stack_slot(emitter, "x2", len_offset);
            abi::emit_symbol_address(emitter, "x3", &candidate_label);
            abi::emit_load_int_immediate(emitter, "x4", candidate_len as i64);
            abi::emit_call_label(emitter, "__rt_strcasecmp");
            emitter.instruction("cmp x0, #0"); // did the runtime string callback name match this userland target?
            emitter.instruction(&format!("b.eq {}", matched_label)); // select this callable case when names match case-insensitively
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", ptr_offset);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", len_offset);
            abi::emit_symbol_address(emitter, "rdx", &candidate_label);
            abi::emit_load_int_immediate(emitter, "rcx", candidate_len as i64);
            abi::emit_call_label(emitter, "__rt_strcasecmp");
            emitter.instruction("test rax, rax"); // did the runtime string callback name match this userland target?
            emitter.instruction(&format!("je {}", matched_label)); // select this callable case when names match case-insensitively
        }
    }
}
