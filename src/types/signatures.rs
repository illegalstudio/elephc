//! Purpose:
//! Defines function signature metadata for user functions, builtins, closures, and callable aliases.
//! Stores parameter names, defaults, variadics, by-reference behavior, and return contracts used by call planning.
//!
//! Called from:
//! - `crate::types::checker::functions`
//! - `crate::types::call_args`
//!
//! Key details:
//! - Builtin signatures must match PHP so named arguments, first-class callables, and mutation semantics stay coherent.

use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;

use super::PhpType;

#[derive(Debug, Clone, PartialEq)]
/// Metadata for a callable's parameter and return type contract.
///
/// Used by call planning, named-argument resolution, first-class callables,
/// and type inference. Builtin signatures must match PHP for coherence with
/// named arguments, callable aliases, and mutation semantics.
pub struct FunctionSig {
    pub params: Vec<(String, PhpType)>,
    pub defaults: Vec<Option<Expr>>,
    pub return_type: PhpType,
    pub declared_return: bool,
    pub ref_params: Vec<bool>,
    pub declared_params: Vec<bool>,
    pub variadic: Option<String>,
    /// `Some(message)` if the declaration carried PHP 8.4 `#[\Deprecated]`.
    /// `Some("")` indicates the attribute was present without an explicit
    /// reason. `None` means the function/method is not deprecated.
    pub deprecation: Option<String>,
}

/// Upgrades a variadic signature for use as a first-class callable.
///
/// If the variadic parameter is not already typed as `Array`, upgrades it to
/// `Array<Mixed>`. Non-variadic signatures are returned unchanged.
///
/// Called from:
/// - first-class callable lowering in codegen
pub(crate) fn callable_wrapper_sig(sig: &FunctionSig) -> FunctionSig {
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return sig.clone();
    };

    let mut wrapper_sig = sig.clone();
    if let Some((name, ty)) = wrapper_sig.params.last_mut() {
        if name == variadic_name {
            if !matches!(ty, PhpType::Array(_)) {
                *ty = PhpType::Array(Box::new(PhpType::Mixed));
            }
            return wrapper_sig;
        }
    }

    wrapper_sig.params.push((
        variadic_name.clone(),
        PhpType::Array(Box::new(PhpType::Mixed)),
    ));
    wrapper_sig.defaults.push(None);
    wrapper_sig.ref_params.push(false);
    wrapper_sig.declared_params.push(false);
    wrapper_sig
}

