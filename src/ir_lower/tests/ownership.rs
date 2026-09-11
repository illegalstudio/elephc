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

/// Eval-backed static and dynamic instanceof probes retire native adapter boxes on every target.
#[test]
fn eval_instanceof_probes_retire_native_metadata_boxes_on_all_targets() {
    let source = r#"<?php
function inspectEvalInstanceofOwners(string $source): void {
    try { eval($source); }
    catch (LogicException $wrong) { echo "wrong"; }
    catch (RuntimeException $right) { echo $right->getMessage(); }
    $box = new RuntimeException("dynamic");
    $target = "RuntimeException";
    echo $box instanceof $target ? "yes" : "no";
}
inspectEvalInstanceofOwners('throw new RuntimeException("right"); // ' . $argc);
"#;
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            target,
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let restore = if name == "linux-x86_64" {
            "add rsp, 96"
        } else {
            "add sp, sp, #96"
        };
        for (symbol, minimum_probes, minimum_cleanup_boxes) in [
            (target.extern_symbol("__elephc_eval_object_is_a"), 2, 1),
            (
                target.extern_symbol("__elephc_eval_object_is_a_dynamic"),
                1,
                2,
            ),
        ] {
            let mut probes = 0;
            for path in asm.split(&format!("{symbol}\n")).skip(1) {
                let cleanup = path
                    .split_once(restore)
                    .expect("instanceof predicate scratch restoration")
                    .0;
                assert!(
                    cleanup
                        .matches("retire temporary eval metadata operand box")
                        .count()
                        >= minimum_cleanup_boxes,
                    "{name}: {symbol}: {cleanup}"
                );
                assert!(
                    cleanup.matches("__rt_decref_mixed").count() >= minimum_cleanup_boxes,
                    "{name}: {symbol}: {cleanup}"
                );
                probes += 1;
            }
            assert!(probes >= minimum_probes, "{name}: {symbol}: {probes}");
        }
    }
}

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

/// Verifies declared-array property appends mutate the borrowed property cell in place.
#[test]
fn property_array_push_borrows_declared_array_cell_for_in_place_mutation() {
    let module = super::lower_source(
        r#"<?php
class C { public array $a; }
$x = new C();
$x->a = [];
$x->a[] = 1;
"#,
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("expected main EIR function");
    let (property_index, property_cell) = function
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, inst)| {
            (inst.op == Op::PropGetForWrite).then_some((index, inst.result?))
        })
        .expect("expected declared-array property load for write");
    assert_eq!(
        function
            .value(property_cell)
            .expect("property cell value metadata")
            .ownership,
        Ownership::Borrowed,
        "the detached cell remains owned by the property slot"
    );
    let (append_index, append) = function
        .instructions
        .iter()
        .enumerate()
        .find(|(_, inst)| inst.op == Op::MixedArrayAppend)
        .expect("expected declared-array property append");
    assert!(
        property_index < append_index,
        "expected the property cell before its in-place append"
    );
    assert_eq!(
        append.operands.first().copied(),
        Some(property_cell),
        "the append must mutate the exact cell returned by PropGetForWrite"
    );
    assert!(
        function.instructions[property_index + 1..append_index]
            .iter()
            .all(|inst| inst.op != Op::Release
                || inst.operands.first().copied() != Some(property_cell)),
        "the borrowed property cell must remain live until the append"
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
