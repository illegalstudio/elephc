//! Purpose:
//! Checks context-free null contracts without guessing generic or lexical class names.
//!
//! Called from:
//! - Staged Nullability declaration, template and variadic regressions.
//!
//! Key details:
//! - Original defaults are checked before binding and source folding.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Known null defaults and original call arguments require an explicitly open contract.
#[test]
fn nullable_declarations_preserve_original_default_and_call_contracts() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for source in ["<?php function f(int $value=null){}",
        "<?php function f(array $value=null){}", "<?php class C{public string $value=null;}",
        "<?php class C{public static int $value=null;}",
        "<?php class C{function f(callable $value=null){}}",
        "<?php interface I{function f(string $value=null);}",
        "<?php trait T{function f(iterable $value=null){}}",
        "<?php enum E{case A;function f(int $value=null){}}",
        "<?php $f=function(int $value=null){};", "<?php $f=fn(int $value=null)=>$value;",
        "<?php const MISSING=null;function f(string $value=MISSING){}",
        "<?php class C{public const string MISSING=null;}",
        "<?php function f(string $value){} if(false){f(null);}",
        "<?php function f(string $value){} f(value:null);",
        "<?php function f(string $value){} f(...[null]);",
        "<?php class C{function __construct(string $value){}} function f(C $value=new C(null)){}"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::semantics::validate_source(&program, profile).unwrap_err();
        assert!(error.message.contains("strict-nullability"), "{source}: {error:?}");
        crate::semantics::validate_source(&program, SemanticProfile::PERMISSIVE).unwrap();
    }
    for source in ["<?php function f(?int $value=null){}", "<?php function f(mixed $value=null){}",
        "<?php function f(float $value=1){}", "<?php function f(string $value){} f(7);",
        "<?php function f(int $value=new self){}"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::semantics::validate_source(&program, profile).unwrap();
    }
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(
        "<?php function trustedDefault(int $value=null){}").unwrap(), SourceMode::Internal).unwrap();
    crate::semantics::validate_source(&program, profile).unwrap();
}

/// A template name is not a nominal non-null proof before its arguments are instantiated.
#[test]
fn nullable_declarations_defer_unresolved_template_contracts() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for source in ["<?php function template<T>():mixed{T $value=null;return $value;}",
        "<?php function template<T>(T $value=null):mixed{return $value;}",
        "<?php class Template<T>{public T $value=null;}"] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Lfc).unwrap();
        crate::semantics::validate_source(&program, profile).unwrap();
    }
}

/// Non-scalar variadic nulls fail while valid and explicitly nullable elements keep their kinds.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn nullable_non_scalar_variadics_preserve_contracts_and_owners() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_variadic_contracts.php");
    for coercions in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true)
            .with(StrictFeature::Coercions, coercions);
        for optimize in [false, true] {
            let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "coercions={coercions} optimize={optimize}: {stdout}; {stderr}");
            assert_eq!(stdout, "AIC|1:1:1:NULL:NULL", "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}

/// Context-free element metadata remains separate from mixed transport on every target.
#[test]
fn nullable_non_scalar_variadics_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_variadic_contracts.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Nominal parameter adaptation happens after later effects and keeps its class and relative contracts.
#[test]
fn nullable_object_arguments_and_nominal_variadics_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_object_arguments.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Public opaque fixtures remain valid eval grammar before the native bridge runs them.
#[test]
fn nullable_public_eval_fragments_preserve_parser_contracts() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    let mut combined = String::new();
    for (name, source) in [
        ("receivers", include_str!("../../../tests/fixtures/strict/nullable_receivers.php")),
        ("builtins", include_str!("../../../tests/fixtures/strict/nullable_builtins.php")),
        ("aliases", include_str!("../../../tests/fixtures/strict/nullable_eval_aliases.php")),
        ("descriptors", include_str!("../../../tests/fixtures/strict/nullable_descriptors.php")),
        ("variadics", include_str!("../../../tests/fixtures/strict/nullable_variadic_contracts.php")),
        ("objects", include_str!("../../../tests/fixtures/strict/nullable_object_arguments.php")),
    ] {
        let fragment = source.strip_prefix("<?php").unwrap().replace("declare(strict_types=0);", "");
        let parsed = elephc_magician::parser::parse_fragment_with_semantics(fragment.as_bytes(), profile);
        assert!(parsed.is_ok(), "{name}: {:?}", parsed.err());
        combined.push_str(&fragment); combined.push(';');
        let parsed = elephc_magician::parser::parse_fragment_with_semantics(combined.as_bytes(), profile);
        assert!(parsed.is_ok(), "combined through {name}: {:?}", parsed.err());
    }
}

/// Generator entry checks are eager, null-only and compatible with string formatting policy.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn nullable_nominal_variadic_generators_validate_before_suspension() {
    let source = "<?php class NominalGeneratorValue{} \
        function nominalGenerator(NominalGeneratorValue ...$values):iterable{echo 'body';yield 1;} \
        function nominalGeneratorDispatch(callable $callback,array $values):mixed{return $callback(...$values);} \
        try{nominalGeneratorDispatch(nominalGenerator(...),[null]);}catch(TypeError $error){echo 'N';} \
        $valid=nominalGeneratorDispatch(nominalGenerator(...),[new NominalGeneratorValue()]);echo 'V';";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true)
        .with(StrictFeature::Strings, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stdout}; {stderr}");
        assert_eq!(stdout, "NV", "{stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}
