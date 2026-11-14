//! Purpose:
//! Verifies property defaults bind under their physical producer policy before user construction.
//!
//! Called from:
//! - Staged property metadata, all-target and native ownership regressions.
//!
//! Key details:
//! - Declared slots bind values, while inferred slots preserve their original runtime kind.

use crate::{source::SourceMode, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

const TRUSTED: &str = r#"<?php
const PROPERTY_BASE = 1; const PROPERTY_CHOOSE = true;
trait PropertyProducerTrait { public float $fromTrait = PROPERTY_BASE + 1; }
class PropertyProducer {
    use PropertyProducerTrait;
    public float $scalar = PROPERTY_BASE;
    public float|null $nullable = PROPERTY_CHOOSE ? PROPERTY_BASE : null;
    public float|stdClass $nominal = PROPERTY_BASE;
    public float $missing;
    public static float $shared = PROPERTY_BASE + 2;
    public static float|null $sharedUnion = PROPERTY_CHOOSE ? PROPERTY_BASE : null;
    public $untyped = '1' + 1;
    public $truthy = true + 1;
    public $nothing = null + 1;
    public $selected = PROPERTY_CHOOSE ? 2 ** -1 : 0;
}
"#;
const USER: &str = r#"<?php
class PropertyConsumer extends PropertyProducer { public float $scalar = 9.5; }
$value = new PropertyProducer(); $child = new PropertyConsumer();
echo is_float($value->scalar) ? 'S' : 'X', $value->scalar, '|';
echo is_float($value->nullable) ? 'N' : 'X', is_float($value->nominal) ? 'O' : 'X';
echo $value->fromTrait, '|', $child->scalar, '|', $child->fromTrait, '|';
echo is_int($value->untyped) ? 'I' : 'X', is_int($value->truthy) ? 'B' : 'X', is_int($value->nothing) ? 'Z' : 'X';
echo is_float($value->selected) ? 'P|' : 'X|';
echo is_float(PropertyProducer::$shared) ? 'T' : 'X', PropertyProducer::$shared;
echo is_float(PropertyProducer::$sharedUnion) ? 'U|' : 'X|';
$reflection = new ReflectionClass('PropertyProducer'); $defaults = $reflection->getDefaultProperties();
echo is_float($defaults['scalar']) ? 'R' : 'X', is_float($defaults['nullable']) ? 'N' : 'X';
echo is_int($defaults['untyped']) ? 'I' : 'X';
echo is_int($defaults['truthy']) ? 'B' : 'X', is_int($defaults['nothing']) ? 'Z' : 'X';
echo is_float($defaults['selected']) ? 'P' : 'X';
"#;

/// Preserves the trusted declaration's source profile through user inheritance and construction.
fn program() -> crate::parser::ast::Program {
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(TRUSTED).unwrap(), SourceMode::Internal).unwrap();
    program.extend(crate::parser::parse(&crate::lexer::tokenize(USER).unwrap()).unwrap());
    program
}

/// Selects numeric guarantees independently of property initialization or inferred local contracts.
fn profile() -> SemanticProfile {
    SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true).with(StrictFeature::Arithmetic, true)
}

/// Schema copies prepare existing defaults while the source and absent typed defaults stay intact.
#[test]
fn strict_property_producers_bind_declared_metadata_without_retyping_inferred_slots() {
    let program = program();
    let checked = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile()).unwrap();
    let info = &checked.classes["PropertyProducer"];
    for (index, (name, _)) in info.properties.iter().enumerate() {
        if name == "missing" { assert!(info.defaults[index].is_none()); continue; }
        let value = info.defaults[index].as_ref().unwrap();
        let expected = if matches!(name.as_str(), "untyped" | "truthy" | "nothing" | "selected") { crate::parser::ast::ProducerBinding::Identity }
            else { crate::parser::ast::ProducerBinding::IntegerToFloat };
        assert_eq!(value.producer_binding, Some(expected), "{name}");
        assert_eq!(value.source_profile.mode, SourceMode::Internal, "{name}");
    }
    let crate::parser::ast::StmtKind::ClassDecl { properties, .. } = &program[3].kind else { panic!("class producer"); };
    assert!(properties.iter().filter_map(|property| property.default.as_ref()).all(|value| value.producer_binding.is_none()));
}

/// Declared property producer binding uses the same EIR path on all five targets.
#[test]
fn strict_property_producers_emit_inherited_trait_and_static_defaults_on_all_targets() {
    let program = program();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile()).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Construction, inheritance, static initialization and reflection retain payloads and clean heaps.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_property_producers_native_initialization_reflection_and_owners() {
    let program = program(); let target = Target::detect_host();
    for selected in [SemanticProfile::PERMISSIVE, profile()] {
        let checked = crate::types::checker::check_staged_semantics(&program, target, selected).unwrap();
        for optimize in [false, true] {
            let _policy = crate::optimize::scoped_semantic_profile(selected);
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
            assert!(output.status.success(), "profile={selected:?} opt={optimize}: {stdout} {stderr}");
            assert_eq!(stdout, "S1|NO2|9.5|2|IBZP|T3U|RNIBZP", "profile={selected:?} opt={optimize}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "profile={selected:?} opt={optimize}: {stderr}");
        }
    }
}
