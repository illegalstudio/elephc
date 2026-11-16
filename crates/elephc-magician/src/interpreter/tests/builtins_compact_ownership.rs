//! Purpose:
//! Interpreter tests for the cell ownership of `compact()`'s arguments.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_compact_ownership`.
//!
//! Key details:
//! - The fake runtime counts references and fails a release of a cell nobody owned, so an
//!   over-release of a variable's storage cell aborts the test.

use super::super::*;
use super::support::*;

/// Verifies `compact($names)` and `compact(...$names)` leave the variable that holds the names alone.
///
/// The argument is a variable read, which hands out a borrowed view of the scope's own cell; a
/// release of that view gives back a reference the call never took.
#[test]
fn compact_leaves_a_variable_argument_alive() {
    let program = parse_fragment(
        br#"$names = ["a", "b"];
$a = 1;
$b = 2;
$first = compact($names);
$second = compact(...$names);
echo count($first) . ":" . count($second) . ":" . $names[0] . $names[1];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:2:ab");
    assert!(values.over_releases().is_empty());
}
