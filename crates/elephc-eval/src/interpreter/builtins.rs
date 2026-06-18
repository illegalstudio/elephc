//! Purpose:
//! Implements eval support for PHP-visible builtins and language-construct helpers.
//! This module owns builtin argument binding, direct builtin execution, callable
//! builtin dispatch, and per-domain helper routines.
//!
//! Called from:
//! - `crate::interpreter::eval_call()` and positional builtin dispatch paths.
//!
//! Key details:
//! - The module is a child of `interpreter` so it can reuse core EvalIR execution
//!   helpers without widening crate-level visibility.
//! - Runtime value creation and PHP coercions still flow through `RuntimeValueOps`.

use super::*;

/// Evaluates string-name function probes against eval and supported builtin tables.
pub(super) fn eval_builtin_function_probe(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\').to_ascii_lowercase();
    values.bool_value(eval_function_probe_exists(context, &name))
}

/// Evaluates `define(name, value)` for eval dynamic constant-name registration.
pub(super) fn eval_builtin_define(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    let defined = eval_define_name(name, value, context, values)?;
    values.bool_value(defined)
}

/// Evaluates `defined(name)` against eval dynamic constant names.
pub(super) fn eval_builtin_defined(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let exists = eval_defined_name(name, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `define(...)` from already materialized call arguments.
fn eval_define_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name, value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let defined = eval_define_name(*name, *value, context, values)?;
    values.bool_value(defined)
}

/// Evaluates `defined(...)` from already materialized call arguments.
fn eval_defined_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let exists = eval_defined_name(*name, context, values)?;
    values.bool_value(exists)
}

/// Normalizes and registers one eval dynamic constant name.
fn eval_define_name(
    name: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = eval_constant_name(name, values)?;
    if name.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if eval_predefined_constant_value(&name).is_some() || context.has_constant(&name) {
        values.warning(DEFINE_ALREADY_DEFINED_WARNING)?;
        return Ok(false);
    }
    let value = values.retain(value)?;
    if context.define_constant(&name, value) {
        Ok(true)
    } else {
        values.release(value)?;
        Ok(false)
    }
}

/// Normalizes and probes one eval dynamic constant name.
fn eval_defined_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = eval_constant_name(name, values)?;
    Ok(eval_predefined_constant_value(&name).is_some() || context.has_constant(&name))
}

