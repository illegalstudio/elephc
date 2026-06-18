//! Purpose:
//! Named and spread argument binding for builtin calls.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Helpers are scoped to the eval interpreter and operate on already parsed
//!   EvalIR call metadata or evaluated runtime-cell handles.

use super::super::super::*;
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

/// Collects ordered bound arguments, rejecting gaps where defaults would be needed.
pub(in crate::interpreter) fn collect_contiguous_bound_args(
    bound_args: Vec<Option<RuntimeCellHandle>>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let Some(last_index) = bound_args.iter().rposition(Option::is_some) else {
        return Ok(Vec::new());
    };
    bound_args
        .into_iter()
        .take(last_index + 1)
        .collect::<Option<Vec<_>>>()
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Collects ordered builtin arguments, applying PHP defaults for special named-call gaps.
pub(in crate::interpreter) fn collect_bound_builtin_args(
    name: &str,
    mut bound_args: Vec<Option<RuntimeCellHandle>>,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if name == "array_splice" && bound_args.get(3).is_some_and(Option::is_some) {
        if bound_args.get(2).is_some_and(Option::is_none) {
            bound_args[2] = Some(values.null()?);
        }
    }
    collect_contiguous_bound_args(bound_args)
}

/// Returns PHP parameter names for builtin calls implemented by eval.
pub(in crate::interpreter) fn eval_builtin_param_names(
    name: &str,
) -> Option<&'static [&'static str]> {
    match name {
        "abs" | "ceil" | "floor" | "sqrt" => Some(&["num"]),
        "array_chunk" => Some(&["array", "length"]),
        "array_column" => Some(&["array", "column_key"]),
        "array_combine" => Some(&["keys", "values"]),
        "array_fill" => Some(&["start_index", "count", "value"]),
        "array_fill_keys" => Some(&["keys", "value"]),
        "array_filter" => Some(&["array", "callback", "mode"]),
        "array_map" => Some(&["callback", "array"]),
        "array_reduce" => Some(&["array", "callback", "initial"]),
        "array_walk" => Some(&["array", "callback"]),
        "uasort" | "uksort" | "usort" => Some(&["array", "callback"]),
        "array_flip" | "array_keys" | "array_pop" | "array_product" | "array_shift"
        | "array_sum" | "array_unique" | "array_rand" | "array_values" | "arsort" | "asort"
        | "krsort" | "ksort" | "natcasesort" | "natsort" | "rsort" | "shuffle" | "sort" => {
            Some(&["array"])
        }
        "array_push" | "array_unshift" => Some(&["array", "values"]),
        "array_key_exists" => Some(&["key", "array"]),
        "array_pad" => Some(&["array", "length", "value"]),
        "array_reverse" => Some(&["array", "preserve_keys"]),
        "array_search" | "in_array" => Some(&["needle", "haystack", "strict"]),
        "array_slice" => Some(&["array", "offset", "length"]),
        "array_splice" => Some(&["array", "offset", "length", "replacement"]),
        "acos" | "asin" | "atan" | "cos" | "cosh" | "deg2rad" | "exp" | "log2" | "log10"
        | "rad2deg" | "sin" | "sinh" | "tan" | "tanh" => Some(&["num"]),
        "atan2" => Some(&["y", "x"]),
        "basename" => Some(&["path", "suffix"]),
        "addslashes" | "base64_decode" | "base64_encode" | "bin2hex" | "grapheme_strrev"
        | "hex2bin" | "rawurldecode" | "rawurlencode" | "stripslashes" | "urldecode"
        | "urlencode" => Some(&["string"]),
        "boolval" | "floatval" | "gettype" | "intval" | "is_array" | "is_bool" | "is_double"
        | "is_finite" | "is_float" | "is_infinite" | "is_int" | "is_integer" | "is_iterable"
        | "is_long" | "is_nan" | "is_null" | "is_numeric" | "is_object" | "is_real"
        | "is_resource" | "is_string" | "is_callable" | "strval" => Some(&["value"]),
        "settype" => Some(&["var", "type"]),
        "get_class" => Some(&["object"]),
        "get_parent_class" => Some(&["object_or_class"]),
        "call_user_func" => Some(&["callback"]),
        "call_user_func_array" => Some(&["callback", "args"]),
        "class_alias" => Some(&["class", "alias", "autoload"]),
        "class_attribute_args" => Some(&["class_name", "attribute_name"]),
        "class_attribute_names" | "class_get_attributes" => Some(&["class_name"]),
        "class_exists" => Some(&["class", "autoload"]),
        "class_implements" | "class_parents" | "class_uses" => {
            Some(&["object_or_class", "autoload"])
        }
        "enum_exists" => Some(&["enum", "autoload"]),
        "interface_exists" => Some(&["interface", "autoload"]),
        "trait_exists" => Some(&["trait", "autoload"]),
        "is_a" | "is_subclass_of" => Some(&["object_or_class", "class", "allow_string"]),
        "chdir" | "mkdir" | "opendir" | "rmdir" | "scandir" => Some(&["directory"]),
        "chmod" => Some(&["filename", "permissions"]),
        "chr" => Some(&["codepoint"]),
        "closedir" | "readdir" | "rewinddir" => Some(&["dir_handle"]),
        "clamp" => Some(&["value", "min", "max"]),
        "clearstatcache" => Some(&["clear_realpath_cache", "filename"]),
        "chop" | "ltrim" | "rtrim" | "trim" => Some(&["string", "characters"]),
        "count" => Some(&["value", "mode"]),
        "copy" | "rename" => Some(&["from", "to"]),
        "crc32" => Some(&["string"]),
        "ctype_alnum" | "ctype_alpha" | "ctype_digit" | "ctype_space" => Some(&["text"]),
        "date" => Some(&["format", "timestamp"]),
        "define" => Some(&["constant_name", "value"]),
        "defined" => Some(&["constant_name"]),
        "dirname" => Some(&["path", "levels"]),
        "disk_free_space" | "disk_total_space" => Some(&["directory"]),
        "exec" | "shell_exec" | "system" | "passthru" => Some(&["command"]),
        "explode" => Some(&["separator", "string"]),
        "fdiv" | "fmod" => Some(&["num1", "num2"]),
        "fclose"
        | "fgetc"
        | "fgets"
        | "feof"
        | "fflush"
        | "fpassthru"
        | "fsync"
        | "fdatasync"
        | "ftell"
        | "rewind"
        | "fstat"
        | "stream_get_meta_data" => Some(&["stream"]),
        "fnmatch" => Some(&["pattern", "filename", "flags"]),
        "fgetcsv" => Some(&["stream", "length", "separator"]),
        "file" | "file_get_contents" | "file_exists" | "fileatime" | "filectime" | "filegroup"
        | "fileinode" | "filemtime" | "fileowner" | "fileperms" | "filesize" | "filetype"
        | "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable" | "is_writable"
        | "is_writeable" | "lstat" | "readfile" | "stat" | "unlink" => Some(&["filename"]),
        "file_put_contents" => Some(&["filename", "data"]),
        "fopen" => Some(&["filename", "mode", "use_include_path", "context"]),
        "fputcsv" => Some(&["stream", "fields", "separator", "enclosure"]),
        "fprintf" => Some(&["stream", "format", "values"]),
        "flock" => Some(&["stream", "operation", "would_block"]),
        "fread" => Some(&["stream", "length"]),
        "fscanf" => Some(&["stream", "format", "vars"]),
        "fseek" => Some(&["stream", "offset", "whence"]),
        "ftruncate" => Some(&["stream", "size"]),
        "fwrite" => Some(&["stream", "data"]),
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
        "getcwd" => Some(&[]),
        "getenv" => Some(&["name"]),
        "glob" => Some(&["pattern"]),
        "hash" => Some(&["algo", "data", "binary"]),
        "hash_algos" => Some(&[]),
        "hash_copy" => Some(&["context"]),
        "hash_equals" => Some(&["known_string", "user_string"]),
        "hash_file" => Some(&["algo", "filename", "binary"]),
        "hash_final" => Some(&["context", "binary"]),
        "hash_hmac" => Some(&["algo", "data", "key", "binary"]),
        "hash_init" => Some(&["algo"]),
        "hash_update" => Some(&["context", "data"]),
        "gzcompress" | "gzdeflate" => Some(&["data", "level"]),
        "gzinflate" | "gzuncompress" => Some(&["data", "max_length"]),
        "hypot" => Some(&["x", "y"]),
        "html_entity_decode" | "htmlentities" | "htmlspecialchars" => Some(&["string"]),
        "implode" => Some(&["separator", "array"]),
        "inet_ntop" => Some(&["ip"]),
        "inet_pton" => Some(&["ip"]),
        "intdiv" => Some(&["num1", "num2"]),
        "iterator_apply" => Some(&["iterator", "callback", "args"]),
        "iterator_count" => Some(&["iterator"]),
        "iterator_to_array" => Some(&["iterator", "preserve_keys"]),
        "ip2long" => Some(&["ip"]),
        "json_decode" => Some(&["json", "associative", "depth", "flags"]),
        "json_encode" => Some(&["value", "flags", "depth"]),
        "json_last_error" | "json_last_error_msg" => Some(&[]),
        "json_validate" => Some(&["json", "depth", "flags"]),
        "link" | "symlink" => Some(&["target", "link"]),
        "linkinfo" | "readlink" => Some(&["path"]),
        "log" => Some(&["num", "base"]),
        "max" | "min" => Some(&["value"]),
        "md5" | "sha1" => Some(&["string", "binary"]),
        "microtime" => Some(&["as_float"]),
        "mktime" => Some(&["hour", "minute", "second", "month", "day", "year"]),
        "nl2br" => Some(&["string", "use_xhtml"]),
        "number_format" => Some(&[
            "num",
            "decimals",
            "decimal_separator",
            "thousands_separator",
        ]),
        "ord" => Some(&["character"]),
        "pathinfo" => Some(&["path", "flags"]),
        "pi" => Some(&[]),
        "php_uname" => Some(&["mode"]),
        "phpversion" => Some(&[]),
        "pclose" => Some(&["handle"]),
        "popen" => Some(&["command", "mode"]),
        "pow" => Some(&["num", "exponent"]),
        "preg_match" => Some(&["pattern", "subject", "matches", "flags", "offset"]),
        "preg_match_all" => Some(&["pattern", "subject", "matches", "flags", "offset"]),
        "preg_replace" => Some(&["pattern", "replacement", "subject", "limit", "count"]),
        "preg_replace_callback" => Some(&["pattern", "callback", "subject", "limit", "count"]),
        "preg_split" => Some(&["pattern", "subject", "limit", "flags"]),
        "print_r" | "var_dump" => Some(&["value"]),
        "putenv" => Some(&["assignment"]),
        "rand" | "mt_rand" | "random_int" => Some(&["min", "max"]),
        "range" => Some(&["start", "end"]),
        "readline" => Some(&["prompt"]),
        "realpath" | "stream_resolve_include_path" => Some(&["path"]),
        "realpath_cache_get" | "realpath_cache_size" => Some(&[]),
        "round" => Some(&["num", "precision"]),
        "sleep" => Some(&["seconds"]),
        "spl_autoload_register" => Some(&["callback", "throw", "prepend"]),
        "spl_autoload_unregister" => Some(&["callback"]),
        "spl_autoload_functions" | "spl_classes" => Some(&[]),
        "spl_autoload_extensions" => Some(&["file_extensions"]),
        "spl_autoload_call" => Some(&["class"]),
        "spl_autoload" => Some(&["class", "file_extensions"]),
        "spl_object_id" | "spl_object_hash" => Some(&["object"]),
        "sscanf" => Some(&["string", "format", "vars"]),
        "sprintf" | "printf" => Some(&["format", "values"]),
        "stream_is_local" | "stream_isatty" | "stream_supports_lock" => Some(&["stream"]),
        "stream_bucket_make_writeable" => Some(&["brigade"]),
        "stream_bucket_new" => Some(&["stream", "buffer"]),
        "stream_bucket_append" | "stream_bucket_prepend" => Some(&["brigade", "bucket"]),
        "stream_copy_to_stream" => Some(&["from", "to", "length", "offset"]),
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
        "stream_get_contents" => Some(&["stream", "length", "offset"]),
        "stream_get_line" => Some(&["stream", "length", "ending"]),
        "stream_get_filters" | "stream_get_transports" | "stream_get_wrappers" => Some(&[]),
        "stream_set_blocking" => Some(&["stream", "enable"]),
        "stream_set_chunk_size" | "stream_set_read_buffer" | "stream_set_write_buffer" => {
            Some(&["stream", "size"])
        }
        "stream_set_timeout" => Some(&["stream", "seconds", "microseconds"]),
        "stream_wrapper_register" => Some(&["protocol", "class", "flags"]),
        "stream_wrapper_unregister" | "stream_wrapper_restore" => Some(&["protocol"]),
        "strcasecmp" | "strcmp" => Some(&["string1", "string2"]),
        "str_contains" | "str_ends_with" | "str_starts_with" => Some(&["haystack", "needle"]),
        "strtotime" => Some(&["datetime"]),
        "strstr" => Some(&["haystack", "needle", "before_needle"]),
        "str_pad" => Some(&["string", "length", "pad_string", "pad_type"]),
        "str_replace" | "str_ireplace" => Some(&["search", "replace", "subject"]),
        "strpos" | "strrpos" => Some(&["haystack", "needle", "offset"]),
        "str_repeat" => Some(&["string", "times"]),
        "str_split" => Some(&["string", "length"]),
        "substr" => Some(&["string", "offset", "length"]),
        "substr_replace" => Some(&["string", "replace", "offset", "length"]),
        "sys_get_temp_dir" | "time" | "tmpfile" => Some(&[]),
        "tempnam" => Some(&["directory", "prefix"]),
        "touch" => Some(&["filename", "mtime", "atime"]),
        "chown" | "lchown" => Some(&["filename", "user"]),
        "chgrp" | "lchgrp" => Some(&["filename", "group"]),
        "lcfirst" | "strlen" | "strrev" | "strtolower" | "strtoupper" | "ucfirst" => {
            Some(&["string"])
        }
        "long2ip" => Some(&["ip"]),
        "ucwords" => Some(&["string", "separators"]),
        "umask" => Some(&["mask"]),
        "usleep" => Some(&["microseconds"]),
        "vfprintf" => Some(&["stream", "format", "values"]),
        "vsprintf" | "vprintf" => Some(&["format", "values"]),
        "wordwrap" => Some(&["string", "width", "break", "cut_long_words"]),
        _ => None,
    }
}
