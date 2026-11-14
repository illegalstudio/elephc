//! Purpose:
//! Verifies exact scalar alternatives survive unions containing nominal object contracts.
//!
//! Called from:
//! - Selected-profile declaration, direct-call and descriptor integration regressions.
//!
//! Key details:
//! - Nominal membership stays explicit instead of disabling noncoercive scalar checks.
//! - Failure paths preserve previous local values and retire fresh rejected objects.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = r#"<?php
interface UnionMarker {}
class UnionMember implements UnionMarker {}
class UnionChild extends UnionMember {}
class UnionOther {}
function unionOriginal(mixed $value): mixed { return $value; }
function unionAccept(UnionMember|float $value): string { return is_float($value) ? 'F' : 'O'; }
function unionInterface(UnionMarker|int $value): string { return is_int($value) ? 'I' : 'M'; }
function unionReturn(mixed $value): UnionMember|float { return $value; }
function unionObjectReturn(mixed $value): UnionMember { return $value; }
function unionDescriptor(callable $callback, mixed $value): mixed { return $callback($value); }
function unionStore(mixed $value): void {
    UnionMember|float $local = 2.5;
    try { $local = $value; } catch (TypeError $error) { echo 'S'; }
    echo is_float($local) ? 'F' : 'O';
}
function unionObjectStore(mixed $value): void {
    UnionMember $local = new UnionChild();
    try { UnionMember $checked = $value; $local = $checked; } catch (TypeError $error) { echo 'U'; }
    echo $local instanceof UnionMember ? 'O' : 'X';
}
echo unionAccept(unionOriginal(new UnionChild())), unionAccept(unionOriginal(2.5));
echo unionInterface(unionOriginal(new UnionChild())), unionInterface(unionOriginal(9007199254740993)), '|';
try { unionAccept(unionOriginal(1)); } catch (TypeError $error) { echo 'D'; }
$callback = unionAccept(...);
echo unionDescriptor($callback, unionOriginal(new UnionChild()));
try { unionDescriptor($callback, unionOriginal(1)); } catch (TypeError $error) { echo 'C'; }
try { unionDescriptor($callback, unionOriginal(new UnionOther())); } catch (TypeError $error) { echo 'N'; }
echo is_float(unionReturn(unionOriginal(2.5))) ? 'R' : 'X';
try { unionReturn(unionOriginal(1)); } catch (TypeError $error) { echo 'T'; }
echo unionObjectReturn(unionOriginal(new UnionChild())) instanceof UnionMember ? 'O' : 'X';
try { unionObjectReturn(unionOriginal(new UnionOther())); } catch (TypeError $error) { echo 'E'; }
unionStore(unionOriginal(1));
unionObjectStore(unionOriginal(new UnionChild()));
unionObjectStore(unionOriginal(new UnionOther()));
"#;

/// Nominal alternatives cannot hide a proven scalar conversion at any declared boundary.
#[test]
fn strict_nominal_unions_reject_known_scalar_conversions() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for source in [
        "<?php class C {} function f(C|float $v): void {} f(1);",
        "<?php class C {} function f(): C|float { return 1; } f();",
        "<?php class C {} C|float $local = 1;",
        "<?php class C {} C|float $local = 1.5; $local = 1;",
        "<?php class C {} function f(C|float $v = 1): void {}",
        "<?php class C {} class D { public C|float $value = 1; }",
    ] {
        for mode in [SourceMode::Php, SourceMode::Lfc] {
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
            let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
                .err().unwrap_or_else(|| panic!("expected original scalar kind rejection: {source}"));
            assert!(error.message.contains("strict-coercions"), "{source}: {error:?}");
        }
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        if let Err(error) = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), SemanticProfile::PERMISSIVE) {
            assert!(!error.message.contains("strict-coercions"), "permissive: {source}: {error:?}");
        }
        let internal = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Internal).unwrap();
        if let Err(error) = crate::types::checker::check_staged_semantics(&internal, Target::detect_host(), profile) {
            assert!(!error.message.contains("strict-coercions"), "Internal: {source}: {error:?}");
        }
    }
}

/// Class and interface membership guards emit together with exact scalar predicates on all targets.
#[test]
fn strict_nominal_unions_emit_class_and_scalar_guards_on_all_targets() {
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

/// Direct and dynamic boundaries preserve class identity, exact floats and local failure atomicity.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_nominal_unions_native_boundaries_and_owned_failures() {
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
        assert_eq!(stdout, "OFMI|DOCNRTOESFOUO", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
