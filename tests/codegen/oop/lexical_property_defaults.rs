//! Purpose:
//! Covers lexical class names in property and promoted-constructor defaults.
//! Checks trait consumer binding and deferred errors for missing parent scopes.
//!
//! Called from:
//! - `cargo test --test codegen_tests property_default_review_`.
//!
//! Key details:
//! - Stored defaults must preserve declaration scope rather than caller or subclass scope.

use crate::support::{compile_and_run_with_heap_debug, without_ir_opt};

/// Method default arrays retain lazy branches around missing parent constants.
#[test]
fn test_property_default_eighth_review_method_lazy_branches() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Probe {
    public static function value(array $items = [true ? 'ok' : parent::A]): array { return $items; }
    public function nested(array $items = ['kept' => ['ok' ?? parent::A]]): array { return $items; }
    public static function missing(array $items = [false ? 'ok' : parent::A]): array { return $items; }
}
$probe = new Probe();
for ($i = 0; $i < 8; $i++) {
    echo json_encode(Probe::value()), ':', json_encode($probe->nested()), '|';
    try { Probe::missing(); } catch (Error $e) { echo $e->getMessage(), '|'; }
}
echo json_encode(Probe::missing(['explicit']));
unset($probe);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    let expected = "[\"ok\"]:{\"kept\":[\"ok\"]}|Cannot access \"parent\" when current class scope has no parent|";
    assert_eq!(out.stdout, format!("{}[\"explicit\"]", expected.repeat(8)), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Method defaults retain earlier constructor effects and retire partial arrays on an error.
#[test]
fn test_property_default_eighth_review_method_prefix_cleanup() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Value { public function __construct() { echo 'C'; } public function __destruct() { echo 'D'; } }
trait Factory { public static function value(array $items = [new Value(), parent::A]): array { return $items; } }
class Probe { use Factory; }
for ($i = 0; $i < 8; $i++) {
    try { Probe::value(); } catch (Error $e) { echo $e->getMessage(), '|'; }
}
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "CDCannot access \"parent\" when current class scope has no parent|".repeat(8), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Opaque eval class reflection raises registered native default errors rather than skipping slots.
#[test]
fn test_property_default_eighth_review_eval_class_defaults() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Probe { public string $good = 'ok'; public int $bad = parent::A; }
class Child extends Probe {}
class Valid { public string $good = 'ok'; public static array $items = [1, 2]; }
function run(string $code): void { echo eval($code); }
for ($i = 0; $i < 8; $i++) {
    run('try { (new ReflectionClass("Probe"))->getDefaultProperties(); } catch (Error $e) { return $e->getMessage() . "|"; } return "missed|";');
    run('try { (new ReflectionClass("Child"))->getDefaultProperties(); } catch (Error $e) { return $e->getMessage() . "|"; } return "missed|";');
}
run('return json_encode((new ReflectionClass("Valid"))->getDefaultProperties());');
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, format!("{}{{\"good\":\"ok\",\"items\":[1,2]}}",
        "Cannot access \"parent\" when current class scope has no parent|".repeat(16)), "{}", out.stderr);
    // Native ReflectionClass metadata has separate pre-existing retention. Isolated
    // interpreter regressions assert zero temporary owners for this materialization path.
}

/// Method branch laziness stays independent of EIR optimization.
#[test]
fn test_property_default_eighth_review_method_lazy_no_ir_opt() {
    without_ir_opt(test_property_default_eighth_review_method_lazy_branches);
}

/// Method prefix cleanup stays independent of EIR optimization.
#[test]
fn test_property_default_eighth_review_method_prefix_no_ir_opt() {
    without_ir_opt(test_property_default_eighth_review_method_prefix_cleanup);
}

/// Native class-default Errors in eval stay independent of EIR optimization.
#[test]
fn test_property_default_eighth_review_eval_class_no_ir_opt() {
    without_ir_opt(test_property_default_eighth_review_eval_class_defaults);
}

