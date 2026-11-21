//! Purpose:
//! Accounts for owned Reflection static/capture snapshots before static binding changes.
//!
//! Called from:
//! - Focused interpreter metadata snapshot ownership regressions.
//!
//! Key details:
//! - The retaining result arrays own entries after temporary key and value leases retire.

use super::super::*;
use super::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Repeated static and captured-variable maps preserve source values without retaining temporaries.
#[test]
fn strict_static_snapshot_eval_temporary_keys_values_and_owners() {
    for strict in [false, true] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        assert!(context.set_global_scope(&mut scope));
        let setup = [
            "function owned_static_value(): int {static $value=7;return $value;} \
             function make_owned_capture(int $value): callable {return static function() use($value): int {return $value;};}",
            "$callback=make_owned_capture(11);",
            "$staticReflection=new ReflectionFunction(\"owned_static_value\");",
            "$captureReflection=new ReflectionFunction($callback);",
        ];
        let reads = ["$staticSnapshot=$staticReflection->getStaticVariables();",
            "$captureSnapshot=$captureReflection->getClosureUsedVariables();",
            "echo $staticSnapshot[\"value\"],\":\",$captureSnapshot[\"value\"],\"|\";"];
        for (stage, source) in setup.into_iter().chain(reads.into_iter().cycle().take(9)).enumerate() {
            let program = crate::parser::parse_fragment_with_semantics(source.as_bytes(), context.semantic_profile()).unwrap();
            let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
                .unwrap_or_else(|status| panic!("stage={stage}, source={source}, strict={strict}, output={}: {status:?}", values.output));
            values.release(result).unwrap();
        }
        assert_eq!(values.output, "7:11|7:11|7:11|", "strict={strict}");
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners {
            assert_eq!(*owners, 0, "strict={strict}: {:?}", values.values[id]);
        }
    }
}

/// A reference wrapper must not change the object key used by synthetic Reflection metadata.
#[test]
fn strict_static_snapshot_fake_reference_preserves_object_identity() {
    let mut values = FakeOps::default();
    let object = values.new_object("stdClass").unwrap();
    let identity = values.object_identity(object).unwrap();
    let reference = values.reference_new(object).unwrap();
    assert_eq!(values.object_identity(reference).unwrap(), identity);
    assert_eq!(values.php_object_handle(reference).unwrap(), identity);
    values.release(object).unwrap();values.release(reference).unwrap();
    assert!(values.cell_owners.values().all(|owners| *owners == 0));
}

/// Walker and VM callers preserve silent constants, deferred effects and all snapshot owners.
#[test]
fn strict_static_reflection_deferred_initializers_and_constant_owners() {
    use super::strict_semantics::staged_numeric_execution;
    let source = include_str!("../../../../../tests/fixtures/strict/reflection_static_constants.php")
        .strip_prefix("<?php\n").unwrap().split("// Native runtime checks").next().unwrap();
    let expected = "NULL:NULL:9:9:3:two:4:2:3:C:NULL:4:reflected_pending:ff00:0|";
    for vm in [false, true] {
        for strict in [false, true] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            assert!(context.set_global_scope(&mut scope));
            let (setup, observations) = source.split_once("$reflection=").unwrap();
            staged_numeric_execution(setup, false, &mut context, &mut scope, &mut values).unwrap();
            let observations = if vm {
                let wrapper = format!("function reflection_observations() {{global $attempts;$reflection={observations}}}");
                staged_numeric_execution(&wrapper, false, &mut context, &mut scope, &mut values).unwrap();
                "if(false){function reflection_observations(){}} reflection_observations();".to_string()
            } else { format!("$reflection={observations}") };
            let result = staged_numeric_execution(&observations, vm, &mut context, &mut scope, &mut values);
            assert!(result.is_ok(), "vm={vm}, strict={strict}, output={}: {:?}", values.output, result.as_ref().err());
            assert_eq!(values.output, format!("{expected}{expected}NULL|NULL|NULL|7:1:7:1|"), "vm={vm}, strict={strict}");
            assert!(context.take_pending_throw().is_none());
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "vm={vm}, strict={strict}: {:?}", values.values[id]); }
        }
    }
}

