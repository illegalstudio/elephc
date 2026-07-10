//! Purpose:
//! Shares static metadata for dynamic object construction with EIR codegen.
//! Keeps the builtin class allow-list independent from object-expression lowering.
//!
//! Called from:
//! - `crate::codegen_support::collect_dynamic_object_factory_classes_in_expr()`.
//!
//! Key details:
//! - These classes have known allocation/runtime layouts and can be included in
//!   emitted class metadata for `new $name` factory paths.

/// Returns builtin class names with allocation paths that are safe for dynamic `new`.
pub(crate) fn supported_dynamic_new_builtin_class_names() -> &'static [&'static str] {
    &[
        "ArrayIterator",
        "ArrayObject",
        "BadFunctionCallException",
        "BadMethodCallException",
        "CallbackFilterIterator",
        "DomainException",
        "Error",
        "Exception",
        "Fiber",
        "FiberError",
        "InvalidArgumentException",
        "IteratorIterator",
        "JsonException",
        "LengthException",
        "LogicException",
        "OutOfBoundsException",
        "OutOfRangeException",
        "OverflowException",
        "RangeException",
        "RecursiveCallbackFilterIterator",
        "ReflectionClass",
        "ReflectionMethod",
        "ReflectionProperty",
        "RuntimeException",
        "SplDoublyLinkedList",
        "SplFixedArray",
        "SplQueue",
        "SplStack",
        "TypeError",
        "UnderflowException",
        "UnexpectedValueException",
        "ValueError",
    ]
}
