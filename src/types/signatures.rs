use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;

use super::PhpType;

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSig {
    pub params: Vec<(String, PhpType)>,
    pub defaults: Vec<Option<Expr>>,
    pub return_type: PhpType,
    pub declared_return: bool,
    pub ref_params: Vec<bool>,
    pub declared_params: Vec<bool>,
    pub variadic: Option<String>,
}

pub(crate) fn builtin_call_sig(name: &str) -> Option<FunctionSig> {
    match name {
        "time" | "phpversion" | "json_last_error" | "pi" | "ptr_null" | "getcwd"
        | "sys_get_temp_dir" => Some(fixed(&[])),

        "strlen" | "strtolower" | "strtoupper" | "ucfirst" | "lcfirst" | "strrev"
        | "addslashes" | "stripslashes" | "nl2br" | "bin2hex" | "hex2bin"
        | "htmlspecialchars" | "htmlentities" | "html_entity_decode" | "urlencode"
        | "urldecode" | "rawurlencode" | "rawurldecode" | "base64_encode"
        | "base64_decode" => Some(fixed(&["string"])),
        "ord" => Some(fixed(&["character"])),
        "chr" => Some(fixed(&["codepoint"])),

        "ctype_alpha" | "ctype_digit" | "ctype_alnum" | "ctype_space" => {
            Some(fixed(&["text"]))
        }

        "intval" | "floatval" | "boolval" | "gettype" | "is_bool" | "is_null"
        | "is_float" | "is_int" | "is_iterable" | "is_string" | "is_numeric"
        | "empty" | "isset" | "unset" | "var_dump" | "print_r" => {
            Some(fixed(&["value"]))
        }
        "settype" => {
            let mut sig = fixed(&["var", "type"]);
            sig.ref_params[0] = true;
            Some(sig)
        }
        "function_exists" => Some(fixed(&["function"])),

        "is_nan" | "is_finite" | "is_infinite" | "abs" | "floor" | "ceil" | "sqrt"
        | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh"
        | "tanh" | "log2" | "log10" | "exp" | "deg2rad" | "rad2deg" => {
            Some(fixed(&["num"]))
        }

        "count" => Some(optional(&["value", "mode"], 1, vec![int_lit(0)])),
        "microtime" => Some(optional(&["as_float"], 0, vec![bool_lit(false)])),
        "php_uname" => Some(optional(&["mode"], 0, vec![string_lit("a")])),
        "readline" => Some(optional(&["prompt"], 0, vec![null_lit()])),
        "umask" => Some(optional(&["mask"], 0, vec![null_lit()])),
        "exit" | "die" => Some(optional(&["status"], 0, vec![int_lit(0)])),

        "trim" | "ltrim" | "rtrim" => Some(optional(
            &["string", "characters"],
            1,
            vec![string_lit(" \n\r\t\u{0b}\0")],
        )),
        "ucwords" => Some(optional(
            &["string", "separators"],
            1,
            vec![string_lit(" \t\r\n\u{0c}\u{0b}")],
        )),
        "substr" => Some(optional(&["string", "offset", "length"], 2, vec![null_lit()])),
        "strpos" | "strrpos" => Some(optional(
            &["haystack", "needle", "offset"],
            2,
            vec![int_lit(0)],
        )),
        "strstr" => Some(optional(
            &["haystack", "needle", "before_needle"],
            2,
            vec![bool_lit(false)],
        )),
        "str_repeat" => Some(fixed(&["string", "times"])),
        "strcmp" | "strcasecmp" => Some(fixed(&["string1", "string2"])),
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            Some(fixed(&["haystack", "needle"]))
        }
        "str_replace" | "str_ireplace" => Some(optional(
            &["search", "replace", "subject", "count"],
            3,
            vec![null_lit()],
        )),
        "explode" => Some(optional(
            &["separator", "string", "limit"],
            2,
            vec![int_lit(i64::MAX)],
        )),
        "implode" => Some(optional(&["separator", "array"], 1, vec![null_lit()])),
        "substr_replace" => Some(optional(
            &["string", "replace", "offset", "length"],
            3,
            vec![null_lit()],
        )),
        "str_pad" => Some(optional(
            &["string", "length", "pad_string", "pad_type"],
            2,
            vec![string_lit(" "), int_lit(1)],
        )),
        "str_split" => Some(optional(&["string", "length"], 1, vec![int_lit(1)])),
        "wordwrap" => Some(optional(
            &["string", "width", "break", "cut_long_words"],
            1,
            vec![int_lit(75), string_lit("\n"), bool_lit(false)],
        )),
        "sprintf" | "printf" => Some(variadic(&["format"], "values")),
        "sscanf" => Some(variadic(&["string", "format"], "vars")),
        "hash" => Some(fixed(&["algo", "data"])),
        "md5" | "sha1" => Some(optional(&["string", "binary"], 1, vec![bool_lit(false)])),
        "number_format" => Some(optional(
            &["num", "decimals", "decimal_separator", "thousands_separator"],
            1,
            vec![int_lit(0), string_lit("."), string_lit(",")],
        )),

        "array_pop" | "array_shift" => Some(first_param_ref(fixed(&["array"]))),
        "array_keys" | "array_values" | "array_reverse" | "array_unique" | "array_flip"
        | "array_sum" | "array_product" | "array_rand" => Some(fixed(&["array"])),
        "sort" | "rsort" | "shuffle" | "natsort" | "natcasesort" | "asort"
        | "arsort" | "ksort" | "krsort" => Some(first_param_ref(fixed(&["array"]))),
        "in_array" => Some(optional(&["needle", "haystack", "strict"], 2, vec![bool_lit(false)])),
        "array_key_exists" => Some(fixed(&["key", "array"])),
        "array_search" => {
            Some(optional(&["needle", "haystack", "strict"], 2, vec![bool_lit(false)]))
        }
        "array_push" | "array_unshift" => Some(first_param_ref(variadic(&["array"], "values"))),
        "array_merge" => Some(variadic(&[], "arrays")),
        "array_diff" | "array_intersect" | "array_diff_key" | "array_intersect_key" => {
            Some(variadic(&["array"], "arrays"))
        }
        "array_combine" => Some(fixed(&["keys", "values"])),
        "array_fill_keys" => Some(fixed(&["keys", "value"])),
        "array_pad" => Some(fixed(&["array", "length", "value"])),
        "array_fill" => Some(fixed(&["start_index", "count", "value"])),
        "array_slice" => Some(optional(
            &["array", "offset", "length"],
            2,
            vec![null_lit()],
        )),
        "array_splice" => Some(first_param_ref(optional(
            &["array", "offset", "length"],
            2,
            vec![null_lit()],
        ))),
        "array_chunk" => Some(fixed(&["array", "length"])),
        "array_column" => Some(fixed(&["array", "column_key"])),
        "range" => Some(fixed(&["start", "end"])),
        "array_map" => Some(variadic(&["callback", "array"], "arrays")),
        "array_filter" => Some(optional(
            &["array", "callback", "mode"],
            1,
            vec![null_lit(), int_lit(0)],
        )),
        "array_reduce" => Some(optional(
            &["array", "callback", "initial"],
            2,
            vec![null_lit()],
        )),
        "array_walk" | "usort" | "uksort" | "uasort" => {
            Some(first_param_ref(fixed(&["array", "callback"])))
        }
        "call_user_func" => Some(variadic(&["callback"], "args")),
        "call_user_func_array" => Some(fixed(&["callback", "args"])),

        "log" => Some(optional(
            &["num", "base"],
            1,
            vec![Expr::new(ExprKind::FloatLiteral(std::f64::consts::E), Span::dummy())],
        )),
        "atan2" => Some(fixed(&["y", "x"])),
        "hypot" => Some(fixed(&["x", "y"])),
        "pow" => Some(fixed(&["num", "exponent"])),
        "intdiv" | "fmod" | "fdiv" => Some(fixed(&["num1", "num2"])),
        "min" | "max" => Some(variadic(&["value"], "values")),
        "rand" | "mt_rand" | "random_int" => Some(fixed(&["min", "max"])),
        "round" => Some(optional(&["num", "precision"], 1, vec![int_lit(0)])),

        "sleep" => Some(fixed(&["seconds"])),
        "usleep" => Some(fixed(&["microseconds"])),
        "getenv" => Some(fixed(&["name"])),
        "putenv" => Some(fixed(&["assignment"])),
        "exec" | "shell_exec" | "system" | "passthru" => Some(fixed(&["command"])),
        "define" => Some(fixed(&["constant_name", "value"])),
        "date" => Some(optional(&["format", "timestamp"], 1, vec![null_lit()])),
        "mktime" => Some(fixed(&["hour", "minute", "second", "month", "day", "year"])),
        "strtotime" => Some(fixed(&["datetime"])),
        "json_encode" => Some(fixed(&["value"])),
        "json_decode" => Some(fixed(&["json"])),
        "preg_match" | "preg_match_all" => Some(fixed(&["pattern", "subject"])),
        "preg_replace" => Some(fixed(&["pattern", "replacement", "subject"])),
        "preg_split" => Some(fixed(&["pattern", "subject"])),

        "file_get_contents" | "file" | "file_exists" | "is_file" | "is_dir"
        | "is_readable" | "is_writable" | "is_writeable" | "is_executable"
        | "is_link" | "filesize" | "filemtime" | "fileatime" | "filectime"
        | "fileperms" | "fileowner" | "filegroup" | "fileinode" | "filetype"
        | "stat" | "lstat" => Some(fixed(&["filename"])),
        "file_put_contents" => Some(fixed(&["filename", "data"])),
        "copy" | "rename" => Some(fixed(&["from", "to"])),
        "unlink" => Some(fixed(&["filename"])),
        "mkdir" | "rmdir" | "chdir" | "scandir" => Some(fixed(&["directory"])),
        "glob" => Some(fixed(&["pattern"])),
        "tempnam" => Some(fixed(&["directory", "prefix"])),
        "chmod" => Some(fixed(&["filename", "permissions"])),
        "chown" => Some(fixed(&["filename", "user"])),
        "chgrp" => Some(fixed(&["filename", "group"])),
        "touch" => Some(optional(
            &["filename", "mtime", "atime"],
            1,
            vec![null_lit(), null_lit()],
        )),
        "basename" => Some(optional(&["path", "suffix"], 1, vec![string_lit("")])),
        "dirname" => Some(optional(&["path", "levels"], 1, vec![int_lit(1)])),
        "fnmatch" => Some(optional(&["pattern", "filename", "flags"], 2, vec![int_lit(0)])),
        "realpath" => Some(fixed(&["path"])),
        "pathinfo" => Some(optional(&["path", "flags"], 1, vec![int_lit(15)])),
        "fopen" => Some(fixed(&["filename", "mode"])),
        "fclose" | "fgets" | "feof" | "ftell" | "rewind" | "fstat" | "fsync"
        | "fflush" | "fdatasync" => Some(fixed(&["stream"])),
        "fread" => Some(fixed(&["stream", "length"])),
        "fwrite" => Some(fixed(&["stream", "data"])),
        "fseek" => Some(optional(&["stream", "offset", "whence"], 2, vec![int_lit(0)])),
        "fgetcsv" => Some(optional(
            &["stream", "length", "separator"],
            1,
            vec![null_lit(), string_lit(",")],
        )),
        "fputcsv" => Some(optional(
            &["stream", "fields", "separator", "enclosure"],
            2,
            vec![string_lit(","), string_lit("\"")],
        )),
        "ftruncate" => Some(fixed(&["stream", "size"])),
        "clearstatcache" => Some(optional(
            &["clear_realpath_cache", "filename"],
            0,
            vec![bool_lit(false), string_lit("")],
        )),

        "ptr" => Some(fixed(&["value"])),
        "ptr_is_null" | "ptr_get" | "ptr_read8" | "ptr_read32" => Some(fixed(&["pointer"])),
        "ptr_offset" => Some(fixed(&["pointer", "offset"])),
        "ptr_set" | "ptr_write8" | "ptr_write32" => Some(fixed(&["pointer", "value"])),
        "ptr_sizeof" => Some(fixed(&["type"])),
        "buffer_new" => Some(fixed(&["length"])),
        "buffer_len" | "buffer_free" => Some(fixed(&["buffer"])),
        _ => None,
    }
}

