//! Purpose:
//! Named and spread argument binding for builtin calls.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Helpers are scoped to the eval interpreter and operate on already parsed
//!   EvalIR call metadata or evaluated runtime-cell handles.

use super::*;

/// Evaluates a direct PHP-visible builtin call with named or spread arguments.
pub(in crate::interpreter) fn eval_builtin_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    let evaluated_args = bind_evaluated_builtin_args(name, evaluated_args, values)?;
    let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? else {
        return Err(EvalStatus::UnsupportedConstruct);
    };
    Ok(result)
}

/// Binds evaluated builtin arguments to PHP parameter order when names are used.
pub(in crate::interpreter) fn bind_evaluated_builtin_args(
    name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if evaluated_args.iter().all(|arg| arg.name.is_none()) {
        return Ok(evaluated_args.into_iter().map(|arg| arg.value).collect());
    }

    let params = eval_builtin_param_names(name).ok_or(EvalStatus::RuntimeFatal)?;
    let mut bound_args = vec![None; params.len()];
    let mut next_positional = 0;

    for arg in evaluated_args {
        if let Some(name) = arg.name {
            bind_builtin_named_arg(params, &mut bound_args, &name, arg.value)?;
        } else {
            bind_dynamic_positional_arg(&mut bound_args, &mut next_positional, arg.value)?;
        }
    }

    collect_bound_builtin_args(name, bound_args, values)
}

