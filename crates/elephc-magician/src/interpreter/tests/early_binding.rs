//! Purpose:
//! Interpreter tests for early binding: the top-level declarations a program binds before its
//! first statement runs, and the ones it leaves to their statement.
//!
//! Called from:
//! - `cargo test -p elephc-magician early_binding` through Rust's test harness.
//!
//! Key details:
//! - Every expected string was produced by running the same fragment under PHP 8.5.12.
//! - Functions are covered on both executors: a fragment the bytecode slice admits, and one a
//!   class declaration keeps on the tree walker.
//! - Jumps across declarations are covered in `jumps.rs`.

use super::super::*;
use super::support::*;

/// Runs one fragment on the fake runtime and returns what it echoed.
fn run(source: &[u8]) -> String {
    let program = parse_fragment(source).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    values.output
}

/// Runs one fragment that must fail and returns what it echoed before failing.
fn run_failing(source: &[u8]) -> String {
    let program = parse_fragment(source).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert!(execute_program(&program, &mut scope, &mut values).is_err());
    values.output
}

/// Verifies a top-level function can be called before its declaration on the bytecode slice.
#[test]
fn early_binding_calls_function_before_its_declaration_on_the_vm() {
    let program = parse_fragment(br#"echo f(); function f() { return "x"; }"#).expect("parse");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let _ = execute_program(&program, &mut scope, &mut values).expect("execute");
    assert!(bytecode::last_execution_used_vm());
    assert_eq!(values.output, "x");
}

/// Verifies a top-level function can be called before its declaration on the tree walker.
#[test]
fn early_binding_calls_function_before_its_declaration_on_the_tree_walker() {
    let program =
        parse_fragment(br#"echo f(); class Gap {} function f() { return "x"; }"#).expect("parse");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let _ = execute_program(&program, &mut scope, &mut values).expect("execute");
    assert!(!bytecode::last_execution_used_vm());
    assert_eq!(values.output, "x");
}

/// Verifies a top-level function whose name is taken stops the fragment before any statement
/// runs, on both executors, as PHP reports it while compiling.
#[test]
fn early_binding_reports_function_redeclaration_before_the_first_statement() {
    for source in [
        &br#"echo "a"; function f() {}"#[..],
        &br#"echo "a"; class Gap {} function f() {}"#[..],
    ] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let setup = parse_fragment(b"function f() {}").expect("parse setup");
        let result = execute_program_with_context(&mut context, &setup, &mut scope, &mut values).unwrap();
        eval_release_value(&mut context, &mut values, result).unwrap();
        let program = parse_fragment(source).expect("parse");
        assert!(execute_program_with_context(&mut context, &program, &mut scope, &mut values).is_err());
        assert_eq!(values.output, "");
    }
    assert_eq!(run_failing(br#"echo "a"; function h() {} function h() {}"#), "");
}

/// Verifies a function declared inside a block or another function is declared only when its
/// statement runs.
#[test]
fn early_binding_leaves_nested_functions_to_their_statement() {
    let output = run(
        br#"echo function_exists("g") ? "yes" : "no"; if (true) { function g() {} } echo function_exists("g") ? "yes" : "no";"#,
    );
    assert_eq!(output, "noyes");
    let output = run(
        br#"echo function_exists("inner") ? "yes" : "no"; function outer() { function inner() {} } echo function_exists("inner") ? "yes" : "no"; outer(); echo function_exists("inner") ? "yes" : "no";"#,
    );
    assert_eq!(output, "nonoyes");
}

/// Verifies a plain class, interface and trait exist before their statement runs.
#[test]
fn early_binding_binds_plain_class_interface_and_trait() {
    let output = run(
        br#"echo class_exists("C") ? "yes" : "no", interface_exists("I") ? "yes" : "no", trait_exists("T") ? "yes" : "no"; class C {} interface I {} trait T {}"#,
    );
    assert_eq!(output, "yesyesyes");
    let output = run(br#"echo trait_exists("TT") ? "yes" : "no"; trait TT { function __toString() { return ""; } }"#);
    assert_eq!(output, "yes");
}

/// Verifies a class is bound early only when its parent is known at that point: a class
/// declared before its parent waits for its statement, one declared after it does not.
#[test]
fn early_binding_binds_child_class_only_after_its_parent() {
    let output = run(
        br#"echo class_exists("K") ? "yes" : "no", class_exists("L") ? "yes" : "no", class_exists("M") ? "yes" : "no"; class K extends L {} class L {} class M extends L {}"#,
    );
    assert_eq!(output, "noyesyes");
}

/// Verifies the shapes PHP links only at run time are declared by their statement: an enum, a
/// class that implements an interface or uses a trait, an interface with a parent, a trait
/// that uses a trait, and a class or interface with `__toString`.
#[test]
fn early_binding_leaves_late_bound_shapes_to_their_statement() {
    let output = run(
        br#"echo enum_exists("E") ? "y" : "n", class_exists("CI") ? "y" : "n", class_exists("CT") ? "y" : "n", interface_exists("J") ? "y" : "n", trait_exists("U") ? "y" : "n", class_exists("S") ? "y" : "n", interface_exists("SI") ? "y" : "n"; interface I {} trait T {} enum E {} class CI implements I {} class CT { use T; } interface J extends I {} trait U { use T; } class S { function __toString() { return ""; } } interface SI { function __toString(); } echo "|", enum_exists("E") ? "y" : "n", class_exists("CI") ? "y" : "n", class_exists("CT") ? "y" : "n", interface_exists("J") ? "y" : "n", trait_exists("U") ? "y" : "n", class_exists("S") ? "y" : "n", interface_exists("SI") ? "y" : "n";"#,
    );
    assert_eq!(output, "nnnnnnn|yyyyyyy");
}

/// Verifies literal constants and static defaults of an early-bound class are readable before
/// its statement.
#[test]
fn early_binding_reads_literal_members_before_the_declaration() {
    let output = run(
        br#"echo C::A, C::$s["k"], C::M; class C { const A = 1; const M = __CLASS__; public static $s = [1, "k" => -2]; }"#,
    );
    assert_eq!(output, "1-2C");
}

/// Verifies binding evaluates no member: a class, interface or trait whose initializers read
/// a constant defined by a later statement is still bound first, and each initializer sees
/// the constant when it is first used.
#[test]
fn early_binding_evaluates_initializers_at_their_first_use() {
    let output = run(br#"define("X", 1); class C { public static $p = X; const K = X; } echo C::$p, C::K;"#);
    assert_eq!(output, "11");
    let output = run(
        br#"echo class_exists("C") ? "y" : "n"; define("X", 1); echo C::K, C::$p; class C { const K = X; public static $p = X + 1; }"#,
    );
    assert_eq!(output, "y12");
    let output = run(
        br#"echo interface_exists("I") ? "y" : "n", trait_exists("T") ? "y" : "n"; define("X", 5); echo I::K; interface I { const K = X; } trait T { const K = X; public static $s = X; }"#,
    );
    assert_eq!(output, "yy5");
    let output = run(
        br#"echo (new C)->p, D::$s; class C { public $p = B::V; } class D extends C { public static $s = self::W; const W = B::V . "w"; } class B { const V = "v"; }"#,
    );
    assert_eq!(output, "vvw");
}

/// Verifies an initializer that cannot be evaluated fails where it is first used, after the
/// statements before that use, and not when its class is bound.
#[test]
fn early_binding_reports_initializer_errors_at_their_first_use() {
    assert_eq!(run_failing(br#"echo "a"; echo C::K; class C { const K = UNDEF; }"#), "a");
    assert_eq!(run_failing(br#"echo "a", C::K; define("X", 1); class C { const K = X; }"#), "a");
}

/// Verifies a class constant can name a class declared later in the fragment.
#[test]
fn early_binding_resolves_constant_of_a_later_class() {
    let output = run(br#"class A { const X = B::Y; } class B { const Y = 1; } echo A::X;"#);
    assert_eq!(output, "1");
}

/// Verifies a class-like whose name is taken is not bound early: the statements before it run,
/// and its own statement reports the redeclaration.
#[test]
fn early_binding_leaves_taken_names_to_their_statement() {
    assert_eq!(run_failing(br#"echo "a"; class D {} class D {}"#), "a");
    assert_eq!(run_failing(br#"echo "a"; class KnownClass {}"#), "a");
    assert_eq!(run_failing(br#"echo "a"; interface Traversable {}"#), "a");
}

/// Verifies an early-bound class PHP cannot link stops the fragment before any statement runs.
#[test]
fn early_binding_reports_invalid_early_class_before_the_first_statement() {
    assert_eq!(run_failing(br#"echo "a"; final class P {} class C extends P {}"#), "");
    assert_eq!(run_failing(br#"echo "a"; interface P {} class C extends P {}"#), "");
}

/// Verifies a declaration after a top-level `return` is still bound.
#[test]
fn early_binding_binds_declarations_after_a_return() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let first = parse_fragment(br#"return; class R {} function r() { return "r"; }"#).expect("parse");
    let result = execute_program_with_context(&mut context, &first, &mut scope, &mut values).unwrap();
    eval_release_value(&mut context, &mut values, result).unwrap();
    let second = parse_fragment(br#"echo class_exists("R") ? "yes" : "no", r();"#).expect("parse");
    let result = execute_program_with_context(&mut context, &second, &mut scope, &mut values).unwrap();
    eval_release_value(&mut context, &mut values, result).unwrap();
    assert_eq!(values.output, "yesr");
}

/// Verifies declarations of a namespaced fragment are bound under their qualified names.
#[test]
fn early_binding_binds_namespaced_declarations() {
    let output = run(
        br#"namespace N; echo f(), class_exists("N\\C") ? "yes" : "no"; function f() { return "x"; } class C {}"#,
    );
    assert_eq!(output, "xyes");
}
