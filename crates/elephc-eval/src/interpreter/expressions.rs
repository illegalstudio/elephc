//! Purpose:
//! Evaluates EvalIR expressions, match expressions, function-like calls, and positional builtin dispatch.
//!
//! Called from:
//! - `crate::interpreter::statements` for expression statements and expression-bearing statements.
//! - Eval builtin modules when they need to evaluate unevaluated argument expressions.
//!
//! Key details:
//! - PHP call argument evaluation order is preserved before binding or ABI-like materialization.
//! - Language constructs such as `eval`, `isset`, and `empty` receive unevaluated expressions.

use super::*;

/// Evaluates one expression to an opaque runtime-cell handle.
pub(in crate::interpreter) fn eval_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match expr {
        EvalExpr::Array(elements) => {
            if elements
                .iter()
                .any(|element| matches!(element, EvalArrayElement::KeyValue { .. }))
            {
                eval_assoc_array(elements, context, scope, values)
            } else {
                eval_indexed_array(elements, context, scope, values)
            }
        }
        EvalExpr::ArrayGet { array, index } => {
            let array = eval_expr(array, context, scope, values)?;
            let index = eval_expr(index, context, scope, values)?;
            values.array_get(array, index)
        }
        EvalExpr::Call { name, args } => eval_call(name, args, context, scope, values),
        EvalExpr::Const(value) => eval_const(value, values),
        EvalExpr::ConstFetch(name) => eval_const_fetch(name, context, values),
        EvalExpr::DynamicCall { callee, args } => {
            eval_dynamic_call(callee, args, context, scope, values)
        }
        EvalExpr::Include {
            path,
            required,
            once,
        } => eval_include_expr(path, *required, *once, context, scope, values),
        EvalExpr::LoadVar(name) => {
            visible_scope_cell(context, scope, name).map_or_else(|| values.null(), Ok)
        }
        EvalExpr::Magic(magic) => eval_magic_const(magic, context, values),
        EvalExpr::Match {
            subject,
            arms,
            default,
        } => eval_match_expr(subject, arms, default.as_deref(), context, scope, values),
        EvalExpr::NamespacedCall {
            name,
            fallback_name,
            args,
        } => eval_namespaced_call(name, fallback_name, args, context, scope, values),
        EvalExpr::NamespacedConstFetch {
            name,
            fallback_name,
        } => eval_namespaced_const_fetch(name, fallback_name, context, values),
        EvalExpr::NewObject { class_name, args } => {
            let args = eval_method_call_arg_values(args, context, scope, values)?;
            if let Some(class) = context.class(class_name).cloned() {
                eval_dynamic_class_new_object(&class, args, context, scope, values)
            } else {
                values
                    .new_object(class_name)
                    .and_then(|object| values.construct_object(object, args).map(|()| object))
            }
        }
        EvalExpr::MethodCall {
            object,
            method,
            args,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            eval_method_call_result(object, method, evaluated_args, context, values)
        }
        EvalExpr::NullCoalesce { value, default } => {
            let value = eval_expr(value, context, scope, values)?;
            if values.is_null(value)? {
                eval_expr(default, context, scope, values)
            } else {
                Ok(value)
            }
        }
        EvalExpr::PropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            values.property_get(object, property)
        }
        EvalExpr::Print(inner) => {
            let value = eval_expr(inner, context, scope, values)?;
            values.echo(value)?;
            values.int(1)
        }
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = eval_expr(condition, context, scope, values)?;
            if values.truthy(condition)? {
                if let Some(then_branch) = then_branch {
                    eval_expr(then_branch, context, scope, values)
                } else {
                    Ok(condition)
                }
            } else {
                eval_expr(else_branch, context, scope, values)
            }
        }
        EvalExpr::Unary { op, expr } => {
            let value = eval_expr(expr, context, scope, values)?;
            match op {
                EvalUnaryOp::Plus => {
                    let zero = values.int(0)?;
                    values.add(zero, value)
                }
                EvalUnaryOp::Negate => {
                    let zero = values.int(0)?;
                    values.sub(zero, value)
                }
                EvalUnaryOp::LogicalNot => {
                    let truthy = values.truthy(value)?;
                    values.bool_value(!truthy)
                }
                EvalUnaryOp::BitNot => values.bit_not(value),
            }
        }
        EvalExpr::Binary { op, left, right } => {
            if *op == EvalBinOp::LogicalAnd {
                let left = eval_expr(left, context, scope, values)?;
                if !values.truthy(left)? {
                    return values.bool_value(false);
                }
                let right = eval_expr(right, context, scope, values)?;
                let truthy = values.truthy(right)?;
                return values.bool_value(truthy);
            }
            if *op == EvalBinOp::LogicalOr {
                let left = eval_expr(left, context, scope, values)?;
                if values.truthy(left)? {
                    return values.bool_value(true);
                }
                let right = eval_expr(right, context, scope, values)?;
                let truthy = values.truthy(right)?;
                return values.bool_value(truthy);
            }
            let left = eval_expr(left, context, scope, values)?;
            let right = eval_expr(right, context, scope, values)?;
            match op {
                EvalBinOp::Add => values.add(left, right),
                EvalBinOp::Sub => values.sub(left, right),
                EvalBinOp::Mul => values.mul(left, right),
                EvalBinOp::Div => values.div(left, right),
                EvalBinOp::Mod => values.modulo(left, right),
                EvalBinOp::Pow => values.pow(left, right),
                EvalBinOp::BitAnd
                | EvalBinOp::BitOr
                | EvalBinOp::BitXor
                | EvalBinOp::ShiftLeft
                | EvalBinOp::ShiftRight => values.bitwise(*op, left, right),
                EvalBinOp::Concat => values.concat(left, right),
                EvalBinOp::LogicalXor => {
                    let left_truthy = values.truthy(left)?;
                    let right_truthy = values.truthy(right)?;
                    values.bool_value(left_truthy ^ right_truthy)
                }
                EvalBinOp::LooseEq
                | EvalBinOp::LooseNotEq
                | EvalBinOp::StrictEq
                | EvalBinOp::StrictNotEq
                | EvalBinOp::Lt
                | EvalBinOp::LtEq
                | EvalBinOp::Gt
                | EvalBinOp::GtEq => values.compare(*op, left, right),
                EvalBinOp::Spaceship => values.spaceship(left, right),
                EvalBinOp::LogicalAnd | EvalBinOp::LogicalOr => {
                    Err(EvalStatus::UnsupportedConstruct)
                }
            }
        }
    }
}

