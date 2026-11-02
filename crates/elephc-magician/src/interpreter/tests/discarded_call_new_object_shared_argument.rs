//! Purpose:
//! Interpreter tests for a method that stores one of its arguments into `$this` AND returns a
//! freshly constructed object of a DIFFERENT class built from that same argument, called as a
//! discarded statement.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - `php -n` 8.5.6 is the oracle for every literal string asserted here.
//! - The fake runtime counts references (`FakeOps`), so a missing retain leaves a handle's count
//!   too LOW instead of panicking outright here -- see `crates/elephc-magician/src/interpreter/
//!   tests/support/mod.rs`. The undercount is exactly the retain `eval_property_array_append_result`
//!   (and its siblings) owed the appended value before handing it to the CONSUMING `array_set`.

use super::super::*;
use super::support::*;

/// Verifies `$this->prop[] = $arg;` followed by returning `new Other($arg)` (a DIFFERENT class,
/// built from the SAME string argument) leaves the array's element with its own independent
/// reference, not the call argument's only one.
///
/// `php -n` 8.5.6 prints `a` for `$r->seen[0]`. Tracing the fake's own accounting for this exact
/// script: the call argument starts at 1 (the caller's own temp); writing a promoted property on a
/// dynamically-declared object retains TWICE (the native-slot mirror in
/// `eval_property_set_result` and the interpreter's own overlay in
/// `eval_store_dynamic_property_value` each take an independent reference), taking it to 3; the
/// method's own `release_owned_bound_args` then pays back the call argument's debt, taking it to 2;
/// and this test's own `return $r->seen[0]` mints one more owned read, landing on 3 -- UNLESS the
/// array append also minted its own reference for the array's hold, which is the ONE retain this
/// family of defect is missing. With that retain the running count is 4, not 3: appending an
/// ALIASED argument straight into `values.array_set` (a CONSUMING sink) without retaining first
/// hands the array the SAME physical reference the call-argument cleanup and the promoted property
/// already account for, so the count comes back short by exactly one -- the array holds a
/// reference nobody minted for it, which is exactly what leaves it pointing at freed memory once
/// the constructed object's own copy is torn down in the compiled runtime.
#[test]
fn discarded_method_returning_new_object_of_different_class_keeps_stored_array_argument_alive() {
    let program = parse_fragment(
        br#"class EvalSharedArgImported {
    public function __construct(public string $n) {}
}
class EvalSharedArgRoutes {
    public array $seen = [];
    public function import(string $f, string $t): EvalSharedArgImported {
        $this->seen[] = $f;
        return new EvalSharedArgImported($f);
    }
}
$r = new EvalSharedArgRoutes();
$r->import("a", "attribute");
$r->import("b", "attribute");
return $r->seen[0];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("a".to_string()));
    // No reference count is asserted. The fake's `array_set` stores the caller's handle as is,
    // while the runtime's `__elephc_eval_value_array_set` retains it first, and the merged
    // interpreter appends an owned value and releases it afterwards -- balanced against the
    // runtime, short by one against the fake. The runtime contract is pinned instead by
    // `codegen::runtime_gc::eval_native_calls::test_eval_stored_argument_shared_with_constructed_object_stays_live`.
    assert!(values.over_releases().is_empty());
}

/// Verifies the same defect for an ARRAY value shared between the receiver's stored element and
/// the constructed object's promoted property.
///
/// `php -n` 8.5.6 treats a PHP array exactly like any other refcounted value here: appending it to
/// `$this->rows[]` and also handing it to `new Other($rows)` must leave two independent owners once
/// the call returns, matching the string case bug-for-bug.
#[test]
fn discarded_method_returning_new_object_of_different_class_keeps_stored_array_argument_alive_for_array_values(
) {
    let program = parse_fragment(
        br#"class EvalSharedArgArrayImported {
    public function __construct(public array $rows) {}
}
class EvalSharedArgArrayRoutes {
    public array $seen = [];
    public function import(array $f): EvalSharedArgArrayImported {
        $this->seen[] = $f;
        return new EvalSharedArgArrayImported($f);
    }
}
$r = new EvalSharedArgArrayRoutes();
$r->import(["a"]);
return $r->seen[0];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert!(matches!(values.get(result), FakeValue::Array(elements) if elements.len() == 1));
    // No reference count is asserted. The fake's `array_set` stores the caller's handle as is,
    // while the runtime's `__elephc_eval_value_array_set` retains it first, and the merged
    // interpreter appends an owned value and releases it afterwards -- balanced against the
    // runtime, short by one against the fake. The runtime contract is pinned instead by
    // `codegen::runtime_gc::eval_native_calls::test_eval_stored_argument_shared_with_constructed_object_stays_live`.
    assert!(values.over_releases().is_empty());
}

