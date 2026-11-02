//! Purpose:
//! Regresses destruction of displaced selected-enum arguments during parameter binding.
//!
//! Called from:
//! - The codegen types integration suite.
//!
//! Key details:
//! - A converted temporary must be destroyed before entering the selected method.
//! - Caller-owned values survive, and throwing destructors prevent method entry.

use crate::support::*;

/// Shares payload-compatible declarations for static and runtime-unpack call paths.
const DECLARATIONS: &str = r#"<?php
class LifetimeCastSource {
    public function __construct(public bool $throwing = false) {}
    public function __toString(): string { echo 'S'; return str_repeat('x', 3); }
    public function __destruct() {
        echo 'D'; if ($this->throwing) { throw new Exception('destructor'); }
    }
}
interface LifetimeSelected {
    public function r(string|bool &$value): string;
    public function v(string|bool $value): string;
    public function t(string|bool ...$tail): string;
}
enum ALifetimeTight implements LifetimeSelected {
    case A;
    public function r(string|bool &$value): string { echo 'M'; return (string)$value; }
    public function v(string|bool $value): string { echo 'V'; return (string)$value; }
    public function t(string|bool ...$tail): string { echo 'T'; return 'ok'; }
}
enum ZLifetimeLoose implements LifetimeSelected {
    case A;
    public function r(mixed &$value): string { echo 'm'; return 'loose'; }
    public function v(mixed $value): string { echo 'v'; return 'loose'; }
    public function t(mixed ...$tail): string { echo 't'; return 'loose'; }
}
function lifetimeSource(bool $throwing = false): mixed { return new LifetimeCastSource($throwing); }
function lifetimeWarm(LifetimeSelected $target, mixed $value): void {
    $target->r($value); $target->v($value); $target->t($value); echo '|';
}
function lifetimePositional(bool $throwing = false): array { return [lifetimeSource($throwing)]; }
function lifetimeNamed(bool $throwing = false): array { return ['value' => lifetimeSource($throwing)]; }
function lifetimePair(): array { return [lifetimeSource(), lifetimeSource()]; }
lifetimeWarm(ZLifetimeLoose::A, 'x');
"#;

/// Static value/reference bindings retire old objects, not the caller's surviving owners.
#[test]
fn test_enum_argument_lifetime_review_static() {
    let out = compile_and_run_with_heap_debug(&format!(r#"{DECLARATIONS}
function lifetimeRef(LifetimeSelected $target): void {{
    $value = lifetimeSource(); $result = $target->r($value);
    echo 'R:', $result, ':', gettype($value), '|';
}}
function lifetimeValue(LifetimeSelected $target): void {{
    $result = $target->v(lifetimeSource()); echo 'R:', $result, '|';
    $result = $target->t(lifetimeSource()); echo 'R:', $result, '|';
    $result = $target->t(extra: lifetimeSource()); echo 'R:', $result, '|';
}}
function lifetimeBorrowed(LifetimeSelected $target): void {{
    $value = lifetimeSource(); $target->v($value); echo 'B:', get_class($value), '|';
}}
lifetimeRef(ALifetimeTight::A); lifetimeValue(ALifetimeTight::A);
lifetimeBorrowed(ALifetimeTight::A);
"#));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "mvt|SDMR:xxx:string|SDVR:xxx|SDTR:ok|SDTR:ok|SVB:LifetimeCastSource|D");
    assert!(out.stderr.lines().next().unwrap().contains("live_blocks=2 live_bytes=80"), "{}", out.stderr);
}

/// Static argument lifetime does not depend on EIR optimization.
#[test]
fn test_enum_argument_lifetime_review_static_no_ir_opt() {
    without_ir_opt(test_enum_argument_lifetime_review_static);
}

/// Dynamic regular and variadic binding release consumed temporary-container payloads.
#[test]
fn test_enum_argument_lifetime_review_unpack() {
    let out = compile_and_run_with_heap_debug(&format!(r#"{DECLARATIONS}
function lifetimeUnpack(LifetimeSelected $target): void {{
    $target->v(...lifetimePositional()); echo 'R|';
    $target->v(...lifetimeNamed()); echo 'R|';
    $target->t(...lifetimePositional()); echo 'R|';
    $target->t(...lifetimeNamed()); echo 'R|';
    $target->t(...lifetimePair()); echo 'R|';
    $held = lifetimePositional(); $target->v(...$held); echo 'B:', get_class($held[0]), '|';
}}
lifetimeUnpack(ALifetimeTight::A);
"#));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "mvt|SDVR|SDVR|SDTR|SDTR|SDSDTR|SVB:LifetimeCastSource|D");
    assert!(out.stderr.lines().next().unwrap().contains("live_blocks=2 live_bytes=80"), "{}", out.stderr);
}

/// Runtime-unpack source retention follows the same contract without EIR optimization.
#[test]
fn test_enum_argument_lifetime_review_unpack_no_ir_opt() {
    without_ir_opt(test_enum_argument_lifetime_review_unpack);
}

/// A conversion-triggered throwing destructor leaves the caller result and never enters a method.
#[test]
fn test_enum_argument_lifetime_review_throwing() {
    let out = compile_and_run_with_heap_debug(&format!(r#"{DECLARATIONS}
function lifetimeThrowRef(LifetimeSelected $target): void {{
    $value = lifetimeSource(true); $result = 'anchor';
    try {{ $result = $target->r($value); }}
    catch (Exception $error) {{ echo 'C:', $error->getMessage(), ':', $result, ':', gettype($value), '|'; }}
}}
function lifetimeThrowValue(LifetimeSelected $target): void {{
    $result = 'anchor';
    try {{ $result = $target->v(lifetimeSource(true)); }}
    catch (Exception $error) {{ echo 'C:', $error->getMessage(), ':', $result, '|'; }}
    try {{ $result = $target->t(lifetimeSource(true)); }}
    catch (Exception $error) {{ echo 'C:', $error->getMessage(), ':', $result, '|'; }}
    try {{ $result = $target->v(...lifetimePositional(true)); }}
    catch (Exception $error) {{ echo 'C:', $error->getMessage(), ':', $result, '|'; }}
    try {{ $result = $target->v(...lifetimeNamed(true)); }}
    catch (Exception $error) {{ echo 'C:', $error->getMessage(), ':', $result, '|'; }}
    try {{ $result = $target->t(...lifetimePositional(true)); }}
    catch (Exception $error) {{ echo 'C:', $error->getMessage(), ':', $result, '|'; }}
    try {{ $result = $target->t(...lifetimeNamed(true)); }}
    catch (Exception $error) {{ echo 'C:', $error->getMessage(), ':', $result, '|'; }}
}}
lifetimeThrowRef(ALifetimeTight::A); lifetimeThrowValue(ALifetimeTight::A);
"#));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("mvt|SDC:destructor:anchor:string|{}", "SDC:destructor:anchor|".repeat(6)));
    assert!(out.stderr.lines().next().unwrap().contains("live_blocks=2 live_bytes=80"), "{}", out.stderr);
}

/// Exceptional snapshot retirement preserves the same unwind guarantees without EIR optimization.
#[test]
fn test_enum_argument_lifetime_review_throwing_no_ir_opt() {
    without_ir_opt(test_enum_argument_lifetime_review_throwing);
}
