//! Purpose:
//! Registry metadata tests for random, scalar formatting, JSON, environment, and time builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Assertions use registry metadata APIs rather than dispatcher literals.

use super::*;

/// Verifies migrated builtin metadata for this registry area.
#[test]
fn declared_builtin_registry_derives_time_and_env_metadata() {        for name in ["rand", "mt_rand", "random_int"] {
            assert_eq!(
                eval_declared_builtin_param_names(name),
                Some(["min", "max"].as_slice()),
                "{name} should declare min/max parameters"
            );
        }
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
            eval_declared_builtin_param_names("header"),
            Some(["header", "replace", "response_code"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("header", 1),
            Some(EvalBuiltinDefaultValue::Bool(true))
        );
        assert_eq!(
            eval_declared_builtin_default_value("header", 2),
            Some(EvalBuiltinDefaultValue::Int(0))
        );
        assert_eq!(
            eval_declared_builtin_param_names("http_response_code"),
            Some(["response_code"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("http_response_code", 0),
            Some(EvalBuiltinDefaultValue::Int(0))
        );
        assert_eq!(
            eval_declared_builtin_param_names("php_uname"),
            Some(["mode"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("php_uname", 0),
            Some(EvalBuiltinDefaultValue::String("a"))
        );
        assert_eq!(
            eval_declared_builtin_param_names("phpversion"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("getenv"),
            Some(["name"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("getservbyname"),
            Some(["service", "protocol"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("call_user_func"),
            Some(["callback", "args"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("exit"),
            Some(["status"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("exit", 0),
            Some(EvalBuiltinDefaultValue::Int(0))
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

}
