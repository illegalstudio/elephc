//! Purpose:
//! Checks real Zend classes connected to compiled methods and object state.
//!
//! Called from:
//! - The real PHP SDK integration test binary on each supported native host.
//!
//! Key details:
//! - Host code loads only the extension and does not require a PHP class wrapper.

use super::*;

/// Exposes a real class with a constructor, instance/static methods and host-visible state.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn phpext_classes_native_pippo_basic_host_roundtrip() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("class_pippo", r#"<?php
#[Export]
final class Pippo {
    public const ANSWER = 42;
    public int $count = 0;
    public string $name = 'initial';
    public array $items = [];
    private int $secret = 10;
    public function __construct(int $count = 3) { $this->count = $count; }
    public function add(int $amount): int { $this->count += $amount; return $this->count; }
    public function greeting(string $prefix): string { return $prefix . $this->name; }
    public function replace(array $items): array { $this->items = $items; return $this->items; }
    public function same(): self { return $this; }
    public function secret(): int { return $this->secret; }
    public static function twice(int $value): int { return $value * 2; }
}
#[Export] function pippo_identity(Pippo $value): Pippo { return $value; }
"#);
    let output = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", module.display())).args(["-r", r#"
$p = new Pippo(count: 7);
$p->name = 'PHP';
var_dump($p->add(5), $p->count, $p->greeting('hello '));
var_dump($p->replace(['x' => 4]), $p->items);
var_dump($p->same() === $p, pippo_identity($p) === $p, $p->secret(), Pippo::twice(9), Pippo::ANSWER);
$r = new ReflectionClass(Pippo::class);
var_dump($r->isInternal(), $r->isFinal(), $r->getExtensionName());
$q = $r->newInstanceWithoutConstructor();
var_dump($q->count, $q->add(2));
unset($p, $q);
gc_collect_cycles();
echo "classes ok\n";
"#]).output().unwrap();
    assert!(output.status.success(), "status {:?}\n{}\n{}", output.status,
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "int(12)\nint(12)\nstring(9) \"hello PHP\"\narray(1) {\n  [\"x\"]=>\n  int(4)\n}\narray(1) {\n  [\"x\"]=>\n  int(4)\n}\nbool(true)\nbool(true)\nint(10)\nint(18)\nint(42)\nbool(true)\nbool(true)\nstring(11) \"class_pippo\"\nint(0)\nint(2)\nclasses ok\n");
    project.assert_clean();
}

/// Preserves native owners and host identity while allowing Zend to collect cross-heap cycles.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn phpext_classes_native_identity_roots_and_cycles() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("class_graph", r#"<?php
#[Export] final class GraphNode {
    public string $name = 'node';
    public ?GraphNode $next = null;
    public array $items = [];
    private ?GraphNode $hidden = null;
    public function __construct(string $name = 'node') { $this->name = $name; }
    public function hide(?GraphNode $node): void { $this->hidden = $node; }
    public function reveal(): ?GraphNode { return $this->hidden; }
    public function replaceLinks(?GraphNode $next, ?GraphNode $hidden): void { $this->next = $next; $this->hidden = $hidden; }
    public static function make(string $name): GraphNode { return new GraphNode($name); }
}
#[Export] function graph_identity(GraphNode $node): GraphNode { return $node; }
#[Export] function graph_tick(): int { return 1; }
#[Export] function graph_values(array $values): array { return $values; }
#[Export] function graph_keep(?GraphNode $node, bool $set): ?GraphNode {
    static $saved = null;
    if ($set) { $saved = $node; }
    return $saved;
}
#[Export] function graph_parent(?GraphNode $child, int $mode): ?GraphNode {
    static $parent = null;
    if ($mode === 2) { $parent = null; return null; }
    if ($parent === null) { $parent = new GraphNode('parent'); }
    if ($mode === 0) { $parent->next = $child; return null; }
    return $parent;
}
"#);
    let output = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", module.display())).args(["-r", r#"
