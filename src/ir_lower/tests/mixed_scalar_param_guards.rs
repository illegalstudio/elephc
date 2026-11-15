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
