//! Purpose:
//! Regressions for forwarding static calls and caller-visible EIR effect summaries.
//!
//! Called from:
//! - The AST-to-EIR unit test module through Rust's test harness.
//!
//! Key details:
//! - Static dispatch is checked on every supported target before EIR optimization.
//! - Owner publication stays ordered inside a function without exposing private state.

use crate::ir::{Effects, Function, Immediate, Instruction, Module, Op};

const TARGETS: [&str; 5] = [
    "macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64",
];

/// Lowers one regression fixture for a supported target.
fn lower_for(source: &str, target: &str) -> Module {
    super::lower_source_at_for_target(
        source,
        std::path::Path::new("main.php"),
        std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    )
}

/// Finds one lowered source function or class method.
fn function<'a>(module: &'a Module, name: &str) -> &'a Function {
    module.functions.iter().chain(&module.class_methods)
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing function {name}"))
}

/// Finds a named static or direct call in one source function.
fn call<'a>(module: &'a Module, caller: &str, callee: &str) -> &'a Instruction {
    function(module, caller).instructions.iter().find(|instruction| {
        let Some(Immediate::Data(id)) = instruction.immediate else { return false; };
        let names = match instruction.op {
            Op::Call => &module.data.function_names,
            Op::StaticMethodCall => &module.data.strings,
            _ => return false,
        };
        names.get(id.as_raw() as usize).is_some_and(|name| name == callee)
    }).unwrap_or_else(|| panic!("missing call {caller} -> {callee}"))
}

/// Early-bound self and parent calls inherit pure, output and throwing implementations.
#[test]
fn call_effect_batch_early_static_receivers() {
    let source = r#"<?php
namespace EffectsBatch;
class Base {
    public static function pure(): int { return 1; }
    public static function noisy(): int { echo "base"; return 2; }
    public static function fail(): int { throw new \RuntimeException("stop"); }
}
class Child extends Base {
    public static function relayPure(): int { return self::pure(); }
    public static function relayNoise(): int { return parent::noisy(); }
    public static function relayThrow(): int { return self::fail(); }
}
class Descendant extends Child {
    public static function pure(): int { echo "descendant"; return 3; }
}
echo Child::relayPure(), Child::relayNoise();
"#;
    for target in TARGETS {
        let module = lower_for(source, target);
        assert_eq!(call(&module, "EffectsBatch\\Child::relayPure", "self::pure").effects,
            Effects::PURE, "{target}");
        assert!(call(&module, "EffectsBatch\\Child::relayNoise", "parent::noisy").effects
            .contains(Effects::OUTPUT), "{target}");
        assert!(call(&module, "EffectsBatch\\Child::relayThrow", "self::fail").effects
            .contains(Effects::MAY_THROW), "{target}");
        assert!(call(&module, "main", "EffectsBatch\\Child::relayNoise").effects
            .contains(Effects::OUTPUT), "{target}");
    }
}

/// A static receiver includes inherited implementations and effects from child overrides.
#[test]
fn call_effect_batch_late_static_overrides() {
    let source = r#"<?php
abstract class Base {
    public static function pure(): int { return 1; }
    public static function value(): int { return 2; }
    public static function relay(): int { return static::value(); }
    public static function relayPure(): int { return static::pure(); }
}
class Child extends Base {
    public static function value(): int { echo "child"; return 3; }
}
class Grandchild extends Child {}
class Throwing extends Base {
    public static function value(): int { throw new RuntimeException("stop"); }
}
echo Child::relay();
"#;
    for target in TARGETS {
        let module = lower_for(source, target);
        assert!(call(&module, "Base::relay", "static::value").effects
            .contains(Effects::OUTPUT | Effects::MAY_THROW), "{target}");
        assert_eq!(call(&module, "Base::relayPure", "static::pure").effects,
            Effects::PURE, "{target}");
        assert!(call(&module, "main", "Child::relay").effects
            .contains(Effects::OUTPUT | Effects::MAY_THROW), "{target}");
    }
}

