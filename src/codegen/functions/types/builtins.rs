//! Purpose:
//! Computes builtin return and parameter types needed by code generation.
//! Keeps emission-time type decisions separate from instruction lowering.
//!
//! Called from:
//! - `crate::codegen::functions::types`
//!
//! Key details:
//! - Results must agree with `crate::types` so local slots and runtime value shapes are selected correctly.

use crate::codegen::builtins::callable_lookup::{lookup_function, FunctionLookup};
use crate::codegen::context::Context;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::{array_key_type_from_value_type, FunctionSig, PhpType};

use super::arrays::{array_like_key_type, array_like_value_type, indexed_array_value_type};
use super::{codegen_static_type, infer_local_type};
use super::union::merge_union_members;

/// Infers the PHP return type for a builtin function call based on the function name,
/// argument expressions, call signature, and optional codegen context.
///
/// Uses `infer_local_type` to determine the types of actual arguments where needed
/// (e.g., for `array_fill_keys`, `array_combine`, `abs`, `min`, `max`). Falls back
/// to a fixed return type per builtin when the argument types are not informative.
/// For unknown builtins, queries `ctx.functions` for user-defined functions;
/// otherwise defaults to `PhpType::Int`.
pub(super) fn infer_function_call_type(
    name: &str,
    args: &[Expr],
    sig: &FunctionSig,
    ctx: Option<&Context>,
) -> PhpType {
    match name {
        "strtolower" | "strtoupper" | "ucfirst" | "lcfirst" | "ucwords" | "trim"
        | "ltrim" | "rtrim" | "chop" | "substr" | "str_repeat" | "strrev" | "str_replace"
        | "str_ireplace" | "substr_replace" | "str_pad" | "chr" | "implode" | "join"
        | "sprintf" | "vsprintf" | "number_format" | "nl2br" | "wordwrap" | "addslashes"
        | "stripslashes" | "htmlspecialchars" | "html_entity_decode" | "htmlentities"
        | "urlencode" | "urldecode" | "rawurlencode" | "rawurldecode" | "base64_encode"
        | "base64_decode" | "bin2hex" | "hex2bin" | "md5" | "sha1" | "hash" | "hash_hmac"
        | "gettype"
        | "strstr" | "readline" | "date" | "gmdate"
        | "json_last_error_msg" | "php_uname" | "phpversion"
        | "date_default_timezone_get"
        | "tempnam" | "getcwd" | "shell_exec" | "preg_replace_callback"
        | "ptr_read_string"
        | "fread" | "fgets" | "stream_get_line"
        | "gethostname" | "gethostbyname"
        | "basename" | "sys_get_temp_dir"
        | "get_class" | "get_parent_class" | "get_resource_type"
        | "exec" | "system" | "preg_replace" => PhpType::Str,
        "json_decode" => PhpType::Mixed,
        // strtotime boxes int|false as a Mixed cell; the internal raw alias stays a plain int.
        "strtotime" => PhpType::Mixed,
        "__elephc_strtotime_raw" => PhpType::Int,
        "call_user_func" | "call_user_func_array" => {
            infer_dynamic_callback_builtin_type(args, ctx).unwrap_or(PhpType::Mixed)
        }
        "array_keys" => {
            let arr_ty = args
                .first()
                .map(|arg| infer_local_type(arg, sig, ctx))
                .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Int)));
            PhpType::Array(Box::new(array_like_key_type(&arr_ty)))
        }
        "array_values" => {
            let arr_ty = args
                .first()
                .map(|arg| infer_local_type(arg, sig, ctx))
                .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Int)));
            PhpType::Array(Box::new(array_like_value_type(&arr_ty)))
        }
        "array_combine" => {
            let key_ty = args
                .first()
                .map(|arg| infer_local_type(arg, sig, ctx))
                .map(|ty| indexed_array_value_type(&ty, PhpType::Str))
                .map(array_key_type_from_value_type)
                .unwrap_or(PhpType::Str);
            let value_ty = args
                .get(1)
                .map(|arg| infer_local_type(arg, sig, ctx))
                .map(|ty| indexed_array_value_type(&ty, PhpType::Int))
                .unwrap_or(PhpType::Int);
            PhpType::AssocArray {
                key: Box::new(key_ty),
                value: Box::new(value_ty),
            }
        }
        "array_fill_keys" => {
            let key_ty = args
                .first()
                .map(|arg| infer_local_type(arg, sig, ctx))
                .map(|ty| indexed_array_value_type(&ty, PhpType::Str))
                .map(array_key_type_from_value_type)
                .unwrap_or(PhpType::Str);
            let value_ty = args
                .get(1)
                .map(|arg| infer_local_type(arg, sig, ctx))
                .unwrap_or(PhpType::Int);
            PhpType::AssocArray {
                key: Box::new(key_ty),
                value: Box::new(value_ty),
            }
        }
        "array_flip" => {
            let arr_ty = args
                .first()
                .map(|arg| infer_local_type(arg, sig, ctx))
                .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Int)));
            match arr_ty {
                PhpType::Array(value) => PhpType::AssocArray {
                    key: Box::new(array_key_type_from_value_type(*value)),
                    value: Box::new(PhpType::Int),
                },
                PhpType::AssocArray { key, value } => PhpType::AssocArray {
                    key: Box::new(array_key_type_from_value_type(*value)),
                    value: key,
                },
                _ => PhpType::AssocArray {
                    key: Box::new(PhpType::Int),
                    value: Box::new(PhpType::Int),
                },
            }
        }
        "array_diff_key" | "array_intersect_key" => args
            .first()
            .map(|arg| infer_local_type(arg, sig, ctx))
            .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Int))),
        "array_merge" => infer_array_merge_type(args, sig, ctx),
        "array_fill" => {
            // Mirrors the emitter dispatch: a non-literal-zero start produces a keyed
            // Mixed-valued hash (`__rt_array_fill_assoc`); a literal-zero start uses the
            // indexed path (the dedicated `__rt_array_fill_str` for string values, or
            // `__rt_array_fill` / `__rt_array_fill_refcounted` for scalars).
            let value_ty = args
                .get(2)
                .map(|arg| infer_local_type(arg, sig, ctx))
                .unwrap_or(PhpType::Int);
            let start_is_literal_zero =
                matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::IntLiteral(0)));
            if !start_is_literal_zero {
                PhpType::AssocArray {
                    key: Box::new(PhpType::Int),
                    value: Box::new(PhpType::Mixed),
                }
            } else {
                PhpType::Array(Box::new(value_ty))
            }
        }
        "explode"
        | "str_split"
        | "file"
        | "scandir"
        | "glob"
        | "array_slice"
        | "array_reverse"
        | "array_unique"
        | "array_chunk"
        | "array_pad"
        | "array_diff"
        | "array_intersect"
        | "array_splice"
        | "array_column"
        | "array_map"
        | "array_filter"
        | "range"
        | "array_rand"
        | "sscanf"
        | "fscanf"
        | "fgetcsv"
        | "preg_split" => {
            if name == "preg_split" && args.len() >= 4 {
                PhpType::Array(Box::new(PhpType::Mixed))
            } else if name == "explode"
                || name == "str_split"
                || name == "file"
                || name == "scandir"
                || name == "glob"
                || name == "fgetcsv"
                || name == "fscanf"
                || name == "preg_split"
            {
                PhpType::Array(Box::new(PhpType::Str))
            } else if !args.is_empty() {
                let arr_ty = infer_local_type(&args[0], sig, ctx);
                match arr_ty {
                    PhpType::Array(t) => PhpType::Array(t),
                    _ => PhpType::Array(Box::new(PhpType::Int)),
                }
            } else {
                PhpType::Array(Box::new(PhpType::Int))
            }
        }
        "floatval" | "floor" | "ceil" | "round" | "sqrt" | "pow" | "fmod" | "fdiv"
        | "microtime" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2"
        | "sinh" | "cosh" | "tanh" | "log" | "log2" | "log10" | "exp" | "hypot" | "pi"
        | "deg2rad" | "rad2deg" => PhpType::Float,
        "is_callable" | "is_int" | "is_float" | "is_string" | "is_bool" | "is_null" | "is_numeric"
        | "is_nan" | "is_finite" | "is_infinite" | "is_array" | "empty" | "isset"
        | "is_file" | "is_dir" | "is_readable" | "is_writable" | "file_exists"
        | "in_array" | "array_key_exists" | "str_contains" | "str_starts_with"
        | "str_ends_with" | "ctype_alpha" | "ctype_digit" | "ctype_alnum"
        | "ctype_space" | "function_exists" | "defined" | "chmod" | "chown" | "chgrp"
        | "touch" | "ftruncate" | "fflush" | "fsync" | "fdatasync" | "ptr_is_null"
        | "json_validate" | "flock" | "symlink" | "link" | "feof" | "rewind"
        | "fclose" => {
            PhpType::Bool
        }
        "define" => PhpType::Bool,
        "date_default_timezone_set" => PhpType::Bool,
        "checkdate" => PhpType::Bool,
        "getdate" => PhpType::Mixed,
        "localtime" => PhpType::Mixed,
        "hrtime" => PhpType::Mixed,
        "umask" | "fpassthru" | "linkinfo" | "fprintf" | "vprintf" | "vfprintf"
        | "fseek" | "ftell" | "fwrite" | "fputcsv" => PhpType::Int,
        "strpos" | "strrpos" | "array_search" | "file_get_contents" | "json_encode"
        | "grapheme_strrev" | "fileatime" | "filectime" | "fileperms" | "fileowner"
        | "filegroup" | "fileinode" | "filetype" | "stat" | "lstat" | "fstat"
        | "fgetc" | "readfile" | "readlink" | "stream_get_contents"
        | "stream_copy_to_stream" => PhpType::Mixed,
        "fopen" | "tmpfile" => merge_union_members(vec![PhpType::stream_resource(), PhpType::Bool]),
        "pathinfo" => infer_pathinfo_type(args),
        "abs" => {
            if !args.is_empty() {
                let t = infer_local_type(&args[0], sig, ctx);
                if matches!(t, PhpType::Mixed | PhpType::Union(_)) {
                    // A boxed Mixed operand keeps its int/float tag at runtime, so the
                    // result type must stay Mixed to match the `__rt_abs_mixed` emitter.
                    PhpType::Mixed
                } else if t == PhpType::Float {
                    PhpType::Float
                } else {
                    PhpType::Int
                }
            } else {
                PhpType::Int
            }
        }
        "array_pop" | "array_shift" => {
            // mirror the emitters: int-element pops/shifts evaluate to a tagged scalar
            // under the tagged null representation (empty array -> tagged null)
            let elem_ty = match args.first().map(|arg| infer_local_type(arg, sig, ctx)) {
                Some(PhpType::Array(elem)) => *elem,
                _ => PhpType::Int,
            };
            if matches!(elem_ty, PhpType::Int)
                && crate::codegen::sentinels::null_repr_is_tagged()
            {
                PhpType::TaggedScalar
            } else {
                elem_ty
            }
        }
        "min" | "max" => {
            if args.len() >= 2 {
                let t0 = infer_local_type(&args[0], sig, ctx);
                let t1 = infer_local_type(&args[1], sig, ctx);
                if t0 == PhpType::Float || t1 == PhpType::Float {
                    PhpType::Float
                } else {
                    PhpType::Int
                }
            } else {
                PhpType::Int
            }
        }
        "clamp" => {
            if args.len() >= 3 {
                let arg_types: Vec<PhpType> = args
                    .iter()
                    .take(3)
                    .map(|arg| infer_local_type(arg, sig, ctx).codegen_repr())
                    .collect();
                if arg_types.iter().all(|ty| *ty == PhpType::Str) {
                    PhpType::Str
                } else if arg_types.iter().all(|ty| *ty == PhpType::Int) {
                    PhpType::Int
                } else if arg_types
                    .iter()
                    .all(|ty| matches!(ty, PhpType::Int | PhpType::Float))
                {
                    PhpType::Float
                } else {
                    PhpType::Mixed
                }
            } else {
                PhpType::Mixed
            }
        }
        "ptr" | "ptr_null" => PhpType::Pointer(None),
        "buffer_len" => PhpType::Int,
        "ptr_offset" => {
            if let Some(first_arg) = args.first() {
                match infer_local_type(first_arg, sig, ctx) {
                    PhpType::Pointer(tag) => PhpType::Pointer(tag),
                    _ => PhpType::Pointer(None),
                }
            } else {
                PhpType::Pointer(None)
            }
        }
        "ptr_get" | "ptr_read8" | "ptr_read16" | "ptr_read32" | "ptr_sizeof"
        | "ptr_write_string" => PhpType::Int,
        "ptr_set" | "ptr_write8" | "ptr_write16" | "ptr_write32" => PhpType::Void,
        "class_attribute_names" => PhpType::Array(Box::new(PhpType::Str)),
        "class_attribute_args" => PhpType::Array(Box::new(PhpType::Mixed)),
        "class_get_attributes" => PhpType::Array(Box::new(PhpType::Object(
            "ReflectionAttribute".to_string(),
        ))),
        "class_implements" | "class_parents" | "class_uses" => {
            merge_union_members(vec![
                PhpType::AssocArray {
                    key: Box::new(PhpType::Str),
                    value: Box::new(PhpType::Str),
                },
                PhpType::Bool,
            ])
        }
        "iterator_count" | "iterator_apply" => PhpType::Int,
        "iterator_to_array" => {
            let source_ty = args
                .first()
                .map(|arg| infer_local_type(arg, sig, ctx))
                .unwrap_or(PhpType::Iterable);
            if let Some(preserve_keys) = args
                .get(1)
                .and_then(iterator_to_array_static_preserve_keys)
                .or_else(|| args.get(1).is_none().then_some(true))
            {
                iterator_to_array_static_result_type(source_ty, preserve_keys)
            } else if matches!(source_ty, PhpType::Array(_)) {
                iterator_to_array_static_result_type(source_ty, true)
            } else {
                merge_union_members(vec![
                    iterator_to_array_static_result_type(source_ty.clone(), true),
                    iterator_to_array_static_result_type(source_ty, false),
                ])
            }
        }
        "stream_bucket_new" | "stream_bucket_make_writeable" => PhpType::Mixed,
        "stream_resolve_include_path" => PhpType::Mixed,
        _ => {
            if let Some(c) = ctx {
                if let Some(fn_sig) = c.functions.get(name) {
                    return fn_sig.return_type.clone();
                }
            }
            PhpType::Int
        }
    }
}

