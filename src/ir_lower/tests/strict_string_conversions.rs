//! Purpose:
//! Checks explicit string conversion parity across syntax, builtins and callable graphs.
//!
//! Called from:
//! - Focused selected-profile EIR and host-native conversion regressions.
//!
//! Key details:
//! - User hooks and temporary strings remain observable through both optimizer layers.

use crate::codegen_support::platform::Target;
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
mixed $effect = 0;
class ExplicitStringText {
    public function __toString(): string { global $effect; $effect += 1; return 'text'; }
}
$convert = strval(...); $name = $argc > 0 ? 'strval' : 'STRVAL';
echo strval(new ExplicitStringText()), '|';
echo $convert(new ExplicitStringText()), '|';
echo $name(new ExplicitStringText()), '|';
echo call_user_func('strval', new ExplicitStringText()), '|';
echo call_user_func_array('strval', [new ExplicitStringText()]), '|';
$mapped = array_map('strval', [new ExplicitStringText(), 42]); echo $mapped[0], ':', $mapped[1], '|';
strval(new ExplicitStringText()); echo $effect, '|';
echo $convert(str_repeat('x', 3)), ':', $convert('kept'), ':', $convert(null), '|';
class ExplicitStringThrow { public function __toString(): string { throw new ValueError('stop'); } }
try { $convert(new ExplicitStringThrow()); echo 'X'; } catch (ValueError $error) { echo 'V'; }
"#;

/// Mirrors the implicit dependency preparation needed by runtime-selected builtin catalogs.
fn prepared() -> crate::parser::ast::Program { prepared_source(SOURCE) }

/// Prepares one independent source-owner regression using the ordinary injector.
fn prepared_source(source: &str) -> crate::parser::ast::Program {
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let program = crate::object_cast_prelude::inject_if_used(program, &mut inventory).unwrap();
    crate::name_resolver::resolve(program).unwrap()
}

/// Prepared string builtin graphs remain target-neutral under the checked conversion profile.
#[test]
fn strict_string_conversions_callable_graphs_all_targets() {
    let program = prepared();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Every invocation retains callback effects, results and owners with both AST/EIR optimizer modes.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_string_conversions_native_callbacks_and_owners() {
    let program = prepared(); let target = Target::detect_host();
    for casts in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, casts);
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        for optimize in [false, true] {
            let _policy = crate::optimize::scoped_semantic_profile(profile);
            let mut program = program.clone();
            if optimize {
                program = crate::optimize::fold_constants_for_target(program, target);
                program = crate::optimize::propagate_constants(program, checked.mixed_storage_local_names(), checked.buffer_read_sites.clone());
                program = crate::optimize::prune_constant_control_flow(program, checked.local_binding_decision_spans());
                program = crate::optimize::normalize_control_flow(program, checked.local_binding_decision_spans());
                program = crate::optimize::eliminate_dead_code(program, checked.local_binding_decision_spans());
            }
            let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            if optimize { crate::ir_passes::optimize_module(&mut module); }
            crate::ir::validate_module(&module).unwrap();
            let output = super::native_module::run(&module);
            let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "casts={casts} opt={optimize}: {stdout} {stderr}");
            assert_eq!(stdout, "text|text|text|text|text|text:42|7|xxx:kept:|V", "casts={casts} opt={optimize}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}

/// Fresh source owners remain balanced for each independently selected callable entry path.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_string_conversions_fresh_source_owner_paths() {
    let target = Target::detect_host();
    for call in ["echo strval(new OwnerText());", "$cast = strval(...); echo $cast(new OwnerText());",
        "$name = $argc > 0 ? 'strval' : 'STRVAL'; echo $name(new OwnerText());",
        "echo call_user_func('strval', new OwnerText());",
        "echo call_user_func_array('strval', [new OwnerText()]);",
        "$items = array_map('strval', [new OwnerText(), 42]); echo $items[0];",
        "strval(new OwnerText());", "$cast = strval(...); echo $cast(str_repeat('x', 3));"] {
        let source = format!("<?php class OwnerText {{ public function __toString(): string {{ return 'text'; }} }} {call}");
        let program = prepared_source(&source);
        let checked = crate::types::checker::check_staged_semantics(&program, target,
            SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{call}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{call}: {stderr}");
    }
}

/// Covers reviewed boolean object projections using real native declarations on one target.
fn boolean_object_target(name: &str) {
    let (program, _) = super::strict_object_casts::prepare("<?php function checkedBoolean(SimpleXMLElement $value): bool { return boolval($value); } function erasedBoolean(mixed $value): bool { return boolval($value); } function booleanCallback(): callable { return boolval(...); }");
    let target = Target::parse(name).unwrap();
    let checked = crate::types::checker::check_staged_semantics(&program, target,
        SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
    let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
    crate::ir::validate_module(&module).unwrap();
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

/// Keeps real-native declaration checks inside each target's focused CI timeout.
macro_rules! boolean_target_test {
    ($name:ident, $target:literal) => {
        /// Emits native and erased boolean conversion graphs on one first-class target.
        #[test]
        fn $name() { boolean_object_target($target); }
    };
}
boolean_target_test!(strict_string_conversions_boolean_macos, "macos-aarch64");
boolean_target_test!(strict_string_conversions_boolean_ios, "ios-arm64");
boolean_target_test!(strict_string_conversions_boolean_ios_sim, "ios-sim-arm64");
boolean_target_test!(strict_string_conversions_boolean_linux_arm64, "linux-aarch64");
boolean_target_test!(strict_string_conversions_boolean_linux_x86_64, "linux-x86_64");
