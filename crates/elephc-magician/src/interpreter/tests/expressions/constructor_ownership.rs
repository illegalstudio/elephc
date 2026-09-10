//! Purpose:
//! Checks temporary ownership during eval construction and native default binding.
//!
//! Called from:
//! - Magician's focused interpreter expression tests.
//!
//! Key details:
//! - Fake release records cover success, binding failure, constructor failure, and borrowed caller values.
//! - Native codegen tests separately verify actual reference counts and retained object fields.

use super::super::super::*;
use super::super::support::*;

/// Releases literal arguments after successful construction and after later argument or constructor failure.
#[test]
fn constructor_ownership_releases_literal_arguments() {
    for source in [
        "return new KnownClass(731);",
        "return new KnownFailingConstructor(731);",
        "return new KnownClass(731, missing_constructor_argument());",
        "$class = 'KnownClass'; return new $class(731);",
    ] {
        let program = parse_fragment(source.as_bytes()).expect("parse constructor fixture");
        let mut values = FakeOps::default();
        let mut scope = ElephcEvalScope::new();
        let _ = execute_program(&program, &mut scope, &mut values);
        let owners: Vec<_> = values.values.iter()
            .filter(|(_, value)| **value == FakeValue::Int(731))
            .map(|(id, _)| *id).collect();
        assert_eq!(owners.len(), 1, "{source}");
        assert_eq!(values.releases.iter().filter(|value| value.as_ptr() as usize == owners[0]).count(), 1, "{source}");
    }
}

/// Keeps a caller variable alive while releasing defaults on success and both native failure paths.
#[test]
fn constructor_ownership_releases_defaults_without_releasing_borrowed_arguments() {
    for (class, reject_type) in [("KnownClass", false), ("KnownFailingConstructor", false), ("KnownClass", true)] {
        let mut values = FakeOps::default();
        let borrowed = values.int(997).expect("allocate caller value");
        let mut scope = ElephcEvalScope::new();
        scope.set("value", borrowed, ScopeCellOwnership::Borrowed);
        let mut context = ElephcEvalContext::new();
        let mut signature = NativeCallableSignature::new(3);
        for (index, name) in ["value", "code", "tail"].iter().enumerate() {
            assert!(signature.set_param_name(index, *name));
        }
        assert!(signature.set_param_default(1, NativeCallableDefault::Int(731)));
        assert!(signature.set_param_default(2, NativeCallableDefault::Int(732)));
        if reject_type {
            assert!(signature.set_param_type(2, EvalParameterType::new(
                vec![EvalParameterTypeVariant::Class("ExpectedObject".to_string())], false,
            )));
        }
        assert!(context.define_native_constructor_signature(class, signature));
        let source = format!("return new {class}($value);");
        let program = parse_fragment(source.as_bytes()).expect("parse constructor fixture");
        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values);
        assert_eq!(result.is_ok(), class == "KnownClass" && !reject_type);
        assert!(!values.releases.contains(&borrowed));
        for expected in [731, 732] {
            let owners: Vec<_> = values.values.iter()
                .filter(|(_, value)| **value == FakeValue::Int(expected))
                .map(|(id, _)| *id).collect();
            assert_eq!(owners.len(), 1);
            assert_eq!(values.releases.iter().filter(|value| value.as_ptr() as usize == owners[0]).count(), 1);
        }
    }
}
