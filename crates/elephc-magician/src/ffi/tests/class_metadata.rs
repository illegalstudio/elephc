//! Purpose:
//! Tests native parent, property, interface, abstract-property, default, and
//! attribute metadata registration.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Every metadata family is read back through `ElephcEvalContext`.

use super::*;

/// Verifies native AOT parent metadata is available for eval static-scope resolution.
#[test]
fn register_native_class_parent_records_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let class = b"KnownChild";
    let parent = b"KnownParent";

    let registered = unsafe {
        __elephc_eval_register_native_class_parent(
            &mut ctx,
            class.as_ptr(),
            class.len() as u64,
            parent.as_ptr(),
            parent.len() as u64,
        )
    };

    assert_eq!(registered, 1);
    assert_eq!(ctx.native_class_parent("knownchild"), Some("KnownParent"));
}

/// Verifies native AOT property type metadata is available to eval reflection.
#[test]
fn register_native_property_type_records_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let property = b"KnownClass::name";
    let property_type = b"?KnownDep";
    let invalid_property = b"KnownClass::bad";
    let invalid_type = b"void";

    let registered = unsafe {
        __elephc_eval_register_native_property_type(
            &mut ctx,
            property.as_ptr(),
            property.len() as u64,
            property_type.as_ptr(),
            property_type.len() as u64,
        )
    };
    let invalid_registered = unsafe {
        __elephc_eval_register_native_property_type(
            &mut ctx,
            invalid_property.as_ptr(),
            invalid_property.len() as u64,
            invalid_type.as_ptr(),
            invalid_type.len() as u64,
        )
    };

    assert_eq!(registered, 1);
    let property_type = ctx
        .native_property_type("knownclass", "name")
        .expect("property type metadata");
    assert!(property_type.allows_null());
    assert_eq!(
        property_type.variants(),
        &[EvalParameterTypeVariant::Class("KnownDep".to_string())]
    );
    assert_eq!(invalid_registered, 0);
    assert!(ctx.native_property_type("KnownClass", "bad").is_none());
}

/// Verifies native AOT interface property contracts are available to eval validation.
#[test]
fn register_native_interface_property_records_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let property = b"KnownContract::KnownParentContract::name";
    let property_type = b"string";
    let invalid_property = b"KnownContract::KnownParentContract::bad";
    let invalid_type = b"void";

    let registered = unsafe {
        __elephc_eval_register_native_interface_property(
            &mut ctx,
            property.as_ptr(),
            property.len() as u64,
            property_type.as_ptr(),
            property_type.len() as u64,
            TEST_NATIVE_PROPERTY_REQUIRES_GET | TEST_NATIVE_PROPERTY_REQUIRES_SET,
        )
    };
    let invalid_registered = unsafe {
        __elephc_eval_register_native_interface_property(
            &mut ctx,
            invalid_property.as_ptr(),
            invalid_property.len() as u64,
            invalid_type.as_ptr(),
            invalid_type.len() as u64,
            TEST_NATIVE_PROPERTY_REQUIRES_GET,
        )
    };

    let requirements = ctx.native_interface_property_requirements("knowncontract");
    assert_eq!(registered, 1);
    assert_eq!(requirements.len(), 1);
    assert_eq!(requirements[0].0, "KnownParentContract");
    assert_eq!(requirements[0].1.name(), "name");
    assert!(requirements[0].1.requires_get());
    assert!(requirements[0].1.requires_set());
    assert_eq!(
        requirements[0].1.property_type().map(|ty| ty.variants()),
        Some(&[EvalParameterTypeVariant::String][..])
    );
    assert_eq!(invalid_registered, 0);
    assert!(ctx
        .native_interface_property_requirements("KnownContract")
        .iter()
        .all(|(_, property)| property.name() != "bad"));
}

/// Verifies native AOT abstract class property contracts are available to eval validation.
#[test]
fn register_native_abstract_property_records_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let property = b"KnownClass::KnownParent::name";
    let property_type = b"string";
    let invalid_property = b"KnownClass::KnownParent::bad";
    let invalid_type = b"void";

    let registered = unsafe {
        __elephc_eval_register_native_abstract_property(
            &mut ctx,
            property.as_ptr(),
            property.len() as u64,
            property_type.as_ptr(),
            property_type.len() as u64,
            TEST_NATIVE_PROPERTY_REQUIRES_GET | TEST_NATIVE_PROPERTY_REQUIRES_SET,
        )
    };
    let invalid_registered = unsafe {
        __elephc_eval_register_native_abstract_property(
            &mut ctx,
            invalid_property.as_ptr(),
            invalid_property.len() as u64,
            invalid_type.as_ptr(),
            invalid_type.len() as u64,
            TEST_NATIVE_PROPERTY_REQUIRES_GET,
        )
    };

    let requirements = ctx.native_abstract_property_requirements("knownclass");
    assert_eq!(registered, 1);
    assert_eq!(requirements.len(), 1);
    assert_eq!(requirements[0].0, "KnownParent");
    assert_eq!(requirements[0].1.name(), "name");
    assert!(requirements[0].1.requires_get());
    assert!(requirements[0].1.requires_set());
    assert_eq!(
        requirements[0].1.property_type().map(|ty| ty.variants()),
        Some(&[EvalParameterTypeVariant::String][..])
    );
    assert_eq!(invalid_registered, 0);
    assert!(ctx
        .native_abstract_property_requirements("KnownClass")
        .iter()
        .all(|(_, property)| property.name() != "bad"));
}

