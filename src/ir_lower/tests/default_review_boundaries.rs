//! Purpose:
//! Verifies native default transport and resolved static arrays on all supported targets.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Each target is independently scheduled to avoid one multi-target timeout envelope.
//! - Failed default registrations must not silently publish an incomplete catalog.

use super::*;

/// Emits lazy spreads, static selectors and prefix catches using each target's shared ABI.
fn default_review_boundaries_on_target(target: &str) {
    let source = r#"<?php
class Value { public function __construct() { throw new Exception('prefix'); } }
class Defaults {
    const ENABLED = true;
    public static array $items = [self::ENABLED ? 'ok' : parent::A];
    public static function value(array $items = [...[parent::A]]): int { return count($items); }
    public function prefix(array $items = [new Value(), parent::A]): void {}
}
class FailedDefaults {
    const ENABLED = false;
    public static array $items = [self::ENABLED ? 'ok' : parent::A];
}
echo json_encode(Defaults::$items);
try { echo json_encode(FailedDefaults::$items); } catch (Error $error) { echo 'class-default'; }
try { Defaults::value(); } catch (Error $error) { echo 'error'; }
try { (new Defaults())->prefix(); } catch (Exception $error) { echo 'prefix'; }
$source = $argv[1];
eval($source);
"#;
    let module = lower_source_at_for_target(source, Path::new("main.php"), Path::new("."),
        Target::parse(target).unwrap());
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    assert!(assembly.contains("__elephc_eval_register_native_static_method_param_default_expression"), "{target}");
    assert!(assembly.contains("eval_native_default_registered"), "{target}: registration result guard");
    assert!(assembly.contains("__elephc_default_array_spread"), "{target}: private spread encoding");
    assert!(assembly.contains("__elephc_eval_register_native_class_default_error"), "{target}: class initialization metadata");
}

/// macOS ARM64 uses the same lazy default transport contract.
#[test]
fn default_review_boundaries_macos() { default_review_boundaries_on_target("macos-aarch64"); }

/// iOS device ARM64 emits guarded default registration.
#[test]
fn default_review_boundaries_ios() { default_review_boundaries_on_target("ios-arm64"); }

/// iOS Simulator ARM64 emits guarded default registration.
#[test]
fn default_review_boundaries_ios_sim() { default_review_boundaries_on_target("ios-sim-arm64"); }

/// Linux ARM64 emits the full lazy default transport contract.
#[test]
fn default_review_boundaries_linux_arm() { default_review_boundaries_on_target("linux-aarch64"); }

/// Linux x86_64 emits the same transport and static initializer semantics.
#[test]
fn default_review_boundaries_linux_x86() { default_review_boundaries_on_target("linux-x86_64"); }