function check($condition, $label) { if (!$condition) { throw new Exception($label); } }
$n = GraphNode::make('retained');
$values = graph_values([$n, ['nested' => $n]]);
check($values[0] === $n && $values[1]['nested'] === $n, 'objects in copied arrays preserve identity');
unset($values);
$weak = WeakReference::create($n);
check(graph_keep($n, true) === $n, 'keep identity');
unset($n);
gc_collect_cycles();
check($weak->get() !== null, 'native static owner');
check(graph_keep(null, false) === $weak->get(), 'retained identity');
graph_keep(null, true);
check($weak->get() === null, 'released static owner');
$child = GraphNode::make('child');
$weakChild = WeakReference::create($child);
graph_parent($child, 0);
unset($child);
gc_collect_cycles();
check($weakChild->get() !== null, 'unobserved parent owns observed child');
$parent = graph_parent(null, 1);
check($parent->next === $weakChild->get(), 'exposed parent preserves child identity');
$parent->next = null;
check($weakChild->get() === null, 'Zend edge owns child without redundant native root');
graph_parent(null, 2);
unset($parent);
$observed = false;
$owner = GraphNode::make('owner');
$child = GraphNode::make('retired');
$owner->replaceLinks($child, $child);
$map = new WeakMap();
$map[$child] = new class(function () use ($owner, &$observed) {
    if ($owner->next !== null || $owner->reveal() !== null) { throw new Exception('partially published graph'); }
    $observed = true;
}) {
    public function __construct(private Closure $check) {}
    public function __destruct() { ($this->check)(); }
};
unset($child);
$owner->replaceLinks(null, null);
check($observed && count($map) === 0, 'WeakMap destructor reentry sees atomic publication');
unset($map, $owner);
$original = GraphNode::make('original');
$original->items = ['key' => 'original'];
$clone = clone $original;
check($clone !== $original && graph_identity($clone) === $clone, 'clone identity');
$clone->name = 'clone';
$clone->items['key'] = 'changed';
check($original->name === 'original' && $original->items['key'] === 'original', 'clone shallow COW');
unset($original, $clone);
for ($i = 0; $i < 1000; $i++) {
    $a = GraphNode::make('a');
    $b = GraphNode::make('b');
    $a->next = $b;
    $a->items = ['self' => $a, 'other' => $b];
    $b->hide($a);
    check($b->reveal() === $a && graph_identity($a) === $a, 'cycle identity');
    $wa = WeakReference::create($a);
    $wb = WeakReference::create($b);
    unset($a, $b);
    gc_collect_cycles();
    check($wa->get() === null && $wb->get() === null, 'cross-heap cycle collectible');
    check(graph_tick() === 1, 'native cycle release');
}
echo "identity and cycles ok\n";
"#]).output().unwrap();
    assert!(output.status.success(), "status {:?}\n{}\n{}", output.status,
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"identity and cycles ok\n");
    project.assert_clean();
}

/// Synchronizes references, indirect array writes, native throws and reentrant PHP output callbacks.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn phpext_classes_native_properties_exceptions_and_callbacks() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("class_state", r#"<?php
#[Export] final class NativeState {
    public int $value = 0;
    public int $uninitialized;
    public array $items = [];
    public mixed $dynamic = null;
    public function __construct(int $value = 0) {
        if ($value < 0) { throw new RuntimeException('constructor'); }
        $this->value = $value;
    }
    public function add(int $amount): int { $this->value += $amount; return $this->value; }
    public function identity(string $value): string { return $value; }
    public function readItems(): array { return $this->items; }
    public function initialize(int $value): void { $this->uninitialized = $value; }
    public function clear(): void { unset($this->uninitialized); }
    public function fail(): void { $this->value = 9; throw new RuntimeException('expected'); }
    public function output(): int { $this->value++; echo 'x'; $this->value++; return $this->value; }
    public function stream(int $count): int { for ($i = 0; $i < $count; $i++) { $this->value++; echo '.'; } return $this->value; }
}
"#);
    let output = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", module.display())).args(["-r", r#"
