//! Purpose:
//! Checks declared initializer alternatives and owners on persistent static bindings.
//!
//! Called from:
//! - Focused interpreter static-origin regressions with walker and VM callers.
//!
//! Key details:
//! - Rejected writes retain the old static value and every activation lease retires.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Static producer alternatives survive calls without retaining rejected or detached values.
#[test]
fn strict_static_locals_eval_origins_alternatives_and_owners() {
    for vm in [false, true] {
        for strict in [false, true] {
            for (declared, initial, next, accepts) in [
                ("int", "1", "2.5", false), ("int", "1", "2", true),
                ("int|float", "1", "2.5", true), ("?int", "null", "2", true),
                ("?int", "null", "2.5", false), ("mixed", "1", "2.5", true),
                ("", "1", "2.5", true), ("array", "[1]", "[2.5]", true),
                ("array", "[1]", "2", false), ("string", "'old'", "2", false),
                ("bool", "false", "1", false),
            ] {
                let mut context = ElephcEvalContext::new();
                context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict)).unwrap();
                let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                assert!(context.set_global_scope(&mut scope));
                let return_hint = if declared.is_empty() { String::new() } else { format!(": {declared}") };
                let setup = format!("function staticproducer(){return_hint} {{return {initial};}} \
                    function staticwrite(mixed $next) {{static $value=staticproducer();$value=$next;return $value;}} \
                    $candidate={next};");
                staged_numeric_execution(&setup, false, &mut context, &mut scope, &mut values).unwrap();
                let source = if vm { "if(false){function staticwrite($next){}} return staticwrite($candidate);" }
                    else { "return staticwrite($candidate);" };
                let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
                if !strict || accepts {
                    let Ok(EvalControl::Return(value)) = result else { panic!("{declared}, {initial}, {next}, strict={strict}, vm={vm}: {:?}", result.as_ref().err()); };
                    assert_static_value(&values, value, scope.visible_cell("candidate").unwrap());
                    values.release(value).unwrap();
                } else {
                    assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{declared}, {initial}, {next}, vm={vm}: {:?}", result.as_ref().err());
                    let error = context.take_pending_throw().expect("static binding TypeError owner");
                    assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
                    let old = context.static_local("staticwrite", "value").unwrap();
                    let expected = staged_numeric_execution(&format!("return {initial};"), false, &mut context, &mut scope, &mut values).unwrap();
                    let EvalControl::Return(expected) = expected else { panic!("initial value"); };
                    assert_static_value(&values, old, expected); values.release(expected).unwrap();
                }
                for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
                for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{declared}, {initial}, {next}, strict={strict}, vm={vm}: {:?}", values.values[id]); }
            }
        }
    }
}

/// Compares copied array contents instead of the incidental identities of their element cells.
fn assert_static_value(values: &FakeOps, actual: RuntimeCellHandle, expected: RuntimeCellHandle) {
    match (values.get(actual), values.get(expected)) {
        (FakeValue::Array(actual), FakeValue::Array(expected)) => {
            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.into_iter().zip(expected) { assert_static_value(values, actual, expected); }
        }
        (actual, expected) => assert_eq!(actual, expected),
    }
}

/// Recursive initialization and later aliases preserve the independent persistent static owner.
#[test]
fn strict_static_locals_eval_recursive_initialization_and_detachment_owners() {
    for vm in [false, true] {
        for strict in [false, true] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            assert!(context.set_global_scope(&mut scope));
            let setup = r#"
                $attempts=0;
                function staticinitial(): int {
                    global $attempts;$attempts++;
                    if($attempts===1){return staticrecursive()+10;}return 2;
                }
                function staticrecursive(): int {static $value=staticinitial();return $value;}
                function staticdetach(mixed $next): void {
                    static $value=1;echo $value,":";
                    $copy=$next;$value=&$copy;$value=3.5;echo $value,"|";
                }
                function staticunset(): void {
                    static $value=1;echo $value,"|";unset($value);$value=2.5;
                }
                class StaticDiscardThrows {
                    public function __destruct(){throw new RuntimeException("retired");}
                }
                function staticretirement(): void {
                    $value=new StaticDiscardThrows();
                    try{static $value=1;}catch(Throwable $error){echo gettype($value),":",$value,"|";}
                }
                function runstatics(): void {
                    staticretirement();staticretirement();
                    echo staticrecursive(),":",staticrecursive(),"|";
                    staticdetach(2.5);staticdetach(2.5);staticunset();staticunset();
                }
            "#;
            staged_numeric_execution(setup, false, &mut context, &mut scope, &mut values).unwrap();
            let source = if vm { "if(false){function runstatics(){}} return runstatics();" }
                else { "return runstatics();" };
            let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values).unwrap();
            let EvalControl::Return(result) = result else { panic!("static caller result"); };
            values.release(result).unwrap();
            assert_eq!(values.output, "integer:1|integer:1|2:2|1:3.5|1:3.5|1|1|", "vm={vm}, strict={strict}");
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners {
                assert_eq!(*owners, 0, "vm={vm}, strict={strict}: {:?}", values.values[id]);
            }
        }
    }
}
