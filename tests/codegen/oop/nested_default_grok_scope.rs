//! Purpose:
//! Covers declaration scope in nested object defaults and free-function defaults.
//!
//! Called from:
//! - The OOP codegen integration suite.
//!
//! Key details:
//! - Constructor names never replace the lexical scope of their arguments.
//! - Free-function defaults cannot borrow the caller's class scope.

use crate::support::{compile_and_run, compile_and_run_with_heap_debug, without_ir_opt};

/// Checks the observable value and retirement of every native owner.
fn check(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Checks reflection semantics separately from its general metadata-graph ownership coverage.
fn check_reflection(source: &str, expected: &str) {
    assert_eq!(compile_and_run(source), expected);
}

/// Nested self factories bind to the declaring class or trait consumer across caller scopes.
#[test]
fn test_nested_default_grok_third_review_constructor_scope() {
    check(r#"<?php
class Wrapper { public function __construct(public object $inner) {} }
class Declaring {
    public function make(Wrapper $value = new Wrapper(new self)): Wrapper { return $value; }
    public function run(): void { echo get_class($this->make()->inner), '|'; }
}
class Child extends Declaring {}
trait Factory {
    public function make(Wrapper $value = new Wrapper(new self)): Wrapper { return $value; }
}
class Consumer { use Factory; }
$declaring = new Declaring();
$declaring->run();
echo get_class($declaring->make()->inner), '|';
echo get_class((new Child())->make()->inner), '|';
echo get_class((new Consumer())->make()->inner);
unset($declaring);
"#, "Declaring|Declaring|Declaring|Consumer");
}

/// Nested constructor scope does not depend on the EIR optimizer.
#[test]
fn test_nested_default_grok_third_review_constructor_scope_no_ir_opt() {
    without_ir_opt(test_nested_default_grok_third_review_constructor_scope);
}

/// Constants inside constructor arguments use the surrounding declaration's scope.
#[test]
fn test_nested_default_grok_third_review_constructor_constant() {
    check(r#"<?php
class Wrapper { public function __construct(public string $name) {} }
class Declaring {
    public const LABEL = 'declaring';
    public function make(Wrapper $value = new Wrapper(self::LABEL)): Wrapper { return $value; }
}
echo (new Declaring())->make()->name;
"#, "declaring");
}

/// Nested constant arguments retain lexical scope without EIR optimization.
#[test]
fn test_nested_default_grok_third_review_constructor_constant_no_ir_opt() {
    without_ir_opt(test_nested_default_grok_third_review_constructor_constant);
}

/// Calling a free function from a class never supplies scope to its relative default.
#[test]
fn test_nested_default_grok_third_review_free_function_scope() {
    check(r#"<?php
function make(object $value = new self): string { return get_class($value); }
function makeParent(object $value = new parent): string { return get_class($value); }
class Base {}
class Caller extends Base {
    public function run(): void {
        echo make(new stdClass()), '|';
        try { make(); } catch (Error $error) { echo $error->getMessage(), '|'; unset($error); }
        echo makeParent(new stdClass()), '|';
        try { makeParent(); } catch (Error $error) { echo $error->getMessage(); unset($error); }
    }
}
(new Caller())->run();
"#, "stdClass|Cannot access \"self\" when no class scope is active|stdClass|Cannot access \"parent\" when no class scope is active");
}

/// Free-function default deferral keeps the same behavior without EIR optimization.
#[test]
fn test_nested_default_grok_third_review_free_function_scope_no_ir_opt() {
    without_ir_opt(test_nested_default_grok_third_review_free_function_scope);
}

/// Descriptor calls preserve the no-class scope of free-function object defaults.
#[test]
fn test_nested_default_grok_third_review_free_function_callbacks() {
    check(r#"<?php
function make(object $value = new self): string { return get_class($value); }
function untyped($value = new parent): string { return get_class($value); }
$factory = make(...);
$parent = untyped(...);
echo $factory(new stdClass()), '|';
try { $factory(); } catch (Error $error) { echo $error->getMessage(), '|'; unset($error); }
echo $parent(new stdClass()), '|';
try { $parent(); } catch (Error $error) { echo $error->getMessage(); unset($error); }
unset($factory, $parent);
"#, "stdClass|Cannot access \"self\" when no class scope is active|stdClass|Cannot access \"parent\" when no class scope is active");
}

/// Free-function descriptor defaults remain lazy without EIR optimization.
#[test]
fn test_nested_default_grok_third_review_free_function_callbacks_no_ir_opt() {
    without_ir_opt(test_nested_default_grok_third_review_free_function_callbacks);
}

/// Opaque eval observes the same lazy no-class Error when invoking a native free function.
#[test]
fn test_nested_default_grok_third_review_free_function_eval() {
    check(r#"<?php
function make(object $value = new self): string { return get_class($value); }
function run(string $source): mixed { return eval($source); }
echo run('return make(new stdClass());'), '|';
echo run('try { return make(); } catch (Error $error) { return $error->getMessage(); }');
"#, "stdClass|Cannot access \"self\" when no class scope is active");
}

/// Native free-function default transport is independent of EIR optimization.
#[test]
fn test_nested_default_grok_third_review_free_function_eval_no_ir_opt() {
    without_ir_opt(test_nested_default_grok_third_review_free_function_eval);
}

/// An unbound object factory raises before PHP checks a scalar parameter's type.
#[test]
fn test_nested_default_grok_third_review_scalar_unbound_factory() {
    check(r#"<?php
function make(int $value = new self): int { return $value; }
echo make(1), '|';
try { make(); } catch (Error $error) { echo $error->getMessage(); unset($error); }
"#, "1|Cannot access \"self\" when no class scope is active");
}

/// Lazy scalar-parameter scope failures remain independent of EIR optimization.
#[test]
fn test_nested_default_grok_third_review_scalar_unbound_factory_no_ir_opt() {
    without_ir_opt(test_nested_default_grok_third_review_scalar_unbound_factory);
}

/// Native descriptors evaluate nested free-function factories in their empty class scope.
#[test]
fn test_nested_default_grok_fourth_review_free_object_callback() {
    check(r#"<?php
class Wrapper { public function __construct(public object $inner) { echo 'WRONG'; } }
function make(object $value = new Wrapper(new self)): object { return $value; }
function other(): object { return new stdClass(); }
$factories = [make(...), other(...)];
$factory = $factories[$argc - 1];
try { $factory(); } catch (Error $error) { echo $error->getMessage(); unset($error); }
unset($factory, $factories);
"#, "Cannot access \"self\" when no class scope is active");
}

/// String callback binding raises a catchable Error for a nested relative constant default.
#[test]
fn test_nested_default_grok_fourth_review_free_array_callback() {
    check(r#"<?php
function make($value = [self::A]): mixed { return $value; }
$names = ['make', 'strlen'];
$name = $names[$argc - 1];
try { call_user_func($name); } catch (Error $error) { echo $error->getMessage(); unset($error); }
unset($name, $names);
"#, "Cannot access \"self\" when no class scope is active");
}

/// AOT free-function reflection keeps constant spelling independently of deferred evaluation.
#[test]
fn test_nested_default_grok_fourth_review_free_reflection_constant() {
    check_reflection(r#"<?php
function first($value = self::A): mixed { return $value; }
function second($value = parent::A): mixed { return $value; }
$function = new ReflectionFunction('first');
$parameters = $function->getParameters();
$parameter = $parameters[0];
echo (int)$parameter->isDefaultValueAvailable(), ':', (int)$parameter->isDefaultValueConstant(),
    ':', $parameter->getDefaultValueConstantName(), '|';
try { $parameter->getDefaultValue(); } catch (Error $error) { echo $error->getMessage(), '|'; unset($error); }
$other = new ReflectionParameter('second', 0);
echo (int)$other->isDefaultValueConstant(), ':', $other->getDefaultValueConstantName();
unset($function, $parameter, $parameters, $other);
"#, "1:1:self::A|Cannot access \"self\" when no class scope is active|1:parent::A");
}

/// Eval transports nested free-function defaults as owned data rather than fatal invoker fallbacks.
#[test]
fn test_nested_default_grok_fourth_review_free_nested_eval() {
    check(r#"<?php
class Wrapper { public function __construct(public object $inner) { echo 'WRONG'; } }
function first($value = [self::A]): mixed { return $value; }
function second(object $value = new Wrapper(new parent)): object { return $value; }
function run(string $source): mixed { return eval($source); }
echo run('try { first(); } catch (Error $error) { return $error->getMessage(); }'), '|';
echo run('try { second(); } catch (Error $error) { return $error->getMessage(); }');
"#, "Cannot access \"self\" when no class scope is active|Cannot access \"parent\" when no class scope is active");
}

/// Eval ReflectionFunction sees native root and nested defaults without executing them eagerly.
#[test]
fn test_nested_default_grok_fourth_review_free_eval_reflection() {
    check_reflection(r#"<?php
function first(object $value = new self): object { return $value; }
function second($value = [parent::A]): mixed { return $value; }
function run(string $source): mixed { return eval($source); }
echo run('$r = new ReflectionFunction("first"); $p = $r->getParameters()[0]; echo (int)$p->isDefaultValueAvailable(), ":"; try { $p->getDefaultValue(); } catch (Error $e) { return $e->getMessage(); }'), '|';
echo run('$r = new ReflectionFunction("second"); $p = $r->getParameters()[0]; echo (int)$p->isDefaultValueAvailable(), ":"; try { $p->getDefaultValue(); } catch (Error $e) { return $e->getMessage(); }');
"#, "1:Cannot access \"self\" when no class scope is active|1:Cannot access \"parent\" when no class scope is active");
}

/// Eval reflection retains the same source constant names as native free-function reflection.
#[test]
fn test_nested_default_grok_fourth_review_free_eval_constant_names() {
    check_reflection(r#"<?php
function first($value = self::A): mixed { return $value; }
function second($value = parent::A): mixed { return $value; }
function run(string $source): mixed { return eval($source); }
echo run('$p = new ReflectionParameter("first", 0); echo (int)$p->isDefaultValueConstant(), ":", $p->getDefaultValueConstantName(), "|"; $p = new ReflectionParameter("second", 0); echo (int)$p->isDefaultValueConstant(), ":", $p->getDefaultValueConstantName();');
"#, "1:self::A|1:parent::A");
}

/// Free-function deferred defaults and reflection metadata agree without EIR optimization.
#[test]
fn test_nested_default_grok_fourth_review_free_without_ir_opt() {
    without_ir_opt(|| {
        test_nested_default_grok_fourth_review_free_object_callback();
        test_nested_default_grok_fourth_review_free_array_callback();
        test_nested_default_grok_fourth_review_free_reflection_constant();
        test_nested_default_grok_fourth_review_free_nested_eval();
        test_nested_default_grok_fourth_review_free_eval_reflection();
        test_nested_default_grok_fourth_review_free_eval_constant_names();
    });
}

/// Property reflection keeps a class-constant-selected default tree available and lazy.
#[test]
fn test_nested_default_grok_fourth_review_property_conditional_reflection() {
    check_reflection(r#"<?php
class Probe { const ENABLED = true; public array $items = [self::ENABLED ? 'ok' : parent::A]; }
$property = new ReflectionProperty('Probe', 'items');
echo (int)$property->hasDefaultValue(), ':', json_encode($property->getDefaultValue()), '|';
$class = new ReflectionClass('Probe');
echo json_encode($class->getDefaultProperties());
unset($property, $class);
"#, "1:[\"ok\"]|{\"items\":[\"ok\"]}");
}

/// Possible deferred property-default errors preserve the surrounding construction catch.
#[test]
fn test_nested_default_grok_fourth_review_property_conditional_catch() {
    check(r#"<?php
class Probe { const ENABLED = false; public array $items = [self::ENABLED ? 'ok' : parent::A]; }
try { new Probe(); } catch (Error $error) { echo $error->getMessage(); unset($error); }
"#, "Cannot access \"parent\" when current class scope has no parent");
}

/// Class and property reflection defer the selected Error until default materialization.
#[test]
fn test_nested_default_grok_fourth_review_property_conditional_reflection_error() {
    check_reflection(r#"<?php
class Probe { const ENABLED = false; public array $items = [self::ENABLED ? 'ok' : parent::A]; }
$property = new ReflectionProperty('Probe', 'items');
echo (int)$property->hasDefaultValue(), '|';
try { $property->getDefaultValue(); } catch (Error $error) { echo $error->getMessage(), '|'; }
$class = new ReflectionClass('Probe');
echo 'created|';
try { $class->getDefaultProperties(); } catch (Error $error) { echo $error->getMessage(); }
"#, "1|Cannot access \"parent\" when current class scope has no parent|created|Cannot access \"parent\" when current class scope has no parent");
}

/// Conditional property defaults retain reflection and catch behavior without EIR optimization.
#[test]
fn test_nested_default_grok_fourth_review_property_without_ir_opt() {
    without_ir_opt(|| {
        test_nested_default_grok_fourth_review_property_conditional_reflection();
        test_nested_default_grok_fourth_review_property_conditional_catch();
        test_nested_default_grok_fourth_review_property_conditional_reflection_error();
    });
}

/// Reflection preserves integer arithmetic results and distinguishes genuinely floating defaults.
#[test]
fn test_nested_default_greptile_integer_arithmetic_reflection() {
    check_reflection(r#"<?php
class Probe { const N = 6; public array $items = [self::N / 2, self::N ** 2, self::N / 4, 2 ** -2, 6.0 / 2, 2.0 ** 2]; }
$property = new ReflectionProperty('Probe', 'items');
$values = $property->getDefaultValue();
echo (int)($values === [3, 36, 1.5, 0.25, 3.0, 4.0]), '|';
$class = new ReflectionClass('Probe');
$defaults = $class->getDefaultProperties();
echo (int)($defaults['items'] === [3, 36, 1.5, 0.25, 3.0, 4.0]);
"#, "1|1");
}

/// Reflection arithmetic keeps the same exact PHP types without EIR optimization.
#[test]
fn test_nested_default_greptile_integer_arithmetic_reflection_no_ir_opt() {
    without_ir_opt(test_nested_default_greptile_integer_arithmetic_reflection);
}

/// Shared numeric default metadata agrees across runtime-selected callbacks and opaque eval.
#[test]
fn test_nested_default_greptile_integer_arithmetic_callable_eval() {
    check(r#"<?php
class Numbers { const N = 6; }
function values(array $items = [Numbers::N / 2, Numbers::N ** 2]): array { return $items; }
function other(array $items = []): array { return $items; }
$callbacks = [values(...), other(...)]; $callback = $callbacks[$argc - 1];
echo (int)($callback() === [3, 36]), '|';
function run(string $source): mixed { return eval($source); }
echo (int)run('return values() === [3, 36];');
unset($callback, $callbacks);
"#, "1|1");
}

/// Callback and eval arithmetic metadata preserve PHP types without EIR optimization.
#[test]
fn test_nested_default_greptile_integer_arithmetic_callable_eval_no_ir_opt() {
    without_ir_opt(test_nested_default_greptile_integer_arithmetic_callable_eval);
}

/// Integer-preserving division may feed MIN_INT to remainder without panicking the compiler.
#[test]
fn test_nested_default_greptile_remainder_overflow() {
    check(r#"<?php
class Numbers { const MIN = -9223372036854775807 - 1; }
function values(array $items = [(Numbers::MIN / 1) % -1]): array { return $items; }
function other(array $items = []): array { return $items; }
$callbacks = [values(...), other(...)]; $callback = $callbacks[$argc - 1];
echo (int)($callback() === [0]), '|';
function run(string $source): mixed { return eval($source); }
echo (int)run('return values() === [0];');
unset($callback, $callbacks);
"#, "1|1");
}

/// Safe default remainder folding is independent of EIR optimization.
#[test]
fn test_nested_default_greptile_remainder_overflow_no_ir_opt() {
    without_ir_opt(test_nested_default_greptile_remainder_overflow);
}

/// Property defaults apply int-to-float coercion without changing parameter or array value types.
#[test]
fn test_nested_default_grok_float_property_metadata() {
    check_reflection(r#"<?php
class Numbers { const N = 6; }
class Probe {
    public float $div = Numbers::N / 2;
    public ?float $pow = Numbers::N ** 2;
    public float $bare = Numbers::N;
    public float $literal = 3;
    public float|int $exact = Numbers::N;
    public float|string $coerced = Numbers::N;
    public static ?float $nullable = Numbers::N;
}
echo (int)((new ReflectionProperty('Probe', 'div'))->getDefaultValue() === 3.0),
    (int)((new ReflectionProperty('Probe', 'pow'))->getDefaultValue() === 36.0),
    (int)((new ReflectionProperty('Probe', 'bare'))->getDefaultValue() === 6.0),
    (int)((new ReflectionProperty('Probe', 'literal'))->getDefaultValue() === 3.0),
    (int)((new ReflectionProperty('Probe', 'exact'))->getDefaultValue() === 6),
    (int)((new ReflectionProperty('Probe', 'coerced'))->getDefaultValue() === 6.0),
    (int)((new ReflectionProperty('Probe', 'nullable'))->getDefaultValue() === 6.0), '|';
$defaults = (new ReflectionClass('Probe'))->getDefaultProperties();
echo (int)($defaults['div'] === 3.0), (int)($defaults['pow'] === 36.0),
    (int)($defaults['bare'] === 6.0), (int)($defaults['literal'] === 3.0),
    (int)($defaults['exact'] === 6), (int)($defaults['coerced'] === 6.0),
    (int)($defaults['nullable'] === 6.0), '|';
function value(float $number = Numbers::N): float { return $number; }
function expression(float $number = Numbers::N / 2): float { return $number; }
echo (int)((new ReflectionParameter('value', 0))->getDefaultValue() === 6),
    (int)((new ReflectionParameter('expression', 0))->getDefaultValue() === 3);
"#, "1111111|1111111|11");
}

/// Property-only numeric coercion is independent of EIR optimization.
#[test]
fn test_nested_default_grok_float_property_metadata_no_ir_opt() {
    without_ir_opt(test_nested_default_grok_float_property_metadata);
}

/// Numeric property defaults use the same PHP coercions in the initialized object and static slot.
#[test]
fn test_nested_default_grok_float_property_initialization() {
    check(r#"<?php
class Numbers { const N = 6; }
class Probe {
    public float $bare = Numbers::N;
    public ?float $nullable = Numbers::N;
    public float|string $coerced = Numbers::N;
    public float|int $exact = Numbers::N;
    public mixed $mixed = Numbers::N;
    public array $values = [Numbers::N];
    public static ?float $shared = Numbers::N / 2;
}
$probe = new Probe();
echo (int)($probe->bare === 6.0), (int)($probe->nullable === 6.0),
    (int)($probe->coerced === 6.0), (int)($probe->exact === 6),
    (int)($probe->mixed === 6), (int)($probe->values[0] === 6),
    (int)(Probe::$shared === 3.0);
unset($probe);
"#, "1111111");
}

/// Numeric initializer coercions are independent of EIR optimization.
#[test]
fn test_nested_default_grok_float_property_initialization_no_ir_opt() {
    without_ir_opt(test_nested_default_grok_float_property_initialization);
}
