//! Purpose:
//! Verifies lexical default binding and deferred trait errors across the supported target matrix.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Declaration-only tests use the full frontend, not executable reachability pruning.
//! - Every target must emit the same catchable error and valid default paths.
//! - Static compound RHS effects precede the deferred error from the element read.

use crate::codegen::platform::Target;
use std::path::Path;

/// Deferred default presence, scope and reflection guards share one target-independent contract.
fn deferred_reflection_defaults_on_target(target_name: &str) {
    let source = r#"<?php
trait Invalid {
    public string $bad = parent::class;
    public function labeled(string $value = parent::class): string { return $value; }
}
class Consumer { use Invalid; public static array $names = [parent::A]; }
class Promoted { public function __construct(public int $value = parent::A) {} }
class Valid { public string $value = 'ok'; public function run($value = self::LABEL) {} const LABEL = 'value'; }
class NumericDefaults {
    const N = 6;
    public float $bare = self::N;
    public ?float $nullable = self::N;
    public float|int $exact = self::N;
    public float|string $coerced = self::N;
    public static ?float $shared = self::N / 2;
}
$numbers = new NumericDefaults();
echo $numbers->bare, $numbers->nullable, $numbers->exact, $numbers->coerced, NumericDefaults::$shared;
echo (new ReflectionProperty('NumericDefaults', 'nullable'))->getDefaultValue();
echo (new ReflectionClass('NumericDefaults'))->getDefaultProperties()['shared'];
echo Consumer::class;
function eagerCompoundValue(): int { echo 'rhs'; return 2; }
try { Consumer::$names[0] += eagerCompoundValue(); }
catch (Error $error) { echo 'compound-default'; }
$bad = new ReflectionProperty(Consumer::class, 'bad');
echo $bad->hasDefaultValue();
try { $bad->getDefaultValue(); } catch (Error $error) { echo $error->getMessage(); }
$parameter = new ReflectionParameter([Promoted::class, '__construct'], 'value');
echo $parameter->isDefaultValueAvailable(), $parameter->getDefaultValueConstantName();
try { $parameter->getDefaultValue(); } catch (Error $error) { echo $error->getMessage(); }
$traitParameter = new ReflectionParameter([Invalid::class, 'labeled'], 'value');
echo $traitParameter->isDefaultValueAvailable();
try { $traitParameter->getDefaultValue(); } catch (Error $error) { echo $error->getMessage(); }
echo (new ReflectionClass(Valid::class))->getDefaultProperties()['value'];
$value = (new ReflectionClass(Valid::class))->newInstanceWithoutConstructor();
echo $value->value;
$source = $argv[1];
eval($source);
class MethodConsumer {
    public function value(int $value = parent::A): int { return $value; }
    public static function staticValue(int $value = parent::A): int { return $value; }
}
class LazyMethod {
    public static function value(array $items = [true ? 'ok' : parent::A]): array { return $items; }
}
class NestedMethodConsumer {
    public static function value(array $items = [parent::A]): array { return $items; }
    public function instance(array $items = ['key' => [parent::A]]): array { return $items; }
    public function __construct(array $items = [parent::A]) {}
}
function freeRoot(int $value = self::A): int { return $value; }
function freeNested(array $values = [parent::A]): array { return $values; }
$freeParameter = new ReflectionParameter('freeRoot', 0);
echo $freeParameter->isDefaultValueConstant(), $freeParameter->getDefaultValueConstantName();
try { freeNested(); } catch (Error $error) { echo 'free-nested'; }
class SelectedDefaultError {
    const ENABLED = false;
    public array $items = [self::ENABLED ? 'ok' : parent::A];
}
$selectedClass = new ReflectionClass('SelectedDefaultError');
try { $selectedClass->getDefaultProperties(); } catch (Error $error) { echo 'class-default'; }
$nestedParameter = new ReflectionParameter([NestedMethodConsumer::class, 'value'], 0);
echo $nestedParameter->isDefaultValueAvailable();
try { $nestedParameter->getDefaultValue(); } catch (Error $error) { echo 'nested-reflection'; }
try { NestedMethodConsumer::value(); } catch (Error $error) { echo 'nested-static'; }
try { new NestedMethodConsumer(); } catch (Error $error) { echo 'nested-constructor'; }
try { call_user_func([NestedMethodConsumer::class, 'value']); }
catch (Error $error) { echo 'nested-descriptor'; }
echo json_encode(LazyMethod::value());
interface ParentlessContract { public function value(int $value = parent::A); }
class LexicalBase { const A = 3; }
class LexicalChild extends LexicalBase {
    public function make(): Closure { return fn(string $parent = parent::class, int $value = parent::A): string => $parent . $value; }
}
echo (new LexicalChild)->make()();
class ParentlessClosure {
    public function make(): Closure { return fn(string $name = parent::class): string => $name; }
    public function nested(): Closure { return fn(array $names = ['parent' => parent::class]): array => $names; }
}
$missingClosure = (new ParentlessClosure())->make();
echo $missingClosure('explicit');
try { $missingClosure(); } catch (Error $error) { echo 'lexical-parent'; }
$nestedClosure = (new ParentlessClosure())->nested();
echo json_encode($nestedClosure([]));
try { $nestedClosure(); } catch (Error $error) { echo 'nested-parent'; }
$unscopedClosure = function(int $value = parent::A): int { return $value; };
echo $unscopedClosure(4);
try { $unscopedClosure(); } catch (Error $error) { echo 'unscoped-parent'; }
$unscopedArrow = fn(string $name = self::class): string => $name;
echo $unscopedArrow('explicit');
try { $unscopedArrow(); } catch (Error $error) { echo 'unscoped-self'; }
try { (new MethodConsumer())->value(); } catch (Error $error) { echo 'instance'; }
try { MethodConsumer::staticValue(); } catch (Error $error) { echo 'static'; }
try { call_user_func([MethodConsumer::class, 'staticValue']); }
catch (Error $error) { echo 'descriptor-static'; }
try { ((new MethodConsumer())->value(...))(); }
catch (Error $error) { echo 'descriptor-instance'; }
echo (new MethodConsumer())->value(7), MethodConsumer::staticValue(value: 8);
"#;
    let module = super::lower_source_at_for_target(source, Path::new("main.php"), Path::new("."),
        Target::parse(target_name).unwrap());
    let consumer = &module.class_infos["Consumer"];
    let lazy_default = module.class_infos["LazyMethod"].static_methods["value"].defaults[0]
        .as_ref().expect("the method retains its default");
    let crate::parser::ast::ExprKind::ArrayLiteral(items) = &lazy_default.kind
        else { panic!("{target_name}: preserve the surrounding default array") };
    assert!(matches!(&items[0].kind, crate::parser::ast::ExprKind::StringLiteral(value) if value == "ok"),
        "{target_name}: a validated literal condition skips its unselected Error");
    assert_eq!(consumer.deferred_property_default_error.as_deref(),
        Some("Cannot use \"parent\" when current class scope has no parent"),
        "{target_name}: initialization must resolve instance defaults before static defaults");
    let slot = consumer.visible_property_index("bad").unwrap();
    assert!(matches!(consumer.defaults[slot].as_ref().unwrap().kind,
        crate::parser::ast::ExprKind::Throw(_)), "{target_name}");
    let valid = &module.class_infos["Valid"];
    let trait_method = &module.declared_trait_methods["Invalid"]["labeled"];
    assert!(matches!(trait_method.signature.defaults[0].as_ref().unwrap().kind,
        crate::parser::ast::ExprKind::Throw(_)), "{target_name}: deferred trait default");
    assert!(!matches!(trait_method.source_defaults[0].as_ref().unwrap().kind,
        crate::parser::ast::ExprKind::Throw(_)), "{target_name}: retain trait source default");
    assert!(module.class_infos["ReflectionParameter"].visible_property_index("__default_error").is_some(),
        "{target_name}: retained parameter Error slot");
    let run = valid.method_decls.iter().find(|method| method.name == "run").unwrap();
    assert!(matches!(run.params[0].2.as_ref().unwrap().kind,
        crate::parser::ast::ExprKind::ScopedConstantAccess {
            receiver: crate::parser::ast::StaticReceiver::Self_, ..
        }), "{target_name}: preserve source-visible constant receiver");
    let main = module.functions.iter().find(|function| function.name == "main").unwrap();
    assert!(main.blocks.iter().any(|block| {
        matches!(block.terminator, Some(crate::ir::Terminator::Throw { .. }))
    }), "{target_name}: lazy lexical defaults lower through ordinary EIR exception control flow");
    assert!(main.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::Borrow),
        "{target_name}: metadata guards must borrow their receiver");
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target_name}: {error:?}"));
    let helper = module.closures.iter().find(|function| function.callable_default.is_some())
        .expect("a nested lazy default has a native EIR helper");
    assert!(helper.flags.is_synthetic, "{target_name}: default helpers are not PHP-visible declarations");
    assert!(helper.instructions.iter().any(|instruction|
        instruction.op == crate::ir::Op::ThrowException),
        "{target_name}: helper errors use ordinary EIR exception control flow");
    assert!(assembly.contains(&crate::names::function_symbol(&helper.name)), "{target_name}: native helper symbol");
    assert!(assembly.contains("__elephc_eval_register_native_property_default_error"), "{target_name}");
    for symbol in ["__elephc_eval_register_native_method_param_default_error",
        "__elephc_eval_register_native_static_method_param_default_error",
        "__elephc_eval_register_native_constructor_param_default_error"] {
        assert!(assembly.contains(symbol), "{target_name}: missing lazy callable default registration {symbol}");
    }
    for symbol in ["__elephc_eval_register_native_method_param_default_expression",
        "__elephc_eval_register_native_static_method_param_default_expression",
        "__elephc_eval_register_native_constructor_param_default_expression",
        "__elephc_eval_register_native_function_param_default_expression",
        "__elephc_eval_register_native_function_param_default_error",
        "__elephc_eval_register_native_function_param_default_constant_name"] {
        assert!(assembly.contains(symbol), "{target_name}: missing nested default expression registration {symbol}");
    }
    assert!(module.class_infos["ReflectionClass"].visible_property_index("__default_error").is_some(),
        "{target_name}: class defaults retain lazy Errors separately from their cached values");
    assert!(module.functions.iter().any(|function| function.lexical_class.is_none()
        && function.callable_default.is_some()), "{target_name}: free defaults have unscoped helpers");
    assert_eq!(module.functions.iter().filter(|function| {
        function.lexical_class.as_deref() == Some("NestedMethodConsumer")
            && function.callable_default.is_some()
    }).count(), 3, "{target_name}: native instance, static and constructor helpers");
    assert!(module.class_infos["ReflectionParameter"].visible_property_index("__default_factory").is_some(),
        "{target_name}: the native and eval paths share one lazy factory slot");
    assert!(module.functions.iter().any(|function| function.instructions.iter().any(|inst|
        inst.op == crate::ir::Op::IToF)),
        "{target_name}: property initializer thunks widen integer values before float storage");
}