function check($condition, $label) { if (!$condition) { throw new Exception($label); } }
$p = new NativeState();
$alias =& $p->value;
check($p->add(3) === 3 && $alias === 3, 'native assignment updates external property alias');
$alias = 5;
check($p->add(2) === 7, 'external alias imports');
unset($alias);
$dynamic =& $p->dynamic;
$dynamic = ['value' => 42];
check($p->add(0) === 7 && $dynamic === ['value' => 42], 'mixed property alias imports');
unset($dynamic);
$p->items['nested']['key'] = 7;
check($p->readItems() === ['nested' => ['key' => 7]], 'indirect array write');
$property = new ReflectionProperty(NativeState::class, 'uninitialized');
check(!$property->isInitialized($p), 'uninitialized property');
$p->initialize(4);
$old =& $p->uninitialized;
$p->clear();
check($old === 4 && !$property->isInitialized($p), 'unset detaches property alias');
$old = 'now unconstrained';
unset($old);
for ($i = 0; $i < 1000; $i++) { check($p->identity("a\0b") === "a\0b", 'owned string identity'); }
try { $p->fail(); throw new Exception('missing native exception'); }
catch (RuntimeException $e) { check($e->getMessage() === 'expected', 'native exception message'); }
check($p->value === 9 && $p->add(1) === 10, 'native exception commits and recovers');
try { new NativeState(-1); throw new Exception('missing constructor exception'); }
catch (RuntimeException $e) { check($e->getMessage() === 'constructor', 'constructor exception'); }
check($p->add(1) === 11, 'constructor failure recovers');
$p->value = 0;
$observed = false;
ob_start(function ($text) use ($p, &$observed) {
    if ($text !== '') {
        $observed = $p->value === 1;
        $p->value = 10;
        if ($p->add(5) !== 15) { throw new Exception('nested native call'); }
    }
    return '';
}, 1);
$result = $p->output();
ob_end_clean();
check($observed && $result === 16 && $p->value === 16, 'callback observes and updates coherent object state');
$q = new NativeState();
$q->value =& $p->value;
try { $p->add(1); throw new Exception('shared exported slots accepted'); }
catch (Error $e) { check(str_contains($e->getMessage(), 'reference cell'), 'explicit reference graph rejection'); }
unset($q->value);
check($p->add(1) === 17, 'reference rejection leaves module usable');
$chunks = 0;
ob_start(function ($text) use (&$chunks) { if ($text !== '') { $chunks++; } return ''; }, 1);
$result = $p->stream(1000);
ob_end_clean();
check($result === 1017 && $chunks === 1000, 'object transport storage stays bounded across output callbacks');
echo "properties and callbacks ok\n";
"#]).output().unwrap();
    assert!(output.status.success(), "status {:?}\n{}\n{}", output.status,
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"properties and callbacks ok\n");
    project.assert_clean();
}

/// Keeps source type names, namespaced constants and immutable defaults visible through Reflection.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn phpext_classes_native_reflection_and_persistent_metadata() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile_with_options("class_metadata", r#"<?php
namespace ExportDemo;
const VERSION = '1.0';
const OPTIONS = ['nested' => ['key' => 'original']];
#[Export] final class Reflected {
    public const float RATE = 2;
    public const array OPTIONS = OPTIONS;
    public float $ratio = 1;
    public ?self $next = null;
    public array $options = OPTIONS;
    private string $secret = 'private';
    public function __construct(public int $promoted = 7, public ?self $parent = null) {}
    public function same(): self { return $this; }
    public function attach(?self $other): void { $this->next = $other; }
    public function choose(self|null $other = null): self|null { return $other; }
    public static function create(): static { return new self(); }
}
"#, &["--heap-size=524288", "--export-constant=ExportDemo\\VERSION", "--export-constant=ExportDemo\\OPTIONS"]);
    let output = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", module.display())).args(["-r", r#"
