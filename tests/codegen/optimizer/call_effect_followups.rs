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

/// Discarding lexical instance results cannot remove NaN-to-bool warning handlers.
#[test]
fn test_call_effect_batch_review_lexical_instance_warnings() {
    let source = r#"<?php
set_error_handler(function(int $level, string $message): bool { echo "warn;"; return true; });
class WarningBase {
    public function consume(bool $value): int { return $value ? 1 : 0; }
}
class WarningChild extends WarningBase {
    public function relay(mixed $value): void { parent::consume($value); self::consume($value); }
}
(new WarningChild())->relay(NAN);
echo "done";
"#;
    assert_eq!(compile_and_run(source), "warn;warn;done");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "warn;warn;done");
}

/// Discarded calls retain null guards after an independent throw keeps the AST catch live.
#[test]
fn test_call_effect_batch_review_null_parameter_guards() {
    let source = r#"<?php
class NullBase {
    public static function accept(null $value): int { return 1; }
}
class NullChild extends NullBase {
    public static function relaySelf(mixed $value, bool $force): void {
        if ($force) { throw new TypeError("keep catch"); }
        self::accept($value);
    }
    public static function relayParent(mixed $value, bool $force): void {
        if ($force) { throw new TypeError("keep catch"); }
        parent::accept($value);
    }
    public static function relayLate(mixed $value, bool $force): void {
        if ($force) { throw new TypeError("keep catch"); }
        static::accept($value);
    }
}
try { NullChild::relaySelf($argc, $argc === 999); } catch (TypeError $e) { echo "self;"; }
try { NullChild::relayParent($argc, $argc === 999); } catch (TypeError $e) { echo "parent;"; }
try { NullChild::relayLate($argc, $argc === 999); } catch (TypeError $e) { echo "late;"; }
echo "done";
"#;
    let expected = "self;parent;late;done";
    assert_eq!(compile_and_run(source), expected);
    assert_eq!(without_ir_opt(|| compile_and_run(source)), expected);
}

/// A private late-static throw keeps stores whose only remaining reader is its catch.
#[test]
fn test_call_effect_batch_review_private_static_handler() {
    let source = r#"<?php
class PrivateBase {
    private static function value(): int { throw new Exception("stop"); }
    public static function relay(): int {
        $seen = 1;
        try { return static::value(); } catch (Exception $e) { return $seen; }
    }
}
class PrivateChild extends PrivateBase {
    public static function other(): int { return 2; }
}
echo PrivateChild::other();
echo PrivateBase::relay();
"#;
    assert_eq!(compile_and_run(source), "21");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "21");
}

/// Abstract literal guards preserve catch-only readers before the invalid dispatch.
#[test]
fn test_call_effect_batch_review_abstract_parameter_guards() {
    let template = r#"<?php
abstract class GuardRoot {
    abstract public static function accept(LITERAL $value): int;
    public static function relay(mixed $value, bool $force): int {
        $seen = 0;
        try {
            if ($force) { throw new Error("keep catch"); }
            $seen = 1;
            return static::accept($value);
        } catch (Error $e) { return $seen; }
    }
}
class GuardConcrete extends GuardRoot {
    public static function accept(mixed $value): int { return 0; }
    public static function keep(): int { return 1; }
}
echo GuardConcrete::keep();
echo GuardRoot::relay($argc, $argc === 999);
"#;
    for literal in ["false", "null"] {
        let source = template.replace("LITERAL", literal);
        assert_eq!(compile_and_run(&source), "11", "{literal}");
        assert_eq!(without_ir_opt(|| compile_and_run(&source)), "11", "{literal}");
    }
}

/// Runtime-defined overrides preserve catch readers and output in both optimizer modes.
#[test]
fn test_call_effect_batch_eval_dispatch_bundle() {
    let cases = [
        BundleCase {
            name: "eval_override_catch_reader",
            source: r#"<?php
class CallEffectEvalCatchBase {
    public static function value(): int { return 0; }
    public static function relay(bool $force): int {
        $seen = 0;
        try {
            if ($force) { throw new Exception("keep catch"); }
            $seen = 1;
            return static::value();
        } catch (Exception $e) { return $seen; }
    }
}
$evalCatchCode = $argc > 0
    ? 'class CallEffectEvalCatchChild extends CallEffectEvalCatchBase { public static function value(): int { throw new Exception("stop"); } }'
    : '';
eval($evalCatchCode);
echo CallEffectEvalCatchChild::relay($argc === 999);
"#,
            expected: "1",
        },
        BundleCase {
            name: "eval_override_output",
            source: r#"<?php
class CallEffectEvalOutputBase {
    public static function value(): int { return 0; }
    public static function relay(): int { return static::value(); }
}
$evalOutputCode = $argc > 0
    ? 'class CallEffectEvalOutputChild extends CallEffectEvalOutputBase { public static function value(): int { echo "output;"; return 7; } }'
    : '';
eval($evalOutputCode);
CallEffectEvalOutputChild::relay();
echo "done";
"#,
            expected: "output;done",
        },
    ];
    assert_bundle(&cases);
    without_ir_opt(|| assert_bundle(&cases));
}