/// Deferred probes preserve an existing Throwable and never install a null origin or storage cell.
#[test]
fn strict_static_reflection_deferral_preserves_pending_throw_and_uninitialized_storage() {
    let constant = |value| Box::new(EvalExpr::Const(EvalConst::Int(value)));
    let expressions = [
        EvalExpr::LoadVar("parameter".into()),
        EvalExpr::Call { name: "must_not_execute".into(), args: Vec::new() },
        EvalExpr::NewObject { class_name: "must_not_construct".into(), args: Vec::new() },
        EvalExpr::Binary { op: EvalBinOp::Div, left: constant(1), right: constant(0) },
        EvalExpr::Cast { target: EvalCastType::Int, expr: Box::new(EvalExpr::Const(EvalConst::Float(f64::INFINITY))) },
    ];
    for strict in [false, true] {
        for expr in &expressions {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict)).unwrap();
            let mut values = FakeOps::default();
            let pending = values.new_object("RuntimeException").unwrap();
            context.set_pending_throw(pending);
            let scope = ElephcEvalScope::new();
            let value = static_locals::initialize_constant("uninvoked", "value", expr, &mut context, &scope, &mut values).unwrap();
            assert!(value.is_none());
            assert!(context.static_local("uninvoked", "value").is_none());
            assert!(context.static_local_origin("uninvoked", "value").is_none());
            assert_eq!(context.take_pending_throw(), Some(pending));
            values.release(pending).unwrap();
            assert!(values.output.is_empty());
            assert!(values.cell_owners.values().all(|owners| *owners == 0), "strict={strict}, expr={expr:?}");
        }
    }
}

/// A failed class-constant preview leaves the dependent static ready for normal evaluation.
#[test]
fn strict_static_reflection_failed_class_constant_initializers_stay_deferred() {
    use super::strict_semantics::staged_numeric_execution;
    let setup = "class FailedStaticConstant{const VALUE=1/0;} \
        function failedStaticConstant(){static $value=FailedStaticConstant::VALUE;return $value;}";
    let reflection = "$reflection=new ReflectionFunction('failedStaticConstant');";
    let snapshot = "$snapshot=$reflection->getStaticVariables();";
    let observation = "echo gettype($snapshot['value']),'|';";
    let invocation = "try{failedStaticConstant();}catch(DivisionByZeroError $error){echo 'D';}";
    for vm in [false, true] {
        for strict in [false, true] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE
                .with(StrictFeature::Locals, strict)
                .with(StrictFeature::Arithmetic, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            assert!(context.set_global_scope(&mut scope));
            let result = staged_numeric_execution(setup, false, &mut context, &mut scope, &mut values);
            assert!(result.is_ok(), "stage=setup, vm={vm}, strict={strict}, output={}: {:?}", values.output, result.as_ref().err());
            if vm {
                let wrapper = format!("function failedConstantObservations(){{{reflection}{snapshot}{observation}{invocation}}}");
                staged_numeric_execution(&wrapper, false, &mut context, &mut scope, &mut values).unwrap();
                let source = "if(false){function failedConstantObservations(){}} failedConstantObservations();";
                let result = staged_numeric_execution(source, true, &mut context, &mut scope, &mut values);
                assert!(result.is_ok(), "stage=vm-observations, strict={strict}, output={}: {:?}", values.output, result.as_ref().err());
            } else {
                for (stage, source) in [
                    ("reflection", reflection),
                    ("snapshot", snapshot),
                    ("observation", observation),
                ] {
                    let result = staged_numeric_execution(source, false, &mut context, &mut scope, &mut values);
                    assert!(result.is_ok(), "stage={stage}, strict={strict}, output={}: {:?}", values.output, result.as_ref().err());
                }
                assert_eq!(values.output, "NULL|", "strict={strict}");
                assert!(context.static_local("failedStaticConstant", "value").is_none());
                let result = staged_numeric_execution(invocation, false, &mut context, &mut scope, &mut values);
                assert!(result.is_ok(), "stage=invocation, strict={strict}, output={}: {:?}", values.output, result.as_ref().err());
            }
            assert_eq!(values.output, "NULL|D", "vm={vm}, strict={strict}");
            assert!(context.static_local("failedStaticConstant", "value").is_none());
            assert!(context.take_pending_throw().is_none());
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "vm={vm}, strict={strict}: {:?}", values.values[id]); }
        }
    }
}

/// Array constants preserve union keys, equality order and every temporary owner.
#[test]
fn strict_static_reflection_array_constant_operators_preserve_keys_and_owners() {
    use super::strict_semantics::staged_numeric_execution;
    let source = r#"
        function reflected_arrays() {
            static $merged=[2=>1,'key'=>2]+[2=>9,3=>4,'tail'=>5];
            static $equal=['a'=>[1],'b'=>2]==['b'=>2,'a'=>[1]];
            static $strict=['b'=>2,'a'=>[1]]===['a'=>[1],'b'=>2];
            static $nested=[false]==[[]];
        }
        $reflection=new ReflectionFunction('reflected_arrays');
        $snapshot=$reflection->getStaticVariables();
        echo $snapshot['merged'][2],':',$snapshot['merged'][3],':',$snapshot['merged']['key'],':',$snapshot['merged']['tail'],':';
        echo $snapshot['equal'],':',$snapshot['strict'],':',$snapshot['nested'];
    "#;
    for strict in [false, true] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        assert!(context.set_global_scope(&mut scope));
        staged_numeric_execution(source, false, &mut context, &mut scope, &mut values).unwrap();
        assert_eq!(values.output, "1:4:2:5:1::1");
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "strict={strict}: {:?}", values.values[id]); }
    }
}
