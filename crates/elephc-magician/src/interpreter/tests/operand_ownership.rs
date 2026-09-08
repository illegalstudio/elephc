//! Purpose:
//! Verifies temporary operand leases, persistent defaults, and raw native reference slots.
//!
//! Called from:
//! - The Magician interpreter unit-test harness.
//!
//! Key details:
//! - FakeOps records explicit retain/release pairs; native GC fixtures verify actual lifetimes.
//! - Native Mixed reference slots must stay pointer-sized despite Rust-only handle provenance.

use super::super::*;
use super::support::*;

/// Escaping borrowed Throwables acquire an owner before argument cleanup; fresh throws transfer theirs.
#[test]
fn pending_throwables_own_their_cells_after_function_return_control() {
    for borrowed in [false, true] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let source = values.new_object("Exception").unwrap();
        let operand = if borrowed { source.borrowed() } else { source };
        let result = eval_declared_return_control_value(
            None, None, None, EvalControl::Throw(operand), &mut context, &mut values,
        );
        assert_eq!(result, Err(EvalStatus::UncaughtThrowable));
        let thrown = context.take_pending_throw().unwrap();
        assert!(!thrown.is_borrowed());
        assert_eq!(values.retains.len(), usize::from(borrowed));
        if borrowed {
            values.release(source).unwrap();
        }
        assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 1);
        values.release(thrown).unwrap();
        assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 0);
    }
}

/// Static storage retains borrowed cells once, preserves same-cell writes, and releases replacements.
#[test]
fn static_property_assignments_acquire_independent_storage_owners() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let source = values.string("kept").unwrap();
    for _ in 0..2 {
        store_static_property_value("Owner", "value", source.borrowed(), &mut context, &mut values).unwrap();
    }
    assert_eq!(values.retains, vec![source]);
    assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 2);
    values.release(source).unwrap();
    let stored = context.static_property("Owner", "value").unwrap();
    assert_eq!(values.string_bytes(stored).unwrap(), b"kept");
    assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 1);

    let replacement = values.string("replacement").unwrap();
    store_static_property_value("Owner", "value", replacement, &mut context, &mut values).unwrap();
    assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 0);
    assert_eq!(values.cell_owners[&(replacement.as_ptr() as usize)], 1);
    assert_eq!(values.retains, vec![source]);
    values.release(replacement).unwrap();
}

/// Echo consumes an expression temporary but preserves a variable's independent storage owner.
#[test]
fn echo_balances_temporary_and_borrowed_operands() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    eval_echo_expr(
        &EvalExpr::Const(EvalConst::String("temporary".into())),
        &mut context, &mut scope, &mut values,
    ).unwrap();
    assert_eq!(values.releases.len(), 1);
    let temporary = values.releases[0];
    assert_eq!(values.cell_owners[&(temporary.as_ptr() as usize)], 0);

    let stored = values.string("stored").unwrap();
    scope.set("value", stored, ScopeCellOwnership::Owned);
    eval_echo_expr(
        &EvalExpr::LoadVar("value".into()), &mut context, &mut scope, &mut values,
    ).unwrap();
    assert_eq!(values.cell_owners[&(stored.as_ptr() as usize)], 1);
    assert_eq!(values.retains, vec![stored]);
    assert_eq!(values.releases, vec![temporary, stored]);
}

/// String repetition consumes temporary inputs even when a negative count rejects the call.
#[test]
fn string_repeat_releases_operands_on_success_and_failure() {
    for count in [3, -1] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let args = [
            EvalExpr::Const(EvalConst::String("x".into())),
            EvalExpr::Const(EvalConst::Int(count)),
        ];
        let result = eval_builtin_str_repeat(&args, &mut context, &mut scope, &mut values);
        // Integer coercion owns one extra cell in addition to the two source operands.
        assert_eq!(values.releases.len(), 3);
        assert_eq!(values.get(values.releases[0]), FakeValue::Int(count));
        assert_eq!(values.get(values.releases[1]), FakeValue::String("x".into()));
        assert_eq!(values.get(values.releases[2]), FakeValue::Int(count));
        for input in &values.releases {
            assert_eq!(values.cell_owners[&(input.as_ptr() as usize)], 0);
        }
        if count < 0 {
            assert_eq!(result, Err(EvalStatus::RuntimeFatal));
        } else {
            let result = result.unwrap();
            assert_eq!(values.string_bytes(result).unwrap(), b"xxx");
            assert_eq!(values.cell_owners[&(result.as_ptr() as usize)], 1);
            values.release(result).unwrap();
        }
    }
}

