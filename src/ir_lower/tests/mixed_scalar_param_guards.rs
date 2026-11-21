//! Purpose:
//! EIR shape coverage for the runtime check on a `mixed` argument bound to a declared scalar.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The guard is one `ScalarParamGuard` instruction on the boxed argument, adding no basic
//!   block; statically typed arguments, nullable parameters and builtins must not pay for it.

use crate::ir::Op;

/// Counts scalar-parameter guards emitted in one lowered function.
fn scalar_predicates(module: &crate::ir::Module, name: &str) -> usize {
    let function = module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing function {name}"));
    function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::ScalarParamGuard)
        .count()
}

/// Verifies only the `mixed` argument bound to a non-nullable scalar parameter is checked:
/// typed arguments, a `?string` parameter, a `= null` default and a builtin emit no predicate.
#[test]
fn mixed_argument_to_scalar_parameter_is_guarded_only_where_needed() {
    let module = super::lower_source(
        r#"<?php
function take(string $s): string { return $s; }
function takeNullable(?string $s): string { return (string) $s; }
function takeDefault(string $s = null): string { return (string) $s; }
function fromMixed(mixed $v): string { return take($v); }
function fromString(string $v, int $i): string { return take($v) . take($i) . take("x"); }
function intoNullable(mixed $v): string { return takeNullable($v) . takeDefault($v); }
function intoBuiltin(mixed $v): int { return strlen($v); }
echo fromMixed("a"), fromString("b", 1), intoNullable(null), intoBuiltin("c");
"#,
    );
    assert_eq!(scalar_predicates(&module, "fromMixed"), 1);
    let from_mixed = module.functions.iter().find(|function| function.name == "fromMixed").unwrap();
    assert_eq!(from_mixed.blocks.len(), 1, "the guard must not split the caller's block");
    assert_eq!(scalar_predicates(&module, "fromString"), 0);
    assert_eq!(scalar_predicates(&module, "intoNullable"), 0);
    assert_eq!(scalar_predicates(&module, "intoBuiltin"), 0);
}

/// Verifies every supported target lowers the guard with its rejection in the cold section:
/// the hot path names the cold label, and the type-error construction follows it.
#[test]
fn scalar_param_guard_emits_out_of_line_rejection_on_every_target() {
    let source = r#"<?php
class Name { public function __toString(): string { return "n"; } }
function take(string $s): string { return $s; }
function takeInt(int $i): int { return $i; }
function fromMixed(mixed $v): string { return take($v) . takeInt($v); }
echo fromMixed(new Name());
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
        let lines: Vec<&str> = asm.lines().collect();
        let branch = lines.iter().position(|line| line.contains("scalar_param_guard_cold")
            && !line.trim_end().ends_with(':')).expect("hot path branches to the cold label");
        let label = lines.iter().position(|line| line.contains("scalar_param_guard_cold")
            && line.trim_end().ends_with(':')).expect("cold label is defined");
        let marker = lines.iter().position(|line| line.contains("cold paths kept out of the hot fallthrough"));
        assert!(label > branch, "{target}: rejection must follow the hot branch");
        if let Some(marker) = marker {
            assert!(label > marker, "{target}: rejection must sit in the cold section");
        }
        assert!(asm.contains("__rt_mixed_instanceof") || asm.contains("eval_object_is_a"),
            "{target}: coercive string guard must accept Stringable objects");
    }
}

/// Rewires each `int`/`float` guard in `main` to a raw operand of `raw_op` and returns the
/// generated assembly for `target`, simulating a pass that retyped the boxed operand.
fn asm_with_raw_guard_operand(source: &str, raw_op: Op, target: &str) -> String {
    let mut module = super::lower_source_at_for_target(
        source,
        std::path::Path::new("main.php"),
        std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let main = module.functions.iter_mut().find(|function| function.name == "main").unwrap();
    let raw = main.instructions.iter().find(|inst| inst.op == raw_op)
        .and_then(|inst| inst.result).expect("raw constant operand");
    let mut rewired = 0;
    for inst in main.instructions.iter_mut().filter(|inst| inst.op == Op::ScalarParamGuard) {
        inst.operands[0] = raw;
        rewired += 1;
    }
    assert!(rewired > 0, "{target}: the fixture must emit a guard");
    crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"))
}

/// A raw string or float reaching an `int`/`float` guard is converted with PHP's rules
/// (numeric scan, precision diagnostics, range refusal), never stored as its register bits.
#[test]
fn raw_string_and_float_guard_operands_are_converted_not_reinterpreted() {
    let string_source = r#"<?php
function ti(int $i): int { return $i; }
function tf(float $f): float { return $f; }
function m(mixed $v): mixed { return $v; }
echo ti(m("1.5")), tf(m("2.5"));
"#;
    let float_source = r#"<?php
function ti(int $i): int { return $i; }
function m(mixed $v): mixed { return $v; }
echo ti(m(1.5));
"#;
    for target in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
        let asm = asm_with_raw_guard_operand(string_source, Op::ConstStr, target);
        let main = &asm[asm.find("main:").unwrap_or(0)..];
        assert!(!main.contains("scalar_param_guard_cold"), "{target}: raw operands take the static path");
        assert!(main.contains("__rt_str_to_number"), "{target}: a raw string must be scanned");
        assert!(main.contains("scalar_param_guard_static_refuse"), "{target}: a non-numeric string must throw");
        let asm = asm_with_raw_guard_operand(float_source, Op::ConstF64, target);
        let main = &asm[asm.find("main:").unwrap_or(0)..];
        assert!(main.contains("float_to_int_diag_exact"), "{target}: a raw float must go through precision diagnostics");
    }
}