/// Evaluates a PHP `match` expression with strict comparison and lazy arm values.
pub(in crate::interpreter) fn eval_match_expr(
    subject: &EvalExpr,
    arms: &[EvalMatchArm],
    default: Option<&EvalExpr>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let subject = eval_expr(subject, context, scope, values)?;
    for arm in arms {
        for pattern in &arm.patterns {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let matched = values.compare(EvalBinOp::StrictEq, subject, pattern)?;
            if values.truthy(matched)? {
                return eval_expr(&arm.value, context, scope, values);
            }
        }
    }
    default
        .map(|expr| eval_expr(expr, context, scope, values))
        .unwrap_or(Err(EvalStatus::RuntimeFatal))
}

/// Returns cloned positional argument expressions, rejecting named arguments.
pub(in crate::interpreter) fn positional_call_arg_exprs(
    args: &[EvalCallArg],
) -> Result<Vec<EvalExpr>, EvalStatus> {
    if args
        .iter()
        .any(|arg| arg.name().is_some() || arg.is_spread())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(args.iter().map(|arg| arg.value().clone()).collect())
}

/// Evaluates a positional-only call argument list in source order.
pub(in crate::interpreter) fn eval_positional_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if args
        .iter()
        .any(|arg| arg.name().is_some() || arg.is_spread())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg.value(), context, scope, values)?);
    }
    Ok(evaluated_args)
}

/// Evaluates method-call arguments, allowing numeric spread but not named args.
pub(in crate::interpreter) fn eval_method_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    if evaluated_args.iter().any(|arg| arg.name.is_some()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(evaluated_args.into_iter().map(|arg| arg.value).collect())
}

