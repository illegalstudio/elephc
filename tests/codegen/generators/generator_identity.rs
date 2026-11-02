//! Purpose:
//! Executes generators whose only yields disappear during constant optimization.
//!
//! Called from:
//! - The codegen integration-test harness.
//!
//! Key details:
//! - Both EIR optimizer settings must preserve laziness and terminal return values.

use crate::support::*;

/// A dead yield in an expression still creates a lazy, immediately completing Generator.
#[test]
fn test_generator_identity_folded_expression_yields() {
    let source = r#"<?php
function deadMatch() {
    echo "M";
    $value = match (1) { 0 => yield 9, default => 2 };
    return $value;
}
function deadShort() {
    echo "S";
    $value = 3 ?: yield 9;
    return $value;
}
enum Choice {
    case One;
    public function values() {
        if (false) { yield 9; }
        if (false) { echo "unreachable"; }
        echo "E";
        return 4;
    }
}
$closure = function () {
    echo "C";
    $value = match (1) { 0 => yield 9, default => 5 };
    return $value;
};
$a = deadMatch();
$b = deadShort();
$c = Choice::One->values();
$d = $closure();
echo "created:";
$a->rewind(); echo $a->getReturn(), ":";
$b->rewind(); echo $b->getReturn(), ":";
$c->rewind(); echo $c->getReturn(), ":";
$d->rewind(); echo $d->getReturn(), ":";
echo $a->valid() || $b->valid() || $c->valid() || $d->valid() ? "bad" : "done";
"#;
    let expected = "created:M2:S3:E4:C5:done";
    assert_eq!(compile_and_run(source), expected);
    without_ir_opt(|| assert_eq!(compile_and_run(source), expected));
}
