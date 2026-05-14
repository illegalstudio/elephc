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