/// Evaluates supported function-like calls from a runtime eval fragment.
pub(in crate::interpreter) fn eval_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_expr_language_construct_name(name) {
        let args = positional_call_arg_exprs(args)?;
        return eval_positional_expr_call(name, &args, context, scope, values);
    }
    if matches!(
        name,
        "array_pop"
            | "array_push"
            | "array_shift"
            | "array_splice"
            | "array_unshift"
            | "arsort"
            | "asort"
            | "krsort"
            | "ksort"
            | "natcasesort"
            | "natsort"
            | "rsort"
            | "shuffle"
            | "sort"
            | "settype"
            | "uasort"
            | "uksort"
            | "usort"
    ) {
        return eval_builtin_array_pop_shift_call(name, args, context, scope, values);
    }
    if eval_php_visible_builtin_exists(name) {
        if eval_call_args_are_plain_positional(args) {
            let args = positional_call_arg_exprs(args)?;
            return eval_positional_expr_call(name, &args, context, scope, values);
        }
        return eval_builtin_call(name, args, context, scope, values);
    }

    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function(function, args, context, scope, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Evaluates an unqualified namespaced function call with PHP's global fallback.
pub(in crate::interpreter) fn eval_namespaced_call(
    name: &str,
    fallback_name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function(function, args, context, scope, values);
    }
    eval_call(fallback_name, args, context, scope, values)
}

/// Evaluates a variable or expression callable and dispatches it with source-order arguments.
pub(in crate::interpreter) fn eval_dynamic_call(
    callee: &EvalExpr,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_expr(callee, context, scope, values)?;
    let callback = eval_callable(callback, values)?;
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    eval_evaluated_callable_with_call_array_args(&callback, evaluated_args, context, values)
}

/// Returns true for language constructs that need unevaluated argument expressions.
pub(in crate::interpreter) fn eval_expr_language_construct_name(name: &str) -> bool {
    matches!(name, "empty" | "eval" | "isset")
}

/// Returns true when every source argument is plain positional.
pub(in crate::interpreter) fn eval_call_args_are_plain_positional(args: &[EvalCallArg]) -> bool {
    args.iter()
        .all(|arg| arg.name().is_none() && !arg.is_spread())
}