function check($condition, $label) { if (!$condition) { throw new Exception($label); } }
$r = new ReflectionClass(ExportDemo\Reflected::class);
check((string)$r->getMethod('same')->getReturnType() === 'self', 'self return reflection');
check((string)$r->getMethod('create')->getReturnType() === 'static', 'static return reflection');
$choose = $r->getMethod('choose');
check((string)$choose->getReturnType() === '?self', 'nullable self return reflection');
check((string)$choose->getParameters()[0]->getType() === '?self', 'nullable self parameter reflection');
check($choose->getParameters()[0]->getDefaultValue() === null, 'nullable default reflection');
check((string)$r->getProperty('next')->getType() === '?self', 'self property reflection');
check($r->getProperty('secret')->isPrivate(), 'private property visibility');
$promoted = $r->getProperty('promoted');
check($promoted->isPromoted() && !$promoted->hasDefaultValue(), 'promoted property metadata');
check($r->getConstructor()->getParameters()[0]->isPromoted()
    && $r->getConstructor()->getParameters()[1]->isPromoted(), 'promoted parameter metadata');
$uninitialized = $r->newInstanceWithoutConstructor();
check(!$promoted->isInitialized($uninitialized) && $uninitialized->same() === $uninitialized
    && !$promoted->isInitialized($uninitialized), 'constructorless promoted property stays uninitialized');
unset($uninitialized);
check($r->getDefaultProperties()['ratio'] === 1.0, 'float default coercion');
check(ExportDemo\Reflected::RATE === 2.0, 'typed class constant coercion');
check((string)$r->getReflectionConstant('RATE')->getType() === 'float', 'typed constant reflection');
check(ExportDemo\VERSION === '1.0', 'namespaced global constant');
$p = ExportDemo\Reflected::create();
check($p->promoted === 7, 'native factory runs promoted constructor');
check($p->same() === $p && $p->choose($p) === $p && $p->choose() === null, 'lexical object types invoke');
$foreign = new class { public ?self $slot = null; };
$foreign->slot =& $p->next;
try { $p->same(); throw new Exception('unrelated self constraints accepted'); }
catch (Error $error) {
    check(str_contains($error->getMessage(), 'identical declared types'), 'relative names resolve to their declaring scopes');
}
unset($foreign->slot, $foreign);
$compatible = new class { public ?ExportDemo\Reflected $slot = null; };
$compatible->slot =& $p->next;
$p->attach($p);
check($compatible->slot === $p, 'equivalent relative and absolute type constraints');
$p->attach(null);
unset($compatible->slot, $compatible);

for ($i = 0; $i < 1000; $i++) {
    $p->options['nested']['key'] = 'changed';
    $copy = ExportDemo\Reflected::OPTIONS;
    $copy['nested']['key'] = 'copy';
    $q = new ExportDemo\Reflected();
    check($q->options === ExportDemo\OPTIONS, 'persistent defaults stay immutable');
    check($q->same() === $q, 'bind array defaults');
    unset($q);
}
echo "reflection ok\n";
"#]).output().unwrap();
    assert!(output.status.success(), "status {:?}\n{}\n{}", output.status,
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"reflection ok\n");
    project.assert_clean();
}

/// Matches PHP's materialized values for nullable, union and mixed typed class constants.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn phpext_classes_native_typed_float_constant_unions_match_php() {
    let project = Project::new();
    let sdk = project.sdk();
    let source = r#"<?php
#[Export] final class FloatConstants {
    public const float EXACT = 2;
    public const ?float NULLABLE = 2;
    public const float|null UNION_NULL = 2;
    public const float|string UNION_STRING = 2;
    public const int|float UNION_INT = 2;
    public const float|int REVERSED_INT = 2;
    public const ?float NULL_VALUE = null;
    public const mixed MIXED_VALUE = 2;
    public const COPIED = self::EXACT;
    public const array NESTED = [self::NULLABLE];
    public mixed $copied = self::UNION_STRING;
    public function exactValue(): mixed { return self::EXACT; }
    public function nullableValue(): mixed { return self::NULLABLE; }
    public function unionNullValue(): mixed { return self::UNION_NULL; }
    public function stringUnionValue(): mixed { return self::UNION_STRING; }
    public function numericUnionValue(): mixed { return self::UNION_INT; }
    public function reversedNumericValue(): mixed { return self::REVERSED_INT; }
    public function nullValue(): mixed { return self::NULL_VALUE; }
    public function mixedValue(): mixed { return self::MIXED_VALUE; }
    public function copiedValue(): mixed { return self::COPIED; }
    public function nestedValue(): mixed { return self::NESTED; }
    public function globalArray(): array { return [FLOAT_COPY]; }
    public function classArray(): array { return [self::COPIED]; }
    public static function nativeValues(): array {
        return [self::EXACT, self::NULLABLE, self::UNION_NULL, self::UNION_STRING,
            self::UNION_INT, self::REVERSED_INT, self::NULL_VALUE, self::MIXED_VALUE,
            self::COPIED, self::NESTED, FLOAT_COPY];
    }
}
const FLOAT_COPY = FloatConstants::NULLABLE;
"#;
    let module = project.compile_with_options("class_float_constants", source,
        &["--heap-size=524288", "--export-constant=FLOAT_COPY"]);
    let check = r#"
