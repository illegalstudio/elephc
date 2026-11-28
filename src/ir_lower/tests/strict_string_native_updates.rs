//! Purpose:
//! Verifies public native property string guards on every supported compiler target.
//!
//! Called from:
//! - Focused staged strict-string lowering and emission tests.
//!
//! Key details:
//! - Read-only native operands precede both kind guards and writable selection.
//! - Trusted implementation code and permissive user code keep their existing updates.

use crate::{ir::{Immediate, Op}, codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Checks native and ordinary branches without linking target-specific XML archives.
fn verify_native_string_updates(name: &str, mode: SourceMode) {
    let source = "<?php function nativeStringNamed(mixed $view, mixed $tail): mixed { return $view->child .= $tail; } \
        function nativeStringDynamic(mixed $view, mixed $name, mixed $tail): mixed { return $view->$name .= $tail; } \
        $xml = new SimpleXMLElement('<root/>');";
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    program = crate::core_prelude::inject_if_used(program, false, false, &mut inventory);
    program = crate::xml_prelude::inject_if_used(program, false, &mut inventory);
    program = crate::dom_html_prelude::inject_if_used(program, false, &mut inventory);
    program = crate::object_cast_prelude::inject_if_used(program, &mut inventory).unwrap();
    program = crate::func_args::desugar(program).unwrap();
    program = crate::name_resolver::resolve(program).unwrap();
    let target = Target::parse(name).unwrap();
    let exports = ["nativestringnamed", "nativestringdynamic"].into_iter().map(str::to_string).collect();
    let empty = std::collections::HashSet::new();
    for selected in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, selected);
        let mut checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let reachable = crate::optimize::reachability::prune_unreachable_declarations(program.clone(), &mut checked,
            crate::optimize::reachability::PruneOptions { inventory: &inventory,
                forced_groups: &empty, exported_functions: &exports, eval_forced: false });
        assert!(checked.classes["SimpleXMLElement"].methods.contains_key("__elephc_property_update_read"),
            "{name}: the shared native contract must retain its read-only update adapter");
        let module = crate::ir_lower::lower_program(&reachable, &checked, target, false).unwrap();
        for function in ["nativeStringNamed", "nativeStringDynamic"] {
            let function = module.functions.iter().find(|candidate| candidate.name == function).unwrap();
            let method = |expected: &str| function.instructions.iter().position(|inst|
                inst.op == Op::MethodCall && matches!(inst.immediate, Some(Immediate::Data(id))
                    if module.data.strings[id.as_raw() as usize] == expected));
            let read = method("__elephc_property_update_read");
            assert_eq!(read.is_some(), selected && mode == SourceMode::Php,
                "{name}: {} {mode:?} strict={selected}", function.name);
            if let Some(read) = read {
                let guard = function.instructions.iter().enumerate().skip(read + 1)
                    .find(|(_, inst)| inst.op == Op::ThrowTypeError).unwrap().0;
                assert!(guard < method("__elephc_property_update").unwrap(),
                    "{name}: {} guards before native mutation", function.name);
            }
        }
        for optimized in [false, true] {
            let mut module = module.clone();
            if optimized { crate::ir_passes::optimize_module(&mut module); }
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Exercises read-only native string update guards for user code.
fn verify_user_string_updates(target: &str) {
    verify_native_string_updates(target, SourceMode::Php);
}

/// Exercises PHP-compatible native updates in compiler-owned code.
fn verify_internal_string_updates(target: &str) {
    verify_native_string_updates(target, SourceMode::Internal);
}

supported_target_cases!(strict_string_native_updates_eir_php, verify_user_string_updates);
supported_target_cases!(strict_string_native_updates_eir_internal, verify_internal_string_updates);
