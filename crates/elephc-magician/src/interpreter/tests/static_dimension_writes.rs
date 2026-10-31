//! Purpose:
//! Checks runtime copy selection and owner retirement for eval static array mutations.
//!
//! Called from:
//! - The Magician interpreter unit-test harness.
//!
//! Key details:
//! - Counts provider API calls instead of relying on machine-dependent timing.
//! - Fake value copies preserve stored entries and index history without runtime key traversal.

use super::super::*;
use super::support::*;

/// Repeated static writes use one provider value copy each and retire displaced boxes.
#[test]
fn static_array_writes_use_the_native_value_copy_contract() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let declarations =
        parse_fragment(b"class StaticCopyContract { public static array $items = []; }").unwrap();
    let returned =
        execute_program_with_context(&mut context, &declarations, &mut scope, &mut values).unwrap();
    eval_release_value(&mut context, &mut values, returned).unwrap();
    for source in [
        "for ($i = 0; $i < 64; $i++) { StaticCopyContract::$items[] = $i; }",
        "for ($i = 0; $i < 64; $i++) { StaticCopyContract::$items[$i] = $i + 1; }",
        "$class = 'StaticCopyContract'; for ($i = 0; $i < 64; $i++) { $class::$items[$i] += 1; }",
    ] {
        let before = values.array_value_copy_calls;
        let key_walks = values.array_iter_key_calls;
        let program = parse_fragment(source.as_bytes()).unwrap();
        let returned =
            execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
        eval_release_value(&mut context, &mut values, returned).unwrap();
        assert_eq!(values.array_value_copy_calls - before, 64, "{source}");
        assert_eq!(
            values.array_iter_key_calls - key_walks,
            0,
            "{source}: no key scan per write"
        );
        let stored = context
            .static_property("StaticCopyContract", "items")
            .unwrap();
        assert_eq!(values.array_len(stored).unwrap(), 64);
        for (id, value) in &values.values {
            if matches!(value, FakeValue::Array(_) | FakeValue::Assoc(_)) {
                assert_eq!(
                    values.cell_owners[id],
                    usize::from(*id == stored.as_ptr() as usize),
                    "{source}: cell {id}"
                );
            }
        }
    }
}

/// Static value aliases remain independent when one slot appends or promotes to string keys.
#[test]
fn static_array_value_aliases_survive_published_cell_replacement() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let source = b"class StaticAliasFirst { public static array $items = [1]; }
        class StaticAliasSecond { public static array $items = []; }
        StaticAliasSecond::$items = StaticAliasFirst::$items;
        $copy = StaticAliasFirst::$items;
        StaticAliasFirst::$items[] = 2;
        StaticAliasFirst::$items['mark'] = 3;
        echo count(StaticAliasFirst::$items), ':', count(StaticAliasSecond::$items), ':', count($copy);";
    let program = parse_fragment(source).unwrap();
    let returned =
        execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    eval_release_value(&mut context, &mut values, returned).unwrap();
    assert_eq!(values.output, "3:1:1");
    let first = context
        .static_property("StaticAliasFirst", "items")
        .unwrap();
    let second = context
        .static_property("StaticAliasSecond", "items")
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(values.cell_owners[&(first.as_ptr() as usize)], 1);
    // Each published value can detach eagerly as long as all three owners remain independent.
    let copy = scope.entry("copy").unwrap().cell();
    assert_ne!(copy, second);
    assert_eq!(values.cell_owners[&(copy.as_ptr() as usize)], 1);
    assert_eq!(values.cell_owners[&(second.as_ptr() as usize)], 1);
    let cleanup = parse_fragment(b"unset($copy);").unwrap();
    let returned =
        execute_program_with_context(&mut context, &cleanup, &mut scope, &mut values).unwrap();
    eval_release_value(&mut context, &mut values, returned).unwrap();
    assert_eq!(values.cell_owners[&(copy.as_ptr() as usize)], 0);
    assert_eq!(values.cell_owners[&(second.as_ptr() as usize)], 1);
}