$values = [];
foreach ((new ReflectionClass(FloatConstants::class))->getReflectionConstants() as $constant) {
    $values[] = [constant('FloatConstants::' . $constant->getName()), $constant->getValue(), (string)$constant->getType()];
}
$values[] = FLOAT_COPY;
$instance = new FloatConstants();
$values[] = $instance->copied;
foreach (['exactValue', 'nullableValue', 'unionNullValue', 'stringUnionValue', 'numericUnionValue',
    'reversedNumericValue', 'nullValue', 'mixedValue', 'copiedValue', 'nestedValue'] as $method) {
    $values[] = $instance->$method();
}
$values[] = FloatConstants::nativeValues();
$values[] = $instance->globalArray();
$values[] = $instance->classArray();
echo json_encode($values, JSON_PRESERVE_ZERO_FRACTION | JSON_THROW_ON_ERROR), "\n";
"#;
    let reference = project.0.join("reference.php");
    fs::write(&reference, format!("{source}\n{check}")).unwrap();
    let expected = Command::new(sdk.php_binary()).arg("-n").arg(&reference).output().unwrap();
    assert!(expected.status.success(), "{}", String::from_utf8_lossy(&expected.stderr));
    assert!(expected.stderr.is_empty(), "{}", String::from_utf8_lossy(&expected.stderr));
    let actual = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", module.display())).args(["-r", check]).output().unwrap();
    assert!(actual.status.success(), "{}", String::from_utf8_lossy(&actual.stderr));
    assert!(actual.stderr.is_empty(), "{}", String::from_utf8_lossy(&actual.stderr));
    assert_eq!(String::from_utf8_lossy(&actual.stdout), String::from_utf8_lossy(&expected.stdout));
    project.assert_clean();
}

/// Leaves published Zend objects usable while refusing unsafe native reentry after a bridge failure.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn phpext_classes_native_published_failure_preserves_host_objects() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("class_failure", r#"<?php
#[Export] final class PublishedObject {
    public int $value = 0;
    public mixed $payload = null;
    public ?PublishedObject $next = null;
    public function drop(): void { $this->next = null; }
    public function run(): void { $this->value = 1; echo 'boundary'; $this->value = 2; }
    public function read(): int { return $this->value; }
}
"#);
    let output = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", module.display())).args(["-r", r#"
$p = new PublishedObject();
$resource = fopen('php://memory', 'r+');
ob_start(function ($text) use ($p, $resource) { if ($text !== '') { $p->payload = $resource; } return ''; }, 1);
try { $p->run(); throw new Exception('missing transport failure'); }
catch (Error $error) { $failed = str_contains($error->getMessage(), 'boundary supports'); }
ob_end_clean();
if (!$failed || $p->value !== 1 || $p->payload !== $resource) { throw new Exception('lost host object state'); }
$alias = $p;
$p->value = 7;
if ($alias !== $p || $alias->value !== 7) { throw new Exception('lost host identity'); }
try { $p->read(); throw new Exception('unsafe native reentry accepted'); }
catch (Error $error) {
    if (!str_contains($error->getMessage(), 'next request')) { throw $error; }
}
$p->payload = null;
fclose($resource);
unset($p, $alias);
gc_collect_cycles();
echo "published failure contained\n";
"#]).output().unwrap();
    assert!(output.status.success(), "status {:?}\n{}\n{}", output.status,
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"published failure contained\n");
    let output = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", module.display())).args(["-r", r#"