/// Reads a PHP constant name from a runtime cell without changing case.
fn eval_constant_name(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = values.string_bytes(name)?;
    String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates `class_exists(...)` against dynamic and generated class-name tables.
pub(super) fn eval_builtin_class_exists(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = match args {
        [name] => eval_expr(name, context, scope, values)?,
        [name, autoload] => {
            let name = eval_expr(name, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            name
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_class_exists_name(name, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `class_exists(...)` from already materialized call arguments.
fn eval_class_exists_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [name] => eval_class_exists_name(*name, context, values)?,
        [name, _autoload] => eval_class_exists_name(*name, context, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP class-name cell and probes dynamic names before generated classes.
fn eval_class_exists_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\');
    if context.has_class(name) {
        return Ok(true);
    }
    values.class_exists(name)
}

/// Evaluates `interface_exists(...)` against generated interface-name metadata.
pub(super) fn eval_builtin_interface_exists(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = match args {
        [name] => eval_expr(name, context, scope, values)?,
        [name, autoload] => {
            let name = eval_expr(name, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            name
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_interface_exists_name(name, values)?;
    values.bool_value(exists)
}

/// Evaluates `interface_exists(...)` from already materialized call arguments.
fn eval_interface_exists_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [name] => eval_interface_exists_name(*name, values)?,
        [name, _autoload] => eval_interface_exists_name(*name, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP interface-name cell and probes generated interface metadata.
fn eval_interface_exists_name(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.interface_exists(name.trim_start_matches('\\'))
}

/// Evaluates `trait_exists(...)` and `enum_exists(...)` against generated metadata.
pub(super) fn eval_builtin_class_like_exists(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let symbol = match args {
        [symbol] => eval_expr(symbol, context, scope, values)?,
        [symbol, autoload] => {
            let symbol = eval_expr(symbol, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            symbol
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_class_like_exists_name(name, symbol, values)?;
    values.bool_value(exists)
}

/// Evaluates materialized `trait_exists(...)` or `enum_exists(...)` arguments.
fn eval_class_like_exists_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [symbol] => eval_class_like_exists_name(name, *symbol, values)?,
        [symbol, _autoload] => eval_class_like_exists_name(name, *symbol, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP class-like name cell and probes generated trait or enum metadata.
fn eval_class_like_exists_name(
    name: &str,
    symbol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let symbol = values.string_bytes(symbol)?;
    let symbol = String::from_utf8(symbol).map_err(|_| EvalStatus::RuntimeFatal)?;
    let symbol = symbol.trim_start_matches('\\');
    match name {
        "trait_exists" => values.trait_exists(symbol),
        "enum_exists" => values.enum_exists(symbol),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates `is_a(...)` and `is_subclass_of(...)` over eval boxed object cells.
pub(super) fn eval_builtin_is_a_relation(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_is_a_relation_result(name, &evaluated_args, context, values)
}

/// Evaluates materialized `is_a(...)` or `is_subclass_of(...)` builtin arguments.
fn eval_is_a_relation_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (object_or_class, target_class, allow_string) = match evaluated_args {
        [object_or_class, target_class] => {
            (*object_or_class, *target_class, name == "is_subclass_of")
        }
        [object_or_class, target_class, allow_string] => (
            *object_or_class,
            *target_class,
            values.truthy(*allow_string)?,
        ),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let target_class = values.string_bytes(target_class)?;
    let target_class = String::from_utf8(target_class).map_err(|_| EvalStatus::RuntimeFatal)?;
    let target_class = target_class.trim_start_matches('\\');
    let is_object = values.type_tag(object_or_class)? == 6;
    let result =
        if is_object && dynamic_object_is_a(object_or_class, target_class, context, values)? {
            !matches!(name, "is_subclass_of")
        } else if is_object || allow_string {
            values.object_is_a(object_or_class, target_class, name == "is_subclass_of")?
        } else {
            false
        };
    values.bool_value(result)
}

/// Returns whether an eval-created object matches a dynamic class name exactly.
fn dynamic_object_is_a(
    object: RuntimeCellHandle,
    target_class: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let identity = values.object_identity(object)?;
    Ok(context
        .dynamic_object_class(identity)
        .is_some_and(|class| class.name().eq_ignore_ascii_case(target_class)))
}

/// Evaluates PHP's `isset(...)` language construct over eval-visible values.
pub(super) fn eval_builtin_isset(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return values.bool_value(false);
    }
    for arg in args {
        if !eval_isset_arg(arg, context, scope, values)? {
            return values.bool_value(false);
        }
    }
    values.bool_value(true)
}

/// Evaluates PHP's `empty(...)` language construct over eval-visible values.
pub(super) fn eval_builtin_empty(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let empty = eval_empty_arg(arg, context, scope, values)?;
    values.bool_value(empty)
}

/// Evaluates one `empty` operand without warning or failing on missing variables.
fn eval_empty_arg(
    arg: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let EvalExpr::LoadVar(name) = arg {
        let Some(value) = visible_scope_cell(context, scope, name) else {
            return Ok(true);
        };
        return Ok(!values.truthy(value)?);
    }
    let value = eval_expr(arg, context, scope, values)?;
    Ok(!values.truthy(value)?)
}

/// Evaluates one `isset` operand without allocating a null cell for missing variables.
fn eval_isset_arg(
    arg: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let EvalExpr::LoadVar(name) = arg {
        let Some(value) = visible_scope_cell(context, scope, name) else {
            return Ok(false);
        };
        return Ok(!values.is_null(value)?);
    }
    let value = eval_expr(arg, context, scope, values)?;
    Ok(!values.is_null(value)?)
}

/// Returns true when a PHP function name is visible to eval builtin probes.
fn eval_function_probe_exists(context: &ElephcEvalContext, name: &str) -> bool {
    !name.contains("::") && (context.has_function(name) || eval_php_visible_builtin_exists(name))
}

/// Returns true for PHP-visible builtin names implemented by the eval interpreter.
pub(super) fn eval_php_visible_builtin_exists(name: &str) -> bool {
    matches!(
        name,
        "abs"
            | "addslashes"
            | "array_chunk"
            | "array_column"
            | "array_combine"
            | "array_fill"
            | "array_fill_keys"
            | "array_filter"
            | "array_flip"
            | "array_map"
            | "array_reduce"
            | "array_walk"
            | "array_key_exists"
            | "array_keys"
            | "array_diff"
            | "array_intersect"
            | "array_diff_key"
            | "array_intersect_key"
            | "array_merge"
            | "array_pad"
            | "array_pop"
            | "array_product"
            | "array_push"
            | "array_rand"
            | "array_reverse"
            | "array_search"
            | "array_shift"
            | "array_slice"
            | "array_splice"
            | "array_sum"
            | "array_unique"
            | "array_unshift"
            | "array_values"
            | "arsort"
            | "asort"
            | "acos"
            | "asin"
            | "atan"
            | "atan2"
            | "basename"
            | "base64_decode"
            | "base64_encode"
            | "bin2hex"
            | "ceil"
            | "chdir"
            | "chmod"
            | "call_user_func"
            | "call_user_func_array"
            | "class_exists"
            | "enum_exists"
            | "interface_exists"
            | "is_a"
            | "is_subclass_of"
            | "boolval"
            | "chop"
            | "chr"
            | "clamp"
            | "clearstatcache"
            | "count"
            | "copy"
            | "cos"
            | "cosh"
            | "crc32"
            | "ctype_alnum"
            | "ctype_alpha"
            | "ctype_digit"
            | "ctype_space"
            | "date"
            | "define"
            | "defined"
            | "deg2rad"
            | "dirname"
            | "disk_free_space"
            | "disk_total_space"
            | "exp"
            | "explode"
            | "fdiv"
            | "file"
            | "file_exists"
            | "fileatime"
            | "filectime"
            | "filegroup"
            | "file_get_contents"
            | "fileinode"
            | "filemtime"
            | "fileowner"
            | "fileperms"
            | "file_put_contents"
            | "filesize"
            | "filetype"
            | "fnmatch"
            | "floor"
            | "floatval"
            | "fmod"
            | "function_exists"
            | "gethostbyaddr"
            | "gethostbyname"
            | "gethostname"
            | "getprotobyname"
            | "getprotobynumber"
            | "getservbyname"
            | "getservbyport"
            | "get_class"
            | "get_parent_class"
            | "get_resource_id"
            | "get_resource_type"
            | "getcwd"
            | "getenv"
            | "gettype"
            | "glob"
            | "hash"
            | "hash_algos"
            | "hash_equals"
            | "hash_file"
            | "hash_hmac"
            | "hex2bin"
            | "html_entity_decode"
            | "htmlentities"
            | "htmlspecialchars"
            | "hypot"
            | "implode"
            | "in_array"
            | "inet_ntop"
            | "inet_pton"
            | "intdiv"
            | "ip2long"
            | "is_dir"
            | "is_executable"
            | "is_file"
            | "is_link"
            | "is_readable"
            | "is_writable"
            | "is_writeable"
            | "intval"
            | "link"
            | "linkinfo"
            | "ltrim"
            | "is_callable"
            | "is_array"
            | "is_bool"
            | "is_double"
            | "is_finite"
            | "is_float"
            | "is_infinite"
            | "is_int"
            | "is_integer"
            | "is_iterable"
            | "is_long"
            | "is_nan"
            | "is_null"
            | "is_numeric"
            | "is_object"
            | "is_real"
            | "is_resource"
            | "is_string"
            | "iterator_apply"
            | "iterator_count"
            | "iterator_to_array"
            | "json_decode"
            | "json_encode"
            | "json_last_error"
            | "json_last_error_msg"
            | "json_validate"
            | "krsort"
            | "ksort"
            | "lcfirst"
            | "log"
            | "log2"
            | "log10"
            | "long2ip"
            | "max"
            | "md5"
            | "microtime"
            | "min"
            | "mkdir"
            | "mktime"
            | "mt_rand"
            | "natcasesort"
            | "natsort"
            | "nl2br"
            | "number_format"
            | "ord"
            | "pathinfo"
            | "pi"
            | "pow"
            | "php_uname"
            | "phpversion"
            | "preg_match"
            | "preg_match_all"
            | "preg_replace"
            | "preg_replace_callback"
            | "preg_split"
            | "putenv"
            | "print_r"
            | "rand"
            | "random_int"
            | "range"
            | "rad2deg"
            | "rawurldecode"
            | "rawurlencode"
            | "readfile"
            | "readlink"
            | "realpath"
            | "realpath_cache_get"
            | "realpath_cache_size"
            | "rename"
            | "rsort"
            | "rtrim"
            | "round"
            | "rmdir"
            | "scandir"
            | "settype"
            | "sleep"
            | "sha1"
            | "shuffle"
            | "sin"
            | "sinh"
            | "sort"
            | "sqrt"
            | "spl_classes"
            | "spl_object_hash"
            | "spl_object_id"
            | "sscanf"
            | "sprintf"
            | "strcasecmp"
            | "stream_get_filters"
            | "stream_get_transports"
            | "stream_get_wrappers"
            | "str_contains"
            | "str_ends_with"
            | "str_ireplace"
            | "str_repeat"
            | "str_replace"
            | "str_starts_with"
            | "strcmp"
            | "stat"
            | "strlen"
            | "strpos"
            | "strrpos"
            | "strrev"
            | "str_pad"
            | "str_split"
            | "strstr"
            | "strtotime"
            | "substr"
            | "stripslashes"
            | "strtolower"
            | "strtoupper"
            | "strval"
            | "symlink"
            | "sys_get_temp_dir"
            | "tempnam"
            | "tan"
            | "tanh"
            | "time"
            | "touch"
            | "trait_exists"
            | "trim"
            | "substr_replace"
            | "ucfirst"
            | "ucwords"
            | "uasort"
            | "uksort"
            | "unlink"
            | "umask"
            | "urldecode"
            | "urlencode"
            | "usort"
            | "usleep"
            | "var_dump"
            | "printf"
            | "vprintf"
            | "vsprintf"
            | "wordwrap"
            | "lstat"
    )
}

/// Evaluates a direct PHP-visible builtin call with named or spread arguments.
pub(super) fn eval_builtin_call(
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
fn bind_evaluated_builtin_args(
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
fn bind_builtin_named_arg(
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
fn collect_contiguous_bound_args(
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
fn collect_bound_builtin_args(
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
fn eval_builtin_param_names(name: &str) -> Option<&'static [&'static str]> {
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
        "addslashes" | "base64_decode" | "base64_encode" | "bin2hex" | "hex2bin"
        | "rawurldecode" | "rawurlencode" | "stripslashes" | "urldecode" | "urlencode" => {
            Some(&["string"])
        }
        "boolval" | "floatval" | "gettype" | "intval" | "is_array" | "is_bool" | "is_double"
        | "is_finite" | "is_float" | "is_infinite" | "is_int" | "is_integer" | "is_iterable"
        | "is_long" | "is_nan" | "is_null" | "is_numeric" | "is_object" | "is_real"
        | "is_resource" | "is_string" | "is_callable" | "strval" => Some(&["value"]),
        "settype" => Some(&["var", "type"]),
        "get_class" => Some(&["object"]),
        "get_parent_class" => Some(&["object_or_class"]),
        "call_user_func" => Some(&["callback"]),
        "call_user_func_array" => Some(&["callback", "args"]),
        "class_exists" => Some(&["class", "autoload"]),
        "enum_exists" => Some(&["enum", "autoload"]),
        "interface_exists" => Some(&["interface", "autoload"]),
        "trait_exists" => Some(&["trait", "autoload"]),
        "is_a" | "is_subclass_of" => Some(&["object_or_class", "class", "allow_string"]),
        "chdir" | "mkdir" | "rmdir" | "scandir" => Some(&["directory"]),
        "chmod" => Some(&["filename", "permissions"]),
        "chr" => Some(&["codepoint"]),
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
        "explode" => Some(&["separator", "string"]),
        "fdiv" | "fmod" => Some(&["num1", "num2"]),
        "fnmatch" => Some(&["pattern", "filename", "flags"]),
        "file" | "file_get_contents" | "file_exists" | "fileatime" | "filectime" | "filegroup"
        | "fileinode" | "filemtime" | "fileowner" | "fileperms" | "filesize" | "filetype"
        | "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable" | "is_writable"
        | "is_writeable" | "lstat" | "readfile" | "stat" | "unlink" => Some(&["filename"]),
        "file_put_contents" => Some(&["filename", "data"]),
        "function_exists" => Some(&["function"]),
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
        "hash_equals" => Some(&["known_string", "user_string"]),
        "hash_file" => Some(&["algo", "filename", "binary"]),
        "hash_hmac" => Some(&["algo", "data", "key", "binary"]),
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
        "realpath" => Some(&["path"]),
        "realpath_cache_get" | "realpath_cache_size" => Some(&[]),
        "round" => Some(&["num", "precision"]),
        "sleep" => Some(&["seconds"]),
        "spl_classes" => Some(&[]),
        "spl_object_id" | "spl_object_hash" => Some(&["object"]),
        "sscanf" => Some(&["string", "format", "vars"]),
        "sprintf" | "printf" => Some(&["format", "values"]),
        "stream_get_filters" | "stream_get_transports" | "stream_get_wrappers" => Some(&[]),
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
        "sys_get_temp_dir" | "time" => Some(&[]),
        "tempnam" => Some(&["directory", "prefix"]),
        "touch" => Some(&["filename", "mtime", "atime"]),
        "lcfirst" | "strlen" | "strrev" | "strtolower" | "strtoupper" | "ucfirst" => {
            Some(&["string"])
        }
        "long2ip" => Some(&["ip"]),
        "ucwords" => Some(&["string", "separators"]),
        "umask" => Some(&["mask"]),
        "usleep" => Some(&["microseconds"]),
        "vsprintf" | "vprintf" => Some(&["format", "values"]),
        "wordwrap" => Some(&["string", "width", "break", "cut_long_words"]),
        _ => None,
    }
}

/// Evaluates `call_user_func($name, ...$args)` inside a runtime eval fragment.
pub(super) fn eval_builtin_call_user_func(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_call_user_func_with_values(evaluated_args, context, values)
}

/// Evaluates `call_user_func_array($name, $args)` inside a runtime eval fragment.
pub(super) fn eval_builtin_call_user_func_array(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [callback, arg_array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback = eval_expr(callback, context, scope, values)?;
    let arg_array = eval_expr(arg_array, context, scope, values)?;
    eval_call_user_func_array_with_values(callback, arg_array, context, values)
}

/// Dispatches `call_user_func_array` after callback and array arguments are evaluated.
fn eval_call_user_func_array_with_values(
    callback: RuntimeCellHandle,
    arg_array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_callable(callback, values)?;
    if !values.is_array_like(arg_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let evaluated_args = eval_array_call_arg_values(arg_array, values)?;
    eval_evaluated_callable_with_call_array_args(&callback, evaluated_args, context, values)
}

/// Dispatches `call_user_func` after its callback and arguments are already evaluated.
fn eval_call_user_func_with_values(
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((callback, callback_args)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback = eval_callable(*callback, values)?;
    eval_evaluated_callable_with_values(&callback, callback_args.to_vec(), context, values)
}

/// Normalizes one PHP callback value for eval dynamic callable dispatch.
pub(super) fn eval_callable(
    callback: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    if values.is_array_like(callback)? {
        return eval_array_callable(callback, values);
    }
    eval_callable_name(callback, values).map(EvaluatedCallable::Named)
}

/// Normalizes one two-element object-method callable array.
fn eval_array_callable(
    callback: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    if values.array_len(callback)? != 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let zero = values.int(0)?;
    let one = values.int(1)?;
    let object = values.array_get(callback, zero)?;
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let method = values.array_get(callback, one)?;
    let method =
        String::from_utf8(values.string_bytes(method)?).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(EvaluatedCallable::ObjectMethod { object, method })
}

/// Normalizes one string callback name for eval dynamic callable dispatch.
fn eval_callable_name(
    callback: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let callback = values.string_bytes(callback)?;
    let callback = String::from_utf8(callback).map_err(|_| EvalStatus::RuntimeFatal)?;
    let callback = callback.trim_start_matches('\\').to_ascii_lowercase();
    if callback.contains("::") {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    Ok(callback)
}

/// Invokes an already normalized callback with source-order positional values.
fn eval_evaluated_callable_with_values(
    callback: &EvaluatedCallable,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match callback {
        EvaluatedCallable::Named(name) => {
            eval_callable_with_values(name, evaluated_args, context, values)
        }
        EvaluatedCallable::ObjectMethod { object, method } => {
            eval_method_call_result(*object, method, evaluated_args, context, values)
        }
    }
}

/// Invokes an already normalized callback with optional named-argument metadata.
pub(super) fn eval_evaluated_callable_with_call_array_args(
    callback: &EvaluatedCallable,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match callback {
        EvaluatedCallable::Named(name) => {
            eval_callable_with_call_array_args(name, evaluated_args, context, values)
        }
        EvaluatedCallable::ObjectMethod { object, method } => {
            if evaluated_args.iter().any(|arg| arg.name.is_some()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            let evaluated_args = evaluated_args.into_iter().map(|arg| arg.value).collect();
            eval_method_call_result(*object, method, evaluated_args, context, values)
        }
    }
}

/// Invokes a PHP-visible callable name with source-order positional values.
fn eval_callable_with_values(
    name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? {
        return Ok(result);
    }
    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function_with_values(&function, evaluated_args, context, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function_with_values(function, evaluated_args, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Invokes a callable with arguments that may carry `call_user_func_array` names.
pub(super) fn eval_callable_with_call_array_args(
    name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.iter().all(|arg| arg.name.is_none()) {
        let evaluated_args = evaluated_args.into_iter().map(|arg| arg.value).collect();
        return eval_callable_with_values(name, evaluated_args, context, values);
    }
    if eval_php_visible_builtin_exists(name) {
        let evaluated_args = bind_evaluated_builtin_args(name, evaluated_args, values)?;
        let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        return Ok(result);
    }
    if let Some(function) = context.function(name).cloned() {
        let evaluated_args = bind_evaluated_function_args(function.params(), evaluated_args)?;
        return eval_dynamic_function_with_values(&function, evaluated_args, context, values);
    }
    if let Some(function) = context.native_function(name) {
        if function.param_names().len() != function.param_count() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let evaluated_args = bind_evaluated_function_args(function.param_names(), evaluated_args)?;
        return eval_native_function_with_values(function, evaluated_args, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Evaluates PHP-visible builtins when they are invoked through a dynamic callable name.
fn eval_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "abs" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.abs(*value)?
        }
        "addslashes" | "stripslashes" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_slashes_result(name, *value, values)?
        }
        "array_combine" => {
            let [keys, values_array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_combine_result(*keys, *values_array, values)?
        }
        "array_column" => {
            let [array, column_key] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_column_result(*array, *column_key, values)?
        }
        "array_chunk" => {
            let [array, length] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_chunk_result(*array, *length, values)?
        }
        "array_fill" => {
            let [start, count, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_fill_result(*start, *count, *value, values)?
        }
        "array_fill_keys" => {
            let [keys, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_fill_keys_result(*keys, *value, values)?
        }
        "array_filter" => match evaluated_args {
            [array] => eval_array_filter_result(*array, None, None, context, values)?,
            [array, callback] => {
                eval_array_filter_result(*array, Some(*callback), None, context, values)?
            }
            [array, callback, mode] => {
                eval_array_filter_result(*array, Some(*callback), Some(*mode), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_map" => {
            let Some((callback, arrays)) = evaluated_args.split_first() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_map_result(*callback, arrays, context, values)?
        }
        "array_reduce" => match evaluated_args {
            [array, callback] => {
                let initial = values.null()?;
                eval_array_reduce_result(*array, *callback, initial, context, values)?
            }
            [array, callback, initial] => {
                eval_array_reduce_result(*array, *callback, *initial, context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_walk" => {
            let [array, callback] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_walk_result(*array, *callback, context, values)?
        }
        "array_pop" | "array_shift" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(&format!(
                "{name}(): Argument #1 ($array) must be passed by reference, value given"
            ))?;
            eval_array_pop_shift_value_result(name, *array, values)?
        }
        "array_push" | "array_unshift" => {
            let Some((array, inserted)) = evaluated_args.split_first() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(&format!(
                "{name}(): Argument #1 ($array) must be passed by reference, value given"
            ))?;
            eval_array_push_unshift_count_result(*array, inserted.len(), values)?
        }
        "array_splice" => {
            let result = match evaluated_args {
                [array, offset] => eval_array_splice_value_result(*array, *offset, None, values)?,
                [array, offset, length] => {
                    eval_array_splice_value_result(*array, *offset, Some(*length), values)?
                }
                [array, offset, length, _replacement] => {
                    eval_array_splice_value_result(*array, *offset, Some(*length), values)?
                }
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            values.warning(
                "array_splice(): Argument #1 ($array) must be passed by reference, value given",
            )?;
            result
        }
        "arsort" | "asort" | "krsort" | "ksort" | "natcasesort" | "natsort" | "rsort"
        | "shuffle" | "sort" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(&format!(
                "{name}(): Argument #1 ($array) must be passed by reference, value given"
            ))?;
            eval_array_sort_value_result(*array, values)?
        }
        "uasort" | "uksort" | "usort" => {
            let [array, callback] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(&format!(
                "{name}(): Argument #1 ($array) must be passed by reference, value given"
            ))?;
            eval_user_sort_value_result(name, *array, *callback, context, values)?
        }
        "array_flip" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_flip_result(*array, values)?
        }
        "array_pad" => {
            let [array, length, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_pad_result(*array, *length, *value, values)?
        }
        "array_product" | "array_sum" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_aggregate_result(name, *array, values)?
        }
        "array_keys" | "array_values" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_projection_result(name, *array, values)?
        }
        "array_key_exists" => {
            let [key, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.array_key_exists(*key, *array)?
        }
        "array_diff" | "array_intersect" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_value_set_result(name, *left, *right, values)?
        }
        "array_diff_key" | "array_intersect_key" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_key_set_result(name, *left, *right, values)?
        }
        "array_merge" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_merge_result(*left, *right, values)?
        }
        "array_rand" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_rand_result(*array, values)?
        }
        "array_reverse" => match evaluated_args {
            [array] => eval_array_reverse_result(*array, false, values)?,
            [array, preserve_keys] => {
                let preserve_keys = values.truthy(*preserve_keys)?;
                eval_array_reverse_result(*array, preserve_keys, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_search" | "in_array" => {
            let [needle, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_search_result(name, *needle, *array, values)?
        }
        "array_slice" => match evaluated_args {
            [array, offset] => eval_array_slice_result(*array, *offset, None, values)?,
            [array, offset, length] => {
                eval_array_slice_result(*array, *offset, Some(*length), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_unique" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_unique_result(*array, values)?
        }
        "range" => {
            let [start, end] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_range_result(*start, *end, values)?
        }
        "base64_encode" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_base64_encode_result(*value, values)?
        }
        "base64_decode" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_base64_decode_result(*value, values)?
        }
        "acos" | "asin" | "atan" | "cos" | "cosh" | "deg2rad" | "exp" | "log2" | "log10"
        | "rad2deg" | "sin" | "sinh" | "tan" | "tanh" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_unary_result(name, *value, values)?
        }
        "atan2" | "hypot" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_pair_result(name, *left, *right, values)?
        }
        "bin2hex" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_bin2hex_result(*value, values)?
        }
        "ceil" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.ceil(*value)?
        }
        "chr" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_chr_result(*value, values)?
        }
        "chdir" | "mkdir" | "rmdir" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_unary_path_bool_result(name, *path, values)?
        }
        "chmod" => {
            let [filename, permissions] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_chmod_result(*filename, *permissions, values)?
        }
        "clearstatcache" => {
            if evaluated_args.len() > 2 {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.null()?
        }
        "clamp" => {
            let [value, min, max] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_clamp_result(*value, *min, *max, values)?
        }
        "copy" | "link" | "rename" | "symlink" => {
            let [from, to] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_binary_path_bool_result(name, *from, *to, values)?
        }
        "floor" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.floor(*value)?
        }
        "fdiv" | "fmod" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_binary_result(name, *left, *right, values)?
        }
        "file" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_result(*filename, values)?
        }
        "file_exists" | "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable"
        | "is_writable" | "is_writeable" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_probe_result(name, *filename, values)?
        }
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_stat_scalar_result(name, *filename, values)?
        }
        "file_get_contents" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_get_contents_result(*filename, values)?
        }
        "file_put_contents" => {
            let [filename, data] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_put_contents_result(*filename, *data, values)?
        }
        "filesize" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_filesize_result(*filename, values)?
        }
        "filetype" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_filetype_result(*filename, values)?
        }
        "fnmatch" => match evaluated_args {
            [pattern, filename] => eval_fnmatch_result(*pattern, *filename, None, values)?,
            [pattern, filename, flags] => {
                eval_fnmatch_result(*pattern, *filename, Some(*flags), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stat" | "lstat" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stat_array_result(name, *filename, values)?
        }
        "linkinfo" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_linkinfo_result(*path, values)?
        }
        "log" => match evaluated_args {
            [num] => eval_log_result(*num, None, values)?,
            [num, base] => eval_log_result(*num, Some(*base), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "readfile" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_readfile_result(*filename, values)?
        }
        "pi" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.float(std::f64::consts::PI)?
        }
        "php_uname" => match evaluated_args {
            [] => eval_php_uname_result(None, values)?,
            [mode] => eval_php_uname_result(Some(*mode), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "pow" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.pow(*left, *right)?
        }
        "preg_match" => match evaluated_args {
            [pattern, subject] => eval_preg_match_result(*pattern, *subject, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "preg_match_all" => match evaluated_args {
            [pattern, subject] => eval_preg_match_all_result(*pattern, *subject, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "preg_replace" => match evaluated_args {
            [pattern, replacement, subject] => {
                eval_preg_replace_result(*pattern, *replacement, *subject, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "preg_replace_callback" => match evaluated_args {
            [pattern, callback, subject] => {
                eval_preg_replace_callback_result(*pattern, *callback, *subject, context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "preg_split" => match evaluated_args {
            [pattern, subject] => eval_preg_split_result(*pattern, *subject, None, None, values)?,
            [pattern, subject, limit] => {
                eval_preg_split_result(*pattern, *subject, Some(*limit), None, values)?
            }
            [pattern, subject, limit, flags] => {
                eval_preg_split_result(*pattern, *subject, Some(*limit), Some(*flags), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "print_r" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_print_r_result(*value, values)?
        }
        "var_dump" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_var_dump_result(*value, values)?
        }
        "rand" | "mt_rand" => match evaluated_args {
            [] => eval_rand_result(None, None, values)?,
            [min, max] => eval_rand_result(Some(*min), Some(*max), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "random_int" => {
            let [min, max] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_random_int_result(*min, *max, values)?
        }
        "rawurldecode" | "urldecode" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_url_decode_result(name, *value, values)?
        }
        "rawurlencode" | "urlencode" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_url_encode_result(name, *value, values)?
        }
        "round" => match evaluated_args {
            [value] => values.round(*value, None)?,
            [value, precision] => values.round(*value, Some(*precision))?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "scandir" => {
            let [directory] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_scandir_result(*directory, values)?
        }
        "sqrt" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.sqrt(*value)?
        }
        "spl_classes" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_spl_classes_result(values)?
        }
        "spl_object_id" | "spl_object_hash" => {
            let [object] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_spl_object_identity_result(name, *object, values)?
        }
        "sscanf" => {
            let [input, format, ..] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_sscanf_result(*input, *format, values)?
        }
        "sprintf" | "printf" => eval_sprintf_like_result(name, evaluated_args, values)?,
        "settype" => {
            let [value, type_name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_settype_value_result(*value, *type_name, values)?
        }
        "strrev" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.strrev(*value)?
        }
        "str_repeat" => {
            let [value, times] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_str_repeat_result(*value, *times, values)?
        }
        "str_replace" | "str_ireplace" => {
            let [search, replace, subject] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_str_replace_result(name, *search, *replace, *subject, values)?
        }
        "str_pad" => match evaluated_args {
            [value, length] => eval_str_pad_result(*value, *length, None, None, values)?,
            [value, length, pad_string] => {
                eval_str_pad_result(*value, *length, Some(*pad_string), None, values)?
            }
            [value, length, pad_string, pad_type] => {
                eval_str_pad_result(*value, *length, Some(*pad_string), Some(*pad_type), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "str_split" => match evaluated_args {
            [value] => eval_str_split_result(*value, None, values)?,
            [value, length] => eval_str_split_result(*value, Some(*length), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "substr" => match evaluated_args {
            [value, offset] => eval_substr_result(*value, *offset, None, values)?,
            [value, offset, length] => eval_substr_result(*value, *offset, Some(*length), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "substr_replace" => match evaluated_args {
            [value, replace, offset] => {
                eval_substr_replace_result(*value, *replace, *offset, None, values)?
            }
            [value, replace, offset, length] => {
                eval_substr_replace_result(*value, *replace, *offset, Some(*length), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "call_user_func" => {
            return eval_call_user_func_with_values(evaluated_args.to_vec(), context, values)
                .map(Some);
        }
        "call_user_func_array" => {
            let [callback, arg_array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            return eval_call_user_func_array_with_values(*callback, *arg_array, context, values)
                .map(Some);
        }
        "boolval" | "floatval" | "intval" | "strval" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_cast_result(name, *value, values)?
        }
        "count" => match evaluated_args {
            [value] => eval_count_result(*value, None, values)?,
            [value, mode] => eval_count_result(*value, Some(*mode), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "crc32" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_crc32_result(*value, values)?
        }
        "ctype_alnum" | "ctype_alpha" | "ctype_digit" | "ctype_space" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ctype_result(name, *value, values)?
        }
        "date" => match evaluated_args {
            [format] => eval_date_result(*format, None, values)?,
            [format, timestamp] => eval_date_result(*format, Some(*timestamp), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "define" => eval_define_result(evaluated_args, context, values)?,
        "defined" => eval_defined_result(evaluated_args, context, values)?,
        "explode" => {
            let [separator, string] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_explode_result(*separator, *string, values)?
        }
        "ord" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ord_result(*value, values)?
        }
        "implode" => {
            let [separator, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_implode_result(*separator, *array, values)?
        }
        "max" | "min" => eval_min_max_result(name, evaluated_args, values)?,
        "microtime" => match evaluated_args {
            [] | [_] => eval_microtime_result(values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "mktime" => {
            let [hour, minute, second, month, day, year] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_mktime_result(*hour, *minute, *second, *month, *day, *year, values)?
        }
        "nl2br" => match evaluated_args {
            [value] => eval_nl2br_result(*value, true, values)?,
            [value, use_xhtml] => {
                let use_xhtml = values.truthy(*use_xhtml)?;
                eval_nl2br_result(*value, use_xhtml, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "number_format" => match evaluated_args {
            [value] => eval_number_format_result(*value, None, None, None, values)?,
            [value, decimals] => {
                eval_number_format_result(*value, Some(*decimals), None, None, values)?
            }
            [value, decimals, decimal_separator] => eval_number_format_result(
                *value,
                Some(*decimals),
                Some(*decimal_separator),
                None,
                values,
            )?,
            [value, decimals, decimal_separator, thousands_separator] => eval_number_format_result(
                *value,
                Some(*decimals),
                Some(*decimal_separator),
                Some(*thousands_separator),
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "basename" => match evaluated_args {
            [path] => eval_basename_result(*path, None, values)?,
            [path, suffix] => eval_basename_result(*path, Some(*suffix), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "dirname" => match evaluated_args {
            [path] => eval_dirname_result(*path, None, values)?,
            [path, levels] => eval_dirname_result(*path, Some(*levels), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "disk_free_space" | "disk_total_space" => {
            let [directory] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_disk_space_result(name, *directory, values)?
        }
        "trim" | "ltrim" | "rtrim" | "chop" => match evaluated_args {
            [value] => eval_trim_like_result(name, *value, None, values)?,
            [value, mask] => eval_trim_like_result(name, *value, Some(*mask), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "function_exists" | "is_callable" => {
            let [name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let name = values.string_bytes(*name)?;
            let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
            let name = name.trim_start_matches('\\').to_ascii_lowercase();
            values.bool_value(eval_function_probe_exists(context, &name))?
        }
        "class_exists" => eval_class_exists_result(evaluated_args, context, values)?,
        "enum_exists" | "trait_exists" => {
            eval_class_like_exists_result(name, evaluated_args, values)?
        }
        "interface_exists" => eval_interface_exists_result(evaluated_args, values)?,
        "is_a" | "is_subclass_of" => {
            eval_is_a_relation_result(name, evaluated_args, context, values)?
        }
        "json_decode" => match evaluated_args {
            [json] => eval_json_decode_result(*json, None, None, None, context, values)?,
            [json, associative] => {
                eval_json_decode_result(*json, Some(*associative), None, None, context, values)?
            }
            [json, associative, depth] => eval_json_decode_result(
                *json,
                Some(*associative),
                Some(*depth),
                None,
                context,
                values,
            )?,
            [json, associative, depth, flags] => eval_json_decode_result(
                *json,
                Some(*associative),
                Some(*depth),
                Some(*flags),
                context,
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "json_encode" => match evaluated_args {
            [value] => eval_json_encode_result(*value, None, None, context, values)?,
            [value, flags] => eval_json_encode_result(*value, Some(*flags), None, context, values)?,
            [value, flags, depth] => {
                eval_json_encode_result(*value, Some(*flags), Some(*depth), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "json_last_error" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.int(context.json_last_error())?
        }
        "json_last_error_msg" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.string(context.json_last_error_msg())?
        }
        "json_validate" => match evaluated_args {
            [json] => eval_json_validate_result(*json, None, None, context, values)?,
            [json, depth] => eval_json_validate_result(*json, Some(*depth), None, context, values)?,
            [json, depth, flags] => {
                eval_json_validate_result(*json, Some(*depth), Some(*flags), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "gethostbyaddr" => {
            let [ip] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gethostbyaddr_result(*ip, values)?
        }
        "gethostbyname" => {
            let [hostname] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gethostbyname_result(*hostname, values)?
        }
        "gethostname" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_gethostname_result(values)?
        }
        "getprotobyname" => {
            let [protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getprotobyname_result(*protocol, values)?
        }
        "getprotobynumber" => {
            let [protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getprotobynumber_result(*protocol, values)?
        }
        "getservbyname" => {
            let [service, protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getservbyname_result(*service, *protocol, values)?
        }
        "getservbyport" => {
            let [port, protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getservbyport_result(*port, *protocol, values)?
        }
        "getcwd" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_getcwd_result(values)?
        }
        "getenv" => {
            let [name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getenv_result(*name, values)?
        }
        "get_class" => {
            let [object] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_get_class_result(*object, context, values)?
        }
        "get_parent_class" => {
            let [object_or_class] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_get_parent_class_result(*object_or_class, values)?
        }
        "get_resource_id" | "get_resource_type" => {
            let [resource] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_resource_introspection_result(name, *resource, values)?
        }
        "gettype" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gettype_result(*value, values)?
        }
        "glob" => {
            let [pattern] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_glob_result(*pattern, values)?
        }
        "hash" | "hash_file" | "hash_hmac" | "md5" | "sha1" => {
            eval_hash_one_shot_result(name, evaluated_args, values)?
        }
        "hash_algos" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_hash_algos_result(values)?
        }
        "hash_equals" => {
            let [known, user] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hash_equals_result(*known, *user, values)?
        }
        "hex2bin" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hex2bin_result(*value, values)?
        }
        "html_entity_decode" | "htmlentities" | "htmlspecialchars" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_html_entity_result(name, *value, values)?
        }
        "inet_ntop" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_inet_ntop_result(*value, values)?
        }
        "inet_pton" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_inet_pton_result(*value, values)?
        }
        "intdiv" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_intdiv_result(*left, *right, values)?
        }
        "iterator_apply" => match evaluated_args {
            [iterator, callback] => {
                let callback = eval_callable(*callback, values)?;
                eval_iterator_apply_result(*iterator, &callback, Vec::new(), context, values)?
            }
            [iterator, callback, args] => {
                let callback = eval_callable(*callback, values)?;
                let callback_args = eval_iterator_apply_arg_values(*args, values)?;
                eval_iterator_apply_result(*iterator, &callback, callback_args, context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "iterator_count" => {
            let [iterator] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_iterator_count_result(*iterator, values)?
        }
        "iterator_to_array" => match evaluated_args {
            [iterator] => eval_iterator_to_array_result(*iterator, true, values)?,
            [iterator, preserve_keys] => {
                let preserve_keys = values.truthy(*preserve_keys)?;
                eval_iterator_to_array_result(*iterator, preserve_keys, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "ip2long" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ip2long_result(*value, values)?
        }
        "phpversion" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_phpversion_result(values)?
        }
        "pathinfo" => match evaluated_args {
            [path] => eval_pathinfo_result(*path, None, values)?,
            [path, flags] => eval_pathinfo_result(*path, Some(*flags), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "putenv" => {
            let [assignment] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_putenv_result(*assignment, values)?
        }
        "realpath" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_realpath_result(*path, values)?
        }
        "realpath_cache_get" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_realpath_cache_get_result(values)?
        }
        "realpath_cache_size" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_realpath_cache_size_result(values)?
        }
        "is_array" | "is_bool" | "is_double" | "is_finite" | "is_float" | "is_infinite"
        | "is_int" | "is_integer" | "is_iterable" | "is_long" | "is_nan" | "is_null"
        | "is_numeric" | "is_object" | "is_real" | "is_resource" | "is_string" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_type_predicate_result(name, *value, values)?
        }
        "sys_get_temp_dir" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_sys_get_temp_dir_result(values)?
        }
        "tempnam" => {
            let [directory, prefix] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_tempnam_result(*directory, *prefix, values)?
        }
        "sleep" => {
            let [seconds] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_sleep_result(*seconds, values)?
        }
        "time" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_time_result(values)?
        }
        "touch" => match evaluated_args {
            [filename] => eval_touch_result(*filename, None, None, values)?,
            [filename, mtime] => eval_touch_result(*filename, Some(*mtime), None, values)?,
            [filename, mtime, atime] => {
                eval_touch_result(*filename, Some(*mtime), Some(*atime), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stream_get_filters" | "stream_get_transports" | "stream_get_wrappers" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_introspection_result(name, values)?
        }
        "strtotime" => {
            let [datetime] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_strtotime_result(*datetime, values)?
        }
        "umask" => match evaluated_args {
            [] => eval_umask_result(None, values)?,
            [mask] => eval_umask_result(Some(*mask), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "usleep" => {
            let [microseconds] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_usleep_result(*microseconds, values)?
        }
        "readlink" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_readlink_result(*path, values)?
        }
        "unlink" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_unlink_result(*filename, values)?
        }
        "strlen" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let bytes = values.string_bytes(*value)?;
            let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(len)?
        }
        "strpos" | "strrpos" => {
            let [haystack, needle] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_position_result(name, *haystack, *needle, values)?
        }
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            let [haystack, needle] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_search_result(name, *haystack, *needle, values)?
        }
        "strstr" => match evaluated_args {
            [haystack, needle] => eval_strstr_result(*haystack, *needle, false, values)?,
            [haystack, needle, before_needle] => {
                let before_needle = values.truthy(*before_needle)?;
                eval_strstr_result(*haystack, *needle, before_needle, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "strcmp" | "strcasecmp" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_compare_result(name, *left, *right, values)?
        }
        "lcfirst" | "strtolower" | "strtoupper" | "ucfirst" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_case_result(name, *value, values)?
        }
        "long2ip" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_long2ip_result(*value, values)?
        }
        "ucwords" => match evaluated_args {
            [value] => eval_ucwords_result(*value, None, values)?,
            [value, separators] => eval_ucwords_result(*value, Some(*separators), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "vsprintf" | "vprintf" => eval_vsprintf_like_result(name, evaluated_args, values)?,
        "wordwrap" => match evaluated_args {
            [value] => eval_wordwrap_result(*value, None, None, None, values)?,
            [value, width] => eval_wordwrap_result(*value, Some(*width), None, None, values)?,
            [value, width, break_string] => {
                eval_wordwrap_result(*value, Some(*width), Some(*break_string), None, values)?
            }
            [value, width, break_string, cut] => eval_wordwrap_result(
                *value,
                Some(*width),
                Some(*break_string),
                Some(*cut),
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// Evaluates PHP's `abs(...)` over one eval expression.
pub(super) fn eval_builtin_abs(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.abs(value)
}

/// Evaluates PHP array aggregate builtins over one eval array expression.
pub(super) fn eval_builtin_array_aggregate(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_aggregate_result(name, array, values)
}

/// Computes `array_sum()` or `array_product()` through eval's numeric value hooks.
fn eval_array_aggregate_result(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = match name {
        "array_sum" => values.int(0)?,
        "array_product" => values.int(1)?,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        result = match name {
            "array_sum" => values.add(result, value)?,
            "array_product" => values.mul(result, value)?,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
    }
    Ok(result)
}

/// Evaluates PHP `array_combine()` over key and value array expressions.
pub(super) fn eval_builtin_array_combine(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, values_array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let keys = eval_expr(keys, context, scope, values)?;
    let values_array = eval_expr(values_array, context, scope, values)?;
    eval_array_combine_result(keys, values_array, values)
}

/// Builds the associative result for `array_combine()` from two eval arrays.
fn eval_array_combine_result(
    keys: RuntimeCellHandle,
    values_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(keys)?;
    if len != values.array_len(values_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }

    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let source_key = values.array_iter_key(keys, position)?;
        let target_key = values.array_get(keys, source_key)?;
        let target_key = values.cast_string(target_key)?;
        let value_key = values.array_iter_key(values_array, position)?;
        let value = values.array_get(values_array, value_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_column()` over row-array and column-key expressions.
pub(super) fn eval_builtin_array_column(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, column_key] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let column_key = eval_expr(column_key, context, scope, values)?;
    eval_array_column_result(array, column_key, values)
}

/// Builds `array_column()` by extracting present row columns into a reindexed array.
fn eval_array_column_result(
    array: RuntimeCellHandle,
    column_key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len)?;
    let mut output_index = 0_i64;
    for position in 0..len {
        let row_key = values.array_iter_key(array, position)?;
        let row = values.array_get(array, row_key)?;
        if !matches!(values.type_tag(row)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            continue;
        }
        let exists = values.array_key_exists(column_key, row)?;
        if !values.truthy(exists)? {
            continue;
        }
        let column = values.array_get(row, column_key)?;
        let target_key = values.int(output_index)?;
        output_index = output_index
            .checked_add(1)
            .ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, target_key, column)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_fill()` over start, count, and value expressions.
pub(super) fn eval_builtin_array_fill(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [start, count, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let start = eval_expr(start, context, scope, values)?;
    let count = eval_expr(count, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_array_fill_result(start, count, value, values)
}

/// Builds an `array_fill()` result with PHP's explicit integer key range.
fn eval_array_fill_result(
    start: RuntimeCellHandle,
    count: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let start = eval_int_value(start, values)?;
    let count = eval_int_value(count, values)?;
    if count < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let count = usize::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut result = if start == 0 {
        values.array_new(count)?
    } else {
        values.assoc_new(count)?
    };
    for offset in 0..count {
        let offset = i64::try_from(offset).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = start.checked_add(offset).ok_or(EvalStatus::RuntimeFatal)?;
        let key = values.int(key)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_fill_keys()` over key-array and value expressions.
pub(super) fn eval_builtin_array_fill_keys(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let keys = eval_expr(keys, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_array_fill_keys_result(keys, value, values)
}

/// Builds an `array_fill_keys()` result preserving the source key iteration order.
fn eval_array_fill_keys_result(
    keys: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(keys)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let source_key = values.array_iter_key(keys, position)?;
        let target_key = values.array_get(keys, source_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_map()` for one source array and a string or null callback.
pub(super) fn eval_builtin_array_map(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((callback, arrays)) = args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback = eval_expr(callback, context, scope, values)?;
    let mut evaluated_arrays = Vec::with_capacity(arrays.len());
    for array in arrays {
        evaluated_arrays.push(eval_expr(array, context, scope, values)?);
    }
    eval_array_map_result(callback, &evaluated_arrays, context, values)
}

/// Maps one eval array with PHP key preservation for the one-array form.
fn eval_array_map_result(
    callback: RuntimeCellHandle,
    arrays: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = arrays else {
        return eval_array_map_variadic_result(callback, arrays, context, values);
    };
    let callback = if values.is_null(callback)? {
        None
    } else {
        Some(eval_callable_name(callback, values)?)
    };
    let len = values.array_len(*array)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(*array, position)?;
        let value = values.array_get(*array, key)?;
        let mapped = if let Some(callback) = callback.as_deref() {
            eval_callable_with_values(callback, vec![value], context, values)?
        } else {
            value
        };
        result = values.array_set(result, key, mapped)?;
    }
    Ok(result)
}

/// Maps multiple eval arrays with PHP's reindexed and null-padded variadic behavior.
fn eval_array_map_variadic_result(
    callback: RuntimeCellHandle,
    arrays: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if arrays.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let callback = if values.is_null(callback)? {
        None
    } else {
        Some(eval_callable_name(callback, values)?)
    };
    let mut lengths = Vec::with_capacity(arrays.len());
    let mut max_len = 0;
    for array in arrays {
        let len = values.array_len(*array)?;
        max_len = max_len.max(len);
        lengths.push(len);
    }

    let mut result = values.array_new(max_len)?;
    for position in 0..max_len {
        let mut callback_args = Vec::with_capacity(arrays.len());
        for (array, len) in arrays.iter().zip(lengths.iter()) {
            let value = if position < *len {
                let key = values.array_iter_key(*array, position)?;
                values.array_get(*array, key)?
            } else {
                values.null()?
            };
            callback_args.push(value);
        }
        let mapped = if let Some(callback) = callback.as_deref() {
            eval_callable_with_values(callback, callback_args, context, values)?
        } else {
            eval_array_map_zipped_row(callback_args, values)?
        };
        let key = values.int(i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, key, mapped)?;
    }
    Ok(result)
}

/// Builds one row for `array_map(null, $a, $b, ...)`.
fn eval_array_map_zipped_row(
    values_row: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut row = values.array_new(values_row.len())?;
    for (index, value) in values_row.into_iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        row = values.array_set(row, key, value)?;
    }
    Ok(row)
}

/// Evaluates PHP `array_reduce()` with an optional initial carry value.
pub(super) fn eval_builtin_array_reduce(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array, callback, initial) = match args {
        [array, callback] => {
            let array = eval_expr(array, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            (array, callback, values.null()?)
        }
        [array, callback, initial] => {
            let array = eval_expr(array, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            let initial = eval_expr(initial, context, scope, values)?;
            (array, callback, initial)
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_array_reduce_result(array, callback, initial, context, values)
}

/// Reduces one eval array by invoking a string callback with carry and item cells.
fn eval_array_reduce_result(
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    initial: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_callable_name(callback, values)?;
    let len = values.array_len(array)?;
    let mut carry = initial;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        carry = eval_callable_with_values(&callback, vec![carry, value], context, values)?;
    }
    Ok(carry)
}

/// Evaluates PHP `array_walk()` for side-effect callbacks over value/key pairs.
pub(super) fn eval_builtin_array_walk(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, callback] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let callback = eval_expr(callback, context, scope, values)?;
    eval_array_walk_result(array, callback, context, values)
}

/// Walks one eval array by invoking a string callback with value and key cells.
fn eval_array_walk_result(
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_callable_name(callback, values)?;
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let _ = eval_callable_with_values(&callback, vec![value, key], context, values)?;
    }
    values.bool_value(true)
}

/// Evaluates direct by-reference `settype()` calls and writes the converted cell back.
fn eval_builtin_settype_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (var_name, type_name) = eval_settype_direct_args(args, context, scope, values)?;
    let value = visible_scope_cell(context, scope, &var_name).map_or_else(|| values.null(), Ok)?;
    let Some(converted) = eval_settype_cast_value(value, type_name, values)? else {
        return values.bool_value(false);
    };
    for replaced in set_scope_cell(
        context,
        scope,
        var_name,
        converted,
        ScopeCellOwnership::Owned,
    )? {
        values.release(replaced)?;
    }
    values.bool_value(true)
}

/// Evaluates and binds direct `settype()` arguments while preserving source order.
fn eval_settype_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(String, RuntimeCellHandle), EvalStatus> {
    let mut var_name = None;
    let mut type_name = None;
    let mut positional_index = 0;
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let parameter = if let Some(name) = arg.name() {
            saw_named = true;
            name
        } else {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let parameter = match positional_index {
                0 => "var",
                1 => "type",
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            positional_index += 1;
            parameter
        };

        match parameter {
            "var" => {
                if var_name.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let EvalExpr::LoadVar(name) = arg.value() else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                var_name = Some(name.clone());
            }
            "type" => {
                if type_name.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                type_name = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let var_name = var_name.ok_or(EvalStatus::RuntimeFatal)?;
    let type_name = type_name.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((var_name, type_name))
}

/// Applies the eval-supported `settype()` scalar target conversion.
fn eval_settype_cast_value(
    value: RuntimeCellHandle,
    type_name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let type_name = values.string_bytes(type_name)?;
    let type_name = String::from_utf8_lossy(&type_name).to_ascii_lowercase();
    let converted = match type_name.as_str() {
        "bool" | "boolean" => Some(values.cast_bool(value)?),
        "float" | "double" => Some(values.cast_float(value)?),
        "int" | "integer" => Some(values.cast_int(value)?),
        "string" => Some(values.cast_string(value)?),
        _ => None,
    };
    Ok(converted)
}

/// Evaluates by-value `settype()` callable dispatch without mutating the source argument.
fn eval_settype_value_result(
    value: RuntimeCellHandle,
    type_name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.warning("settype(): Argument #1 ($var) must be passed by reference, value given")?;
    if let Some(converted) = eval_settype_cast_value(value, type_name, values)? {
        values.release(converted)?;
        return values.bool_value(true);
    }
    values.bool_value(false)
}

/// Evaluates direct by-reference `array_pop()` / `array_shift()` calls and writes back the array.
pub(super) fn eval_builtin_array_pop_shift_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if name == "settype" {
        return eval_builtin_settype_call(args, context, scope, values);
    }
    if matches!(name, "array_push" | "array_unshift") {
        return eval_builtin_array_push_unshift_call(name, args, context, scope, values);
    }
    if name == "array_splice" {
        return eval_builtin_array_splice_call(args, context, scope, values);
    }
    if matches!(
        name,
        "arsort"
            | "asort"
            | "krsort"
            | "ksort"
            | "natcasesort"
            | "natsort"
            | "rsort"
            | "shuffle"
            | "sort"
    ) {
        return eval_builtin_array_sort_call(name, args, context, scope, values);
    }
    if matches!(name, "uasort" | "uksort" | "usort") {
        return eval_builtin_user_sort_call(name, args, context, scope, values);
    }

    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if arg.is_spread() || !matches!(arg.name(), None | Some("array")) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let EvalExpr::LoadVar(var_name) = arg.value() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let Some(entry) =
        scope_entry(context, scope, var_name).filter(|entry| entry.flags().is_visible())
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = entry.cell();
    let ownership = entry.flags().ownership;
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }

    let (result, replacement) = eval_array_pop_shift_replacement(name, array, values)?;
    for replaced in set_scope_cell(context, scope, var_name.clone(), replacement, ownership)? {
        values.release(replaced)?;
    }
    Ok(result)
}

/// Evaluates direct by-reference `array_push()` / `array_unshift()` calls.
fn eval_builtin_array_push_unshift_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 || !eval_call_args_are_plain_positional(args) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let EvalExpr::LoadVar(var_name) = args[0].value() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let mut inserted = Vec::with_capacity(args.len() - 1);
    for arg in &args[1..] {
        inserted.push(eval_expr(arg.value(), context, scope, values)?);
    }
    let Some(entry) =
        scope_entry(context, scope, var_name).filter(|entry| entry.flags().is_visible())
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = entry.cell();
    let ownership = entry.flags().ownership;
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }

    let replacement = eval_array_push_unshift_replacement(name, array, &inserted, values)?;
    let result = eval_array_push_unshift_count_result(array, inserted.len(), values)?;
    for replaced in set_scope_cell(context, scope, var_name.clone(), replacement, ownership)? {
        values.release(replaced)?;
    }
    Ok(result)
}

/// Evaluates direct by-reference `array_splice()` calls and writes back the array.
fn eval_builtin_array_splice_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array_name, offset, length, replacement_arg) =
        eval_array_splice_direct_args(args, context, scope, values)?;
    let Some(entry) =
        scope_entry(context, scope, &array_name).filter(|entry| entry.flags().is_visible())
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = entry.cell();
    let ownership = entry.flags().ownership;
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }

    let (removed, replacement) =
        eval_array_splice_removed_and_replacement(array, offset, length, replacement_arg, values)?;
    for replaced in set_scope_cell(context, scope, array_name, replacement, ownership)? {
        values.release(replaced)?;
    }
    Ok(removed)
}

/// Evaluates direct by-reference array ordering calls and writes back the array.
fn eval_builtin_array_sort_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let array_name = eval_array_sort_direct_arg(args)?;
    let Some(entry) =
        scope_entry(context, scope, &array_name).filter(|entry| entry.flags().is_visible())
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = entry.cell();
    let ownership = entry.flags().ownership;
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }

    let replacement = eval_array_sort_replacement(name, array, values)?;
    let result = values.bool_value(true)?;
    for replaced in set_scope_cell(context, scope, array_name, replacement, ownership)? {
        values.release(replaced)?;
    }
    Ok(result)
}

/// Evaluates direct by-reference user-comparator sort calls and writes back the array.
fn eval_builtin_user_sort_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array_name, callback) = eval_user_sort_direct_args(args, context, scope, values)?;
    let Some(entry) =
        scope_entry(context, scope, &array_name).filter(|entry| entry.flags().is_visible())
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = entry.cell();
    let ownership = entry.flags().ownership;
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }

    let replacement = eval_user_sort_replacement(name, array, callback, context, values)?;
    let result = values.bool_value(true)?;
    for replaced in set_scope_cell(context, scope, array_name, replacement, ownership)? {
        values.release(replaced)?;
    }
    Ok(result)
}

/// Evaluates and binds direct user-sort arguments while preserving source order.
fn eval_user_sort_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(String, RuntimeCellHandle), EvalStatus> {
    let mut array = None;
    let mut callback = None;
    let mut positional_index = 0;
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let parameter = if let Some(name) = arg.name() {
            saw_named = true;
            name
        } else {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let parameter = match positional_index {
                0 => "array",
                1 => "callback",
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            positional_index += 1;
            parameter
        };

        match parameter {
            "array" => {
                if array.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let EvalExpr::LoadVar(name) = arg.value() else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                array = Some(name.clone());
            }
            "callback" => {
                if callback.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                callback = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let array = array.ok_or(EvalStatus::RuntimeFatal)?;
    let callback = callback.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((array, callback))
}

/// Returns the dynamic callable result for by-value user-comparator sort calls.
fn eval_user_sort_value_result(
    name: &str,
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let replacement = eval_user_sort_replacement(name, array, callback, context, values)?;
    values.release(replacement)?;
    values.bool_value(true)
}

/// One source array entry used by eval user-comparator sort routines.
struct EvalUserSortEntry {
    source_key: RuntimeCellHandle,
    value: RuntimeCellHandle,
}

/// Builds the sorted replacement array for user-comparator sort builtins.
fn eval_user_sort_replacement(
    name: &str,
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_callable_name(callback, values)?;
    let mut entries = eval_user_sort_entries(array, values)?;
    eval_user_sort_entries_in_place(name, &callback, &mut entries, context, values)?;
    if name == "usort" {
        return eval_user_sort_reindex_result(entries, values);
    }
    eval_user_sort_preserve_key_result(entries, values)
}

/// Collects source keys and values from one eval array for user sorting.
fn eval_user_sort_entries(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalUserSortEntry>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut entries = Vec::with_capacity(len);
    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        entries.push(EvalUserSortEntry { source_key, value });
    }
    Ok(entries)
}

/// Sorts entries by repeatedly invoking the PHP comparator callback.
fn eval_user_sort_entries_in_place(
    name: &str,
    callback: &str,
    entries: &mut [EvalUserSortEntry],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for pass in 0..entries.len() {
        let upper = entries.len().saturating_sub(pass + 1);
        for index in 0..upper {
            let comparison = eval_user_sort_compare(
                name,
                callback,
                &entries[index],
                &entries[index + 1],
                context,
                values,
            )?;
            if comparison > 0 {
                entries.swap(index, index + 1);
            }
        }
    }
    Ok(())
}

/// Invokes one user-sort comparator and returns its integer ordering result.
fn eval_user_sort_compare(
    name: &str,
    callback: &str,
    left: &EvalUserSortEntry,
    right: &EvalUserSortEntry,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let args = if name == "uksort" {
        vec![left.source_key, right.source_key]
    } else {
        vec![left.value, right.value]
    };
    let result = eval_callable_with_values(callback, args, context, values)?;
    eval_int_value(result, values)
}

/// Builds the reindexed result for `usort()`.
fn eval_user_sort_reindex_result(
    entries: Vec<EvalUserSortEntry>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(entries.len())?;
    for (index, entry) in entries.into_iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, key, entry.value)?;
    }
    Ok(result)
}

/// Builds the key-preserving result for `uksort()` and `uasort()`.
fn eval_user_sort_preserve_key_result(
    entries: Vec<EvalUserSortEntry>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(entries.len())?;
    for entry in entries {
        result = values.array_set(result, entry.source_key, entry.value)?;
    }
    Ok(result)
}

/// Extracts the direct variable argument accepted by eval array ordering builtins.
fn eval_array_sort_direct_arg(args: &[EvalCallArg]) -> Result<String, EvalStatus> {
    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if arg.is_spread() || !matches!(arg.name(), None | Some("array")) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let EvalExpr::LoadVar(name) = arg.value() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    Ok(name.clone())
}

/// Returns the dynamic callable result for by-value array ordering calls.
fn eval_array_sort_value_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.bool_value(true)
}

/// Sort key shape supported by eval's homogeneous array ordering implementation.
#[derive(Clone)]
enum EvalArraySortKey {
    Numeric(f64),
    Natural(Vec<u8>),
    String(Vec<u8>),
}

/// One source array entry plus its precomputed ordering key.
struct EvalArraySortEntry {
    sort_key: EvalArraySortKey,
    source_key: RuntimeCellHandle,
    value: RuntimeCellHandle,
}

/// Builds the sorted replacement array for eval array ordering builtins.
fn eval_array_sort_replacement(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut entries = match name {
        "krsort" | "ksort" => eval_array_key_sort_entries(array, values)?,
        "natcasesort" => eval_array_natural_sort_entries(array, true, values)?,
        "natsort" => eval_array_natural_sort_entries(array, false, values)?,
        "arsort" | "asort" | "rsort" | "sort" => eval_array_value_sort_entries(array, values)?,
        "shuffle" => return eval_array_shuffle_replacement(array, values),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    entries.sort_by(|left, right| {
        let order = eval_array_sort_key_cmp(&left.sort_key, &right.sort_key);
        if matches!(name, "arsort" | "krsort" | "rsort") {
            order.reverse()
        } else {
            order
        }
    });

    if matches!(
        name,
        "arsort" | "asort" | "krsort" | "ksort" | "natcasesort" | "natsort"
    ) {
        return eval_array_preserve_key_sort_result(entries, values);
    }
    eval_array_reindex_sort_result(entries, values)
}

/// Builds a shuffled, reindexed replacement array for `shuffle()`.
fn eval_array_shuffle_replacement(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut entries = Vec::with_capacity(len);
    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        entries.push(values.array_get(array, source_key)?);
    }

    for index in (1..entries.len()).rev() {
        let swap_with = (eval_random_u128() % ((index + 1) as u128)) as usize;
        entries.swap(index, swap_with);
    }

    let mut result = values.array_new(entries.len())?;
    for (index, value) in entries.into_iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Builds an indexed result for `sort()` / `rsort()` after value ordering.
fn eval_array_reindex_sort_result(
    entries: Vec<EvalArraySortEntry>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(entries.len())?;
    for (index, entry) in entries.into_iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, key, entry.value)?;
    }
    Ok(result)
}

/// Builds a key-preserving associative result after value or key ordering.
fn eval_array_preserve_key_sort_result(
    entries: Vec<EvalArraySortEntry>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(entries.len())?;
    for entry in entries {
        result = values.array_set(result, entry.source_key, entry.value)?;
    }
    Ok(result)
}

/// Collects values and comparable value-sort keys from one eval array.
fn eval_array_value_sort_entries(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalArraySortEntry>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut entries = Vec::with_capacity(len);
    let mut expects_numeric = None;

    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let sort_key = eval_array_sort_key(value, values)?;
        let is_numeric = matches!(sort_key, EvalArraySortKey::Numeric(_));
        match expects_numeric {
            Some(expected) if expected != is_numeric => return Err(EvalStatus::RuntimeFatal),
            Some(_) => {}
            None => expects_numeric = Some(is_numeric),
        }
        entries.push(EvalArraySortEntry {
            sort_key,
            source_key,
            value,
        });
    }

    Ok(entries)
}

/// Collects values and natural-sort keys from one eval array.
fn eval_array_natural_sort_entries(
    array: RuntimeCellHandle,
    case_insensitive: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalArraySortEntry>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut entries = Vec::with_capacity(len);
    let mut expects_numeric = None;

    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let sort_key = eval_array_natural_sort_key(value, case_insensitive, values)?;
        let is_numeric = matches!(sort_key, EvalArraySortKey::Numeric(_));
        match expects_numeric {
            Some(expected) if expected != is_numeric => return Err(EvalStatus::RuntimeFatal),
            Some(_) => {}
            None => expects_numeric = Some(is_numeric),
        }
        entries.push(EvalArraySortEntry {
            sort_key,
            source_key,
            value,
        });
    }

    Ok(entries)
}

/// Collects values and comparable key-sort keys from one eval array.
fn eval_array_key_sort_entries(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalArraySortEntry>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut entries = Vec::with_capacity(len);

    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let sort_key = eval_array_sort_key(source_key, values)?;
        entries.push(EvalArraySortEntry {
            sort_key,
            source_key,
            value,
        });
    }

    Ok(entries)
}

/// Converts one scalar eval value into a natural-sort key.
fn eval_array_natural_sort_key(
    value: RuntimeCellHandle,
    case_insensitive: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalArraySortKey, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_INT | EVAL_TAG_FLOAT => {
            Ok(EvalArraySortKey::Numeric(eval_float_value(value, values)?))
        }
        EVAL_TAG_STRING => {
            let mut bytes = values.string_bytes(value)?;
            if case_insensitive {
                bytes.make_ascii_lowercase();
            }
            Ok(EvalArraySortKey::Natural(bytes))
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Converts one scalar eval value into a homogeneous sort key.
fn eval_array_sort_key(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalArraySortKey, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_INT | EVAL_TAG_FLOAT => {
            Ok(EvalArraySortKey::Numeric(eval_float_value(value, values)?))
        }
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(value)?;
            match eval_array_numeric_string_sort_key(&bytes) {
                Some(value) => Ok(EvalArraySortKey::Numeric(value)),
                None => Ok(EvalArraySortKey::String(bytes)),
            }
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Parses one PHP numeric string into the numeric sort domain when possible.
fn eval_array_numeric_string_sort_key(bytes: &[u8]) -> Option<f64> {
    if !eval_is_numeric_string(bytes) {
        return None;
    }
    std::str::from_utf8(bytes).ok()?.parse::<f64>().ok()
}

/// Compares two precomputed eval sort keys.
fn eval_array_sort_key_cmp(
    left: &EvalArraySortKey,
    right: &EvalArraySortKey,
) -> std::cmp::Ordering {
    match (left, right) {
        (EvalArraySortKey::Numeric(left), EvalArraySortKey::Numeric(right)) => {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        }
        (EvalArraySortKey::Natural(left), EvalArraySortKey::Natural(right)) => {
            eval_natural_bytes_cmp(left, right)
        }
        (EvalArraySortKey::String(left), EvalArraySortKey::String(right)) => left.cmp(right),
        _ => eval_array_sort_key_rank(left).cmp(&eval_array_sort_key_rank(right)),
    }
}

/// Returns a deterministic rank for mixed key-sort domains.
fn eval_array_sort_key_rank(key: &EvalArraySortKey) -> u8 {
    match key {
        EvalArraySortKey::Numeric(_) => 0,
        EvalArraySortKey::Natural(_) => 1,
        EvalArraySortKey::String(_) => 2,
    }
}

/// Compares byte strings with a small PHP-style natural ordering.
fn eval_natural_bytes_cmp(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left.len() && right_index < right.len() {
        if left[left_index].is_ascii_digit() && right[right_index].is_ascii_digit() {
            let order = eval_natural_digit_run_cmp(left, &mut left_index, right, &mut right_index);
            if order != std::cmp::Ordering::Equal {
                return order;
            }
            continue;
        }

        let order = left[left_index].cmp(&right[right_index]);
        if order != std::cmp::Ordering::Equal {
            return order;
        }
        left_index += 1;
        right_index += 1;
    }
    left.len().cmp(&right.len())
}

/// Compares two natural-sort digit runs and advances both byte indexes past them.
fn eval_natural_digit_run_cmp(
    left: &[u8],
    left_index: &mut usize,
    right: &[u8],
    right_index: &mut usize,
) -> std::cmp::Ordering {
    let left_start = *left_index;
    let right_start = *right_index;
    while *left_index < left.len() && left[*left_index].is_ascii_digit() {
        *left_index += 1;
    }
    while *right_index < right.len() && right[*right_index].is_ascii_digit() {
        *right_index += 1;
    }

    let left_digits = &left[left_start..*left_index];
    let right_digits = &right[right_start..*right_index];
    let left_trimmed = eval_trim_leading_zeroes(left_digits);
    let right_trimmed = eval_trim_leading_zeroes(right_digits);
    left_trimmed
        .len()
        .cmp(&right_trimmed.len())
        .then_with(|| left_trimmed.cmp(right_trimmed))
        .then_with(|| left_digits.len().cmp(&right_digits.len()))
}

/// Drops leading zero bytes while keeping one zero for an all-zero digit run.
fn eval_trim_leading_zeroes(digits: &[u8]) -> &[u8] {
    let trimmed = digits
        .iter()
        .position(|digit| *digit != b'0')
        .map_or(&digits[digits.len().saturating_sub(1)..], |index| {
            &digits[index..]
        });
    if trimmed.is_empty() {
        digits
    } else {
        trimmed
    }
}

/// Evaluates and binds direct `array_splice()` arguments while preserving source order.
fn eval_array_splice_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalArraySpliceDirectArgs, EvalStatus> {
    let mut array = None;
    let mut offset = None;
    let mut length = None;
    let mut replacement = None;
    let mut positional_index = 0;
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let parameter = if let Some(name) = arg.name() {
            saw_named = true;
            name
        } else {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let parameter = match positional_index {
                0 => "array",
                1 => "offset",
                2 => "length",
                3 => "replacement",
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            positional_index += 1;
            parameter
        };

        match parameter {
            "array" => {
                if array.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let EvalExpr::LoadVar(name) = arg.value() else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                array = Some(name.clone());
            }
            "offset" => {
                if offset.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                offset = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            "length" => {
                if length.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                length = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            "replacement" => {
                if replacement.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                replacement = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let array = array.ok_or(EvalStatus::RuntimeFatal)?;
    let offset = offset.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((array, offset, length, replacement))
}

/// Returns the removed elements that `array_splice()` would produce without mutating the source.
fn eval_array_splice_value_result(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (start, end) = eval_array_splice_bounds(array, offset, length, values)?;
    eval_array_splice_removed(array, start, end, values)
}

/// Builds both removed and replacement arrays for direct `array_splice()` write-back.
fn eval_array_splice_removed_and_replacement(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    replacement: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let (start, end) = eval_array_splice_bounds(array, offset, length, values)?;
    let removed = eval_array_splice_removed(array, start, end, values)?;
    let inserted = eval_array_splice_insert_values(replacement, values)?;
    let replacement = eval_array_splice_replacement(array, start, end, &inserted, values)?;
    Ok((removed, replacement))
}

/// Converts splice offset and length cells into bounded source positions.
fn eval_array_splice_bounds(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(usize, usize), EvalStatus> {
    let len = values.array_len(array)?;
    let offset = eval_int_value(offset, values)?;
    let start = eval_slice_start(len, offset)?;
    let end = match length {
        Some(length) if values.type_tag(length)? != EVAL_TAG_NULL => {
            eval_slice_end(len, start, eval_int_value(length, values)?)?
        }
        _ => len,
    };
    Ok((start, end))
}

/// Builds the reindexed/string-key-preserving removed array returned by `array_splice()`.
fn eval_array_splice_removed(
    array: RuntimeCellHandle,
    start: usize,
    end: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = end.saturating_sub(start);
    if eval_array_range_keys_are_int(array, start, end, values)? {
        let mut result = values.array_new(len)?;
        let mut target = 0_i64;
        for position in start..end {
            let key = values.array_iter_key(array, position)?;
            let value = values.array_get(array, key)?;
            let target_key = values.int(target)?;
            target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
            result = values.array_set(result, target_key, value)?;
        }
        return Ok(result);
    }

    let mut result = values.assoc_new(len)?;
    let mut next_int_key = 0_i64;
    for position in start..end {
        let source_key = values.array_iter_key(array, position)?;
        let target_key = if values.type_tag(source_key)? == EVAL_TAG_INT {
            let key = values.int(next_int_key)?;
            next_int_key = next_int_key
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            key
        } else {
            source_key
        };
        let value = values.array_get(array, source_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Expands the optional `array_splice()` replacement value into inserted values.
fn eval_array_splice_insert_values(
    replacement: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let Some(replacement) = replacement else {
        return Ok(Vec::new());
    };
    if !matches!(
        values.type_tag(replacement)?,
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC
    ) {
        return Ok(vec![replacement]);
    }

    let len = values.array_len(replacement)?;
    let mut inserted = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.array_iter_key(replacement, position)?;
        inserted.push(values.array_get(replacement, key)?);
    }
    Ok(inserted)
}

/// Builds the source replacement after removing the requested splice range.
fn eval_array_splice_replacement(
    array: RuntimeCellHandle,
    start: usize,
    end: usize,
    inserted: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let new_len = len
        .saturating_sub(end.saturating_sub(start))
        .checked_add(inserted.len())
        .ok_or(EvalStatus::RuntimeFatal)?;
    if eval_array_splice_remaining_keys_are_int(array, start, end, len, values)? {
        let mut result = values.array_new(new_len)?;
        let mut target = 0_i64;
        for position in 0..start {
            let key = values.array_iter_key(array, position)?;
            let value = values.array_get(array, key)?;
            let target_key = values.int(target)?;
            target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
            result = values.array_set(result, target_key, value)?;
        }
        for value in inserted {
            let target_key = values.int(target)?;
            target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
            result = values.array_set(result, target_key, *value)?;
        }
        for position in end..len {
            let key = values.array_iter_key(array, position)?;
            let value = values.array_get(array, key)?;
            let target_key = values.int(target)?;
            target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
            result = values.array_set(result, target_key, value)?;
        }
        return Ok(result);
    }

    let mut result = values.assoc_new(new_len)?;
    let mut next_int_key = 0_i64;
    for position in 0..start {
        let source_key = values.array_iter_key(array, position)?;
        let target_key = if values.type_tag(source_key)? == EVAL_TAG_INT {
            let key = values.int(next_int_key)?;
            next_int_key = next_int_key
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            key
        } else {
            source_key
        };
        let value = values.array_get(array, source_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    for value in inserted {
        let target_key = values.int(next_int_key)?;
        next_int_key = next_int_key
            .checked_add(1)
            .ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, target_key, *value)?;
    }
    for position in end..len {
        let source_key = values.array_iter_key(array, position)?;
        let target_key = if values.type_tag(source_key)? == EVAL_TAG_INT {
            let key = values.int(next_int_key)?;
            next_int_key = next_int_key
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            key
        } else {
            source_key
        };
        let value = values.array_get(array, source_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Returns true when every key in one source position range is integer-shaped.
fn eval_array_range_keys_are_int(
    array: RuntimeCellHandle,
    start: usize,
    end: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for position in start..end {
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? != EVAL_TAG_INT {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Returns true when every key outside the removed splice range is integer-shaped.
fn eval_array_splice_remaining_keys_are_int(
    array: RuntimeCellHandle,
    start: usize,
    end: usize,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for position in 0..len {
        if (start..end).contains(&position) {
            continue;
        }
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? != EVAL_TAG_INT {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Returns the value produced by `array_pop()` / `array_shift()` without mutating the source.
fn eval_array_pop_shift_value_result(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(array)?;
    if len == 0 {
        return values.null();
    }
    let position = match name {
        "array_pop" => len - 1,
        "array_shift" => 0,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let key = values.array_iter_key(array, position)?;
    values.array_get(array, key)
}

/// Builds the return value plus replacement array for direct pop/shift write-back.
fn eval_array_pop_shift_replacement(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let len = values.array_len(array)?;
    let tag = values.type_tag(array)?;
    if len == 0 {
        let replacement = match tag {
            EVAL_TAG_ARRAY => values.array_new(0)?,
            EVAL_TAG_ASSOC => values.assoc_new(0)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        };
        return Ok((values.null()?, replacement));
    }

    let removed_position = match name {
        "array_pop" => len - 1,
        "array_shift" => 0,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let removed_key = values.array_iter_key(array, removed_position)?;
    let removed_value = values.array_get(array, removed_key)?;
    let replacement = match tag {
        EVAL_TAG_ARRAY => {
            eval_array_pop_shift_indexed_replacement(array, removed_position, len, values)?
        }
        EVAL_TAG_ASSOC => {
            eval_array_pop_shift_assoc_replacement(name, array, removed_position, len, values)?
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    Ok((removed_value, replacement))
}

/// Rebuilds an indexed array after removing one position and reindexing values.
fn eval_array_pop_shift_indexed_replacement(
    array: RuntimeCellHandle,
    removed_position: usize,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(len.saturating_sub(1))?;
    let mut target = 0_i64;
    for position in 0..len {
        if position == removed_position {
            continue;
        }
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let target_key = values.int(target)?;
        target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Rebuilds an associative array after pop/shift, preserving PHP key behavior.
fn eval_array_pop_shift_assoc_replacement(
    name: &str,
    array: RuntimeCellHandle,
    removed_position: usize,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if name == "array_shift"
        && eval_array_remaining_keys_are_int(array, removed_position, len, values)?
    {
        return eval_array_pop_shift_indexed_replacement(array, removed_position, len, values);
    }

    let mut result = values.assoc_new(len.saturating_sub(1))?;
    let mut next_int_key = 0_i64;
    for position in 0..len {
        if position == removed_position {
            continue;
        }
        let source_key = values.array_iter_key(array, position)?;
        let target_key = if name == "array_shift" && values.type_tag(source_key)? == EVAL_TAG_INT {
            let key = values.int(next_int_key)?;
            next_int_key = next_int_key
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            key
        } else {
            source_key
        };
        let value = values.array_get(array, source_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Returns true when every remaining key is an integer after removing one element.
fn eval_array_remaining_keys_are_int(
    array: RuntimeCellHandle,
    removed_position: usize,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for position in 0..len {
        if position == removed_position {
            continue;
        }
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? != EVAL_TAG_INT {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Returns the resulting element count for by-value push/unshift dynamic calls.
fn eval_array_push_unshift_count_result(
    array: RuntimeCellHandle,
    inserted_len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let total = values
        .array_len(array)?
        .checked_add(inserted_len)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let total = i64::try_from(total).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(total)
}

/// Builds the replacement array for direct push/unshift write-back.
fn eval_array_push_unshift_replacement(
    name: &str,
    array: RuntimeCellHandle,
    inserted: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match (name, values.type_tag(array)?) {
        ("array_push", EVAL_TAG_ARRAY) => {
            eval_array_push_indexed_replacement(array, inserted, values)
        }
        ("array_push", EVAL_TAG_ASSOC) => {
            eval_array_push_assoc_replacement(array, inserted, values)
        }
        ("array_unshift", EVAL_TAG_ARRAY) => {
            eval_array_unshift_indexed_replacement(array, inserted, values)
        }
        ("array_unshift", EVAL_TAG_ASSOC) => {
            eval_array_unshift_assoc_replacement(array, inserted, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Rebuilds an indexed array after appending values.
fn eval_array_push_indexed_replacement(
    array: RuntimeCellHandle,
    inserted: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len.saturating_add(inserted.len()))?;
    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let target_key =
            values.int(i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, target_key, value)?;
    }
    for (offset, value) in inserted.iter().copied().enumerate() {
        let position = len.checked_add(offset).ok_or(EvalStatus::RuntimeFatal)?;
        let key = values.int(i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Rebuilds an associative array after appending values at PHP's next integer keys.
fn eval_array_push_assoc_replacement(
    array: RuntimeCellHandle,
    inserted: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.assoc_new(len.saturating_add(inserted.len()))?;
    let mut next_key = 0_i64;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? == EVAL_TAG_INT {
            next_key = next_key.max(eval_int_value(key, values)?.saturating_add(1));
        }
        let value = values.array_get(array, key)?;
        result = values.array_set(result, key, value)?;
    }
    for value in inserted.iter().copied() {
        let key = values.int(next_key)?;
        next_key = next_key.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Rebuilds an indexed array after prepending values and reindexing the original tail.
fn eval_array_unshift_indexed_replacement(
    array: RuntimeCellHandle,
    inserted: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len.saturating_add(inserted.len()))?;
    let mut target = 0_i64;
    for value in inserted.iter().copied() {
        let key = values.int(target)?;
        target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, key, value)?;
    }
    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let key = values.int(target)?;
        target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Rebuilds an associative array after prepending values and reindexing integer keys.
fn eval_array_unshift_assoc_replacement(
    array: RuntimeCellHandle,
    inserted: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    if eval_array_keys_are_int(array, len, values)? {
        return eval_array_unshift_indexed_replacement(array, inserted, values);
    }

    let mut result = values.assoc_new(len.saturating_add(inserted.len()))?;
    let mut next_int_key = 0_i64;
    for value in inserted.iter().copied() {
        let key = values.int(next_int_key)?;
        next_int_key = next_int_key
            .checked_add(1)
            .ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, key, value)?;
    }
    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let target_key = if values.type_tag(source_key)? == EVAL_TAG_INT {
            let key = values.int(next_int_key)?;
            next_int_key = next_int_key
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            key
        } else {
            source_key
        };
        let value = values.array_get(array, source_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Returns true when every key in the array is integer-shaped.
fn eval_array_keys_are_int(
    array: RuntimeCellHandle,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? != EVAL_TAG_INT {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Evaluates PHP `array_filter()` for null and string-callback filtering modes.
pub(super) fn eval_builtin_array_filter(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array] => {
            let array = eval_expr(array, context, scope, values)?;
            eval_array_filter_result(array, None, None, context, values)
        }
        [array, callback] => {
            let array = eval_expr(array, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            eval_array_filter_result(array, Some(callback), None, context, values)
        }
        [array, callback, mode] => {
            let array = eval_expr(array, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            let mode = eval_expr(mode, context, scope, values)?;
            eval_array_filter_result(array, Some(callback), Some(mode), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Filters eval array entries through PHP truthiness or a string callback.
fn eval_array_filter_result(
    array: RuntimeCellHandle,
    callback: Option<RuntimeCellHandle>,
    mode: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = match callback {
        Some(callback) if !values.is_null(callback)? => Some(eval_callable_name(callback, values)?),
        _ => None,
    };
    let mode = match mode {
        Some(mode) => eval_array_filter_mode_value(mode, values)?,
        None => EVAL_ARRAY_FILTER_USE_VALUE,
    };

    let len = values.array_len(array)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let keep = if let Some(callback) = callback.as_deref() {
            let args = eval_array_filter_callback_args(mode, key, value)?;
            let result = eval_callable_with_values(callback, args, context, values)?;
            values.truthy(result)?
        } else {
            values.truthy(value)?
        };
        if keep {
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// Reads and validates the optional `array_filter()` callback mode.
fn eval_array_filter_mode_value(
    mode: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let mode = eval_int_value(mode, values)?;
    match mode {
        EVAL_ARRAY_FILTER_USE_VALUE | EVAL_ARRAY_FILTER_USE_BOTH | EVAL_ARRAY_FILTER_USE_KEY => {
            Ok(mode)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds the callback argument list for one `array_filter()` entry.
fn eval_array_filter_callback_args(
    mode: i64,
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    match mode {
        EVAL_ARRAY_FILTER_USE_VALUE => Ok(vec![value]),
        EVAL_ARRAY_FILTER_USE_BOTH => Ok(vec![value, key]),
        EVAL_ARRAY_FILTER_USE_KEY => Ok(vec![key]),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `array_chunk()` over one array and chunk-size expression.
pub(super) fn eval_builtin_array_chunk(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, length] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let length = eval_expr(length, context, scope, values)?;
    eval_array_chunk_result(array, length, values)
}

/// Builds an `array_chunk()` result as nested reindexed arrays.
fn eval_array_chunk_result(
    array: RuntimeCellHandle,
    length: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let chunk_size = eval_int_value(length, values)?;
    if chunk_size <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let chunk_size = usize::try_from(chunk_size).map_err(|_| EvalStatus::RuntimeFatal)?;
    let len = values.array_len(array)?;
    let chunk_count = len.div_ceil(chunk_size);
    let mut result = values.array_new(chunk_count)?;

    for chunk_index in 0..chunk_count {
        let start = chunk_index * chunk_size;
        let end = usize::min(start + chunk_size, len);
        let mut chunk = values.array_new(end - start)?;
        for source_position in start..end {
            let source_key = values.array_iter_key(array, source_position)?;
            let value = values.array_get(array, source_key)?;
            let target_index =
                i64::try_from(source_position - start).map_err(|_| EvalStatus::RuntimeFatal)?;
            let target_index = values.int(target_index)?;
            chunk = values.array_set(chunk, target_index, value)?;
        }
        let result_key = i64::try_from(chunk_index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let result_key = values.int(result_key)?;
        result = values.array_set(result, result_key, chunk)?;
    }

    Ok(result)
}

/// Evaluates PHP `array_slice()` over array, offset, and optional length expressions.
pub(super) fn eval_builtin_array_slice(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array, offset] => {
            let array = eval_expr(array, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            eval_array_slice_result(array, offset, None, values)
        }
        [array, offset, length] => {
            let array = eval_expr(array, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_array_slice_result(array, offset, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds an `array_slice()` result with PHP offset and length bounds.
fn eval_array_slice_result(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let offset = eval_int_value(offset, values)?;
    let start = eval_slice_start(len, offset)?;
    let end = match length {
        Some(length) if values.type_tag(length)? != EVAL_TAG_NULL => {
            eval_slice_end(len, start, eval_int_value(length, values)?)?
        }
        _ => len,
    };

    let mut result = values.array_new(end.saturating_sub(start))?;
    for source_position in start..end {
        let source_key = values.array_iter_key(array, source_position)?;
        let source_value = values.array_get(array, source_key)?;
        let target_key =
            i64::try_from(source_position - start).map_err(|_| EvalStatus::RuntimeFatal)?;
        let target_key = values.int(target_key)?;
        result = values.array_set(result, target_key, source_value)?;
    }
    Ok(result)
}

/// Converts a PHP array-slice offset into a bounded source position.
fn eval_slice_start(len: usize, offset: i64) -> Result<usize, EvalStatus> {
    if offset >= 0 {
        let offset = usize::try_from(offset).map_err(|_| EvalStatus::RuntimeFatal)?;
        return Ok(usize::min(offset, len));
    }

    let tail = offset
        .checked_abs()
        .ok_or(EvalStatus::RuntimeFatal)
        .and_then(|value| usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal))?;
    Ok(len.saturating_sub(tail))
}

/// Converts a PHP array-slice length into a bounded exclusive end position.
fn eval_slice_end(len: usize, start: usize, length: i64) -> Result<usize, EvalStatus> {
    if length >= 0 {
        let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
        return Ok(usize::min(start.saturating_add(length), len));
    }

    let tail = length
        .checked_abs()
        .ok_or(EvalStatus::RuntimeFatal)
        .and_then(|value| usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal))?;
    Ok(usize::max(start, len.saturating_sub(tail)))
}

/// Evaluates PHP `array_pad()` over array, target length, and pad value expressions.
pub(super) fn eval_builtin_array_pad(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, length, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let length = eval_expr(length, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_array_pad_result(array, length, value, values)
}

/// Builds an `array_pad()` result by copying values and padding left or right.
fn eval_array_pad_result(
    array: RuntimeCellHandle,
    length: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let target = eval_int_value(length, values)?;
    let target_len = target
        .checked_abs()
        .ok_or(EvalStatus::RuntimeFatal)
        .and_then(|value| usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal))?;
    let result_len = usize::max(len, target_len);
    let pad_count = result_len.saturating_sub(len);
    let mut result = values.array_new(result_len)?;
    let mut output_index = 0usize;

    if target < 0 {
        let (padded, next_index) =
            eval_array_pad_append_repeated(result, output_index, pad_count, value, values)?;
        result = padded;
        output_index = next_index;
    }

    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let source_value = values.array_get(array, source_key)?;
        let target_key = i64::try_from(output_index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let target_key = values.int(target_key)?;
        result = values.array_set(result, target_key, source_value)?;
        output_index += 1;
    }

    if target > 0 {
        result = eval_array_pad_append_repeated(result, output_index, pad_count, value, values)?.0;
    }

    Ok(result)
}

/// Appends the same pad value at consecutive indexed positions in an array result.
fn eval_array_pad_append_repeated(
    mut array: RuntimeCellHandle,
    start_index: usize,
    count: usize,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, usize), EvalStatus> {
    let mut next_index = start_index;
    for _ in 0..count {
        let key = i64::try_from(next_index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(key)?;
        array = values.array_set(array, key, value)?;
        next_index += 1;
    }
    Ok((array, next_index))
}

/// Evaluates PHP `array_flip()` over one eval array expression.
pub(super) fn eval_builtin_array_flip(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_flip_result(array, values)
}

/// Builds the associative result for `array_flip()` using PHP's valid value-key subset.
fn eval_array_flip_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        if !matches!(values.type_tag(value)?, EVAL_TAG_INT | EVAL_TAG_STRING) {
            continue;
        }
        result = values.array_set(result, value, key)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_unique()` over one eval array expression.
pub(super) fn eval_builtin_array_unique(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_unique_result(array, values)
}

/// Builds `array_unique()` by comparing values with PHP's default string comparison mode.
fn eval_array_unique_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut seen = Vec::<Vec<u8>>::with_capacity(len);
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let unique_key = values.string_bytes(value)?;
        if seen.iter().any(|seen_key| seen_key == &unique_key) {
            continue;
        }
        seen.push(unique_key);
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP array projection builtins over one eval array expression.
pub(super) fn eval_builtin_array_projection(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_projection_result(name, array, values)
}

/// Builds the indexed result array for `array_keys()` or `array_values()`.
fn eval_array_projection_result(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = match name {
            "array_keys" => key,
            "array_values" => values.array_get(array, key)?,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        let index = values.int(position as i64)?;
        result = values.array_set(result, index, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `iterator_apply()` for eval-supported Traversable object inputs.
pub(super) fn eval_builtin_iterator_apply(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [iterator, callback] => {
            let iterator = eval_expr(iterator, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            let callback = eval_callable(callback, values)?;
            eval_iterator_apply_result(iterator, &callback, Vec::new(), context, values)
        }
        [iterator, callback, callback_args] => {
            let iterator = eval_expr(iterator, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            let callback = eval_callable(callback, values)?;
            let callback_args = eval_expr(callback_args, context, scope, values)?;
            let callback_args = eval_iterator_apply_arg_values(callback_args, values)?;
            eval_iterator_apply_result(iterator, &callback, callback_args, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Converts the optional `iterator_apply()` callback-args value into call arguments.
fn eval_iterator_apply_arg_values(
    args: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    if values.is_null(args)? {
        return Ok(Vec::new());
    }
    if !values.is_array_like(args)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_array_call_arg_values(args, values)
}

/// Applies a callback to each valid position of an eval-supported Traversable object.
fn eval_iterator_apply_result(
    iterator: RuntimeCellHandle,
    callback: &EvaluatedCallable,
    callback_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(iterator)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let count = match eval_iterator_apply_iterator_object(
        iterator,
        callback,
        &callback_args,
        context,
        values,
    ) {
        Ok(count) => count,
        Err(EvalStatus::UnsupportedConstruct) => {
            let iterator = values.method_call(iterator, "getiterator", Vec::new())?;
            eval_iterator_apply_iterator_object(
                iterator,
                callback,
                &callback_args,
                context,
                values,
            )?
        }
        Err(err) => return Err(err),
    };
    values.int(count)
}

/// Drives one Iterator object through `rewind()`, `valid()`, callback, and `next()`.
fn eval_iterator_apply_iterator_object(
    iterator: RuntimeCellHandle,
    callback: &EvaluatedCallable,
    callback_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let _ = values.method_call(iterator, "rewind", Vec::new())?;
    let mut count = 0_i64;
    loop {
        let valid = values.method_call(iterator, "valid", Vec::new())?;
        if !values.truthy(valid)? {
            return Ok(count);
        }
        count = count.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
        let result = eval_evaluated_callable_with_call_array_args(
            callback,
            callback_args.to_vec(),
            context,
            values,
        )?;
        if !values.truthy(result)? {
            return Ok(count);
        }
        let _ = values.method_call(iterator, "next", Vec::new())?;
    }
}

/// Evaluates PHP `iterator_count()` for eval-supported array iterator inputs.
pub(super) fn eval_builtin_iterator_count(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [iterator] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let iterator = eval_expr(iterator, context, scope, values)?;
    eval_iterator_count_result(iterator, values)
}

/// Returns the element count for eval-supported array iterator inputs.
fn eval_iterator_count_result(
    iterator: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(iterator)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(iterator)?;
    values.int(i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Evaluates PHP `iterator_to_array()` for eval-supported array iterator inputs.
pub(super) fn eval_builtin_iterator_to_array(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [iterator] => {
            let iterator = eval_expr(iterator, context, scope, values)?;
            eval_iterator_to_array_result(iterator, true, values)
        }
        [iterator, preserve_keys] => {
            let iterator = eval_expr(iterator, context, scope, values)?;
            let preserve_keys = eval_expr(preserve_keys, context, scope, values)?;
            let preserve_keys = values.truthy(preserve_keys)?;
            eval_iterator_to_array_result(iterator, preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Copies eval-supported array iterator inputs into a PHP array result.
fn eval_iterator_to_array_result(
    iterator: RuntimeCellHandle,
    preserve_keys: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(iterator)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if preserve_keys {
        return eval_array_copy_preserve_keys(iterator, values);
    }
    eval_array_projection_result("array_values", iterator, values)
}

/// Copies one array-like eval value while preserving iteration keys and order.
fn eval_array_copy_preserve_keys(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_reverse()` over an eval array expression.
pub(super) fn eval_builtin_array_reverse(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array] => {
            let array = eval_expr(array, context, scope, values)?;
            eval_array_reverse_result(array, false, values)
        }
        [array, preserve_keys] => {
            let array = eval_expr(array, context, scope, values)?;
            let preserve_keys = eval_expr(preserve_keys, context, scope, values)?;
            let preserve_keys = values.truthy(preserve_keys)?;
            eval_array_reverse_result(array, preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds an `array_reverse()` result while preserving PHP key rules.
fn eval_array_reverse_result(
    array: RuntimeCellHandle,
    preserve_keys: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut keys = Vec::with_capacity(len);
    let mut has_string_key = false;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        has_string_key |= values.type_tag(key)? == EVAL_TAG_STRING;
        keys.push(key);
    }

    let mut result = if preserve_keys || has_string_key {
        values.assoc_new(len)?
    } else {
        values.array_new(len)?
    };
    let mut next_numeric_key = 0_i64;

    for key in keys.into_iter().rev() {
        let value = values.array_get(array, key)?;
        let target_key = if preserve_keys || values.type_tag(key)? == EVAL_TAG_STRING {
            key
        } else {
            let key = values.int(next_numeric_key)?;
            next_numeric_key += 1;
            key
        };
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_key_exists()` over a key and array expression.
pub(super) fn eval_builtin_array_key_exists(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [key, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let key = eval_expr(key, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    values.array_key_exists(key, array)
}

/// Evaluates PHP array search builtins over a needle and haystack expression.
pub(super) fn eval_builtin_array_search(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [needle, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let needle = eval_expr(needle, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    eval_array_search_result(name, needle, array, values)
}

/// Searches an eval array with PHP's default loose comparison semantics.
fn eval_array_search_result(
    name: &str,
    needle: RuntimeCellHandle,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let equal = values.compare(EvalBinOp::LooseEq, needle, value)?;
        if values.truthy(equal)? {
            return match name {
                "in_array" => values.bool_value(true),
                "array_search" => Ok(key),
                _ => Err(EvalStatus::UnsupportedConstruct),
            };
        }
    }
    match name {
        "in_array" => values.bool_value(false),
        "array_search" => values.bool_value(false),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP value-set array builtins over two eval array expressions.
pub(super) fn eval_builtin_array_value_set(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_array_value_set_result(name, left, right, values)
}

/// Builds `array_diff()` or `array_intersect()` using PHP's default string comparison mode.
fn eval_array_value_set_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left_len = values.array_len(left)?;
    let right_len = values.array_len(right)?;
    let mut right_values = Vec::with_capacity(right_len);
    for position in 0..right_len {
        let key = values.array_iter_key(right, position)?;
        let value = values.array_get(right, key)?;
        right_values.push(values.string_bytes(value)?);
    }

    let mut result = values.assoc_new(left_len)?;
    for position in 0..left_len {
        let key = values.array_iter_key(left, position)?;
        let value = values.array_get(left, key)?;
        let comparable = values.string_bytes(value)?;
        let found = right_values
            .iter()
            .any(|right_value| right_value == &comparable);
        let keep = match name {
            "array_diff" => !found,
            "array_intersect" => found,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        if keep {
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// Evaluates PHP key-set array builtins over two eval array expressions.
pub(super) fn eval_builtin_array_key_set(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_array_key_set_result(name, left, right, values)
}

/// Builds `array_diff_key()` or `array_intersect_key()` by testing first-array keys.
fn eval_array_key_set_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left_len = values.array_len(left)?;
    let mut result = values.assoc_new(left_len)?;
    for position in 0..left_len {
        let key = values.array_iter_key(left, position)?;
        let value = values.array_get(left, key)?;
        let exists = values.array_key_exists(key, right)?;
        let found = values.truthy(exists)?;
        let keep = match name {
            "array_diff_key" => !found,
            "array_intersect_key" => found,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        if keep {
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// Evaluates PHP `array_rand()` over one eval array expression.
pub(super) fn eval_builtin_array_rand(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_rand_result(array, values)
}

/// Returns a valid random key from a non-empty eval array.
fn eval_array_rand_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    if len == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let position = eval_random_position(len);
    values.array_iter_key(array, position)
}

/// Chooses a pseudo-random array position within `[0, len)`.
fn eval_random_position(len: usize) -> usize {
    (eval_random_u128() % (len as u128)) as usize
}

/// Produces a process-local pseudo-random word for non-cryptographic eval builtins.
fn eval_random_u128() -> u128 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = u128::from(EVAL_RANDOM_COUNTER.fetch_add(1, Ordering::Relaxed));
    let pid = u128::from(std::process::id());
    let mut value = nanos ^ (counter.wrapping_mul(0x9e37_79b9_7f4a_7c15)) ^ (pid << 64);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Evaluates PHP `rand()` and `mt_rand()` over zero args or an inclusive range.
pub(super) fn eval_builtin_rand(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_rand_result(None, None, values),
        [min, max] => {
            let min = eval_expr(min, context, scope, values)?;
            let max = eval_expr(max, context, scope, values)?;
            eval_rand_result(Some(min), Some(max), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `random_int()` over an inclusive integer range.
pub(super) fn eval_builtin_random_int(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [min, max] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let min = eval_expr(min, context, scope, values)?;
    let max = eval_expr(max, context, scope, values)?;
    eval_random_int_result(min, max, values)
}

/// Returns one non-cryptographic random integer using PHP's inclusive range rules.
fn eval_rand_result(
    min: Option<RuntimeCellHandle>,
    max: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (min, max) = match (min, max) {
        (None, None) => (0, i64::from(i32::MAX)),
        (Some(min), Some(max)) => (eval_int_value(min, values)?, eval_int_value(max, values)?),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let low = min.min(max);
    let high = min.max(max);
    let width = (i128::from(high) - i128::from(low) + 1) as u128;
    let offset = (eval_random_u128() % width) as i128;
    let sampled = i128::from(low) + offset;
    let sampled = i64::try_from(sampled).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(sampled)
}

/// Returns one eval `random_int()` value in the inclusive range `[min, max]`.
fn eval_random_int_result(
    min: RuntimeCellHandle,
    max: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let min = eval_int_value(min, values)?;
    let max = eval_int_value(max, values)?;
    if min > max {
        return Err(EvalStatus::RuntimeFatal);
    }
    let width = (i128::from(max) - i128::from(min) + 1) as u128;
    let offset = (eval_random_u128() % width) as i128;
    let sampled = i128::from(min) + offset;
    let sampled = i64::try_from(sampled).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(sampled)
}

/// Evaluates PHP `range()` over integer-compatible start and end expressions.
pub(super) fn eval_builtin_range(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [start, end] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let start = eval_expr(start, context, scope, values)?;
    let end = eval_expr(end, context, scope, values)?;
    eval_range_result(start, end, values)
}

/// Builds an inclusive ascending or descending integer `range()` result.
fn eval_range_result(
    start: RuntimeCellHandle,
    end: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let start = eval_int_value(start, values)?;
    let end = eval_int_value(end, values)?;
    let distance = if start <= end {
        end.checked_sub(start).ok_or(EvalStatus::RuntimeFatal)?
    } else {
        start.checked_sub(end).ok_or(EvalStatus::RuntimeFatal)?
    };
    let count = distance.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
    let count = usize::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?;
    let step = if start <= end { 1_i64 } else { -1_i64 };
    let mut current = start;
    let mut result = values.array_new(count)?;

    for index in 0..count {
        let key = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(key)?;
        let value = values.int(current)?;
        result = values.array_set(result, key, value)?;
        if index + 1 < count {
            current = current.checked_add(step).ok_or(EvalStatus::RuntimeFatal)?;
        }
    }
    Ok(result)
}

/// Evaluates PHP `array_merge()` over two array expressions.
pub(super) fn eval_builtin_array_merge(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_array_merge_result(left, right, values)
}

/// Builds an `array_merge()` result with PHP numeric reindexing and string-key overwrites.
fn eval_array_merge_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left_len = values.array_len(left)?;
    let right_len = values.array_len(right)?;
    let capacity = left_len
        .checked_add(right_len)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut result = values.assoc_new(capacity)?;
    let mut next_numeric_key = 0_i64;
    result = eval_array_merge_append_operand(result, left, &mut next_numeric_key, values)?;
    eval_array_merge_append_operand(result, right, &mut next_numeric_key, values)
}

/// Appends one source array to an `array_merge()` result using PHP key handling.
fn eval_array_merge_append_operand(
    mut result: RuntimeCellHandle,
    source: RuntimeCellHandle,
    next_numeric_key: &mut i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(source)?;
    for position in 0..len {
        let source_key = values.array_iter_key(source, position)?;
        let source_value = values.array_get(source, source_key)?;
        let target_key = if values.type_tag(source_key)? == EVAL_TAG_STRING {
            source_key
        } else {
            let target_key = values.int(*next_numeric_key)?;
            *next_numeric_key = (*next_numeric_key)
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            target_key
        };
        result = values.array_set(result, target_key, source_value)?;
    }
    Ok(result)
}

/// Evaluates PHP `explode()` over separator and string expressions.
pub(super) fn eval_builtin_explode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [separator, string] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let separator = eval_expr(separator, context, scope, values)?;
    let string = eval_expr(string, context, scope, values)?;
    eval_explode_result(separator, string, values)
}

/// Splits one PHP byte string into an indexed array using a non-empty separator.
fn eval_explode_result(
    separator: RuntimeCellHandle,
    string: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let separator = values.string_bytes(separator)?;
    if separator.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let string = values.string_bytes(string)?;
    let mut result = values.array_new(0)?;
    let mut start = 0;
    let mut index = 0_i64;
    while let Some(found) = eval_find_subslice(&string, &separator, start) {
        result = eval_push_explode_segment(result, index, &string[start..found], values)?;
        start = found + separator.len();
        index += 1;
    }
    eval_push_explode_segment(result, index, &string[start..], values)
}

/// Appends one split segment to an indexed `explode()` result array.
fn eval_push_explode_segment(
    array: RuntimeCellHandle,
    index: i64,
    segment: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.int(index)?;
    let value = values.string_bytes_value(segment)?;
    values.array_set(array, key, value)
}

/// Finds `needle` inside `haystack` starting from one byte offset.
fn eval_find_subslice(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    haystack
        .get(start..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + start)
}

/// Evaluates PHP `implode()` over separator and array expressions.
pub(super) fn eval_builtin_implode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [separator, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let separator = eval_expr(separator, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    eval_implode_result(separator, array, values)
}

/// Joins array values in eval iteration order using PHP string conversion.
fn eval_implode_result(
    separator: RuntimeCellHandle,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !values.is_array_like(array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let separator = values.string_bytes(separator)?;
    let len = values.array_len(array)?;
    let mut output = Vec::new();
    for position in 0..len {
        if position > 0 {
            output.extend_from_slice(&separator);
        }
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let value = values.string_bytes(value)?;
        output.extend_from_slice(&value);
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP's `ceil(...)` over one eval expression.
pub(super) fn eval_builtin_ceil(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.ceil(value)
}

/// Evaluates PHP's `floor(...)` over one eval expression.
pub(super) fn eval_builtin_floor(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.floor(value)
}

/// Evaluates PHP's zero-argument `pi()` builtin.
pub(super) fn eval_builtin_pi(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.float(std::f64::consts::PI)
}

/// Evaluates PHP's `pow(...)` over two eval expressions.
pub(super) fn eval_builtin_pow(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    values.pow(left, right)
}

/// Evaluates PHP's `round(...)` over one value and an optional precision expression.
pub(super) fn eval_builtin_round(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            values.round(value, None)
        }
        [value, precision] => {
            let value = eval_expr(value, context, scope, values)?;
            let precision = eval_expr(precision, context, scope, values)?;
            values.round(value, Some(precision))
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `number_format(...)` over one number and optional separators.
pub(super) fn eval_builtin_number_format(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_number_format_result(value, None, None, None, values)
        }
        [value, decimals] => {
            let value = eval_expr(value, context, scope, values)?;
            let decimals = eval_expr(decimals, context, scope, values)?;
            eval_number_format_result(value, Some(decimals), None, None, values)
        }
        [value, decimals, decimal_separator] => {
            let value = eval_expr(value, context, scope, values)?;
            let decimals = eval_expr(decimals, context, scope, values)?;
            let decimal_separator = eval_expr(decimal_separator, context, scope, values)?;
            eval_number_format_result(value, Some(decimals), Some(decimal_separator), None, values)
        }
        [value, decimals, decimal_separator, thousands_separator] => {
            let value = eval_expr(value, context, scope, values)?;
            let decimals = eval_expr(decimals, context, scope, values)?;
            let decimal_separator = eval_expr(decimal_separator, context, scope, values)?;
            let thousands_separator = eval_expr(thousands_separator, context, scope, values)?;
            eval_number_format_result(
                value,
                Some(decimals),
                Some(decimal_separator),
                Some(thousands_separator),
                values,
            )
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Formats one PHP numeric value with grouped thousands and fixed decimals.
fn eval_number_format_result(
    value: RuntimeCellHandle,
    decimals: Option<RuntimeCellHandle>,
    decimal_separator: Option<RuntimeCellHandle>,
    thousands_separator: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_float_value(value, values)?;
    let decimals = match decimals {
        Some(decimals) => eval_int_value(decimals, values)?,
        None => 0,
    };
    let decimal_separator = match decimal_separator {
        Some(separator) => values.string_bytes(separator)?,
        None => b".".to_vec(),
    };
    let thousands_separator = match thousands_separator {
        Some(separator) => values.string_bytes(separator)?,
        None => b",".to_vec(),
    };
    let output =
        eval_number_format_bytes(value, decimals, &decimal_separator, &thousands_separator)?;
    values.string_bytes_value(&output)
}

/// Evaluates direct positional `sprintf()` or `printf()` calls in source order.
pub(super) fn eval_builtin_sprintf_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_sprintf_like_result(name, &evaluated_args, values)
}

/// Evaluates direct positional `vsprintf()` or `vprintf()` calls in source order.
pub(super) fn eval_builtin_vsprintf_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_vsprintf_like_result(name, &evaluated_args, values)
}

/// Evaluates direct positional `sscanf()` calls in source order.
pub(super) fn eval_builtin_sscanf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let input = eval_expr(&args[0], context, scope, values)?;
    let format = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        eval_expr(arg, context, scope, values)?;
    }
    eval_sscanf_result(input, format, values)
}

/// Dispatches already evaluated `sprintf()` or `printf()` arguments.
fn eval_sprintf_like_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "sprintf" => eval_sprintf_result(evaluated_args, values),
        "printf" => eval_printf_result(evaluated_args, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Dispatches already evaluated `vsprintf()` or `vprintf()` arguments.
fn eval_vsprintf_like_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "vsprintf" => eval_vsprintf_result(evaluated_args, values),
        "vprintf" => eval_vprintf_result(evaluated_args, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Parses one string through the eval `sscanf()` subset and returns an indexed array.
fn eval_sscanf_result(
    input: RuntimeCellHandle,
    format: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let input = values.string_bytes(input)?;
    let format = values.string_bytes(format)?;
    let matches = eval_sscanf_matches(&input, &format);
    let mut result = values.array_new(matches.len())?;
    for (index, matched) in matches.iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        let value = values.string_bytes_value(matched)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Extracts `%d`, `%f`, `%s`, and `%%` matches with the same subset as native `sscanf()`.
fn eval_sscanf_matches(input: &[u8], format: &[u8]) -> Vec<Vec<u8>> {
    let mut matches = Vec::new();
    let mut input_index = 0;
    let mut format_index = 0;

    while format_index < format.len() {
        if format[format_index] != b'%' {
            if input_index >= input.len() || input[input_index] != format[format_index] {
                break;
            }
            input_index += 1;
            format_index += 1;
            continue;
        }

        format_index += 1;
        if format_index >= format.len() {
            break;
        }

        match format[format_index] {
            b'%' => {
                if input_index >= input.len() || input[input_index] != b'%' {
                    break;
                }
                input_index += 1;
            }
            b'd' => matches.push(eval_sscanf_scan_int(input, &mut input_index)),
            b'f' => matches.push(eval_sscanf_scan_float(input, &mut input_index)),
            b's' => matches.push(eval_sscanf_scan_word(input, &mut input_index)),
            _ => {}
        }
        format_index += 1;
    }

    matches
}

/// Scans the native `sscanf()` `%d` subset as a matched byte slice.
fn eval_sscanf_scan_int(input: &[u8], input_index: &mut usize) -> Vec<u8> {
    let start = *input_index;
    if input.get(*input_index) == Some(&b'-') {
        *input_index += 1;
    }
    while input
        .get(*input_index)
        .is_some_and(|byte| byte.is_ascii_digit())
    {
        *input_index += 1;
    }
    input[start..*input_index].to_vec()
}

/// Scans the native `sscanf()` `%f` subset as a matched byte slice.
fn eval_sscanf_scan_float(input: &[u8], input_index: &mut usize) -> Vec<u8> {
    let start = *input_index;
    if input
        .get(*input_index)
        .is_some_and(|byte| matches!(byte, b'+' | b'-'))
    {
        *input_index += 1;
    }
    while input
        .get(*input_index)
        .is_some_and(|byte| byte.is_ascii_digit())
    {
        *input_index += 1;
    }
    if input.get(*input_index) == Some(&b'.') {
        *input_index += 1;
        while input
            .get(*input_index)
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            *input_index += 1;
        }
    }
    if input
        .get(*input_index)
        .is_some_and(|byte| matches!(byte, b'e' | b'E'))
    {
        *input_index += 1;
        if input
            .get(*input_index)
            .is_some_and(|byte| matches!(byte, b'+' | b'-'))
        {
            *input_index += 1;
        }
        while input
            .get(*input_index)
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            *input_index += 1;
        }
    }
    input[start..*input_index].to_vec()
}

/// Scans the native `sscanf()` `%s` subset as a non-space byte word.
fn eval_sscanf_scan_word(input: &[u8], input_index: &mut usize) -> Vec<u8> {
    let start = *input_index;
    while input
        .get(*input_index)
        .is_some_and(|byte| !matches!(byte, b' ' | b'\t' | b'\n'))
    {
        *input_index += 1;
    }
    input[start..*input_index].to_vec()
}

/// Formats `sprintf()` arguments and returns the resulting PHP string.
fn eval_sprintf_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((format, format_args)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    let output = eval_sprintf_bytes(&format, format_args, values)?;
    values.string_bytes_value(&output)
}

/// Formats `printf()` arguments, echoes the result, and returns its byte count.
fn eval_printf_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((format, format_args)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    let output = eval_sprintf_bytes(&format, format_args, values)?;
    let len = i64::try_from(output.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let output = values.string_bytes_value(&output)?;
    values.echo(output)?;
    values.int(len)
}

/// Formats `vsprintf()` array arguments and returns the resulting PHP string.
fn eval_vsprintf_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [format, array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    let format_args = eval_sprintf_argument_array_values(*array, values)?;
    let output = eval_sprintf_bytes(&format, &format_args, values)?;
    values.string_bytes_value(&output)
}

/// Formats `vprintf()` array arguments, echoes the result, and returns its byte count.
fn eval_vprintf_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [format, array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    let format_args = eval_sprintf_argument_array_values(*array, values)?;
    let output = eval_sprintf_bytes(&format, &format_args, values)?;
    let len = i64::try_from(output.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let output = values.string_bytes_value(&output)?;
    values.echo(output)?;
    values.int(len)
}

/// Reads `vsprintf()` values in PHP array iteration order while ignoring keys.
fn eval_sprintf_argument_array_values(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if !values.is_array_like(array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(array)?;
    let mut args = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        args.push(values.array_get(array, key)?);
    }
    Ok(args)
}

/// Formats one printf-style byte string through eval runtime value coercions.
fn eval_sprintf_bytes(
    format: &[u8],
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let mut output = Vec::new();
    let mut index = 0;
    let mut arg_index = 0;
    while index < format.len() {
        if format[index] != b'%' {
            output.push(format[index]);
            index += 1;
            continue;
        }
        index += 1;
        if index >= format.len() {
            break;
        }
        if format[index] == b'%' {
            output.push(b'%');
            index += 1;
            continue;
        }

        let (spec, next_index) = eval_parse_sprintf_spec(format, index)?;
        index = next_index;
        let Some(arg) = args.get(arg_index).copied() else {
            return Err(EvalStatus::RuntimeFatal);
        };
        arg_index += 1;
        let bytes = eval_format_sprintf_arg(spec, arg, values)?;
        output.extend_from_slice(&bytes);
    }
    Ok(output)
}

/// Parses flags, width, precision, and terminal type for one format specifier.
fn eval_parse_sprintf_spec(
    format: &[u8],
    mut index: usize,
) -> Result<(EvalSprintfSpec, usize), EvalStatus> {
    let mut spec = EvalSprintfSpec {
        left_align: false,
        force_sign: false,
        space_sign: false,
        zero_pad: false,
        alternate: false,
        width: None,
        precision: None,
        specifier: 0,
    };
    while index < format.len() {
        match format[index] {
            b'-' => spec.left_align = true,
            b'+' => spec.force_sign = true,
            b' ' => spec.space_sign = true,
            b'0' => spec.zero_pad = true,
            b'#' => spec.alternate = true,
            _ => break,
        }
        index += 1;
    }
    let (width, next_index) = eval_parse_sprintf_number(format, index)?;
    spec.width = width;
    index = next_index;
    if index < format.len() && format[index] == b'.' {
        let (precision, next_index) = eval_parse_sprintf_number(format, index + 1)?;
        spec.precision = Some(precision.unwrap_or(0));
        index = next_index;
    }
    if index >= format.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    spec.specifier = format[index];
    Ok((spec, index + 1))
}

/// Parses an unsigned decimal number from a format specifier component.
fn eval_parse_sprintf_number(
    format: &[u8],
    mut index: usize,
) -> Result<(Option<usize>, usize), EvalStatus> {
    let start = index;
    let mut value = 0usize;
    while index < format.len() && format[index].is_ascii_digit() {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(usize::from(format[index] - b'0')))
            .ok_or(EvalStatus::RuntimeFatal)?;
        index += 1;
    }
    if index == start {
        Ok((None, index))
    } else {
        Ok((Some(value), index))
    }
}

/// Formats one runtime value according to a parsed eval sprintf specifier.
fn eval_format_sprintf_arg(
    spec: EvalSprintfSpec,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    match spec.specifier {
        b's' => eval_format_sprintf_string(spec, value, values),
        b'f' | b'e' | b'g' => eval_format_sprintf_float(spec, value, values),
        b'c' => eval_format_sprintf_char(spec, value, values),
        _ => eval_format_sprintf_int(spec, value, values),
    }
}

/// Formats a `%s` operand after PHP string coercion.
fn eval_format_sprintf_string(
    spec: EvalSprintfSpec,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    if let Some(precision) = spec.precision {
        bytes.truncate(precision);
    }
    Ok(eval_sprintf_apply_width(bytes, spec, false))
}

/// Formats an integer-like operand for decimal, unsigned, hex, and octal specifiers.
fn eval_format_sprintf_int(
    spec: EvalSprintfSpec,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let value = eval_int_value(value, values)?;
    let mut output = Vec::new();
    match spec.specifier {
        b'u' => {
            let digits = eval_sprintf_precision_pad((value as u64).to_string().into_bytes(), spec);
            output.extend_from_slice(&digits);
        }
        b'x' | b'X' => {
            let unsigned = value as u64;
            if spec.alternate && unsigned != 0 {
                output.extend_from_slice(if spec.specifier == b'X' { b"0X" } else { b"0x" });
            }
            let digits = if spec.specifier == b'X' {
                format!("{unsigned:X}").into_bytes()
            } else {
                format!("{unsigned:x}").into_bytes()
            };
            output.extend_from_slice(&eval_sprintf_precision_pad(digits, spec));
        }
        b'o' => {
            let unsigned = value as u64;
            let mut digits = eval_sprintf_precision_pad(format!("{unsigned:o}").into_bytes(), spec);
            if spec.alternate && !digits.starts_with(b"0") {
                output.push(b'0');
            }
            output.append(&mut digits);
        }
        _ => {
            let value = value as i128;
            let magnitude = if value < 0 {
                (-value) as u128
            } else {
                value as u128
            };
            if value < 0 {
                output.push(b'-');
            } else if spec.force_sign {
                output.push(b'+');
            } else if spec.space_sign {
                output.push(b' ');
            }
            let digits = eval_sprintf_precision_pad(magnitude.to_string().into_bytes(), spec);
            output.extend_from_slice(&digits);
        }
    }
    Ok(eval_sprintf_apply_width(output, spec, true))
}

/// Formats a `%c` operand as the low byte of its integer value.
fn eval_format_sprintf_char(
    spec: EvalSprintfSpec,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let value = eval_int_value(value, values)?;
    Ok(eval_sprintf_apply_width(vec![value as u8], spec, false))
}

/// Formats a floating-point operand for the eval printf-family subset.
fn eval_format_sprintf_float(
    spec: EvalSprintfSpec,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let value = eval_float_value(value, values)?;
    let precision = spec.precision.unwrap_or(6);
    let mut output = if value.is_nan() {
        b"NAN".to_vec()
    } else if value.is_infinite() {
        b"INF".to_vec()
    } else {
        match spec.specifier {
            b'e' => format!("{value:.precision$e}").into_bytes(),
            b'g' => format!("{value:.precision$}").into_bytes(),
            _ => format!("{value:.precision$}").into_bytes(),
        }
    };
    if value.is_sign_negative() && !output.starts_with(b"-") {
        output.insert(0, b'-');
    } else if value.is_sign_positive() && value.is_finite() {
        if spec.force_sign {
            output.insert(0, b'+');
        } else if spec.space_sign {
            output.insert(0, b' ');
        }
    }
    Ok(eval_sprintf_apply_width(output, spec, true))
}

/// Applies integer precision by left-padding digits with zeros.
fn eval_sprintf_precision_pad(mut digits: Vec<u8>, spec: EvalSprintfSpec) -> Vec<u8> {
    if matches!(spec.precision, Some(0)) && digits == b"0" {
        digits.clear();
        return digits;
    }
    let Some(precision) = spec.precision else {
        return digits;
    };
    if digits.len() >= precision {
        return digits;
    }
    let mut output = vec![b'0'; precision - digits.len()];
    output.append(&mut digits);
    output
}

/// Applies field width and alignment to one formatted eval sprintf replacement.
fn eval_sprintf_apply_width(mut bytes: Vec<u8>, spec: EvalSprintfSpec, numeric: bool) -> Vec<u8> {
    let Some(width) = spec.width else {
        return bytes;
    };
    if bytes.len() >= width {
        return bytes;
    }
    let pad_len = width - bytes.len();
    if spec.left_align {
        bytes.extend(std::iter::repeat_n(b' ', pad_len));
        return bytes;
    }
    if numeric && spec.zero_pad && spec.precision.is_none() {
        let prefix_len = eval_sprintf_zero_pad_prefix_len(&bytes);
        let mut output = Vec::with_capacity(width);
        output.extend_from_slice(&bytes[..prefix_len]);
        output.extend(std::iter::repeat_n(b'0', pad_len));
        output.extend_from_slice(&bytes[prefix_len..]);
        return output;
    }
    let mut output = Vec::with_capacity(width);
    output.extend(std::iter::repeat_n(b' ', pad_len));
    output.append(&mut bytes);
    output
}

/// Returns the sign and alternate-prefix length that should precede zero padding.
fn eval_sprintf_zero_pad_prefix_len(bytes: &[u8]) -> usize {
    let mut prefix_len = usize::from(matches!(bytes.first(), Some(b'+' | b'-' | b' ')));
    if bytes.len() >= prefix_len + 2
        && bytes[prefix_len] == b'0'
        && matches!(bytes[prefix_len + 1], b'x' | b'X')
    {
        prefix_len += 2;
    }
    prefix_len
}

/// Converts one eval value to PHP float and returns the scalar payload.
pub(super) fn eval_float_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<f64, EvalStatus> {
    let value = values.cast_float(value)?;
    let bytes = values.string_bytes(value)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .parse::<f64>()
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Produces PHP `number_format()` bytes for finite scalar values.
fn eval_number_format_bytes(
    value: f64,
    decimals: i64,
    decimal_separator: &[u8],
    thousands_separator: &[u8],
) -> Result<Vec<u8>, EvalStatus> {
    if !value.is_finite() {
        return Ok(value.to_string().into_bytes());
    }
    let decimals = decimals.clamp(-308, 308);
    let display_decimals = decimals.max(0) as usize;
    let abs_value = value.abs();
    let scaled = if decimals >= 0 {
        let scale = 10_f64.powi(decimals as i32);
        (abs_value * scale).round()
    } else {
        let scale = 10_f64.powi((-decimals) as i32);
        (abs_value / scale).round() * scale
    };
    if scaled > (u128::MAX as f64) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let scaled = scaled as u128;
    let scale = 10_u128
        .checked_pow(display_decimals as u32)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let integer = if display_decimals == 0 {
        scaled
    } else {
        scaled / scale
    };
    let fraction = if display_decimals == 0 {
        0
    } else {
        scaled % scale
    };

    let mut output = Vec::new();
    if value.is_sign_negative() && scaled != 0 {
        output.push(b'-');
    }
    eval_append_grouped_decimal(&mut output, integer, thousands_separator);
    if display_decimals > 0 {
        output.extend_from_slice(decimal_separator);
        let fraction = format!("{fraction:0display_decimals$}");
        output.extend_from_slice(fraction.as_bytes());
    }
    Ok(output)
}

/// Appends one unsigned decimal integer with optional three-digit grouping.
fn eval_append_grouped_decimal(output: &mut Vec<u8>, value: u128, separator: &[u8]) {
    let digits = value.to_string();
    if separator.is_empty() {
        output.extend_from_slice(digits.as_bytes());
        return;
    }
    let first_group = match digits.len() % 3 {
        0 => 3,
        len => len,
    };
    output.extend_from_slice(&digits.as_bytes()[..first_group]);
    let mut index = first_group;
    while index < digits.len() {
        output.extend_from_slice(separator);
        output.extend_from_slice(&digits.as_bytes()[index..index + 3]);
        index += 3;
    }
}

/// Evaluates PHP's `sqrt(...)` over one eval expression.
pub(super) fn eval_builtin_sqrt(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.sqrt(value)
}

/// Evaluates PHP's `strrev(...)` over one eval expression.
pub(super) fn eval_builtin_strrev(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.strrev(value)
}

/// Evaluates PHP's `chr(...)` over one eval expression.
pub(super) fn eval_builtin_chr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_chr_result(value, values)
}

/// Converts one eval value to a PHP integer and returns the low byte as a string.
fn eval_chr_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_int_value(value, values)?;
    values.string_bytes_value(&[value as u8])
}

/// Evaluates PHP's `str_repeat(...)` over one eval expression pair.
pub(super) fn eval_builtin_str_repeat(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value, times] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let times = eval_expr(times, context, scope, values)?;
    eval_str_repeat_result(value, times, values)
}

/// Repeats one PHP string byte sequence according to a PHP-cast integer count.
fn eval_str_repeat_result(
    value: RuntimeCellHandle,
    times: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let times = eval_int_value(times, values)?;
    if times < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let times = usize::try_from(times).map_err(|_| EvalStatus::RuntimeFatal)?;
    let capacity = bytes
        .len()
        .checked_mul(times)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut output = Vec::with_capacity(capacity);
    for _ in 0..times {
        output.extend_from_slice(&bytes);
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP's `str_replace(...)` or `str_ireplace(...)` over eval expressions.
pub(super) fn eval_builtin_str_replace(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [search, replace, subject] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let search = eval_expr(search, context, scope, values)?;
    let replace = eval_expr(replace, context, scope, values)?;
    let subject = eval_expr(subject, context, scope, values)?;
    eval_str_replace_result(name, search, replace, subject, values)
}

/// Replaces every non-overlapping occurrence of a byte-string needle in a subject.
fn eval_str_replace_result(
    name: &str,
    search: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let search = values.string_bytes(search)?;
    let replace = values.string_bytes(replace)?;
    let subject = values.string_bytes(subject)?;
    if search.is_empty() {
        return values.string_bytes_value(&subject);
    }

    let mut output = Vec::with_capacity(subject.len());
    let mut start = 0;
    while let Some(found) = eval_find_replace_match(name, &subject, &search, start)? {
        output.extend_from_slice(&subject[start..found]);
        output.extend_from_slice(&replace);
        start = found + search.len();
    }
    output.extend_from_slice(&subject[start..]);
    values.string_bytes_value(&output)
}

/// Finds the next replacement match using case-sensitive or ASCII-insensitive comparison.
fn eval_find_replace_match(
    name: &str,
    subject: &[u8],
    search: &[u8],
    start: usize,
) -> Result<Option<usize>, EvalStatus> {
    match name {
        "str_replace" => Ok(eval_find_subslice(subject, search, start)),
        "str_ireplace" => Ok(subject
            .get(start..)
            .and_then(|tail| {
                tail.windows(search.len())
                    .position(|window| window.eq_ignore_ascii_case(search))
            })
            .map(|position| position + start)),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP `str_pad(...)` over a string, target length, pad string, and pad mode.
pub(super) fn eval_builtin_str_pad(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_str_pad_result(value, length, None, None, values)
        }
        [value, length, pad_string] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            let pad_string = eval_expr(pad_string, context, scope, values)?;
            eval_str_pad_result(value, length, Some(pad_string), None, values)
        }
        [value, length, pad_string, pad_type] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            let pad_string = eval_expr(pad_string, context, scope, values)?;
            let pad_type = eval_expr(pad_type, context, scope, values)?;
            eval_str_pad_result(value, length, Some(pad_string), Some(pad_type), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Pads one byte string to a PHP target length using cyclic pad bytes.
fn eval_str_pad_result(
    value: RuntimeCellHandle,
    length: RuntimeCellHandle,
    pad_string: Option<RuntimeCellHandle>,
    pad_type: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let target_length = eval_int_value(length, values)?;
    let Ok(target_length) = usize::try_from(target_length) else {
        return values.string_bytes_value(&bytes);
    };
    if target_length <= bytes.len() {
        return values.string_bytes_value(&bytes);
    }

    let pad_string = match pad_string {
        Some(pad_string) => values.string_bytes(pad_string)?,
        None => b" ".to_vec(),
    };
    if pad_string.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let pad_type = match pad_type {
        Some(pad_type) => eval_int_value(pad_type, values)?,
        None => 1,
    };
    let (left_pad, right_pad) = eval_str_pad_sides(target_length - bytes.len(), pad_type)?;
    let capacity = bytes
        .len()
        .checked_add(left_pad)
        .and_then(|size| size.checked_add(right_pad))
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut output = Vec::with_capacity(capacity);
    eval_append_repeated_pad(&mut output, &pad_string, left_pad);
    output.extend_from_slice(&bytes);
    eval_append_repeated_pad(&mut output, &pad_string, right_pad);
    values.string_bytes_value(&output)
}

/// Splits a `str_pad()` pad budget into left and right byte counts.
fn eval_str_pad_sides(pad_budget: usize, pad_type: i64) -> Result<(usize, usize), EvalStatus> {
    match pad_type {
        0 => Ok((pad_budget, 0)),
        1 => Ok((0, pad_budget)),
        2 => Ok((pad_budget / 2, pad_budget - (pad_budget / 2))),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Appends `count` bytes by cycling through the provided non-empty pad string.
fn eval_append_repeated_pad(output: &mut Vec<u8>, pad_string: &[u8], count: usize) {
    for index in 0..count {
        output.push(pad_string[index % pad_string.len()]);
    }
}

/// Evaluates PHP `str_split(...)` over one string and optional chunk length.
pub(super) fn eval_builtin_str_split(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_str_split_result(value, None, values)
        }
        [value, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_str_split_result(value, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Splits one byte string into indexed string chunks using PHP `str_split()` rules.
fn eval_str_split_result(
    value: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let length = match length {
        Some(length) => eval_int_value(length, values)?,
        None => 1,
    };
    if length <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut result = values.array_new(0)?;
    for (index, chunk) in bytes.chunks(length).enumerate() {
        let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(index)?;
        let value = values.string_bytes_value(chunk)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP's `nl2br(...)` over one eval expression and optional XHTML flag.
pub(super) fn eval_builtin_nl2br(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_nl2br_result(value, true, values)
        }
        [value, use_xhtml] => {
            let value = eval_expr(value, context, scope, values)?;
            let use_xhtml = eval_expr(use_xhtml, context, scope, values)?;
            let use_xhtml = values.truthy(use_xhtml)?;
            eval_nl2br_result(value, use_xhtml, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Inserts an HTML line break before each PHP newline sequence while preserving bytes.
fn eval_nl2br_result(
    value: RuntimeCellHandle,
    use_xhtml: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let br = if use_xhtml {
        b"<br />".as_slice()
    } else {
        b"<br>".as_slice()
    };
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\r' || byte == b'\n' {
            output.extend_from_slice(br);
            output.push(byte);
            if index + 1 < bytes.len()
                && ((byte == b'\r' && bytes[index + 1] == b'\n')
                    || (byte == b'\n' && bytes[index + 1] == b'\r'))
            {
                output.push(bytes[index + 1]);
                index += 2;
                continue;
            }
        } else {
            output.push(byte);
        }
        index += 1;
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP's `substr(...)` over one eval string, offset, and optional length.
pub(super) fn eval_builtin_substr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, offset] => {
            let value = eval_expr(value, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            eval_substr_result(value, offset, None, values)
        }
        [value, offset, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_substr_result(value, offset, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Slices a PHP byte string using PHP `substr()` offset and length rules.
fn eval_substr_result(
    value: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let total = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let offset = eval_int_value(offset, values)?;
    let start = if offset < 0 {
        (total + offset).max(0)
    } else {
        offset.min(total)
    };
    let end = match length {
        None => total,
        Some(length) if values.is_null(length)? => total,
        Some(length) => {
            let length = eval_int_value(length, values)?;
            if length < 0 {
                (total + length).max(0)
            } else {
                start.saturating_add(length).min(total)
            }
        }
    };
    let end = end.max(start);
    let start = usize::try_from(start).map_err(|_| EvalStatus::RuntimeFatal)?;
    let end = usize::try_from(end).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string_bytes_value(&bytes[start..end])
}

/// Evaluates PHP's `substr_replace(...)` over eval scalar byte strings.
pub(super) fn eval_builtin_substr_replace(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, replace, offset] => {
            let value = eval_expr(value, context, scope, values)?;
            let replace = eval_expr(replace, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            eval_substr_replace_result(value, replace, offset, None, values)
        }
        [value, replace, offset, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let replace = eval_expr(replace, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_substr_replace_result(value, replace, offset, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Replaces the byte range selected by PHP `substr_replace()` scalar rules.
fn eval_substr_replace_result(
    value: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let replacement = values.string_bytes(replace)?;
    let total = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let offset = eval_int_value(offset, values)?;
    let start = if offset < 0 {
        (total + offset).max(0)
    } else {
        offset.min(total)
    };
    let end = match length {
        None => total,
        Some(length) if values.is_null(length)? => total,
        Some(length) => {
            let length = eval_int_value(length, values)?;
            if length < 0 {
                (total + length).max(start)
            } else {
                start.saturating_add(length).min(total)
            }
        }
    };
    let start = usize::try_from(start).map_err(|_| EvalStatus::RuntimeFatal)?;
    let end = usize::try_from(end).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut output = Vec::with_capacity(bytes.len() + replacement.len());
    output.extend_from_slice(&bytes[..start]);
    output.extend_from_slice(&replacement);
    output.extend_from_slice(&bytes[end..]);
    values.string_bytes_value(&output)
}

/// Evaluates eval HTML entity encode/decode builtins over one string expression.
pub(super) fn eval_builtin_html_entity(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_html_entity_result(name, value, values)
}

/// Applies the eval-supported HTML entity transform for one PHP string value.
fn eval_html_entity_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "htmlspecialchars" | "htmlentities" => eval_htmlspecialchars_result(value, values),
        "html_entity_decode" => eval_html_entity_decode_result(value, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Encodes the HTML-special byte characters covered by elephc's static helper.
fn eval_htmlspecialchars_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    for byte in bytes {
        match byte {
            b'&' => output.extend_from_slice(b"&amp;"),
            b'<' => output.extend_from_slice(b"&lt;"),
            b'>' => output.extend_from_slice(b"&gt;"),
            b'"' => output.extend_from_slice(b"&quot;"),
            b'\'' => output.extend_from_slice(b"&#039;"),
            _ => output.push(byte),
        }
    }
    values.string_bytes_value(&output)
}

/// Decodes one pass of the HTML entities emitted by the eval/static encoders.
fn eval_html_entity_decode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'&' {
            if let Some((decoded, width)) = eval_html_entity_at(&bytes[index..]) {
                output.push(decoded);
                index += width;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    values.string_bytes_value(&output)
}

/// Returns the decoded byte and consumed width for one supported HTML entity.
fn eval_html_entity_at(bytes: &[u8]) -> Option<(u8, usize)> {
    for (entity, decoded) in [
        (b"&lt;".as_slice(), b'<'),
        (b"&gt;".as_slice(), b'>'),
        (b"&quot;".as_slice(), b'"'),
        (b"&#039;".as_slice(), b'\''),
        (b"&#39;".as_slice(), b'\''),
        (b"&amp;".as_slice(), b'&'),
    ] {
        if bytes.starts_with(entity) {
            return Some((decoded, entity.len()));
        }
    }
    None
}

/// Evaluates PHP URL encode builtins over one eval string expression.
pub(super) fn eval_builtin_url_encode(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_url_encode_result(name, value, values)
}

/// Percent-encodes one PHP string using query-style or RFC 3986 URL rules.
fn eval_url_encode_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in bytes {
        if eval_url_encode_keeps_byte(name, byte)? {
            output.push(byte);
        } else if name == "urlencode" && byte == b' ' {
            output.push(b'+');
        } else {
            output.push(b'%');
            output.push(HEX[(byte >> 4) as usize]);
            output.push(HEX[(byte & 0x0f) as usize]);
        }
    }
    values.string_bytes_value(&output)
}

/// Returns whether a byte remains unescaped for the selected PHP URL encoder.
fn eval_url_encode_keeps_byte(name: &str, byte: u8) -> Result<bool, EvalStatus> {
    let common = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.');
    match name {
        "urlencode" => Ok(common),
        "rawurlencode" => Ok(common || byte == b'~'),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP URL decode builtins over one eval string expression.
pub(super) fn eval_builtin_url_decode(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_url_decode_result(name, value, values)
}

/// Decodes `%XX` sequences and optionally maps `+` to space for `urldecode()`.
fn eval_url_decode_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let plus_to_space = match name {
        "urldecode" => true,
        "rawurldecode" => false,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'+' && plus_to_space {
            output.push(b' ');
            index += 1;
        } else if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (
                eval_hex_nibble(bytes[index + 1]),
                eval_hex_nibble(bytes[index + 2]),
            ) {
                output.push((high << 4) | low);
                index += 3;
                continue;
            }
            output.push(bytes[index]);
            index += 1;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP `ctype_*` predicates over one eval string expression.
pub(super) fn eval_builtin_ctype(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_ctype_result(name, value, values)
}

/// Returns the PHP boolean result for one ASCII `ctype_*` byte-string check.
fn eval_ctype_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut matches = !bytes.is_empty();
    for byte in bytes {
        if !eval_ctype_byte_matches(name, byte)? {
            matches = false;
            break;
        }
    }
    values.bool_value(matches)
}

/// Checks one byte against the selected PHP ASCII character class.
fn eval_ctype_byte_matches(name: &str, byte: u8) -> Result<bool, EvalStatus> {
    match name {
        "ctype_alpha" => Ok(byte.is_ascii_alphabetic()),
        "ctype_digit" => Ok(byte.is_ascii_digit()),
        "ctype_alnum" => Ok(byte.is_ascii_alphanumeric()),
        "ctype_space" => Ok(matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP `crc32(...)` over one eval string expression.
pub(super) fn eval_builtin_crc32(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_crc32_result(value, values)
}

/// Computes PHP's non-negative CRC-32 integer over one converted byte string.
fn eval_crc32_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    values.int(i64::from(eval_crc32_bytes(&bytes)))
}

/// Evaluates one-shot PHP hash digest builtins over eval expressions.
pub(super) fn eval_builtin_hash_one_shot(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_hash_one_shot_result(name, &evaluated_args, values)
}

/// Computes the result for one-shot PHP hash digest builtins from evaluated args.
fn eval_hash_one_shot_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "md5" | "sha1" => {
            let (data, binary) = match evaluated_args {
                [data] => (*data, false),
                [data, binary] => (*data, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let data = values.string_bytes(data)?;
            eval_hash_digest_result(name.as_bytes(), &data, binary, values)
        }
        "hash" => {
            let (algo, data, binary) = match evaluated_args {
                [algo, data] => (*algo, *data, false),
                [algo, data, binary] => (*algo, *data, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let algo = values.string_bytes(algo)?;
            let data = values.string_bytes(data)?;
            eval_hash_digest_result(&algo, &data, binary, values)
        }
        "hash_file" => {
            let (algo, filename, binary) = match evaluated_args {
                [algo, filename] => (*algo, *filename, false),
                [algo, filename, binary] => (*algo, *filename, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            eval_hash_file_result(algo, filename, binary, values)
        }
        "hash_hmac" => {
            let (algo, data, key, binary) = match evaluated_args {
                [algo, data, key] => (*algo, *data, *key, false),
                [algo, data, key, binary] => (*algo, *data, *key, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let algo = values.string_bytes(algo)?;
            let data = values.string_bytes(data)?;
            let key = values.string_bytes(key)?;
            eval_hash_hmac_result(&algo, &data, &key, binary, values)
        }
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Reads a local file and returns its PHP hash digest or false when it cannot be read.
fn eval_hash_file_result(
    algo: RuntimeCellHandle,
    filename: RuntimeCellHandle,
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let algo = values.string_bytes(algo)?;
    let path = eval_path_string(filename, values)?;
    match std::fs::read(path) {
        Ok(data) => eval_hash_digest_result(&algo, &data, binary, values),
        Err(_) => values.bool_value(false),
    }
}

/// Computes a one-shot raw digest and formats it as PHP hex or raw bytes.
fn eval_hash_digest_result(
    algo: &[u8],
    data: &[u8],
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_crypto_hash(algo, data)?;
    eval_format_digest_result(&raw, binary, values)
}

/// Computes a one-shot raw HMAC digest and formats it as PHP hex or raw bytes.
fn eval_hash_hmac_result(
    algo: &[u8],
    data: &[u8],
    key: &[u8],
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_crypto_hmac(algo, data, key)?;
    eval_format_digest_result(&raw, binary, values)
}

/// Calls the elephc-crypto one-shot hash ABI and returns the raw digest bytes.
fn eval_crypto_hash(algo: &[u8], data: &[u8]) -> Result<Vec<u8>, EvalStatus> {
    let mut output = [0_u8; 64];
    let len = unsafe {
        elephc_crypto::elephc_crypto_hash(
            algo.as_ptr(),
            algo.len(),
            data.as_ptr(),
            data.len(),
            output.as_mut_ptr(),
        )
    };
    eval_crypto_digest_bytes(len, &output)
}

/// Calls the elephc-crypto one-shot HMAC ABI and returns the raw digest bytes.
fn eval_crypto_hmac(algo: &[u8], data: &[u8], key: &[u8]) -> Result<Vec<u8>, EvalStatus> {
    let mut output = [0_u8; 64];
    let len = unsafe {
        elephc_crypto::elephc_crypto_hmac(
            algo.as_ptr(),
            algo.len(),
            key.as_ptr(),
            key.len(),
            data.as_ptr(),
            data.len(),
            output.as_mut_ptr(),
        )
    };
    eval_crypto_digest_bytes(len, &output)
}

/// Converts a crypto ABI digest length into an owned digest byte vector.
fn eval_crypto_digest_bytes(len: isize, output: &[u8; 64]) -> Result<Vec<u8>, EvalStatus> {
    let len = usize::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
    if len > output.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(output[..len].to_vec())
}

/// Formats a raw digest using PHP's `$binary` flag convention.
fn eval_format_digest_result(
    raw: &[u8],
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if binary {
        return values.string_bytes_value(raw);
    }
    values.string(&eval_lower_hex_bytes(raw))
}

/// Evaluates PHP `hash_algos()` with no arguments.
pub(super) fn eval_builtin_hash_algos(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_hash_algos_result(values)
}

/// Builds the indexed array returned by eval `hash_algos()`.
fn eval_hash_algos_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_static_string_array_result(EVAL_HASH_ALGOS, values)
}

/// Builds one indexed PHP array from a static string slice.
fn eval_static_string_array_result(
    items: &[&str],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(items.len())?;
    for (index, item) in items.iter().enumerate() {
        let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(index)?;
        let value = values.string(item)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `spl_classes()` with no arguments.
pub(super) fn eval_builtin_spl_classes(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_spl_classes_result(values)
}

/// Builds the static class-name list returned by eval `spl_classes()`.
fn eval_spl_classes_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_static_string_array_result(EVAL_SPL_CLASS_NAMES, values)
}

/// Evaluates PHP stream introspection list builtins with no arguments.
pub(super) fn eval_builtin_stream_introspection(
    name: &str,
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_stream_introspection_result(name, values)
}

/// Builds the static list returned by one eval stream introspection builtin.
fn eval_stream_introspection_result(
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let items = match name {
        "stream_get_filters" => EVAL_STREAM_FILTERS,
        "stream_get_transports" => EVAL_STREAM_TRANSPORTS,
        "stream_get_wrappers" => EVAL_STREAM_WRAPPERS,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_static_string_array_result(items, values)
}

/// Evaluates PHP `time()` with no arguments.
pub(super) fn eval_builtin_time(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_time_result(values)
}

/// Returns the current Unix timestamp as a boxed PHP integer.
fn eval_time_result(values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(eval_current_unix_timestamp()?)
}

/// Returns the current Unix timestamp as an integer payload.
fn eval_current_unix_timestamp() -> Result<i64, EvalStatus> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .as_secs();
    i64::try_from(timestamp).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates PHP `date($format, $timestamp = time())` for the eval subset.
pub(super) fn eval_builtin_date(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [format] => {
            let format = eval_expr(format, context, scope, values)?;
            eval_date_result(format, None, values)
        }
        [format, timestamp] => {
            let format = eval_expr(format, context, scope, values)?;
            let timestamp = eval_expr(timestamp, context, scope, values)?;
            eval_date_result(format, Some(timestamp), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Formats one Unix timestamp through PHP `date()` token rules supported by elephc.
fn eval_date_result(
    format: RuntimeCellHandle,
    timestamp: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let format = values.string_bytes(format)?;
    let timestamp = match timestamp {
        Some(timestamp) => eval_int_value(timestamp, values)?,
        None => eval_current_unix_timestamp()?,
    };
    let tm = eval_localtime(timestamp)?;
    let output = eval_format_date_bytes(&format, &tm, timestamp)?;
    values.string_bytes_value(&output)
}

/// Converts one Unix timestamp to local broken-down time through libc.
fn eval_localtime(timestamp: i64) -> Result<libc::tm, EvalStatus> {
    let raw: libc::time_t = timestamp.try_into().map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut tm = MaybeUninit::<libc::tm>::uninit();
    let result = unsafe { libc::localtime_r(&raw, tm.as_mut_ptr()) };
    if result.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(unsafe { tm.assume_init() })
}

/// Applies PHP `date()` tokens to one local broken-down timestamp.
fn eval_format_date_bytes(
    format: &[u8],
    tm: &libc::tm,
    timestamp: i64,
) -> Result<Vec<u8>, EvalStatus> {
    let mut output = Vec::new();
    let mut escaped = false;
    for byte in format {
        if escaped {
            output.push(*byte);
            escaped = false;
            continue;
        }
        if *byte == b'\\' {
            escaped = true;
            continue;
        }
        eval_push_date_token(&mut output, *byte, tm, timestamp)?;
    }
    if escaped {
        output.push(b'\\');
    }
    Ok(output)
}

/// Appends the expansion for one PHP `date()` token, or the token literal.
fn eval_push_date_token(
    output: &mut Vec<u8>,
    token: u8,
    tm: &libc::tm,
    timestamp: i64,
) -> Result<(), EvalStatus> {
    match token {
        b'Y' => eval_push_padded_number(output, i64::from(tm.tm_year) + 1900, 4),
        b'm' => eval_push_padded_number(output, i64::from(tm.tm_mon) + 1, 2),
        b'd' => eval_push_padded_number(output, i64::from(tm.tm_mday), 2),
        b'H' => eval_push_padded_number(output, i64::from(tm.tm_hour), 2),
        b'i' => eval_push_padded_number(output, i64::from(tm.tm_min), 2),
        b's' => eval_push_padded_number(output, i64::from(tm.tm_sec), 2),
        b'l' => output.extend_from_slice(EVAL_WEEKDAY_NAMES[eval_tm_weekday_index(tm)?].as_bytes()),
        b'F' => output.extend_from_slice(EVAL_MONTH_NAMES[eval_tm_month_index(tm)?].as_bytes()),
        b'D' => output
            .extend_from_slice(EVAL_WEEKDAY_SHORT_NAMES[eval_tm_weekday_index(tm)?].as_bytes()),
        b'M' => {
            output.extend_from_slice(EVAL_MONTH_SHORT_NAMES[eval_tm_month_index(tm)?].as_bytes())
        }
        b'N' => {
            let weekday = tm.tm_wday;
            let iso_weekday = if weekday == 0 { 7 } else { weekday };
            output.extend_from_slice(iso_weekday.to_string().as_bytes());
        }
        b'j' => output.extend_from_slice(tm.tm_mday.to_string().as_bytes()),
        b'n' => output.extend_from_slice((tm.tm_mon + 1).to_string().as_bytes()),
        b'G' => output.extend_from_slice(tm.tm_hour.to_string().as_bytes()),
        b'g' => {
            let hour = tm.tm_hour % 12;
            let hour = if hour == 0 { 12 } else { hour };
            output.extend_from_slice(hour.to_string().as_bytes());
        }
        b'A' => output.extend_from_slice(if tm.tm_hour < 12 { b"AM" } else { b"PM" }),
        b'a' => output.extend_from_slice(if tm.tm_hour < 12 { b"am" } else { b"pm" }),
        b'U' => output.extend_from_slice(timestamp.to_string().as_bytes()),
        _ => output.push(token),
    }
    Ok(())
}

/// Returns a checked month index for PHP `date()` name tables.
fn eval_tm_month_index(tm: &libc::tm) -> Result<usize, EvalStatus> {
    let index = usize::try_from(tm.tm_mon).map_err(|_| EvalStatus::RuntimeFatal)?;
    if index >= EVAL_MONTH_NAMES.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(index)
}

/// Returns a checked weekday index for PHP `date()` name tables.
fn eval_tm_weekday_index(tm: &libc::tm) -> Result<usize, EvalStatus> {
    let index = usize::try_from(tm.tm_wday).map_err(|_| EvalStatus::RuntimeFatal)?;
    if index >= EVAL_WEEKDAY_NAMES.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(index)
}

/// Appends one zero-padded decimal value with the requested minimum width.
fn eval_push_padded_number(output: &mut Vec<u8>, value: i64, width: usize) {
    output.extend_from_slice(format!("{value:0width$}").as_bytes());
}

/// Evaluates PHP `mktime(hour, minute, second, month, day, year)`.
pub(super) fn eval_builtin_mktime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [hour, minute, second, month, day, year] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let hour = eval_expr(hour, context, scope, values)?;
    let minute = eval_expr(minute, context, scope, values)?;
    let second = eval_expr(second, context, scope, values)?;
    let month = eval_expr(month, context, scope, values)?;
    let day = eval_expr(day, context, scope, values)?;
    let year = eval_expr(year, context, scope, values)?;
    eval_mktime_result(hour, minute, second, month, day, year, values)
}

/// Converts PHP date components to a local Unix timestamp through libc `mktime`.
fn eval_mktime_result(
    hour: RuntimeCellHandle,
    minute: RuntimeCellHandle,
    second: RuntimeCellHandle,
    month: RuntimeCellHandle,
    day: RuntimeCellHandle,
    year: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timestamp = eval_mktime_timestamp(
        eval_int_cell_as_c_int(hour, values)?,
        eval_int_cell_as_c_int(minute, values)?,
        eval_int_cell_as_c_int(second, values)?,
        eval_int_cell_as_c_int(month, values)?,
        eval_int_cell_as_c_int(day, values)?,
        eval_int_cell_as_c_int(year, values)?,
    )?;
    values.int(timestamp)
}

/// Converts local date components into a Unix timestamp through libc `mktime`.
fn eval_mktime_timestamp(
    hour: libc::c_int,
    minute: libc::c_int,
    second: libc::c_int,
    month: libc::c_int,
    day: libc::c_int,
    year: libc::c_int,
) -> Result<i64, EvalStatus> {
    let mut tm = unsafe { MaybeUninit::<libc::tm>::zeroed().assume_init() };
    tm.tm_hour = hour;
    tm.tm_min = minute;
    tm.tm_sec = second;
    tm.tm_mon = month - 1;
    tm.tm_mday = day;
    tm.tm_year = year - 1900;
    tm.tm_isdst = -1;
    let timestamp = unsafe { libc::mktime(&mut tm) };
    i64::try_from(timestamp).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Casts one eval cell to a PHP int and checks it fits a libc `c_int`.
fn eval_int_cell_as_c_int(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<libc::c_int, EvalStatus> {
    let value = eval_int_value(value, values)?;
    libc::c_int::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates PHP `strtotime(datetime)` for eval's supported date-string subset.
pub(super) fn eval_builtin_strtotime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [datetime] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let datetime = eval_expr(datetime, context, scope, values)?;
    eval_strtotime_result(datetime, values)
}

/// Parses one eval `strtotime()` input and boxes the resulting timestamp.
fn eval_strtotime_result(
    datetime: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(datetime)?;
    let timestamp = eval_strtotime_bytes(&bytes)?;
    values.int(timestamp)
}

/// Parses eval's supported `strtotime()` strings into local Unix timestamps.
fn eval_strtotime_bytes(bytes: &[u8]) -> Result<i64, EvalStatus> {
    let bytes = eval_trim_ascii_whitespace(bytes);
    if bytes.eq_ignore_ascii_case(b"now") {
        return eval_current_unix_timestamp();
    }
    let Some((year, month, day, hour, minute, second)) = eval_parse_iso_datetime(bytes) else {
        return Ok(-1);
    };
    eval_mktime_timestamp(hour, minute, second, month, day, year)
}

/// Trims ASCII whitespace from both ends of one byte slice.
fn eval_trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[start..end]
}

/// Parses fixed-width ISO date and datetime forms supported by eval `strtotime()`.
fn eval_parse_iso_datetime(
    bytes: &[u8],
) -> Option<(
    libc::c_int,
    libc::c_int,
    libc::c_int,
    libc::c_int,
    libc::c_int,
    libc::c_int,
)> {
    if bytes.len() != 10 && bytes.len() != 16 && bytes.len() != 19 {
        return None;
    }
    if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return None;
    }
    let year = eval_parse_fixed_digits(bytes, 0, 4)?;
    let month = eval_parse_fixed_digits(bytes, 5, 2)?;
    let day = eval_parse_fixed_digits(bytes, 8, 2)?;
    let (hour, minute, second) = if bytes.len() == 10 {
        (0, 0, 0)
    } else {
        if !matches!(bytes.get(10), Some(b' ') | Some(b'T') | Some(b't')) {
            return None;
        }
        if bytes.get(13) != Some(&b':') {
            return None;
        }
        let hour = eval_parse_fixed_digits(bytes, 11, 2)?;
        let minute = eval_parse_fixed_digits(bytes, 14, 2)?;
        let second = if bytes.len() == 19 {
            if bytes.get(16) != Some(&b':') {
                return None;
            }
            eval_parse_fixed_digits(bytes, 17, 2)?
        } else {
            0
        };
        (hour, minute, second)
    };
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return None;
    }
    Some((year, month, day, hour, minute, second))
}

/// Parses a fixed-width decimal field as a libc-compatible integer.
fn eval_parse_fixed_digits(bytes: &[u8], start: usize, len: usize) -> Option<libc::c_int> {
    let end = start.checked_add(len)?;
    let field = bytes.get(start..end)?;
    let mut value: libc::c_int = 0;
    for byte in field {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?;
        value = value.checked_add(libc::c_int::from(byte - b'0'))?;
    }
    Some(value)
}

/// Evaluates PHP `microtime()` with an optional ignored argument.
pub(super) fn eval_builtin_microtime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_microtime_result(values),
        [as_float] => {
            let _ = eval_expr(as_float, context, scope, values)?;
            eval_microtime_result(values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the current Unix timestamp with microsecond precision as a boxed float.
fn eval_microtime_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| EvalStatus::RuntimeFatal)?;
    let seconds = timestamp.as_secs() as f64;
    let micros = f64::from(timestamp.subsec_micros()) / 1_000_000.0;
    values.float(seconds + micros)
}

/// Evaluates PHP `sleep($seconds)` over one eval expression.
pub(super) fn eval_builtin_sleep(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [seconds] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let seconds = eval_expr(seconds, context, scope, values)?;
    eval_sleep_result(seconds, values)
}

/// Sleeps for a non-negative number of seconds and returns PHP's remaining-seconds value.
fn eval_sleep_result(
    seconds: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let seconds = eval_int_value(seconds, values)?;
    let seconds = u64::try_from(seconds).map_err(|_| EvalStatus::RuntimeFatal)?;
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    values.int(0)
}

/// Evaluates PHP `usleep($microseconds)` over one eval expression.
pub(super) fn eval_builtin_usleep(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [microseconds] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let microseconds = eval_expr(microseconds, context, scope, values)?;
    eval_usleep_result(microseconds, values)
}

/// Sleeps for a non-negative number of microseconds and returns PHP null.
fn eval_usleep_result(
    microseconds: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let microseconds = eval_int_value(microseconds, values)?;
    let microseconds = u64::try_from(microseconds).map_err(|_| EvalStatus::RuntimeFatal)?;
    std::thread::sleep(std::time::Duration::from_micros(microseconds));
    values.null()
}

/// Evaluates PHP `phpversion()` with no arguments.
pub(super) fn eval_builtin_phpversion(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_phpversion_result(values)
}

/// Returns the root elephc package version as a boxed PHP string.
fn eval_phpversion_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string(eval_compiler_php_version())
}

/// Reads the root package version from the workspace manifest used by native `phpversion()`.
pub(super) fn eval_compiler_php_version() -> &'static str {
    let mut in_package = false;
    for line in EVAL_ROOT_CARGO_TOML.lines() {
        let line = line.trim();
        if line == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && line.starts_with('[') {
            break;
        }
        if in_package {
            if let Some(value) = line.strip_prefix("version = ") {
                return value.trim_matches('"');
            }
        }
    }
    env!("CARGO_PKG_VERSION")
}

/// Evaluates PHP `php_uname($mode = "a")` over zero or one eval expression.
pub(super) fn eval_builtin_php_uname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_php_uname_result(None, values),
        [mode] => {
            let mode = eval_expr(mode, context, scope, values)?;
            eval_php_uname_result(Some(mode), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Reads the local uname fields and formats the PHP `php_uname()` mode result.
fn eval_php_uname_result(
    mode: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mode = match mode {
        Some(mode) => {
            let bytes = values.string_bytes(mode)?;
            let [mode] = bytes.as_slice() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            *mode
        }
        None => b'a',
    };

    let mut utsname = std::mem::MaybeUninit::<libc::utsname>::zeroed();
    let status = unsafe {
        // libc writes all uname fields into the stack-owned utsname buffer.
        libc::uname(utsname.as_mut_ptr())
    };
    if status != 0 {
        return values.string("");
    }
    let utsname = unsafe {
        // `uname` succeeded, so libc initialized the full `utsname` structure.
        utsname.assume_init()
    };
    let sysname = eval_uname_field_bytes(&utsname.sysname);
    let nodename = eval_uname_field_bytes(&utsname.nodename);
    let release = eval_uname_field_bytes(&utsname.release);
    let version = eval_uname_field_bytes(&utsname.version);
    let machine = eval_uname_field_bytes(&utsname.machine);

    match mode {
        b'a' => {
            let mut output = Vec::new();
            for field in [&sysname, &nodename, &release, &version, &machine] {
                if !output.is_empty() {
                    output.push(b' ');
                }
                output.extend_from_slice(field);
            }
            values.string_bytes_value(&output)
        }
        b's' => values.string_bytes_value(&sysname),
        b'n' => values.string_bytes_value(&nodename),
        b'r' => values.string_bytes_value(&release),
        b'v' => values.string_bytes_value(&version),
        b'm' => values.string_bytes_value(&machine),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Copies one NUL-terminated `utsname` field into raw PHP string bytes.
fn eval_uname_field_bytes(field: &[libc::c_char]) -> Vec<u8> {
    let length = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    field[..length].iter().map(|byte| *byte as u8).collect()
}

/// Evaluates PHP `getcwd()` with no arguments.
pub(super) fn eval_builtin_getcwd(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_getcwd_result(values)
}

/// Returns the process current working directory as a boxed PHP string.
fn eval_getcwd_result(values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let cwd = std::env::current_dir().map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(cwd.to_string_lossy().as_ref())
}

/// Evaluates one PHP filesystem predicate over an eval expression.
pub(super) fn eval_builtin_file_probe(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_probe_result(name, filename, values)
}

/// Computes one local filesystem predicate and returns a PHP boolean.
fn eval_file_probe_result(
    name: &str,
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let path = std::path::Path::new(&path);
    let result = match name {
        "file_exists" => path.exists(),
        "is_dir" => path.is_dir(),
        "is_executable" => eval_path_is_executable(path),
        "is_file" => path.is_file(),
        "is_link" => std::fs::symlink_metadata(path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false),
        "is_readable" => eval_path_is_readable(path),
        "is_writable" | "is_writeable" => eval_path_is_writable(path),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(result)
}

/// Evaluates one scalar PHP stat metadata builtin over an eval expression.
pub(super) fn eval_builtin_file_stat_scalar(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_stat_scalar_result(name, filename, values)
}

/// Returns scalar stat metadata, using PHP false for failure where native elephc does.
fn eval_file_stat_scalar_result(
    name: &str,
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) if name == "filemtime" => return values.int(0),
        Err(_) => return values.bool_value(false),
    };
    match name {
        "fileatime" => values.int(metadata.atime()),
        "filectime" => values.int(metadata.ctime()),
        "filegroup" => values.int(i64::from(metadata.gid())),
        "fileinode" => {
            values.int(i64::try_from(metadata.ino()).map_err(|_| EvalStatus::RuntimeFatal)?)
        }
        "filemtime" => values.int(metadata.mtime()),
        "fileowner" => values.int(i64::from(metadata.uid())),
        "fileperms" => values.int(i64::from(metadata.mode())),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `file_get_contents($filename)` over one eval expression.
pub(super) fn eval_builtin_file_get_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_get_contents_result(filename, values)
}

/// Reads a local file into a PHP string, or returns false when it cannot be opened.
fn eval_file_get_contents_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    match std::fs::read(path) {
        Ok(bytes) => values.string_bytes_value(&bytes),
        Err(_) => {
            values.warning("Warning: file_get_contents(): Failed to open stream\n")?;
            values.bool_value(false)
        }
    }
}

/// Evaluates PHP `file($filename)` over one eval expression.
pub(super) fn eval_builtin_file(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_result(filename, values)
}

/// Reads one local file and returns an indexed array of line byte strings.
fn eval_file_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            values.warning("Warning: file_get_contents(): Failed to open stream\n")?;
            return values.array_new(0);
        }
    };
    eval_file_lines_array(&bytes, values)
}

/// Splits file payload bytes into runtime array entries, preserving trailing newlines.
fn eval_file_lines_array(
    bytes: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(0)?;
    let mut line_start = 0;
    let mut line_index = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        result =
            eval_array_set_indexed_bytes(result, line_index, &bytes[line_start..=index], values)?;
        line_start = index + 1;
        line_index += 1;
    }
    if line_start < bytes.len() {
        result = eval_array_set_indexed_bytes(result, line_index, &bytes[line_start..], values)?;
    }
    Ok(result)
}

/// Evaluates PHP `readfile($filename)` over one eval expression.
pub(super) fn eval_builtin_readfile(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_readfile_result(filename, values)
}

/// Streams one local file to eval output and returns a byte count, false, or -1.
fn eval_readfile_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let path = std::path::Path::new(&path);
    if path.is_dir() {
        return values.int(-1);
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return values.bool_value(false),
    };
    let output = values.string_bytes_value(&bytes)?;
    values.echo(output)?;
    values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Evaluates PHP `file_put_contents($filename, $data)` over one eval expression.
pub(super) fn eval_builtin_file_put_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename, data] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    let data = eval_expr(data, context, scope, values)?;
    eval_file_put_contents_result(filename, data, values)
}

/// Writes a PHP string to a local file and returns the written byte count or false.
fn eval_file_put_contents_result(
    filename: RuntimeCellHandle,
    data: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let data = values.string_bytes(data)?;
    match std::fs::write(path, &data) {
        Ok(()) => values.int(i64::try_from(data.len()).map_err(|_| EvalStatus::RuntimeFatal)?),
        Err(_) => values.bool_value(false),
    }
}

/// Evaluates PHP `filesize($filename)` over one eval expression.
pub(super) fn eval_builtin_filesize(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_filesize_result(filename, values)
}

/// Returns one local file size in bytes, or zero when stat fails.
fn eval_filesize_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let len = std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    values.int(i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Evaluates PHP `filetype($filename)` over one eval expression.
pub(super) fn eval_builtin_filetype(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_filetype_result(filename, values)
}

/// Returns the PHP filetype string for one path, or false when lstat fails.
fn eval_filetype_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let file_type = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata.file_type(),
        Err(_) => return values.bool_value(false),
    };
    let label = if file_type.is_file() {
        "file"
    } else if file_type.is_dir() {
        "dir"
    } else if file_type.is_symlink() {
        "link"
    } else if file_type.is_char_device() {
        "char"
    } else if file_type.is_block_device() {
        "block"
    } else if file_type.is_fifo() {
        "fifo"
    } else if file_type.is_socket() {
        "socket"
    } else {
        "unknown"
    };
    values.string(label)
}

/// Evaluates PHP `stat($filename)` or `lstat($filename)` over one eval expression.
pub(super) fn eval_builtin_stat_array(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_stat_array_result(name, filename, values)
}

/// Builds PHP's stat array for one local path, or returns false on stat failure.
fn eval_stat_array_result(
    name: &str,
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let metadata = match name {
        "stat" => std::fs::metadata(path),
        "lstat" => std::fs::symlink_metadata(path),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let metadata = match metadata {
        Ok(metadata) => metadata,
        Err(_) => return values.bool_value(false),
    };
    eval_stat_metadata_array(&metadata, values)
}

/// Converts filesystem metadata into PHP's numeric-and-string keyed stat array.
fn eval_stat_metadata_array(
    metadata: &std::fs::Metadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let fields = [
        ("dev", eval_u64_to_i64(metadata.dev())?),
        ("ino", eval_u64_to_i64(metadata.ino())?),
        ("mode", i64::from(metadata.mode())),
        ("nlink", eval_u64_to_i64(metadata.nlink())?),
        ("uid", i64::from(metadata.uid())),
        ("gid", i64::from(metadata.gid())),
        ("rdev", eval_u64_to_i64(metadata.rdev())?),
        ("size", eval_u64_to_i64(metadata.size())?),
        ("atime", metadata.atime()),
        ("mtime", metadata.mtime()),
        ("ctime", metadata.ctime()),
        ("blksize", eval_u64_to_i64(metadata.blksize())?),
        ("blocks", eval_u64_to_i64(metadata.blocks())?),
    ];
    let mut result = values.assoc_new(fields.len() * 2)?;
    for (index, (name, value)) in fields.iter().enumerate() {
        result = eval_stat_array_set_int_key(result, index, *value, values)?;
        result = eval_stat_array_set_string_key(result, name, *value, values)?;
    }
    Ok(result)
}

/// Inserts one integer stat field under a numeric PHP array key.
fn eval_stat_array_set_int_key(
    array: RuntimeCellHandle,
    key: usize,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.int(i64::try_from(key).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let value = values.int(value)?;
    values.array_set(array, key, value)
}

/// Inserts one integer stat field under a string PHP array key.
fn eval_stat_array_set_string_key(
    array: RuntimeCellHandle,
    key: &str,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.int(value)?;
    values.array_set(array, key, value)
}

/// Converts unsigned stat metadata into the signed integer payload used by PHP cells.
fn eval_u64_to_i64(value: u64) -> Result<i64, EvalStatus> {
    i64::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates PHP `disk_free_space($directory)` or `disk_total_space($directory)`.
pub(super) fn eval_builtin_disk_space(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    eval_disk_space_result(name, directory, values)
}

/// Reports available or total filesystem bytes as a PHP float, or 0.0 on failure.
fn eval_disk_space_result(
    name: &str,
    directory: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(directory)?;
    let Ok(path) = CString::new(bytes) else {
        return values.float(0.0);
    };
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    let status = unsafe {
        // libc writes the statvfs fields for this NUL-terminated local path.
        libc::statvfs(path.as_ptr(), stats.as_mut_ptr())
    };
    if status != 0 {
        return values.float(0.0);
    }
    let stats = unsafe {
        // `statvfs` succeeded, so libc initialized the full stat buffer.
        stats.assume_init()
    };
    let block_size = if stats.f_frsize > 0 {
        stats.f_frsize
    } else {
        stats.f_bsize
    };
    let blocks = match name {
        "disk_free_space" => stats.f_bavail,
        "disk_total_space" => stats.f_blocks,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.float((block_size as f64) * (blocks as f64))
}

/// Evaluates a one-path filesystem operation that returns a PHP boolean.
pub(super) fn eval_builtin_unary_path_bool(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_unary_path_bool_result(name, path, values)
}

/// Executes a one-path local filesystem operation and returns whether it succeeded.
fn eval_unary_path_bool_result(
    name: &str,
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    let ok = match name {
        "chdir" => std::env::set_current_dir(path).is_ok(),
        "mkdir" => std::fs::create_dir(path).is_ok(),
        "rmdir" => std::fs::remove_dir(path).is_ok(),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(ok)
}

/// Evaluates a two-path filesystem operation that returns a PHP boolean.
pub(super) fn eval_builtin_binary_path_bool(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [from, to] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let from = eval_expr(from, context, scope, values)?;
    let to = eval_expr(to, context, scope, values)?;
    eval_binary_path_bool_result(name, from, to, values)
}

/// Executes a two-path local filesystem operation and returns whether it succeeded.
fn eval_binary_path_bool_result(
    name: &str,
    from: RuntimeCellHandle,
    to: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let from = eval_path_string(from, values)?;
    let to = eval_path_string(to, values)?;
    let ok = match name {
        "copy" => std::fs::copy(from, to).is_ok(),
        "link" => std::fs::hard_link(from, to).is_ok(),
        "rename" => std::fs::rename(from, to).is_ok(),
        "symlink" => std::os::unix::fs::symlink(from, to).is_ok(),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(ok)
}

/// Evaluates PHP `chmod($filename, $permissions)` over eval expressions.
pub(super) fn eval_builtin_chmod(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename, permissions] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    let permissions = eval_expr(permissions, context, scope, values)?;
    eval_chmod_result(filename, permissions, values)
}

/// Changes one local file's mode and returns whether the operation succeeded.
fn eval_chmod_result(
    filename: RuntimeCellHandle,
    permissions: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let mode = eval_int_value(permissions, values)? as u32;
    let permissions = std::fs::Permissions::from_mode(mode);
    values.bool_value(std::fs::set_permissions(path, permissions).is_ok())
}

/// Evaluates PHP `scandir($directory)` over one eval expression.
pub(super) fn eval_builtin_scandir(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    eval_scandir_result(directory, values)
}

/// Lists one local directory into an indexed string array, or an empty array on failure.
fn eval_scandir_result(
    directory: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(directory, values)?;
    let Ok(entries) = std::fs::read_dir(path) else {
        return values.array_new(0);
    };
    let mut names = vec![".".to_string(), "..".to_string()];
    for entry in entries {
        let entry = entry.map_err(|_| EvalStatus::RuntimeFatal)?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    let mut result = values.array_new(names.len())?;
    for (index, name) in names.iter().enumerate() {
        result = eval_array_set_indexed_bytes(result, index, name.as_bytes(), values)?;
    }
    Ok(result)
}

/// Evaluates PHP `glob($pattern)` over one eval expression.
pub(super) fn eval_builtin_glob(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pattern = eval_expr(pattern, context, scope, values)?;
    eval_glob_result(pattern, values)
}

/// Expands one local glob pattern into a sorted indexed PHP string array.
fn eval_glob_result(
    pattern: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let pattern = eval_path_string(pattern, values)?;
    let matches = eval_glob_matches(&pattern);
    let mut result = values.array_new(matches.len())?;
    for (index, path) in matches.iter().enumerate() {
        result = eval_array_set_indexed_bytes(result, index, path.as_bytes(), values)?;
    }
    Ok(result)
}

/// Collects sorted matches for one local glob pattern.
fn eval_glob_matches(pattern: &str) -> Vec<String> {
    if pattern.is_empty() {
        return Vec::new();
    }
    if !eval_glob_component_has_magic(pattern) {
        return std::path::Path::new(pattern)
            .exists()
            .then(|| pattern.to_string())
            .into_iter()
            .collect();
    }
    let absolute = pattern.starts_with('/');
    let components: Vec<&str> = pattern
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    let mut matches = Vec::new();
    let base = if absolute {
        std::path::PathBuf::from("/")
    } else {
        std::path::PathBuf::from(".")
    };
    let prefix = if absolute { "/" } else { "" };
    eval_glob_collect(&base, prefix, &components, &mut matches);
    matches.sort();
    matches
}

/// Recursively expands one glob path component at a time.
fn eval_glob_collect(
    base: &std::path::Path,
    prefix: &str,
    components: &[&str],
    matches: &mut Vec<String>,
) {
    let Some((component, rest)) = components.split_first() else {
        if base.exists() && !prefix.is_empty() {
            matches.push(prefix.to_string());
        }
        return;
    };
    if !eval_glob_component_has_magic(component) {
        let next_base = base.join(component);
        if rest.is_empty() {
            if next_base.exists() {
                matches.push(eval_glob_join_output(prefix, component));
            }
        } else if next_base.is_dir() {
            let next_prefix = eval_glob_join_output(prefix, component);
            eval_glob_collect(&next_base, &next_prefix, rest, matches);
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    for name in names {
        if !eval_fnmatch_bytes(component.as_bytes(), name.as_bytes(), EVAL_FNM_PERIOD) {
            continue;
        }
        let next_base = base.join(&name);
        if rest.is_empty() {
            matches.push(eval_glob_join_output(prefix, &name));
        } else if next_base.is_dir() {
            let next_prefix = eval_glob_join_output(prefix, &name);
            eval_glob_collect(&next_base, &next_prefix, rest, matches);
        }
    }
}

/// Joins a display path prefix and component while preserving absolute-root output.
fn eval_glob_join_output(prefix: &str, component: &str) -> String {
    if prefix.is_empty() {
        component.to_string()
    } else if prefix == "/" {
        format!("/{component}")
    } else {
        format!("{prefix}/{component}")
    }
}

/// Returns whether a glob component contains wildcard syntax.
fn eval_glob_component_has_magic(component: &str) -> bool {
    component
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'*' | b'?' | b'['))
}

/// Writes one byte-string value into an indexed runtime array at a zero-based position.
fn eval_array_set_indexed_bytes(
    array: RuntimeCellHandle,
    index: usize,
    value: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let value = values.string_bytes_value(value)?;
    values.array_set(array, key, value)
}

/// Evaluates PHP `tempnam($directory, $prefix)` over eval expressions.
pub(super) fn eval_builtin_tempnam(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory, prefix] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    let prefix = eval_expr(prefix, context, scope, values)?;
    eval_tempnam_result(directory, prefix, values)
}

/// Creates a unique local temporary file and returns its path, or an empty string on failure.
fn eval_tempnam_result(
    directory: RuntimeCellHandle,
    prefix: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let directory = eval_path_string(directory, values)?;
    let prefix = values.string_bytes(prefix)?;
    let prefix = String::from_utf8_lossy(&prefix);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    for attempt in 0..1000_u32 {
        let candidate =
            std::path::Path::new(&directory).join(eval_tempnam_filename(&prefix, nonce, attempt));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return values.string(candidate.to_string_lossy().as_ref()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return values.string(""),
        }
    }
    values.string("")
}

/// Builds one deterministic tempnam candidate basename from prefix, process, and attempt data.
fn eval_tempnam_filename(prefix: &str, nonce: u128, attempt: u32) -> String {
    format!("{}{}_{:x}_{attempt}", prefix, std::process::id(), nonce)
}

/// Evaluates PHP `touch($filename, $mtime = null, $atime = null)` over eval expressions.
pub(super) fn eval_builtin_touch(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [filename] => {
            let filename = eval_expr(filename, context, scope, values)?;
            eval_touch_result(filename, None, None, values)
        }
        [filename, mtime] => {
            let filename = eval_expr(filename, context, scope, values)?;
            let mtime = eval_expr(mtime, context, scope, values)?;
            eval_touch_result(filename, Some(mtime), None, values)
        }
        [filename, mtime, atime] => {
            let filename = eval_expr(filename, context, scope, values)?;
            let mtime = eval_expr(mtime, context, scope, values)?;
            let atime = eval_expr(atime, context, scope, values)?;
            eval_touch_result(filename, Some(mtime), Some(atime), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Creates or stamps one local file and returns whether the operation succeeded.
fn eval_touch_result(
    filename: RuntimeCellHandle,
    mtime: Option<RuntimeCellHandle>,
    atime: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let (mtime, atime) = eval_touch_times(mtime, atime, values)?;
    let file = match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
    {
        Ok(file) => file,
        Err(_) => return values.bool_value(false),
    };
    let times = std::fs::FileTimes::new()
        .set_modified(mtime)
        .set_accessed(atime);
    values.bool_value(file.set_times(times).is_ok())
}

/// Resolves PHP touch timestamp defaults into concrete system times.
fn eval_touch_times(
    mtime: Option<RuntimeCellHandle>,
    atime: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(std::time::SystemTime, std::time::SystemTime), EvalStatus> {
    let now = std::time::SystemTime::now();
    let Some(mtime) = mtime else {
        return Ok((now, now));
    };
    if values.is_null(mtime)? {
        if let Some(atime) = atime {
            if !values.is_null(atime)? {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        return Ok((now, now));
    }
    let mtime = eval_system_time_from_unix(eval_int_value(mtime, values)?)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let Some(atime) = atime else {
        return Ok((mtime, mtime));
    };
    if values.is_null(atime)? {
        return Ok((mtime, mtime));
    }
    let atime = eval_system_time_from_unix(eval_int_value(atime, values)?)
        .ok_or(EvalStatus::RuntimeFatal)?;
    Ok((mtime, atime))
}

/// Converts a Unix timestamp in seconds into a `SystemTime`.
fn eval_system_time_from_unix(seconds: i64) -> Option<std::time::SystemTime> {
    if seconds >= 0 {
        std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(seconds as u64))
    } else {
        std::time::UNIX_EPOCH.checked_sub(std::time::Duration::from_secs(seconds.unsigned_abs()))
    }
}

/// Evaluates PHP `umask($mask = null)` over an optional eval expression.
pub(super) fn eval_builtin_umask(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_umask_result(None, values),
        [mask] => {
            let mask = eval_expr(mask, context, scope, values)?;
            eval_umask_result(Some(mask), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Applies PHP `umask()` semantics and returns the previous mask.
fn eval_umask_result(
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let previous = match mask {
        Some(mask) => {
            let mask = eval_int_value(mask, values)? as u32;
            unsafe { umask(mask) }
        }
        None => unsafe {
            let current = umask(0);
            umask(current);
            current
        },
    };
    values.int(i64::from(previous))
}

/// Evaluates PHP `readlink($path)` over one eval expression.
pub(super) fn eval_builtin_readlink(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_readlink_result(path, values)
}

/// Reads one symbolic-link target string, or returns PHP false on failure.
fn eval_readlink_result(
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    match std::fs::read_link(path) {
        Ok(target) => values.string(target.to_string_lossy().as_ref()),
        Err(_) => values.bool_value(false),
    }
}

/// Evaluates PHP `linkinfo($path)` over one eval expression.
pub(super) fn eval_builtin_linkinfo(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_linkinfo_result(path, values)
}

/// Returns one symlink metadata device id, or PHP's `-1` failure sentinel.
fn eval_linkinfo_result(
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    let dev = match std::fs::symlink_metadata(path) {
        Ok(metadata) => i64::try_from(metadata.dev()).map_err(|_| EvalStatus::RuntimeFatal)?,
        Err(_) => -1,
    };
    values.int(dev)
}

/// Evaluates `clearstatcache(...)` as an ordered no-op in eval.
pub(super) fn eval_builtin_clearstatcache(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    for arg in args {
        eval_expr(arg, context, scope, values)?;
    }
    values.null()
}

/// Evaluates PHP `unlink($filename)` over one eval expression.
pub(super) fn eval_builtin_unlink(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_unlink_result(filename, values)
}

/// Deletes one local file and returns whether it succeeded.
fn eval_unlink_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    values.bool_value(std::fs::remove_file(path).is_ok())
}

/// Converts one eval value to a filesystem path string.
pub(super) fn eval_path_string(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let filename = values.string_bytes(filename)?;
    Ok(String::from_utf8_lossy(&filename).into_owned())
}

/// Returns whether a path can be opened for reading by the current process.
fn eval_path_is_readable(path: &std::path::Path) -> bool {
    std::fs::File::open(path).is_ok() || std::fs::read_dir(path).is_ok()
}

/// Returns whether a path has any executable bit set in its Unix mode.
fn eval_path_is_executable(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Returns whether a path can be written by the current process.
fn eval_path_is_writable(path: &std::path::Path) -> bool {
    if path.is_file() {
        return std::fs::OpenOptions::new().write(true).open(path).is_ok();
    }
    if !path.is_dir() {
        return false;
    }
    let probe = path.join(format!(
        ".elephc_eval_writable_probe_{}",
        std::process::id()
    ));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

/// Evaluates PHP `basename($path, $suffix = "")` over one eval expression.
pub(super) fn eval_builtin_basename(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [path] => {
            let path = eval_expr(path, context, scope, values)?;
            eval_basename_result(path, None, values)
        }
        [path, suffix] => {
            let path = eval_expr(path, context, scope, values)?;
            let suffix = eval_expr(suffix, context, scope, values)?;
            eval_basename_result(path, Some(suffix), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `basename()` bytes and returns them as a runtime string.
fn eval_basename_result(
    path: RuntimeCellHandle,
    suffix: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let suffix = suffix
        .map(|suffix| values.string_bytes(suffix))
        .transpose()?;
    let result = eval_basename_bytes(&path, suffix.as_deref());
    values.string_bytes_value(&result)
}

/// Extracts a PHP basename from one path byte string.
fn eval_basename_bytes(path: &[u8], suffix: Option<&[u8]>) -> Vec<u8> {
    let mut end = path.len();
    while end > 0 && path[end - 1] == b'/' {
        end -= 1;
    }
    if end == 0 {
        return Vec::new();
    }
    let mut start = end;
    while start > 0 && path[start - 1] != b'/' {
        start -= 1;
    }
    let mut result = path[start..end].to_vec();
    if let Some(suffix) = suffix {
        if !suffix.is_empty() && suffix.len() < result.len() && result.ends_with(suffix) {
            result.truncate(result.len() - suffix.len());
        }
    }
    result
}

/// Evaluates PHP `dirname($path, $levels = 1)` over one eval expression.
pub(super) fn eval_builtin_dirname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [path] => {
            let path = eval_expr(path, context, scope, values)?;
            eval_dirname_result(path, None, values)
        }
        [path, levels] => {
            let path = eval_expr(path, context, scope, values)?;
            let levels = eval_expr(levels, context, scope, values)?;
            eval_dirname_result(path, Some(levels), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `dirname()` bytes and returns them as a runtime string.
fn eval_dirname_result(
    path: RuntimeCellHandle,
    levels: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let levels = match levels {
        Some(levels) => eval_int_value(levels, values)?,
        None => 1,
    };
    if levels < 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut current = path;
    for _ in 0..levels {
        current = eval_dirname_once(&current);
    }
    values.string_bytes_value(&current)
}

/// Applies one PHP `dirname()` parent traversal to a path byte string.
fn eval_dirname_once(path: &[u8]) -> Vec<u8> {
    if path.is_empty() {
        return b".".to_vec();
    }
    let mut end = path.len();
    while end > 0 && path[end - 1] == b'/' {
        end -= 1;
    }
    if end == 0 {
        return b"/".to_vec();
    }
    let mut cursor = end;
    while cursor > 0 {
        cursor -= 1;
        if path[cursor] == b'/' {
            let mut parent_end = cursor;
            while parent_end > 0 && path[parent_end - 1] == b'/' {
                parent_end -= 1;
            }
            return if parent_end == 0 {
                b"/".to_vec()
            } else {
                path[..parent_end].to_vec()
            };
        }
    }
    b".".to_vec()
}

/// Evaluates PHP `realpath($path)` over one eval expression.
pub(super) fn eval_builtin_realpath(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_realpath_result(path, values)
}

/// Canonicalizes one path or returns PHP false when the path cannot be resolved.
fn eval_realpath_result(
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let path = String::from_utf8_lossy(&path);
    let Ok(canonical) = std::fs::canonicalize(path.as_ref()) else {
        return values.bool_value(false);
    };
    let canonical = canonical.to_string_lossy();
    values.string(canonical.as_ref())
}

/// Evaluates PHP `pathinfo($path, $flags = PATHINFO_ALL)` over one eval expression.
pub(super) fn eval_builtin_pathinfo(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [path] => {
            let path = eval_expr(path, context, scope, values)?;
            eval_pathinfo_result(path, None, values)
        }
        [path, flags] => {
            let path = eval_expr(path, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_pathinfo_result(path, Some(flags), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `pathinfo()` as either an associative array or one component string.
fn eval_pathinfo_result(
    path: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let Some(flags) = flags else {
        return eval_pathinfo_array_result(&path, values);
    };
    let flags = eval_int_value(flags, values)?;
    if flags == EVAL_PATHINFO_ALL {
        return eval_pathinfo_array_result(&path, values);
    }
    let component = eval_pathinfo_component_bytes(&path, flags);
    values.string_bytes_value(&component)
}

/// Builds the PHP `pathinfo()` associative-array result for all components.
fn eval_pathinfo_array_result(
    path: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(4)?;
    if !path.is_empty() {
        let dirname = eval_pathinfo_dirname_bytes(path);
        result = eval_pathinfo_array_set(result, "dirname", &dirname, values)?;
    }
    let parts = eval_pathinfo_parts(path);
    result = eval_pathinfo_array_set(result, "basename", &parts.basename, values)?;
    if parts.has_extension {
        result = eval_pathinfo_array_set(result, "extension", &parts.extension, values)?;
    }
    eval_pathinfo_array_set(result, "filename", &parts.filename, values)
}

/// Inserts one string component into a PHP `pathinfo()` associative result.
fn eval_pathinfo_array_set(
    array: RuntimeCellHandle,
    key: &str,
    value: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.string_bytes_value(value)?;
    values.array_set(array, key, value)
}

/// Returns one PHP `pathinfo()` component for a non-all bitmask.
fn eval_pathinfo_component_bytes(path: &[u8], flags: i64) -> Vec<u8> {
    if flags & EVAL_PATHINFO_DIRNAME != 0 {
        return eval_pathinfo_dirname_bytes(path);
    }
    let parts = eval_pathinfo_parts(path);
    if flags & EVAL_PATHINFO_BASENAME != 0 {
        return parts.basename;
    }
    if flags & EVAL_PATHINFO_EXTENSION != 0 {
        return parts.extension;
    }
    if flags & EVAL_PATHINFO_FILENAME != 0 {
        return parts.filename;
    }
    Vec::new()
}

/// Computes the dirname component with `pathinfo("")`'s empty-string exception.
fn eval_pathinfo_dirname_bytes(path: &[u8]) -> Vec<u8> {
    if path.is_empty() {
        Vec::new()
    } else {
        eval_dirname_once(path)
    }
}

/// Splits pathinfo basename, extension, and filename components.
fn eval_pathinfo_parts(path: &[u8]) -> EvalPathInfoParts {
    let basename = eval_basename_bytes(path, None);
    let Some(dot) = basename.iter().rposition(|byte| *byte == b'.') else {
        return EvalPathInfoParts {
            filename: basename.clone(),
            basename,
            extension: Vec::new(),
            has_extension: false,
        };
    };
    EvalPathInfoParts {
        filename: basename[..dot].to_vec(),
        extension: basename[dot + 1..].to_vec(),
        basename,
        has_extension: true,
    }
}

/// Pathinfo components derived from a basename.
struct EvalPathInfoParts {
    /// Full basename component.
    basename: Vec<u8>,
    /// Extension component after the final dot, possibly empty for trailing-dot names.
    extension: Vec<u8>,
    /// Filename component before the final dot.
    filename: Vec<u8>,
    /// Whether the basename contained a dot and therefore has an extension key.
    has_extension: bool,
}

/// Evaluates PHP `fnmatch($pattern, $filename, $flags = 0)` over eval expressions.
pub(super) fn eval_builtin_fnmatch(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern, filename] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let filename = eval_expr(filename, context, scope, values)?;
            eval_fnmatch_result(pattern, filename, None, values)
        }
        [pattern, filename, flags] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let filename = eval_expr(filename, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_fnmatch_result(pattern, filename, Some(flags), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Runs PHP-style shell glob matching for one pattern/name pair.
fn eval_fnmatch_result(
    pattern: RuntimeCellHandle,
    filename: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let pattern = values.string_bytes(pattern)?;
    let filename = values.string_bytes(filename)?;
    let flags = match flags {
        Some(flags) => eval_int_value(flags, values)?,
        None => 0,
    };
    values.bool_value(eval_fnmatch_bytes(&pattern, &filename, flags))
}

/// Matches byte strings using the eval-supported `fnmatch()` grammar and flags.
fn eval_fnmatch_bytes(pattern: &[u8], filename: &[u8], flags: i64) -> bool {
    let mut memo = vec![vec![None; filename.len() + 1]; pattern.len() + 1];
    eval_fnmatch_at(pattern, filename, flags, 0, 0, &mut memo)
}

/// Recursively matches a pattern suffix against a filename suffix with memoization.
fn eval_fnmatch_at(
    pattern: &[u8],
    filename: &[u8],
    flags: i64,
    pattern_index: usize,
    filename_index: usize,
    memo: &mut [Vec<Option<bool>>],
) -> bool {
    if let Some(result) = memo[pattern_index][filename_index] {
        return result;
    }
    let result = if pattern_index == pattern.len() {
        filename_index == filename.len()
    } else {
        match pattern[pattern_index] {
            b'*' => eval_fnmatch_star(
                pattern,
                filename,
                flags,
                pattern_index,
                filename_index,
                memo,
            ),
            b'?' => {
                eval_fnmatch_single_wildcard(filename, flags, filename_index)
                    && eval_fnmatch_at(
                        pattern,
                        filename,
                        flags,
                        pattern_index + 1,
                        filename_index + 1,
                        memo,
                    )
            }
            b'[' => eval_fnmatch_class_or_literal(
                pattern,
                filename,
                flags,
                pattern_index,
                filename_index,
                memo,
            ),
            b'\\' if flags & EVAL_FNM_NOESCAPE == 0 => {
                let (literal, next_pattern_index) =
                    eval_fnmatch_escaped_literal(pattern, pattern_index);
                eval_fnmatch_literal(filename, flags, filename_index, literal)
                    && eval_fnmatch_at(
                        pattern,
                        filename,
                        flags,
                        next_pattern_index,
                        filename_index + 1,
                        memo,
                    )
            }
            literal => {
                eval_fnmatch_literal(filename, flags, filename_index, literal)
                    && eval_fnmatch_at(
                        pattern,
                        filename,
                        flags,
                        pattern_index + 1,
                        filename_index + 1,
                        memo,
                    )
            }
        }
    };
    memo[pattern_index][filename_index] = Some(result);
    result
}

/// Handles `*`, including pathname and leading-period restrictions.
fn eval_fnmatch_star(
    pattern: &[u8],
    filename: &[u8],
    flags: i64,
    pattern_index: usize,
    filename_index: usize,
    memo: &mut [Vec<Option<bool>>],
) -> bool {
    let mut next_pattern_index = pattern_index + 1;
    while next_pattern_index < pattern.len() && pattern[next_pattern_index] == b'*' {
        next_pattern_index += 1;
    }
    if eval_fnmatch_at(
        pattern,
        filename,
        flags,
        next_pattern_index,
        filename_index,
        memo,
    ) {
        return true;
    }
    let mut cursor = filename_index;
    while cursor < filename.len() && eval_fnmatch_wildcard_can_consume(filename, flags, cursor) {
        cursor += 1;
        if eval_fnmatch_at(pattern, filename, flags, next_pattern_index, cursor, memo) {
            return true;
        }
    }
    false
}

/// Returns whether `?` can consume the current filename byte.
fn eval_fnmatch_single_wildcard(filename: &[u8], flags: i64, filename_index: usize) -> bool {
    filename_index < filename.len()
        && eval_fnmatch_wildcard_can_consume(filename, flags, filename_index)
}

/// Handles a bracket class, or falls back to a literal `[` when the class is malformed.
fn eval_fnmatch_class_or_literal(
    pattern: &[u8],
    filename: &[u8],
    flags: i64,
    pattern_index: usize,
    filename_index: usize,
    memo: &mut [Vec<Option<bool>>],
) -> bool {
    if filename_index >= filename.len()
        || !eval_fnmatch_wildcard_can_consume(filename, flags, filename_index)
    {
        return false;
    }
    let Some((matches, next_pattern_index)) =
        eval_fnmatch_class_matches(pattern, pattern_index + 1, filename[filename_index], flags)
    else {
        return eval_fnmatch_literal(filename, flags, filename_index, b'[')
            && eval_fnmatch_at(
                pattern,
                filename,
                flags,
                pattern_index + 1,
                filename_index + 1,
                memo,
            );
    };
    matches
        && eval_fnmatch_at(
            pattern,
            filename,
            flags,
            next_pattern_index,
            filename_index + 1,
            memo,
        )
}

/// Matches one bracket class body against the current filename byte.
fn eval_fnmatch_class_matches(
    pattern: &[u8],
    mut index: usize,
    candidate: u8,
    flags: i64,
) -> Option<(bool, usize)> {
    let negated = matches!(pattern.get(index).copied(), Some(b'!' | b'^'));
    if negated {
        index += 1;
    }
    let mut matched = false;
    let mut closed = false;
    while index < pattern.len() {
        if pattern[index] == b']' {
            closed = true;
            index += 1;
            break;
        }
        let start = eval_fnmatch_class_char(pattern, &mut index, flags)?;
        if index + 1 < pattern.len() && pattern[index] == b'-' && pattern[index + 1] != b']' {
            index += 1;
            let end = eval_fnmatch_class_char(pattern, &mut index, flags)?;
            if eval_fnmatch_byte_in_range(candidate, start, end, flags) {
                matched = true;
            }
        } else if eval_fnmatch_byte_eq(candidate, start, flags) {
            matched = true;
        }
    }
    closed.then_some((if negated { !matched } else { matched }, index))
}

/// Reads one character from a bracket class, respecting escapes when enabled.
fn eval_fnmatch_class_char(pattern: &[u8], index: &mut usize, flags: i64) -> Option<u8> {
    if *index >= pattern.len() {
        return None;
    }
    if pattern[*index] == b'\\' && flags & EVAL_FNM_NOESCAPE == 0 && *index + 1 < pattern.len() {
        *index += 2;
        return Some(pattern[*index - 1]);
    }
    let byte = pattern[*index];
    *index += 1;
    Some(byte)
}

/// Returns whether one candidate byte falls within a possibly case-folded range.
fn eval_fnmatch_byte_in_range(candidate: u8, start: u8, end: u8, flags: i64) -> bool {
    let candidate = eval_fnmatch_fold(candidate, flags);
    let start = eval_fnmatch_fold(start, flags);
    let end = eval_fnmatch_fold(end, flags);
    if start <= end {
        candidate >= start && candidate <= end
    } else {
        candidate >= end && candidate <= start
    }
}

/// Reads an escaped literal token outside bracket classes.
fn eval_fnmatch_escaped_literal(pattern: &[u8], pattern_index: usize) -> (u8, usize) {
    if pattern_index + 1 < pattern.len() {
        (pattern[pattern_index + 1], pattern_index + 2)
    } else {
        (b'\\', pattern_index + 1)
    }
}

/// Returns whether one literal pattern byte matches the current filename byte.
fn eval_fnmatch_literal(filename: &[u8], flags: i64, filename_index: usize, literal: u8) -> bool {
    filename_index < filename.len()
        && eval_fnmatch_byte_eq(filename[filename_index], literal, flags)
}

/// Returns whether a wildcard token may consume the current filename byte.
fn eval_fnmatch_wildcard_can_consume(filename: &[u8], flags: i64, filename_index: usize) -> bool {
    if filename_index >= filename.len() {
        return false;
    }
    if flags & EVAL_FNM_PATHNAME != 0 && filename[filename_index] == b'/' {
        return false;
    }
    if flags & EVAL_FNM_PERIOD != 0
        && eval_fnmatch_is_leading_period(filename, flags, filename_index)
    {
        return false;
    }
    true
}

/// Returns whether the current byte is a leading period for `FNM_PERIOD`.
fn eval_fnmatch_is_leading_period(filename: &[u8], flags: i64, filename_index: usize) -> bool {
    filename[filename_index] == b'.'
        && (filename_index == 0
            || (flags & EVAL_FNM_PATHNAME != 0 && filename[filename_index - 1] == b'/'))
}

/// Compares bytes using ASCII case folding when `FNM_CASEFOLD` is present.
fn eval_fnmatch_byte_eq(left: u8, right: u8, flags: i64) -> bool {
    eval_fnmatch_fold(left, flags) == eval_fnmatch_fold(right, flags)
}

/// Applies eval fnmatch's ASCII case folding.
fn eval_fnmatch_fold(byte: u8, flags: i64) -> u8 {
    if flags & EVAL_FNM_CASEFOLD != 0 {
        byte.to_ascii_lowercase()
    } else {
        byte
    }
}

/// Evaluates PHP `preg_match()` over eval expressions.
pub(super) fn eval_builtin_preg_match(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern, subject] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            eval_preg_match_result(pattern, subject, values)
        }
        [pattern, subject, matches] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let EvalExpr::LoadVar(matches_name) = matches else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let (result, matches_array) =
                eval_preg_match_capture_result(pattern, subject, None, values)?;
            for replaced in set_scope_cell(
                context,
                scope,
                matches_name.clone(),
                matches_array,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
            Ok(result)
        }
        [pattern, subject, matches, flags] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let EvalExpr::LoadVar(matches_name) = matches else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let flags = eval_expr(flags, context, scope, values)?;
            let (result, matches_array) =
                eval_preg_match_capture_result(pattern, subject, Some(flags), values)?;
            for replaced in set_scope_cell(
                context,
                scope,
                matches_name.clone(),
                matches_array,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
            Ok(result)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns whether one regex matches the subject string.
fn eval_preg_match_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    values.int(i64::from(regex.is_match(&subject)))
}

/// Returns the match flag plus PHP `$matches` capture array for one regex search.
fn eval_preg_match_capture_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    let flags = eval_preg_match_flags(flags, values)?;
    let offset_capture = flags & EVAL_PREG_OFFSET_CAPTURE != 0;
    let unmatched_as_null = flags & EVAL_PREG_UNMATCHED_AS_NULL != 0;
    if let Some(captures) = regex.captures(&subject) {
        let matches = eval_preg_capture_array(
            &subject,
            Some(&captures),
            offset_capture,
            unmatched_as_null,
            values,
        )?;
        let matched = values.int(1)?;
        return Ok((matched, matches));
    }
    let matches =
        eval_preg_capture_array(&subject, None, offset_capture, unmatched_as_null, values)?;
    let matched = values.int(0)?;
    Ok((matched, matches))
}

/// Returns supported `preg_match()` flags.
fn eval_preg_match_flags(
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(flags) = flags else {
        return Ok(0);
    };
    let flags = eval_int_value(flags, values)?;
    let supported = EVAL_PREG_OFFSET_CAPTURE | EVAL_PREG_UNMATCHED_AS_NULL;
    if flags & !supported != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(flags)
}

/// Evaluates PHP `preg_match_all()` over eval expressions.
pub(super) fn eval_builtin_preg_match_all(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern, subject] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            eval_preg_match_all_result(pattern, subject, values)
        }
        [pattern, subject, matches] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let EvalExpr::LoadVar(matches_name) = matches else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let (result, matches_array) =
                eval_preg_match_all_capture_result(pattern, subject, None, values)?;
            for replaced in set_scope_cell(
                context,
                scope,
                matches_name.clone(),
                matches_array,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
            Ok(result)
        }
        [pattern, subject, matches, flags] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let EvalExpr::LoadVar(matches_name) = matches else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let flags = eval_expr(flags, context, scope, values)?;
            let (result, matches_array) =
                eval_preg_match_all_capture_result(pattern, subject, Some(flags), values)?;
            for replaced in set_scope_cell(
                context,
                scope,
                matches_name.clone(),
                matches_array,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
            Ok(result)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Counts all non-overlapping regex matches in one subject string.
fn eval_preg_match_all_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    let count = regex.captures_iter(&subject).count();
    values.int(i64::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Returns the match count plus PHP's default `PREG_PATTERN_ORDER` `$matches` array.
fn eval_preg_match_all_capture_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let capture_count = regex.captures_len();
    let subject = values.string_bytes(subject)?;
    let captures: Vec<Captures<'_>> = regex.captures_iter(&subject).collect();
    let count = values.int(i64::try_from(captures.len()).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let flags = eval_preg_match_all_flags(flags, values)?;
    let matches = if flags & EVAL_PREG_SET_ORDER != 0 {
        eval_preg_match_all_set_order_array(&subject, &captures, capture_count, flags, values)?
    } else {
        eval_preg_match_all_pattern_order_array(&subject, &captures, capture_count, flags, values)?
    };
    Ok((count, matches))
}

/// Returns supported `preg_match_all()` flags.
fn eval_preg_match_all_flags(
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(flags) = flags else {
        return Ok(EVAL_PREG_PATTERN_ORDER);
    };
    let flags = eval_int_value(flags, values)?;
    let supported = EVAL_PREG_PATTERN_ORDER
        | EVAL_PREG_SET_ORDER
        | EVAL_PREG_OFFSET_CAPTURE
        | EVAL_PREG_UNMATCHED_AS_NULL;
    if flags & !supported != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(flags)
}

/// Builds PHP's default `preg_match_all()` pattern-order capture matrix.
fn eval_preg_match_all_pattern_order_array(
    subject: &[u8],
    captures: &[Captures<'_>],
    capture_count: usize,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let offset_capture = flags & EVAL_PREG_OFFSET_CAPTURE != 0;
    let unmatched_as_null = flags & EVAL_PREG_UNMATCHED_AS_NULL != 0;
    let mut outer = values.array_new(capture_count)?;
    for capture_index in 0..capture_count {
        let mut row = values.array_new(captures.len())?;
        for (match_index, capture) in captures.iter().enumerate() {
            let key =
                values.int(i64::try_from(match_index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
            let value = eval_preg_capture_value(
                subject,
                capture,
                capture_index,
                offset_capture,
                unmatched_as_null,
                values,
            )?;
            row = values.array_set(row, key, value)?;
        }
        let key =
            values.int(i64::try_from(capture_index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        outer = values.array_set(outer, key, row)?;
    }
    Ok(outer)
}

/// Builds PHP's `preg_match_all(..., PREG_SET_ORDER)` match-order capture matrix.
fn eval_preg_match_all_set_order_array(
    subject: &[u8],
    captures: &[Captures<'_>],
    capture_count: usize,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let offset_capture = flags & EVAL_PREG_OFFSET_CAPTURE != 0;
    let unmatched_as_null = flags & EVAL_PREG_UNMATCHED_AS_NULL != 0;
    let mut outer = values.array_new(captures.len())?;
    for (match_index, capture) in captures.iter().enumerate() {
        let mut row = values.array_new(capture_count)?;
        for capture_index in 0..capture_count {
            let key =
                values.int(i64::try_from(capture_index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
            let value = eval_preg_capture_value(
                subject,
                capture,
                capture_index,
                offset_capture,
                unmatched_as_null,
                values,
            )?;
            row = values.array_set(row, key, value)?;
        }
        let key = values.int(i64::try_from(match_index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        outer = values.array_set(outer, key, row)?;
    }
    Ok(outer)
}

/// Evaluates PHP `preg_replace()` over eval expressions.
pub(super) fn eval_builtin_preg_replace(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, replacement, subject] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pattern = eval_expr(pattern, context, scope, values)?;
    let replacement = eval_expr(replacement, context, scope, values)?;
    let subject = eval_expr(subject, context, scope, values)?;
    eval_preg_replace_result(pattern, replacement, subject, values)
}

/// Replaces every regex match with a PHP-style backreference-expanded replacement.
fn eval_preg_replace_result(
    pattern: RuntimeCellHandle,
    replacement: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let replacement = values.string_bytes(replacement)?;
    let subject = values.string_bytes(subject)?;
    let mut result = Vec::with_capacity(subject.len());
    let mut cursor = 0;
    for captures in regex.captures_iter(&subject) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        result.extend_from_slice(&subject[cursor..matched.start()]);
        eval_preg_expand_replacement(&replacement, &subject, &captures, &mut result);
        cursor = matched.end();
    }
    result.extend_from_slice(&subject[cursor..]);
    values.string_bytes_value(&result)
}

/// Evaluates PHP `preg_replace_callback()` over eval expressions.
pub(super) fn eval_builtin_preg_replace_callback(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, callback, subject] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pattern = eval_expr(pattern, context, scope, values)?;
    let callback = eval_expr(callback, context, scope, values)?;
    let subject = eval_expr(subject, context, scope, values)?;
    eval_preg_replace_callback_result(pattern, callback, subject, context, values)
}

/// Replaces every regex match by invoking an eval-supported callback with `$matches`.
fn eval_preg_replace_callback_result(
    pattern: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let callback = eval_callable_name(callback, values)?;
    let subject = values.string_bytes(subject)?;
    let mut result = Vec::with_capacity(subject.len());
    let mut cursor = 0;
    for captures in regex.captures_iter(&subject) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        result.extend_from_slice(&subject[cursor..matched.start()]);
        let matches = eval_preg_capture_array(&subject, Some(&captures), false, false, values)?;
        let callback_result = eval_callable_with_values(&callback, vec![matches], context, values)?;
        let callback_result = values.cast_string(callback_result)?;
        let callback_bytes = values.string_bytes(callback_result)?;
        result.extend_from_slice(&callback_bytes);
        cursor = matched.end();
    }
    result.extend_from_slice(&subject[cursor..]);
    values.string_bytes_value(&result)
}

/// Evaluates PHP `preg_split()` over eval expressions.
pub(super) fn eval_builtin_preg_split(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern, subject] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            eval_preg_split_result(pattern, subject, None, None, values)
        }
        [pattern, subject, limit] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let limit = eval_expr(limit, context, scope, values)?;
            eval_preg_split_result(pattern, subject, Some(limit), None, values)
        }
        [pattern, subject, limit, flags] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let limit = eval_expr(limit, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_preg_split_result(pattern, subject, Some(limit), Some(flags), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Splits a subject string with eval-supported `preg_split()` flags.
fn eval_preg_split_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    limit: Option<RuntimeCellHandle>,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    let limit = eval_preg_split_limit(limit, values)?;
    let flags = eval_preg_split_flags(flags, values)?;
    let no_empty = flags & EVAL_PREG_SPLIT_NO_EMPTY != 0;
    let capture_delimiters = flags & EVAL_PREG_SPLIT_DELIM_CAPTURE != 0;
    let offset_capture = flags & EVAL_PREG_SPLIT_OFFSET_CAPTURE != 0;
    let mut pieces = Vec::<EvalPregSplitPiece>::new();
    let mut cursor = 0;

    for captures in regex.captures_iter(&subject) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        if eval_preg_split_reached_limit(&pieces, limit) {
            break;
        }
        eval_preg_split_push_piece(
            &mut pieces,
            &subject[cursor..matched.start()],
            cursor,
            no_empty,
        );
        if capture_delimiters {
            eval_preg_split_push_captures(&mut pieces, &subject, &captures, no_empty);
        }
        cursor = matched.end();
    }
    eval_preg_split_push_piece(&mut pieces, &subject[cursor..], cursor, no_empty);

    let mut result = values.array_new(pieces.len())?;
    for (index, piece) in pieces.iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        let value = eval_preg_split_piece_value(piece, offset_capture, values)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Compiles one eval PCRE-style delimited pattern into a Rust regex.
fn eval_preg_regex(
    pattern: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Regex, EvalStatus> {
    let pattern = values.string_bytes(pattern)?;
    let (body, modifiers) = eval_preg_pattern_parts(&pattern)?;
    let body = String::from_utf8(body).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut builder = RegexBuilder::new(&body);
    builder
        .case_insensitive(modifiers.case_insensitive)
        .multi_line(modifiers.multi_line)
        .dot_matches_new_line(modifiers.dot_matches_new_line)
        .swap_greed(modifiers.swap_greed);
    builder.build().map_err(|_| EvalStatus::RuntimeFatal)
}

/// Regex modifiers supported by eval `preg_*` pattern stripping.
#[derive(Default)]
struct EvalPregModifiers {
    case_insensitive: bool,
    multi_line: bool,
    dot_matches_new_line: bool,
    swap_greed: bool,
}

/// One `preg_split()` output segment plus its byte offset in the subject.
struct EvalPregSplitPiece {
    bytes: Vec<u8>,
    offset: usize,
}

/// Splits a PHP delimited regex into body bytes and supported modifiers.
fn eval_preg_pattern_parts(pattern: &[u8]) -> Result<(Vec<u8>, EvalPregModifiers), EvalStatus> {
    if pattern.len() < 2 || pattern[0].is_ascii_alphanumeric() || pattern[0].is_ascii_whitespace() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let delimiter = pattern[0];
    if delimiter == b'\\' {
        return Err(EvalStatus::RuntimeFatal);
    }
    let closing = eval_preg_closing_delimiter(delimiter);
    let close_index =
        eval_preg_find_closing_delimiter(pattern, closing).ok_or(EvalStatus::RuntimeFatal)?;
    let body = eval_preg_unescape_delimiter(&pattern[1..close_index], delimiter, closing);
    let modifiers = eval_preg_modifiers(&pattern[close_index + 1..])?;
    Ok((body, modifiers))
}

/// Returns the closing regex delimiter for PHP's paired delimiter forms.
fn eval_preg_closing_delimiter(delimiter: u8) -> u8 {
    match delimiter {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        b'<' => b'>',
        _ => delimiter,
    }
}

/// Finds the first unescaped closing regex delimiter.
fn eval_preg_find_closing_delimiter(pattern: &[u8], closing: u8) -> Option<usize> {
    let mut escaped = false;
    for (index, byte) in pattern.iter().copied().enumerate().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            continue;
        }
        if byte == closing {
            return Some(index);
        }
    }
    None
}

/// Removes escapes that only protect the PHP regex delimiter from pattern stripping.
fn eval_preg_unescape_delimiter(body: &[u8], delimiter: u8, closing: u8) -> Vec<u8> {
    let mut result = Vec::with_capacity(body.len());
    let mut index = 0;
    while index < body.len() {
        if body[index] == b'\\'
            && index + 1 < body.len()
            && matches!(body[index + 1], byte if byte == delimiter || byte == closing)
        {
            result.push(body[index + 1]);
            index += 2;
        } else {
            result.push(body[index]);
            index += 1;
        }
    }
    result
}

/// Parses eval-supported PHP regex modifiers.
fn eval_preg_modifiers(modifiers: &[u8]) -> Result<EvalPregModifiers, EvalStatus> {
    let mut parsed = EvalPregModifiers::default();
    for modifier in modifiers {
        match *modifier {
            b'i' => parsed.case_insensitive = true,
            b'm' => parsed.multi_line = true,
            b's' => parsed.dot_matches_new_line = true,
            b'U' => parsed.swap_greed = true,
            b'u' => {}
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
    Ok(parsed)
}

/// Builds PHP's indexed `$matches` capture array for one regex result.
fn eval_preg_capture_array(
    subject: &[u8],
    captures: Option<&Captures<'_>>,
    offset_capture: bool,
    unmatched_as_null: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = captures.map_or(0, |captures| {
        eval_preg_visible_capture_len(captures, unmatched_as_null)
    });
    let mut result = values.array_new(len)?;
    if let Some(captures) = captures {
        for index in 0..len {
            let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
            let value = eval_preg_capture_value(
                subject,
                captures,
                index,
                offset_capture,
                unmatched_as_null,
                values,
            )?;
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// Returns the capture count PHP should expose, dropping trailing unmatched groups.
fn eval_preg_visible_capture_len(captures: &Captures<'_>, unmatched_as_null: bool) -> usize {
    if unmatched_as_null {
        return captures.len();
    }
    let mut len = captures.len();
    while len > 1 && captures.get(len - 1).is_none() {
        len -= 1;
    }
    len
}

/// Returns one captured byte range from the original subject.
fn eval_preg_capture_bytes<'a>(
    subject: &'a [u8],
    captures: &Captures<'_>,
    index: usize,
) -> Option<&'a [u8]> {
    captures
        .get(index)
        .map(|matched| &subject[matched.start()..matched.end()])
}

/// Builds one capture entry as either a string or PHP's `[string, byte_offset]` pair.
fn eval_preg_capture_value(
    subject: &[u8],
    captures: &Captures<'_>,
    index: usize,
    offset_capture: bool,
    unmatched_as_null: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let matched = captures.get(index);
    let value = if matched.is_none() && unmatched_as_null {
        values.null()?
    } else {
        let bytes = matched.as_ref().map_or(b"".as_slice(), |matched| {
            &subject[matched.start()..matched.end()]
        });
        values.string_bytes_value(bytes)?
    };
    if !offset_capture {
        return Ok(value);
    }

    let offset = matched.map_or(Ok(-1_i64), |matched| {
        i64::try_from(matched.start()).map_err(|_| EvalStatus::RuntimeFatal)
    })?;
    let offset = values.int(offset)?;
    let mut pair = values.array_new(2)?;
    let value_key = values.int(0)?;
    pair = values.array_set(pair, value_key, value)?;
    let offset_key = values.int(1)?;
    values.array_set(pair, offset_key, offset)
}

/// Appends one replacement string after expanding `$n`, `${n}`, and `\n` captures.
fn eval_preg_expand_replacement(
    replacement: &[u8],
    subject: &[u8],
    captures: &Captures<'_>,
    result: &mut Vec<u8>,
) {
    let mut index = 0;
    while index < replacement.len() {
        match replacement[index] {
            b'$' => {
                if let Some((capture_index, next_index)) =
                    eval_preg_replacement_capture_index(replacement, index + 1)
                {
                    if let Some(bytes) = eval_preg_capture_bytes(subject, captures, capture_index) {
                        result.extend_from_slice(bytes);
                    }
                    index = next_index;
                } else {
                    result.push(replacement[index]);
                    index += 1;
                }
            }
            b'\\' if index + 1 < replacement.len() && replacement[index + 1].is_ascii_digit() => {
                let (capture_index, next_index) =
                    eval_preg_decimal_capture_index(replacement, index + 1);
                if let Some(bytes) = eval_preg_capture_bytes(subject, captures, capture_index) {
                    result.extend_from_slice(bytes);
                }
                index = next_index;
            }
            byte => {
                result.push(byte);
                index += 1;
            }
        }
    }
}

/// Parses a dollar-style replacement capture reference.
fn eval_preg_replacement_capture_index(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    if bytes.get(index).copied() == Some(b'{') {
        let mut cursor = index + 1;
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == start || bytes.get(cursor).copied() != Some(b'}') {
            return None;
        }
        let capture = eval_preg_decimal_bytes_to_usize(&bytes[start..cursor])?;
        return Some((capture, cursor + 1));
    }
    if bytes.get(index).is_some_and(u8::is_ascii_digit) {
        let (capture, next) = eval_preg_decimal_capture_index(bytes, index);
        return Some((capture, next));
    }
    None
}

/// Parses a one- or two-digit replacement capture reference.
fn eval_preg_decimal_capture_index(bytes: &[u8], index: usize) -> (usize, usize) {
    let mut cursor = index;
    let end = usize::min(bytes.len(), index + 2);
    while cursor < end && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    (
        eval_preg_decimal_bytes_to_usize(&bytes[index..cursor]).unwrap_or(0),
        cursor,
    )
}

/// Converts ASCII decimal bytes into a `usize` capture index.
fn eval_preg_decimal_bytes_to_usize(bytes: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    for byte in bytes {
        value = value.checked_mul(10)?;
        value = value.checked_add(usize::from(byte - b'0'))?;
    }
    Some(value)
}

/// Returns the PHP `preg_split()` limit, treating zero as unlimited.
fn eval_preg_split_limit(
    limit: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<usize>, EvalStatus> {
    let Some(limit) = limit else {
        return Ok(None);
    };
    let limit = eval_int_value(limit, values)?;
    if limit <= 0 {
        return Ok(None);
    }
    usize::try_from(limit)
        .map(Some)
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Returns supported `preg_split()` flags.
fn eval_preg_split_flags(
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(flags) = flags else {
        return Ok(0);
    };
    let flags = eval_int_value(flags, values)?;
    let supported =
        EVAL_PREG_SPLIT_NO_EMPTY | EVAL_PREG_SPLIT_DELIM_CAPTURE | EVAL_PREG_SPLIT_OFFSET_CAPTURE;
    if flags & !supported != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(flags)
}

/// Returns whether `preg_split()` should stop splitting and emit the remaining subject.
fn eval_preg_split_reached_limit(pieces: &[EvalPregSplitPiece], limit: Option<usize>) -> bool {
    matches!(limit, Some(limit) if limit > 0 && pieces.len() + 1 >= limit)
}

/// Pushes one `preg_split()` output piece, honoring `PREG_SPLIT_NO_EMPTY`.
fn eval_preg_split_push_piece(
    pieces: &mut Vec<EvalPregSplitPiece>,
    piece: &[u8],
    offset: usize,
    no_empty: bool,
) {
    if no_empty && piece.is_empty() {
        return;
    }
    pieces.push(EvalPregSplitPiece {
        bytes: piece.to_vec(),
        offset,
    });
}

/// Pushes captured delimiters for `PREG_SPLIT_DELIM_CAPTURE`.
fn eval_preg_split_push_captures(
    pieces: &mut Vec<EvalPregSplitPiece>,
    subject: &[u8],
    captures: &Captures<'_>,
    no_empty: bool,
) {
    for index in 1..captures.len() {
        if let Some(matched) = captures.get(index) {
            eval_preg_split_push_piece(
                pieces,
                &subject[matched.start()..matched.end()],
                matched.start(),
                no_empty,
            );
        }
    }
}

/// Converts one split segment to a string or PHP `[string, byte_offset]` pair.
fn eval_preg_split_piece_value(
    piece: &EvalPregSplitPiece,
    offset_capture: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = values.string_bytes_value(&piece.bytes)?;
    if !offset_capture {
        return Ok(value);
    }

    let offset = i64::try_from(piece.offset).map_err(|_| EvalStatus::RuntimeFatal)?;
    let offset = values.int(offset)?;
    let mut pair = values.array_new(2)?;
    let value_key = values.int(0)?;
    pair = values.array_set(pair, value_key, value)?;
    let offset_key = values.int(1)?;
    values.array_set(pair, offset_key, offset)
}

/// Evaluates PHP `gethostbyaddr($ip)` over one eval expression.
pub(super) fn eval_builtin_gethostbyaddr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_gethostbyaddr_result(ip, values)
}

/// Reverse-resolves one IPv4 address, returns the input on miss, or PHP false when malformed.
fn eval_gethostbyaddr_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ip_bytes = values.string_bytes(ip)?;
    let ip_text = String::from_utf8_lossy(&ip_bytes);
    let Ok(ipv4) = ip_text.parse::<std::net::Ipv4Addr>() else {
        return values.bool_value(false);
    };
    let octets = ipv4.octets();
    let resolved = unsafe {
        // libc reads the stack-owned IPv4 octets during this call and returns
        // static resolver storage, which is copied before the next resolver call.
        let host = libc_gethostbyaddr(
            octets.as_ptr().cast::<libc::c_void>(),
            octets.len() as libc::socklen_t,
            libc::AF_INET,
        );
        if host.is_null() || (*host).h_name.is_null() {
            None
        } else {
            Some(CStr::from_ptr((*host).h_name).to_bytes().to_vec())
        }
    };
    match resolved {
        Some(name) if !name.is_empty() => values.string_bytes_value(&name),
        _ => values.string(ip_text.as_ref()),
    }
}

/// Evaluates PHP `gethostbyname($hostname)` over one eval expression.
pub(super) fn eval_builtin_gethostbyname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [hostname] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let hostname = eval_expr(hostname, context, scope, values)?;
    eval_gethostbyname_result(hostname, values)
}

/// Resolves one host name to an IPv4 string, or returns the original input on failure.
fn eval_gethostbyname_result(
    hostname: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let hostname = values.string_bytes(hostname)?;
    let hostname = String::from_utf8_lossy(&hostname);
    if hostname.parse::<std::net::Ipv4Addr>().is_ok() {
        return values.string(hostname.as_ref());
    }
    let resolved = (hostname.as_ref(), 0_u16)
        .to_socket_addrs()
        .ok()
        .and_then(|addrs| {
            addrs
                .filter_map(|addr| match addr.ip() {
                    std::net::IpAddr::V4(ip) => Some(ip.to_string()),
                    std::net::IpAddr::V6(_) => None,
                })
                .next()
        });
    values.string(resolved.as_deref().unwrap_or_else(|| hostname.as_ref()))
}

/// Evaluates PHP `gethostname()` over one eval expression.
pub(super) fn eval_builtin_gethostname(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_gethostname_result(values)
}

/// Reads the current host name through libc and returns an empty string on failure.
fn eval_gethostname_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut buffer = [0 as libc::c_char; 256];
    let status = unsafe {
        // libc writes at most buffer.len() bytes into this stack buffer.
        libc::gethostname(buffer.as_mut_ptr(), buffer.len())
    };
    if status != 0 {
        return values.string("");
    }
    let length = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    let hostname = buffer[..length]
        .iter()
        .map(|byte| *byte as u8)
        .collect::<Vec<_>>();
    values.string_bytes_value(&hostname)
}

/// Evaluates PHP `getprotobyname($protocol)` over one eval expression.
pub(super) fn eval_builtin_getprotobyname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getprotobyname_result(protocol, values)
}

/// Looks up an IP protocol number by name or alias.
fn eval_getprotobyname_result(
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(protocol) = eval_lowercase_c_string(protocol, values)? else {
        return values.bool_value(false);
    };
    let entry = unsafe {
        // libc returns a process-global protoent; copy scalar fields before another lookup.
        libc_getprotobyname(protocol.as_ptr())
    };
    if entry.is_null() {
        return values.bool_value(false);
    }
    let number = unsafe { (*entry).p_proto };
    values.int(i64::from(number))
}

/// Evaluates PHP `getprotobynumber($protocol)` over one eval expression.
pub(super) fn eval_builtin_getprotobynumber(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getprotobynumber_result(protocol, values)
}

/// Looks up an IP protocol name by numeric protocol id.
fn eval_getprotobynumber_result(
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let protocol = eval_int_value(protocol, values)?;
    let Ok(protocol) = libc::c_int::try_from(protocol) else {
        return values.bool_value(false);
    };
    let entry = unsafe {
        // libc returns a process-global protoent; copy the name before another lookup.
        libc_getprotobynumber(protocol)
    };
    eval_protoent_name_or_false(entry, values)
}

/// Evaluates PHP `getservbyname($service, $protocol)` over two eval expressions.
pub(super) fn eval_builtin_getservbyname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [service, protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let service = eval_expr(service, context, scope, values)?;
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getservbyname_result(service, protocol, values)
}

/// Looks up an internet service port by service name and protocol.
fn eval_getservbyname_result(
    service: RuntimeCellHandle,
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(service) = eval_lowercase_c_string(service, values)? else {
        return values.bool_value(false);
    };
    let Some(protocol) = eval_lowercase_c_string(protocol, values)? else {
        return values.bool_value(false);
    };
    let entry = unsafe {
        // libc returns a process-global servent; copy scalar fields before another lookup.
        libc_getservbyname(service.as_ptr(), protocol.as_ptr())
    };
    if entry.is_null() {
        return values.bool_value(false);
    }
    let port = unsafe { u16::from_be((*entry).s_port as u16) };
    values.int(i64::from(port))
}

/// Evaluates PHP `getservbyport($port, $protocol)` over two eval expressions.
pub(super) fn eval_builtin_getservbyport(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [port, protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let port = eval_expr(port, context, scope, values)?;
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getservbyport_result(port, protocol, values)
}

/// Looks up an internet service name by port and protocol.
fn eval_getservbyport_result(
    port: RuntimeCellHandle,
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let port = eval_int_value(port, values)?;
    let Ok(port) = u16::try_from(port) else {
        return values.bool_value(false);
    };
    let Some(protocol) = eval_lowercase_c_string(protocol, values)? else {
        return values.bool_value(false);
    };
    let network_port = port.to_be() as libc::c_int;
    let entry = unsafe {
        // libc returns a process-global servent; copy the name before another lookup.
        libc_getservbyport(network_port, protocol.as_ptr())
    };
    eval_servent_name_or_false(entry, values)
}

/// Converts a PHP value to a NUL-free lowercase C string for libc database lookups.
fn eval_lowercase_c_string(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<CString>, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let bytes = bytes
        .into_iter()
        .map(|byte| byte.to_ascii_lowercase())
        .collect::<Vec<_>>();
    Ok(CString::new(bytes).ok())
}

/// Copies a protoent canonical name into a PHP string or returns PHP false.
fn eval_protoent_name_or_false(
    entry: *mut libc::protoent,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if entry.is_null() {
        return values.bool_value(false);
    }
    let name = unsafe {
        let name = (*entry).p_name;
        if name.is_null() {
            return values.bool_value(false);
        }
        CStr::from_ptr(name).to_bytes().to_vec()
    };
    values.string_bytes_value(&name)
}

/// Copies a servent canonical name into a PHP string or returns PHP false.
fn eval_servent_name_or_false(
    entry: *mut libc::servent,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if entry.is_null() {
        return values.bool_value(false);
    }
    let name = unsafe {
        let name = (*entry).s_name;
        if name.is_null() {
            return values.bool_value(false);
        }
        CStr::from_ptr(name).to_bytes().to_vec()
    };
    values.string_bytes_value(&name)
}

/// Evaluates PHP `long2ip($ip)` over one eval expression.
pub(super) fn eval_builtin_long2ip(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_long2ip_result(ip, values)
}

/// Formats one 32-bit IPv4 integer as a dotted-quad string.
fn eval_long2ip_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ip = eval_int_value(ip, values)? as u32;
    values.string(&eval_format_ipv4(ip))
}

/// Evaluates PHP `ip2long($ip)` over one eval expression.
pub(super) fn eval_builtin_ip2long(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_ip2long_result(ip, values)
}

/// Parses a dotted-quad IPv4 string into an integer or PHP false.
fn eval_ip2long_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(ip)?;
    match eval_parse_ipv4(&bytes) {
        Some(ip) => values.int(i64::from(ip)),
        None => values.bool_value(false),
    }
}

/// Evaluates PHP `inet_pton($ip)` over one eval expression.
pub(super) fn eval_builtin_inet_pton(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_inet_pton_result(ip, values)
}

/// Packs a dotted-quad IPv4 string into four network-order bytes or PHP false.
fn eval_inet_pton_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(ip)?;
    let Some(ip) = eval_parse_ipv4(&bytes) else {
        return values.bool_value(false);
    };
    values.string_bytes_value(&ip.to_be_bytes())
}

/// Evaluates PHP `inet_ntop($binary)` over one eval expression.
pub(super) fn eval_builtin_inet_ntop(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [binary] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let binary = eval_expr(binary, context, scope, values)?;
    eval_inet_ntop_result(binary, values)
}

/// Renders a four-byte IPv4 string as dotted-quad text or PHP false.
fn eval_inet_ntop_result(
    binary: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(binary)?;
    let [a, b, c, d] = bytes.as_slice() else {
        return values.bool_value(false);
    };
    let ip = u32::from_be_bytes([*a, *b, *c, *d]);
    values.string(&eval_format_ipv4(ip))
}

/// Parses exactly four decimal IPv4 octets separated by dots.
fn eval_parse_ipv4(bytes: &[u8]) -> Option<u32> {
    let mut octets = [0_u8; 4];
    let mut position = 0_usize;
    let mut index = 0_usize;

    while index < 4 {
        if position >= bytes.len() {
            return None;
        }
        let start = position;
        let mut value = 0_u16;
        while position < bytes.len() && bytes[position].is_ascii_digit() {
            value = value
                .checked_mul(10)?
                .checked_add(u16::from(bytes[position] - b'0'))?;
            position += 1;
            if position - start > 3 || value > 255 {
                return None;
            }
        }
        if position == start {
            return None;
        }
        octets[index] = value as u8;
        index += 1;
        if index == 4 {
            return (position == bytes.len()).then(|| u32::from_be_bytes(octets));
        }
        if bytes.get(position).copied() != Some(b'.') {
            return None;
        }
        position += 1;
    }
    None
}

/// Formats one packed IPv4 integer into dotted-quad text.
fn eval_format_ipv4(ip: u32) -> String {
    let [a, b, c, d] = ip.to_be_bytes();
    format!("{}.{}.{}.{}", a, b, c, d)
}

/// Evaluates PHP `getenv($name)` over one eval expression.
pub(super) fn eval_builtin_getenv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    eval_getenv_result(name, values)
}

/// Reads one environment variable and returns an empty string when it is unset.
fn eval_getenv_result(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8_lossy(&name);
    let value = std::env::var_os(name.as_ref())
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    values.string(&value)
}

/// Evaluates PHP `putenv($assignment)` over one eval expression.
pub(super) fn eval_builtin_putenv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [assignment] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let assignment = eval_expr(assignment, context, scope, values)?;
    eval_putenv_result(assignment, values)
}

/// Applies one `putenv()` assignment to the host environment.
fn eval_putenv_result(
    assignment: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let assignment = values.string_bytes(assignment)?;
    if let Some(separator) = assignment.iter().position(|byte| *byte == b'=') {
        let name = String::from_utf8_lossy(&assignment[..separator]);
        let value = String::from_utf8_lossy(&assignment[separator + 1..]);
        std::env::set_var(name.as_ref(), value.as_ref());
    } else {
        let name = String::from_utf8_lossy(&assignment);
        std::env::remove_var(name.as_ref());
    }
    values.bool_value(true)
}

/// Evaluates PHP `sys_get_temp_dir()` with no arguments.
pub(super) fn eval_builtin_sys_get_temp_dir(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_sys_get_temp_dir_result(values)
}

/// Returns the same temporary directory literal as the native static builtin.
fn eval_sys_get_temp_dir_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string("/tmp")
}

/// Evaluates PHP `realpath_cache_get()` with no arguments.
pub(super) fn eval_builtin_realpath_cache_get(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_realpath_cache_get_result(values)
}

/// Returns elephc's intentionally empty realpath-cache view.
fn eval_realpath_cache_get_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.array_new(0)
}

/// Evaluates PHP `realpath_cache_size()` with no arguments.
pub(super) fn eval_builtin_realpath_cache_size(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_realpath_cache_size_result(values)
}

/// Returns zero because elephc does not maintain a runtime realpath cache.
fn eval_realpath_cache_size_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(0)
}

/// Returns the standard zlib/PHP CRC-32 checksum for a byte slice.
fn eval_crc32_bytes(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// Casts one eval value to PHP int and returns the scalar payload.
pub(super) fn eval_int_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let value = values.cast_int(value)?;
    let bytes = values.string_bytes(value)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .parse::<i64>()
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates PHP's `bin2hex(...)` over one eval expression.
pub(super) fn eval_builtin_bin2hex(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_bin2hex_result(value, values)
}

/// Converts one eval value through PHP string conversion and returns lowercase hex bytes.
fn eval_bin2hex_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    values.string(&eval_lower_hex_bytes(&bytes))
}

/// Converts bytes to lowercase hexadecimal text.
fn eval_lower_hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

/// Evaluates PHP's `hex2bin(...)` over one eval expression.
pub(super) fn eval_builtin_hex2bin(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_hex2bin_result(value, values)
}

/// Converts one eval value through PHP string conversion and decodes hexadecimal bytes.
fn eval_hex2bin_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    if bytes.len() % 2 != 0 {
        values.warning(HEX2BIN_ODD_LENGTH_WARNING)?;
        return values.bool_value(false);
    }
    let mut output = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let Some(high) = eval_hex_nibble(pair[0]) else {
            values.warning(HEX2BIN_INVALID_WARNING)?;
            return values.bool_value(false);
        };
        let Some(low) = eval_hex_nibble(pair[1]) else {
            values.warning(HEX2BIN_INVALID_WARNING)?;
            return values.bool_value(false);
        };
        output.push((high << 4) | low);
    }
    values.string_bytes_value(&output)
}

/// Returns the four-bit value for one hexadecimal byte.
fn eval_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Evaluates PHP's `addslashes(...)` or `stripslashes(...)` over one eval expression.
pub(super) fn eval_builtin_slashes(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_slashes_result(name, value, values)
}

/// Applies PHP byte-string escaping or unescaping for addslashes/stripslashes.
fn eval_slashes_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "addslashes" => eval_addslashes_result(value, values),
        "stripslashes" => eval_stripslashes_result(value, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Escapes NUL, quotes, and backslashes using PHP `addslashes()` byte semantics.
fn eval_addslashes_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    for byte in bytes {
        match byte {
            0 => output.extend_from_slice(b"\\0"),
            b'\'' | b'"' | b'\\' => {
                output.push(b'\\');
                output.push(byte);
            }
            _ => output.push(byte),
        }
    }
    values.string_bytes_value(&output)
}

/// Removes backslash quoting using PHP `stripslashes()` byte semantics.
fn eval_stripslashes_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 1;
            if let Some(byte) = bytes.get(index).copied() {
                output.push(if byte == b'0' { 0 } else { byte });
                index += 1;
            }
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP's `base64_encode(...)` over one eval expression.
pub(super) fn eval_builtin_base64_encode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_base64_encode_result(value, values)
}

/// Converts one eval value through PHP string conversion and returns Base64 text.
fn eval_base64_encode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = String::with_capacity(((bytes.len() + 2) / 3) * 4);
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(third & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    values.string(&output)
}

/// Evaluates PHP's one-argument `base64_decode(...)` over one eval expression.
pub(super) fn eval_builtin_base64_decode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_base64_decode_result(value, values)
}

/// Converts one eval value through PHP string conversion and decodes Base64 bytes.
fn eval_base64_decode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let input = values.string_bytes(value)?;
    let mut output = Vec::with_capacity((input.len() / 4) * 3);
    let mut quartet = Vec::with_capacity(4);
    for byte in input {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            quartet.push(None);
        } else if let Some(value) = eval_base64_decode_sextet(byte) {
            quartet.push(Some(value));
        } else {
            continue;
        }
        if quartet.len() == 4 {
            eval_push_base64_decoded_quartet(&quartet, &mut output);
            quartet.clear();
        }
    }
    if !quartet.is_empty() {
        while quartet.len() < 4 {
            quartet.push(None);
        }
        eval_push_base64_decoded_quartet(&quartet, &mut output);
    }
    values.string_bytes_value(&output)
}

/// Returns the six-bit Base64 value for one encoded byte.
fn eval_base64_decode_sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Appends decoded bytes for one padded or unpadded Base64 quartet.
fn eval_push_base64_decoded_quartet(quartet: &[Option<u8>], output: &mut Vec<u8>) {
    let (Some(first), Some(second)) = (quartet[0], quartet[1]) else {
        return;
    };
    output.push((first << 2) | (second >> 4));
    let Some(third) = quartet[2] else {
        return;
    };
    output.push(((second & 0x0f) << 4) | (third >> 2));
    let Some(fourth) = quartet[3] else {
        return;
    };
    output.push(((third & 0x03) << 6) | fourth);
}

/// Evaluates PHP one-argument floating-point math builtins over one eval expression.
pub(super) fn eval_builtin_float_unary(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_float_unary_result(name, value, values)
}

/// Dispatches an evaluated value through the matching PHP floating-point unary math function.
fn eval_float_unary_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_float_value(value, values)?;
    let result = match name {
        "acos" => value.acos(),
        "asin" => value.asin(),
        "atan" => value.atan(),
        "cos" => value.cos(),
        "cosh" => value.cosh(),
        "deg2rad" => value.to_radians(),
        "exp" => value.exp(),
        "log2" => value.log2(),
        "log10" => value.log10(),
        "rad2deg" => value.to_degrees(),
        "sin" => value.sin(),
        "sinh" => value.sinh(),
        "tan" => value.tan(),
        "tanh" => value.tanh(),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.float(result)
}

/// Evaluates PHP two-argument floating-point math builtins over eval expressions.
pub(super) fn eval_builtin_float_pair(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_float_pair_result(name, left, right, values)
}

/// Dispatches an evaluated pair through PHP `atan2()` or `hypot()`.
fn eval_float_pair_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left = eval_float_value(left, values)?;
    let right = eval_float_value(right, values)?;
    let result = match name {
        "atan2" => left.atan2(right),
        "hypot" => left.hypot(right),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.float(result)
}

/// Evaluates PHP `log($num, $base = e)` over eval expressions.
pub(super) fn eval_builtin_log(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [num] => {
            let num = eval_expr(num, context, scope, values)?;
            eval_log_result(num, None, values)
        }
        [num, base] => {
            let num = eval_expr(num, context, scope, values)?;
            let base = eval_expr(base, context, scope, values)?;
            eval_log_result(num, Some(base), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `log()` from already evaluated arguments.
fn eval_log_result(
    num: RuntimeCellHandle,
    base: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let num = eval_float_value(num, values)?;
    let result = match base {
        Some(base) => num.log(eval_float_value(base, values)?),
        None => num.ln(),
    };
    values.float(result)
}

/// Evaluates PHP `intdiv(...)` over two eval expressions.
pub(super) fn eval_builtin_intdiv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_intdiv_result(left, right, values)
}

/// Computes PHP integer division from already evaluated arguments.
fn eval_intdiv_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left = eval_int_value(left, values)?;
    let right = eval_int_value(right, values)?;
    if right == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = left.checked_div(right).ok_or(EvalStatus::RuntimeFatal)?;
    values.int(result)
}

/// Evaluates PHP floating-point binary math builtins over two eval expressions.
pub(super) fn eval_builtin_float_binary(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_float_binary_result(name, left, right, values)
}

/// Dispatches an evaluated pair through the matching PHP float math hook.
fn eval_float_binary_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "fdiv" => values.fdiv(left, right),
        "fmod" => values.fmod(left, right),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP `clamp($value, $min, $max)` over three eval expressions.
pub(super) fn eval_builtin_clamp(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value, min, max] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let min = eval_expr(min, context, scope, values)?;
    let max = eval_expr(max, context, scope, values)?;
    eval_clamp_result(value, min, max, values)
}

/// Selects the inclusive clamp result after validating bound order and NaN bounds.
fn eval_clamp_result(
    value: RuntimeCellHandle,
    min: RuntimeCellHandle,
    max: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_clamp_bound_is_nan(min, values)? || eval_clamp_bound_is_nan(max, values)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let invalid_bounds = values.compare(EvalBinOp::Gt, min, max)?;
    if values.truthy(invalid_bounds)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let above_max = values.compare(EvalBinOp::Gt, value, max)?;
    if values.truthy(above_max)? {
        return Ok(max);
    }
    let below_min = values.compare(EvalBinOp::Lt, value, min)?;
    if values.truthy(below_min)? {
        return Ok(min);
    }
    Ok(value)
}

/// Returns whether a clamp bound is a floating-point NaN value.
fn eval_clamp_bound_is_nan(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_FLOAT {
        return Ok(false);
    }
    Ok(eval_float_value(value, values)?.is_nan())
}

/// Evaluates PHP numeric `min(...)` and `max(...)` over eval expressions.
pub(super) fn eval_builtin_min_max(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_min_max_result(name, &evaluated_args, values)
}

/// Selects the smallest or largest evaluated cell using runtime comparison hooks.
fn eval_min_max_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((&first, rest)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let op = match name {
        "min" => EvalBinOp::Lt,
        "max" => EvalBinOp::Gt,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let mut selected = first;
    for candidate in rest {
        let better = values.compare(op, *candidate, selected)?;
        if values.truthy(better)? {
            selected = *candidate;
        }
    }
    Ok(selected)
}

/// Evaluates PHP scalar cast builtins over one eval expression.
pub(super) fn eval_builtin_cast(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_cast_result(name, value, values)
}

/// Dispatches an already evaluated value through the matching PHP cast hook.
fn eval_cast_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "intval" => values.cast_int(value),
        "floatval" => values.cast_float(value),
        "strval" => values.cast_string(value),
        "boolval" => values.cast_bool(value),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP's `gettype(...)` over one eval expression.
pub(super) fn eval_builtin_gettype(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_gettype_result(value, values)
}

/// Converts one boxed runtime tag into PHP's `gettype()` spelling.
fn eval_gettype_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    values.string(eval_gettype_name(tag))
}

/// Evaluates PHP's `get_class(...)` over one eval object expression.
pub(super) fn eval_builtin_get_class(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let object = eval_expr(object, context, scope, values)?;
    eval_get_class_result(object, context, values)
}

/// Resolves the PHP-visible class name for one already materialized object cell.
fn eval_get_class_result(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Ok(identity) = values.object_identity(object) {
        if let Some(class) = context.dynamic_object_class(identity) {
            return values.string(class.name().trim_start_matches('\\'));
        }
    }
    values.object_class_name(object)
}

/// Evaluates PHP's SPL object identity builtins over one eval object expression.
pub(super) fn eval_builtin_spl_object_identity(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let object = eval_expr(object, context, scope, values)?;
    eval_spl_object_identity_result(name, object, values)
}

/// Returns the unboxed object-payload identity in the native SPL builtin spelling.
fn eval_spl_object_identity_result(
    name: &str,
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let identity = values.object_identity(object)? as i64;
    match name {
        "spl_object_id" => values.int(identity),
        "spl_object_hash" => values.string(&identity.to_string()),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP's `get_parent_class(...)` over one eval object or class-name expression.
pub(super) fn eval_builtin_get_parent_class(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object_or_class] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let object_or_class = eval_expr(object_or_class, context, scope, values)?;
    eval_get_parent_class_result(object_or_class, values)
}

/// Resolves the PHP-visible parent class name for one object or class-name cell.
fn eval_get_parent_class_result(
    object_or_class: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.parent_class_name(object_or_class)
}

/// Evaluates `get_resource_type(...)` and `get_resource_id(...)` over one eval value.
pub(super) fn eval_builtin_resource_introspection(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [resource] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let resource = eval_expr(resource, context, scope, values)?;
    eval_resource_introspection_result(name, resource, values)
}

/// Evaluates a materialized resource introspection builtin argument.
fn eval_resource_introspection_result(
    name: &str,
    resource: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(resource)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    match name {
        "get_resource_type" => values.string("stream"),
        "get_resource_id" => values.cast_int(resource),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Returns the PHP-visible type name for a concrete eval runtime tag.
fn eval_gettype_name(tag: u64) -> &'static str {
    match tag {
        EVAL_TAG_INT => "integer",
        EVAL_TAG_FLOAT => "double",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_BOOL => "boolean",
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_OBJECT => "object",
        EVAL_TAG_RESOURCE => "resource",
        EVAL_TAG_NULL => "NULL",
        _ => "NULL",
    }
}

/// Evaluates PHP scalar/container type predicate builtins over one eval expression.
pub(super) fn eval_builtin_type_predicate(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_type_predicate_result(name, value, values)
}

/// Converts a concrete runtime tag into a PHP `is_*` predicate result.
fn eval_type_predicate_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    let result = match name {
        "is_int" | "is_integer" | "is_long" => tag == EVAL_TAG_INT,
        "is_float" | "is_double" | "is_real" => tag == EVAL_TAG_FLOAT,
        "is_string" => tag == EVAL_TAG_STRING,
        "is_bool" => tag == EVAL_TAG_BOOL,
        "is_null" => tag == EVAL_TAG_NULL,
        "is_array" | "is_iterable" => matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC),
        "is_object" => tag == EVAL_TAG_OBJECT,
        "is_resource" => tag == EVAL_TAG_RESOURCE,
        "is_nan" => eval_float_value(value, values)?.is_nan(),
        "is_infinite" => eval_float_value(value, values)?.is_infinite(),
        "is_finite" => eval_float_value(value, values)?.is_finite(),
        "is_numeric" => {
            tag == EVAL_TAG_INT
                || tag == EVAL_TAG_FLOAT
                || (tag == EVAL_TAG_STRING && eval_is_numeric_string(&values.string_bytes(value)?))
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.bool_value(result)
}

/// Matches the static backend's legacy ASCII numeric-string scan.
fn eval_is_numeric_string(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    let mut index = 0;
    let mut consumed_digits = 0;
    if bytes[index] == b'-' {
        index += 1;
        if index >= bytes.len() {
            return false;
        }
    }

    while index < bytes.len() {
        if bytes[index] == b'.' {
            index += 1;
            break;
        }
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        consumed_digits += 1;
        index += 1;
    }

    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        consumed_digits += 1;
        index += 1;
    }

    consumed_digits > 0
}

/// Evaluates PHP's `hash_equals(...)` over two eval expressions.
pub(super) fn eval_builtin_hash_equals(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [known, user] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let known = eval_expr(known, context, scope, values)?;
    let user = eval_expr(user, context, scope, values)?;
    eval_hash_equals_result(known, user, values)
}

/// Compares two converted strings with PHP `hash_equals()` semantics.
fn eval_hash_equals_result(
    known: RuntimeCellHandle,
    user: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let known = values.string_bytes(known)?;
    let user = values.string_bytes(user)?;
    if known.len() != user.len() {
        return values.bool_value(false);
    }
    let mut diff = 0u8;
    for (known, user) in known.iter().zip(user.iter()) {
        diff |= known ^ user;
    }
    values.bool_value(diff == 0)
}

/// Evaluates PHP string comparison builtins over two eval expressions.
pub(super) fn eval_builtin_string_compare(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_string_compare_result(name, left, right, values)
}

/// Compares two converted strings and returns -1, 0, or 1.
fn eval_string_compare_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut left = values.string_bytes(left)?;
    let mut right = values.string_bytes(right)?;
    match name {
        "strcmp" => {}
        "strcasecmp" => {
            left.make_ascii_lowercase();
            right.make_ascii_lowercase();
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    let result = match left.cmp(&right) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    };
    values.int(result)
}

/// Evaluates PHP's byte-string search predicates over two eval expressions.
pub(super) fn eval_builtin_string_search(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [haystack, needle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let haystack = eval_expr(haystack, context, scope, values)?;
    let needle = eval_expr(needle, context, scope, values)?;
    eval_string_search_result(name, haystack, needle, values)
}

/// Checks one converted haystack for one converted needle using PHP byte-string semantics.
fn eval_string_search_result(
    name: &str,
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let matched = match name {
        "str_contains" => {
            needle.is_empty()
                || haystack
                    .windows(needle.len())
                    .any(|window| window == needle)
        }
        "str_starts_with" => haystack.starts_with(&needle),
        "str_ends_with" => haystack.ends_with(&needle),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.bool_value(matched)
}

/// Evaluates PHP byte-string position builtins over two eval expressions.
pub(super) fn eval_builtin_string_position(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [haystack, needle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let haystack = eval_expr(haystack, context, scope, values)?;
    let needle = eval_expr(needle, context, scope, values)?;
    eval_string_position_result(name, haystack, needle, values)
}

/// Returns the first or last byte offset of a converted needle, or PHP `false`.
fn eval_string_position_result(
    name: &str,
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let position = match name {
        "strpos" if needle.is_empty() => Some(0),
        "strpos" => haystack
            .windows(needle.len())
            .position(|window| window == needle),
        "strrpos" if needle.is_empty() => Some(haystack.len()),
        "strrpos" => haystack
            .windows(needle.len())
            .rposition(|window| window == needle),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    match position {
        Some(position) => {
            let position = i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(position)
        }
        None => values.bool_value(false),
    }
}

/// Evaluates PHP `strstr(...)` over haystack, needle, and optional prefix mode.
pub(super) fn eval_builtin_strstr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [haystack, needle] => {
            let haystack = eval_expr(haystack, context, scope, values)?;
            let needle = eval_expr(needle, context, scope, values)?;
            eval_strstr_result(haystack, needle, false, values)
        }
        [haystack, needle, before_needle] => {
            let haystack = eval_expr(haystack, context, scope, values)?;
            let needle = eval_expr(needle, context, scope, values)?;
            let before_needle = eval_expr(before_needle, context, scope, values)?;
            let before_needle = values.truthy(before_needle)?;
            eval_strstr_result(haystack, needle, before_needle, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the suffix or prefix selected by PHP `strstr()`, or `false` when absent.
fn eval_strstr_result(
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    before_needle: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let position = if needle.is_empty() {
        Some(0)
    } else {
        eval_find_subslice(&haystack, &needle, 0)
    };
    let Some(position) = position else {
        return values.bool_value(false);
    };
    let result = if before_needle {
        &haystack[..position]
    } else {
        &haystack[position..]
    };
    values.string_bytes_value(result)
}

const PHP_DEFAULT_TRIM_MASK: &[u8] = b" \n\r\t\x0B\x0C\0";

/// Evaluates PHP trim-like string builtins over one eval expression and optional mask.
pub(super) fn eval_builtin_trim_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_trim_like_result(name, value, None, values)
        }
        [value, mask] => {
            let value = eval_expr(value, context, scope, values)?;
            let mask = eval_expr(mask, context, scope, values)?;
            eval_trim_like_result(name, value, Some(mask), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Trims one converted string using PHP's default mask or a caller-provided byte mask.
fn eval_trim_like_result(
    name: &str,
    value: RuntimeCellHandle,
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let explicit_mask;
    let trim_mask = if let Some(mask) = mask {
        explicit_mask = values.string_bytes(mask)?;
        explicit_mask.as_slice()
    } else {
        PHP_DEFAULT_TRIM_MASK
    };

    let mut start = 0;
    let mut end = bytes.len();
    if matches!(name, "trim" | "ltrim") {
        while start < end && trim_mask.contains(&bytes[start]) {
            start += 1;
        }
    }
    if matches!(name, "trim" | "rtrim" | "chop") {
        while end > start && trim_mask.contains(&bytes[end - 1]) {
            end -= 1;
        }
    }
    if !matches!(name, "trim" | "ltrim" | "rtrim" | "chop") {
        return Err(EvalStatus::UnsupportedConstruct);
    }

    let value =
        String::from_utf8(bytes[start..end].to_vec()).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(&value)
}

/// Evaluates PHP ASCII case-conversion string builtins over one eval expression.
pub(super) fn eval_builtin_string_case(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_string_case_result(name, value, values)
}

/// Converts one eval value through PHP string conversion and ASCII case mapping.
fn eval_string_case_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    match name {
        "strtolower" => {
            for byte in &mut bytes {
                if byte.is_ascii_uppercase() {
                    *byte += b'a' - b'A';
                }
            }
        }
        "strtoupper" => {
            for byte in &mut bytes {
                if byte.is_ascii_lowercase() {
                    *byte -= b'a' - b'A';
                }
            }
        }
        "ucfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_lowercase()) {
                bytes[0] -= b'a' - b'A';
            }
        }
        "lcfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_uppercase()) {
                bytes[0] += b'a' - b'A';
            }
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    let value = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(&value)
}

/// Evaluates PHP `ucwords(...)` over one string and optional separator expression.
pub(super) fn eval_builtin_ucwords(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_ucwords_result(value, None, values)
        }
        [value, separators] => {
            let value = eval_expr(value, context, scope, values)?;
            let separators = eval_expr(separators, context, scope, values)?;
            eval_ucwords_result(value, Some(separators), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Uppercases ASCII lowercase bytes at the start of words separated by PHP delimiters.
fn eval_ucwords_result(
    value: RuntimeCellHandle,
    separators: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    let separators = match separators {
        Some(separators) => values.string_bytes(separators)?,
        None => b" \t\r\n\x0c\x0b".to_vec(),
    };
    let mut word_start = true;
    for byte in &mut bytes {
        if separators.contains(byte) {
            word_start = true;
        } else if word_start {
            if byte.is_ascii_lowercase() {
                *byte -= b'a' - b'A';
            }
            word_start = false;
        }
    }
    values.string_bytes_value(&bytes)
}

/// Evaluates PHP `wordwrap(...)` over one string and optional wrapping controls.
pub(super) fn eval_builtin_wordwrap(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_wordwrap_result(value, None, None, None, values)
        }
        [value, width] => {
            let value = eval_expr(value, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            eval_wordwrap_result(value, Some(width), None, None, values)
        }
        [value, width, break_string] => {
            let value = eval_expr(value, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            let break_string = eval_expr(break_string, context, scope, values)?;
            eval_wordwrap_result(value, Some(width), Some(break_string), None, values)
        }
        [value, width, break_string, cut] => {
            let value = eval_expr(value, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            let break_string = eval_expr(break_string, context, scope, values)?;
            let cut = eval_expr(cut, context, scope, values)?;
            eval_wordwrap_result(value, Some(width), Some(break_string), Some(cut), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Wraps a byte string at PHP word boundaries and preserves existing newlines.
fn eval_wordwrap_result(
    value: RuntimeCellHandle,
    width: Option<RuntimeCellHandle>,
    break_string: Option<RuntimeCellHandle>,
    cut: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let width = match width {
        Some(width) => eval_int_value(width, values)?,
        None => 75,
    };
    let break_string = match break_string {
        Some(break_string) => values.string_bytes(break_string)?,
        None => b"\n".to_vec(),
    };
    if break_string.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let cut = match cut {
        Some(cut) => values.truthy(cut)?,
        None => false,
    };
    if width == 0 && cut {
        return Err(EvalStatus::RuntimeFatal);
    }
    if bytes.is_empty() {
        return values.string_bytes_value(&bytes);
    }
    let output = eval_wordwrap_bytes(&bytes, width, &break_string, cut);
    values.string_bytes_value(&output)
}

/// Applies the core PHP word-wrap scan over already converted byte slices.
fn eval_wordwrap_bytes(bytes: &[u8], width: i64, break_string: &[u8], cut: bool) -> Vec<u8> {
    if width < 0 && cut {
        let mut output = Vec::with_capacity(bytes.len() + (bytes.len() * break_string.len()));
        for byte in bytes {
            output.extend_from_slice(break_string);
            output.push(*byte);
        }
        return output;
    }

    let width = width.max(0) as usize;
    let mut output = Vec::with_capacity(bytes.len());
    let mut line_start = 0;
    let mut last_space = None;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                output.extend_from_slice(&bytes[line_start..=index]);
                index += 1;
                line_start = index;
                last_space = None;
            }
            b' ' => {
                if index.saturating_sub(line_start) >= width {
                    output.extend_from_slice(&bytes[line_start..index]);
                    output.extend_from_slice(break_string);
                    index += 1;
                    line_start = index;
                    last_space = None;
                } else {
                    last_space = Some(index);
                    index += 1;
                }
            }
            _ if index.saturating_sub(line_start) >= width => {
                if let Some(space) = last_space {
                    output.extend_from_slice(&bytes[line_start..space]);
                    output.extend_from_slice(break_string);
                    line_start = space + 1;
                    last_space = None;
                } else if cut && width > 0 {
                    output.extend_from_slice(&bytes[line_start..index]);
                    output.extend_from_slice(break_string);
                    line_start = index;
                } else {
                    index += 1;
                }
            }
            _ => {
                index += 1;
            }
        }
    }
    output.extend_from_slice(&bytes[line_start..]);
    output
}
