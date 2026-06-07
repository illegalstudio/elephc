//! Purpose:
//! Ownership-operation tests for AST-to-EIR local assignment lowering.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - Verifies the Phase 03 ownership surface emits explicit acquire/release
//!   markers for refcounted local values before the future EIR backend exists.

use crate::ir::print_module;

/// Verifies storing a freshly allocated array releases the temporary producer after the store.
#[test]
fn fresh_array_local_assignment_releases_source_after_store() {
    let module = super::lower_source("<?php $a = [1];");
    let text = print_module(&module);
    let store = text.find("store_local").expect("expected local store in lowered IR");
    let release = text.find("release").expect("expected release in lowered IR");
    assert!(text.contains("acquire"), "expected acquire in {text}");
    assert!(store < release, "expected release after store in {text}");
    assert_eq!(text.matches("release").count(), 1, "expected one release in {text}");
}

/// Verifies storing a freshly returned `array_column()` result releases the producer.
#[test]
fn array_column_assignment_releases_source_after_store() {
    let module = super::lower_source(
        r#"<?php
$users = [["name" => "Ada"], ["name" => "Linus"]];
$names = array_column($users, "name");
"#,
    );
    let text = print_module(&module);
    let builtin = text.find("builtin_call").expect("expected array_column call in lowered IR");
    let tail = &text[builtin..];
    let store = tail.find("store_local").expect("expected local store after array_column");
    let release = tail.find("release").expect("expected release after array_column store");
    assert!(store < release, "expected release after store in {text}");
}

/// Verifies nested array literals release refcounted row temporaries after insertion.
#[test]
fn nested_array_literal_releases_pushed_hash_temporary() {
    let module = super::lower_source(r#"<?php $users = [["name" => "Ada"]];"#);
    let text = print_module(&module);
    let push = text.find("array_push").expect("expected row append in lowered IR");
    let tail = &text[push..];
    let release = tail.find("release").expect("expected row release after append");
    assert!(release > 0, "expected release after array_push in {text}");
}

/// Verifies property array rewrites acquire the container before in-place mutation.
#[test]
fn property_array_push_acquires_container_before_rewrite_release() {
    let module = super::lower_source(
        r#"<?php
class C { public array $a; }
$x = new C();
$x->a = [];
$x->a[] = 1;
"#,
    );
    let text = print_module(&module);
    let prop_get = text.find("prop_get").expect("expected property load in lowered IR");
    let tail = &text[prop_get..];
    let acquire = tail.find("acquire").expect("expected property container acquire");
    let push = tail.find("array_push").expect("expected property array push");
    assert!(
        acquire < push,
        "expected property container acquire before array_push in {text}"
    );
}

/// Verifies overwriting a refcounted array local releases the previous value.
#[test]
fn overwriting_array_local_emits_release() {
    let module = super::lower_source("<?php $a = [1]; $a = [2];");
    let text = print_module(&module);
    assert!(text.contains("acquire"), "expected acquire in {text}");
    assert!(text.contains("release"), "expected release in {text}");
    assert_eq!(text.matches("array_new").count(), 2, "expected two arrays in {text}");
}

/// Verifies string locals participate in explicit ownership operations.
#[test]
fn overwriting_string_local_emits_release() {
    let module = super::lower_source(r#"<?php $s = "a"; $s = "b";"#);
    let text = print_module(&module);
    assert!(text.contains("acquire"), "expected acquire in {text}");
    assert!(text.contains("release"), "expected release in {text}");
}

/// Verifies appends into mixed function parameters use an explicit append opcode.
#[test]
fn mixed_parameter_array_push_uses_explicit_opcode() {
    let module = super::lower_source(
        r#"<?php
function add($arr, $value) {
    $arr[] = $value;
    return $arr;
}
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains("mixed_array_append"),
        "expected mixed_array_append for mixed parameter array push in {text}"
    );
}
