//! Purpose:
//! Models the small fake class-name, namespace, Throwable, and inheritance graph
//! used by interpreter tests.
//!
//! Called from:
//! - Fake object construction, reflection, and `is_a` runtime operations.
//!
//! Key details:
//! - Matching is case-insensitive to mirror PHP class-like names.

/// Returns whether a fake runtime class stores PHP Throwable constructor state.
pub(super) fn fake_runtime_exception_like_class(class_name: &str) -> bool {
    [
        "Exception",
        "JsonException",
        "ReflectionException",
        "Error",
        "ValueError",
        "TypeError",
    ]
    .iter()
    .any(|known| class_name.eq_ignore_ascii_case(known))
}

/// Splits one PHP class-like name into namespace and short-name parts.
pub(super) fn reflection_name_parts(reflected_name: &str) -> (&str, &str) {
    match reflected_name.rfind('\\') {
        Some(separator) => (
            &reflected_name[..separator],
            &reflected_name[separator + 1..],
        ),
        None => ("", reflected_name),
    }
}

/// Checks the small fake Throwable inheritance graph used by eval interpreter tests.
pub(super) fn fake_runtime_object_is_a(class_name: &str, target_class: &str, exclude_self: bool) -> bool {
    if class_name.eq_ignore_ascii_case(target_class) {
        return !exclude_self;
    }
    if class_name.eq_ignore_ascii_case("KnownClass")
        && target_class.eq_ignore_ascii_case("ParentClass")
    {
        return true;
    }
    if class_name.eq_ignore_ascii_case("KnownClass")
        && target_class.eq_ignore_ascii_case("KnownInterface")
    {
        return true;
    }
    if class_name.eq_ignore_ascii_case("ReflectionObject")
        && target_class.eq_ignore_ascii_case("ReflectionClass")
    {
        return true;
    }
    if target_class.eq_ignore_ascii_case("Throwable") {
        return fake_runtime_exception_like_class(class_name);
    }
    if target_class.eq_ignore_ascii_case("Exception") {
        return ["Exception", "JsonException", "ReflectionException"]
            .iter()
            .any(|known| class_name.eq_ignore_ascii_case(known));
    }
    if target_class.eq_ignore_ascii_case("Error") {
        return ["Error", "ValueError", "TypeError"]
            .iter()
            .any(|known| class_name.eq_ignore_ascii_case(known));
    }
    false
}
