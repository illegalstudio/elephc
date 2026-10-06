//! Purpose:
//! Covers lexical class names in property and promoted-constructor defaults.
//! Checks trait consumer binding and deferred errors for missing parent scopes.
//!
//! Called from:
//! - `cargo test --test codegen_tests property_default_review_`.
//!
//! Key details:
//! - Stored defaults must preserve declaration scope rather than caller or subclass scope.

use crate::support::compile_and_run_with_heap_debug;

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
