//! Purpose:
//! Interpreter tests for native function dispatch, scope array mutation, ownership, break, and continue.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover integration edges between scope cells and fake runtime hooks.

use super::super::*;
use super::support::*;

mod argument_ownership;

/// Same-cell native writeback dirties the local or global snapshot without replacing its owner.
#[test]
fn native_same_cell_reference_writeback_marks_the_owning_snapshot_dirty() {
    for through_method in [false, true] {
        for through_global in [false, true] {
            let mut context = ElephcEvalContext::new();
            let mut local = ElephcEvalScope::new();
            let mut global = ElephcEvalScope::new();
            let mut values = FakeOps::default();
            let value = values.string("updated").unwrap();
            let owner = if through_global { &mut global } else { &mut local };
            owner.set_from_aot("text", value, crate::scope::ScopeCellOwnership::Owned);
            owner.mark_name_clean("text");
            if through_global {
                assert!(context.set_global_scope(&mut global));
                local.mark_global_alias("text");
            }
            let target = EvalReferenceTarget::Variable { scope: &mut local, name: "text".into() };
            if through_method {
                write_back_method_ref_target(&target, value, &mut context, &mut values).unwrap();
            } else {
                let argument = BoundMethodArg {
                    value, ref_target: Some(target), variadic_ref_targets: Vec::new(),
                };
                write_back_native_callable_ref_args(&[argument], &mut context, &mut values).unwrap();
            }
            let owner = if through_global { &global } else { &local };
            assert!(owner.entry("text").unwrap().flags().dirty);
            assert_eq!(owner.visible_cell("text"), Some(value));
            assert!(values.releases.is_empty(), "writeback must preserve the original scope owner");
        }
    }
}

/// Native object contracts reject assignments before an interpreted catch or later statement runs.
#[test]
fn native_object_snapshot_contract_is_checked_before_assignment() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let initialize = parse_fragment(b"class NativeScopeNode {} $node = new NativeScopeNode();").unwrap();
    execute_program_with_context(&mut context, &initialize, &mut scope, &mut values).unwrap();
    let original = scope.visible_cell("node").unwrap();
    scope.set_native_storage_type("node".into(), Some(EvalParameterType::new(
        vec![EvalParameterTypeVariant::Class("NativeScopeNode".into())], false,
    )));
    let program = parse_fragment(b"try { $node = null; } catch (TypeError $error) { echo 'caught'; } unset($error);").unwrap();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert_eq!(scope.visible_cell("node"), Some(original));
    assert_eq!(values.output, "caught");
    let program = parse_fragment(b"unset($node); $node = 3;").unwrap();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert_eq!(values.get(scope.visible_cell("node").unwrap()), FakeValue::Int(3));
}

/// In-place array writes dirty imported snapshots even when the fake runtime keeps the same box.
#[test]
fn native_array_snapshots_mark_element_mutations_dirty() {
    for source in ["$items[0] = 4;", "$items[] = 3;", "$items[0] += 1;", "unset($items[1]);"] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let initialize = parse_fragment(b"$items = [1, 2];").unwrap();
        execute_program_with_context(&mut context, &initialize, &mut scope, &mut values).unwrap();
        scope.mark_all_clean();
        let original = scope.visible_cell("items").unwrap();
        let program = parse_fragment(source.as_bytes()).unwrap();
        execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
        assert!(scope.entry("items").unwrap().flags().dirty, "{source}: mutation lost its reload marker");
        if !source.starts_with("unset") {
            assert_eq!(scope.visible_cell("items"), Some(original), "{source}: must exercise the same-box shortcut");
        }
    }
}

/// A local global-alias write marks the global snapshot that native reload actually reads.
#[test]
fn native_global_array_snapshot_marks_in_place_alias_writes_dirty() {
    let mut context = ElephcEvalContext::new();
    let mut local = ElephcEvalScope::new();
    let mut global = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let array = values.array_new(1).unwrap();
    let key = values.int(0).unwrap();
    let value = values.int(1).unwrap();
    values.array_set(array, key, value).unwrap();
    global.set_from_aot("items", array, crate::scope::ScopeCellOwnership::Owned);
    global.mark_name_clean("items");
    assert!(context.set_global_scope(&mut global));
    let program = parse_fragment(b"global $items; $items[] = 2;").unwrap();
    execute_program_with_context(&mut context, &program, &mut local, &mut values).unwrap();
    assert!(global.entry("items").unwrap().flags().dirty);
    assert_eq!(global.visible_cell("items"), Some(array));
}

