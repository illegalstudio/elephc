//! Purpose:
//! Provides eval-side method contracts for PHP builtin interfaces.
//! This keeps runtime interface declaration checks aligned with the main checker catalog.
//!
//! Called from:
//! - `crate::interpreter::statements` while registering eval class-like declarations.
//!
//! Key details:
//! - `Traversable` remains marker-only, matching the current checker model.
//! - Child interfaces include their builtin parent method requirements.

use super::*;

/// Returns method requirements for one PHP builtin interface name.
pub(super) fn builtin_interface_method_requirements(
    interface: &str,
) -> Vec<(String, EvalInterfaceMethod)> {
    let interface = interface.trim_start_matches('\\');
    let mut requirements = Vec::new();
    if interface.eq_ignore_ascii_case("Iterator") {
        append_iterator_interface_requirements(&mut requirements);
    } else if interface.eq_ignore_ascii_case("IteratorAggregate") {
        requirements.push((
            String::from("IteratorAggregate"),
            builtin_interface_method(
                "getIterator",
                &[],
                Some(builtin_class_type("Traversable")),
            ),
        ));
    } else if interface.eq_ignore_ascii_case("ArrayAccess") {
        append_array_access_interface_requirements(&mut requirements);
    } else if interface.eq_ignore_ascii_case("Countable") {
        requirements.push((
            String::from("Countable"),
            builtin_interface_method(
                "count",
                &[],
                Some(builtin_type(EvalParameterTypeVariant::Int)),
            ),
        ));
    } else if interface.eq_ignore_ascii_case("OuterIterator") {
        append_iterator_interface_requirements(&mut requirements);
        requirements.push((
            String::from("OuterIterator"),
            builtin_interface_method(
                "getInnerIterator",
                &[],
                Some(nullable_builtin_class_type("Iterator")),
            ),
        ));
    } else if interface.eq_ignore_ascii_case("RecursiveIterator") {
        append_iterator_interface_requirements(&mut requirements);
        requirements.push((
            String::from("RecursiveIterator"),
            builtin_interface_method(
                "getChildren",
                &[],
                Some(nullable_builtin_class_type("RecursiveIterator")),
            ),
        ));
        requirements.push((
            String::from("RecursiveIterator"),
            builtin_interface_method(
                "hasChildren",
                &[],
                Some(builtin_type(EvalParameterTypeVariant::Bool)),
            ),
        ));
    } else if interface.eq_ignore_ascii_case("SeekableIterator") {
        append_iterator_interface_requirements(&mut requirements);
        requirements.push((
            String::from("SeekableIterator"),
            builtin_interface_method(
                "seek",
                &[("offset", builtin_type(EvalParameterTypeVariant::Int))],
                Some(builtin_type(EvalParameterTypeVariant::Void)),
            ),
        ));
    } else if interface.eq_ignore_ascii_case("SplObserver") {
        requirements.push((
            String::from("SplObserver"),
            builtin_interface_method(
                "update",
                &[("subject", builtin_class_type("SplSubject"))],
                Some(builtin_type(EvalParameterTypeVariant::Void)),
            ),
        ));
    } else if interface.eq_ignore_ascii_case("SplSubject") {
        requirements.push((
            String::from("SplSubject"),
            builtin_interface_method(
                "attach",
                &[("observer", builtin_class_type("SplObserver"))],
                Some(builtin_type(EvalParameterTypeVariant::Void)),
            ),
        ));
        requirements.push((
            String::from("SplSubject"),
            builtin_interface_method(
                "detach",
                &[("observer", builtin_class_type("SplObserver"))],
                Some(builtin_type(EvalParameterTypeVariant::Void)),
            ),
        ));
        requirements.push((
            String::from("SplSubject"),
            builtin_interface_method(
                "notify",
                &[],
                Some(builtin_type(EvalParameterTypeVariant::Void)),
            ),
        ));
    } else if interface.eq_ignore_ascii_case("Stringable") {
        requirements.push((
            String::from("Stringable"),
            builtin_interface_method(
                "__toString",
                &[],
                Some(builtin_type(EvalParameterTypeVariant::String)),
            ),
        ));
    } else if interface.eq_ignore_ascii_case("JsonSerializable") {
        requirements.push((
            String::from("JsonSerializable"),
            builtin_interface_method(
                "jsonSerialize",
                &[],
                Some(builtin_type(EvalParameterTypeVariant::Mixed)),
            ),
        ));
    }
    requirements
}

/// Appends the five methods required by PHP's `Iterator` interface.
fn append_iterator_interface_requirements(requirements: &mut Vec<(String, EvalInterfaceMethod)>) {
    for (method, return_type) in [
        ("current", EvalParameterTypeVariant::Mixed),
        ("key", EvalParameterTypeVariant::Mixed),
        ("next", EvalParameterTypeVariant::Void),
        ("valid", EvalParameterTypeVariant::Bool),
        ("rewind", EvalParameterTypeVariant::Void),
    ] {
        requirements.push((
            String::from("Iterator"),
            builtin_interface_method(method, &[], Some(builtin_type(return_type))),
        ));
    }
}

/// Appends the four methods required by PHP's `ArrayAccess` interface.
fn append_array_access_interface_requirements(
    requirements: &mut Vec<(String, EvalInterfaceMethod)>,
) {
    for (method, params, return_type) in [
        (
            "offsetExists",
            vec![("offset", builtin_type(EvalParameterTypeVariant::Mixed))],
            EvalParameterTypeVariant::Bool,
        ),
        (
            "offsetGet",
            vec![("offset", builtin_type(EvalParameterTypeVariant::Mixed))],
            EvalParameterTypeVariant::Mixed,
        ),
        (
            "offsetSet",
            vec![
                ("offset", builtin_type(EvalParameterTypeVariant::Mixed)),
                ("value", builtin_type(EvalParameterTypeVariant::Mixed)),
            ],
            EvalParameterTypeVariant::Void,
        ),
        (
            "offsetUnset",
            vec![("offset", builtin_type(EvalParameterTypeVariant::Mixed))],
            EvalParameterTypeVariant::Void,
        ),
    ] {
        requirements.push((
            String::from("ArrayAccess"),
            builtin_interface_method(method, &params, Some(builtin_type(return_type))),
        ));
    }
}

/// Builds one synthetic eval interface method requirement for a PHP builtin interface.
fn builtin_interface_method(
    name: &str,
    params: &[(&str, EvalParameterType)],
    return_type: Option<EvalParameterType>,
) -> EvalInterfaceMethod {
    EvalInterfaceMethod::new(
        name,
        params
            .iter()
            .map(|(param_name, _)| (*param_name).to_string())
            .collect(),
    )
    .with_parameter_types(
        params
            .iter()
            .map(|(_, param_type)| Some(param_type.clone()))
            .collect(),
    )
    .with_return_type(return_type)
}

/// Returns one non-nullable scalar/object builtin interface type.
fn builtin_type(variant: EvalParameterTypeVariant) -> EvalParameterType {
    EvalParameterType::new(vec![variant], false)
}

/// Returns one non-nullable class/interface builtin interface type.
fn builtin_class_type(name: &str) -> EvalParameterType {
    builtin_type(EvalParameterTypeVariant::Class(name.to_string()))
}

/// Returns one nullable class/interface builtin interface type.
fn nullable_builtin_class_type(name: &str) -> EvalParameterType {
    EvalParameterType::new(vec![EvalParameterTypeVariant::Class(name.to_string())], true)
}