/// Opaque eval prevents late-static refinement while self and parent remain early-bound.
#[test]
fn call_effect_batch_dynamic_class_barrier() {
    let module = super::lower_source(r#"<?php
class Base {
    public static function value(): int { return 1; }
}
class Child extends Base {
    public static function relay(): int { return static::value(); }
    public static function relaySelf(): int { return self::value(); }
    public static function relayParent(): int { return parent::value(); }
}
function install(string $source): void { eval($source); }
echo Child::value();
"#);
    assert_eq!(call(&module, "Child::relay", "static::value").effects,
        Op::StaticMethodCall.default_effects());
    for (caller, callee) in [("Child::relaySelf", "self::value"),
        ("Child::relayParent", "parent::value")]
    {
        assert_eq!(call(&module, caller, callee).effects,
            call(&module, "main", "Child::value").effects);
    }
}

/// Lexical calls to instance methods use the implementation bound to self or parent.
#[test]
fn call_effect_batch_lexical_instance_receivers() {
    let module = super::lower_source(r#"<?php
class Base {
    public function value(): int { echo "base"; return 1; }
}
class Child extends Base {
    public function relay(): int { return parent::value(); }
    public function relaySelf(): int { return self::value(); }
}
"#);
    assert!(call(&module, "Child::relay", "parent::value").effects.contains(Effects::OUTPUT));
    assert!(call(&module, "Child::relaySelf", "self::value").effects.contains(Effects::OUTPUT));
}

/// A proven late-static body still includes warnings from its bool argument coercion.
#[test]
fn call_effect_batch_late_static_argument_warnings() {
    let module = super::lower_source(r#"<?php
class Base {
    public static function consume(bool $value): int { return 1; }
    public static function relay(mixed $value): int { return static::consume($value); }
}
class Child extends Base {}
"#);
    let effects = call(&module, "Base::relay", "static::consume").effects;
    assert!(effects.contains(Effects::MAY_WARN));
    assert!(!effects.contains(Effects::MAY_DEOPT));
}

/// Abstract classes contribute callable static bodies even when concrete children override them.
#[test]
fn call_effect_batch_abstract_static_implementations() {
    let source = r#"<?php
abstract class Root {
    abstract public static function value(): int;
    public static function relay(): int { return static::value(); }
}
abstract class Middle extends Root {
    public static function value(): int { echo "abstract"; return 1; }
}
class Concrete extends Middle {
    public static function value(): int { return 2; }
}
echo Middle::relay();
"#;
    for target in TARGETS {
        let module = lower_for(source, target);
        assert!(call(&module, "Root::relay", "static::value").effects
            .contains(Effects::OUTPUT), "{target}");
        assert!(call(&module, "main", "Middle::relay").effects
            .contains(Effects::OUTPUT), "{target}");
    }
}

/// Publishing call owners does not impersonate PHP global access in caller summaries.
#[test]
fn call_effect_batch_operand_owners_are_private() {
    let module = super::lower_source(r#"<?php
$saved = 0;
function size(string $text): int { return strlen($text); }
function wrap(string $text): int { return size($text); }
function update(string $text): int { global $saved; $saved = size($text); return $saved; }
echo wrap("abc"), update("def");
"#);
    let wrapper = function(&module, "wrap");
    for op in [Op::PushCallOperandOwner, Op::PopCallOperandOwner] {
        let instructions = wrapper.instructions.iter().filter(|instruction| instruction.op == op)
            .collect::<Vec<_>>();
        assert!(!instructions.is_empty(), "missing {}", op.name());
        assert!(instructions.iter().all(|instruction| instruction.effects == op.default_effects()));
    }
    assert!(!call(&module, "main", "wrap").effects
        .intersects(Effects::READS_GLOBAL | Effects::WRITES_GLOBAL));
    assert!(call(&module, "main", "update").effects.contains(Effects::WRITES_GLOBAL));
}

/// Implicit receivers do not consume the first user argument's bool-coercion effects.
#[test]
fn call_effect_batch_review_lexical_instance_coercions() {
    let source = r#"<?php
class Base {
    public function consume(bool $value): int { return $value ? 1 : 0; }
}
class Child extends Base {
    public function relayParent(mixed $value): int { return parent::consume($value); }
    public function relaySelf(mixed $value): int { return self::consume($value); }
}
"#;
    for target in TARGETS {
        let module = lower_for(source, target);
        for (caller, callee) in [("Child::relayParent", "parent::consume"),
            ("Child::relaySelf", "self::consume")]
        {
            assert!(call(&module, caller, callee).effects.contains(Effects::MAY_WARN),
                "{target}: {caller}");
        }
    }
}

/// Null-only parameters preserve throwing guards for early and late static receivers.
#[test]
fn call_effect_batch_review_null_parameter_guards() {
    let source = r#"<?php
class Base {
    public static function accept(null $value): int { return 1; }
}
class Child extends Base {
    public static function relaySelf(mixed $value): int { return self::accept($value); }
    public static function relayParent(mixed $value): int { return parent::accept($value); }
    public static function relayLate(mixed $value): int { return static::accept($value); }
}
"#;
    for target in TARGETS {
        let module = lower_for(source, target);
        for (caller, callee) in [("Child::relaySelf", "self::accept"),
            ("Child::relayParent", "parent::accept"), ("Child::relayLate", "static::accept")]
        {
            assert!(call(&module, caller, callee).effects.contains(Effects::MAY_THROW),
                "{target}: {caller}");
        }
    }
}

/// A callable abstract receiver retains failure effects beside pure concrete overrides.
#[test]
fn call_effect_batch_review_abstract_static_failure() {
    let source = r#"<?php
abstract class Root {
    abstract public static function value(): int;
    public static function relay(): int { return static::value(); }
}
class Concrete extends Root {
    public static function value(): int { return 1; }
}
echo Concrete::value();
Root::relay();
"#;
    for target in TARGETS {
        let module = lower_for(source, target);
        assert!(call(&module, "Root::relay", "static::value").effects
            .contains(Effects::READS_HEAP | Effects::MAY_FATAL), "{target}");
        assert!(call(&module, "main", "Root::relay").effects
            .contains(Effects::MAY_FATAL), "{target}");
    }
}
