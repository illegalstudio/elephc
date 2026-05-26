//! Purpose:
//! Integration or regression tests for SPL builtin diagnostics.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - These fixtures lock down conservative checker contracts that codegen can lower safely.

use super::*;

// Tests that `spl_autoload_register()` rejects more than 3 arguments.
// Fixture: 4 arguments passed to a function that accepts at most 3.
/// Verifies that error SPL autoload register wrong args.
#[test]
fn test_error_spl_autoload_register_wrong_args() {
    expect_error(
        "<?php spl_autoload_register(null, true, false, 1);",
        "spl_autoload_register() takes at most 3 arguments",
    );
}

// Tests that `spl_autoload_unregister()` requires exactly 1 argument.
// Fixture: zero arguments passed to a function that requires 1.
/// Verifies that error SPL autoload unregister wrong args.
#[test]
fn test_error_spl_autoload_unregister_wrong_args() {
    expect_error(
        "<?php spl_autoload_unregister();",
        "spl_autoload_unregister() takes exactly 1 argument",
    );
}

// Tests that `spl_autoload_functions()` takes no arguments.
// Fixture: 1 argument passed to a parameterless function.
/// Verifies that error SPL autoload functions wrong args.
#[test]
fn test_error_spl_autoload_functions_wrong_args() {
    expect_error(
        "<?php spl_autoload_functions(1);",
        "spl_autoload_functions() takes no arguments",
    );
}

// Tests that `spl_autoload_call()` requires exactly 1 argument.
// Fixture: zero arguments passed to a function that requires 1.
/// Verifies that error SPL autoload call wrong args.
#[test]
fn test_error_spl_autoload_call_wrong_args() {
    expect_error(
        "<?php spl_autoload_call();",
        "spl_autoload_call() takes exactly 1 argument",
    );
}

// Tests that `spl_autoload()` requires 1 or 2 arguments.
// Fixture: zero arguments passed to a function that requires 1 or 2.
/// Verifies that error SPL autoload wrong args.
#[test]
fn test_error_spl_autoload_wrong_args() {
    expect_error(
        "<?php spl_autoload();",
        "spl_autoload() takes 1 or 2 arguments",
    );
}

// Tests that `spl_classes()` takes no arguments.
// Fixture: 1 argument passed to a parameterless function.
/// Verifies that error SPL classes wrong args.
#[test]
fn test_error_spl_classes_wrong_args() {
    expect_error(
        "<?php spl_classes(1);",
        "spl_classes() takes no arguments",
    );
}