/// Evaluates builtins and language constructs after positional-only argument validation.
pub(in crate::interpreter) fn eval_positional_expr_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "abs" => eval_builtin_abs(args, context, scope, values),
        "addslashes" | "stripslashes" => eval_builtin_slashes(name, args, context, scope, values),
        "array_combine" => eval_builtin_array_combine(args, context, scope, values),
        "array_chunk" => eval_builtin_array_chunk(args, context, scope, values),
        "array_column" => eval_builtin_array_column(args, context, scope, values),
        "array_fill" => eval_builtin_array_fill(args, context, scope, values),
        "array_fill_keys" => eval_builtin_array_fill_keys(args, context, scope, values),
        "array_filter" => eval_builtin_array_filter(args, context, scope, values),
        "array_flip" => eval_builtin_array_flip(args, context, scope, values),
        "array_map" => eval_builtin_array_map(args, context, scope, values),
        "array_reduce" => eval_builtin_array_reduce(args, context, scope, values),
        "array_walk" => eval_builtin_array_walk(args, context, scope, values),
        "array_keys" | "array_values" => {
            eval_builtin_array_projection(name, args, context, scope, values)
        }
        "array_key_exists" => eval_builtin_array_key_exists(args, context, scope, values),
        "array_diff" | "array_intersect" => {
            eval_builtin_array_value_set(name, args, context, scope, values)
        }
        "array_diff_key" | "array_intersect_key" => {
            eval_builtin_array_key_set(name, args, context, scope, values)
        }
        "array_merge" => eval_builtin_array_merge(args, context, scope, values),
        "array_product" | "array_sum" => {
            eval_builtin_array_aggregate(name, args, context, scope, values)
        }
        "array_pad" => eval_builtin_array_pad(args, context, scope, values),
        "array_rand" => eval_builtin_array_rand(args, context, scope, values),
        "array_reverse" => eval_builtin_array_reverse(args, context, scope, values),
        "array_search" | "in_array" => {
            eval_builtin_array_search(name, args, context, scope, values)
        }
        "array_slice" => eval_builtin_array_slice(args, context, scope, values),
        "array_unique" => eval_builtin_array_unique(args, context, scope, values),
        "acos" | "asin" | "atan" | "cos" | "cosh" | "deg2rad" | "exp" | "log2" | "log10"
        | "rad2deg" | "sin" | "sinh" | "tan" | "tanh" => {
            eval_builtin_float_unary(name, args, context, scope, values)
        }
        "atan2" | "hypot" => eval_builtin_float_pair(name, args, context, scope, values),
        "base64_encode" => eval_builtin_base64_encode(args, context, scope, values),
        "base64_decode" => eval_builtin_base64_decode(args, context, scope, values),
        "basename" => eval_builtin_basename(args, context, scope, values),
        "bin2hex" => eval_builtin_bin2hex(args, context, scope, values),
        "ceil" => eval_builtin_ceil(args, context, scope, values),
        "chdir" | "mkdir" | "rmdir" => {
            eval_builtin_unary_path_bool(name, args, context, scope, values)
        }
        "chmod" => eval_builtin_chmod(args, context, scope, values),
        "chr" => eval_builtin_chr(args, context, scope, values),
        "clamp" => eval_builtin_clamp(args, context, scope, values),
        "clearstatcache" => eval_builtin_clearstatcache(args, context, scope, values),
        "call_user_func" => eval_builtin_call_user_func(args, context, scope, values),
        "call_user_func_array" => eval_builtin_call_user_func_array(args, context, scope, values),
        "class_exists" => eval_builtin_class_exists(args, context, scope, values),
        "interface_exists" => eval_builtin_interface_exists(args, context, scope, values),
        "trait_exists" | "enum_exists" => {
            eval_builtin_class_like_exists(name, args, context, scope, values)
        }
        "is_a" | "is_subclass_of" => eval_builtin_is_a_relation(name, args, context, scope, values),
        "chop" => eval_builtin_trim_like(name, args, context, scope, values),
        "boolval" | "floatval" | "intval" | "strval" => {
            eval_builtin_cast(name, args, context, scope, values)
        }
        "count" => eval_builtin_count(args, context, scope, values),
        "copy" | "link" | "rename" | "symlink" => {
            eval_builtin_binary_path_bool(name, args, context, scope, values)
        }
        "crc32" => eval_builtin_crc32(args, context, scope, values),
        "ctype_alnum" | "ctype_alpha" | "ctype_digit" | "ctype_space" => {
            eval_builtin_ctype(name, args, context, scope, values)
        }
        "date" => eval_builtin_date(args, context, scope, values),
        "define" => eval_builtin_define(args, context, scope, values),
        "defined" => eval_builtin_defined(args, context, scope, values),
        "dirname" => eval_builtin_dirname(args, context, scope, values),
        "disk_free_space" | "disk_total_space" => {
            eval_builtin_disk_space(name, args, context, scope, values)
        }
        "empty" => eval_builtin_empty(args, context, scope, values),
        "exec" | "shell_exec" | "system" | "passthru" => {
            eval_builtin_process_command(name, args, context, scope, values)
        }
        "eval" => eval_nested_eval(args, context, scope, values),
        "explode" => eval_builtin_explode(args, context, scope, values),
        "fdiv" | "fmod" => eval_builtin_float_binary(name, args, context, scope, values),
        "file" => eval_builtin_file(args, context, scope, values),
        "file_exists" => eval_builtin_file_probe(name, args, context, scope, values),
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => eval_builtin_file_stat_scalar(name, args, context, scope, values),
        "file_get_contents" => eval_builtin_file_get_contents(args, context, scope, values),
        "file_put_contents" => eval_builtin_file_put_contents(args, context, scope, values),
        "filesize" => eval_builtin_filesize(args, context, scope, values),
        "filetype" => eval_builtin_filetype(args, context, scope, values),
        "fnmatch" => eval_builtin_fnmatch(args, context, scope, values),
        "stat" | "lstat" => eval_builtin_stat_array(name, args, context, scope, values),
        "floor" => eval_builtin_floor(args, context, scope, values),
        "function_exists" | "is_callable" => {
            eval_builtin_function_probe(args, context, scope, values)
        }
        "gethostbyaddr" => eval_builtin_gethostbyaddr(args, context, scope, values),
        "gethostbyname" => eval_builtin_gethostbyname(args, context, scope, values),
        "gethostname" => eval_builtin_gethostname(args, values),
        "getprotobyname" => eval_builtin_getprotobyname(args, context, scope, values),
        "getprotobynumber" => eval_builtin_getprotobynumber(args, context, scope, values),
        "getservbyname" => eval_builtin_getservbyname(args, context, scope, values),
        "getservbyport" => eval_builtin_getservbyport(args, context, scope, values),
        "get_class" => eval_builtin_get_class(args, context, scope, values),
        "get_parent_class" => eval_builtin_get_parent_class(args, context, scope, values),
        "get_resource_id" | "get_resource_type" => {
            eval_builtin_resource_introspection(name, args, context, scope, values)
        }
        "getcwd" => eval_builtin_getcwd(args, values),
        "getenv" => eval_builtin_getenv(args, context, scope, values),
        "gettype" => eval_builtin_gettype(args, context, scope, values),
        "glob" => eval_builtin_glob(args, context, scope, values),
        "hash" | "hash_file" | "hash_hmac" | "md5" | "sha1" => {
            eval_builtin_hash_one_shot(name, args, context, scope, values)
        }
        "chown" | "chgrp" | "lchown" | "lchgrp" => {
            eval_builtin_chown_like(name, args, context, scope, values)
        }
        "hash_algos" => eval_builtin_hash_algos(args, values),
        "hash_equals" => eval_builtin_hash_equals(args, context, scope, values),
        "hex2bin" => eval_builtin_hex2bin(args, context, scope, values),
        "html_entity_decode" | "htmlentities" | "htmlspecialchars" => {
            eval_builtin_html_entity(name, args, context, scope, values)
        }
        "implode" => eval_builtin_implode(args, context, scope, values),
        "inet_ntop" => eval_builtin_inet_ntop(args, context, scope, values),
        "inet_pton" => eval_builtin_inet_pton(args, context, scope, values),
        "intdiv" => eval_builtin_intdiv(args, context, scope, values),
        "iterator_apply" => eval_builtin_iterator_apply(args, context, scope, values),
        "iterator_count" => eval_builtin_iterator_count(args, context, scope, values),
        "iterator_to_array" => eval_builtin_iterator_to_array(args, context, scope, values),
        "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable" | "is_writable"
        | "is_writeable" => eval_builtin_file_probe(name, args, context, scope, values),
        "is_array" | "is_bool" | "is_double" | "is_finite" | "is_float" | "is_infinite"
        | "is_int" | "is_integer" | "is_iterable" | "is_long" | "is_nan" | "is_null"
        | "is_numeric" | "is_object" | "is_real" | "is_resource" | "is_string" => {
            eval_builtin_type_predicate(name, args, context, scope, values)
        }
        "ip2long" => eval_builtin_ip2long(args, context, scope, values),
        "json_decode" => eval_builtin_json_decode(args, context, scope, values),
        "json_encode" => eval_builtin_json_encode(args, context, scope, values),
        "json_last_error" => eval_builtin_json_last_error(args, context, values),
        "json_last_error_msg" => eval_builtin_json_last_error_msg(args, context, values),
        "json_validate" => eval_builtin_json_validate(args, context, scope, values),
        "linkinfo" => eval_builtin_linkinfo(args, context, scope, values),
        "ltrim" | "rtrim" => eval_builtin_trim_like(name, args, context, scope, values),
        "log" => eval_builtin_log(args, context, scope, values),
        "max" | "min" => eval_builtin_min_max(name, args, context, scope, values),
        "microtime" => eval_builtin_microtime(args, context, scope, values),
        "mktime" => eval_builtin_mktime(args, context, scope, values),
        "nl2br" => eval_builtin_nl2br(args, context, scope, values),
        "number_format" => eval_builtin_number_format(args, context, scope, values),
        "ord" => eval_builtin_ord(args, context, scope, values),
        "pathinfo" => eval_builtin_pathinfo(args, context, scope, values),
        "pi" => eval_builtin_pi(args, values),
        "php_uname" => eval_builtin_php_uname(args, context, scope, values),
        "phpversion" => eval_builtin_phpversion(args, values),
        "pow" => eval_builtin_pow(args, context, scope, values),
        "preg_match" => eval_builtin_preg_match(args, context, scope, values),
        "preg_match_all" => eval_builtin_preg_match_all(args, context, scope, values),
        "preg_replace" => eval_builtin_preg_replace(args, context, scope, values),
        "preg_replace_callback" => eval_builtin_preg_replace_callback(args, context, scope, values),
        "preg_split" => eval_builtin_preg_split(args, context, scope, values),
        "print_r" => eval_builtin_print_r(args, context, scope, values),
        "putenv" => eval_builtin_putenv(args, context, scope, values),
        "rand" | "mt_rand" => eval_builtin_rand(args, context, scope, values),
        "random_int" => eval_builtin_random_int(args, context, scope, values),
        "range" => eval_builtin_range(args, context, scope, values),
        "rawurldecode" | "urldecode" => eval_builtin_url_decode(name, args, context, scope, values),
        "rawurlencode" | "urlencode" => eval_builtin_url_encode(name, args, context, scope, values),
        "readfile" => eval_builtin_readfile(args, context, scope, values),
        "readlink" => eval_builtin_readlink(args, context, scope, values),
        "realpath" => eval_builtin_realpath(args, context, scope, values),
        "realpath_cache_get" => eval_builtin_realpath_cache_get(args, values),
        "realpath_cache_size" => eval_builtin_realpath_cache_size(args, values),
        "round" => eval_builtin_round(args, context, scope, values),
        "scandir" => eval_builtin_scandir(args, context, scope, values),
        "isset" => eval_builtin_isset(args, context, scope, values),
        "sleep" => eval_builtin_sleep(args, context, scope, values),
        "sqrt" => eval_builtin_sqrt(args, context, scope, values),
        "spl_classes" => eval_builtin_spl_classes(args, values),
        "spl_object_id" | "spl_object_hash" => {
            eval_builtin_spl_object_identity(name, args, context, scope, values)
        }
        "sscanf" => eval_builtin_sscanf(args, context, scope, values),
        "sprintf" | "printf" => eval_builtin_sprintf_like(name, args, context, scope, values),
        "sys_get_temp_dir" => eval_builtin_sys_get_temp_dir(args, values),
        "tempnam" => eval_builtin_tempnam(args, context, scope, values),
        "time" => eval_builtin_time(args, values),
        "touch" => eval_builtin_touch(args, context, scope, values),
        "stream_get_filters" | "stream_get_transports" | "stream_get_wrappers" => {
            eval_builtin_stream_introspection(name, args, values)
        }
        "strtotime" => eval_builtin_strtotime(args, context, scope, values),
        "unlink" => eval_builtin_unlink(args, context, scope, values),
        "strrev" => eval_builtin_strrev(args, context, scope, values),
        "str_repeat" => eval_builtin_str_repeat(args, context, scope, values),
        "str_replace" | "str_ireplace" => {
            eval_builtin_str_replace(name, args, context, scope, values)
        }
        "str_pad" => eval_builtin_str_pad(args, context, scope, values),
        "str_split" => eval_builtin_str_split(args, context, scope, values),
        "strstr" => eval_builtin_strstr(args, context, scope, values),
        "substr" => eval_builtin_substr(args, context, scope, values),
        "substr_replace" => eval_builtin_substr_replace(args, context, scope, values),
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            eval_builtin_string_search(name, args, context, scope, values)
        }
        "strcmp" | "strcasecmp" => eval_builtin_string_compare(name, args, context, scope, values),
        "strlen" => eval_builtin_strlen(args, context, scope, values),
        "strpos" | "strrpos" => eval_builtin_string_position(name, args, context, scope, values),
        "lcfirst" | "strtolower" | "strtoupper" | "ucfirst" => {
            eval_builtin_string_case(name, args, context, scope, values)
        }
        "long2ip" => eval_builtin_long2ip(args, context, scope, values),
        "trim" => eval_builtin_trim_like(name, args, context, scope, values),
        "ucwords" => eval_builtin_ucwords(args, context, scope, values),
        "umask" => eval_builtin_umask(args, context, scope, values),
        "usleep" => eval_builtin_usleep(args, context, scope, values),
        "var_dump" => eval_builtin_var_dump(args, context, scope, values),
        "vsprintf" | "vprintf" => eval_builtin_vsprintf_like(name, args, context, scope, values),
        "wordwrap" => eval_builtin_wordwrap(args, context, scope, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}
