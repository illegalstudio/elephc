//! Purpose:
//! Groups builtin registry lookup, argument binding, callable dispatch, and
//! evaluated-argument builtin dispatch.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core eval call paths.
//!
//! Key details:
//! - The large by-value dispatch match is isolated from argument planning and
//!   callable normalization.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::super::*;
use super::spec::EvalBuiltinSpec;

mod binding;
mod callable;
mod callable_validation;
mod dispatch;
mod dynamic_mutation;
mod names;
mod signature;

pub(in crate::interpreter) use binding::*;
pub(in crate::interpreter) use callable::*;
pub(in crate::interpreter) use callable_validation::*;
pub(in crate::interpreter) use dispatch::*;
pub(in crate::interpreter) use dynamic_mutation::*;
pub(in crate::interpreter) use names::*;
pub(in crate::interpreter) use signature::*;

/// Lazy registry of builtins migrated to declarative eval specs.
struct DeclaredBuiltinRegistry {
    /// Case-insensitive lookup keyed by canonical lowercase PHP builtin name.
    by_name: HashMap<String, &'static EvalBuiltinSpec>,
    /// Stable ordered list of registered canonical names.
    names: Vec<&'static str>,
}

/// Global eval builtin registry built from inventory submissions.
static DECLARED_BUILTIN_REGISTRY: OnceLock<DeclaredBuiltinRegistry> = OnceLock::new();

/// Builds the declarative registry and rejects duplicate builtin names.
fn build_declared_builtin_registry() -> DeclaredBuiltinRegistry {
    let mut by_name = HashMap::new();
    let mut names = Vec::new();

    for spec in inventory::iter::<EvalBuiltinSpec> {
        validate_declared_builtin_spec(spec);
        let key = spec.name.to_ascii_lowercase();
        if by_name.insert(key, spec).is_some() {
            panic!(
                "duplicate eval builtin name registered in inventory: \"{}\"",
                spec.name
            );
        }
        names.push(spec.name);
    }

    names.sort_unstable();
    DeclaredBuiltinRegistry { by_name, names }
}

/// Validates static spec invariants before the registry is exposed.
fn validate_declared_builtin_spec(spec: &EvalBuiltinSpec) {
    let expected_param_names = spec.params.len() + usize::from(spec.variadic.is_some());
    assert_eq!(
        expected_param_names,
        spec.param_names.len(),
        "eval builtin {} has mismatched params and param_names",
        spec.name
    );
    for (param, name) in spec.params.iter().zip(spec.param_names.iter()) {
        assert_eq!(
            param.name, *name,
            "eval builtin {} has a param_names entry out of sync",
            spec.name
        );
        if param.by_ref {
            assert!(
                spec.by_ref_params.contains(&param.name),
                "eval builtin {} marks {} by-ref without listing it",
                spec.name,
                param.name
            );
        }
    }
    for by_ref_name in spec.by_ref_params {
        assert!(
            spec.params
                .iter()
                .any(|param| param.name == *by_ref_name && param.by_ref),
            "eval builtin {} lists {} as by-ref without marking the parameter",
            spec.name,
            by_ref_name
        );
    }
    if let Some(variadic) = spec.variadic {
        assert_eq!(
            spec.param_names.last().copied(),
            Some(variadic),
            "eval builtin {} has a variadic name out of sync",
            spec.name
        );
    }
    let _ = spec.area();
}

/// Returns the declarative registry, initializing it on first access.
fn declared_builtin_registry() -> &'static DeclaredBuiltinRegistry {
    DECLARED_BUILTIN_REGISTRY.get_or_init(build_declared_builtin_registry)
}

/// Looks up a declaratively migrated eval builtin with PHP case-insensitive matching.
pub(in crate::interpreter) fn eval_declared_builtin_spec(
    name: &str,
) -> Option<&'static EvalBuiltinSpec> {
    let key = name.trim_start_matches('\\').to_ascii_lowercase();
    declared_builtin_registry().by_name.get(&key).copied()
}