/// Separates target cases so each keeps CI's normal per-test timeout budget.
macro_rules! deferred_reflection_target_case {
    ($name:ident, $target:literal) => {
        /// Verifies deferred default reflection metadata and assembly for one supported target.
        #[test]
        fn $name() { deferred_reflection_defaults_on_target($target); }
    };
}

deferred_reflection_target_case!(property_default_review_reflection_macos_aarch64, "macos-aarch64");
deferred_reflection_target_case!(property_default_review_reflection_ios_arm64, "ios-arm64");
deferred_reflection_target_case!(property_default_review_reflection_ios_sim_arm64, "ios-sim-arm64");
deferred_reflection_target_case!(property_default_review_reflection_linux_aarch64, "linux-aarch64");
deferred_reflection_target_case!(property_default_review_reflection_linux_x86_64, "linux-x86_64");

/// Every target reuses preallocated reference cells and initializes missing by-name cells.
#[test]
fn property_default_review_reference_cells_on_all_supported_targets() {
    let source = r#"<?php
class ReferenceProbe {
    public const FLAG = true;
    public string $label = self::FLAG ? 'ok' : 'no';
    public int $count;
    public array $items = self::FLAG ? ['ok'] : [];
}
function inspect(ReferenceProbe $probe): void {
    try { $alias = &$probe->count; } catch (Error $error) { echo $error->getMessage(); }
    $copy = &$probe->items; $copy[] = 'extra';
}
inspect(new ReferenceProbe());
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let class = &module.class_infos["ReferenceProbe"];
        assert!(class.owned_reference_properties.contains("count"), "{name}");
        assert!(class.owned_reference_properties.contains("items"), "{name}");
        let thunk = format!("_class_propinit_{}", class.class_id);
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let initializer = assembly.split(&format!("@fn name={thunk} ")).nth(1)
            .expect("the initializer is emitted")
            .split("@endfn").next().unwrap();
        assert!(initializer.contains("property_reference_initialized"),
            "{name}: the initializer must guard allocation of each object-owned reference cell");
        assert!(initializer.contains("__rt_reference_cell_new"),
            "{name}: zeroed by-name allocations still need an initial reference cell");
    }
}

