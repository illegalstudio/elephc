//! Purpose:
//! Terminal `return <expr>` plus `Generator::getReturn` retrieval, including bare `return;` that just terminates the generator without producing a value.
//!
//! Called from:
//!  - `cargo test` via the integration test harness; aggregated under
//!    `tests::codegen::generators` in `tests/codegen/generators/mod.rs`.
//!
//! Key details:
//!  - Verifies terminal return storage is independent from yielded values and
//!    remains accessible after foreach exhausts the generator.

use crate::support::*;

/// Verifies `Generator::getReturn()` retrieves the value from a terminal `return $v;`
/// inside a generator after the generator is exhausted via `foreach`.
///
/// Fixture: generator yields `1`, `2` then returns `42`. After foreach exhausts the
/// generator, `getReturn()` is called and must return `42`.
/// Regression: return value slot must be independent from yield slots.
#[test]
fn test_generator_return_value_via_get_return() {
    // `return $v;` inside a generator stashes $v in the frame's
    // return_value slot and terminates. `getReturn()` retrieves it.
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield 1;
    yield 2;
    return 42;
}
$g = gen();
foreach ($g as $v) { echo $v; echo " "; }
echo "ret=";
echo $g->getReturn();
"#,
    );
    assert_eq!(out, "1 2 ret=42");
}

/// Verifies `return;` (bare return, no value) terminates the generator without writing
/// a return value. The previously zero-initialised return_value cell surfaces as null/0.
///
/// Fixture: generator yields `1`, executes bare `return;`, then attempts `yield 99`
/// which must be unreachable. Foreach consumes only `1` and prints `done`.
/// Regression: bare return must not emit a spurious return value.
#[test]
fn test_generator_bare_return_terminates() {
    // `return;` (no value) terminates the generator without writing a
    // return value. The previously zero-initialised return_value cell
    // surfaces as null/0.
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield 1;
    return;
    yield 99;
}
foreach (gen() as $v) { echo $v; echo " "; }
echo "done";
"#,
    );
    assert_eq!(out, "1 done");
}
