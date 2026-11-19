//! Purpose:
//! Verifies public initialized-read admission across supported compiler targets.
//!
//! Called from:
//! - Focused public semantic-profile library tests.
//!
//! Key details:
//! - Uses the public checker instead of staged semantic helpers.

use crate::{codegen_support::platform::Target, types::CheckOptions};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Every public target accepts Initialization independently of unfinished guarantees.
#[test]
fn initialized_public_checker_emits_on_every_target() {
    let source = "<?php function initializedPublic(bool $assign):void { if($assign){$value=null;} echo $value??'missing'; try{echo gettype($value);}catch(Error $error){echo'I';} } initializedPublic(false);";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::check_with_target_and_options(&program, target,
            CheckOptions { semantic_profile: profile, ..Default::default() },
            &crate::generics::GenericContext::default()).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        assert_eq!(module.semantic_profile, profile);
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Recursion is pair-specific and an escaping getter leaves the next read usable.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_recursive_getters_native_values_and_owners() {
    let source = include_str!("../../../tests/fixtures/strict/initialized_recursive_getters.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, crate::source::SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stderr}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "E:1|ready:2|right:2|text:2.5|other:1:1|value|", "{stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}

/// Guard metadata cannot erase source requirements, operand shapes, data identity or effects.
#[test]
fn initialized_getter_metadata_validation() {
    let source = include_str!("../../../tests/fixtures/strict/initialized_recursive_getters.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    let original = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, false, crate::source::SourceMode::Php);
    for case in 0..5 {
        let mut module = original.clone();
        let inst = module.functions.iter_mut().flat_map(|function| function.instructions.iter_mut())
            .find(|inst| matches!(inst.immediate, Some(crate::ir::Immediate::InitializedGetter { .. }))).unwrap();
        match case {
            0 => inst.operands[1] = inst.operands[0],
            1 => if let Some(crate::ir::Immediate::InitializedGetter { property, .. }) = &mut inst.immediate {
                *property = crate::ir::DataId::from_raw(u32::MAX);
            },
            2 => inst.effects.remove(crate::ir::Effects::WRITES_GLOBAL),
            3 => inst.op = crate::ir::Op::Call,
            _ => module.semantic_profile = SemanticProfile::PERMISSIVE,
        }
        assert!(crate::ir::validate_module(&module).is_err(), "case={case}");
    }
}

/// Every target emits the shared guard ABI with a distinct getter identity namespace.
#[test]
fn initialized_getter_runtime_guards_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/initialized_recursive_getters.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = super::semantic_sources::lower_capturing_arguments(source, target, profile, false, crate::source::SourceMode::Php);
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let runtime = crate::codegen_support::generate_runtime_with_semantics(8_388_608,
            target, module.required_runtime_features, false, false, profile);
        assert!(runtime.contains("__rt_magic_get_guard_push:"), "{name}");
        assert!(runtime.contains(if name == "linux-x86_64" { "or rdi, 1" } else { "orr x0, x0, #1" }), "{name}");
        assert!(runtime.contains("__rt_magic_set_guard_push:"), "{name}");
    }
}

/// Copying a concrete local's provisional Mixed view retires its temporary box in both modes.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_conditional_copy_read_owners() {
    for value in ["'ready'", "str_repeat('r', 5)", "7"] {
        let source = format!("<?php function copyInitializedView(bool $assign):void {{if($assign){{$source={value};}}try{{$copy=$source;echo $copy;}}catch(Error $error){{echo 'I';}}}}copyInitializedView(true);copyInitializedView(false);");
        for strict in [false, true] {
            for optimize in [false, true] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, strict);
                let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(), profile, optimize, crate::source::SourceMode::Php);
                let output = super::native_module::run(&module);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "value={value}, strict={strict}, opt={optimize}: {stderr}");
                let expected = match (value, strict) {
                    ("7", false) => "77", ("7", true) => "7I",
                    ("'ready'", false) => "ready", ("'ready'", true) => "readyI",
                    (_, false) => "rrrrr", (_, true) => "rrrrrI",
                };
                assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{stderr}");
                assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "value={value}, strict={strict}, opt={optimize}: {stderr}");
            }
        }
    }
}
