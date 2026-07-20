//! Purpose:
//! Registry metadata tests for stream and stream-socket builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Assertions use registry metadata APIs rather than dispatcher literals.

use super::*;

/// Verifies migrated builtin metadata for this registry area.
#[test]
fn declared_builtin_registry_derives_stream_metadata() {        assert_eq!(
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
        for name in [
            "fclose",
            "fgetc",
            "fgets",
            "feof",
            "fflush",
            "fpassthru",
            "fsync",
            "fdatasync",
            "ftell",
            "rewind",
            "fstat",
            "stream_get_meta_data",
        ] {
            assert_eq!(
                eval_declared_builtin_param_names(name),
                Some(["stream"].as_slice()),
                "{name} should declare one stream parameter"
            );
        }
        assert_eq!(
            eval_declared_builtin_param_names("fread"),
            Some(["stream", "length"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("fgetcsv"),
            Some(["stream", "length", "separator"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("fgetcsv", 2),
            Some(EvalBuiltinDefaultValue::String(","))
        );
        assert_eq!(
            eval_declared_builtin_param_names("flock"),
            Some(["stream", "operation", "would_block"].as_slice())
        );
        assert_eq!(
            eval_builtin_signature_shape("flock").map(|shape| shape.by_ref_params),
            Some(["would_block"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("fsockopen"),
            Some(["hostname", "port", "error_code", "error_message", "timeout"].as_slice())
        );
        assert_eq!(
            eval_builtin_signature_shape("fsockopen").map(|shape| shape.by_ref_params),
            Some(["error_code", "error_message"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("fwrite"),
            Some(["stream", "data"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("fseek"),
            Some(["stream", "offset", "whence"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("fseek", 2),
            Some(EvalBuiltinDefaultValue::Int(0))
        );
        assert_eq!(
            eval_declared_builtin_param_names("ftruncate"),
            Some(["stream", "size"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_copy_to_stream"),
            Some(["from", "to", "length", "offset"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("stream_copy_to_stream", 2),
            Some(EvalBuiltinDefaultValue::Null)
        );
        assert_eq!(
            eval_declared_builtin_default_value("stream_copy_to_stream", 3),
            Some(EvalBuiltinDefaultValue::Int(-1))
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_get_contents"),
            Some(["stream", "length", "offset"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("stream_get_contents", 1),
            Some(EvalBuiltinDefaultValue::Null)
        );
        assert_eq!(
            eval_declared_builtin_default_value("stream_get_contents", 2),
            Some(EvalBuiltinDefaultValue::Int(-1))
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_context_set_option"),
            Some(["context", "wrapper_or_options", "option_name", "value"].as_slice())
        );
        assert_eq!(
            eval_builtin_signature_shape("stream_context_set_option")
                .map(|shape| shape.required_param_count),
            Some(2)
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_get_line"),
            Some(["stream", "length", "ending"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("stream_get_line", 2),
            Some(EvalBuiltinDefaultValue::String(""))
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_select"),
            Some(["read", "write", "except", "seconds", "microseconds"].as_slice())
        );
        assert_eq!(
            eval_builtin_signature_shape("stream_select").map(|shape| shape.by_ref_params),
            Some(["read", "write", "except"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_socket_recvfrom"),
            Some(["socket", "length", "flags", "address"].as_slice())
        );
        assert_eq!(
            eval_builtin_signature_shape("stream_socket_recvfrom")
                .map(|shape| shape.by_ref_params),
            Some(["address"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_wrapper_register"),
            Some(["protocol", "class", "flags"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("vfprintf"),
            Some(["stream", "format", "values"].as_slice())
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

}