/// Binds one named builtin-call value to the matching PHP parameter slot.
pub(in crate::interpreter) fn bind_builtin_named_arg(
    params: &[&str],
    bound_args: &mut [Option<RuntimeCellHandle>],
    name: &str,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    let Some(param_index) = params.iter().position(|param| *param == name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if bound_args[param_index].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    bound_args[param_index] = Some(value);
    Ok(())
}

/// Collects ordered builtin arguments, applying PHP defaults for named-call gaps.
pub(in crate::interpreter) fn collect_bound_builtin_args(
    name: &str,
    bound_args: Vec<Option<RuntimeCellHandle>>,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if !bound_args.iter().any(Option::is_some) {
        return Ok(Vec::new());
    }

    let shape = eval_builtin_signature_shape(name).ok_or(EvalStatus::RuntimeFatal)?;
    let last_index = bound_args
        .iter()
        .rposition(Option::is_some)
        .expect("non-empty bound args has a last supplied arg");
    let mut args = Vec::with_capacity(last_index + 1);

    for (index, arg) in bound_args.into_iter().take(last_index + 1).enumerate() {
        if let Some(value) = arg {
            args.push(value);
        } else if index >= shape.required_param_count {
            args.push(eval_builtin_default_arg(name, index, values)?);
        } else {
            return Err(EvalStatus::RuntimeFatal);
        }
    }

    Ok(args)
}

/// Materializes one builtin default argument as a runtime cell.
fn eval_builtin_default_arg(
    name: &str,
    index: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match eval_builtin_default_value(name, index).ok_or(EvalStatus::RuntimeFatal)? {
        EvalBuiltinDefaultValue::Null => values.null(),
        EvalBuiltinDefaultValue::Bool(value) => values.bool_value(value),
        EvalBuiltinDefaultValue::Int(value) => values.int(value),
        EvalBuiltinDefaultValue::Float(value) => values.float(value),
        EvalBuiltinDefaultValue::String(value) => values.string(value),
        EvalBuiltinDefaultValue::Bytes(value) => values.string_bytes_value(value),
        EvalBuiltinDefaultValue::EmptyArray => values.array_new(0),
    }
}

/// Returns PHP parameter names for builtin calls implemented by eval.
pub(in crate::interpreter) fn eval_builtin_param_names(
    name: &str,
) -> Option<&'static [&'static str]> {
    if let Some(params) = eval_declared_builtin_param_names(name) {
        return Some(params);
    }

    match name {
        "array_chunk" => Some(&["array", "length"]),
        "array_column" => Some(&["array", "column_key"]),
        "array_combine" => Some(&["keys", "values"]),
        "array_fill" => Some(&["start_index", "count", "value"]),
        "array_fill_keys" => Some(&["keys", "value"]),
        "array_filter" => Some(&["array", "callback", "mode"]),
        "array_map" => Some(&["callback", "array", "arrays"]),
        "array_reduce" => Some(&["array", "callback", "initial"]),
        "array_walk" => Some(&["array", "callback"]),
        "uasort" | "uksort" | "usort" => Some(&["array", "callback"]),
        "array_pop" | "array_shift" | "arsort" | "asort" | "krsort" | "ksort"
        | "natcasesort" | "natsort" | "rsort" | "shuffle" | "sort" => Some(&["array"]),
        "array_merge" => Some(&["arrays"]),
        "array_diff" | "array_intersect" | "array_diff_key" | "array_intersect_key" => {
            Some(&["array", "arrays"])
        }
        "array_push" | "array_unshift" => Some(&["array", "values"]),
        "array_splice" => Some(&["array", "offset", "length", "replacement"]),
        "empty" => Some(&["value"]),
        "is_callable" => Some(&["value", "syntax_only", "callable_name"]),
        "buffer_new" => Some(&["length"]),
        "buffer_len" | "buffer_free" => Some(&["buffer"]),
        "settype" => Some(&["var", "type"]),
        "get_called_class" => Some(&[]),
        "get_class" => Some(&["object"]),
        "get_class_methods" => Some(&["object_or_class"]),
        "get_class_vars" => Some(&["class"]),
        "get_object_vars" => Some(&["object"]),
        "get_parent_class" => Some(&["object_or_class"]),
        "call_user_func" => Some(&["callback", "args"]),
        "call_user_func_array" => Some(&["callback", "args"]),
        "class_alias" => Some(&["class", "alias", "autoload"]),
        "class_attribute_args" => Some(&["class_name", "attribute_name"]),
        "class_attribute_names" | "class_get_attributes" => Some(&["class_name"]),
        "class_exists" => Some(&["class", "autoload"]),
        "class_implements" | "class_parents" | "class_uses" => {
            Some(&["object_or_class", "autoload"])
        }
        "method_exists" => Some(&["object_or_class", "method"]),
        "property_exists" => Some(&["object_or_class", "property"]),
        "enum_exists" => Some(&["enum", "autoload"]),
        "interface_exists" => Some(&["interface", "autoload"]),
        "trait_exists" => Some(&["trait", "autoload"]),
        "is_a" | "is_subclass_of" => Some(&["object_or_class", "class", "allow_string"]),
        "define" => Some(&["constant_name", "value"]),
        "defined" => Some(&["constant_name"]),
        "die" | "exit" => Some(&["status"]),
        "exec" | "shell_exec" | "system" | "passthru" => Some(&["command"]),
        "fgetcsv" => Some(&["stream", "length", "separator"]),
        "fopen" => Some(&["filename", "mode", "use_include_path", "context"]),
        "fputcsv" => Some(&["stream", "fields", "separator", "enclosure"]),
        "fprintf" => Some(&["stream", "format", "values"]),
        "fsockopen" | "pfsockopen" => {
            Some(&["hostname", "port", "error_code", "error_message", "timeout"])
        }
        "flock" => Some(&["stream", "operation", "would_block"]),
        "fscanf" => Some(&["stream", "format", "vars"]),
        "function_exists" => Some(&["function"]),
        "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => Some(&[]),
        "gethostbyaddr" => Some(&["ip"]),
        "gethostbyname" => Some(&["hostname"]),
        "gethostname" => Some(&[]),
        "getprotobyname" => Some(&["protocol"]),
        "getprotobynumber" => Some(&["protocol"]),
        "getservbyname" => Some(&["service", "protocol"]),
        "getservbyport" => Some(&["port", "protocol"]),
        "get_resource_id" | "get_resource_type" => Some(&["resource"]),
        "getenv" => Some(&["name"]),
        "inet_ntop" => Some(&["ip"]),
        "inet_pton" => Some(&["ip"]),
        "iterator_apply" => Some(&["iterator", "callback", "args"]),
        "iterator_count" => Some(&["iterator"]),
        "iterator_to_array" => Some(&["iterator", "preserve_keys"]),
        "ip2long" => Some(&["ip"]),
        "isset" | "unset" => Some(&["var", "vars"]),
        "php_uname" => Some(&["mode"]),
        "phpversion" => Some(&[]),
        "ptr" => Some(&["value"]),
        "ptr_null" => Some(&[]),
        "ptr_is_null" | "ptr_get" | "ptr_read8" | "ptr_read16" | "ptr_read32" => {
            Some(&["pointer"])
        }
        "ptr_offset" => Some(&["pointer", "offset"]),
        "ptr_read_string" => Some(&["pointer", "length"]),
        "ptr_set" | "ptr_write8" | "ptr_write16" | "ptr_write32" => {
            Some(&["pointer", "value"])
        }
        "ptr_write_string" => Some(&["pointer", "string"]),
        "ptr_sizeof" => Some(&["type"]),
        "print_r" => Some(&["value", "return"]),
        "var_dump" => Some(&["value", "values"]),
        "putenv" => Some(&["assignment"]),
        "readline" => Some(&["prompt"]),
        "spl_autoload_register" => Some(&["callback", "throw", "prepend"]),
        "spl_autoload_unregister" => Some(&["callback"]),
        "spl_autoload_functions" | "spl_classes" => Some(&[]),
        "spl_autoload_extensions" => Some(&["file_extensions"]),
        "spl_autoload_call" => Some(&["class"]),
        "spl_autoload" => Some(&["class", "file_extensions"]),
        "spl_object_id" | "spl_object_hash" => Some(&["object"]),
        "stream_bucket_make_writeable" => Some(&["brigade"]),
        "stream_bucket_new" => Some(&["stream", "buffer"]),
        "stream_bucket_append" | "stream_bucket_prepend" => Some(&["brigade", "bucket"]),
        "stream_context_create" => Some(&["options", "params"]),
        "stream_context_get_default" => Some(&["options"]),
        "stream_context_get_options" | "stream_context_get_params" => Some(&["context"]),
        "stream_context_set_default" => Some(&["options"]),
        "stream_context_set_option" => {
            Some(&["context", "wrapper_or_options", "option_name", "value"])
        }
        "stream_context_set_params" => Some(&["context", "params"]),
        "stream_filter_register" => Some(&["filter_name", "class"]),
        "stream_filter_append" | "stream_filter_prepend" => {
            Some(&["stream", "filtername", "read_write", "params"])
        }
        "stream_filter_remove" => Some(&["stream_filter"]),
        "stream_select" => Some(&["read", "write", "except", "seconds", "microseconds"]),
        "stream_socket_server" | "stream_socket_client" => Some(&["address"]),
        "stream_socket_accept" => Some(&["socket", "timeout", "peer_name"]),
        "stream_socket_enable_crypto" => {
            Some(&["stream", "enable", "crypto_method", "session_stream"])
        }
        "stream_socket_get_name" => Some(&["socket", "remote"]),
        "stream_socket_pair" => Some(&["domain", "type", "protocol"]),
        "stream_socket_recvfrom" => Some(&["socket", "length", "flags", "address"]),
        "stream_socket_sendto" => Some(&["socket", "data", "flags", "address"]),
        "stream_socket_shutdown" => Some(&["stream", "mode"]),
        "stream_wrapper_register" => Some(&["protocol", "class", "flags"]),
        "stream_wrapper_unregister" | "stream_wrapper_restore" => Some(&["protocol"]),
        "long2ip" => Some(&["ip"]),
        "vfprintf" => Some(&["stream", "format", "values"]),
        _ => None,
    }
}