// Tests that `spl_autoload_extensions()` rejects integer as first argument.
// The setter form only accepts a string literal or null.
/// Verifies that error SPL autoload extensions rejects integer setter.
#[test]
fn test_error_spl_autoload_extensions_rejects_int_setter() {
    expect_error(
        "<?php spl_autoload_extensions(123);",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

// Tests that `spl_autoload_extensions()` rejects boolean as first argument.
// The setter form only accepts a string literal or null.
/// Verifies that error SPL autoload extensions rejects boolean setter.
#[test]
fn test_error_spl_autoload_extensions_rejects_bool_setter() {
    expect_error(
        "<?php spl_autoload_extensions(true);",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

// Tests that `spl_autoload_extensions()` rejects array as first argument.
// The setter form only accepts a string literal or null.
/// Verifies that error SPL autoload extensions rejects array setter.
#[test]
fn test_error_spl_autoload_extensions_rejects_array_setter() {
    expect_error(
        "<?php spl_autoload_extensions([\".php\"]);",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

// Tests that `spl_autoload_extensions()` rejects object as first argument.
// The setter form only accepts a string literal or null.
/// Verifies that error SPL autoload extensions rejects object setter.
#[test]
fn test_error_spl_autoload_extensions_rejects_object_setter() {
    expect_error(
        "<?php class Box {} spl_autoload_extensions(new Box());",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

// Tests that `spl_autoload_extensions()` rejects a dynamic string variable.
// The setter form only accepts a string literal or null, not a runtime value.
/// Verifies that error SPL autoload extensions rejects dynamic string setter.
#[test]
fn test_error_spl_autoload_extensions_rejects_dynamic_string_setter() {
    expect_error(
        "<?php $ext = \".php\"; spl_autoload_extensions($ext);",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

// Tests that `spl_object_id()` argument must be an object.
// Fixture: typed `mixed` parameter in a user function, passed a non-object.
/// Verifies that error SPL object ID rejects mixed.
#[test]
fn test_error_spl_object_id_rejects_mixed() {
    expect_error(
        "<?php function id(mixed $value): int { return spl_object_id($value); }",
        "spl_object_id() argument must be an object",
    );
}

// Tests that `spl_object_hash()` argument must be an object.
// Fixture: typed `mixed` parameter in a user function, passed a non-object.
/// Verifies that error SPL object hash rejects mixed.
#[test]
fn test_error_spl_object_hash_rejects_mixed() {
    expect_error(
        "<?php function hash_value(mixed $value): string { return spl_object_hash($value); }",
        "spl_object_hash() argument must be an object",
    );
}

// Tests that `SplDoublyLinkedList` cannot be redeclared as a user class.
// Built-in SPL classes are reserved and reject redefinition.
/// Verifies that error SPL doubly linked list cannot be redeclared.
#[test]
fn test_error_spl_doubly_linked_list_cannot_be_redeclared() {
    expect_error(
        "<?php class SplDoublyLinkedList {}",
        "Cannot redeclare built-in SPL class: SplDoublyLinkedList",
    );
}

// Tests that `SplFixedArray` cannot be redeclared as a user class.
// Built-in SPL classes are reserved and reject redefinition.
/// Verifies that error SPL fixed array cannot be redeclared.
#[test]
fn test_error_spl_fixed_array_cannot_be_redeclared() {
    expect_error(
        "<?php class SplFixedArray {}",
        "Cannot redeclare built-in SPL class: SplFixedArray",
    );
}

// Tests that `SplFixedArray::getIterator()` is deferred until the iterator phase.
// The method exists at codegen time but the error is raised at runtime iteration.
/// Verifies that error internal iterator cannot be redeclared.
#[test]
fn test_error_internal_iterator_cannot_be_redeclared() {
    expect_error(
        "<?php class InternalIterator {}",
        "Cannot redeclare built-in SPL class: InternalIterator",
    );
}

/// Verifies that error internal iterator constructor is private.
#[test]
fn test_error_internal_iterator_constructor_is_private() {
    expect_error(
        "<?php $fixed = new SplFixedArray(1); $it = new InternalIterator($fixed);",
        "Cannot access private constructor: InternalIterator::__construct",
    );
}

/// Verifies that error filter iterator is abstract.
#[test]
fn test_error_filter_iterator_is_abstract() {
    expect_error(
        "<?php $it = new FilterIterator(new ArrayIterator([]));",
        "Cannot instantiate abstract class: FilterIterator",
    );
}

/// Verifies that error recursive filter iterator is abstract.
#[test]
fn test_error_recursive_filter_iterator_is_abstract() {
    expect_error(
        "<?php $it = new RecursiveFilterIterator(new RecursiveArrayIterator([]));",
        "Cannot instantiate abstract class: RecursiveFilterIterator",
    );
}

/// Verifies that error callback filter iterator requires callable.
#[test]
fn test_error_callback_filter_iterator_requires_callable() {
    expect_error(
        "<?php $it = new CallbackFilterIterator(new ArrayIterator([]), 123);",
        "Constructor 'CallbackFilterIterator::__construct' parameter $callback expects Callable, got Int",
    );
}

/// Verifies that error recursive callback filter iterator requires callable.
#[test]
fn test_error_recursive_callback_filter_iterator_requires_callable() {
    expect_error(
        "<?php $it = new RecursiveCallbackFilterIterator(new RecursiveArrayIterator([]), 123);",
        "Constructor 'RecursiveCallbackFilterIterator::__construct' parameter $callback expects Callable, got Int",
    );
}

/// Verifies that error recursive iterator iterator requires recursive iterator.
#[test]
fn test_error_recursive_iterator_iterator_requires_recursive_iterator() {
    expect_error(
        "<?php $it = new RecursiveIteratorIterator(new ArrayIterator([]));",
        "Constructor 'RecursiveIteratorIterator::__construct' parameter $iterator expects Object(\"RecursiveIterator\"), got Object(\"ArrayIterator\")",
    );
}

/// Verifies that error iterator count rejects non traversable.
#[test]
fn test_error_iterator_count_rejects_non_traversable() {
    expect_error(
        "<?php iterator_count(123);",
        "iterator_count() first argument must be a statically known array or Traversable",
    );
}

/// Verifies that error iterator to array rejects array preserve keys.
#[test]
fn test_error_iterator_to_array_rejects_array_preserve_keys() {
    expect_error(
        "<?php $preserve = []; iterator_to_array([1, 2], $preserve);",
        "iterator_to_array() preserve_keys must be bool-compatible scalar",
    );
}

/// Verifies that error iterator apply rejects array source.
#[test]
fn test_error_iterator_apply_rejects_array_source() {
    expect_error(
        "<?php function cb(): bool { return true; } iterator_apply([1], \"cb\");",
        "iterator_apply() first argument must be Traversable",
    );
}
