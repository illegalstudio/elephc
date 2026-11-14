//! Purpose:
//! Verifies nonliteral declaration values bind under producer policy before user call boundaries.
//!
//! Called from:
//! - Staged all-target and native declaration producer regressions.
//!
//! Key details:
//! - Raw defaults remain available for reflection while aliases and unions retain bound value kinds.

use crate::{source::SourceMode, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

const TRUSTED: &str = r#"<?php
const PRODUCER_BASE = 1; const PRODUCER_FLAG = true;
function aliasProducer(float $value = PRODUCER_BASE): float { return $value; }
function expressionProducer(float $value = PRODUCER_BASE + 1): float { return $value; }
function unionProducer(float|null $value = PRODUCER_FLAG ? PRODUCER_BASE : null): mixed { return $value; }
function nullProducer(float|null $value = null): mixed { return $value; }
function nominalProducer(float|stdClass $value = PRODUCER_BASE): mixed { return $value; }
function objectProducer(float|stdClass $value = new stdClass()): mixed { return $value; }
function arithmeticProducer(float $value = '1' + 1): float { return $value; }
function identityProducer($value = '1' + 1): mixed { return $value; }
function powerProducer($value = 2 ** -1): mixed { return $value; }
function overflowProducer(float $value = PHP_INT_MAX + 1): float { return $value; }
function lazyProducer(float|null $value = PRODUCER_FLAG ? PRODUCER_BASE : new self): mixed { return $value; }
trait ProducerTrait { public function imported(float $value = PRODUCER_BASE + 4): float { return $value; } }
$producerClosure = function(float $value = PRODUCER_BASE + 2): float { return $value; };
$producerArrow = fn(float $value = PRODUCER_BASE + 3): float => $value;
$producerLazyClosure = function(float|null $value = PRODUCER_FLAG ? PRODUCER_BASE : new self): mixed { return $value; };
class ProducerConstants { public const float VALUE = PRODUCER_BASE + 1; public const float ALIAS = self::VALUE + 1; }
"#;
const USER: &str = r#"<?php
class ProducerConsumer { use ProducerTrait; }
function producerUnknown(mixed $value): mixed { return $value; }
function producerArguments(bool $present): array { return $present ? [1] : []; }
echo aliasProducer(), '|', expressionProducer(), '|';
echo arithmeticProducer(), '|', is_int(identityProducer()) ? 'I' : 'X';
echo is_float(powerProducer()) ? 'P' : 'X', is_float(overflowProducer()) && overflowProducer() > 0.0 ? 'B' : 'X';
$lazy = lazyProducer(...); echo is_float($lazy()) ? 'L' : 'X', is_float($producerLazyClosure()) ? 'K|' : 'X|';
echo is_float(unionProducer()) ? 'U' : 'X', nullProducer() === null ? 'N' : 'X';
echo is_float(nominalProducer()) ? 'O|' : 'X|';
echo objectProducer() instanceof stdClass ? 'A|' : 'X|';
$call = expressionProducer(...); echo $call(), '|', $producerClosure(), '|', $producerArrow(), '|';
$host = new ProducerConsumer(); echo $host->imported(), '|';
$reflection = new ReflectionFunction('expressionProducer');
echo is_float($reflection->getParameters()[0]->getDefaultValue()) ? 'R|' : 'X|';
echo expressionProducer(...producerArguments($argc < 0)), '|';
echo is_float(ProducerConstants::VALUE) ? 'C' : 'X', ProducerConstants::ALIAS, '|';
try { expressionProducer(producerUnknown(1)); } catch (TypeError $error) { echo 'D'; }
try { $call(producerUnknown(1)); } catch (TypeError $error) { echo 'C'; }
try { expressionProducer(...producerArguments($argc > 0)); } catch (TypeError $error) { echo 'S'; }
"#;

/// Retains separate physical modes for trusted producers and their user consumers.
fn program() -> crate::parser::ast::Program {
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(TRUSTED).unwrap(), SourceMode::Internal).unwrap();
    program.extend(crate::parser::parse(&crate::lexer::tokenize(USER).unwrap()).unwrap());
    program
}

/// Combines independent numeric restrictions while trusted producers retain their own PHP rules.
fn profile() -> SemanticProfile {
    SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)
        .with(StrictFeature::Arithmetic, true).with(StrictFeature::Overflow, true)
}

/// Omitted aliases preserve their raw reflection identity and source origin in signature metadata.
#[test]
fn strict_default_producers_preserve_source_defaults_and_exact_user_arguments() {
    let program = program();
    let profile = profile();
    let checked = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap();
    let default = checked.functions["aliasProducer"].defaults[0].as_ref().unwrap();
    assert!(matches!(default.kind, crate::parser::ast::ExprKind::ConstRef(_)));
    assert_eq!(default.source_profile.mode, SourceMode::Internal);
    assert_eq!(default.producer_binding, None);
    let prepared = checked.functions["aliasProducer"].default_value(0).unwrap();
    assert_eq!(prepared.producer_binding, Some(crate::parser::ast::ProducerBinding::IntegerToFloat));
    assert!(!default.same_value_producer(&prepared));
    let rebound = prepared.with_kind(crate::parser::ast::ExprKind::IntLiteral(1));
    assert_eq!(crate::types::checker::infer_expr_type_syntactic(&rebound), crate::types::PhpType::Float);
    let folded = crate::optimize::fold_constant_expression(rebound);
    assert!(matches!(folded.kind, crate::parser::ast::ExprKind::FloatLiteral(value) if value == 1.0));
    let mut invalid = program.clone();
    invalid.extend(crate::parser::parse(&crate::lexer::tokenize("<?php expressionProducer(1);").unwrap()).unwrap());
    assert!(crate::types::checker::check_staged_semantics(&invalid, Target::detect_host(), profile)
        .err().unwrap().message.contains("strict-coercions"));
}

/// Producer binding lowers through EIR for every supported target.
#[test]
fn strict_default_producers_emit_nonliteral_values_on_all_targets() {
    let profile = profile();
    let program = program();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Aliases, expression defaults, descriptor calls and reflection preserve payloads and ownership.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_default_producers_native_nonliteral_kinds_and_owners() {
    let program = program();
    let target = Target::detect_host();
    let profile = profile();
    for optimize in [false, true] {
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
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
        assert_eq!(stdout, "1|2|2|IPBLK|UNO|A|2|3|4|5|R|2|C3|DCS", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
