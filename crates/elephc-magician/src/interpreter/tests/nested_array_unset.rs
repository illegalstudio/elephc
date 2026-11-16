//! Purpose:
//! Pins `unset($a[EXPR][$k])` to ONE evaluation of the container's keys, php's silent no-op for a
//! missing link, and php's `Undefined variable` warning for a missing root.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::nested_array_unset`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5's output on the same fragment.
//! - The bug: the nested unset probed the container with `isset`, read it, then stored it back
//!   as an lvalue, evaluating `f()` in `unset($a[f()][$k])` three times.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed with the warnings it raised.
fn run(fragment: &[u8]) -> (String, Vec<String>) {
    let program = parse_fragment(fragment).expect("parse nested unset fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute nested unset fragment");
    (values.output.clone(), values.warnings.clone())
}

/// The container key runs once and the element is removed from the stored container.
#[test]
fn the_container_key_of_a_nested_unset_is_evaluated_once() {
    let (output, warnings) = run(br#"function f() { echo "f;"; return 0; }
$a = [[1 => 1, 2 => 2]];
$k = 1;
unset($a[f()][$k]);
echo count($a[0]), ":", isset($a[0][1]) ? "set" : "gone", ":", $a[0][2];"#);
    assert_eq!(output, "f;1:gone:2");
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// A missing link is a silent no-op that still evaluates both keys once, and creates nothing.
#[test]
fn a_missing_link_is_a_silent_no_op_after_both_keys_run() {
    let (output, warnings) = run(br#"function f() { echo "f;"; return 0; }
function g() { echo "g;"; return 1; }
$a = [[1 => 1]];
unset($a[f() + 4][g()]);
echo count($a);"#);
    assert_eq!(output, "f;g;1");
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// An undefined root warns once, after the key ran, and stays undefined.
#[test]
fn an_undefined_root_warns_after_the_key_and_stays_undefined() {
    let (output, warnings) = run(br#"function f() { echo "f;"; return 0; }
unset($b[f()][1]);
echo isset($b) ? "b" : "no b";"#);
    assert_eq!(output, "f;no b");
    assert_eq!(warnings, vec!["Undefined variable $b".to_string()]);
}

/// A three-level chain evaluates its computed middle key once.
#[test]
fn a_computed_middle_key_of_a_three_level_unset_runs_once() {
    let (output, warnings) = run(br#"function f() { echo "f;"; return 0; }
$c = ['x' => ['y' => ['z' => 1, 'w' => 2]]];
unset($c['x'][f() ? 'q' : 'y']['z']);
echo implode(",", array_keys($c['x']['y']));"#);
    assert_eq!(output, "f;w");
    assert!(warnings.is_empty(), "{warnings:?}");
}