/// Infers the result type for `array_merge()` during legacy codegen.
///
/// A statically empty left operand has `Array<Void>` and should not force later indexed
/// reads of the merged result to materialize null/void values when the right operand
/// supplies concrete elements.
fn infer_array_merge_type(args: &[Expr], sig: &FunctionSig, ctx: Option<&Context>) -> PhpType {
    let first = args
        .first()
        .map(|arg| infer_local_type(arg, sig, ctx))
        .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Int)));
    match first {
        PhpType::Array(elem) if is_empty_array_element_type(elem.as_ref()) => args
            .get(1)
            .map(|arg| infer_local_type(arg, sig, ctx))
            .and_then(|ty| match ty {
                PhpType::Array(elem) if is_scalar_merge_element_type(elem.as_ref()) => {
                    Some(PhpType::Array(elem))
                }
                _ => None,
            })
            .unwrap_or_else(|| PhpType::Array(elem)),
        PhpType::Array(elem) => PhpType::Array(elem),
        _ => PhpType::Array(Box::new(PhpType::Int)),
    }
}

/// Returns true for the element sentinel used by statically empty indexed arrays.
fn is_empty_array_element_type(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Void)
}

/// Returns true for element types copied safely by the scalar merge runtime helper.
fn is_scalar_merge_element_type(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Callable | PhpType::Void
    )
}

