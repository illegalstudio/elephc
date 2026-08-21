//! Purpose:
//! Ownership-operation tests for AST-to-EIR local assignment lowering.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - Verifies the Phase 03 ownership surface emits explicit acquire/release
//!   markers for refcounted local values before the future EIR backend exists.

use crate::ir::{print_module, Op, Ownership, ValueDef};
use crate::ir_lower::context::php_types_can_alias_storage;
use crate::types::PhpType;

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
    let builtin = text
        .find("runtime.array_column")
        .expect("expected typed array_column runtime call in lowered IR");
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

/// Verifies static-property stores persist concat scratch bytes before later reuse.
#[test]
fn static_string_property_concat_store_acquires_persistent_storage() {
    let module = super::lower_source(
        r#"<?php
class Collector {
    public static string $written = "";

    public static function append(string $data): void {
        self::$written .= $data;
    }
}
"#,
    );
    let text = print_module(&module);
    let concat = text
        .find("str_concat")
        .expect("expected static-property string concat");
    let assignment = &text[concat..];
    let acquire = assignment
        .find("acquire")
        .expect("expected concat storage acquire");
    let store = assignment
        .find("store_static_property")
        .expect("expected static-property store");
    assert!(
        acquire < store,
        "expected concat acquire before static-property store in {text}"
    );
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
        .find("runtime.trim")
        .expect("expected typed trim runtime call");
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

/// Verifies a user-call result that aliases a borrowed Mixed argument is not released.
#[test]
fn borrowed_user_call_result_is_not_treated_as_an_owning_temporary() {
    let module = super::lower_source(
        r#"<?php
function identity(mixed $value): mixed { return $value; }
$values = [1];
$value = array_pop($values);
echo identity($value);
echo $value;
"#,
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("expected main EIR function");
    let result = function
        .instructions
        .iter()
        .find(|inst| inst.op == Op::Call)
        .and_then(|inst| inst.result)
        .expect("expected the identity user-call result");
    assert!(
        function
            .instructions
            .iter()
            .all(|inst| inst.op != Op::Release || inst.operands.first().copied() != Some(result)),
        "a user-call result borrowed from a local argument must not be released"
    );
    assert_ne!(
        function.value(result).expect("call result metadata").ownership,
        Ownership::Owned,
        "borrowed call results must not publish an owning EIR contract"
    );
}

/// Verifies fresh boxed producers publish `Owned` instead of requiring codegen inference.
#[test]
fn fresh_boxed_producers_publish_owned_eir_metadata() {
    let module = super::lower_source(
        r#"<?php
function checked_add(int $value): mixed { return $value + 1; }
function boxed_scalar(int $value): mixed { return $value; }
function scratch_string(int $value): string { return "v" . $value; }
echo checked_add(1);
echo boxed_scalar(2);
echo scratch_string(3);
"#,
    );

    let mut observed = Vec::new();
    for function in &module.functions {
        for inst in &function.instructions {
            if !matches!(inst.op, Op::ICheckedAdd | Op::MixedBox) {
                continue;
            }
            let result = inst.result.expect("owning producer must have a result");
            let ownership = function
                .value(result)
                .expect("owning producer result metadata")
                .ownership;
            observed.push((inst.op, ownership));
            assert_eq!(
                ownership,
                Ownership::Owned,
                "{} must publish owned EIR metadata",
                inst.op.name()
            );
            assert_eq!(
                inst.result_ownership,
                Ownership::Owned,
                "{} instruction metadata must match its result value",
                inst.op.name()
            );
        }
    }

    assert!(
        observed.iter().any(|(op, _)| *op == Op::ICheckedAdd),
        "expected a checked-add producer"
    );
    assert!(
        observed.iter().any(|(op, _)| *op == Op::MixedBox),
        "expected a MixedBox producer"
    );

    let scratch_function = module
        .functions
        .iter()
        .find(|function| function.name == "scratch_string")
        .expect("expected scratch_string EIR function");
    let scratch_result = scratch_function
        .instructions
        .iter()
        .find(|inst| inst.op == Op::StrConcat)
        .and_then(|inst| inst.result)
        .expect("expected a scratch string concat result");
    assert_ne!(
        scratch_function
            .value(scratch_result)
            .expect("scratch string metadata")
            .ownership,
        Ownership::Owned,
        "concat scratch storage must retain its string-specific ownership contract"
    );
}

/// Verifies a freshly boxed owned Mixed argument to a callee that returns it is not
/// released as an argument temporary (issue #604). The argument box and the returned
/// box are the same allocation, so the caller must let ownership flow through the
/// result; releasing the argument as well frees the box once too often.
#[test]
fn owned_mixed_argument_returned_by_callee_is_not_released_as_arg_temp() {
    let module = super::lower_source(
        r#"<?php
function idv(mixed $value): mixed { return $value; }
function run(int $i): void {
    $r = idv($i + 1);
    echo $r;
}
run(5);
"#,
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "run")
        .expect("expected the run EIR function");
    let argument = function
        .instructions
        .iter()
        .find(|inst| inst.op == Op::Call)
        .and_then(|inst| inst.operands.first().copied())
        .expect("expected the idv call argument value");
    assert!(
        function
            .instructions
            .iter()
            .all(|inst| inst.op != Op::Release || inst.operands.first().copied() != Some(argument)),
        "a fresh owned Mixed argument returned by the callee must not also be released as an arg temporary"
    );
}

/// Verifies disjoint nullable unions do not suppress an owned argument release.
#[test]
fn disjoint_refcounted_unions_do_not_alias_storage() {
    let object_or_null = PhpType::Union(vec![
        PhpType::Object("DOMElement".to_string()),
        PhpType::Void,
    ]);
    let string_or_false = PhpType::Union(vec![PhpType::Str, PhpType::False]);
    assert!(!php_types_can_alias_storage(
        &object_or_null,
        &string_or_false,
    ));
}

/// Verifies recursive union comparison keeps every genuine shared-storage family.
#[test]
fn overlapping_refcounted_unions_still_alias_storage() {
    let node_or_null = PhpType::Union(vec![
        PhpType::Object("DOMNode".to_string()),
        PhpType::Void,
    ]);
    let element_or_false = PhpType::Union(vec![
        PhpType::Object("DOMElement".to_string()),
        PhpType::False,
    ]);
    let mixed_or_null = PhpType::Union(vec![PhpType::Mixed, PhpType::Void]);
    let indexed_array_or_null = PhpType::Union(vec![
        PhpType::Array(Box::new(PhpType::Int)),
        PhpType::Void,
    ]);
    let associative_array_or_false = PhpType::Union(vec![
        PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Str),
        },
        PhpType::False,
    ]);
    let iterable_or_false = PhpType::Union(vec![PhpType::Iterable, PhpType::False]);
    let callable_or_null = PhpType::Union(vec![PhpType::Callable, PhpType::Void]);
    let buffer_or_null = PhpType::Union(vec![
        PhpType::Buffer(Box::new(PhpType::Int)),
        PhpType::Void,
    ]);

    assert!(php_types_can_alias_storage(
        &node_or_null,
        &element_or_false,
    ));
    assert!(php_types_can_alias_storage(
        &mixed_or_null,
        &PhpType::Str,
    ));
    assert!(php_types_can_alias_storage(
        &associative_array_or_false,
        &indexed_array_or_null,
    ));
    assert!(php_types_can_alias_storage(
        &indexed_array_or_null,
        &PhpType::Array(Box::new(PhpType::Str)),
    ));
    assert!(php_types_can_alias_storage(
        &iterable_or_false,
        &PhpType::Object("IteratorAggregate".to_string()),
    ));
    assert!(php_types_can_alias_storage(
        &callable_or_null,
        &PhpType::Callable,
    ));
    assert!(php_types_can_alias_storage(
        &buffer_or_null,
        &PhpType::Buffer(Box::new(PhpType::Str)),
    ));
}