/// Valid and unbound trait defaults both pass through target-independent EIR.
#[test]
fn property_default_review_all_supported_targets() {
    let source = r#"<?php
trait Bound {
    public string $name = self::class;
    public static string $owner = parent::class;
    public array $names = [self::class, parent::class];
    public function __construct(public string $promoted = parent::class) {}
}

class Base {}
class Consumer extends Base { use Bound; }
trait Unbound { public string $name = parent::class; }
class UnboundConsumer { use Unbound; public static string $valid = 'ok'; }
trait Promoted { public function __construct(public string $name = parent::class) {} }
class PromotedConsumer { use Promoted; }
class DefaultScopeOrphan {
    public static function selfValue(object $value = new self): object { return $value; }
    public static function parentValue(object $value = new parent): object { return $value; }
}
$defaultScopeClosure = function(object $value = new self): object { return $value; };
$consumer = new Consumer();
echo $consumer->name, Consumer::$owner, $consumer->promoted;
try { new UnboundConsumer(); } catch (Error $e) { echo $e->getMessage(); }
try { echo UnboundConsumer::$valid; } catch (Error $e) { echo $e->getMessage(); }
try { new PromotedConsumer(); } catch (Error $e) { echo $e->getMessage(); }
$explicit = new PromotedConsumer('explicit');
echo $explicit->name;
echo get_class(DefaultScopeOrphan::selfValue());
try { DefaultScopeOrphan::parentValue(); } catch (Error $e) { echo $e->getMessage(); }
try { $defaultScopeClosure(); } catch (Error $e) { echo $e->getMessage(); }
unset($defaultScopeClosure);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let orphan = &module.class_infos["DefaultScopeOrphan"];
        assert!(matches!(orphan.static_methods["selfvalue"].defaults[0].as_ref().unwrap().kind,
            crate::parser::ast::ExprKind::NewObject { .. }),
            "{name}: new self binds to its declaring class");
        assert!(matches!(orphan.static_methods["parentvalue"].defaults[0].as_ref().unwrap().kind,
            crate::parser::ast::ExprKind::Throw(_)),
            "{name}: missing new parent scope stays a lazy Error");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// An initialization failure remains inside the active source-level handler.
#[test]
fn property_default_review_constructor_handler() {
    let module = super::lower_source(r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer { use Values; public function __construct(string $value) {} }
function argument(): string { echo 'argument'; return 'value'; }
try { new Consumer(argument()); } catch (Error $e) { echo 'caught'; }
"#);
    let main = module.functions.iter().find(|function| function.name == "main").unwrap();
    assert!(main.instructions.iter().any(|inst| inst.op == crate::ir::Op::TryPushHandler));
    assert!(main.instructions.iter().any(|inst| inst.op == crate::ir::Op::ThrowException));
}

/// Production metadata-aware DCE retains constructor and static-write Error catches too.
#[test]
fn property_default_review_metadata_catch_reachability() {
    let source = r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer {
    use Values;
    public static string $valid = 'ok';
    public function __construct(string $value = 'value') {}
}
try { new Consumer(); } catch (Error $e) { echo 'new'; }
try { Consumer::$valid = 'value'; } catch (Error $e) { echo 'write'; }
"#;
    let parsed = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let check = crate::types::check(&parsed).unwrap();
    let optimizer = crate::optimize::PostTypecheckOptimizer::new_with_type_metadata(
        &parsed, &check.functions, &check.classes, &check.interfaces,
    );
    let optimized = optimizer.eliminate_dead_code(parsed, check.local_binding_decision_spans());
    assert_eq!(optimized.iter().filter(|statement| {
        matches!(&statement.kind, crate::parser::ast::StmtKind::Try { catches, .. } if !catches.is_empty())
    }).count(), 2);
}

/// Metadata-aware DCE preserves omitted-default handlers while pruning supplied clean calls.
#[test]
fn property_default_fourth_review_method_catch_reachability() {
    let source = r#"<?php
class Consumer {
    public function value(int $value = parent::A): int { return $value; }
    public static function staticValue(int $value = parent::A): int { return $value; }
}
try { (new Consumer())->value(); } catch (Error $error) { echo 'instance'; }
try { Consumer::staticValue(); } catch (Error $error) { echo 'static'; }
try { (new Consumer())->value(9); } catch (Error $error) { echo 'bad'; }
try { Consumer::staticValue(value: 9); } catch (Error $error) { echo 'bad'; }
"#;
    let parsed = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let compatibility = crate::optimize::eliminate_dead_code(parsed.clone(), Default::default());
    assert_eq!(compatibility.iter().filter(|statement| {
        matches!(&statement.kind, crate::parser::ast::StmtKind::Try { catches, .. } if !catches.is_empty())
    }).count(), 4, "AST-only defaults must remain conservative without checker metadata");
    let check = crate::types::check(&parsed).unwrap();
    let optimizer = crate::optimize::PostTypecheckOptimizer::new_with_type_metadata(
        &parsed, &check.functions, &check.classes, &check.interfaces,
    );
    let optimized = optimizer.eliminate_dead_code(parsed, check.local_binding_decision_spans());
    assert_eq!(optimized.iter().filter(|statement| {
        matches!(&statement.kind, crate::parser::ast::StmtKind::Try { catches, .. } if !catches.is_empty())
    }).count(), 2);
}

/// Production DCE retains an eager compound RHS exception before the deferred property Error.
#[test]
fn property_default_compound_review_metadata_catch_reachability() {
    for update in ["+=", ".="] {
        let source = format!(r#"<?php
trait Values {{ public string $bad = parent::class; }}
class Consumer {{ use Values; public static array $items = [1]; }}
function value(): int {{ throw new RuntimeException('rhs'); }}
try {{ Consumer::$items[0] {update} value(); }}
catch (RuntimeException $error) {{ echo 'rhs'; }}
catch (Error $error) {{ echo 'default'; }}
"#);
        let parsed = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
        let check = crate::types::check(&parsed).unwrap();
        let optimizer = crate::optimize::PostTypecheckOptimizer::new_with_type_metadata(
            &parsed, &check.functions, &check.classes, &check.interfaces,
        );
        let optimized = optimizer.eliminate_dead_code(parsed, check.local_binding_decision_spans());
        assert!(optimized.iter().any(|statement| matches!(&statement.kind,
            crate::parser::ast::StmtKind::Try { catches, .. } if catches.iter().any(|catch|
                catch.exception_types.iter().any(|name| name == "RuntimeException")))),
            "{update}: retain the RHS exception handler: {optimized:?}");
    }
}

/// Spread defaults must have executable helpers even when they contain no lexical Error node.
fn native_spread_defaults_on_target(target_name: &str) {
    let source = r#"<?php
function spreadDefault(array $items = [PHP_INT_MAX - 1 => "a", ...["b"]]): array { return $items; }
class SpreadDefaults {
    public static function valid(array $items = [PHP_INT_MAX - 1 => "a", ...["b"]]): array { return $items; }
    public function invalid(array $items = [PHP_INT_MAX => "a", ...["b"]]): array { return $items; }
    public function __construct(array $items = [PHP_INT_MAX => "a", ...["b"]]) {}
}
eval($argv[1]);
"#;
    let module = super::lower_source_at_for_target(source, Path::new("main.php"), Path::new("."),
        Target::parse(target_name).unwrap());
    let helpers = module.functions.iter().chain(module.closures.iter())
        .filter(|function| function.callable_default.is_some()).collect::<Vec<_>>();
    assert_eq!(helpers.len(), 4, "{target_name}: free, instance, static and constructor defaults");
    assert_eq!(helpers.iter().filter(|function| function.lexical_class.as_deref() == Some("SpreadDefaults")).count(), 3,
        "{target_name}: default helpers preserve their declaration scope");
    for helper in &helpers {
        assert!(helper.instructions.iter().any(|instruction|
            instruction.op == crate::ir::Op::HashSpread
                && instruction.effects.contains(crate::ir::Effects::MAY_THROW)),
            "{target_name}: spread exhaustion participates in ordinary EIR exception cleanup");
    }
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target_name}: {error:?}"));
    for symbol in ["__elephc_eval_register_native_method_param_default_expression",
        "__elephc_eval_register_native_static_method_param_default_expression",
        "__elephc_eval_register_native_constructor_param_default_expression",
        "__elephc_eval_register_native_function_param_default_expression"] {
        assert!(assembly.contains(symbol), "{target_name}: missing spread expression metadata {symbol}");
    }
    for helper in helpers {
        assert!(assembly.contains(&crate::names::function_symbol(&helper.name)),
            "{target_name}: helper must be emitted for native callable and Reflection factories");
    }
}

/// Keeps each target within the normal unit-test timeout budget.
macro_rules! native_spread_default_target_case {
    ($name:ident, $target:literal) => {
        /// Verifies executable spread defaults and their exception effects on one supported target.
        #[test]
        fn $name() { native_spread_defaults_on_target($target); }
    };
}

native_spread_default_target_case!(native_spread_defaults_macos_aarch64, "macos-aarch64");
native_spread_default_target_case!(native_spread_defaults_ios_arm64, "ios-arm64");
native_spread_default_target_case!(native_spread_defaults_ios_sim_arm64, "ios-sim-arm64");
native_spread_default_target_case!(native_spread_defaults_linux_aarch64, "linux-aarch64");
native_spread_default_target_case!(native_spread_defaults_linux_x86_64, "linux-x86_64");
