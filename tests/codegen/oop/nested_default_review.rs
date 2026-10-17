//! Purpose:
//! Regressions for nested lexical default errors across native and eval call surfaces.
//!
//! Called from:
//! - The codegen integration suite through the OOP test module.
//!
//! Key details:
//! - Defaults remain lazy while earlier constructors and supplied owners keep PHP ordering.
//! - Native cleanup checks are strict; Reflection metadata retains its existing baseline.

use crate::support::{compile_and_run_with_heap_debug, without_ir_opt};

const ERROR: &str = "Cannot access \"parent\" when current class scope has no parent";

/// Checks exact PHP output and optionally requires all generated runtime owners to retire.
fn check(source: &str, expected: &str, clean: bool) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    if clean {
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// Omitted nested method and constructor defaults keep otherwise unreachable Error handlers.
#[test]
fn test_nested_default_ninth_review_catches() {
    check(r#"<?php
class Probe {
    public static function value(array $items = [parent::A]): int { return 1; }
    public function instance(array $items = ['key' => [parent::A]]): int { return 2; }
}
class Maker { public function __construct(array $items = [parent::A]) {} }
$probe = new Probe();
for ($i = 0; $i < 4; $i++) {
    try { Probe::value(); } catch (Error $e) { echo $e->getMessage(), '|'; }
    try { $probe->instance(); } catch (Error $e) { echo $e->getMessage(), '|'; }
    try { new Maker(); } catch (Error $e) { echo $e->getMessage(), '|'; }
}
echo Probe::value([1]), $probe->instance([2]);
unset($probe);
"#, &format!("{}12", format!("{ERROR}|").repeat(12)), true);
}

/// Catch reachability does not depend on EIR optimization.
#[test]
fn test_nested_default_ninth_review_catches_no_ir_opt() {
    without_ir_opt(test_nested_default_ninth_review_catches);
}

/// Descriptor callbacks execute nested native defaults without aborting or losing prefix owners.
#[test]
fn test_nested_default_ninth_review_callbacks() {
    check(r#"<?php
class Value {
    public function __construct() { echo 'C'; }
    public function __destruct() { echo 'D'; }
}
class Probe {
    public static function value(array $items = [new Value(), parent::A]): int { return 1; }
    public function instance(array $items = [new Value(), parent::A]): int { return 2; }
}
$probe = new Probe();
$static = Probe::value(...); $instance = $probe->instance(...);
for ($i = 0; $i < 4; $i++) {
    try { call_user_func([Probe::class, 'value']); } catch (Error $e) { echo $e->getMessage(), '|'; }
    try { call_user_func([$probe, 'instance']); } catch (Error $e) { echo $e->getMessage(), '|'; }
    try { $static(); } catch (Error $e) { echo $e->getMessage(), '|'; }
    try { $instance(); } catch (Error $e) { echo $e->getMessage(), '|'; }
}
echo $static([1]), $instance([2]);
unset($static, $instance, $probe);
"#, &format!("{}12", format!("CD{ERROR}|").repeat(16)), true);
}

/// Native default helper invocation stays independent of EIR optimization.
#[test]
fn test_nested_default_ninth_review_callbacks_no_ir_opt() {
    without_ir_opt(test_nested_default_ninth_review_callbacks);
}

/// Earlier supplied temporaries are rooted before omitted nested defaults execute.
#[test]
fn test_nested_default_ninth_review_argument_cleanup() {
    check(r#"<?php
class Payload {
    public static int $destroyed = 0;
    public function __destruct() { self::$destroyed++; }
}
class Probe {
    public static int $destroyed = 0;
    public static function value(Payload $value, array $items = [parent::A]): void {}
    public function instance(Payload $value, array $items = [parent::A]): void {}
    public function __destruct() { self::$destroyed++; }
}
function run(): void { for ($i = 0; $i < 4; $i++) {
    try { Probe::value(new Payload()); } catch (Error $e) { echo $e->getMessage(), '|'; }
    try { Probe::value(value: new Payload()); } catch (Error $e) { echo $e->getMessage(), '|'; }
    try { (new Probe())->instance(new Payload()); } catch (Error $e) { echo $e->getMessage(), '|'; }
} }
run();
echo Payload::$destroyed, ':', Probe::$destroyed;
"#, &format!("{}12:4", format!("{ERROR}|").repeat(12)), true);
}

/// Argument and temporary receiver unwind ownership is invariant with optimization disabled.
#[test]
fn test_nested_default_ninth_review_argument_cleanup_no_ir_opt() {
    without_ir_opt(test_nested_default_ninth_review_argument_cleanup);
}

/// Native Reflection retains default presence and executes its complete tree only on request.
#[test]
fn test_nested_default_ninth_review_reflection() {
    check(r#"<?php
class Value {
    public function __construct() { echo 'C'; }
    public function __destruct() { echo 'D'; }
}
class Probe {
    public static function value(array $items = [new Value(), parent::A]): array { return $items; }
}
$parameter = new ReflectionParameter([Probe::class, 'value'], 0);
echo (int)$parameter->isDefaultValueAvailable(), '|';
for ($i = 0; $i < 4; $i++) {
    try { $parameter->getDefaultValue(); } catch (Error $e) { echo $e->getMessage(), '|'; }
}
unset($parameter);
"#, &format!("1|{}", format!("CD{ERROR}|").repeat(4)), false);
}

/// Reflection default materialization stays lazy when EIR optimization is disabled.
#[test]
fn test_nested_default_ninth_review_reflection_no_ir_opt() {
    without_ir_opt(test_nested_default_ninth_review_reflection);
}

/// Opaque eval transports nested defaults as data rather than throwing through native callbacks.
#[test]
fn test_nested_default_ninth_review_eval() {
    check(r#"<?php
class Value {
    public function __construct() { echo 'C'; }
    public function __destruct() { echo 'D'; }
}
class Probe {
    public static function value(array $items = [new Value(), parent::A]): array { return $items; }
    public function instance(array $items = ['key' => [parent::A]]): array { return $items; }
}
class Maker { public function __construct(array $items = [parent::A]) {} }
function run(string $source): void { echo eval($source); }
$native = new ReflectionMethod(Probe::class, 'value');
unset($native);
for ($i = 0; $i < 4; $i++) {
    run('try { Probe::value(); } catch (Error $e) { return $e->getMessage() . "|"; } return "missed|";');
    run('try { (new Probe())->instance(); } catch (Error $e) { return $e->getMessage() . "|"; } return "missed|";');
    run('try { new Maker(); } catch (Error $e) { return $e->getMessage() . "|"; } return "missed|";');
    run('$parameters = (new ReflectionMethod("Probe", "value"))->getParameters(); $p = $parameters[0]; echo (int)$p->isDefaultValueAvailable(), "|"; try { $p->getDefaultValue(); } catch (Error $e) { return $e->getMessage() . "|"; } return "missed|";');
}
"#, &format!("CD{ERROR}|{ERROR}|{ERROR}|1|CD{ERROR}|").repeat(4), false);
}

/// Eval's error transport and reflection defaults do not depend on EIR optimization.
#[test]
fn test_nested_default_ninth_review_eval_no_ir_opt() {
    without_ir_opt(test_nested_default_ninth_review_eval);
}

/// An eval-created parameter keeps its lazy factory callable after returning to native code.
#[test]
fn test_nested_default_ninth_review_eval_reflection_escape() {
    check(r#"<?php
class Value {
    public function __construct() { echo 'C'; }
    public function __destruct() { echo 'D'; }
}
class Probe {
    public static function value(array $items = [new Value(), parent::A]): array { return $items; }
}
function reflect(string $source): mixed { return eval($source); }
$parameter = reflect('return new ReflectionParameter(["Probe", "value"], 0);');
echo (int)$parameter->isDefaultValueAvailable(), '|';
for ($i = 0; $i < 4; $i++) {
    try { $parameter->getDefaultValue(); } catch (Error $e) { echo $e->getMessage(), '|'; }
}
unset($parameter);
"#, &format!("1|{}", format!("CD{ERROR}|").repeat(4)), false);
}

/// Escaped eval default factories retain their context when EIR optimization is disabled.
#[test]
fn test_nested_default_ninth_review_eval_reflection_escape_no_ir_opt() {
    without_ir_opt(test_nested_default_ninth_review_eval_reflection_escape);
}

/// Property normalization retains untaken ternary and coalescing branches around lazy Errors.
#[test]
fn test_nested_default_ninth_review_property_branches() {
    check(r#"<?php
class Probe {
    public array $items = [true ? 'ok' : parent::A, false && parent::A,
        true || parent::A, 'ok' ?: parent::A];
    public static array $other = ['key' => ['ok' ?? parent::A]];
}
for ($i = 0; $i < 4; $i++) {
    $probe = new Probe();
    echo json_encode($probe->items), ':', json_encode(Probe::$other), '|';
    unset($probe);
}
"#, "[\"ok\",false,true,\"ok\"]:{\"key\":[\"ok\"]}|".repeat(4).as_str(), true);
}

/// Lazy property default branches stay intact without EIR optimization.
#[test]
fn test_nested_default_ninth_review_property_branches_no_ir_opt() {
    without_ir_opt(test_nested_default_ninth_review_property_branches);
}

/// Class-constant conditions keep deferred property branches lazy until initialization.
#[test]
fn test_nested_default_tenth_review_nonliteral_property_condition() {
    check(r#"<?php
class Probe {
    public const ENABLED = true;
    public array $items = [self::ENABLED ? 'ok' : parent::A];
}
for ($i = 0; $i < 4; $i++) {
    $probe = new Probe();
    echo json_encode($probe->items), '|';
    unset($probe);
}
"#, "[\"ok\"]|".repeat(4).as_str(), true);
}

/// Nonliteral property conditions initialize identically without EIR optimization.
#[test]
fn test_nested_default_tenth_review_nonliteral_property_condition_no_ir_opt() {
    without_ir_opt(test_nested_default_tenth_review_nonliteral_property_condition);
}

/// Dynamic and reflected allocation share the EIR property initializer for nonliteral branches.
#[test]
fn test_nested_default_tenth_review_nonliteral_dynamic_property_condition() {
    check(r#"<?php
class Probe {
    public const ENABLED = true;
    public array $items = [self::ENABLED ? 'ok' : parent::A];
}
$class = Probe::class;
$reflection = new ReflectionClass(Probe::class);
for ($i = 0; $i < 4; $i++) {
    $dynamic = new $class();
    $reflected = $reflection->newInstanceWithoutConstructor();
    echo json_encode($dynamic->items), ':', json_encode($reflected->items), '|';
    unset($dynamic, $reflected);
}
unset($reflection);
"#, "[\"ok\"]:[\"ok\"]|".repeat(4).as_str(), false);
}

/// Dynamic nonliteral property initialization remains stable without EIR optimization.
#[test]
fn test_nested_default_tenth_review_nonliteral_dynamic_property_condition_no_ir_opt() {
    without_ir_opt(test_nested_default_tenth_review_nonliteral_dynamic_property_condition);
}

/// Chained reads preserve their object type after a deferred property initializer throws.
#[test]
fn test_nested_default_tenth_review_chained_new_error() {
    check(r#"<?php
trait Defaults { public int $value = parent::A; }
class Probe { use Defaults; }
for ($i = 0; $i < 4; $i++) {
    try { echo (new Probe())->value; }
    catch (Error $error) { echo $error->getMessage(), '|'; }
}
echo 'after';
"#, &format!("{}after", format!("{ERROR}|").repeat(4)), true);
}

/// Chained deferred construction errors retain their result type without EIR optimization.
#[test]
fn test_nested_default_tenth_review_chained_new_error_no_ir_opt() {
    without_ir_opt(test_nested_default_tenth_review_chained_new_error);
}

/// Builds identical slot checks for direct, dynamic and reflected allocation paths.
fn reference_property_program(construction: &str) -> String {
    r#"<?php
class Probe {
    public const FLAG = true;
    public string $label = self::FLAG ? 'ok' : 'no';
    public int $count;
    public array $items = self::FLAG ? ['ok'] : [];
}
function inspect(Probe $probe): void {
    echo $probe->label, ':', isset($probe->count) ? 'bad' : 'U', ':';
    try { $alias = &$probe->count; echo 'bad'; }
    catch (Error $error) { echo 'E:'; unset($error); }
    $copy = &$probe->items;
    $copy[] = 'extra';
    echo json_encode($probe->items), '|';
}
/* construction */
"#.replace("/* construction */", construction)
}

/// Shared initializer thunks preserve uninitialized reference properties without leaking cells.
#[test]
fn test_nested_default_grok_review_reference_property_cells() {
    check(&reference_property_program("inspect(new Probe()); $class = Probe::class; $dynamic = new $class(); if ($dynamic instanceof Probe) { inspect($dynamic); } else { echo 'bad-class'; } unset($dynamic);"),
        "ok:U:E:[\"ok\",\"extra\"]|".repeat(2).as_str(), true);
}

/// Reference-cell initialization is equally balanced without EIR optimization.
#[test]
fn test_nested_default_grok_review_reference_property_cells_no_ir_opt() {
    without_ir_opt(test_nested_default_grok_review_reference_property_cells);
}

/// By-name reflected construction installs an uninitialized cell when allocation did not.
#[test]
fn test_nested_default_grok_review_reflected_reference_property_cells() {
    check(&reference_property_program("$reflection = new ReflectionClass(Probe::class); inspect($reflection->newInstanceWithoutConstructor()); unset($reflection);"),
        "ok:U:E:[\"ok\",\"extra\"]|", false);
}

/// Reflected reference slots retain their initialization state without EIR optimization.
#[test]
fn test_nested_default_grok_review_reflected_reference_property_cells_no_ir_opt() {
    without_ir_opt(test_nested_default_grok_review_reflected_reference_property_cells);
}

/// PHP defers global parent::class closure defaults using the no-scope self wording.
#[test]
fn test_nested_default_grok_review_global_parent_class_default() {
    check(r#"<?php
$closure = function (string $name = parent::class): string { return $name; };
$arrow = fn(string $name = parent::class): string => $name;
echo $closure('kept'), '|', $arrow('kept'), '|';
try { $closure(); } catch (Error $error) { echo $error->getMessage(), '|'; unset($error); }
try { $arrow(); } catch (Error $error) { echo $error->getMessage(); unset($error); }
unset($closure, $arrow);
"#, "kept|kept|Cannot use \"self\" when no class scope is active|Cannot use \"self\" when no class scope is active", true);
}

/// Global relative-name default deferral retains PHP's behavior without EIR optimization.
#[test]
fn test_nested_default_grok_review_global_parent_class_default_no_ir_opt() {
    without_ir_opt(test_nested_default_grok_review_global_parent_class_default);
}

/// A trait's native default helpers preserve each consuming class's constructor scope.
#[test]
fn test_nested_default_ninth_review_trait_helper_scope() {
    check(r#"<?php
trait Defaults {
    public static function value(array $items = [new self(), parent::A]): array { return $items; }
}
class First {
    use Defaults;
    public function __construct() { echo 'A'; }
    public function __destruct() { echo 'a'; }
}
class Second {
    use Defaults;
    public function __construct() { echo 'B'; }
    public function __destruct() { echo 'b'; }
}
$first = First::value(...); $second = Second::value(...);
for ($i = 0; $i < 4; $i++) {
    try { $first(); } catch (Error $e) { echo $e->getMessage(), '|'; }
    try { $second(); } catch (Error $e) { echo $e->getMessage(), '|'; }
}
unset($first, $second);
"#, format!("Aa{ERROR}|Bb{ERROR}|").repeat(4).as_str(), true);
}

/// Trait-consumer constructor scope is independent of EIR optimization.
#[test]
fn test_nested_default_ninth_review_trait_helper_scope_no_ir_opt() {
    without_ir_opt(test_nested_default_ninth_review_trait_helper_scope);
}

/// Inherited native defaults construct the declaring class rather than the receiver subclass.
#[test]
fn test_nested_default_ninth_review_inherited_factory_scope() {
    check(r#"<?php
class Probe {
    public function __construct() { echo get_class($this), 'C'; }
    public function __destruct() { echo get_class($this), 'D'; }
    public static function value(array $items = [new self(), parent::A]): array { return $items; }
}
class Child extends Probe {}
function run(string $source): mixed { return eval($source); }
$native = new ReflectionParameter([Child::class, 'value'], 0);
$escaped = run('return new ReflectionParameter(["Child", "value"], 0);');
echo (int)$native->isDefaultValueAvailable(), '|';
for ($i = 0; $i < 4; $i++) {
    try { call_user_func([Child::class, 'value']); } catch (Error $e) { echo $e->getMessage(), '|'; }
    try { $native->getDefaultValue(); } catch (Error $e) { echo $e->getMessage(), '|'; }
    echo run('try { Child::value(); } catch (Error $e) { return $e->getMessage() . "|"; } return "missed|";');
    try { $escaped->getDefaultValue(); } catch (Error $e) { echo $e->getMessage(), '|'; }
}
unset($native, $escaped);
"#, &format!("1|{}", format!("ProbeCProbeD{ERROR}|").repeat(16)), false);
}

/// Inherited default factories preserve the constructed class without EIR optimization.
#[test]
fn test_nested_default_ninth_review_inherited_factory_scope_no_ir_opt() {
    without_ir_opt(test_nested_default_ninth_review_inherited_factory_scope);
}