/// Verifies native AOT property default metadata is available to eval reflection.
#[test]
fn register_native_property_default_records_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let count = b"KnownClass::count";
    let label = b"KnownClass::label";
    let invalid = b"KnownClass::invalid";
    let label_value = b"ok";

    let scalar_registered = unsafe {
        __elephc_eval_register_native_property_default_scalar(
            &mut ctx,
            count.as_ptr(),
            count.len() as u64,
            2,
            42,
        )
    };
    let string_registered = unsafe {
        __elephc_eval_register_native_property_default_string(
            &mut ctx,
            label.as_ptr(),
            label.len() as u64,
            label_value.as_ptr(),
            label_value.len() as u64,
        )
    };
    let invalid_registered = unsafe {
        __elephc_eval_register_native_property_default_scalar(
            &mut ctx,
            invalid.as_ptr(),
            invalid.len() as u64,
            99,
            0,
        )
    };

    assert_eq!(scalar_registered, 1);
    assert_eq!(
        ctx.native_property_default("knownclass", "count"),
        Some(NativeCallableDefault::Int(42))
    );
    assert_eq!(string_registered, 1);
    assert_eq!(
        ctx.native_property_default("KnownClass", "label"),
        Some(NativeCallableDefault::String("ok".to_string()))
    );
    assert_eq!(invalid_registered, 0);
    assert!(ctx
        .native_property_default("KnownClass", "invalid")
        .is_none());
}

/// Verifies native AOT member attributes are available to eval reflection.
#[test]
fn register_native_member_attribute_records_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let method_record = native_member_attribute_record(
        0,
        "KnownClass::run",
        "Route",
        Some(&[
            EvalAttributeArg::String("api".to_string()),
            EvalAttributeArg::Named {
                name: "path".to_string(),
                value: Box::new(EvalAttributeArg::String("/users".to_string())),
            },
            EvalAttributeArg::Int(7),
            EvalAttributeArg::Float(1.5f64.to_bits()),
            EvalAttributeArg::Array(vec![
                EvalAttributeArg::Int(1),
                EvalAttributeArg::String("two".to_string()),
            ]),
            EvalAttributeArg::Bool(true),
            EvalAttributeArg::Null,
        ]),
    );
    let property_record = native_member_attribute_record(1, "KnownClass::id", "Column", None);
    let constant_record = native_member_attribute_record(
        2,
        "KnownClass::LIMIT",
        "Limit",
        Some(&[EvalAttributeArg::Int(100)]),
    );
    let class_record = native_member_attribute_record(
        3,
        "KnownClass",
        "Entity",
        Some(&[EvalAttributeArg::String("model".to_string())]),
    );
    let invalid_record = [99, 0, 0, 0, 0];

    let method_registered = unsafe {
        __elephc_eval_register_native_member_attribute(
            &mut ctx,
            method_record.as_ptr(),
            method_record.len() as u64,
        )
    };
    let property_registered = unsafe {
        __elephc_eval_register_native_member_attribute(
            &mut ctx,
            property_record.as_ptr(),
            property_record.len() as u64,
        )
    };
    let constant_registered = unsafe {
        __elephc_eval_register_native_member_attribute(
            &mut ctx,
            constant_record.as_ptr(),
            constant_record.len() as u64,
        )
    };
    let class_registered = unsafe {
        __elephc_eval_register_native_member_attribute(
            &mut ctx,
            class_record.as_ptr(),
            class_record.len() as u64,
        )
    };
    let invalid_registered = unsafe {
        __elephc_eval_register_native_member_attribute(
            &mut ctx,
            invalid_record.as_ptr(),
            invalid_record.len() as u64,
        )
    };

    assert_eq!(method_registered, 1);
    let method_attributes = ctx.native_method_attributes("knownclass", "RUN");
    assert_eq!(method_attributes.len(), 1);
    assert_eq!(method_attributes[0].name(), "Route");
    assert_eq!(
        method_attributes[0].args(),
        Some(
            [
                EvalAttributeArg::String("api".to_string()),
                EvalAttributeArg::Named {
                    name: "path".to_string(),
                    value: Box::new(EvalAttributeArg::String("/users".to_string())),
                },
                EvalAttributeArg::Int(7),
                EvalAttributeArg::Float(1.5f64.to_bits()),
                EvalAttributeArg::Array(vec![
                    EvalAttributeArg::Int(1),
                    EvalAttributeArg::String("two".to_string()),
                ]),
                EvalAttributeArg::Bool(true),
                EvalAttributeArg::Null,
            ]
            .as_slice()
        )
    );
    assert_eq!(property_registered, 1);
    let property_attributes = ctx.native_property_attributes("KnownClass", "id");
    assert_eq!(property_attributes.len(), 1);
    assert_eq!(property_attributes[0].name(), "Column");
    assert!(property_attributes[0].args().is_none());
    assert_eq!(constant_registered, 1);
    let constant_attributes = ctx.native_constant_attributes("KnownClass", "LIMIT");
    assert_eq!(constant_attributes.len(), 1);
    assert_eq!(constant_attributes[0].name(), "Limit");
    assert_eq!(
        constant_attributes[0].args(),
        Some([EvalAttributeArg::Int(100)].as_slice())
    );
    assert_eq!(class_registered, 1);
    let class_attributes = ctx.native_class_attributes("knownclass");
    assert_eq!(class_attributes.len(), 1);
    assert_eq!(class_attributes[0].name(), "Entity");
    assert_eq!(
        class_attributes[0].args(),
        Some([EvalAttributeArg::String("model".to_string())].as_slice())
    );
    assert_eq!(invalid_registered, 0);
}