/// Borrowed string inputs keep their scope owner after the repetition operand lease is released.
#[test]
fn string_repeat_preserves_borrowed_scope_input() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let source = values.string("x").unwrap();
    scope.set("source", source, ScopeCellOwnership::Owned);
    let args = [
        EvalExpr::LoadVar("source".into()),
        EvalExpr::Const(EvalConst::Int(2)),
    ];
    let result = eval_builtin_str_repeat(&args, &mut context, &mut scope, &mut values).unwrap();
    assert_eq!(values.retains, vec![source]);
    assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 1);
    assert_eq!(values.string_bytes(result).unwrap(), b"xx");
    values.release(result).unwrap();
}

/// Reflection storage retains a borrowed argument and balances replacements and identical writes.
#[test]
fn static_property_storage_owns_reflection_arguments() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let first = values.string("first").unwrap();
    store_borrowed_static_property("Stored", "value", first.borrowed(), &mut context, &mut values).unwrap();
    values.release(first).unwrap();
    assert_eq!(context.static_property("Stored", "value"), Some(first));
    assert_eq!(values.cell_owners[&(first.as_ptr() as usize)], 1);
    store_borrowed_static_property("Stored", "value", first.borrowed(), &mut context, &mut values).unwrap();
    assert_eq!(values.cell_owners[&(first.as_ptr() as usize)], 1);
    let second = values.int(5).unwrap();
    store_borrowed_static_property("Stored", "value", second.borrowed(), &mut context, &mut values).unwrap();
    values.release(second).unwrap();
    assert_eq!(values.cell_owners[&(first.as_ptr() as usize)], 0);
    assert_eq!(values.cell_owners[&(second.as_ptr() as usize)], 1);
    assert_eq!(values.retains, vec![first, second]);
}

/// Receiver leases must not make the fake runtime run a destructor before the last owner exits.
#[test]
fn retained_object_lease_is_not_a_final_release() {
    let mut values = FakeOps::default();
    let object = values.new_object("stdClass").unwrap();
    let lease = values.retain(object).unwrap();
    assert_eq!(values.final_object_identity_for_release(lease).unwrap(), None);
    values.release(lease).unwrap();
    assert_eq!(values.final_object_identity_for_release(object).unwrap(),
        Some(values.object_identity(object).unwrap()));
}

/// A throwing eval destructor consumes the final object owner instead of leaking it on early return.
#[test]
fn throwing_eval_destructor_releases_its_final_owner() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let program = parse_fragment(br#"
class ThrowingEvalLease {
    public function __destruct() { echo "drop"; throw new Exception("release"); }
}
$value = new ThrowingEvalLease();
"#).unwrap();
    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    values.release(result).unwrap();
    let object = scope.unset("value").unwrap();
    assert_eq!(values.cell_owners[&(object.as_ptr() as usize)], 1);
    assert_eq!(eval_release_value(&mut context, &mut values, object), Err(EvalStatus::UncaughtThrowable));
    assert_eq!(values.cell_owners[&(object.as_ptr() as usize)], 0);
    assert_eq!(values.output, "drop");
    let thrown = context.take_pending_throw().expect("destructor exception survives release");
    values.release(thrown).unwrap();
}

/// Adapters forwarding an argument receive a borrow that survives cleanup through a retained return.
#[test]
fn method_arguments_keep_borrowed_returns_alive() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let args = [EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(7)))];
    let result = with_eval_method_arguments(
        &args, &mut context, &mut scope, &mut values,
        |arguments, _, _, _| {
            assert!(arguments[0].value.is_borrowed());
            Ok(arguments[0].value)
        },
    ).unwrap();
    assert_eq!(values.retains, vec![result]);
    assert_eq!(values.releases, vec![result]);
    assert!(!result.is_borrowed());
}

