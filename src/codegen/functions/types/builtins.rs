//! Purpose:
//! Computes builtin return and parameter types needed by code generation.
//! Keeps emission-time type decisions separate from instruction lowering.
//!
//! Called from:
//! - `crate::codegen::functions::types`
//!
//! Key details:
//! - Results must agree with `crate::types` so local slots and runtime value shapes are selected correctly.

use crate::codegen::context::Context;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::{array_key_type_from_value_type, FunctionSig, PhpType};

use super::arrays::{array_like_key_type, array_like_value_type, indexed_array_value_type};
use super::infer_local_type;
use super::union::merge_union_members;

pub(super) fn infer_function_call_type(
    name: &str,
    args: &[Expr],
    sig: &FunctionSig,
    ctx: Option<&Context>,
) -> PhpType {
    match name {
        "strtolower" | "strtoupper" | "ucfirst" | "lcfirst" | "ucwords" | "trim"
        | "ltrim" | "rtrim" | "substr" | "str_repeat" | "strrev" | "str_replace"
        | "str_ireplace" | "substr_replace" | "str_pad" | "chr" | "implode" | "join"
        | "sprintf" | "number_format" | "nl2br" | "wordwrap" | "addslashes"
        | "stripslashes" | "htmlspecialchars" | "html_entity_decode" | "htmlentities"
        | "urlencode" | "urldecode" | "rawurlencode" | "rawurldecode" | "base64_encode"
        | "base64_decode" | "bin2hex" | "hex2bin" | "md5" | "sha1" | "hash" | "gettype"
        | "strstr" | "readline" | "date"
        | "json_last_error_msg" | "php_uname" | "phpversion"
        | "tempnam" | "getcwd" | "shell_exec" | "preg_replace_callback" => PhpType::Str,
        "json_decode" => PhpType::Mixed,
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
        "explode"
        | "str_split"
        | "file"
        | "scandir"
        | "glob"
        | "array_merge"
        | "array_slice"
        | "array_reverse"
        | "array_unique"
        | "array_chunk"
        | "array_pad"
        | "array_fill"
        | "array_diff"
        | "array_intersect"
        | "array_splice"
        | "array_column"
        | "array_map"
        | "array_filter"
        | "range"
        | "array_rand"
        | "sscanf"
        | "fgetcsv"
        | "preg_split" => {
            if name == "explode"
                || name == "str_split"
                || name == "file"
                || name == "scandir"
                || name == "glob"
                || name == "fgetcsv"
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
        | "ctype_space" | "function_exists" | "chmod" | "chown" | "chgrp"
        | "touch" | "ftruncate" | "fflush" | "fsync" | "fdatasync" | "ptr_is_null"
        | "json_validate" | "flock" | "symlink" | "link" => {
            PhpType::Bool
        }
        "define" => PhpType::Bool,
        "umask" | "fpassthru" | "linkinfo" => PhpType::Int,
        "strpos" | "strrpos" | "array_search" | "file_get_contents" | "json_encode" | "fileatime"
        | "filectime" | "fileperms" | "fileowner" | "filegroup" | "fileinode"
        | "filetype" | "stat" | "lstat" | "fstat" | "fgetc" | "readfile" | "readlink" => PhpType::Mixed,
        "fopen" | "tmpfile" => merge_union_members(vec![PhpType::stream_resource(), PhpType::Bool]),
        "pathinfo" => infer_pathinfo_type(args),
        "abs" => {
            if !args.is_empty() {
                let t = infer_local_type(&args[0], sig, ctx);
                if t == PhpType::Float {
                    PhpType::Float
                } else {
                    PhpType::Int
                }
            } else {
                PhpType::Int
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
        "ptr_get" | "ptr_read8" | "ptr_read32" | "ptr_sizeof" => PhpType::Int,
        "class_attribute_names" => PhpType::Array(Box::new(PhpType::Str)),
        "class_attribute_args" => PhpType::Array(Box::new(PhpType::Mixed)),
        "class_get_attributes" => PhpType::Array(Box::new(PhpType::Object(
            "ReflectionAttribute".to_string(),
        ))),
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