$p = new PublishedObject();
$child = new PublishedObject();
$p->next = $child;
$p->read();
$resource = fopen('php://memory', 'r+');
$failed = false;
$map = new WeakMap();
$map[$child] = new class(function () use ($p, $resource, &$failed) {
    ob_start(function ($text) use ($p, $resource) { if ($text !== '') { $p->payload = $resource; } return ''; }, 1);
    try { $p->run(); }
    catch (Error $error) { $failed = str_contains($error->getMessage(), 'boundary supports'); }
    finally { ob_end_clean(); }
}) {
    public function __construct(private Closure $check) {}
    public function __destruct() { ($this->check)(); }
};
unset($child);
$p->drop();
if (!$failed || count($map) !== 0 || $p->next !== null || $p->value !== 1) {
    throw new Exception('nested failure corrupted retiring object registry');
}
try { $p->read(); throw new Exception('nested failure accepted native reentry'); }
catch (Error $error) { if (!str_contains($error->getMessage(), 'next request')) { throw $error; } }
$p->payload = null;
fclose($resource);
echo "nested published failure contained\n";
"#]).output().unwrap();
    assert!(output.status.success(), "status {:?}\n{}\n{}", output.status,
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"nested published failure contained\n");
    project.assert_clean();
}

/// Rejects unsupported host object surfaces before any SDK discovery in terminal modes.
#[test]
fn phpext_classes_reject_unsupported_shapes_before_sdk() {
    let project = Project::new();
    for (index, source) in [
        "<?php #[Export] class NotFinal {}",
        "<?php #[Export] final class StaticProperty { public static int $value = 0; }",
        "<?php #[Export] final class UntypedProperty { public $value = 0; }",
        "<?php #[Export] final class Magic { public function __destruct() {} }",
        "<?php #[Export] final class PrivateConstructor { private function __construct() {} }",
        "<?php #[Export] interface Contract {}",
        "<?php #[Export] trait Reusable {}",
        "<?php #[Export] enum Choice { case One; }",
    ].iter().enumerate() {
        let path = project.0.join(format!("unsupported_class_{index}.php"));
        fs::write(&path, source).unwrap();
        for terminal in ["--check", "--emit-ir"] {
            let output = Command::new(compiler_binary()).args(["--emit", "phpext", terminal]).arg(&path)
                .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk")).output().unwrap();
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{source}: {terminal}");
            assert!(stderr.contains("PHP class export"), "{source}: {terminal}: {stderr}");
            assert!(!stderr.contains("query php-config"), "{source}: {terminal}: {stderr}");
        }
    }
}

/// Drains queued leases after lazy binding fills the heap without allocating a journal per release.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn phpext_classes_native_heap_pressure_reclaims_queued_leases() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile_with_options("class_pressure", r#"<?php
#[Export] final class SmallNativeObject {}
#[Export] function pressure_input(array $values): int { return count($values); }
#[Export] function pressure_alive(): int { return 42; }
"#, &["--heap-size=65536"]);
    let output = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", module.display())).args(["-r", r#"
for ($round = 0; $round < 3; $round++) {
    $objects = [];
    for ($i = 0; $i < 10000; $i++) { $objects[] = new SmallNativeObject(); }
    $failed = false;
    try { pressure_input($objects); }
    catch (Error $error) { $failed = true; }
    if (!$failed) { throw new Exception('native heap pressure did not occur'); }
    // PHP exception traces may own the argument array until the caught Error is released.
    unset($error, $objects);
    gc_collect_cycles();
    if (pressure_alive() !== 42 || pressure_input([]) !== 0) { throw new Exception('queued native leases did not release'); }
}
echo "heap pressure recovered\n";
"#]).output().unwrap();
    assert!(output.status.success(), "status {:?}\n{}\n{}", output.status,
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"heap pressure recovered\n");
    project.assert_clean();
}