/// The string conversion that follows a deferred positional guard carries the call's span.
///
/// A conversion that can warn publishes its span as the current diagnostic line, so a
/// span-less one published line 0. Inside an injected prelude body (where the call's own span
/// is compiler-generated and never published) that overwrote the user's call-site line, and a
/// later exception reported line 0.
#[test]
fn deferred_guard_string_conversion_keeps_the_call_span() {
    let module = super::lower_source(
        r#"<?php
function take(string $s): string { return $s; }
function fromMixed(mixed $v): string {
    return take($v);
}
echo fromMixed("a");
"#,
    );
    let function = module.functions.iter().find(|function| function.name == "fromMixed").unwrap();
    let guarded = function.instructions.iter().find(|inst| inst.op == Op::ScalarParamGuard)
        .map(|guard| guard.operands[0]).expect("the mixed argument is guarded");
    let conversion = function.instructions.iter()
        .find(|inst| inst.op == Op::Cast && inst.operands.first() == Some(&guarded))
        .expect("the guarded box converts to the string parameter");
    let span = conversion.span.expect("the conversion carries a span");
    assert_eq!(span.line, 4, "the conversion reports the call's line");
}

/// An accepted `string` check on an owned box costs what `main`'s unchecked conversion costs:
/// one persisted copy of the converted string (the call's own operand root) and no retained box.
///
/// The box is moved into its unwind slot instead of being retained there, and the converted
/// string, when no later check can throw, stays the owned operand the call roots. Publishing it
/// before the call made the call persist a second copy, which cost a third more allocations and
/// about 45% on a `len($words[$i & 3])` loop.
#[test]
fn accepted_string_guard_persists_the_converted_string_once() {
    let module = super::lower_source(
        r#"<?php
function take(string $s): int { return strlen($s); }
function walk(mixed $words): int { return take($words[1]); }
function walkTwo(mixed $words): int { return take2($words[1], $words[2]); }
function take2(string $a, string $b): int { return strlen($a . $b); }
echo walk(["a", "b"]), walkTwo(["a", "b", "c"]);
"#,
    );
    let function = module.functions.iter().find(|function| function.name == "walk").unwrap();
    assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::ScalarParamGuard).count(), 1);
    let value_type = |function: &crate::ir::Function, value| function.value(value).map(|value| value.ir_type);
    let string_acquires = function.instructions.iter()
        .filter(|inst| inst.op == Op::Acquire
            && inst.result.is_some_and(|value| value_type(function, value) == Some(crate::ir::IrType::Str)))
        .count();
    assert_eq!(string_acquires, 1, "the converted string is persisted once, by the call");
    // The guard reads a borrow of the element box itself, which its slot owns without an
    // `Acquire`.
    let guarded = function.instructions.iter().find(|inst| inst.op == Op::ScalarParamGuard)
        .map(|guard| guard.operands[0]).unwrap();
    let borrow = function.instructions.iter().find(|inst| inst.result == Some(guarded)).unwrap();
    assert_eq!(borrow.op, Op::Borrow);
    let element = borrow.operands[0];
    assert!(function.instructions.iter().any(|inst| inst.result == Some(element) && inst.op != Op::Acquire),
        "the guarded box is the element read, not a retained copy");
    assert!(!function.instructions.iter().any(|inst| inst.op == Op::Acquire && inst.operands == [element]),
        "the guarded box is moved into its slot, not retained");
    // A string converted before a later check that can throw is still published for it.
    let two = module.functions.iter().find(|function| function.name == "walkTwo").unwrap();
    assert!(two.instructions.iter().any(|inst| inst.op == Op::Acquire
        && inst.immediate == Some(crate::ir::Immediate::Bool(true))
        && inst.result.is_some_and(|value| value_type(two, value) == Some(crate::ir::IrType::Str))),
        "the first converted string is published before the second check");
}