/// Call-array extraction releases its cells on errors and promotes borrowed callback returns.
#[test]
fn call_array_arguments_balance_extracted_owners_on_return_and_error() {
    for fail_dispatch in [false, true] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let array = values.assoc_new(1).unwrap();
        let key = values.string("value").unwrap();
        let source = values.string("original").unwrap();
        let array = values.array_set(array, key, source).unwrap();
        values.release(key).unwrap();
        // FakeOps returns the stored handle instead of creating the runtime's extracted cell.
        // Its source owner therefore supplies the single extraction consumed by this test.
        // Native collection ownership is covered by the codegen argument-lifetime regressions.
        let mut extracted = None;
        let result = with_eval_array_call_arguments(
            array, &mut context, &mut values,
            |arguments, _, _| {
                assert_eq!(arguments[0].name.as_deref(), Some("value"));
                assert!(arguments[0].value.is_borrowed());
                extracted = Some(arguments[0].value);
                if fail_dispatch { Err(EvalStatus::RuntimeFatal) } else { Ok(arguments[0].value) }
            },
        );
        assert_eq!(result.is_err(), fail_dispatch);
        let extracted = extracted.unwrap();
        assert_eq!(values.cell_owners[&(extracted.as_ptr() as usize)], usize::from(!fail_dispatch));
        if let Ok(result) = result {
            assert_eq!(values.string_bytes(result).unwrap(), b"original");
            values.release(result).unwrap();
        }
        values.release(array).unwrap();
    }
}

/// Dispatch failures still release source-created arguments while preserving borrowed variables.
#[test]
fn failed_method_dispatch_releases_temporary_arguments() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let borrowed = values.int(1).unwrap();
    scope.set("existing", borrowed, ScopeCellOwnership::Owned);
    let args = [
        EvalCallArg::positional(EvalExpr::LoadVar("existing".into())),
        EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(7))),
    ];
    let result = with_eval_method_arguments(
        &args, &mut context, &mut scope, &mut values,
        |_, _, _, _| Err(EvalStatus::RuntimeFatal),
    );
    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    assert_eq!(values.retains, vec![borrowed]);
    assert_eq!(values.releases.len(), 2);
    assert_eq!(values.releases[0], borrowed);
    assert_eq!(values.cell_owners[&(borrowed.as_ptr() as usize)], 1);
}

/// A later argument may replace the first argument's variable without destroying its value lease.
#[test]
fn call_arguments_retain_source_before_later_global_replacement() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let source = values.string("original").unwrap();
    scope.set("source", source, ScopeCellOwnership::Owned);
    context.set_global_scope(&mut scope);
    let declaration = parse_fragment(
        b"function replaceArgument() { global $source; $source = 9; return 0; }",
    ).unwrap();
    execute_program_outcome_with_context(&mut context, &declaration, &mut scope, &mut values).unwrap();
    let args = [
        EvalCallArg::positional(EvalExpr::LoadVar("source".into())),
        EvalCallArg::positional(EvalExpr::Call { name: "replaceargument".into(), args: vec![] }),
    ];
    let result = with_eval_call_arguments(
        &args, &mut context, &mut scope, &mut values,
        |arguments, _, scope, values| {
            assert_ne!(scope.visible_cell("source"), Some(source));
            assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 1);
            assert_eq!(arguments[0].value, source);
            Ok(arguments[0].value)
        },
    ).unwrap();
    assert_eq!(result, source);
    assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 1);
    values.release(result).unwrap();
    assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 0);
}

/// Ref-aware builtin adapters retain their first input before a later argument replaces its global.
#[test]
fn builtin_arguments_retain_source_before_later_global_replacement() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let source = values.string("strlen").unwrap();
    scope.set("source", source, ScopeCellOwnership::Owned);
    context.set_global_scope(&mut scope);
    let declaration = parse_fragment(
        b"function replaceCallable() { global $source; $source = null; return true; }",
    ).unwrap();
    execute_program_outcome_with_context(&mut context, &declaration, &mut scope, &mut values).unwrap();
    let args = [
        EvalCallArg::positional(EvalExpr::LoadVar("source".into())),
        EvalCallArg::positional(EvalExpr::Call { name: "replacecallable".into(), args: vec![] }),
    ];
    let result = eval_builtin_is_callable_call(&args, &mut context, &mut scope, &mut values).unwrap();
    assert!(values.truthy(result).unwrap());
    assert!(values.retains.contains(&source));
    assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 0);
    values.release(result).unwrap();
}