/// Verifies an internal DOM call releases a disjoint nullable node argument.
///
/// `DOMDocument::saveXML()` returns `string|false` while its optional node
/// argument is `DOMElement|null`. Both are lowered as boxed runtime values, but
/// they cannot share a payload; suppressing the release keeps a temporary DOM
/// wrapper alive and changes later php-src object-handle reuse.
#[test]
fn dom_save_xml_releases_disjoint_nullable_node_argument() {
    let module = super::lower_source(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
echo $document->saveXML($document->documentElement);
"#,
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("expected main EIR function");
    let save_xml_argument = function
        .instructions
        .iter()
        .find(|instruction| {
            instruction.op == Op::InternalExtensionCall
                && instruction.result.is_some_and(|result| {
                    matches!(
                        function.value(result).map(|value| &value.php_type),
                        Some(PhpType::Union(members))
                            if members.contains(&PhpType::Str)
                                && members.contains(&PhpType::False)
                    )
                })
                && instruction.operands.iter().any(|operand| {
                    matches!(
                        function.value(*operand).map(|value| &value.php_type),
                        Some(PhpType::Union(members))
                            if members.iter().any(|member| matches!(member, PhpType::Object(_)))
                                && members.contains(&PhpType::Void)
                    )
                })
        })
        .and_then(|instruction| {
            instruction.operands.iter().copied().find(|operand| {
                matches!(
                    function.value(*operand).map(|value| &value.php_type),
                    Some(PhpType::Union(members))
                        if members.iter().any(|member| matches!(member, PhpType::Object(_)))
                            && members.contains(&PhpType::Void)
                )
            })
        })
        .expect("expected DOMDocument::saveXML nullable node argument");
    assert!(
        function.instructions.iter().any(|instruction| {
            instruction.op == Op::Release
                && instruction.operands.first().copied() == Some(save_xml_argument)
        }),
        "saveXML must release a disjoint nullable node argument"
    );
}

/// Verifies `get_class()` releases an owned Mixed container read after class lookup.
#[test]
fn get_class_releases_owned_mixed_argument() {
    let module = super::lower_source(
        r#"<?php
$values = ["object" => new stdClass(), "number" => 1];
echo get_class($values["object"]);
"#,
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("expected main EIR function");
    let argument = function
        .instructions
        .iter()
        .find(|instruction| instruction.op == Op::RuntimeCall)
        .and_then(|instruction| instruction.operands.first().copied())
        .expect("expected get_class runtime argument");
    assert!(
        function.instructions.iter().any(|instruction| {
            instruction.op == Op::Release
                && instruction.operands.first().copied() == Some(argument)
        }),
        "get_class must release its owned Mixed argument after reading object metadata"
    );
}
