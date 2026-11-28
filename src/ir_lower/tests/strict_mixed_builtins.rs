//! Purpose:
//! Verifies logical builtin argument domains and numeric result alternatives under Mixed.
//!
//! Called from:
//! - Focused staged semantic-profile checker, emitter and native regressions.
//!
//! Key details:
//! - Boxed aggregate storage is independent from the finite numeric source result.
//! - Overloaded string joins require the selected argument roles to be proven.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Unknown and partially supported join alternatives require narrowing at direct and alias calls.
#[test]
fn strict_mixed_join_roles_require_supported_alternatives() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    for source in [
        "<?php function f(mixed $value):string{return implode($value);}",
        "<?php function f(mixed $value):string{return implode(',', $value);}",
        "<?php function f(array $items,mixed $glue):string{return join($glue,$items);}",
        "<?php function f(array|string $value):string{return implode($value);}",
        "<?php function f(array|false $value):string{return join(',',$value);}",
        "<?php function f(mixed $value):string{$join=implode(...);return $join($value);}",
        "<?php function f(mixed $value):string{return \\ImPlOdE(',', $value);}",
        "<?php function f(array $arguments):string{return implode(...$arguments);}",
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .err().unwrap_or_else(|| panic!("expected a strict diagnostic: {source}"));
        assert!(error.message.contains("strict-mixed"), "{source}: {error:?}");
    }
}

/// Existing array guards and weak scalar separator alternatives retain their independent policies.
#[test]
fn strict_mixed_join_roles_preserve_guards_and_weak_separators() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    for source in [
        "<?php function f(mixed $value):string{if(is_array($value)){return implode($value);}return '';}",
        "<?php function f(array $items,int|string $glue):string{return join($glue,$items);}",
        "<?php $join=implode(...);echo $join(['a','b']);",
        "<?php class Joinable{public function __toString():string{return ',';}}function f(array $items,Joinable|string $glue):string{return \\JoIn($glue,$items);}",
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Direct and callable aggregates keep closed numeric alternatives on every supported emitter.
#[test]
fn strict_mixed_numeric_aggregate_results_keep_numeric_proofs() {
    let source = "<?php $sum=array_sum(...);$product=array_product(...);echo array_sum([2,3])+array_product([2,3]),':',$sum([1,2])+1,':',$product([2,3])+1;function resourceProof(mixed $value):int{if(is_resource($value)){return get_resource_id($value);}return 0;}echo '|',resourceProof(STDOUT)===get_resource_id(STDOUT)?'R':'bad',':',resourceProof(1);";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
    #[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{errors}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "11:4:7|R:0", "{errors}");
        assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
    }
}

/// Boxed resource and formatting containers need proof while guards preserve valid callers.
#[test]
fn strict_mixed_generic_builtin_inputs_require_domains() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    for source in [
        "<?php function f(mixed $value):int{return get_resource_id($value);}",
        "<?php function f(mixed $value):string{return get_resource_type($value);}",
        "<?php function f(mixed $values):string{return vsprintf('%s',$values);}",
        "<?php function f(mixed $values):int{return vprintf('%s',$values);}",
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .err().unwrap_or_else(|| panic!("expected a strict diagnostic: {source}"));
        assert!(error.message.contains("strict-mixed"), "{source}: {error:?}");
    }
    for source in [
        "<?php function f(mixed $value):int{if(is_resource($value)){return get_resource_id($value);}return 0;}",
        "<?php function f(mixed $values):string{if(is_array($values)){return vsprintf('%s',$values);}return '';}",
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Unselected guarantees keep the legacy callable fallback when direct builtin validation is narrower.
#[test]
fn strict_mixed_callable_fallback_preserves_unselected_profiles() {
    let source = "<?php function f(mixed $value):mixed{$reverse=array_reverse(...);return $reverse($value);}";
    for profile in [SemanticProfile::PERMISSIVE,
        SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}
