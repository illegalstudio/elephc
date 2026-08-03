//! Purpose:
//! Regression tests for AST-to-EIR lowering of indexed array expressions.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - Array access result metadata must come from the lowered array value, not
//!   from syntactic fallback inference that lacks local type facts.

use crate::ir::print_module;

/// Verifies indexed array access preserves string and float element metadata.
/// The indices are runtime-unknown (`$argc`) so the accesses survive AST-level
/// array-fact propagation, which folds constant-index reads of literal-backed
/// locals before lowering.
#[test]
fn indexed_array_access_uses_array_element_type() {
    let module = super::lower_source(
        r#"<?php
$strings = ["a", "b"];
echo $strings[$argc];
$floats = [1.5, 2.5];
echo $floats[$argc];
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains(": Str php=string own=maybe_owned = array_get"),
        "missing string array_get metadata in {text}"
    );
    assert!(
        text.contains(": F64 php=float = array_get"),
        "missing float array_get metadata in {text}"
    );
}

/// Verifies an associative local append is represented by the typed
/// `HashAppend` opcode instead of an unclassified `RuntimeCall`, allowing every
/// backend to enforce the same hash-specific ownership and failure semantics.
#[test]
fn associative_array_append_uses_typed_hash_append() {
    let module = super::lower_source(
        r#"<?php
$items = [PHP_INT_MAX => 1];
$items[] = 2;
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains("hash_append"),
        "missing typed hash_append in {text}"
    );
    assert!(
        !text.contains("runtime_call"),
        "associative append retained an unclassified runtime_call in {text}"
    );
}
