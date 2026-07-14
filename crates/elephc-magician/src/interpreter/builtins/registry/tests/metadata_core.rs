//! Purpose:
//! Registry metadata tests for core, arrays, class, callable, and raw-memory builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Assertions use registry metadata APIs rather than dispatcher literals.

use super::*;

/// Verifies migrated builtin metadata for this registry area.
#[test]
fn declared_builtin_registry_derives_core_metadata() {
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
            eval_declared_builtin_param_names("array_map"),
            Some(["callback", "array", "arrays"].as_slice())
        );
        assert_eq!(
            eval_builtin_signature_shape("array_map").map(|shape| shape.variadic),
            Some(Some("arrays"))
        );
        assert_eq!(
            eval_declared_builtin_param_names("array_filter"),
            Some(["array", "callback", "mode"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("array_filter", 1),
            Some(EvalBuiltinDefaultValue::Null)
        );
        assert_eq!(
            eval_declared_builtin_default_value("array_filter", 2),
            Some(EvalBuiltinDefaultValue::Int(0))
        );
        assert_eq!(
            eval_declared_builtin_param_names("iterator_to_array"),
            Some(["iterator", "preserve_keys"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("iterator_to_array", 1),
            Some(EvalBuiltinDefaultValue::Bool(true))
        );
        assert_eq!(
            eval_declared_builtin_spec("array_pop").map(EvalBuiltinSpec::by_ref_param_names),
            Some(["array"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("array_push"),
            Some(["array", "values"].as_slice())
        );
        assert_eq!(
            eval_builtin_signature_shape("array_push").map(|shape| shape.variadic),
            Some(Some("values"))
        );
        assert_eq!(
            eval_declared_builtin_default_value("array_splice", 2),
            Some(EvalBuiltinDefaultValue::Null)
        );
        assert_eq!(
            eval_declared_builtin_default_value("array_splice", 3),
            Some(EvalBuiltinDefaultValue::EmptyArray)
        );
        assert_eq!(
            eval_declared_builtin_param_names("settype"),
            Some(["var", "type"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_spec("settype").map(EvalBuiltinSpec::by_ref_param_names),
            Some(["var"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("print_r"),
            Some(["value", "return"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("print_r", 1),
            Some(EvalBuiltinDefaultValue::Bool(false))
        );
        assert_eq!(
            eval_declared_builtin_param_names("var_dump"),
            Some(["value", "values"].as_slice())
        );
        assert_eq!(
            eval_builtin_signature_shape("var_dump").map(|shape| shape.variadic),
            Some(Some("values"))
        );
        assert_eq!(
            eval_declared_builtin_param_names("class_exists"),
            Some(["class", "autoload"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("class_exists", 1),
            Some(EvalBuiltinDefaultValue::Bool(true))
        );
        assert_eq!(
            eval_declared_builtin_param_names("get_class"),
            Some(["object"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("get_class", 0),
            Some(EvalBuiltinDefaultValue::Null)
        );
        assert_eq!(
            eval_declared_builtin_param_names("is_callable"),
            Some(["value", "syntax_only", "callable_name"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_spec("is_callable").map(EvalBuiltinSpec::by_ref_param_names),
            Some(["callable_name"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("is_callable", 1),
            Some(EvalBuiltinDefaultValue::Bool(false))
        );
        assert_eq!(
            eval_builtin_signature_shape("isset").map(|shape| shape.variadic),
            Some(Some("vars"))
        );
        assert_eq!(
            eval_declared_builtin_param_names("spl_autoload_register"),
            Some(["callback", "throw", "prepend"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("spl_autoload_register", 2),
            Some(EvalBuiltinDefaultValue::Bool(false))
        );
        assert_eq!(
            eval_declared_builtin_param_names("buffer_new"),
            Some(["length"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("ptr_read_string"),
            Some(["pointer", "length"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("ptr_sizeof"),
            Some(["type"].as_slice())
        );

}