/// Infers the result type for dynamic callback builtins when the callback has static metadata.
fn infer_dynamic_callback_builtin_type(args: &[Expr], ctx: Option<&Context>) -> Option<PhpType> {
    let callback = args.first()?;
    let ctx = ctx?;
    infer_callback_expr_return_type(callback, ctx)
}

/// Infers the return type of a callable expression used by `call_user_func*`.
fn infer_callback_expr_return_type(callback: &Expr, ctx: &Context) -> Option<PhpType> {
    if let Some(sig) = crate::codegen::callables::callable_sig(callback, ctx) {
        return Some(sig.return_type);
    }

    match &callback.kind {
        ExprKind::StringLiteral(name) => infer_string_callback_return_type(name, ctx),
        ExprKind::Closure {
            return_type: Some(type_ann),
            ..
        } => Some(codegen_static_type(type_ann, ctx)),
        ExprKind::Closure { body, .. } => {
            Some(crate::types::checker::infer_return_type_syntactic(body))
        }
        ExprKind::Assignment { value, .. } => infer_callback_expr_return_type(value, ctx),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => matching_callback_branch_return_type(then_expr, else_expr, ctx),
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => {
            matching_callback_branch_return_type(value, default, ctx)
        }
        _ => None,
    }
}

/// Infers the return type of a string-named callback when it resolves at compile time.
fn infer_string_callback_return_type(name: &str, ctx: &Context) -> Option<PhpType> {
    match lookup_function(ctx, name)? {
        FunctionLookup::UserFunction(name)
        | FunctionLookup::IncludeVariant(name)
        | FunctionLookup::Extern(name) => ctx
            .functions
            .get(&name)
            .map(|sig| sig.return_type.clone()),
        FunctionLookup::Builtin(name) => {
            crate::types::first_class_callable_builtin_sig(&name).map(|sig| sig.return_type)
        }
    }
}