/// Returns whether a PHP-visible builtin has migrated into the declarative registry.
pub(in crate::interpreter) fn eval_declared_builtin_exists(name: &str) -> bool {
    eval_declared_builtin_spec(name).is_some()
}

/// Returns stable canonical names for builtins in the declarative registry.
pub(in crate::interpreter) fn eval_declared_builtin_function_names() -> &'static [&'static str] {
    declared_builtin_registry().names.as_slice()
}

/// Returns PHP parameter names for a declaratively migrated builtin.
pub(in crate::interpreter) fn eval_declared_builtin_param_names(
    name: &str,
) -> Option<&'static [&'static str]> {
    eval_declared_builtin_spec(name).map(|spec| spec.param_names)
}

/// Returns a default value from a declaratively migrated builtin spec.
pub(in crate::interpreter) fn eval_declared_builtin_default_value(
    name: &str,
    param_index: usize,
) -> Option<EvalBuiltinDefaultValue> {
    eval_declared_builtin_spec(name).and_then(|spec| spec.default_value(param_index))
}

/// Dispatches a declaratively migrated builtin from unevaluated positional expressions.
pub(in crate::interpreter) fn eval_declared_builtin_direct_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(spec) = eval_declared_builtin_spec(name) else {
        return Ok(None);
    };
    let Some(hook) = spec.direct else {
        return Ok(None);
    };
    hook.call(spec.name, args, context, scope, values).map(Some)
}