/// Looks up a builtin function's canonical call signature.
///
/// Returns `Some(FunctionSig)` for known PHP builtins (e.g., `strlen`, `array_push`);
/// returns `None` for untracked or user-defined functions. The returned signature
/// reflects PHP's actual parameter ordering, defaults, variadics, and by-ref params.
///
/// Called from:
/// - type checker builtin validation
/// - first-class callable builtin sig construction
/// - optimizer effect modeling for builtins
pub(crate) fn builtin_call_sig(name: &str) -> Option<FunctionSig> {
    match name {
        "time" | "phpversion" | "json_last_error" | "json_last_error_msg" | "pi"
        | "ptr_null" | "getcwd" | "sys_get_temp_dir" | "tmpfile" => Some(fixed(&[])),

        "strlen" | "strtolower" | "strtoupper" | "ucfirst" | "lcfirst" | "strrev"
        | "grapheme_strrev" | "addslashes" | "stripslashes" | "nl2br" | "bin2hex"
        | "hex2bin" | "htmlspecialchars" | "htmlentities" | "html_entity_decode"
        | "urlencode" | "urldecode" | "rawurlencode" | "rawurldecode"
        | "base64_encode" | "base64_decode" => Some(fixed(&["string"])),
        "gzcompress" => Some(optional(&["data", "level"], 1, vec![int_lit(-1)])),
        "gzdeflate" => Some(optional(&["data", "level"], 1, vec![int_lit(-1)])),
        "gzinflate" => Some(optional(&["data", "max_length"], 1, vec![int_lit(0)])),
        "gzuncompress" => Some(optional(&["data", "max_length"], 1, vec![int_lit(0)])),
        "ord" => Some(fixed(&["character"])),
        "chr" => Some(fixed(&["codepoint"])),

        "ctype_alpha" | "ctype_digit" | "ctype_alnum" | "ctype_space" => {
            Some(fixed(&["text"]))
        }

        "intval" | "floatval" | "boolval" | "gettype" | "is_bool" | "is_null"
        | "is_float" | "is_int" | "is_iterable" | "is_string" | "is_numeric"
        | "empty" | "var_dump" | "print_r" => {
            Some(fixed(&["value"]))
        }
        "isset" => Some(variadic(&["var"], "vars")),
        "unset" => Some(variadic(&["var"], "vars")),
        "settype" => {
            let mut sig = fixed(&["var", "type"]);
            sig.ref_params[0] = true;
            Some(sig)
        }
        "function_exists" => Some(fixed(&["function"])),
        "is_callable" => Some(fixed(&["value"])),
        "defined" => Some(fixed(&["constant_name"])),
        "class_alias" => Some(optional(
            &["class", "alias", "autoload"],
            2,
            vec![bool_lit(true)],
        )),
        "class_exists" => Some(optional(&["class", "autoload"], 1, vec![bool_lit(true)])),
        "interface_exists" => Some(optional(
            &["interface", "autoload"],
            1,
            vec![bool_lit(true)],
        )),
        "trait_exists" => Some(optional(&["trait", "autoload"], 1, vec![bool_lit(true)])),
        "enum_exists" => Some(optional(&["enum", "autoload"], 1, vec![bool_lit(true)])),
        "class_implements" | "class_parents" | "class_uses" => Some(optional(
            &["object_or_class", "autoload"],
            1,
            vec![bool_lit(true)],
        )),
        "iterator_to_array" => Some(optional(
            &["iterator", "preserve_keys"],
            1,
            vec![bool_lit(true)],
        )),
        "iterator_count" => Some(fixed(&["iterator"])),
        "iterator_apply" => Some(optional(
            &["iterator", "callback", "args"],
            2,
            vec![null_lit()],
        )),
        "get_class" => Some(optional(&["object"], 0, vec![null_lit()])),
        "get_parent_class" => Some(optional(&["object_or_class"], 0, vec![null_lit()])),
        "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => {
            Some(fixed(&[]))
        }
        "is_a" => Some(optional(
            &["object_or_class", "class", "allow_string"],
            2,
            vec![bool_lit(false)],
        )),
        "is_subclass_of" => Some(optional(
            &["object_or_class", "class", "allow_string"],
            2,
            vec![bool_lit(true)],
        )),
        "is_resource" => Some(fixed(&["value"])),
        "get_resource_type" | "get_resource_id" => Some(fixed(&["resource"])),
        "class_attribute_names" | "class_get_attributes" => Some(fixed(&["class_name"])),
        "class_attribute_args" => Some(fixed(&["class_name", "attribute_name"])),

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

        "trim" | "ltrim" | "rtrim" | "chop" => Some(optional(
            &["string", "characters"],
            1,
            vec![string_lit(" \n\r\t\u{0b}\u{0c}\0")],
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
        "fprintf" => Some(variadic(&["stream", "format"], "values")),
        "vsprintf" | "vprintf" => Some(fixed(&["format", "values"])),
        "vfprintf" => Some(fixed(&["stream", "format", "values"])),
        "sscanf" => Some(variadic(&["string", "format"], "vars")),
        "fscanf" => Some(variadic(&["stream", "format"], "vars")),
        "hash" => Some(fixed(&["algo", "data"])),
        "md5" | "sha1" => Some(optional(&["string", "binary"], 1, vec![bool_lit(false)])),
        "crc32" => Some(fixed(&["string"])),
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
        "clamp" => Some(fixed(&["value", "min", "max"])),
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
        "json_encode" => Some(optional(
            &["value", "flags", "depth"],
            1,
            vec![int_lit(0), int_lit(512)],
        )),
        "json_decode" => Some(optional(
            &["json", "associative", "depth", "flags"],
            1,
            vec![null_lit(), int_lit(512), int_lit(0)],
        )),
        "json_validate" => Some(optional(
            &["json", "depth", "flags"],
            1,
            vec![int_lit(512), int_lit(0)],
        )),
        "preg_match" => {
            let mut sig = optional(
                &["pattern", "subject", "matches"],
                2,
                vec![Expr::new(ExprKind::ArrayLiteral(Vec::new()), Span::dummy())],
            );
            sig.ref_params[2] = true;
            Some(sig)
        }
        "preg_match_all" => Some(fixed(&["pattern", "subject"])),
        "preg_replace_callback" => Some(fixed(&["pattern", "callback", "subject"])),
        "preg_replace" => Some(fixed(&["pattern", "replacement", "subject"])),
        "preg_split" => Some(optional(
            &["pattern", "subject", "limit", "flags"],
            2,
            vec![int_lit(-1), int_lit(0)],
        )),

        "file_get_contents" | "file" | "file_exists" | "is_file" | "is_dir"
        | "is_readable" | "is_writable" | "is_writeable" | "is_executable"
        | "is_link" | "filesize" | "filemtime" | "fileatime" | "filectime"
        | "fileperms" | "fileowner" | "filegroup" | "fileinode" | "filetype"
        | "stat" | "lstat" => Some(fixed(&["filename"])),
        "disk_free_space" | "disk_total_space" => Some(fixed(&["directory"])),
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
        "fopen" => Some(optional(
            &["filename", "mode", "use_include_path", "context"],
            2,
            vec![bool_lit(false), null_lit()],
        )),
        "fclose" | "fgets" | "fgetc" | "fpassthru" | "feof" | "ftell" | "rewind"
        | "fstat" | "fsync" | "fflush" | "fdatasync" => Some(fixed(&["stream"])),
        "flock" => {
            let mut sig = optional(&["stream", "operation", "would_block"], 2, vec![null_lit()]);
            sig.ref_params[2] = true;
            Some(sig)
        }
        "readfile" => Some(fixed(&["filename"])),
        "symlink" | "link" => Some(fixed(&["target", "link"])),
        "readlink" | "linkinfo" => Some(fixed(&["path"])),
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
        "stream_isatty" | "stream_is_local" | "stream_supports_lock"
        | "stream_get_contents" | "stream_get_meta_data" => Some(fixed(&["stream"])),
        "stream_copy_to_stream" => Some(fixed(&["from", "to"])),
        "stream_socket_server" => Some(fixed(&["address"])),
        "stream_socket_client" => Some(fixed(&["address"])),
        "stream_socket_accept" => {
            let mut sig = optional(
                &["socket", "timeout", "peer_name"],
                1,
                vec![null_lit(), null_lit()],
            );
            sig.ref_params[2] = true;
            Some(sig)
        }
        "fsockopen" | "pfsockopen" => {
            let mut sig = optional(
                &["hostname", "port", "error_code", "error_message", "timeout"],
                2,
                vec![null_lit(), null_lit(), null_lit()],
            );
            sig.ref_params[2] = true;
            sig.ref_params[3] = true;
            Some(sig)
        }
        "stream_wrapper_register" => Some(optional(
            &["protocol", "class", "flags"],
            2,
            vec![int_lit(0)],
        )),
        "stream_wrapper_unregister" => Some(fixed(&["protocol"])),
        "stream_wrapper_restore" => Some(fixed(&["protocol"])),
        "stream_socket_enable_crypto" => Some(optional(
            &["stream", "enable", "crypto_method", "session_stream"],
            2,
            vec![null_lit(), null_lit()],
        )),
        "stream_context_create" => Some(optional(
            &["options", "params"],
            0,
            vec![null_lit(), null_lit()],
        )),
        "stream_context_get_default" => {
            Some(optional(&["options"], 0, vec![null_lit()]))
        }
        "stream_context_set_default" => Some(fixed(&["options"])),
        "stream_context_set_option" => Some(optional(
            &["context", "wrapper_or_options", "option_name", "value"],
            2,
            vec![null_lit(), null_lit()],
        )),
        "stream_context_set_params" => Some(fixed(&["context", "params"])),
        "stream_context_get_options" => Some(fixed(&["context"])),
        "stream_context_get_params" => Some(fixed(&["context"])),
        "stream_resolve_include_path" => Some(fixed(&["filename"])),
        "stream_filter_register" => Some(fixed(&["filter_name", "class"])),
        "stream_bucket_make_writeable" => Some(fixed(&["brigade"])),
        "stream_bucket_new" => Some(fixed(&["stream", "buffer"])),
        "stream_bucket_append" => Some(fixed(&["brigade", "bucket"])),
        "stream_bucket_prepend" => Some(fixed(&["brigade", "bucket"])),
        "stream_set_chunk_size" => Some(fixed(&["stream", "size"])),
        "stream_set_read_buffer" => Some(fixed(&["stream", "size"])),
        "stream_set_write_buffer" => Some(fixed(&["stream", "size"])),
        "stream_get_line" => Some(optional(
            &["stream", "length", "ending"],
            2,
            vec![string_lit("")],
        )),
        "stream_select" => {
            let mut sig = optional(
                &["read", "write", "except", "seconds", "microseconds"],
                4,
                vec![int_lit(0)],
            );
            sig.ref_params[0] = true;
            sig.ref_params[1] = true;
            sig.ref_params[2] = true;
            Some(sig)
        }
        "stream_set_blocking" => Some(fixed(&["stream", "enable"])),
        "stream_set_timeout" => Some(optional(
            &["stream", "seconds", "microseconds"],
            2,
            vec![int_lit(0)],
        )),
        "stream_socket_sendto" => Some(optional(
            &["socket", "data", "flags", "address"],
            2,
            vec![int_lit(0), string_lit("")],
        )),
        "stream_socket_recvfrom" => {
            let mut sig = optional(
                &["socket", "length", "flags", "address"],
                2,
                vec![int_lit(0), string_lit("")],
            );
            sig.ref_params[3] = true;
            Some(sig)
        }
        "stream_socket_get_name" => Some(fixed(&["socket", "remote"])),
        "stream_socket_pair" => Some(fixed(&["domain", "type", "protocol"])),
        "popen" => Some(fixed(&["command", "mode"])),
        "pclose" => Some(fixed(&["handle"])),
        "opendir" => Some(fixed(&["directory"])),
        "readdir" | "closedir" | "rewinddir" => Some(fixed(&["dir_handle"])),
        "stream_socket_shutdown" => Some(fixed(&["stream", "mode"])),
        "gethostname" => Some(fixed(&[])),
        "gethostbyname" => Some(fixed(&["hostname"])),
        "gethostbyaddr" => Some(fixed(&["ip"])),
        "getprotobyname" => Some(fixed(&["protocol"])),
        "getprotobynumber" => Some(fixed(&["protocol"])),
        "getservbyname" => Some(fixed(&["service", "protocol"])),
        "getservbyport" => Some(fixed(&["port", "protocol"])),
        "long2ip" => Some(fixed(&["ip"])),
        "ip2long" => Some(fixed(&["ip"])),
        "inet_ntop" | "inet_pton" => Some(fixed(&["ip"])),
        "stream_get_transports" | "stream_get_wrappers" | "stream_get_filters" => {
            Some(fixed(&[]))
        }
        "stream_filter_append" | "stream_filter_prepend" => Some(optional(
            &["stream", "filtername", "read_write", "params"],
            2,
            vec![int_lit(3), null_lit()],
        )),
        "stream_filter_remove" => Some(fixed(&["stream_filter"])),

        "spl_autoload_register" => Some(optional(
            &["callback", "throw", "prepend"],
            0,
            vec![null_lit(), bool_lit(true), bool_lit(false)],
        )),
        "spl_autoload_unregister" => Some(fixed(&["callback"])),
        "spl_autoload_functions" | "spl_classes" => Some(fixed(&[])),
        "spl_autoload_extensions" => {
            Some(optional(&["file_extensions"], 0, vec![null_lit()]))
        }
        "spl_autoload_call" => Some(fixed(&["class"])),
        "spl_autoload" => Some(optional(&["class", "file_extensions"], 1, vec![null_lit()])),
        "spl_object_id" | "spl_object_hash" => Some(fixed(&["object"])),

        "ptr" => Some(fixed(&["value"])),
        "ptr_is_null" | "ptr_get" | "ptr_read8" | "ptr_read16" | "ptr_read32" => {
            Some(fixed(&["pointer"]))
        }
        "ptr_read_string" => Some(fixed(&["pointer", "length"])),
        "ptr_offset" => Some(fixed(&["pointer", "offset"])),
        "ptr_set" | "ptr_write8" | "ptr_write16" | "ptr_write32" => {
            Some(fixed(&["pointer", "value"]))
        }
        "ptr_write_string" => Some(fixed(&["pointer", "string"])),
        "ptr_sizeof" => Some(fixed(&["type"])),
        "buffer_new" => Some(fixed(&["length"])),
        "buffer_len" | "buffer_free" => Some(fixed(&["buffer"])),
        _ => None,
    }
}

/// Returns the signature used when a builtin is accessed as a first-class callable.
///
/// Some builtins (e.g., `strlen`, `count`, `buffer_len`) have explicit first-class
/// signatures with precise parameter and return types. Unknown builtins fall through
/// to `general_first_class_callable_builtin_sig`.
///
/// Called from:
/// - first-class callable lowering for builtin references
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
            deprecation: None,
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
            deprecation: None,
        }),
        "buffer_len" => Some(FunctionSig {
            params: vec![("buffer".to_string(), PhpType::Buffer(Box::new(PhpType::Int)))],
            defaults: vec![None],
            return_type: PhpType::Int,
            declared_return: true,
            ref_params: vec![false],
            declared_params: vec![true],
            variadic: None,
            deprecation: None,
        }),
        _ => general_first_class_callable_builtin_sig(name),
    }
}