/// Verifies eval fragments can dispatch registered native AOT functions.
#[test]
fn execute_program_calls_registered_native_function() {
    let program = parse_fragment(br#"return native_answer();"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 0);
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}

/// A global native factory can recover its interpreter context across ordinary helper frames.
#[test]
fn native_global_call_exposes_and_retires_its_active_context() {
    /// Returns the result only while the global native call exposes the expected context.
    unsafe extern "C" fn probe(
        descriptor: *mut std::ffi::c_void,
        _args: *mut crate::value::RuntimeCell,
        _invocation_scope: i64,
    ) -> *mut crate::value::RuntimeCell {
        let (expected_context, result) = unsafe {
            &*(descriptor
                as *const (*mut ElephcEvalContext, *mut crate::value::RuntimeCell))
        };
        assert_eq!(crate::context::native_frame_active_eval_context(), Some(*expected_context));
        *result
    }
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).unwrap();
    let mut descriptor = (&mut context as *mut ElephcEvalContext, expected.as_ptr());
    let native = NativeFunction::new(std::ptr::from_mut(&mut descriptor).cast(), probe, 0);
    assert!(context.define_native_function("native_context_probe", native).is_ok());
    let program = parse_fragment(b"return native_context_probe();").unwrap();
    assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap(), expected);
    assert_eq!(crate::context::native_frame_active_eval_context(), None);
}