/// Infers a branch result only when both possible callbacks return the same type.
fn matching_callback_branch_return_type(
    left: &Expr,
    right: &Expr,
    ctx: &Context,
) -> Option<PhpType> {
    let left_ty = infer_callback_expr_return_type(left, ctx)?;
    let right_ty = infer_callback_expr_return_type(right, ctx)?;
    if left_ty == right_ty {
        Some(left_ty)
    } else {
        None
    }
}

/// Provides the Iterator to array static preserve keys helper used by the builtins module.
fn iterator_to_array_static_preserve_keys(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::IntLiteral(value) => Some(*value != 0),
        ExprKind::FloatLiteral(value) => Some(*value != 0.0),
        ExprKind::StringLiteral(value) => Some(!value.is_empty() && value != "0"),
        ExprKind::Null => Some(false),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => Some(*value != 0),
            ExprKind::FloatLiteral(value) => Some(*value != 0.0),
            _ => None,
        },
        _ => None,
    }
}

/// Computes the type metadata for iterator to array static result.
fn iterator_to_array_static_result_type(source_ty: PhpType, preserve_keys: bool) -> PhpType {
    match source_ty {
        PhpType::Array(elem_ty) => PhpType::Array(elem_ty),
        PhpType::AssocArray { key, value } if preserve_keys => PhpType::AssocArray { key, value },
        PhpType::AssocArray { value, .. } => PhpType::Array(value),
        _ if preserve_keys => PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
        _ => PhpType::Array(Box::new(PhpType::Mixed)),
    }
}

