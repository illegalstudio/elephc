//! Purpose:
//! Verifies source declaration defaults preserve original scalar kinds.
//!
//! Called from:
//! - Selected-profile frontend, all-target and native default regressions.
//!
//! Key details:
//! - Declaration policy applies even when the declaration is unused or called indirectly.

use crate::{source::SourceMode, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
const DEFAULT_FLOAT = 4.25;
function defaultValue(float $value = DEFAULT_FLOAT): float { return $value; }
function nullableDefault(?int $value = null): ?int { return $value; }
function unboundDefault(int $value = new self): int { return $value; }
class DefaultHolder {
    const VALUE = 6.25;
    public float $value = self::VALUE;
    public static float $shared = 8.25;
    public function instance(float $value = 2.5): float { return $value; }
    public static function stat(float $value = 3.5): float { return $value; }
}
echo defaultValue(), '|';
$call = defaultValue(...); echo $call(), '|';
$closure = function(float $value = 1.5): float { return $value; }; echo $closure(), '|';
$arrow = fn(float $value = 7.5): float => $value; echo $arrow(), '|';
$holder = new DefaultHolder(); echo $holder->value, '|', DefaultHolder::$shared, '|';
echo $holder->instance(), '|', DefaultHolder::stat(), '|';
echo nullableDefault() === null ? 'N' : 'X';
echo unboundDefault(5), '|';
try { unboundDefault(); } catch (Error $error) { echo 'E'; }
"#;

/// Unused scalar defaults are diagnosed independently of schema-pass and caller policies.
#[test]
fn strict_defaults_reject_original_kinds_across_declaration_surfaces() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for source in ["<?php function f(float $x = 1) {}",
        "<?php declare(strict_types=0); function f(float $x = 1) {}",
        "<?php class C { public float $x = 1; }",
        "<?php class C { public static float $x = 1; }",
        "<?php class C { function f(float $x = 1) {} }",
        "<?php class C { function __construct(public float $x = 1) {} }",
        "<?php interface I { public function f(float $x = 1); }",
        "<?php trait T { public function f(float $x = 1) {} }",
        "<?php enum E { case A; public function f(float $x = 1) {} }",
        "<?php $f = function(float $x = 1) {};",
        "<?php $f = fn(float $x = 1) => $x;",
        "<?php const DEFAULT_INT = 1; function f(float $x = DEFAULT_INT) {}",
        "<?php class C { const VALUE = 1; public float $x = self::VALUE; }",
        "<?php class C { public const float VALUE = 1; }",
        "<?php class C { function __construct(float $v) {} } function f(C $v = new C(1)) {}",
        "<?php class C { function __construct(float $v) {} } class D extends C {} function f(D $v = new D(v: 1)) {}",
        "<?php trait T { function __construct(float $v) {} } class C { use T; } function f(C $v = new C(...[1])) {}"] {
        for mode in [SourceMode::Php, SourceMode::Lfc] {
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
            let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
                .err().unwrap_or_else(|| panic!("expected strict default failure: {source}"));
            assert!(error.message.contains("strict-coercions"), "{source}: {error:?}");
        }
        for (mode, selected) in [(SourceMode::Php, SemanticProfile::PERMISSIVE), (SourceMode::Internal, profile)] {
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
            crate::types::checker::check_staged_semantics(&program, Target::detect_host(), selected).unwrap();
        }
    }
    for source in ["<?php function make(int $value = new self): int { return $value; }",
        "<?php trait T { const VALUE = 4.25; public function f(float $x = self::VALUE) {} }",
        "<?php trait T { public string $name = parent::class; } class C { use T; }"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap();
    }
    let mut mixed = crate::parser::parse_with_mode(&crate::lexer::tokenize("<?php function trustedDefault(float $x = 1) {}").unwrap(), SourceMode::Internal).unwrap();
    mixed.extend(crate::parser::parse(&crate::lexer::tokenize("<?php function userDefault(float $x = 2.5) {}").unwrap()).unwrap());
    crate::types::checker::check_staged_semantics(&mixed, Target::detect_host(), profile).unwrap();
}

/// Valid original defaults materialize through every supported target ABI.
#[test]
fn strict_defaults_preserve_exact_declarations_on_all_targets() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Default values keep their actual numeric payloads across direct and descriptor invocation.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_defaults_native_payloads_callables_properties_and_owners() {
    let target = Target::detect_host();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
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
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {stdout} {stderr}");
        assert_eq!(stdout, "4.25|4.25|1.5|7.5|6.25|8.25|2.5|3.5|N5|E", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