/// Verifies registered native AOT function return types are enforced after dispatch.
#[test]
fn execute_program_checks_registered_native_function_return_type() {
    let program = parse_fragment(br#"return native_answer();"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.string("not-an-int").expect("allocate fake result");
    let mut native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 0);
    native.set_return_type(EvalParameterType::new(
        vec![EvalParameterTypeVariant::Int],
        false,
    ));
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("native return type mismatch should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
    assert_eq!(values.releases.iter().filter(|value| **value == expected).count(), 1);
}

/// Releases the native result that runtime return-type coercion replaces with a new cell.
#[test]
fn execute_program_releases_coerced_native_return_value() {
    let program = parse_fragment(br#"return native_answer();"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let original = values.string("42").expect("allocate native result");
    let mut native = NativeFunction::new(original.as_ptr().cast(), fake_native_return_descriptor, 0);
    native.set_return_type(EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false));
    assert!(context.define_native_function("native_answer", native).is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("coerce native return");

    assert_eq!(values.get(result), FakeValue::Int(42));
    assert_ne!(result, original);
    assert_eq!(values.releases.iter().filter(|value| **value == original).count(), 1);
    assert!(!values.releases.contains(&result));
}

/// Releases the coerced result when destruction of the original Stringable object throws.
#[test]
fn execute_program_releases_coerced_return_when_destructor_throws() {
    let program = parse_fragment(br#"
class ReturnCleanupFailure {
    public function __toString(): string { return "coerced-return"; }
    public function __destruct() { throw new Exception("cleanup"); }
}
function string_return(): string { return new ReturnCleanupFailure(); }
return string_return();
"#).expect("parse return cleanup fixture");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::UncaughtThrowable));
    let strings = values.values.iter().filter_map(|(id, value)| {
        (*value == FakeValue::String("coerced-return".to_string())).then_some(*id)
    }).collect::<Vec<_>>();
    assert_eq!(strings.len(), 1);
    assert_eq!(values.releases.iter().filter(|value| value.as_ptr() as usize == strings[0]).count(), 1);
}

/// Releases invalid native results for void and never contracts before propagating validation failure.
#[test]
fn execute_program_releases_invalid_native_void_and_never_returns() {
    for variant in [EvalParameterTypeVariant::Void, EvalParameterTypeVariant::Never] {
        let program = parse_fragment(br#"return native_answer();"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let original = values.int(42).expect("allocate native result");
        let mut native = NativeFunction::new(original.as_ptr().cast(), fake_native_return_descriptor, 0);
        native.set_return_type(EvalParameterType::new(vec![variant], false));
        assert!(context.define_native_function("native_answer", native).is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values);

        assert_eq!(result, Err(EvalStatus::RuntimeFatal));
        assert_eq!(values.releases.iter().filter(|value| **value == original).count(), 1);
    }
}

/// Releases every argument-array index on success and when either insertion fails.
#[test]
fn execute_program_releases_native_argument_array_keys() {
    for failure in [None, Some(0), Some(1)] {
        let program = parse_fragment(br#"return native_answer("left", "right");"#)
            .expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate native result");
        let native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
        assert!(context.define_native_function("native_answer", native).is_ok());
        if let Some(index) = failure { values.fail_array_set_call(index); }

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values);

        assert_eq!(result, failure.map_or(Ok(expected), |_| Err(EvalStatus::UnsupportedConstruct)));
        let keys = values.values.iter().filter_map(|(id, value)| {
            matches!(value, FakeValue::Int(0 | 1)).then_some(*id)
        }).collect::<Vec<_>>();
        assert_eq!(keys.len(), failure.map_or(2, |index| index + 1));
        for key in keys {
            assert_eq!(values.releases.iter().filter(|value| value.as_ptr() as usize == key).count(), 1);
        }
    }
}

/// Verifies raw native by-reference staging is released when invoker argument setup fails.
#[test]
fn execute_program_cleans_native_raw_ref_slots_when_arg_array_build_fails() {
    let program = parse_fragment(br#"$value = "keep"; native_string_ref($value);"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let mut native =
        NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 1);
    assert!(native.set_param_name(0, "value"));
    assert!(native.set_param_type(
        0,
        EvalParameterType::new(vec![EvalParameterTypeVariant::String], false)
    ));
    assert!(native.set_param_by_ref(0, true));
    assert!(context
        .define_native_function("native_string_ref", native)
        .is_ok());
    values.fail_array_set_call(0);

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("argument-array build failure should abort native dispatch");
    let value = scope
        .entry("value")
        .expect("scope should retain the original string")
        .cell();

    assert_eq!(err, EvalStatus::UnsupportedConstruct);
    assert_eq!(values.get(value), FakeValue::String("keep".to_string()));
    assert!(values.releases.iter().any(|release| *release == value));
}

/// Verifies raw native by-reference staging is released after a successful invoker call.
#[test]
fn execute_program_cleans_native_raw_ref_slots_after_normal_return() {
    let program = parse_fragment(br#"$value = "keep"; return native_string_ref($value);"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let mut native =
        NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 1);
    assert!(native.set_param_name(0, "value"));
    assert!(native.set_param_type(
        0,
        EvalParameterType::new(vec![EvalParameterTypeVariant::String], false)
    ));
    assert!(native.set_param_by_ref(0, true));
    assert!(context
        .define_native_function("native_string_ref", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("native dispatch should return normally");
    let value = scope
        .entry("value")
        .expect("scope should retain the original string")
        .cell();

    assert_eq!(result, expected);
    assert_eq!(values.get(value), FakeValue::String("keep".to_string()));
    assert!(values.releases.iter().any(|release| *release == value));
}

/// Verifies raw native by-reference staging is released when the invoker signals fatal.
#[test]
fn execute_program_cleans_native_raw_ref_slots_after_null_invoker_return() {
    let program = parse_fragment(br#"$value = "keep"; native_string_ref($value);"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let mut native = NativeFunction::new(
        std::ptr::null_mut(),
        fake_native_null_descriptor,
        1,
    );
    assert!(native.set_param_name(0, "value"));
    assert!(native.set_param_type(
        0,
        EvalParameterType::new(vec![EvalParameterTypeVariant::String], false)
    ));
    assert!(native.set_param_by_ref(0, true));
    assert!(context
        .define_native_function("native_string_ref", native)
        .is_ok());

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("null invoker result should become a runtime fatal");
    let value = scope
        .entry("value")
        .expect("scope should retain the original string")
        .cell();

    assert_eq!(err, EvalStatus::RuntimeFatal);
    assert_eq!(values.get(value), FakeValue::String("keep".to_string()));
    assert!(values.releases.iter().any(|release| *release == value));
}

/// Test native invoker that returns NULL to model a bridge-level fatal result.
unsafe extern "C" fn fake_native_null_descriptor(
    _descriptor: *mut std::ffi::c_void,
    _args: *mut crate::value::RuntimeCell,
    _invocation_scope: i64,
) -> *mut crate::value::RuntimeCell {
    std::ptr::null_mut()
}

/// Verifies direct eval calls can bind registered native parameters by name.
#[test]
fn execute_program_calls_registered_native_function_with_named_args() {
    let program = parse_fragment(br#"return native_answer(right: 2, left: 1);"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let mut native =
        NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(native.set_param_name(0, "left"));
    assert!(native.set_param_name(1, "right"));
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}
/// Verifies direct eval calls can unpack arrays into registered native parameters.
#[test]
fn execute_program_calls_registered_native_function_with_spread_args() {
    let program =
        parse_fragment(br#"return native_answer(...[1, 2]);"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let mut native =
        NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(native.set_param_name(0, "left"));
    assert!(native.set_param_name(1, "right"));
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}
/// Verifies indexed array writes mutate an existing scope array.
#[test]
fn execute_program_writes_indexed_scope_array() {
    let program = parse_fragment(br#"$items = ["a"]; $items[1] = "b"; return $items[1];"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("b".to_string()));
}
/// Verifies indexed array append writes use the next visible index.
#[test]
fn execute_program_appends_indexed_scope_array() {
    let program = parse_fragment(br#"$items = ["a"]; $items[] = "b"; return $items[1];"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("b".to_string()));
}
/// Verifies associative append starts at key zero when only string keys exist.
#[test]
fn execute_program_appends_assoc_scope_array_with_string_keys() {
    let program =
        parse_fragment(br#"$items = ["name" => "Ada"]; $items[] = "Grace"; return $items[0];"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
}
/// Verifies associative append uses one plus the largest existing integer key.
#[test]
fn execute_program_appends_assoc_scope_array_after_positive_int_key() {
    let program = parse_fragment(
        br#"$items = [2 => "two", "name" => "Ada"]; $items[] = "tail"; return $items[3];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
/// Verifies associative append preserves PHP's largest-negative-key behavior.
#[test]
fn execute_program_appends_assoc_scope_array_after_negative_int_key() {
    let program =
        parse_fragment(br#"$items = [-2 => "minus"]; $items[] = "tail"; return $items[-1];"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
/// Verifies mutating a borrowed scope array does not make the eval scope own it.
#[test]
fn execute_program_preserves_borrowed_array_ownership() {
    let program = parse_fragment(br#"$items[0] = "b";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let array = values.array_new(1).expect("create fake array");
    scope.set("items", array, ScopeCellOwnership::Borrowed);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let entry = scope.entry("items").expect("scope should contain items");

    assert_eq!(entry.cell(), array);
    assert_eq!(entry.flags().ownership, ScopeCellOwnership::Borrowed);
    assert_eq!(values.retains.iter().filter(|cell| **cell == array).count(), 1);
    assert_eq!(values.releases.iter().filter(|cell| **cell == array).count(), 1);
    assert_eq!(values.cell_owners[&(array.as_ptr() as usize)], 1);
    // The temporary array lease, normalized key, and RHS are released, not the caller's owner.
    assert!(values.releases.iter().any(|cell| values.get(*cell) == FakeValue::Int(0)));
    assert!(values.releases.iter().any(|cell| values.get(*cell) == FakeValue::String("b".into())));
    for released in values.releases.iter().filter(|cell| **cell != array) {
        assert_eq!(values.cell_owners[&(released.as_ptr() as usize)], 0);
    }
}
/// Verifies replacing an eval-owned scope value releases the old cell.
#[test]
fn execute_program_releases_replaced_scope_value() {
    let program = parse_fragment(br#"$x = "old"; $x = "new";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.releases.len(), 1);
    assert_eq!(
        values.get(values.releases[0]),
        FakeValue::String("old".to_string())
    );
}
/// Verifies unsetting an eval-owned scope value releases the old cell.
#[test]
fn execute_program_releases_unset_scope_value() {
    let program = parse_fragment(br#"$x = "old"; unset($x);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.releases.len(), 1);
    assert_eq!(
        values.get(values.releases[0]),
        FakeValue::String("old".to_string())
    );
}
/// Verifies break exits a runtime eval loop before later statements run.
#[test]
fn execute_program_break_exits_loop() {
    let program = parse_fragment(br#"while ($flag) { echo "a"; break; echo "b"; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let flag = values.bool_value(true).expect("create fake bool");
    scope.set("flag", flag, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "a");
}
/// Verifies continue restarts a runtime eval loop and observes later scope updates.
#[test]
fn execute_program_continue_restarts_loop() {
    let program = parse_fragment(
        br#"while ($flag) { $flag = false; continue; echo "unreachable"; } echo "done";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let flag = values.bool_value(true).expect("create fake bool");
    scope.set("flag", flag, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "done");
}
