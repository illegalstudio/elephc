//! Purpose:
//! Ownership-operation tests for AST-to-EIR local assignment lowering.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - Verifies the Phase 03 ownership surface emits explicit acquire/release
//!   markers for refcounted local values before the future EIR backend exists.

use crate::ir::{print_module, Op, ValueDef};

/// Returns the printed EIR for `main`, excluding built-in helper and property-init functions.
fn main_function_text(text: &str) -> &str {
    let start = text.find("function main()").expect("expected lowered main function");
    let tail = &text[start..];
    match tail[1..].find("\n  function ") {
        Some(next_function) => &tail[..1 + next_function],
        None => tail,
    }
}

/// Returns the printed EIR slice for one named function.
fn named_function_text<'a>(text: &'a str, name: &str) -> &'a str {
    let needle = format!("function {name}(");
    let start = text.find(&needle).expect("expected named lowered function");
    let tail = &text[start..];
    match tail[1..].find("\n  function ") {
        Some(next_function) => &tail[..1 + next_function],
        None => tail,
    }
}

/// Verifies storing a freshly allocated array releases the temporary producer after the store.
#[test]
fn fresh_array_local_assignment_releases_source_after_store() {
    let module = super::lower_source("<?php $a = [1];");
    let text = print_module(&module);
    let main = main_function_text(&text);
    let store = main.find("store_local").expect("expected local store in lowered IR");
    let release = main.find("release").expect("expected release in lowered IR");
    assert!(main.contains("acquire"), "expected acquire in {text}");
    assert!(store < release, "expected release after store in {text}");
    assert_eq!(main.matches("release").count(), 1, "expected one release in {text}");
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
    let main = main_function_text(&text);
    assert!(main.contains("acquire"), "expected acquire in {text}");
    assert!(main.contains("release"), "expected release in {text}");
    assert_eq!(main.matches("array_new").count(), 2, "expected two arrays in {text}");
}

/// Verifies string locals participate in explicit ownership operations.
#[test]
fn overwriting_string_local_emits_release() {
    let module = super::lower_source(r#"<?php $s = "a"; $s = "b";"#);
    let text = print_module(&module);
    assert!(text.contains("acquire"), "expected acquire in {text}");
    assert!(text.contains("release"), "expected release in {text}");
}

/// Verifies a borrowed string result is retained before its aliased source slot is released.
#[test]
fn self_reassignment_acquires_borrowed_string_before_releasing_slot() {
    let module = super::lower_source(
        r#"<?php
function normalize(string $value): string {
    $value = trim($value);
    return $value;
}
echo normalize("  hi  ");
"#,
    );
    let text = print_module(&module);
    let function = named_function_text(&text, "normalize");
    let builtin = function
        .find("builtin_call")
        .expect("expected trim builtin call");
    let assignment = &function[builtin..];
    let acquire = assignment.find("acquire").expect("expected retained trim result");
    let release = assignment
        .find("release")
        .expect("expected previous slot release");
    let store = assignment
        .find("store_local")
        .expect("expected replacement local store");
    assert!(
        acquire < release && release < store,
        "expected acquire before old-slot release and store in {function}"
    );
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

/// Stringifying a Mixed local read must not release its slot-backed source.
#[test]
fn mixed_string_cast_does_not_release_local_source() {
    let module = super::lower_source(
        r#"<?php
function render_mixed(mixed $value): string {
    $first = (string) $value;
    return $first . "|" . (string) $value;
}
echo render_mixed(str_repeat("alive", 1));
"#,
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "render_mixed")
        .expect("expected render_mixed EIR function");
    let cast_sources = function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::Cast)
        .filter_map(|inst| inst.operands.first().copied())
        .collect::<Vec<_>>();
    assert_eq!(cast_sources.len(), 2, "expected two Mixed string casts");
    for source in cast_sources {
        assert!(
            function
                .instructions
                .iter()
                .all(|inst| inst.op != Op::Release || inst.operands.first().copied() != Some(source)),
            "a Mixed local read must survive stringification"
        );
    }
}

/// Stringifying an owned Mixed container read must release that exact source value.
#[test]
fn mixed_string_cast_releases_owned_container_read() {
    let module = super::lower_source(
        r#"<?php
$values = ["s" => str_repeat("x", 1), "n" => 1];
echo (string) $values["s"];
"#,
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("expected main EIR function");
    let source = function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::Cast)
        .filter_map(|inst| inst.operands.first().copied())
        .find(|source| {
            let Some(value) = function.value(*source) else {
                return false;
            };
            let ValueDef::Instruction { inst, .. } = value.def else {
                return false;
            };
            function
                .instruction(inst)
                .is_some_and(|inst| matches!(inst.op, Op::ArrayGet | Op::HashGet))
        })
        .expect("expected a Mixed string cast sourced from a container read");
    assert!(
        function
            .instructions
            .iter()
            .any(|inst| inst.op == Op::Release && inst.operands.first().copied() == Some(source)),
        "the owned Mixed container read must be released after stringification"
    );
}
