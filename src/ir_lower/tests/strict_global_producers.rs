//! Purpose:
//! Verifies global constant reads retain physical producer semantics through aliases and metadata.
//!
//! Called from:
//! - Staged global-constant emitter and native ownership regressions.
//!
//! Key details:
//! - User readers observe prepared PHP values rather than re-evaluating trusted syntax as user code.

use crate::{source::SourceMode, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

const TRUSTED: &str = r#"<?php
const GLOBAL_PRODUCER_ARITHMETIC = '1' + 1;
const GLOBAL_PRODUCER_POWER = 2 ** -1;
const GLOBAL_PRODUCER_OVERFLOW = PHP_INT_MAX + 1;
const GLOBAL_PRODUCER_ALIAS = GLOBAL_PRODUCER_ARITHMETIC;
const GLOBAL_PRODUCER_RESOURCE = STDIN;
define('GLOBAL_PRODUCER_DEFINE', '2' + 1);
"#;
const USER: &str = r#"<?php
const GLOBAL_USER_ALIAS = GLOBAL_PRODUCER_ALIAS;
function globalProducerRead(): mixed { return GLOBAL_PRODUCER_ARITHMETIC; }
echo is_int(GLOBAL_PRODUCER_ARITHMETIC) ? 'I' : 'X', GLOBAL_PRODUCER_ARITHMETIC, '|';
echo is_float(GLOBAL_PRODUCER_POWER) ? 'P' : 'X', GLOBAL_PRODUCER_POWER, '|';
echo is_float(GLOBAL_PRODUCER_OVERFLOW) ? 'O|' : 'X|';
echo is_int(GLOBAL_PRODUCER_ALIAS) ? 'A' : 'X', is_int(GLOBAL_USER_ALIAS) ? 'U' : 'X';
echo is_int(GLOBAL_PRODUCER_DEFINE) ? 'D' : 'X', GLOBAL_PRODUCER_DEFINE, '|';
echo is_int(globalProducerRead()) ? 'F' : 'X';
echo is_float(abs(GLOBAL_PRODUCER_OVERFLOW)) ? 'R' : 'X';
$absolute = abs(...); echo is_float($absolute(GLOBAL_PRODUCER_OVERFLOW)) ? 'Q|' : 'X|';
echo 'C', constant('GLOBAL_PRODUCER_ARITHMETIC'), '|';
$constants = get_defined_constants(); echo is_float($constants['GLOBAL_PRODUCER_OVERFLOW']) ? 'G' : 'X';
echo is_resource($constants['GLOBAL_PRODUCER_RESOURCE']) ? 'S' : 'X';
echo is_resource(constant('GLOBAL_PRODUCER_RESOURCE')) ? 'T' : 'X', is_resource(GLOBAL_PRODUCER_RESOURCE) ? 'U' : 'X';
"#;

/// Retains producer and consumer modes independently of the selected compilation guarantees.
fn program() -> crate::parser::ast::Program {
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(TRUSTED).unwrap(), SourceMode::Internal).unwrap();
    program.extend(crate::parser::parse(&crate::lexer::tokenize(USER).unwrap()).unwrap());
    program
}

/// Selects all currently integrated numeric groundwork without admitting unfinished CLI features.
fn profile() -> SemanticProfile {
    SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)
        .with(StrictFeature::Arithmetic, true).with(StrictFeature::Overflow, true)
}

/// Prescan retains physical producers for builtin, trusted, user alias and static define entries.
#[test]
fn strict_global_producers_prescan_keeps_profiles_and_original_source() {
    let program = program();
    let constants = crate::source::with_parse_mode(crate::source::SourceProfile {
        mode: SourceMode::Lfc, strict_types: true,
    }, || crate::codegen_support::collect_constants(&program, Target::detect_host()));
    for name in ["GLOBAL_PRODUCER_ARITHMETIC", "GLOBAL_PRODUCER_ALIAS", "GLOBAL_PRODUCER_DEFINE", "PHP_INT_MAX"] {
        let value = &constants[name].0;
        assert_eq!(value.source_profile.mode, SourceMode::Internal, "{name}");
        assert_eq!(value.producer_binding, Some(crate::parser::ast::ProducerBinding::Identity), "{name}");
    }
    assert_eq!(constants["GLOBAL_USER_ALIAS"].0.source_profile.mode, SourceMode::Php);
    let crate::parser::ast::StmtKind::ConstDecl { value, .. } = &program[0].kind else { panic!("source constant"); };
    assert_eq!(value.producer_binding, None);
    let user = crate::parser::parse(&crate::lexer::tokenize(TRUSTED).unwrap()).unwrap();
    assert!(crate::types::checker::check_staged_semantics(&user, Target::detect_host(), profile())
        .err().unwrap().message.contains("strict-arithmetic"));
}

/// Global producer materialization uses the same EIR semantics on all supported targets.
#[test]
fn strict_global_producers_emit_alias_and_define_values_on_all_targets() {
    let program = program();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile()).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Constants, aliases and static define reads preserve original kinds and retire temporary owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_global_producers_native_const_alias_and_define_kinds_and_owners() {
    let program = program(); let target = Target::detect_host();
    let selected = profile();
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
        assert!(output.status.success(), "opt={optimize}: {stdout} {stderr}");
        assert_eq!(stdout, "I2|P0.5|O|AUD3|FRQ|C2|GSTU", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
