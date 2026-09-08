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