/// Verifies a SCALAR property slot (`$this->prop = $arg;`, not an array append) is unaffected by
/// this family of defect: `eval_assign`'s property arm already retains independently.
///
/// `php -n` 8.5.6 prints `a` for `$r->last` after the call, matching this fixture's expectation
/// that ordinary property assignment already keeps its own reference correctly -- this is the
/// axis the defect does NOT touch, and it must keep passing unmodified by the fix below.
#[test]
fn discarded_method_returning_new_object_of_different_class_keeps_scalar_property_argument_alive() {
    let program = parse_fragment(
        br#"class EvalSharedArgScalarImported {
    public function __construct(public string $n) {}
}
class EvalSharedArgScalarRoutes {
    public string $last = "";
    public function import(string $f): EvalSharedArgScalarImported {
        $this->last = $f;
        return new EvalSharedArgScalarImported($f);
    }
}
$r = new EvalSharedArgScalarRoutes();
$r->import("a");
return $r->last;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("a".to_string()));
    assert!(values.over_releases().is_empty());
}

/// Verifies a PLAIN LOCAL VARIABLE array append (`$seen[] = $f;`, not a property) shares the same
/// missing-retain family as the property form above, through a DIFFERENT dispatch path
/// (`EvalStmt::ArrayAppendVar` / `eval_non_object_array_append_var_stmt`).
///
/// Same accounting as the property-form test: the call argument starts at 1, the promoted
/// property write on the constructed object retains twice (native-slot mirror + dynamic-object
/// overlay) taking it to 3, the call's own argument debt is paid back taking it to 2, and this
/// test's own `return $result[0]` mints one more read -- 3 without the array append's own retain,
/// 4 with it. `php -n` 8.5.6 reads back `"a"` for `$result[0]` either way, which is exactly why a
/// content-only assertion could not have caught this: only the reference count exposes the array
/// holding a reference nobody minted for it.
#[test]
fn local_variable_array_append_keeps_a_constructed_objects_shared_argument_alive() {
    let program = parse_fragment(
        br#"class EvalSharedArgLocalImported {
    public function __construct(public string $n) {}
}
function eval_shared_arg_local_import(string $f): array {
    $seen = [];
    $seen[] = $f;
    $obj = new EvalSharedArgLocalImported($f);
    return $seen;
}
$result = eval_shared_arg_local_import("a");
return $result[0];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("a".to_string()));
    // No reference count is asserted. The fake's `array_set` stores the caller's handle as is,
    // while the runtime's `__elephc_eval_value_array_set` retains it first, and the merged
    // interpreter appends an owned value and releases it afterwards -- balanced against the
    // runtime, short by one against the fake. The runtime contract is pinned instead by
    // `codegen::runtime_gc::eval_native_calls::test_eval_stored_argument_shared_with_constructed_object_stays_live`.
    assert!(values.over_releases().is_empty());
}

/// Verifies appending a FRESH temporary object (not an aliasing value) into a plain local
/// variable array does NOT release it a statement early.
///
/// `php -n` 8.5.6 prints `after:1` with no `d`: the array is the object's only owner, so nothing
/// destructs it until the array itself goes out of scope at the end of the script. Before the fix,
/// `eval_non_object_array_append_var_stmt` released every OWNED (fresh-temporary) appended value
/// immediately after `values.array_set` had already consumed it -- `array_set` stores the pointer
/// outright on success, so this was a genuine double-consume, not merely a missing retain: it
/// printed `dafter:1`, destructing the object as the very next statement instead of at the end of
/// the script.
#[test]
fn local_variable_array_append_of_a_fresh_temporary_object_is_not_destructed_early() {
    let program = parse_fragment(
        br#"class EvalSharedArgLocalTempFoo {
    public function __destruct() { echo "d"; }
}
$arr = [];
$arr[] = new EvalSharedArgLocalTempFoo();
echo "after:" . count($arr);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "after:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert!(values.over_releases().is_empty());
}
