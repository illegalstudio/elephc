//! Purpose:
//! End-to-end regressions for purity and may-throw v2 effect refinement.
//!
//! Called from:
//! - `cargo test --test codegen_tests_core optimizer::effects_v2`.
//!
//! Key details:
//! - Fixtures keep runtime-selected instance dispatch and catchable typed-property errors
//!   observable through the complete optimized PHP-to-native pipeline.

use super::*;

/// Verifies DCE preserves an unused virtual call when a reachable override emits output.
#[test]
fn virtual_override_effects_survive_dead_code_elimination() {
    let out = compile_and_run(
        r#"<?php
class EffectBase {
    public function touch(): void {}
    public function run(): void { $this->touch(); }
}
final class EffectChild extends EffectBase {
    public function touch(): void { echo "child"; }
}
$receiver = $argc > 0 ? new EffectChild() : new EffectBase();
$receiver->run();
"#,
    );

    assert_eq!(out, "child");
}

/// Verifies a typed-property initialization error keeps its matching catch reachable.
#[test]
fn typed_property_read_keeps_catch_reachable() {
    let out = compile_and_run(
        r#"<?php
final class EffectBox {
    public int $value;
    public function read(): int { return $this->value; }
}
try {
    (new EffectBox())->read();
} catch (Error $error) {
    echo "caught";
}
"#,
    );

    assert_eq!(out, "caught");
}

/// Verifies an unused property read still invokes its observable PHP 8.4 get hook.
#[test]
fn property_get_hook_effect_survives_dead_code_elimination() {
    let out = compile_and_run(
        r#"<?php
final class EffectHookBox {
    public int $computed {
        get {
            echo "hook";
            return 1;
        }
    }
}
(new EffectHookBox())->computed;
"#,
    );

    assert_eq!(out, "hook");
}
