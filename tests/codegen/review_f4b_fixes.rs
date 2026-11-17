//! Purpose:
//! Regression tests for the eval/bridge review findings fixed in the final round (batch 4b):
//! compiled `iterable` parameters accepting interpreted Traversables.
//!
//! Called from:
//! - The codegen integration harness (`tests/codegen/mod.rs`).
//!
//! Key details:
//! - Expected values were measured with `php -n`.

use crate::support::*;

/// A compiled `iterable` parameter accepts an interpreted Traversable, and a compiled `foreach`
/// over an `object` parameter walks an interpreted aggregate (php prints `123` twice).
#[test]
fn test_compiled_iterable_and_object_params_accept_interpreted_traversables() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalAgg implements IteratorAggregate { function getIterator(): Iterator { return new ArrayIterator([1, 2, 3]); } }');
function walk(iterable $i) { foreach ($i as $x) echo $x; echo "\n"; }
function walk_obj(object $o) { foreach ($o as $x) echo $x; echo "\n"; }
$a = eval('return new EvalAgg();');
walk($a);
walk_obj($a);
"#,
    );
    assert_eq!(out, "123\n123\n");
}

