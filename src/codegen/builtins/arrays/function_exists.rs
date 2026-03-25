use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// All built-in function names recognized by elephc.
const BUILTINS: &[&str] = &[
    // system
    "exit", "die", "define", "time", "microtime", "sleep", "usleep",
    "getenv", "putenv", "php_uname", "phpversion", "exec", "shell_exec",
    "system", "passthru", "date", "mktime", "strtotime",
    "json_encode", "json_decode", "json_last_error",
    "preg_match", "preg_match_all", "preg_replace", "preg_split",
    // strings
    "strlen", "intval", "number_format", "substr", "strpos", "strrpos",
    "strstr", "strtolower", "strtoupper", "ucfirst", "lcfirst",
    "trim", "ltrim", "rtrim", "str_repeat", "strrev", "ord", "chr",
    "strcmp", "strcasecmp", "str_contains", "str_starts_with", "str_ends_with",
    "str_replace", "explode", "implode", "ucwords", "str_ireplace",
    "substr_replace", "str_pad", "str_split", "addslashes", "stripslashes",
    "nl2br", "wordwrap", "bin2hex", "hex2bin", "htmlspecialchars",
    "htmlentities", "html_entity_decode", "urlencode", "urldecode",
    "rawurlencode", "rawurldecode", "base64_encode", "base64_decode",
    "ctype_alpha", "ctype_digit", "ctype_alnum", "ctype_space",
    "sprintf", "md5", "sha1", "printf", "hash", "sscanf",
    // arrays
    "count", "array_push", "array_pop", "in_array", "array_keys",
    "array_values", "sort", "rsort", "isset", "array_key_exists",
    "array_search", "array_reverse", "array_unique", "array_sum",
    "array_product", "array_shift", "array_unshift", "array_merge",
    "array_slice", "array_splice", "array_combine", "array_flip",
    "array_chunk", "array_column", "array_pad", "array_fill",
    "array_fill_keys", "array_diff", "array_intersect",
    "array_diff_key", "array_intersect_key", "array_rand", "shuffle",
    "range", "asort", "arsort", "ksort", "krsort", "natsort", "natcasesort",
    "array_map", "array_filter", "array_reduce", "array_walk",
    "usort", "uksort", "uasort", "call_user_func", "call_user_func_array",
    "function_exists",
    // math
    "abs", "floor", "ceil", "round", "sqrt", "pow", "min", "max",
    "intdiv", "fmod", "fdiv", "rand", "mt_rand", "random_int",
    // types
    "is_bool", "boolval", "is_null", "floatval", "is_float", "is_int",
    "is_string", "is_numeric", "is_nan", "is_infinite", "is_finite",
    "gettype", "empty", "unset", "settype",
    // io
    "var_dump", "print_r", "fopen", "fclose", "fread", "fwrite",
    "fgets", "feof", "readline", "fseek", "ftell", "rewind",
    "file_get_contents", "file_put_contents", "file", "file_exists",
    "is_file", "is_dir", "is_readable", "is_writable", "filesize",
    "filemtime", "copy", "rename", "unlink", "mkdir", "rmdir",
    "scandir", "glob", "getcwd", "chdir", "tempnam", "sys_get_temp_dir",
    "fgetcsv", "fputcsv",
];

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("function_exists()");

    // -- resolve function name at compile time --
    let func_name = match &args[0].kind {
        ExprKind::StringLiteral(name) => name.clone(),
        _ => panic!("function_exists() argument must be a string literal"),
    };

    // -- emit constant true/false based on whether function is known --
    if ctx.functions.contains_key(&func_name) || BUILTINS.contains(&func_name.as_str()) {
        emitter.instruction("mov x0, #1");                                      // function exists → return true
    } else {
        emitter.instruction("mov x0, #0");                                      // function not found → return false
    }

    Some(PhpType::Bool)
}