/// Infers the return type for `pathinfo()` based on its optional second argument (the
/// `PATHINFO_*` constant). Returns a string for `PATHINFO_EXTENSION`, an associative
/// array of strings for `PATHINFO_DIRNAME`/`PATHINFO_BASENAME`/`PATHINFO_FILENAME`,
/// and `PhpType::Mixed` when no flag is present or the flag is not statically resolvable.
fn infer_pathinfo_type(args: &[Expr]) -> PhpType {
    match args.get(1).and_then(pathinfo_static_flag_value) {
        None if args.len() == 1 => PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Str),
        },
        Some(15) => PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Str),
        },
        Some(_) => PhpType::Str,
        None => PhpType::Mixed,
    }
}

/// Statically evaluates the given expression as a `PATHINFO_*` constant bitmask,
/// handling integer literals, constant references (`PATHINFO_DIRNAME`, etc.),
/// negation, and bitwise AND/OR/XOR combinations thereof. Returns `None` if the
/// expression cannot be resolved to a constant at compile time.
fn pathinfo_static_flag_value(flag: &Expr) -> Option<i64> {
    match &flag.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => match name.as_str() {
            "PATHINFO_DIRNAME" => Some(1),
            "PATHINFO_BASENAME" => Some(2),
            "PATHINFO_EXTENSION" => Some(4),
            "PATHINFO_FILENAME" => Some(8),
            "PATHINFO_ALL" => Some(15),
            _ => None,
        },
        ExprKind::Negate(inner) => pathinfo_static_flag_value(inner).map(|value| -value),
        ExprKind::BinaryOp { left, op, right } => {
            let left = pathinfo_static_flag_value(left)?;
            let right = pathinfo_static_flag_value(right)?;
            match op {
                BinOp::BitAnd => Some(left & right),
                BinOp::BitOr => Some(left | right),
                BinOp::BitXor => Some(left ^ right),
                _ => None,
            }
        }
        _ => None,
    }
}