/// Named builtin gaps own synthesized defaults and release them on success and rejected operations.
#[test]
fn named_builtin_defaults_are_released_after_dispatch() {
    for pad_type in [0, 99] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let source = values.string("x").unwrap();
        scope.set("source", source, ScopeCellOwnership::Owned);
        let args = [
            EvalCallArg::named("string", EvalExpr::LoadVar("source".into())),
            EvalCallArg::named("length", EvalExpr::Const(EvalConst::Int(3))),
            EvalCallArg::named("pad_type", EvalExpr::Const(EvalConst::Int(pad_type))),
        ];
        let result = eval_builtin_call("str_pad", &args, &mut context, &mut scope, &mut values);
        if pad_type == 0 {
            let result = result.unwrap();
            assert_eq!(values.string_bytes(result).unwrap(), b"  x");
            values.release(result).unwrap();
        } else {
            assert_eq!(result, Err(EvalStatus::RuntimeFatal));
        }
        let defaults = values.releases.iter().copied()
            .filter(|cell| values.get(*cell) == FakeValue::String(" ".into()))
            .collect::<Vec<_>>();
        assert_eq!(defaults.len(), 1, "the omitted pad string must have one released owner");
        assert_eq!(values.cell_owners[&(defaults[0].as_ptr() as usize)], 0);
        assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 1);
    }
}

/// A malformed later spread releases itself and any previously evaluated source arguments.
#[test]
fn failed_argument_evaluation_releases_previous_temporaries() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let args = [
        EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(7))),
        EvalCallArg::spread(EvalExpr::Const(EvalConst::Int(9))),
    ];
    let result = with_eval_method_arguments(
        &args, &mut context, &mut scope, &mut values,
        |_, _, _, _| panic!("invalid spread must fail before dispatch"),
    );
    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    assert_eq!(values.releases.len(), 2);
    assert_eq!(values.get(values.releases[0]), FakeValue::Int(9));
    assert_eq!(values.get(values.releases[1]), FakeValue::Int(7));
}

/// Native activation cleanup releases defaults while leaving borrowed caller operands intact.
#[test]
fn native_bound_argument_cleanup_preserves_borrowed_inputs() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let borrowed = values.int(7).unwrap();
    let default = values.string("default").unwrap();
    let args = [borrowed.borrowed(), default].into_iter().map(|value| BoundMethodArg {
        value, ref_target: None, variadic_ref_targets: Vec::new(),
    }).collect::<Vec<_>>();
    release_native_bound_args(&args, &mut context, &mut values).unwrap();
    assert_eq!(values.releases, vec![default]);
}

/// Reference writeback acquires caller ownership before a native coercion temporary is released.
#[test]
fn native_reference_writeback_retains_replacement_storage() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let original = values.int(7).unwrap();
    let converted = values.string("7").unwrap();
    scope.set("value", original, ScopeCellOwnership::Owned);
    let args = vec![BoundMethodArg {
        value: converted,
        ref_target: Some(EvalReferenceTarget::Variable {
            scope: &mut scope, name: "value".into(),
        }),
        variadic_ref_targets: Vec::new(),
    }];
    write_back_native_callable_ref_args(&args, &mut context, &mut values).unwrap();
    release_native_bound_args(&args, &mut context, &mut values).unwrap();
    assert_eq!(scope.visible_cell("value"), Some(converted));
    assert_eq!(scope.entry("value").unwrap().flags().ownership, ScopeCellOwnership::Owned);
    assert_eq!(values.retains, vec![converted]);
    assert_eq!(values.releases, vec![original, converted]);
}

