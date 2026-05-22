//! Purpose:
//! Integration or regression tests for SPL builtin diagnostics.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - These fixtures lock down conservative checker contracts that codegen can lower safely.

use super::*;

#[test]
fn test_error_spl_autoload_register_wrong_args() {
    expect_error(
        "<?php spl_autoload_register(null, true, false, 1);",
        "spl_autoload_register() takes at most 3 arguments",
    );
}

#[test]
fn test_error_spl_autoload_unregister_wrong_args() {
    expect_error(
        "<?php spl_autoload_unregister();",
        "spl_autoload_unregister() takes exactly 1 argument",
    );
}

#[test]
fn test_error_spl_autoload_functions_wrong_args() {
    expect_error(
        "<?php spl_autoload_functions(1);",
        "spl_autoload_functions() takes no arguments",
    );
}

#[test]
fn test_error_spl_autoload_call_wrong_args() {
    expect_error(
        "<?php spl_autoload_call();",
        "spl_autoload_call() takes exactly 1 argument",
    );
}

#[test]
fn test_error_spl_autoload_wrong_args() {
    expect_error(
        "<?php spl_autoload();",
        "spl_autoload() takes 1 or 2 arguments",
    );
}

#[test]
fn test_error_spl_classes_wrong_args() {
    expect_error(
        "<?php spl_classes(1);",
        "spl_classes() takes no arguments",
    );
}

#[test]
fn test_error_spl_autoload_extensions_rejects_int_setter() {
    expect_error(
        "<?php spl_autoload_extensions(123);",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

#[test]
fn test_error_spl_autoload_extensions_rejects_bool_setter() {
    expect_error(
        "<?php spl_autoload_extensions(true);",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

#[test]
fn test_error_spl_autoload_extensions_rejects_array_setter() {
    expect_error(
        "<?php spl_autoload_extensions([\".php\"]);",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

#[test]
fn test_error_spl_autoload_extensions_rejects_object_setter() {
    expect_error(
        "<?php class Box {} spl_autoload_extensions(new Box());",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

#[test]
fn test_error_spl_autoload_extensions_rejects_dynamic_string_setter() {
    expect_error(
        "<?php $ext = \".php\"; spl_autoload_extensions($ext);",
        "spl_autoload_extensions() argument must be a string literal or null",
    );
}

#[test]
fn test_error_spl_object_id_rejects_mixed() {
    expect_error(
        "<?php function id(mixed $value): int { return spl_object_id($value); }",
        "spl_object_id() argument must be an object",
    );
}

#[test]
fn test_error_spl_object_hash_rejects_mixed() {
    expect_error(
        "<?php function hash_value(mixed $value): string { return spl_object_hash($value); }",
        "spl_object_hash() argument must be an object",
    );
}

#[test]
fn test_error_spl_doubly_linked_list_cannot_be_redeclared() {
    expect_error(
        "<?php class SplDoublyLinkedList {}",
        "Cannot redeclare built-in SPL class: SplDoublyLinkedList",
    );
}

#[test]
fn test_error_spl_fixed_array_cannot_be_redeclared() {
    expect_error(
        "<?php class SplFixedArray {}",
        "Cannot redeclare built-in SPL class: SplFixedArray",
    );
}

#[test]
fn test_error_spl_fixed_array_get_iterator_is_deferred_until_iterator_phase() {
    expect_error(
        "<?php $fixed = new SplFixedArray(); $fixed->getIterator();",
        "Undefined method: SplFixedArray::getIterator",
    );
}

#[test]
fn test_error_iterator_count_rejects_non_traversable() {
    expect_error(
        "<?php iterator_count(123);",
        "iterator_count() first argument must be a statically known array or Traversable",
    );
}

#[test]
fn test_error_iterator_to_array_rejects_array_preserve_keys() {
    expect_error(
        "<?php $preserve = []; iterator_to_array([1, 2], $preserve);",
        "iterator_to_array() preserve_keys must be bool-compatible scalar",
    );
}

#[test]
fn test_error_iterator_apply_rejects_array_source() {
    expect_error(
        "<?php function cb(): bool { return true; } iterator_apply([1], \"cb\");",
        "iterator_apply() first argument must be Traversable",
    );
}

#[test]
fn test_error_iterator_apply_rejects_dynamic_assoc_args_array() {
    expect_error(
        r#"<?php
class Range implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return false; }
    public function current(): int { return 0; }
    public function key(): int { return 0; }
    public function next(): void {}
}
function cb(): bool { return true; }
$args = ["name" => "value"];
iterator_apply(new Range(), "cb", $args);
"#,
        "iterator_apply() args must be null, a literal array of scalar literals, or an indexed array value",
    );
}

#[test]
fn test_error_iterator_apply_dynamic_args_require_known_callback_signature() {
    expect_error(
        r#"<?php
class Range implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return false; }
    public function current(): int { return 0; }
    public function key(): int { return 0; }
    public function next(): void {}
}
function make_cb(): callable {
    return function(string $prefix): bool {
        return true;
    };
}
$args = ["value"];
iterator_apply(new Range(), make_cb(), $args);
"#,
        "iterator_apply() dynamic args require a statically known callable signature",
    );
}