/// Fallback first-class callable signature builder for builtins without an explicit override.
/// Constructs typed signatures for builtins where the canonical call signature
/// (from `builtin_call_sig`) is not appropriate for first-class use.
fn general_first_class_callable_builtin_sig(name: &str) -> Option<FunctionSig> {
    match name {
        "time" | "json_last_error" => Some(typed_first_class_builtin_sig(
            name,
            &[],
            PhpType::Int,
        )),
        "phpversion" | "getcwd" | "sys_get_temp_dir" | "json_last_error_msg" => {
            Some(typed_first_class_builtin_sig(name, &[], PhpType::Str))
        }
        "pi" => Some(typed_first_class_builtin_sig(name, &[], PhpType::Float)),
        "intval" | "ord" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Str],
            PhpType::Int,
        )),
        "floatval" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Mixed],
            PhpType::Float,
        )),
        "boolval" | "is_bool" | "is_null" | "is_float" | "is_int" | "is_iterable"
        | "is_string" | "is_numeric" | "is_nan" | "is_finite" | "is_infinite"
        | "ctype_alpha" | "ctype_digit" | "ctype_alnum" | "ctype_space" => {
            Some(typed_first_class_builtin_sig(name, &[PhpType::Mixed], PhpType::Bool))
        }
        "defined" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Str],
            PhpType::Bool,
        )),
        "gettype" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Mixed],
            PhpType::Str,
        )),
        "grapheme_strrev" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Str],
            PhpType::Union(vec![PhpType::Str, PhpType::Bool]),
        )),
        "strtolower" | "strtoupper" | "ucfirst" | "lcfirst" | "strrev"
        | "addslashes" | "stripslashes" | "nl2br" | "bin2hex" | "hex2bin"
        | "htmlspecialchars" | "htmlentities" | "html_entity_decode" | "urlencode"
        | "urldecode" | "rawurlencode" | "rawurldecode" | "base64_encode"
        | "base64_decode" | "trim" | "ltrim" | "rtrim" | "chop" | "ucwords" | "substr"
        | "str_repeat" | "strstr" | "str_replace" | "str_ireplace" | "explode"
        | "implode" | "substr_replace" | "str_pad" | "str_split" | "wordwrap"
        | "sprintf" | "hash" | "md5" | "sha1" | "crc32" | "number_format" | "chr" => {
            Some(typed_first_class_builtin_sig(
                name,
                &[PhpType::Str],
                PhpType::Str,
            ))
        }
        "strpos" | "strrpos" | "strcmp" | "strcasecmp" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Str],
            PhpType::Int,
        )),
        "str_contains" | "str_starts_with" | "str_ends_with" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Str, PhpType::Str],
            PhpType::Bool,
        )),
        "array_keys" | "array_values" | "array_reverse" | "array_unique" | "array_rand" => {
            Some(typed_first_class_builtin_sig(
                name,
                &[PhpType::Array(Box::new(PhpType::Mixed))],
                PhpType::Array(Box::new(PhpType::Mixed)),
            ))
        }
        "array_chunk" | "array_pad" | "array_fill" | "array_slice" | "array_diff"
        | "array_intersect" | "range" => return_typed_first_class_builtin_sig(
            name,
            PhpType::Array(Box::new(PhpType::Mixed)),
        ),
        "array_flip" | "array_combine" | "array_fill_keys" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Array(Box::new(PhpType::Mixed))],
            PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            },
        )),
        "array_sum" | "array_product" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Array(Box::new(PhpType::Int))],
            PhpType::Int,
        )),
        "array_key_exists" | "in_array" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Mixed, PhpType::Array(Box::new(PhpType::Mixed))],
            PhpType::Bool,
        )),
        "array_search" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Mixed, PhpType::Array(Box::new(PhpType::Mixed)), PhpType::Bool],
            PhpType::Mixed,
        )),
        "array_pop" | "array_shift" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Array(Box::new(PhpType::Mixed))],
            PhpType::Mixed,
        )),
        "iterator_to_array" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Iterable, PhpType::Bool],
            PhpType::Array(Box::new(PhpType::Mixed)),
        )),
        "iterator_count" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Iterable],
            PhpType::Int,
        )),
        "iterator_apply" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Object("Traversable".to_string())],
            PhpType::Int,
        )),
        "array_push" | "array_unshift" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Array(Box::new(PhpType::Mixed)), PhpType::Mixed],
            PhpType::Void,
        )),
        "sort" | "rsort" | "shuffle" | "natsort" | "natcasesort" | "asort"
        | "arsort" | "ksort" | "krsort" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Array(Box::new(PhpType::Mixed))],
            PhpType::Void,
        )),
        "is_file" | "is_dir" | "is_readable" | "is_writable" | "is_writeable"
        | "is_executable" | "is_link" | "file_exists" | "fnmatch" | "chmod" | "chown"
        | "chgrp" | "touch" | "ftruncate" | "fflush" | "fsync" | "fdatasync"
        | "symlink" | "link" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Str],
            PhpType::Bool,
        )),
        "file_get_contents" | "file" | "filesize" | "filemtime" | "fileatime"
        | "filectime" | "fileperms" | "fileowner" | "filegroup" | "fileinode"
        | "filetype" | "stat" | "lstat" | "basename" | "dirname" | "realpath"
        | "pathinfo" | "readlink" | "linkinfo" | "tempnam" => {
            let mut sig = builtin_call_sig(name)?;
            sig.return_type = PhpType::Mixed;
            sig.declared_return = true;
            Some(sig)
        }
        "abs" | "min" | "max" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Mixed],
            PhpType::Mixed,
        )),
        "clamp" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Mixed, PhpType::Mixed, PhpType::Mixed],
            PhpType::Mixed,
        )),
        "floor" | "ceil" | "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan"
        | "sinh" | "cosh" | "tanh" | "log2" | "log10" | "exp" | "deg2rad"
        | "rad2deg" | "microtime" | "log" | "atan2" | "hypot" | "pow" | "fmod"
        | "fdiv" | "round" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Mixed],
            PhpType::Float,
        )),
        "intdiv" | "rand" | "mt_rand" | "random_int" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Int, PhpType::Int],
            PhpType::Int,
        )),
        "date" | "php_uname" | "readline" => {
            let mut sig = builtin_call_sig(name)?;
            sig.return_type = PhpType::Str;
            sig.declared_return = true;
            Some(sig)
        }
        "json_encode" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Mixed, PhpType::Int, PhpType::Int],
            PhpType::Union(vec![PhpType::Str, PhpType::Bool]),
        )),
        "json_decode" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Str, PhpType::Bool, PhpType::Int, PhpType::Int],
            PhpType::Mixed,
        )),
        "json_validate" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Str, PhpType::Int, PhpType::Int],
            PhpType::Bool,
        )),
        "preg_replace_callback" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Str, PhpType::Callable, PhpType::Str],
            PhpType::Str,
        )),
        "ptr_read16" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Pointer(None)],
            PhpType::Int,
        )),
        "ptr_write16" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Pointer(None), PhpType::Int],
            PhpType::Void,
        )),
        "ptr_read_string" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Pointer(None), PhpType::Int],
            PhpType::Str,
        )),
        "ptr_write_string" => Some(typed_first_class_builtin_sig(
            name,
            &[PhpType::Pointer(None), PhpType::Str],
            PhpType::Int,
        )),
        _ => None,
    }
}