/// A zero-argument activation does not allocate a synthetic variadic array or PHP variable.
#[test]
fn empty_activation_has_no_synthetic_argument_cells() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let binding = bind_evaluated_function_args_with_ref_mode(
        &[], &[], &[], &[], &[], Vec::new(), EvalByRefBindingMode::RequireTarget,
        &mut context, &mut values,
    ).unwrap();
    assert!(binding.params.is_empty());
    assert!(binding.args.is_empty());
    assert_eq!(binding.frame.actual_count(), 0);
    assert!(values.retains.is_empty());
    assert!(values.releases.is_empty());
}

/// Surplus argument snapshots are owned by the activation and released without consuming caller cells.
#[test]
fn activation_surplus_snapshot_releases_its_own_lease() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let value = values.string("extra").unwrap();
    let result = values.int(42).unwrap();
    let binding = bind_evaluated_function_args_with_ref_mode(
        &[], &[], &[], &[], &[], positional_args(vec![value.borrowed()]),
        EvalByRefBindingMode::RequireTarget, &mut context, &mut values,
    ).unwrap();
    assert!(binding.params.is_empty());
    assert!(binding.args.is_empty());
    assert_eq!(binding.frame.surplus_arg(0), Some(value));
    context.push_function_args(binding.frame);
    assert_eq!(release_function_args(Ok(result), &mut context, &mut values).unwrap(), result);
    assert_eq!(values.retains, vec![value]);
    assert_eq!(values.releases, vec![value]);
    assert!(context.current_function_args().is_none());
}

/// Borrowed reads acquire a temporary lease while freshly allocated scalar operands are consumed.
#[test]
fn scalar_operand_cleanup_preserves_borrowed_scope_owners() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let source = values.int(2).unwrap();
    scope.set("source", source, ScopeCellOwnership::Owned);
    let expr = EvalExpr::Binary {
        op: EvalBinOp::Lt,
        left: Box::new(EvalExpr::LoadVar("source".into())),
        right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
    };
    assert!(eval_condition(&expr, &mut context, &mut scope, &mut values).unwrap());
    assert_eq!(values.retains, vec![source]);
    assert_eq!(values.releases.len(), 3);
    assert_eq!(values.releases.iter().filter(|cell| **cell == source).count(), 1);
    assert_eq!(scope.visible_cell("source"), Some(source));
    assert!(scope.visible_cell("source").unwrap().is_borrowed());
}

/// Argument-evaluation failures release previously acquired leases without consuming scope storage.
#[test]
fn failed_operand_evaluation_releases_earlier_arguments() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let source = values.int(2).unwrap();
    scope.set("source", source, ScopeCellOwnership::Owned);
    let first = EvalExpr::LoadVar("source".into());
    let second = EvalExpr::Call { name: "missing_operand_function".into(), args: vec![] };
    let result = with_eval_operands(
        &[&first, &second], &mut context, &mut scope, &mut values,
        |_, _, _, _| panic!("consumer must not run after argument evaluation fails"),
    );
    assert!(result.is_err());
    assert_eq!(values.retains.iter().filter(|cell| **cell == source).count(), 1);
    assert_eq!(values.releases.iter().filter(|cell| **cell == source).count(), 1);
}

/// Statement consumers release owned temporaries and retained storage even when the write fails.
#[test]
fn statement_operand_cleanup_covers_success_and_failure() {
    for fail in [false, true] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let source = values.int(2).unwrap();
        scope.set("source", source, ScopeCellOwnership::Owned);
        let borrowed = EvalExpr::LoadVar("source".into());
        let temporary = EvalExpr::Const(EvalConst::Int(3));
        let result = with_eval_void_operands(
            &[&borrowed, &temporary], &mut context, &mut scope, &mut values,
            |args, _, _, values| {
                assert_eq!(args[0], source);
                assert!(values.releases.is_empty());
                if fail { Err(EvalStatus::RuntimeFatal) } else { Ok(()) }
            },
        );
        assert_eq!(result.is_err(), fail);
        assert_eq!(values.retains, vec![source]);
        assert_eq!(values.releases.len(), 2);
        assert_eq!(values.releases.iter().filter(|cell| **cell == source).count(), 1);
        assert_eq!(scope.visible_cell("source"), Some(source));
    }
}

