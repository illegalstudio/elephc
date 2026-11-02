//! Purpose: verify PHP's request superglobals stay visible from inside a nested
//! activation without an explicit `global` statement, exactly as php's own
//! implicit-global behavior does for `$_SERVER`, `$_ENV`, `$_GET`, `$_POST`,
//! `$_COOKIE`, `$_FILES`, `$_REQUEST`, and `$_SESSION`.
//! Called from: the focused interpreter unit suite.
//! Key details: every case sets `context.set_global_scope` to the SAME scope object
//! used to execute the top-level program, mirroring a script whose own top level
//! IS the program's global scope (the shape a dynamically included/eval'd PHP
//! file has in practice).

use super::super::*;
use super::support::*;

/// Verifies a bare superglobal read inside a plain declared function sees the
/// value assigned at top level, with no `global $_SERVER;` statement anywhere.
#[test]
fn superglobal_name_is_visible_inside_a_function_without_a_global_statement() {
    let program = parse_fragment(
        br#"
$_SERVER["K"] = "sv";
function r() { return $_SERVER["K"] ?? "missing"; }
return r();
"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert!(context.set_global_scope(&mut scope));

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("sv".to_string()));
}

/// Verifies a bare superglobal read inside a declared method sees the same value.
#[test]
fn superglobal_name_is_visible_inside_a_method_without_a_global_statement() {
    let program = parse_fragment(
        br#"
$_SERVER["K"] = "sv";
class SuperglobalReader {
    public function read() { return $_SERVER["K"] ?? "missing"; }
}
$reader = new SuperglobalReader();
return $reader->read();
"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert!(context.set_global_scope(&mut scope));

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("sv".to_string()));
}

/// Verifies a bare superglobal read inside a closure sees the same value.
#[test]
fn superglobal_name_is_visible_inside_a_closure_without_a_global_statement() {
    let program = parse_fragment(
        br#"
$_SERVER["K"] = "sv";
$read = function () { return $_SERVER["K"] ?? "missing"; };
return $read();
"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert!(context.set_global_scope(&mut scope));

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("sv".to_string()));
}

/// Verifies a bare superglobal read inside an arrow function sees the same value.
#[test]
fn superglobal_name_is_visible_inside_an_arrow_function_without_a_global_statement() {
    let program = parse_fragment(
        br#"
$_SERVER["K"] = "sv";
$read = fn() => $_SERVER["K"] ?? "missing";
return $read();
"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert!(context.set_global_scope(&mut scope));

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("sv".to_string()));
}

/// Verifies a superglobal written inside a function is visible from the caller
/// after the call returns: php's superglobals are genuinely global storage, not
/// a snapshot copied into the callee's activation.
#[test]
fn writing_a_superglobal_inside_a_function_is_visible_after_it_returns() {
    let program = parse_fragment(
        br#"
function w() { $_SERVER["K"] = "sv"; }
w();
return $_SERVER["K"] ?? "missing";
"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert!(context.set_global_scope(&mut scope));

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("sv".to_string()));
}

/// Verifies `global $x;` inside a declared function already reaches the same
/// top-level scope when that scope IS the context's global scope -- the
/// generic named-alias mechanism, unrelated to the superglobal name list.
#[test]
fn global_statement_inside_a_function_reads_and_writes_top_level_scope() {
    let program = parse_fragment(
        br#"
$x = "top";
function readX() { global $x; return $x; }
function writeX() { global $x; $x = "changed"; }
$before = readX();
writeX();
return $before . ":" . $x;
"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert!(context.set_global_scope(&mut scope));

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("top:changed".to_string()));
}