/// Parentless lexical closures and arrows are declared and fail only when their default is used.
#[test]
fn test_property_default_seventh_review_parentless_lexical() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Probe {
    public function closure(): Closure { return function(string $name = parent::class): string { return $name; }; }
    public function arrow(): Closure { return fn(string $name = parent::class): string => $name; }
}
$probe = new Probe(); $closure = $probe->closure(); $arrow = $probe->arrow();
echo $closure('X'), ':', $arrow('Y'), '|';
try { $closure(); } catch (Error $e) { echo $e->getMessage(), '|'; }
try { $arrow(); } catch (Error $e) { echo $e->getMessage(); }
unset($closure, $arrow, $probe);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    let message = "Cannot use \"parent\" when current class scope has no parent";
    assert_eq!(out.stdout, format!("X:Y|{message}|{message}"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Enum lexical defaults remain lazy without growing the runtime's existing singleton baseline.
#[test]
fn test_property_default_seventh_review_enum_lexical() {
    let retained = |iterations| {
        let source = format!(r#"<?php
enum Choice {{ case Item; public function make(): Closure {{ return fn(string $name = parent::class): string => $name; }} }}
$closure = Choice::Item->make();
echo $closure('Z'), '|';
for ($i = 0; $i < {iterations}; $i++) {{
    try {{ $closure(); }} catch (Error $e) {{ echo $e->getMessage(), '|'; }}
}}
unset($closure);
"#);
        let out = compile_and_run_with_heap_debug(&source);
        assert!(out.success, "{}", out.stderr);
        let message = "Cannot use \"parent\" when current class scope has no parent|";
        assert_eq!(out.stdout, format!("Z|{}", message.repeat(iterations)), "{}", out.stderr);
        let totals = out.stderr.lines().find(|line| line.starts_with("HEAP DEBUG: allocs=")).unwrap();
        totals.split_once("live_blocks=").unwrap().1.split_once(" peak_live_bytes=").unwrap().0.to_string()
    };
    assert_eq!(retained(1), retained(24));
}

/// Trait consumers bind their own scope and preserve first-error ordering in nested array defaults.
#[test]
fn test_property_default_seventh_review_trait_array_lexical() {
    let out = compile_and_run_with_heap_debug(r#"<?php
trait Factory { public function make(): Closure { return fn(array $items = [parent::A, parent::class]): array => $items; } }
class Orphan { use Factory; }
class Base { const A = 7; }
class Bound extends Base { use Factory; }
$orphan = new Orphan(); $missing = $orphan->make();
echo json_encode($missing([4, 'X'])), '|';
try { $missing(); } catch (Error $e) { echo $e->getMessage(), '|'; }
$bound = new Bound(); $good = $bound->make(); echo json_encode($good());
unset($missing, $good, $orphan, $bound);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "[4,\"X\"]|Cannot access \"parent\" when current class scope has no parent|[7,\"Base\"]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Both class-name keywords outside a lexical class produce PHP's self no-scope message lazily.
#[test]
fn test_property_default_seventh_review_unscoped_class_names() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$parent = function(string $name = parent::class): string { return $name; };
$self = fn(string $name = self::class): string => $name;
$arrow = fn(string $name = parent::class): string => $name;
echo $parent('P'), ':', $self('S'), ':', $arrow('A'), '|';
try { $parent(); } catch (Error $e) { echo $e->getMessage(), '|'; }
try { $self(); } catch (Error $e) { echo $e->getMessage(), '|'; }
try { $arrow(); } catch (Error $e) { echo $e->getMessage(); }
unset($parent, $self, $arrow);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    let message = "Cannot use \"self\" when no class scope is active";
    assert_eq!(out.stdout, format!("P:S:A|{message}|{message}|{message}"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Unscoped ordinary constants defer their access error and accept an explicit typed argument.
#[test]
fn test_property_default_seventh_review_unscoped_parent_constant() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$value = function(int $value = parent::A): int { return $value; };
echo $value(4), '|';
try { $value(); } catch (Error $e) { echo $e->getMessage(); }
unset($value);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "4|Cannot access \"parent\" when no class scope is active", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A default array evaluates earlier constructors and retires its partial values before a lazy error.
#[test]
fn test_property_default_seventh_review_array_prefix_effects() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Value {
    public function __construct() { echo 'C'; }
    public function __destruct() { echo 'D'; }
}
class Factory { public function make(): Closure {
    return fn(array $items = [new Value(), parent::class]): array => $items;
} }
$factory = new Factory(); $closure = $factory->make();
echo json_encode($closure([])), '|';
try { $closure(); } catch (Error $e) { echo $e->getMessage(); }
unset($closure, $factory);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "[]|CDCannot use \"parent\" when current class scope has no parent", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A native helper preserves a successful lazy branch and returns an owned array on every call.
#[test]
fn test_property_default_seventh_review_lazy_branches() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Factory { public function make(): Closure {
    return fn(array $items = [true ? 'ok' : parent::class]): array => $items;
} }
$factory = new Factory(); $closure = $factory->make();
for ($i = 0; $i < 12; $i++) { echo json_encode($closure()), '|'; }
unset($closure, $factory);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "[\"ok\"]|".repeat(12), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A successful native default transfers its owned object without retaining the helper's box.
#[test]
fn test_property_default_seventh_review_owned_helper_success() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Value {
    public function __construct() { echo 'C'; }
    public function __destruct() { echo 'D'; }
}
class Factory { public function make(): Closure {
    return fn(object $value = new Value() ?? parent::class): object => $value;
} }
$factory = new Factory(); $closure = $factory->make();
for ($i = 0; $i < 12; $i++) { $value = $closure(); unset($value); }
unset($closure, $factory);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "CD".repeat(12), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested keyed defaults retire earlier object and string values when an unscoped error is caught.
#[test]
fn test_property_default_seventh_review_nested_keyed_cleanup() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Value {
    public function __construct() { echo 'C'; }
    public function __destruct() { echo 'D'; }
}
$closure = fn(array $items = ['owner' => new Value(), 'nested' => ['bad' => self::class]]): array => $items;
for ($i = 0; $i < 12; $i++) {
    try { $closure(); } catch (Error $e) { echo $e->getMessage(), '|'; }
}
unset($closure);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "CDCannot use \"self\" when no class scope is active|".repeat(12), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Omitted versus explicit defaults, lexical scopes and closure cleanup remain independent of IR passes.
#[test]
fn test_property_default_seventh_review_without_ir_opt() {
    without_ir_opt(|| {
        test_property_default_seventh_review_parentless_lexical();
        test_property_default_seventh_review_enum_lexical();
        test_property_default_seventh_review_trait_array_lexical();
        test_property_default_seventh_review_unscoped_class_names();
        test_property_default_seventh_review_unscoped_parent_constant();
        test_property_default_seventh_review_array_prefix_effects();
        test_property_default_seventh_review_lazy_branches();
        test_property_default_seventh_review_nested_keyed_cleanup();
        test_property_default_seventh_review_owned_helper_success();
    });
}

/// Eval callback default Errors keep identical ownership without EIR optimization.
#[test]
fn test_property_default_sixth_review_eval_callable_without_ir_opt() {
    without_ir_opt(test_property_default_sixth_review_eval_callable);
}

/// Eval direct-call default Errors keep identical semantics without EIR optimization.
#[test]
fn test_property_default_sixth_review_eval_method_without_ir_opt() {
    without_ir_opt(test_property_default_sixth_review_eval_method);
}

/// Eval constructor default Errors keep identical semantics without EIR optimization.
#[test]
fn test_property_default_sixth_review_eval_constructor_without_ir_opt() {
    without_ir_opt(test_property_default_sixth_review_eval_constructor);
}

/// Lexical closure and interface defaults do not depend on EIR optimization.
#[test]
fn test_property_default_sixth_review_lexical_without_ir_opt() {
    without_ir_opt(|| {
        test_property_default_sixth_review_closure_scope();
        test_property_default_sixth_review_arrow_scope();
        test_property_default_sixth_review_interface_parent_constant();
    });
}

/// Dynamic eval callable adapters retain a lazy method-default Error and accept explicit values.
#[test]
fn test_property_default_sixth_review_eval_callable() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Consumer { public static function value(int $value = parent::A): int { return $value; } }
function run(string $code): void { try { echo eval($code); } catch (Error $e) { echo get_class($e), ':', $e->getMessage(), '|'; } }
run('return call_user_func([Consumer::class, "value"]);');
run('return call_user_func([Consumer::class, "value"], 4);');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Error:Cannot access \"parent\" when current class scope has no parent|4");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Dynamic eval direct static calls materialize the same catchable missing-parent Error.
#[test]
fn test_property_default_sixth_review_eval_method() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Consumer { public static function value(int $value = parent::A): int { return $value; } }
function run(string $code): void { try { echo eval($code); } catch (Error $e) { echo get_class($e), ':', $e->getMessage(), '|'; } }
run('return Consumer::value();'); run('return Consumer::value(4);');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Error:Cannot access \"parent\" when current class scope has no parent|4");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Dynamic eval constructors defer promoted parameter errors until the argument is omitted.
#[test]
fn test_property_default_sixth_review_eval_constructor() {
    let out = compile_and_run_with_heap_debug(r#"<?php
trait Values { public function __construct(public int $value = parent::A) {} }
class Consumer { use Values; }
function run(string $code): void { try { eval($code); } catch (Error $e) { echo get_class($e), ':', $e->getMessage(), '|'; } }
run('new Consumer();'); run('$object = new Consumer(4); echo $object->value;');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Error:Cannot access \"parent\" when current class scope has no parent|4");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Interface defaults have no parent class scope but remain declared and lazily reflectable.
#[test]
fn test_property_default_sixth_review_interface_parent_constant() {
    let out = crate::support::compile_and_run(r#"<?php
interface I { public function f(int $a = parent::A); }
class C implements I { public function f(int $a = 4): int { return $a; } }
$parameter = new ReflectionParameter([I::class, 'f'], 'a');
echo $parameter->isDefaultValueAvailable() ? 'available|' : 'missing|';
try { $parameter->getDefaultValue(); } catch (Error $e) { echo $e->getMessage(), '|'; }
echo (new C)->f();
"#);
    assert_eq!(out, "available|Cannot access \"parent\" when current class scope has no parent|4");
}

/// Repeated interface default Errors do not grow ReflectionParameter's fixed metadata graph.
#[test]
fn test_property_default_sixth_review_interface_default_ownership() {
    let retained = |iterations| {
        let source = format!(r#"<?php
interface I {{ public function f(int $a = parent::A); }}
class C implements I {{ public function f(int $a = 4): int {{ return $a; }} }}
$parameter = new ReflectionParameter([I::class, 'f'], 'a');
for ($i = 0; $i < {iterations}; $i++) {{
    try {{ $parameter->getDefaultValue(); }} catch (Error $error) {{}}
}}
unset($parameter); echo 'done';
"#);
        let out = compile_and_run_with_heap_debug(&source);
        assert!(out.success, "{}", out.stderr);
        assert_eq!(out.stdout, "done");
        let totals = out.stderr.lines().find(|line| line.starts_with("HEAP DEBUG: allocs=")).unwrap();
        totals.split_once("live_blocks=").unwrap().1.split_once(" peak_live_bytes=").unwrap().0.to_string()
    };
    assert_eq!(retained(1), retained(40));
}

/// Closure defaults bind self and parent in their enclosing lexical class.
#[test]
fn test_property_default_sixth_review_closure_scope() {
    let out = compile_and_run(r#"<?php
class Base { const A = 3; }
class Child extends Base {
    public function make(): Closure { return function(string $parent = parent::class, string $self = self::class, int $value = parent::A): string { return $parent . ':' . $self . ':' . $value; }; }
}
echo (new Child)->make()(), '|', (new Child)->make()('other');
"#);
    assert_eq!(out, "Base:Child:3|other:Child:3");
}

/// Arrow defaults use the same lexical scope instead of the descriptor caller's scope.
#[test]
fn test_property_default_sixth_review_arrow_scope() {
    let out = compile_and_run(r#"<?php
class Base { const A = 3; }
class Child extends Base {
    public function make(): Closure { return fn(string $parent = parent::class, string $self = self::class, int $value = parent::A): string => $parent . ':' . $self . ':' . $value; }
}
echo (new Child)->make()(), '|', (new Child)->make()('other');
"#);
    assert_eq!(out, "Base:Child:3|other:Child:3");
}

/// Descriptor-backed static calls materialize lazy default Errors inside the caller's handler.
#[test]
fn test_property_default_fifth_review_static_descriptor() {
    let out = compile_and_run(r#"<?php
class Consumer {
    public static function value(int $value = parent::A, int $other = 2): int {
        echo 'body'; return $value + $other;
    }
}
try { call_user_func([Consumer::class, 'value']); }
catch (Error $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
try { call_user_func_array([Consumer::class, 'value'], ['other' => 4]); }
catch (Error $error) { echo $error->getMessage(), '|'; }
echo call_user_func([Consumer::class, 'value'], 7), '|after';
"#);
    let message = "Cannot access \"parent\" when current class scope has no parent";
    assert_eq!(out, format!("Error:{message}|{message}|body9|after"));
}

/// Immediate instance callable defaults throw rather than aborting or entering the method body.
#[test]
fn test_property_default_fifth_review_instance_descriptor() {
    let out = compile_and_run(r#"<?php
class Consumer {
    public function inst(int $value = parent::A): int { echo 'body'; return $value; }
}
try { ((new Consumer())->inst(...))(); }
catch (Error $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
echo ((new Consumer())->inst(...))(7), '|after';
"#);
    assert_eq!(out, "Error:Cannot access \"parent\" when current class scope has no parent|body7|after");
}

/// An invoker releases previously staged heap arguments when a later default raises an Error.
#[test]
fn test_property_default_fifth_review_descriptor_argument_ownership() {
    let out = compile_and_run(r#"<?php
class Payload { public function __destruct() { echo 'D'; } }
class Consumer {
    public static function value(Payload $payload, int $value = parent::A): int {
        echo 'body'; return $value;
    }
}
function invoke(): void {
    $payload = new Payload();
    try { call_user_func([Consumer::class, 'value'], $payload); }
    catch (Error $error) { echo 'caught|'; }
}
invoke();
echo 'after';
"#);
    assert_eq!(out, "caught|Dafter");
}

/// Earlier heap defaults unwind and trait-imported parent class names keep their own wording.
#[test]
fn test_property_default_fifth_review_descriptor_default_ownership() {
    let out = compile_and_run(r#"<?php
trait Values {
    public static function labeled(string $label = parent::class): string { return $label; }
}
class Consumer {
    use Values;
    public static function value(array $items = ['key' => [1]], int $value = parent::A): int {
        echo 'body'; return $value;
    }
}
try { call_user_func([Consumer::class, 'value']); }
catch (Error $error) { echo 'array-caught|'; }
try { call_user_func([Consumer::class, 'labeled']); }
catch (Error $error) { echo $error->getMessage(), '|'; }
echo call_user_func([Consumer::class, 'labeled'], 'explicit');
"#);
    assert_eq!(out, "array-caught|Cannot use \"parent\" when current class scope has no parent|explicit");
}

/// Exact instance calls retain handlers for omitted defaults, including static and dynamic unpacks.
#[test]
fn test_property_default_fourth_review_exact_instance_calls() {
    let out = compile_and_run(r#"<?php
class Consumer { public function value(int $value = parent::A, int $other = 2): int { return $value + $other; } }
class Child extends Consumer {}
try { (new Consumer())->value(); } catch (Error $error) { echo 'instance|'; }
try { (new Child())->value(...[]); } catch (Error $error) { echo 'inherited|'; }
try { (new Consumer())->value(other: 4); } catch (Error $error) { echo 'named|'; }
$args = $argc > 100 ? [8] : [];
try { (new Consumer())->value(...$args); } catch (Error $error) { echo 'dynamic|'; }
echo (new Consumer())->value(5), '|', (new Consumer())->value(...[7]);
"#);
    assert_eq!(out, "instance|inherited|named|dynamic|7|9");
}

/// Exact static calls retain lazy-default handlers without evaluating a supplied default.
#[test]
fn test_property_default_fourth_review_exact_static_calls() {
    let out = compile_and_run(r#"<?php
class Consumer { public static function value(int $value = parent::A, int $other = 2): int { return $value + $other; } }
class Child extends Consumer {}
try { Consumer::value(); } catch (Error $error) { echo 'static|'; }
try { Child::value(...[]); } catch (Error $error) { echo 'inherited|'; }
try { Consumer::value(other: 4); } catch (Error $error) { echo 'named|'; }
$args = $argc > 100 ? [8] : [];
try { Consumer::value(...$args); } catch (Error $error) { echo 'dynamic|'; }
echo Consumer::value(value: 5), '|', Consumer::value(...[7]);
"#);
    assert_eq!(out, "static|inherited|named|dynamic|7|9");
}

/// Caller arguments and temporary receivers unwind once when a named omission selects an Error.
#[test]
fn test_property_default_fourth_review_named_argument_ownership() {
    let out = compile_and_run(r#"<?php
class Payload {}
class Consumer {
    public function value(int $value = parent::A, ?Payload $payload = null): int { return $value; }
    public static function staticValue(int $value = parent::A, ?Payload $payload = null): int { return $value; }
    public function positional(Payload $payload, int $value = parent::A): int { return $value; }
    public static function staticPositional(Payload $payload, int $value = parent::A): int { return $value; }
    public function text(int $value = parent::A): string { return 'v' . $value; }
}
function argument(): Payload { echo 'S'; return new Payload(); }
try { (new Consumer())->value(payload: argument()); } catch (Error $error) { echo 'E|'; }
try { Consumer::staticValue(payload: argument()); } catch (Error $error) { echo 'E'; }
echo '|';
try { (new Consumer())->positional(argument()); } catch (Error $error) { echo 'E|'; }
try { Consumer::staticPositional(argument()); } catch (Error $error) { echo 'E'; }
echo '|', (new Consumer())->text(7);
"#);
    assert_eq!(out, "SE|SE|SE|SE|v7");
}

/// Mixed parent constant/name arrays in trait properties and signatures keep their lazy first error.
#[test]
fn test_property_default_third_review_trait_parent_arrays_remain_lazy() {
    let out = compile_and_run(r#"<?php
trait Properties { public array $names = [parent::A, parent::class]; }
trait Methods { public function value(array $names = [parent::A, parent::class]): array { return $names; } }
trait Promotion { public function __construct(public array $names = [parent::A, parent::class]) {} }
class PropertyConsumer { use Properties; }
class MethodConsumer { use Methods; }
class PromotedConsumer { use Promotion; }
echo 'declared|';
try { new PropertyConsumer(); } catch (Error $error) { echo $error->getMessage(), '|'; }
$method = new MethodConsumer(); echo json_encode($method->value([9])), '|';
try { $method->value(); } catch (Error $error) { echo $error->getMessage(), '|'; }
$promoted = new PromotedConsumer([9]); echo json_encode($promoted->names), '|';
try { new PromotedConsumer(); } catch (Error $error) { echo $error->getMessage(); }
"#);
    let message = "Cannot access \"parent\" when current class scope has no parent";
    assert_eq!(out, format!("declared|{message}|[9]|{message}|[9]|{message}"));
}

/// A real parent binds every member of the same promoted and ordinary method defaults.
#[test]
fn test_property_default_third_review_bound_parent_arrays() {
    let out = compile_and_run(r#"<?php
class Base { public const A = 1; }
class Consumer extends Base {
    public function __construct(public array $promoted = [parent::A, parent::class]) {}
    public function value(array $names = [parent::A, parent::class]): array { return $names; }
}
$value = new Consumer();
echo json_encode($value->promoted), '|', json_encode($value->value());
"#);
    assert_eq!(out, "[1,\"Base\"]|[1,\"Base\"]");
}

/// Trait declaration order determines the first instance-default error instead of error wording.
#[test]
fn test_property_default_second_review_trait_error_order() {
    for (traits, diagnostic) in [
        ("Names, Numbers", "Cannot use"), ("Numbers, Names", "Cannot access"),
    ] {
        let out = crate::support::compile_and_run(&format!(r#"<?php
trait Names {{ public string $name = parent::class; }}
trait Numbers {{ public int $number = parent::A; }}
class Probe {{ use {traits}; }}
try {{ new Probe(); }} catch (Error $error) {{ echo $error->getMessage(); }}
"#));
        assert_eq!(out, format!("{diagnostic} \"parent\" when current class scope has no parent"));
    }
}

/// Instance-default errors precede static-default errors for both construction and static access.
#[test]
fn test_property_default_second_review_instance_before_static() {
    let out = crate::support::compile_and_run(r#"<?php
trait Names { public string $name = parent::class; }
class Probe { use Names; public static int $number = parent::A; }
try { new Probe(); } catch (Error $error) { echo $error->getMessage(), '|'; }
try { echo Probe::$number; } catch (Error $error) { echo $error->getMessage(), '|'; }
"#);
    assert_eq!(out, "Cannot use \"parent\" when current class scope has no parent|Cannot use \"parent\" when current class scope has no parent|");
}

/// Class initialization prioritizes missing parent constants without changing per-slot reflection.
#[test]
fn test_property_default_oct9_mixed_parent_errors() {
    let out = crate::support::compile_and_run(r#"<?php
trait Names { public string $name = parent::class; }
class Probe { use Names; public int $number = parent::A; }
try { new Probe(); } catch (Error $e) { echo $e->getMessage(), '|'; }
try { echo (new ReflectionProperty(Probe::class, 'name'))->getDefaultValue(); }
catch (Error $e) { echo $e->getMessage(), '|'; }
try { echo (new ReflectionProperty(Probe::class, 'number'))->getDefaultValue(); }
catch (Error $e) { echo $e->getMessage(), '|'; }
"#);
    assert_eq!(out, "Cannot access \"parent\" when current class scope has no parent|Cannot use \"parent\" when current class scope has no parent|Cannot access \"parent\" when current class scope has no parent|");
}

/// Instantiated generic constant receivers survive method and reflected parameter defaults.
#[test]
fn test_property_default_generic_receiver_keeps_type_arguments() {
    let out = crate::support::compile_and_run(r#"<?php
class Value<T> { public const int DEFAULT_VALUE = 7; }
function useDefault(int $value = Value<int>::DEFAULT_VALUE): int { return $value; }
class Holder {
    public function number(int $value = Value<int>::DEFAULT_VALUE): int { return $value; }
}
$parameter = new ReflectionParameter(['Holder', 'number'], 'value');
echo useDefault(), ':', (new Holder())->number(), ':', $parameter->getDefaultValue();
"#);
    assert_eq!(out, "7:7:7");
}

/// Deferred method and promoted defaults remain available with their source constant name.
#[test]
fn test_property_default_followup_reflection_parameter_deferred_constant() {
    let out = crate::support::compile_and_run(r#"<?php
class Consumer {
    public function value(int $value = parent::A): int { return $value; }
    public function __construct(public int $promoted = parent::A) {}
}
$value = new ReflectionParameter([Consumer::class, 'value'], 'value');
echo $value->isDefaultValueAvailable() ? 'available' : 'missing';
echo ':', $value->isDefaultValueConstant() ? 'constant' : 'literal';
echo ':', $value->getDefaultValueConstantName();
try { $value->getDefaultValue(); } catch (Error $error) { echo ':', $error->getMessage(); }
echo '|';
$promoted = new ReflectionParameter([Consumer::class, '__construct'], 'promoted');
echo $promoted->isPromoted() ? 'promoted' : 'bad';
echo ':', $promoted->isDefaultValueAvailable() ? 'available' : 'missing';
echo ':', $promoted->isDefaultValueConstant() ? 'constant' : 'literal';
echo ':', $promoted->getDefaultValueConstantName();
try { $promoted->getDefaultValue(); } catch (Error $error) { echo ':', $error->getMessage(); }
unset($value, $promoted);
"#);
    assert_eq!(out, "available:constant:parent::A:Cannot access \"parent\" when current class scope has no parent|promoted:available:constant:parent::A:Cannot access \"parent\" when current class scope has no parent");
}

/// Trait signatures with unresolved parent names are reflected without aborting metadata emission.
#[test]
fn test_property_default_followup_reflection_trait_parameter() {
    let out = crate::support::compile_and_run(r#"<?php
trait Values {
    public function labeled(string $value = parent::class): string { return $value; }
    public function other(): string { return 'ok'; }
}

class TraitConsumer { use Values; }
$method = new ReflectionMethod(TraitConsumer::class, 'labeled');
$parameters = $method->getParameters();
$parameter = $parameters[0];
echo $parameter->isDefaultValueAvailable() ? 'available' : 'missing';
echo ':', $parameter->isDefaultValueConstant() ? 'constant' : 'literal';
try { $parameter->getDefaultValue(); } catch (Error $error) { echo ':', $error->getMessage(); }
echo '|';
$direct = new ReflectionParameter([TraitConsumer::class, 'labeled'], 'value');
echo $direct->isDefaultValueAvailable() ? 'available' : 'missing';
echo ':', (new ReflectionMethod(TraitConsumer::class, 'other'))->getName();
echo ':', (new ReflectionClass(TraitConsumer::class))->getName();
$trait = new ReflectionParameter([Values::class, 'labeled'], 'value');
echo ':', $trait->isDefaultValueAvailable() ? 'available' : 'missing';
try { $trait->getDefaultValue(); } catch (Error $error) { echo ':', $error->getMessage(); }
unset($method, $parameters, $parameter, $direct, $trait);
"#);
    assert_eq!(out, "available:literal:Cannot use \"parent\" when current class scope has no parent|available:other:TraitConsumer:available:Cannot use \"parent\" when current class scope has no parent");
}

/// Repeated deferred-default Errors add no retained owners to the known reflection metadata graph.
#[test]
fn test_property_default_followup_repeated_reflection_errors_are_bounded() {
    let retained = |count: usize| {
        let source = format!(r#"<?php
class Consumer {{ public function value(int $value = parent::A): int {{ return $value; }} }}
$parameter = new ReflectionParameter([Consumer::class, 'value'], 'value');
for ($i = 0; $i < {count}; $i++) {{
    try {{ $parameter->getDefaultValue(); echo 'bad'; }} catch (Error $error) {{}}
}}
unset($parameter, $error);
echo 'done';
"#);
        let output = compile_and_run_with_heap_debug(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "done");
        let summary = output.stderr.lines().find(|line| line.starts_with("HEAP DEBUG: allocs="))
            .expect("heap totals");
        summary.split_once("live_blocks=").unwrap().1
            .split_once(" peak_live_bytes=").unwrap().0.to_string()
    };
    assert_eq!(retained(1), retained(40));
}

/// Runs each regression with native ownership diagnostics enabled.
fn compile_and_run(source: &str) -> String {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "stdout: {}\n{}",
        output.stdout, output.stderr.lines().take(8).collect::<Vec<_>>().join("\n"));
    output.stdout
}

/// Direct and nested property defaults bind self and parent before literal materialization.
#[test]
fn test_property_default_review_lexical_class_names() {
    let out = compile_and_run(r#"<?php
namespace Defaults;
class Base {}
class Leaf extends Base {
    public string $name = self::class;
    public static string $owner = parent::class;
    public array $names = [self::class, parent::class];
}
class Descendant extends Leaf {}
$leaf = new Leaf();
$descendant = new Descendant();
echo $leaf->name, '|', $descendant->name, '|', Leaf::$owner, '|';
echo implode(',', $leaf->names);
"#);
    assert_eq!(out, "Defaults\\Leaf|Defaults\\Leaf|Defaults\\Base|Defaults\\Leaf,Defaults\\Base");
}

/// Promoted class-name defaults are bound to the constructor declaration, not its caller.
#[test]
fn test_property_default_review_promoted_class_names() {
    let out = compile_and_run(r#"<?php
class Base {}
class Leaf extends Base {
    public function __construct(public string $name = self::class, public string $owner = parent::class) {}
}
$leaf = new Leaf();
echo $leaf->name, '|', $leaf->owner;
"#);
    assert_eq!(out, "Leaf|Base");
}

/// A valid trait consumer supplies its own class and parent for both ordinary and promoted defaults.
#[test]
fn test_property_default_review_trait_consumer_class_names() {
    let out = compile_and_run(r#"<?php
trait Values {
    public string $name = self::class;
    public static string $owner = parent::class;
    public function __construct(public string $promoted = parent::class) {}
}
class Base {}
class Consumer extends Base { use Values; }
$consumer = new Consumer();
echo $consumer->name, '|', Consumer::$owner, '|', $consumer->promoted;
"#);
    assert_eq!(out, "Consumer|Base|Base");
}

/// Importing an unresolved trait parent default is legal until its value is needed.
#[test]
fn test_property_default_review_trait_without_parent_declaration() {
    let out = compile_and_run(r#"<?php
trait InstanceValue { public string $name = parent::class; }
trait StaticValue { public static string $name = parent::class; }
trait PromotedValue { public function __construct(public string $name = parent::class) {} }
class InstanceConsumer { use InstanceValue; }
class StaticConsumer { use StaticValue; }
class PromotedConsumer { use PromotedValue; }
echo 'declared';
"#);
    assert_eq!(out, "declared");
}

/// Missing trait consumer parents throw a catchable Error at allocation or static-value access.
#[test]
fn test_property_default_review_trait_without_parent_use() {
    let out = compile_and_run(r#"<?php
trait InstanceValue { public string $name = parent::class; }
trait StaticValue { public static string $name = parent::class; }
trait PromotedValue { public function __construct(public string $name = parent::class) {} }
class InstanceConsumer { use InstanceValue; }
class StaticConsumer { use StaticValue; }
class PromotedConsumer { use PromotedValue; }
try { new InstanceConsumer(); } catch (Error $e) { echo $e->getMessage(), '|'; }
try { echo StaticConsumer::$name; } catch (Error $e) { echo $e->getMessage(), '|'; }
try { new PromotedConsumer(); } catch (Error $e) { echo $e->getMessage(); }
"#);
    let message = "Cannot use \"parent\" when current class scope has no parent";
    assert_eq!(out, format!("{message}|{message}|{message}"));
}

/// An explicit promoted argument bypasses only its lazy default, including dynamic construction.
#[test]
fn test_property_default_review_promoted_explicit_argument() {
    let out = compile_and_run(r#"<?php
trait Values { public function __construct(public string $name = parent::class) {} }
class Consumer { use Values; }
$first = new Consumer('explicit');
$class = $argc > 100 ? 'Unknown' : Consumer::class;
$second = new $class('dynamic');
echo $first->name, '|', $second->name, '|';
try { new $class(); } catch (Error $e) { echo $e->getMessage(); }
"#);
    assert_eq!(out, "explicit|dynamic|Cannot use \"parent\" when current class scope has no parent");
}

/// A fixed-class lazy default preserves previously allocated objects when caught.
#[test]
fn test_property_default_review_promoted_fixed_catch_ownership() {
    let out = compile_and_run(r#"<?php
trait Values { public function __construct(public string $name = parent::class) {} }
class Consumer { use Values; }
$first = new Consumer('explicit');
try { new Consumer(); } catch (Error $e) { echo $e->getMessage(), '|'; }
echo $first->name;
"#);
    assert_eq!(out, "Cannot use \"parent\" when current class scope has no parent|explicit");
}

/// A dynamically selected promoted class accepts an explicit value without touching its default.
#[test]
fn test_property_default_review_promoted_dynamic_explicit() {
    let out = compile_and_run(r#"<?php
trait Values { public function __construct(public string $name = parent::class) {} }
class Consumer { use Values; }
$class = $argc > 100 ? 'Unknown' : Consumer::class;
$value = new $class('explicit');
echo $value->name;
"#);
    assert_eq!(out, "explicit");
}

/// Class initialization errors remain catchable when a constructor has ordinary parameters.
#[test]
fn test_property_default_review_constructor_arguments_are_not_evaluated() {
    let out = compile_and_run(r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer { use Values; public function __construct(string $value) {} }
function argument(): string { echo 'argument'; return 'value'; }
try { new Consumer(argument()); } catch (Error $e) { echo 'caught'; }
"#);
    assert_eq!(out, "caught");
}

/// Boxed runtime class names reject invalid defaults before evaluating constructor arguments.
#[test]
fn test_property_default_review_boxed_dynamic_initialization() {
    let out = compile_and_run(r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer { use Values; public function __construct(string $value) {} }
function argument(): string { echo 'wrong'; return 'value'; }
$names = ['class' => Consumer::class, 'count' => 1];
$name = $names['class'];
try { new $name(argument()); } catch (Error $e) { echo 'caught'; }
"#);
    assert_eq!(out, "caught");
}

/// One unresolved ordinary default invalidates every initialization boundary, but not metadata.
#[test]
fn test_property_default_review_class_initialization_boundaries() {
    let out = compile_and_run(r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer {
    use Values;
    public static string $valid = 'ok';
    public const NAME = 'constant';
    public function __construct(string $argument = 'value') {}
    public static function method(): string { return 'method'; }
}
function argument(): string { echo 'arg'; return 'value'; }
function value(): string { echo 'rhs:'; return 'value'; }
echo Consumer::class, '|', Consumer::NAME, '|', Consumer::method(), '|';
try { new Consumer(argument()); } catch (Error $e) { echo 'new|'; }
try { echo Consumer::$valid; } catch (Error $e) { echo 'read|'; }
try { Consumer::$valid = value(); } catch (Error $e) { echo 'write|'; }
try { echo isset(Consumer::$valid); } catch (Error $e) { echo 'isset|'; }
try { echo empty(Consumer::$valid); } catch (Error $e) { echo 'empty|'; }
$class = $argc > 100 ? 'Unknown' : Consumer::class;
try { new $class(argument()); } catch (Error $e) { echo 'dynamic'; }
"#);
    assert_eq!(out, "Consumer|constant|method|new|read|rhs:write|isset|empty|dynamic");
}

/// Reflection can describe an invalid consumer, but materializing its defaults raises Error.
#[test]
fn test_property_default_review_reflection_initialization_boundaries() {
    let out = crate::support::compile_and_run(r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer { use Values; public static string $valid = 'ok'; }
$metadata = new ReflectionClass(Consumer::class);
echo $metadata->getName(), '|';
try { new Consumer(); } catch (Error $e) { echo 'new|'; }
try { $metadata->getDefaultProperties(); } catch (Error $e) { echo 'defaults|'; }
try { $metadata->getStaticProperties(); } catch (Error $e) { echo 'statics|'; }
try { $metadata->newInstanceWithoutConstructor(); } catch (Error $e) { echo 'reflection'; }
"#);
    assert_eq!(out, "Consumer|new|defaults|statics|reflection");
}

/// Ordinary missing-parent constants remain lazy and use the access diagnostic.
#[test]
fn test_property_default_review_missing_parent_constant_properties() {
    let out = compile_and_run(r#"<?php
class InstanceValue { public int $value = parent::A; }
class StaticValue { public static int $value = parent::A; }
class ArrayValue { public array $value = ['nested' => parent::A]; }
trait Values { public int $value = parent::A; }
class Consumer { use Values; }
echo 'declared|';
try { new InstanceValue(); } catch (Error $e) { echo $e->getMessage(), '|'; }
try { echo StaticValue::$value; } catch (Error $e) { echo $e->getMessage(), '|'; }
try { new Consumer(); } catch (Error $e) { echo $e->getMessage(); }
try { new ArrayValue(); } catch (Error $e) { echo '|', $e->getMessage(); }
"#);
    let message = "Cannot access \"parent\" when current class scope has no parent";
    assert_eq!(out, format!("declared|{message}|{message}|{message}|{message}"));
}

/// Supplying promoted arguments bypasses unresolved ordinary parent constants.
#[test]
fn test_property_default_review_missing_parent_constant_promotion() {
    let out = compile_and_run(r#"<?php
class Consumer { public function __construct(public int $value = parent::A) {} }
$first = new Consumer(9);
echo $first->value, '|';
try { new Consumer(); } catch (Error $e) { echo $e->getMessage(); }
"#);
    assert_eq!(out, "9|Cannot access \"parent\" when current class scope has no parent");
}

/// Ordinary method arguments use the same lazy access error as promoted arguments.
#[test]
fn test_property_default_review_missing_parent_constant_method() {
    let out = compile_and_run(r#"<?php
class Consumer { public function value(int $value = parent::A): int { return $value; } }
$consumer = new Consumer();
echo $consumer->value(9), '|';
try { $consumer->value(); } catch (Error $e) { echo $e->getMessage(); }
"#);
    assert_eq!(out, "9|Cannot access \"parent\" when current class scope has no parent");
}

/// Imported ordinary method class-name defaults remain lazy, unlike direct class declarations.
#[test]
fn test_property_default_review_trait_method_class_default() {
    let out = compile_and_run(r#"<?php
trait Values { public function value(string $value = parent::class): string { return $value; } }
class Consumer { use Values; }
$consumer = new Consumer();
echo $consumer->value('explicit'), '|';
try { $consumer->value(); } catch (Error $e) { echo $e->getMessage(); }
"#);
    assert_eq!(out, "explicit|Cannot use \"parent\" when current class scope has no parent");
}

/// Property reflection keeps deferred defaults and errors local to the requested slot.
#[test]
fn test_property_default_review_reflection_preserves_deferred_defaults() {
    let out = crate::support::compile_and_run(r#"<?php
trait Values { public string $bad = parent::class; public static array $names = [parent::class]; }
class Consumer { use Values; public string $valid = 'ok'; }
$bad = new ReflectionProperty(Consumer::class, 'bad');
echo $bad->hasDefaultValue() ? 'default|' : 'missing|';
try { echo $bad->getDefaultValue(); } catch (Error $e) { echo $e->getMessage(), '|'; }
$names = (new ReflectionClass(Consumer::class))->getProperty('names');
echo $names->hasDefaultValue() ? 'default|' : 'missing|';
try { $names->getDefaultValue(); } catch (Error $e) { echo $e->getMessage(), '|'; }
echo (new ReflectionProperty(Consumer::class, 'valid'))->getDefaultValue();
"#);
    let message = "Cannot use \"parent\" when current class scope has no parent";
    assert_eq!(out, format!("default|{message}|default|{message}|ok"));
}

/// Valid inline reflectors survive metadata guards even when another class has invalid defaults.
#[test]
fn test_property_default_review_reflection_guard_borrows_receiver() {
    let out = crate::support::compile_and_run(r#"<?php
trait Invalid { public string $bad = parent::class; }
class Consumer { use Invalid; }
class Valid { public string $value = 'ok'; }
echo Consumer::class, '|';
$defaults = (new ReflectionClass(Valid::class))->getDefaultProperties();
echo $defaults['value'], '|';
$value = (new ReflectionClass(Valid::class))->newInstanceWithoutConstructor();
echo $value->value;
"#);
    assert_eq!(out, "Consumer|ok|ok");
}

/// Dynamic eval reflection sees the same native deferred default metadata as AOT callers.
#[test]
fn test_property_default_review_native_eval_reflection_defaults() {
    let out = crate::support::compile_and_run(r#"<?php
trait Invalid { public string $bad = parent::class; }
class Consumer { use Invalid; public string $valid = 'ok'; }
class ConstantValue { public int $bad = parent::A; }
$source = 'foreach (["Consumer", "ConstantValue"] as $class) {'
    . '$property = new ReflectionProperty($class, "bad");'
    . 'echo $property->hasDefaultValue() ? "default|" : "missing|";'
    . 'try { $property->getDefaultValue(); } catch (Error $e) { echo $e->getMessage(), "|"; }}'
    . 'echo (new ReflectionProperty("Consumer", "valid"))->getDefaultValue();';
eval($source);
"#);
    assert_eq!(out, "default|Cannot use \"parent\" when current class scope has no parent|default|Cannot access \"parent\" when current class scope has no parent|ok");
}

/// Runtime class allocation reports a default error before creating or destroying the object.
#[test]
fn test_property_default_review_late_static_initialization() {
    let out = compile_and_run(r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer {
    use Values;
    public static function create(): object { return new static(); }
    public function __destruct() { echo 'wrong'; }
}
try { Consumer::create(); } catch (Error $e) { echo 'caught'; }
"#);
    assert_eq!(out, "caught");
}

/// Invalid class defaults still evaluate array-write operands in PHP source order.
#[test]
fn test_property_default_review_static_array_write_order() {
    let out = compile_and_run(r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer { use Values; public static array $items = [1]; }
function value(): int { echo 'v'; return 2; }
function index(): int { echo 'k'; return 0; }
try { Consumer::$items[] = value(); } catch (Error $e) { echo 'E|'; }
try { Consumer::$items[index()] = value(); } catch (Error $e) { echo 'E'; }
"#);
    assert_eq!(out, "vE|kvE");
}