/// Same-cell assignments consume a new lease and release the old owner, including reference aliases.
#[test]
fn owned_assignments_balance_identical_reference_cells() {
    for reference in [false, true] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let source = values.int(2).unwrap();
        scope.set("source", source, ScopeCellOwnership::Owned);
        let name = if reference {
            scope.set_reference("alias", "source", source, ScopeCellOwnership::Borrowed);
            "alias"
        } else { "source" };
        let program = parse_fragment(format!("${name} = ${name}; ${name} = ${name};").as_bytes()).unwrap();
        execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
        assert_eq!(values.retains.iter().filter(|cell| **cell == source).count(), 2);
        assert_eq!(values.releases.iter().filter(|cell| **cell == source).count(), 2);
        assert_eq!(scope.drain_owned_cells(), vec![source]);
    }
}

/// Materialized leases retain borrowed values and consume fresh ones, including failed writes.
#[test]
fn materialized_value_cleanup_covers_success_and_failure() {
    for borrowed in [false, true] {
        for fail in [false, true] {
            let mut values = FakeOps::default();
            let mut context = ElephcEvalContext::new();
            let cell = values.int(5).unwrap();
            let input = if borrowed { cell.borrowed() } else { cell };
            let result = with_eval_value_lease(input, &mut context, &mut values, |value, _, values| {
                assert_eq!(value, cell);
                assert!(!value.is_borrowed());
                assert!(values.releases.is_empty());
                if fail { Err(EvalStatus::RuntimeFatal) } else { Ok(()) }
            });
            assert_eq!(result.is_err(), fail);
            assert_eq!(values.retains.len(), usize::from(borrowed));
            assert_eq!(values.releases, vec![cell]);
        }
    }
}

/// A failed compound-assignment RHS releases the prior property read and the receiver lease.
#[test]
fn failed_compound_property_rhs_releases_prior_operands() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let cell = values.int(5).unwrap();
    let object = values.alloc(FakeValue::Object(vec![("n".into(), cell)]));
    scope.set("box", object, ScopeCellOwnership::Owned);
    let program = parse_fragment(b"$box->n += missing_compound_operand();").unwrap();
    assert!(execute_program_with_context(&mut context, &program, &mut scope, &mut values).is_err());
    assert_eq!(values.retains, vec![object]);
    let operands = values.releases.iter().copied()
        .filter(|released| *released == cell || *released == object).collect::<Vec<_>>();
    assert_eq!(operands, vec![cell, object]);
    assert_eq!(scope.visible_cell("box"), Some(object));
}

/// A failed indexed-property RHS releases the copied array, original read, key, and receiver.
#[test]
fn failed_property_array_rhs_releases_prior_operands() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let array = values.array_new(0).unwrap();
    let object = values.alloc(FakeValue::Object(vec![("items".into(), array)]));
    scope.set("box", object, ScopeCellOwnership::Owned);
    let program = parse_fragment(b"$box->items[0] = missing_array_operand();").unwrap();
    assert!(execute_program_with_context(&mut context, &program, &mut scope, &mut values).is_err());
    assert_eq!(values.releases.iter().filter(|cell| **cell == array).count(), 1);
    assert_eq!(values.releases.iter().filter(|cell| **cell == object).count(), 1);
    assert_eq!(scope.visible_cell("box"), Some(object));
}

/// Shallow copies release iteration cells and abandon partial arrays if insertion fails.
#[test]
fn shallow_array_copy_releases_iteration_leases() {
    for fail in [false, true] {
        let mut values = FakeOps::default();
        let cell = values.int(7).unwrap();
        let array = values.alloc(FakeValue::Array(vec![cell]));
        if fail { values.fail_array_set_call = Some(0); }
        let result = values.array_clone_shallow(array);
        assert_eq!(result.is_err(), fail);
        assert_eq!(values.releases.iter().filter(|released| **released == cell).count(), 1);
        assert!(!values.releases.contains(&array));
        assert_eq!(values.releases.len(), if fail { 3 } else { 2 });
    }
}

