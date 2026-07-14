//! Purpose:
//! Registry metadata tests for string formatting, compression, hash, and stream-setting builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Assertions use registry metadata APIs rather than dispatcher literals.

use super::*;

/// Verifies migrated builtin metadata for this registry area.
#[test]
fn declared_builtin_registry_derives_misc_metadata() {        assert_eq!(
            eval_declared_builtin_param_names("explode"),
            Some(["separator", "string", "limit"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("explode", 2),
            Some(EvalBuiltinDefaultValue::Int(i64::MAX))
        );
        assert_eq!(
            eval_declared_builtin_param_names("implode"),
            Some(["separator", "array"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("implode", 0),
            Some(EvalBuiltinDefaultValue::Null)
        );
        assert_eq!(
            eval_builtin_signature_shape("implode").map(|shape| shape.required_param_count),
            Some(1)
        );
        assert_eq!(
            eval_declared_builtin_param_names("sprintf"),
            Some(["format", "values"].as_slice())
        );
        assert_eq!(
            eval_builtin_signature_shape("sprintf").map(|shape| shape.variadic),
            Some(Some("values"))
        );
        assert_eq!(
            eval_declared_builtin_param_names("sscanf"),
            Some(["string", "format", "vars"].as_slice())
        );
        assert_eq!(
            eval_builtin_signature_shape("sscanf").map(|shape| shape.variadic),
            Some(Some("vars"))
        );
        assert_eq!(
            eval_declared_builtin_param_names("vsprintf"),
            Some(["format", "values"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("gzcompress"),
            Some(["data", "level"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("gzcompress", 1),
            Some(EvalBuiltinDefaultValue::Int(-1))
        );
        assert_eq!(
            eval_declared_builtin_default_value("gzinflate", 1),
            Some(EvalBuiltinDefaultValue::Int(0))
        );
        assert_eq!(
            eval_declared_builtin_param_names("hash"),
            Some(["algo", "data", "binary"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("hash", 2),
            Some(EvalBuiltinDefaultValue::Bool(false))
        );
        assert_eq!(
            eval_declared_builtin_param_names("hash_algos"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("hash_init"),
            Some(["algo", "flags", "key"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("hash_init", 1),
            Some(EvalBuiltinDefaultValue::Int(0))
        );
        assert_eq!(
            eval_declared_builtin_default_value("hash_init", 2),
            Some(EvalBuiltinDefaultValue::String(""))
        );
        assert_eq!(
            eval_declared_builtin_param_names("md5"),
            Some(["string", "binary"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("sha1", 1),
            Some(EvalBuiltinDefaultValue::Bool(false))
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
