//! Purpose:
//! Verifies trusted declaration defaults bind before user invocation kind guards.
//!
//! Called from:
//! - Selected-profile call-planner, all-target and native default regressions.
//!
//! Key details:
//! - Stored source defaults remain original; materialized omitted values use their declared PHP kind.

use crate::{source::SourceMode, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

const INTERNAL: &str = r#"<?php
function internalDefault(float $value = 1): float { return $value; }
function internalNamed(float $value = 1, string $label = 'n'): string { return $label . ':' . $value; }
class InternalDefaultHost {
    public function instance(float $value = 1): float { return $value; }
    public static function stat(float $value = 1): float { return $value; }
}
$internalClosure = function(float $value = 1): float { return $value; };
$internalArrow = fn(float $value = 1): float => $value;
"#;
const USER: &str = r#"<?php
function suppliedDefault(mixed $value): mixed { return $value; }
function defaultArguments(bool $present): array { return $present ? [1] : []; }
echo internalDefault(), '|';
$call = internalDefault(...); echo $call(), '|';
$host = new InternalDefaultHost(); echo $host->instance(), '|', InternalDefaultHost::stat(), '|';
echo $internalClosure(), '|', $internalArrow(), '|';
echo internalNamed(label: 'named'), '|';
$reflection = new ReflectionFunction('internalDefault');
echo is_float($reflection->getParameters()[0]->getDefaultValue()) ? 'R|' : 'X|';
echo internalDefault(...defaultArguments($argc < 0)), '|';
try { internalDefault(suppliedDefault(1)); } catch (TypeError $error) { echo 'D'; }
try { $call(suppliedDefault(1)); } catch (TypeError $error) { echo 'C'; }
try { internalDefault(...defaultArguments($argc > 0)); } catch (TypeError $error) { echo 'S'; }
"#;

/// Keeps the declaration and the calling source in their own physical policies.
fn program() -> crate::parser::ast::Program {
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(INTERNAL).unwrap(), SourceMode::Internal).unwrap();
    program.extend(crate::parser::parse(&crate::lexer::tokenize(USER).unwrap()).unwrap());
    program
}

/// Omission binds a copy while explicit equal literals and stored source defaults retain original kinds.
#[test]
fn strict_trusted_defaults_planner_preserves_source_and_explicit_argument_kinds() {
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(INTERNAL).unwrap(), SourceMode::Internal).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let checked = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap();
    let sig = &checked.functions["internalNamed"];
    assert!(matches!(sig.defaults[0].as_ref().unwrap().kind, crate::parser::ast::ExprKind::IntLiteral(1)));
    let named = crate::parser::ast::Expr::new(crate::parser::ast::ExprKind::NamedArg {
        name: "label".into(),
        value: Box::new(crate::parser::ast::Expr::new(
            crate::parser::ast::ExprKind::StringLiteral("named".into()), crate::span::Span::dummy(),
        )),
    }, crate::span::Span::dummy());
    let plan = crate::types::call_args::plan_call_args(sig, &[named], crate::span::Span::dummy(), false, false).unwrap();
    assert!(matches!(plan.regular_args.first(), Some(crate::types::call_args::PlannedRegularArg::Default(value))
        if matches!(value.kind, crate::parser::ast::ExprKind::FloatLiteral(value) if value == 1.0)));
    let explicit = [crate::parser::ast::Expr::int_lit(1)];
    let plan = crate::types::call_args::plan_call_args(sig, &explicit, crate::span::Span::dummy(), false, false).unwrap();
    assert!(matches!(plan.normalized_args()[0].kind, crate::parser::ast::ExprKind::IntLiteral(1)));
}

/// Every supported emitter receives the producer's bound default value.
#[test]
fn strict_trusted_defaults_emit_declared_omitted_values_on_all_targets() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let program = program();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Defaults are accepted across callables while explicit direct and spread int values still fail.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_trusted_defaults_native_callables_named_spreads_and_owners() {
    let target = Target::detect_host();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let program = program();
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
        assert_eq!(stdout, "1|1|1|1|1|1|named:1|R|1|DCS", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
