//! Purpose:
//! End-to-end regressions for runtime eval object construction through AOT classes.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_constructor` through Rust's test harness.
//!
//! Key details:
//! - Fixtures focus on constructor bridge argument binding and by-reference
//!   writeback for non-variable eval caller targets.

use crate::support::compile_and_run;

/// Verifies AOT constructor by-reference args write back to eval lvalue targets.
#[test]
fn test_eval_dynamic_new_constructor_by_ref_writes_back_to_lvalue_targets() {
    let out = compile_and_run(
        r#"<?php
class EvalCtorRefTargetBridge {
    public function __construct(int &$value) {
        $value = $value + 5;
    }
}

class EvalCtorRefTargetBox {
    public int $value = 3;
}

class EvalCtorRefTargetStatic {
    public static int $value = 4;
}

echo eval('$items = ["x" => "1"];
new EvalCtorRefTargetBridge($items["x"]);
echo gettype($items["x"]) . ":" . $items["x"] . "|";

$nested = ["outer" => ["inner" => "2"]];
new EvalCtorRefTargetBridge($nested["outer"]["inner"]);
echo gettype($nested["outer"]["inner"]) . ":" . $nested["outer"]["inner"] . "|";

$box = new EvalCtorRefTargetBox();
new EvalCtorRefTargetBridge($box->value);
echo gettype($box->value) . ":" . $box->value . "|";

EvalCtorRefTargetStatic::$value = "4";
new EvalCtorRefTargetBridge(EvalCtorRefTargetStatic::$value);
return gettype(EvalCtorRefTargetStatic::$value) . ":" . EvalCtorRefTargetStatic::$value;');
"#,
    );

    assert_eq!(out, "integer:6|integer:7|integer:8|integer:9");
}

/// Verifies AOT constructor by-reference writeback happens before a catchable throw.
#[test]
fn test_eval_dynamic_new_constructor_by_ref_lvalue_writeback_before_throw() {
    let out = compile_and_run(
        r#"<?php
class EvalCtorThrowRefTargetBridge {
    public function __construct(int &$value) {
        $value = $value + 11;
        throw new Exception("ctor-lvalue");
    }
}

class EvalCtorThrowRefTargetBox {
    public int $value = 5;
}

echo eval('$items = ["x" => "1"];
try {
    new EvalCtorThrowRefTargetBridge($items["x"]);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":";
}
echo gettype($items["x"]) . ":" . $items["x"] . "|";

$box = new EvalCtorThrowRefTargetBox();
try {
    new EvalCtorThrowRefTargetBridge($box->value);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":";
}
return gettype($box->value) . ":" . $box->value;');
"#,
    );

    assert_eq!(
        out,
        "Exception:ctor-lvalue:integer:12|Exception:ctor-lvalue:integer:16"
    );
}