/// Builds a first-class callable signature by overriding parameter and return types
/// on top of an existing builtin catalog entry.
///
/// Panics if `name` is not in the builtin catalog — all first-class callable builtins
/// must have a prior `builtin_call_sig` entry.
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

/// Builds a first-class callable signature by overriding only the return type
/// on top of an existing builtin catalog entry. Returns `None` if the builtin
/// has no catalog entry.
fn return_typed_first_class_builtin_sig(name: &str, return_type: PhpType) -> Option<FunctionSig> {
    let mut sig = builtin_call_sig(name)?;
    sig.return_type = return_type;
    sig.declared_return = true;
    Some(sig)
}

/// Constructs a signature with all parameters required (no defaults).
fn fixed(params: &[&str]) -> FunctionSig {
    make_sig(params, vec![None; params.len()], None)
}

/// Constructs a signature with some trailing parameters optional.
///
/// `required` indicates how many leading params are mandatory; the rest receive
/// defaults from `optional_defaults` (mapped positionally). Defaults are padded
/// with `None` if fewer are provided than total params.
fn optional(params: &[&str], required: usize, optional_defaults: Vec<Expr>) -> FunctionSig {
    let mut defaults = vec![None; required];
    defaults.extend(optional_defaults.into_iter().map(Some));
    while defaults.len() < params.len() {
        defaults.push(None);
    }
    make_sig(params, defaults, None)
}

