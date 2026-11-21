//! Purpose:
//! Checks the physical array transport of native method and constructor collectors.
//!
//! Called from:
//! - Magician's native-scope interpreter tests.
//!
//! Key details:
//! - By-value EIR collectors use hashes even when empty or entirely positional.
//! - Reference collectors retain their distinct indexed-cell transport.

use super::*;
use crate::context::{NativeCallableShape, NativeCallableSignature};

/// Matches generated constructor/method storage without changing count metadata or ownership.
#[test]
fn native_collector_storage_matches_generated_eir_transport() {
    for source_variadic in [false, true] {
        for by_ref in [false, true] {
            let mut signature = NativeCallableSignature::new(1);
            assert!(signature.set_variadic_index(0));
            assert!(signature.set_param_by_ref(0, by_ref));
            assert!(signature.set_param_name(0, if source_variadic { "rest" } else { "" }));
            signature.set_shape(NativeCallableShape::new(0, 0, source_variadic, !source_variadic));
            let mut context = ElephcEvalContext::new();
            let mut values = FakeOps::default();
            assert!(context.define_native_constructor_signature("KnownClass", signature));
            let mut scope = ElephcEvalScope::new();
            let program = parse_fragment(b"return new KnownClass();").unwrap();
            let object = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
                .expect("construct with empty collector");
            let FakeValue::Object(properties) = values.get(object) else { panic!("expected object") };
            let collector = FakeOps::object_property(&properties, "x").expect("fake constructor exposes its first argument");
            if by_ref {
                assert!(matches!(values.get(collector), FakeValue::Array(_)));
            } else {
                assert!(matches!(values.get(collector), FakeValue::Assoc(_)), "native by-value collector must not depend on whether PHP supplied a string key");
            }
            assert_eq!(values.array_len(collector).unwrap(), usize::from(!source_variadic));
            if !source_variadic {
                let key = values.int(0).unwrap();
                let count = values.array_get(collector, key).unwrap();
                assert_eq!(values.get(count), FakeValue::Int(0));
                values.release(key).unwrap();
                values.release(count).unwrap();
            }
            assert_eq!(values.releases.iter().filter(|&&cell| cell == collector).count(), 1);
        }
    }
}
