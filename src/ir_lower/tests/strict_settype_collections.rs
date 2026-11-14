//! Purpose:
//! Verifies array/object settype targets and their generated helper dependencies.
//!
//! Called from:
//! - Focused staged EIR and native strict conversion regressions.
//!
//! Key details:
//! - No explicit object-cast syntax keeps the implicit dependency under test.

use crate::codegen_support::platform::Target;
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
$cast = settype(...); mixed $value = null; $alias =& $value;
$cast($value, 'array'); echo count($alias), '|';
$value = 'item'; $cast($value, 'ARRAY'); echo $alias[0], '|';
$copy = $value; $value[0] = 'changed'; echo $copy[0], ':', $alias[0], '|';
$value = ['name' => 'kept', 2 => 'number']; $cast($value, 'object');
echo $alias->name, ':', $alias->{'2'}, '|';
$same = $value; $name = 'settype'; $name($value, 'object'); echo $same === $alias ? 'I|' : 'X|';
$name($value, 'array'); echo $alias['name'], ':', $alias[2], '|';
$value = null; $target = $argc > 0 ? 'object' : 'OBJECT'; $cast($value, $target);
echo is_object($alias) ? 'N|' : 'X|';
$value = 42; $cast($value, 'object'); echo $alias->scalar, '|';
$value = ['a' => 'kept']; call_user_func('settype', $value, 'object'); echo is_array($alias) ? 'V' : 'X';
"#;

/// Prepares only the implicit object-cast dependency through its production injector.
fn prepared() -> (crate::parser::ast::Program, crate::optimize::reachability::PreludeInventory) {
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let program = crate::object_cast_prelude::inject_if_used(program, &mut inventory).unwrap();
    (crate::name_resolver::resolve(program).unwrap(), inventory)
}

/// Every target retains and emits helpers required solely by settype's conversion graph.
#[test]
fn strict_settype_collections_all_target_dependencies() {
    let (program, inventory) = prepared();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
        let mut checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let program = crate::optimize::reachability::prune_unreachable_declarations(program.clone(), &mut checked,
            crate::optimize::reachability::PruneOptions { inventory: &inventory,
                forced_groups: &Default::default(), exported_functions: &Default::default(), eval_forced: false });
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        assert!(module.functions.iter().any(|function| function.name == crate::object_cast_prelude::DYNAMIC_CAST_HELPER), "{name}");
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Both numeric profiles and EIR optimizer settings preserve object identity, COW and owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_settype_collections_native_values_and_owners() {
    let (program, _) = prepared(); let target = Target::detect_host();
    for casts in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, casts);
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        for optimize in [false, true] {
            let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            if optimize { crate::ir_passes::optimize_module(&mut module); }
            crate::ir::validate_module(&module).unwrap();
            let output = super::native_module::run(&module);
            let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "casts={casts} opt={optimize}: {stdout} {stderr}");
            assert_eq!(stdout, "0|item|item:changed|kept:number|I|kept:number|N|42|V", "casts={casts} opt={optimize}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}