/// Failure while retiring old static storage consumes each displaced receiver owner once.
#[test]
fn static_array_replacement_cleanup_failure_keeps_the_published_owner() {
    for failing_owner in 0..2 {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let program =
            parse_fragment(b"class StaticCleanupCopy { public static array $items = []; }")
                .unwrap();
        let returned =
            execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
        eval_release_value(&mut context, &mut values, returned).unwrap();
        let original = context
            .static_property("StaticCleanupCopy", "items")
            .unwrap();
        // A constant RHS/key stage owns no temporary that releases before the displaced slot.
        values.fail_release_call = Some(values.releases.len() + failing_owner);
        let mutation = parse_fragment(b"StaticCleanupCopy::$items[0] = 7;").unwrap();
        let result = execute_program_with_context(&mut context, &mutation, &mut scope, &mut values);
        assert_eq!(result, Err(EvalStatus::UncaughtThrowable));
        let published = context
            .static_property("StaticCleanupCopy", "items")
            .unwrap();
        assert_ne!(original, published);
        assert_eq!(values.cell_owners[&(original.as_ptr() as usize)], 0);
        assert_eq!(values.cell_owners[&(published.as_ptr() as usize)], 1);
        assert_eq!(values.array_len(published).unwrap(), 0);
    }
}

/// Read visibility failure preserves static storage before any replacement box is copied.
#[test]
fn static_array_read_access_failure_preserves_the_slot() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let program =
        parse_fragment(b"class StaticDeniedCopy { private static array $items = []; }")
            .unwrap();
    let returned =
        execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    eval_release_value(&mut context, &mut values, returned).unwrap();
    let original = context
        .static_property("StaticDeniedCopy", "items")
        .unwrap();
    let before = values.array_value_copy_calls;
    let mutation = parse_fragment(b"StaticDeniedCopy::$items[0] = 7;").unwrap();
    assert_eq!(
        execute_program_with_context(&mut context, &mutation, &mut scope, &mut values),
        Err(EvalStatus::UncaughtThrowable)
    );
    assert_eq!(
        context.static_property("StaticDeniedCopy", "items"),
        Some(original)
    );
    assert_eq!(values.array_value_copy_calls, before);
    assert_eq!(values.array_len(original).unwrap(), 0);
    assert_eq!(values.cell_owners[&(original.as_ptr() as usize)], 1);
    if let Some(error) = context.take_pending_throw() {
        eval_release_value(&mut context, &mut values, error).unwrap();
    }
}

/// Static appends retain deleted-tail history and report exhausted integer keys before writing.
#[test]
fn static_array_appends_preserve_history_and_exhaustion() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let source = br#"
class StaticIndexHistory { public static mixed $items = null; }
$original = [1, 2, 3]; unset($original[2]);
StaticIndexHistory::$items = $original;
StaticIndexHistory::$items[] = 9;
echo isset(StaticIndexHistory::$items[3]) ? 'history|' : 'bad|';
StaticIndexHistory::$items = [PHP_INT_MAX => 1];
try { StaticIndexHistory::$items[] = 2; }
catch (Error $error) { echo $error->getMessage(), '|'; }
echo count(StaticIndexHistory::$items);
unset($original, $error);
"#;
    let program = parse_fragment(source).unwrap();
    let returned =
        execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    eval_release_value(&mut context, &mut values, returned).unwrap();
    assert_eq!(
        values.output,
        "history|Cannot add element to the array as the next element is already occupied|1"
    );
    let stored = context
        .static_property("StaticIndexHistory", "items")
        .unwrap();
    assert_eq!(values.array_next_index(stored).unwrap(), None);
    assert_eq!(values.array_len(stored).unwrap(), 1);
    assert_eq!(values.cell_owners[&(stored.as_ptr() as usize)], 1);
}