/// Dispatches a declaratively migrated builtin from already evaluated argument cells.
pub(in crate::interpreter) fn eval_declared_builtin_values_call(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(spec) = eval_declared_builtin_spec(name) else {
        return Ok(None);
    };
    let Some(hook) = spec.values else {
        return Ok(None);
    };
    hook.call(spec.name, evaluated_args, context, values)
        .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies representative migrated builtins are present in the declarative registry.
    #[test]
    fn declared_builtin_registry_exposes_representative_migrated_builtins() {
        for name in [
            "abs",
            "acos",
            "addslashes",
            "array_key_exists",
            "array_keys",
            "array_reverse",
            "array_sum",
            "basename",
            "boolval",
            "base64_encode",
            "bin2hex",
            "checkdate",
            "chdir",
            "chgrp",
            "chmod",
            "chown",
            "closedir",
            "clearstatcache",
            "copy",
            "count",
            "ctype_alpha",
            "date",
            "date_default_timezone_get",
            "date_default_timezone_set",
            "dirname",
            "disk_free_space",
            "disk_total_space",
            "file",
            "file_exists",
            "file_get_contents",
            "file_put_contents",
            "fileatime",
            "filectime",
            "filegroup",
            "fileinode",
            "filemtime",
            "fileowner",
            "fileperms",
            "filesize",
            "filetype",
            "floatval",
            "fnmatch",
            "getdate",
            "getcwd",
            "gettype",
            "gmdate",
            "gmmktime",
            "glob",
            "grapheme_strrev",
            "hash_equals",
            "hex2bin",
            "htmlspecialchars",
            "hrtime",
            "intval",
            "is_array",
            "is_bool",
            "is_dir",
            "is_double",
            "is_executable",
            "is_file",
            "is_finite",
            "is_float",
            "is_infinite",
            "is_int",
            "is_integer",
            "is_iterable",
            "is_link",
            "is_long",
            "is_nan",
            "is_null",
            "is_numeric",
            "is_object",
            "is_readable",
            "is_real",
            "is_resource",
            "is_scalar",
            "is_string",
            "is_writable",
            "is_writeable",
            "json_decode",
            "json_encode",
            "json_last_error",
            "json_last_error_msg",
            "json_validate",
            "lchgrp",
            "lchown",
            "link",
            "linkinfo",
            "localtime",
            "log",
            "lstat",
            "microtime",
            "min",
            "mkdir",
            "mktime",
            "nl2br",
            "number_format",
            "opendir",
            "pathinfo",
            "pclose",
            "popen",
            "preg_match",
            "preg_match_all",
            "preg_replace",
            "preg_replace_callback",
            "preg_split",
            "range",
            "rawurlencode",
            "readdir",
            "readfile",
            "readlink",
            "realpath",
            "realpath_cache_get",
            "realpath_cache_size",
            "rename",
            "rewinddir",
            "rmdir",
            "scandir",
            "sleep",
            "stat",
            "stream_get_filters",
            "stream_get_transports",
            "stream_get_wrappers",
            "stream_is_local",
            "stream_isatty",
            "stream_resolve_include_path",
            "stream_set_blocking",
            "stream_set_chunk_size",
            "stream_set_read_buffer",
            "stream_set_timeout",
            "stream_set_write_buffer",
            "stream_supports_lock",
            "str_contains",
            "str_pad",
            "str_replace",
            "strlen",
            "str_repeat",
            "strrev",
            "strtotime",
            "substr",
            "symlink",
            "sys_get_temp_dir",
            "tempnam",
            "time",
            "tmpfile",
            "touch",
            "trim",
            "strval",
            "umask",
            "unlink",
            "usleep",
            "wordwrap",
        ] {
            assert!(
                eval_declared_builtin_exists(name),
                "{name} should be registered declaratively"
            );
        }
    }

    /// Verifies migrated builtin metadata is derived from declarative specs.
    #[test]
    fn declared_builtin_registry_derives_migrated_metadata() {
        assert_eq!(
            eval_declared_builtin_param_names("count"),
            Some(["value", "mode"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("count", 1),
            Some(EvalBuiltinDefaultValue::Int(0))
        );
        assert_eq!(
            eval_declared_builtin_param_names("strlen"),
            Some(["string"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("is_finite"),
            Some(["num"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("is_object"),
            Some(["value"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("log"),
            Some(["num", "base"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("log", 1),
            Some(EvalBuiltinDefaultValue::Float(std::f64::consts::E))
        );
        assert_eq!(
            eval_declared_builtin_param_names("max"),
            Some(["value", "values"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("number_format"),
            Some(
                [
                    "num",
                    "decimals",
                    "decimal_separator",
                    "thousands_separator",
                ]
                .as_slice()
            )
        );
        assert_eq!(
            eval_declared_builtin_param_names("ctype_alpha"),
            Some(["text"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("str_repeat"),
            Some(["string", "times"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("wordwrap"),
            Some(["string", "width", "break", "cut_long_words"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("wordwrap", 2),
            Some(EvalBuiltinDefaultValue::String("\n"))
        );
        assert_eq!(
            eval_declared_builtin_param_names("json_decode"),
            Some(["json", "associative", "depth", "flags"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("json_decode", 1),
            Some(EvalBuiltinDefaultValue::Null)
        );
        assert_eq!(
            eval_declared_builtin_default_value("json_encode", 2),
            Some(EvalBuiltinDefaultValue::Int(512))
        );
        assert_eq!(
            eval_declared_builtin_param_names("json_last_error"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("date"),
            Some(["format", "timestamp"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("date", 1),
            Some(EvalBuiltinDefaultValue::Null)
        );
        assert_eq!(
            eval_declared_builtin_param_names("date_default_timezone_get"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("getdate"),
            Some(["timestamp"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("hrtime"),
            Some(["as_number"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("hrtime", 0),
            Some(EvalBuiltinDefaultValue::Bool(false))
        );
        assert_eq!(
            eval_declared_builtin_default_value("localtime", 0),
            Some(EvalBuiltinDefaultValue::Null)
        );
        assert_eq!(
            eval_declared_builtin_default_value("localtime", 1),
            Some(EvalBuiltinDefaultValue::Bool(false))
        );
        assert_eq!(
            eval_declared_builtin_param_names("microtime"),
            Some(["as_float"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("strtotime"),
            Some(["datetime", "baseTimestamp"].as_slice())
        );
        assert_eq!(eval_declared_builtin_param_names("time"), Some([].as_slice()));
        assert_eq!(
            eval_declared_builtin_param_names("preg_match"),
            Some(["pattern", "subject", "matches", "flags"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("preg_match", 2),
            Some(EvalBuiltinDefaultValue::EmptyArray)
        );
        assert_eq!(
            eval_declared_builtin_default_value("preg_match_all", 3),
            Some(EvalBuiltinDefaultValue::Int(0))
        );
        assert_eq!(
            eval_builtin_signature_shape("preg_match").map(|shape| shape.by_ref_params),
            Some(["matches"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("preg_replace_callback"),
            Some(["pattern", "callback", "subject"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("preg_split", 2),
            Some(EvalBuiltinDefaultValue::Int(-1))
        );
        assert_eq!(
            eval_declared_builtin_param_names("basename"),
            Some(["path", "suffix"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("basename", 1),
            Some(EvalBuiltinDefaultValue::String(""))
        );
        assert_eq!(
            eval_declared_builtin_default_value("dirname", 1),
            Some(EvalBuiltinDefaultValue::Int(1))
        );
        assert_eq!(
            eval_declared_builtin_default_value("fnmatch", 2),
            Some(EvalBuiltinDefaultValue::Int(0))
        );
        assert_eq!(
            eval_declared_builtin_default_value("pathinfo", 1),
            Some(EvalBuiltinDefaultValue::Int(15))
        );
        assert_eq!(
            eval_declared_builtin_param_names("disk_free_space"),
            Some(["directory"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("getcwd"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("glob"),
            Some(["pattern"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("linkinfo"),
            Some(["path"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("realpath"),
            Some(["path"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_resolve_include_path"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("realpath_cache_get"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("sys_get_temp_dir"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("file_exists"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("file"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("file_get_contents"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("file_put_contents"),
            Some(["filename", "data"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("readfile"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("filemtime"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("filesize"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("is_writable"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stat"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("chdir"),
            Some(["directory"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("chmod"),
            Some(["filename", "permissions"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("chown"),
            Some(["filename", "user"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("chgrp"),
            Some(["filename", "group"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("clearstatcache", 0),
            Some(EvalBuiltinDefaultValue::Bool(false))
        );
        assert_eq!(
            eval_declared_builtin_default_value("clearstatcache", 1),
            Some(EvalBuiltinDefaultValue::String(""))
        );
        assert_eq!(
            eval_declared_builtin_param_names("link"),
            Some(["target", "link"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("rename"),
            Some(["from", "to"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("scandir"),
            Some(["directory"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("tempnam"),
            Some(["directory", "prefix"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("popen"),
            Some(["command", "mode"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("pclose"),
            Some(["handle"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("opendir"),
            Some(["directory"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("closedir"),
            Some(["dir_handle"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("readdir"),
            Some(["dir_handle"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("rewinddir"),
            Some(["dir_handle"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("tmpfile"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_get_wrappers"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_get_transports"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_get_filters"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_is_local"),
            Some(["stream"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_supports_lock"),
            Some(["stream"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_isatty"),
            Some(["stream"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_set_blocking"),
            Some(["stream", "enable"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_set_chunk_size"),
            Some(["stream", "size"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_set_read_buffer"),
            Some(["stream", "size"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_set_write_buffer"),
            Some(["stream", "size"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_set_timeout"),
            Some(["stream", "seconds", "microseconds"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("stream_set_timeout", 2),
            Some(EvalBuiltinDefaultValue::Int(0))
        );
        assert_eq!(
            eval_declared_builtin_default_value("touch", 1),
            Some(EvalBuiltinDefaultValue::Null)
        );
        assert_eq!(
            eval_declared_builtin_default_value("umask", 0),
            Some(EvalBuiltinDefaultValue::Null)
        );
    }
}
