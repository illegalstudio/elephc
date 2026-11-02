//! Purpose:
//! Exercises forwarding static-call effects through native codegen and EIR optimization.
//!
//! Called from:
//! - The codegen integration test suite through Rust's test harness.
//!
//! Key details:
//! - Discarded call results must preserve output, exceptions and genuine global mutation.
//! - The same fixture runs with EIR optimization enabled and disabled.

use super::*;

/// Static dispatch keeps each override's observable effects in both optimizer modes.
#[test]
fn test_call_effect_batch_static_dispatch_preserves_observable_effects() {
    let source = r#"<?php
class EffectBase {
    public static function value(): int { echo "base;"; return 1; }
    public static function relay(): int { return static::value(); }
    public static function early(): int { return self::value(); }
}
class EffectChild extends EffectBase {
    public static function value(): int { echo "child;"; return 2; }
    public static function fromParent(): int { return parent::early(); }
}
class EffectThrowing extends EffectBase {
    public static function value(): int { throw new RuntimeException("stop"); }
}
abstract class EffectAbstract {
    public static function value(): int { echo "abstract;"; return 1; }
    public static function relay(): int { return static::value(); }
}
class EffectConcrete extends EffectAbstract {
    public static function value(): int { return 2; }
}
$saved = 0;
function effectSize(string $text): int { return strlen($text); }
function effectUpdate(string $text): int {
    global $saved;
    $saved = effectSize($text);
    return $saved;
}
EffectAbstract::relay();
for ($i = 0; $i < $argc + 1; $i++) {
    EffectChild::relay();
    EffectChild::early();
    EffectChild::fromParent();
}
try { EffectThrowing::relay(); } catch (RuntimeException $e) { echo "caught;"; }
effectUpdate("abcd");
echo $saved;
"#;
    let expected = "abstract;child;base;base;child;base;base;caught;4";
    assert_eq!(compile_and_run(source), expected);
    assert_eq!(without_ir_opt(|| compile_and_run(source)), expected);
}