/// Class-constant defaults acquire a builder-owned lease instead of consuming the persistent cell.
#[test]
fn class_vars_default_cleanup_preserves_class_constants() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let program = parse_fragment(br#"
class OperandDefaults { const TOKEN = "keep"; public string $value = self::TOKEN; }
$first = get_class_vars("OperandDefaults"); unset($first);
$second = get_class_vars("OperandDefaults"); unset($second);
"#).unwrap();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    let constant = context.class_constant_cell("OperandDefaults", "TOKEN").unwrap();
    assert_eq!(values.retains.iter().filter(|cell| **cell == constant).count(), 2);
    assert_eq!(values.releases.iter().filter(|cell| **cell == constant).count(), 2);
}

/// First and cached PropertyHookType case reads both borrow the context's singleton owner.
#[test]
fn property_hook_case_condition_preserves_cached_owner() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let expr = EvalExpr::ClassConstantFetch {
        class_name: "PropertyHookType".into(), constant: "Get".into(),
    };
    assert!(eval_condition(&expr, &mut context, &mut scope, &mut values).unwrap());
    assert!(eval_condition(&expr, &mut context, &mut scope, &mut values).unwrap());
    let case = context.enum_case("PropertyHookType", "Get").unwrap();
    assert_eq!(values.retains.iter().filter(|cell| **cell == case).count(), 2);
    assert_eq!(values.releases.iter().filter(|cell| **cell == case).count(), 2);
}

/// Array literal keys, automatic-key arithmetic, and nested results release construction leases.
#[test]
fn array_literal_construction_releases_temporary_leases() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let program = parse_fragment(br#"return [3 => "a", 1 => "b", "5" => [7], 8, "z" => 9];"#).unwrap();
    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    for (id, value) in &values.values {
        if *id != result.as_ptr() as usize {
            assert_eq!(values.releases.iter().filter(|cell| cell.as_ptr() as usize == *id).count(), 1,
                "literal operand {id} ({value:?}) must release exactly its construction lease");
        }
    }
    assert!(!values.releases.contains(&result));
}

/// Failed insertion frees its key/value operands and the partially constructed result array.
#[test]
fn array_literal_insertion_failure_releases_partial_result() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    values.fail_array_set_call(1);
    let program = parse_fragment(br#"return ["one" => 1, "two" => 2];"#).unwrap();
    assert!(execute_program_with_context(&mut context, &program, &mut scope, &mut values).is_err());
    for id in values.values.keys() {
        assert_eq!(values.releases.iter().filter(|cell| cell.as_ptr() as usize == *id).count(), 1,
            "abandoned literal cell {id} must be released");
    }
}

/// A returned reference parameter cannot transfer the owner already exposed through writeback.
#[test]
fn returned_reference_parameter_retains_an_independent_result_lease() {
    let mut values = FakeOps::default();
    let mut scope = ElephcEvalScope::new();
    let cell = values.int(25).unwrap();
    scope.set_reference("value", "value", cell, ScopeCellOwnership::Owned);
    let result = retain_static_local_return(Ok(cell.borrowed()), &[], &scope, &mut values).unwrap();
    assert_eq!(result, cell);
    assert!(!result.is_borrowed());
    assert_eq!(values.retains, vec![cell]);
}

/// Mixed invoker writeback updates exactly one native pointer and preserves the neighboring slot.
#[test]
fn mixed_native_reference_slots_remain_pointer_sized() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let old = values.int(1).unwrap();
    let replacement = values.int(2).unwrap();
    let guard = values.int(3).unwrap();
    let mut slots = [old.as_ptr(), guard.as_ptr()];
    let target = EvalReferenceTarget::InvokerSlot {
        slot: slots.as_mut_ptr() as usize, source_tag: EVAL_TAG_MIXED,
    };
    write_back_method_ref_target(&target, replacement.borrowed(), &mut context, &mut values).unwrap();
    assert_eq!(slots, [replacement.as_ptr(), guard.as_ptr()]);
    let read = eval_reference_target_value(&target, &mut context, &mut values).unwrap();
    assert_eq!(read, replacement);
    assert!(!read.is_borrowed());
    assert_eq!(values.releases, vec![old]);
    assert_eq!(values.retains, vec![replacement, replacement]);
}