/// Constructs a variadic signature — trailing param collects excess arguments as an array.
///
/// `regular_params` lists the fixed parameters; `variadic_name` names the trailing
/// variadic parameter. The variadic param starts as an empty `array` default.
fn variadic(regular_params: &[&str], variadic_name: &str) -> FunctionSig {
    let mut params = regular_params.to_vec();
    params.push(variadic_name);
    let mut defaults = vec![None; regular_params.len()];
    defaults.push(Some(Expr::new(ExprKind::ArrayLiteral(Vec::new()), Span::dummy())));
    make_sig(&params, defaults, Some(variadic_name))
}

/// Marks the first parameter of a signature as by-reference.
fn first_param_ref(mut sig: FunctionSig) -> FunctionSig {
    if let Some(first_ref) = sig.ref_params.first_mut() {
        *first_ref = true;
    }
    sig
}

/// Low-level `FunctionSig` constructor from raw parts.
///
/// Assembles params as `Mixed` types, sets all other fields from arguments,
/// and defaults `deprecation` to `None`.
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
        deprecation: None,
    }
}

/// Constructs an `i64` literal expression for use in default parameter values.
fn int_lit(value: i64) -> Expr {
    Expr::new(ExprKind::IntLiteral(value), Span::dummy())
}

/// Constructs a string literal expression for use in default parameter values.
fn string_lit(value: &str) -> Expr {
    Expr::new(ExprKind::StringLiteral(value.to_string()), Span::dummy())
}