pub(crate) fn first_class_callable_builtin_sig(name: &str) -> Option<FunctionSig> {
    match name {
        "strlen" => Some(FunctionSig {
            params: vec![("string".to_string(), PhpType::Str)],
            defaults: vec![None],
            return_type: PhpType::Int,
            declared_return: true,
            ref_params: vec![false],
            declared_params: vec![true],
            variadic: None,
        }),
        "count" => Some(FunctionSig {
            params: vec![(
                "value".to_string(),
                PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed),
                    value: Box::new(PhpType::Mixed),
                },
            )],
            defaults: vec![None],
            return_type: PhpType::Int,
            declared_return: true,
            ref_params: vec![false],
            declared_params: vec![true],
            variadic: None,
        }),
        "buffer_len" => Some(FunctionSig {
            params: vec![("buffer".to_string(), PhpType::Buffer(Box::new(PhpType::Int)))],
            defaults: vec![None],
            return_type: PhpType::Int,
            declared_return: true,
            ref_params: vec![false],
            declared_params: vec![true],
            variadic: None,
        }),
        _ => general_first_class_callable_builtin_sig(name),
    }
}

fn general_first_class_callable_builtin_sig(name: &str) -> Option<FunctionSig> {
    match name {
        "intval" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Str],
            PhpType::Int,
        )),
        "strtolower" | "strtoupper" | "ucfirst" | "lcfirst" | "strrev"
        | "addslashes" | "stripslashes" | "nl2br" | "bin2hex" | "hex2bin"
        | "htmlspecialchars" | "htmlentities" | "html_entity_decode" | "urlencode"
        | "urldecode" | "rawurlencode" | "rawurldecode" | "base64_encode"
        | "base64_decode" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Str],
            PhpType::Str,
        )),
        "array_sum" | "array_product" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Array(Box::new(PhpType::Int))],
            PhpType::Int,
        )),
        _ => None,
    }
}

