//! Purpose:
//! Verifies declared local initializers guard original scalar kinds before storage conversion.
//!
//! Called from:
//! - Selected-profile frontend, all-target and native library regressions.
//!
//! Key details:
//! - LFC declarations constrain values; ordinary untyped numeric writes keep their own kinds.

use crate::{source::SourceMode, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
function incomingLocal(mixed $value): mixed { return $value; }
function showLocal(mixed $value): void { echo is_int($value) ? 'I' : 'F', $value, '|'; }
function pureLocal(mixed $value): string {
    try { float $local = $value; return 'X'; } catch (TypeError $error) { return 'P'; }
}
function untypedLocal(bool $pick): mixed {
    $value = 2.5; $value = 9007199254740993;
    if ($pick) { $value = 2.5; }
    return $value;
}
float $floating = incomingLocal(4.25); echo $floating, '|';
int $integer = incomingLocal(9007199254740993); echo $integer, '|';
string $text = incomingLocal('owned-' . $argc); echo substr($text, 0, 6), '|';
bool $flag = incomingLocal(false); echo $flag ? 'X' : 'b';
?int $nullable = incomingLocal(null); echo $nullable === null ? 'n' : 'X';
int|false|null $choice = incomingLocal(false); echo $choice === false ? 'f' : 'X';
try { float $floating = incomingLocal(1); echo 'X'; } catch (TypeError $error) { echo 'F'; }
try { int $badInteger = incomingLocal(2.0); echo 'X'; } catch (TypeError $error) { echo 'I'; }
try { string $badText = incomingLocal(2); echo 'X'; } catch (TypeError $error) { echo 'S'; }
try { bool $badFlag = incomingLocal(1); echo 'X'; } catch (TypeError $error) { echo 'B'; }
echo ':', $floating, '|';
showLocal(untypedLocal($argc > 0)); showLocal(untypedLocal($argc < 0)); echo pureLocal(1);
"#;

/// Proven invalid initializers fail before weak declared-type compatibility accepts them.
#[test]
fn strict_typed_initializers_frontend_known_kinds_and_explicit_values() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for source in ["<?php float $value = 1;", "<?php int $value = true;",
        "<?php string $value = 1;", "<?php bool $value = 1;"] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Lfc).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .err().unwrap_or_else(|| panic!("expected strict initializer failure: {source}"));
        assert!(error.message.contains("strict-coercions"), "{source}: {error:?}");
    }
    let source = "<?php float $value = (float) 1; int $integer = 2; bool $flag = false; string $text = 'value';";
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Lfc).unwrap();
    crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap();
}

/// Dynamic declarations materialize only an accepted scalar on every supported ABI.
#[test]
fn strict_typed_initializers_original_guards_emit_on_all_targets() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(SOURCE).unwrap(), SourceMode::Lfc).unwrap();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        assert!(module.functions.iter().any(|f| f.instructions.iter().any(|i| i.op == crate::ir::Op::ThrowTypeError)), "{name}");
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Failed declarations preserve previous values while untyped numeric branches preserve exact original kinds.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_typed_initializers_native_values_failures_untyped_flow_and_owners() {
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(SOURCE).unwrap(), SourceMode::Lfc).unwrap();
    let target = Target::detect_host();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    for (declaration, expected) in [("float $x = incomingLocal(4.25); echo $x;", "4.25"),
        ("bool $x = incomingLocal(false); echo $x;", ""),
        ("incomingLocal(null);", ""),
        ("?int $x = incomingLocal(null);", ""),
        ("?int $x = incomingLocal(null); echo $x === null;", "1"),
        ("?int $x = incomingLocal(9007199254740993); echo $x;", "9007199254740993"),
        ("int|false|null $x = incomingLocal(false); echo $x === false;", "1")] {
        let source = format!("<?php function incomingLocal(mixed $value): mixed {{ return $value; }} {declaration}");
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&source).unwrap(), SourceMode::Lfc).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success() && stderr.contains("HEAP DEBUG: leak summary: clean"),
            "{declaration}: {stderr}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{declaration}");
    }
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
        assert_eq!(stdout, "4.25|9007199254740993|owned-|bnfFISB:4.25|F2.5|I9007199254740993|P", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