/// Constructs a boolean literal expression for use in default parameter values.
fn bool_lit(value: bool) -> Expr {
    Expr::new(ExprKind::BoolLiteral(value), Span::dummy())
}

/// Constructs a null literal expression for use in default parameter values.
fn null_lit() -> Expr {
    Expr::new(ExprKind::Null, Span::dummy())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Computes the callable signature metadata for variadic.
    fn variadic_sig(params: Vec<(String, PhpType)>) -> FunctionSig {
        FunctionSig {
            defaults: vec![None; params.len()],
            return_type: PhpType::Mixed,
            declared_return: false,
            ref_params: vec![false; params.len()],
            declared_params: vec![false; params.len()],
            params,
            variadic: Some("values".to_string()),
            deprecation: None,
        }
    }

    /// Builds the parameter metadata for callable wrapper sig retypes existing non array variadic.
    #[test]
    fn callable_wrapper_sig_retypes_existing_non_array_variadic_param() {
        let sig = variadic_sig(vec![
            ("format".to_string(), PhpType::Str),
            ("values".to_string(), PhpType::Mixed),
        ]);

        let wrapper_sig = callable_wrapper_sig(&sig);

        assert_eq!(wrapper_sig.params.len(), 2);
        assert_eq!(
            wrapper_sig.params[1],
            (
                "values".to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
            )
        );
        assert_eq!(wrapper_sig.defaults.len(), 2);
        assert_eq!(wrapper_sig.ref_params.len(), 2);
        assert_eq!(wrapper_sig.declared_params.len(), 2);
    }

    /// Builds the parameter metadata for callable wrapper sig appends missing variadic.
    #[test]
    fn callable_wrapper_sig_appends_missing_variadic_param() {
        let sig = variadic_sig(vec![("format".to_string(), PhpType::Str)]);

        let wrapper_sig = callable_wrapper_sig(&sig);

        assert_eq!(wrapper_sig.params.len(), 2);
        assert_eq!(
            wrapper_sig.params[1],
            (
                "values".to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
            )
        );
        assert_eq!(wrapper_sig.defaults.len(), 2);
        assert_eq!(wrapper_sig.ref_params.len(), 2);
        assert_eq!(wrapper_sig.declared_params.len(), 2);
    }
}