fn typed_first_class_builtin_sig(
    name: &str,
    param_types: &[PhpType],
    return_type: PhpType,
) -> FunctionSig {
    let mut sig = builtin_call_sig(name).expect("first-class builtin must have a catalog signature");
    for (idx, param_ty) in param_types.iter().enumerate() {
        if let Some((_, ty)) = sig.params.get_mut(idx) {
            *ty = param_ty.clone();
        }
    }
    sig.return_type = return_type;
    sig.declared_return = true;
    sig
}

fn fixed(params: &[&str]) -> FunctionSig {
    make_sig(params, vec![None; params.len()], None)
}

fn optional(params: &[&str], required: usize, optional_defaults: Vec<Expr>) -> FunctionSig {
    let mut defaults = vec![None; required];
    defaults.extend(optional_defaults.into_iter().map(Some));
    while defaults.len() < params.len() {
        defaults.push(None);
    }
    make_sig(params, defaults, None)
}

fn variadic(regular_params: &[&str], variadic_name: &str) -> FunctionSig {
    let mut params = regular_params.to_vec();
    params.push(variadic_name);
    let mut defaults = vec![None; regular_params.len()];
    defaults.push(Some(Expr::new(ExprKind::ArrayLiteral(Vec::new()), Span::dummy())));
    make_sig(&params, defaults, Some(variadic_name))
}

fn first_param_ref(mut sig: FunctionSig) -> FunctionSig {
    if let Some(first_ref) = sig.ref_params.first_mut() {
        *first_ref = true;
    }
    sig
}

fn make_sig(params: &[&str], defaults: Vec<Option<Expr>>, variadic: Option<&str>) -> FunctionSig {
    FunctionSig {
        params: params
            .iter()
            .map(|name| ((*name).to_string(), PhpType::Mixed))
            .collect(),
        defaults,
        return_type: PhpType::Mixed,
        declared_return: false,
        ref_params: vec![false; params.len()],
        declared_params: vec![false; params.len()],
        variadic: variadic.map(str::to_string),
    }
}

fn int_lit(value: i64) -> Expr {
    Expr::new(ExprKind::IntLiteral(value), Span::dummy())
}

fn string_lit(value: &str) -> Expr {
    Expr::new(ExprKind::StringLiteral(value.to_string()), Span::dummy())
}

fn bool_lit(value: bool) -> Expr {
    Expr::new(ExprKind::BoolLiteral(value), Span::dummy())
}

fn null_lit() -> Expr {
    Expr::new(ExprKind::Null, Span::dummy())
}
