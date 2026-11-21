//! Purpose:
//! Checks builtin original keys and key-producing arrays under both key policies.
//!
//! Called from:
//! - Focused staged walker and bytecode profile regressions.
//!
//! Key details:
//! - Fixtures share native compiler coverage and assert complete owner retirement.

use super::super::*;
use super::support::*;
use super::strict_semantics::staged_numeric_execution;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Invokes already-declared fixtures through either engine without compiling their bodies twice.
fn call(source: &str, declaration: &str, vm: bool, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut FakeOps,
) {
    let source = if vm { format!("if(false){{{declaration}}} {source}") } else { source.into() };
    let result = staged_numeric_execution(&source, vm, context, scope, values);
    assert!(result.is_ok(), "vm={vm}: {source}: {:?}; {}", result.as_ref().err(), values.output);
}

/// Both engines preserve source keys across direct, named, spread and callable builtin calls.
#[test]
fn exact_eval_builtin_keys_walker_vm_calls_producers_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/builtin_key_inputs.php")
        .strip_prefix("<?php\n").unwrap();
    for exact in [false, true] {
        for vm in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            values.fixed_profile = Some(profile); assert!(context.set_global_scope(&mut scope));
            staged_numeric_execution(source, false, &mut context, &mut scope, &mut values).unwrap();
            let mut expected = String::new();
            for (argument, value, invalid) in [("true", 20, true), ("0.0", 10, true), ("null", 30, true),
                ("0", 10, false), ("'0'", if exact { 31 } else { 10 }, false), ("''", 30, false)] {
                call(&format!("builtinKeyInputs({argument});"), "function builtinKeyInputs($key){}", vm,
                    &mut context, &mut scope, &mut values);
                if exact && invalid { expected.push_str("D|N|S|C|U|I|E|Q|"); }
                else { expected.push_str(&format!("d|n|s|c|u|i|e|{value}|")); }
            }
            call("builtinProducedKeys();", "function builtinProducedKeys(){}", vm, &mut context, &mut scope, &mut values);
            let kind = if exact { "string" } else { "integer" };
            expected.push_str(&format!("{kind}:0:7|string:01:7|{kind}:0:11|string:01:22|{kind}:0:2|string:01:1|{kind}:0:2|string:01:1|"));
            expected.push_str(if exact { "11|33|" } else { "22|44|" });
            assert_eq!(values.output, expected, "exact={exact} vm={vm}");
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners {
                assert_eq!(*owners, 0, "exact={exact} vm={vm}: {:?}", values.values[id]);
            }
        }
    }
}

/// Producer guards retire partial results and allow null to select complete column rows.
#[test]
fn exact_eval_builtin_key_producers_reject_original_kinds_and_release_partial_results() {
    let setup = "class BuiltinKeyObject {} function keyProducerGuard($key) { \
        try { $a = array_fill_keys(['valid', $key], 7); echo 'f|'; } catch(TypeError $e) { echo 'F|'; unset($e); } \
        try { $b = array_combine(['valid', $key], [1, 2]); echo 'c|'; } catch(TypeError $e) { echo 'C|'; unset($e); } \
        try { $c = array_column([['name' => 3]], $key); echo 'l|'; } catch(TypeError $e) { echo 'L|'; unset($e); } \
        unset($a, $b, $c); } \
        function wholeColumnRows() { $rows = [['name' => 'a'], ['name' => 'b']]; \
        $result = array_column($rows, null); unset($rows); echo $result[0]['name'], $result[1]['name']; }";
    for vm in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
        let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default(); values.fixed_profile = Some(profile);
        assert!(context.set_global_scope(&mut scope));
        staged_numeric_execution(setup, false, &mut context, &mut scope, &mut values).unwrap();
        for argument in ["true", "0.5", "null", "[]", "new BuiltinKeyObject()"] {
            staged_numeric_execution(&format!("$originalKey = {argument};"), false,
                &mut context, &mut scope, &mut values).unwrap();
            call("keyProducerGuard($originalKey);", "function keyProducerGuard($key){}", vm,
                &mut context, &mut scope, &mut values);
            staged_numeric_execution("unset($originalKey);", false, &mut context, &mut scope, &mut values).unwrap();
        }
        call("wholeColumnRows();", "function wholeColumnRows(){}", vm, &mut context, &mut scope, &mut values);
        assert_eq!(values.output, "F|C|L|F|C|L|F|C|l|F|C|L|F|C|L|ab", "vm={vm}");
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "vm={vm}: {:?}", values.values[id]); }
    }
}

/// Reflected builtin invocations and methods validate original keys after named argument planning.
#[test]
fn exact_eval_builtin_key_reflection_walker_vm_types_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/builtin_key_reflection.php")
        .strip_prefix("<?php\n").unwrap()
        .replace("$function = new ReflectionFunction('array_key_exists');",
            "global $builtinReflection; $function = $builtinReflection;");
    for exact in [false, true] {
        for vm in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default(); values.fixed_profile = Some(profile);
            assert!(context.set_global_scope(&mut scope));
            // FakeOps has no native builtin-reflection constructor. Supply its registered
            // identity while retaining the real invoke/invokeArgs dispatch and key guards.
            let reflection = values.new_object("ReflectionFunction").unwrap();
            context.register_eval_reflection_function(values.object_identity(reflection).unwrap(), "array_key_exists");
            scope.set("builtinReflection", reflection, ScopeCellOwnership::Owned);
            staged_numeric_execution(&source, false, &mut context, &mut scope, &mut values).unwrap();
            let mut expected = String::new();
            for (argument, invalid) in [("true", true), ("0.0", true), ("null", true),
                ("0", false), ("'0'", false), ("''", false)] {
                call(&format!("reflectedKeyInputs({argument});"), "function reflectedKeyInputs($key){}", vm,
                    &mut context, &mut scope, &mut values);
                expected.push_str(if exact && invalid { "R|A|C|T|V|" } else { "r|a|c|t|v|" });
                expected.push_str(if exact { "4|p|q|" } else { "3|p|q|" });
            }
            assert_eq!(values.output, expected, "exact={exact} vm={vm}");
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "exact={exact} vm={vm}: {:?}", values.values[id]); }
        }
    }
}
