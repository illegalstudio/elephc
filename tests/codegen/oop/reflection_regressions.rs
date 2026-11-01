//! Purpose:
//! End-to-end regressions for Reflection metadata materialization and typed dispatch.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixtures exercise closed-world reflection metadata using real user declarations.

use super::*;

/// Rebinding native metadata must not replace declaration paths with the entry-point path.
#[test]
fn test_eval_foreign_reflection_preserves_native_declaration_source() {
    let out = compile_cli_files_and_run(
        &[
            ("main.php", r#"<?php
require __DIR__ . '/ForeignFile.php';
$object = new FileMetadataTarget();
$reflection = new ReflectionObject($object);
$method = new ReflectionMethod(FileMetadataTarget::class, 'read');
eval(file_get_contents(__DIR__ . '/inspect.txt'));
"#),
            ("ForeignFile.php", "<?php\nclass FileMetadataTarget {\n    public const MARK = 'ok';\n    public function read(): int { return 1; }\n}\n"),
            ("inspect.txt", "echo count($reflection->getConstants()), ':', basename($reflection->getFileName()), ':', $reflection->getStartLine(), ':', $reflection->getEndLine(), '|'; echo $method->getNumberOfParameters(), ':', basename($method->getFileName()), ':', $method->getStartLine(), ':', $method->getEndLine();"),
        ],
        "main.php",
    );
    assert_eq!(out, "1:ForeignFile.php:2:5|0:ForeignFile.php:4:4");
}

/// Native reflection metadata remains usable after returning an object from another eval frame.
#[test]
fn test_eval_foreign_frame_reflection_rebinds_native_classlikes() {
    let out = compile_cli_files_and_run(
        &[
            ("main.php", r#"<?php
class ForeignClass { public const MARK = 'class'; }
interface ForeignContract { public const MARK = 'interface'; }
trait ForeignTrait { public const MARK = 'trait'; }
enum ForeignEnum { case Ready; public const MARK = 'enum'; }
class_alias(ForeignClass::class, 'ForeignAlias');
function createForeignReflection(string $name): ReflectionClass {
    return eval(file_get_contents(__DIR__ . '/create.txt'));
}
foreach (['ForeignClass', 'ForeignContract', 'ForeignTrait', 'ForeignEnum', '\\foreignclass', 'ForeignAlias'] as $name) {
    $reflection = createForeignReflection($name);
    eval(file_get_contents(__DIR__ . '/inspect.txt'));
}
"#),
            ("create.txt", "return new ReflectionClass($name);"),
            ("inspect.txt", "echo $reflection->getName(), ':', $reflection->getConstants()['MARK'], '|';"),
        ],
        "main.php",
    );
    assert_eq!(out, "ForeignClass:class|ForeignContract:interface|ForeignTrait:trait|ForeignEnum:enum|ForeignClass:class|ForeignClass:class|");
}

/// Associative object results must expose raw objects to compiled consumers, not boxed cells.
#[test]
fn test_eval_associative_object_return_preserves_identity_and_aliases() {
    let out = compile_cli_files_and_run(
        &[
            ("main.php", r#"<?php
interface MapContract { public function load(); }
class MapImplementation implements MapContract { public function load() {} }
$owner = new ReflectionClass(MapImplementation::class);
$interfaces = $owner->getInterfaces();
$copy = $interfaces;
unset($interfaces);
$reflection = $copy['MapContract'];
echo get_class($reflection), ':';
eval(file_get_contents(__DIR__ . '/inspect.txt'));
"#),
            ("inspect.txt", "echo $reflection->getName(), ':', count($reflection->getConstants());"),
        ],
        "main.php",
    );
    assert_eq!(out, "ReflectionClass:MapContract:0");
}

/// The same associative-object boundary applies to trait maps and empty maps.
#[test]
fn test_eval_associative_object_return_handles_traits_and_empty_maps() {
    let out = compile_cli_files_and_run(
        &[
            ("main.php", r#"<?php
trait MapTrait {}
class MapTraitOwner { use MapTrait; }
class EmptyMapOwner {}
$owner = new ReflectionClass(MapTraitOwner::class);
$traits = $owner->getTraits();
$reflection = $traits['MapTrait'];
echo get_class($reflection), ':';
eval(file_get_contents(__DIR__ . '/inspect.txt'));
$empty = new ReflectionClass(EmptyMapOwner::class);
echo ':', count($empty->getTraits()), ':', count($empty->getInterfaces());
"#),
            ("inspect.txt", "echo $reflection->getName();"),
        ],
        "main.php",
    );
    assert_eq!(out, "ReflectionClass:MapTrait:0:0");
}

/// Verifies Reflection materializes indexed and associative literal class constants as arrays.
#[test]
fn test_reflection_class_materializes_literal_array_constants() {
    let out = compile_and_run(
        r#"<?php
class ArrayConstantTarget {
    private const SECOND = 'second';
    public const PAD = STR_PAD_RIGHT;
    public const TAGS = ['first', self::SECOND];
    public const OPTIONS = ['mode' => 'strict', 7 => 40 + 2, 'pad' => STR_PAD_RIGHT];
}

$reflection = new ReflectionClass(ArrayConstantTarget::class);
$tags = $reflection->getConstant('TAGS');
$options = $reflection->getConstants()['OPTIONS'];
$memberTags = $reflection->getReflectionConstant('TAGS')->getValue();
echo $tags[0] . ':' . $tags[1] . ':';
echo $options['mode'] . ':' . $options[7] . ':' . $options['pad'] . ':';
echo $memberTags[0] . ':' . $memberTags[1];
echo ':' . $reflection->getConstant('PAD');
"#,
    );
    assert_eq!(out, "first:second:strict:42:1:first:second:1");
}

/// Verifies a `Reflector`-typed parameter dispatches the common concrete
/// `getAttributes()` method through the internal interface table.
#[test]
fn test_reflector_typed_get_attributes_dispatch() {
    let out = compile_and_run(
        r#"<?php
class PlainReflectionTarget {}
function attribute_count(Reflector $reflector): int {
    return count($reflector->getAttributes());
}
echo attribute_count(new ReflectionClass(PlainReflectionTarget::class));
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies a runtime class-name reflection query returns filtered class attributes as an array.
#[test]
fn test_reflection_runtime_class_name_filtered_attributes_are_iterable() {
    let out = compile_and_run(
        r#"<?php
#[Attribute]
class RequiredDependency {
    public function __construct(public string $className) {}
}

#[RequiredDependency('base')]
class DependentClass {}

$class = DependentClass::class;
$attributes = (new ReflectionClass($class))->getAttributes(RequiredDependency::class);
foreach ($attributes as $attribute) {
    echo $attribute->newInstance()->className;
}
"#,
    );
    assert_eq!(out, "base");
}

/// Verifies filtered class attributes remain iterable for runtime-declared classes.
#[test]
fn test_reflection_runtime_declared_class_filtered_attributes_are_iterable() {
    let out = compile_and_run(
        r#"<?php
#[Attribute]
class RequiredDependency {
    public function __construct(public string $className) {}
}

$definition = '#[RequiredDependency(\'base\')] class DependentClass {}';
eval($definition);
$class = 'DependentClass';
$filter = 'RequiredDependency';
$attributes = (new ReflectionClass($class))->getAttributes($filter);
foreach ($attributes as $attribute) {
    echo $attribute->newInstance()->className;
}
"#,
    );
    assert_eq!(out, "base");
}

/// Verifies runtime ReflectionClass construction gives attribute-free classes an empty array.
#[test]
fn test_runtime_reflection_attribute_free_class_returns_empty_array() {
    let out = compile_and_run(
        r#"<?php
class PlainReflectionTarget {}

$source = 'return ["PlainReflectionTarget"];';
$classes = eval($source);
$class = $classes[0];
$attributes = (new ReflectionClass($class))->getAttributes();
echo is_array($attributes) ? count($attributes) : 'not-array';
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies a runtime class name preserves AOT attribute metadata and filtering.
#[test]
fn test_runtime_reflection_aot_attribute_filter_returns_iterable_array() {
    let out = compile_and_run(
        r#"<?php
#[Attribute]
class RequiredDependency {
    public function __construct(public string $className) {}
}

#[RequiredDependency('base')]
class DecoratedReflectionTarget {}

$source = 'return ["DecoratedReflectionTarget"];';
$classes = eval($source);
$attributes = (new ReflectionClass($classes[0]))->getAttributes(RequiredDependency::class);
foreach ($attributes as $attribute) {
    echo $attribute->newInstance()->className;
}
"#,
    );
    assert_eq!(out, "base");
}

/// Verifies runtime reflection materializes an AOT attribute `::class` argument as a string.
#[test]
fn test_runtime_reflection_aot_attribute_class_constant_argument_is_iterable() {
    let out = compile_and_run(
        r#"<?php
#[Attribute]
class RequiredDependency {
    public function __construct(public string $className) {}
}

class DependencyTarget {}

#[RequiredDependency(DependencyTarget::class)]
class DecoratedReflectionTarget {}

$source = 'return ["DecoratedReflectionTarget"];';
$classes = eval($source);
$attributes = (new ReflectionClass($classes[0]))->getAttributes(RequiredDependency::class);
foreach ($attributes as $attribute) {
    echo $attribute->newInstance()->className;
}
"#,
    );
    assert_eq!(out, "DependencyTarget");
}

/// Verifies an AOT method preserves runtime reflection attributes from an eval-produced class name.
#[test]
fn test_aot_method_runtime_reflection_attribute_array_is_iterable() {
    let out = compile_and_run(
        r#"<?php
#[Attribute]
class RequiredDependency {
    public function __construct(public string $className) {}
}

#[RequiredDependency('base')]
class DecoratedReflectionTarget {}

class AttributeResolver {
    public function resolve(string $class): string {
        $result = '';
        foreach ((new ReflectionClass($class))->getAttributes(RequiredDependency::class) as $attribute) {
            $result .= $attribute->newInstance()->className;
        }

        return $result;
    }
}

$source = 'return ["DecoratedReflectionTarget"];';
$classes = eval($source);
echo (new AttributeResolver())->resolve($classes[0]);
"#,
    );
    assert_eq!(out, "base");
}

/// Verifies eval-to-AOT by-reference calls preserve runtime reflection attribute arrays.
#[test]
fn test_eval_aot_by_ref_method_runtime_reflection_attributes_are_iterable() {
    let out = compile_and_run(
        r#"<?php
#[Attribute]
class RequiredDependency {
    public function __construct(public string $className) {}
}

#[RequiredDependency('base')]
class DecoratedReflectionTarget {}

class AttributeResolver {
    public function resolve(string $class, array &$resolved, array &$visiting = []): string {
        foreach ((new ReflectionClass($class))->getAttributes(RequiredDependency::class) as $attribute) {
            $resolved[$class] = true;
            $visiting[$class] = true;
            return $attribute->newInstance()->className;
        }

        return 'empty';
    }
}

$source = '$resolved = []; $visiting = []; $resolver = new AttributeResolver(); echo $resolver->resolve("DecoratedReflectionTarget", $resolved, $visiting);';
eval($source);
"#,
    );
    assert_eq!(out, "base");
}

/// Verifies autoloaded attribute metadata remains iterable through an eval-produced class name.
#[test]
fn test_autoloaded_runtime_reflection_attribute_array_is_iterable() {
    let out = compile_and_run_files(
        &[
            (
                "decorated.php",
                r#"<?php
namespace Fixture;

#[\RequiredDependency('base')]
class DecoratedTarget {}
"#,
            ),
            (
                "entry.php",
                r#"<?php
#[Attribute]
class RequiredDependency {
    public function __construct(public string $className) {}
}

class AttributeResolver {
    public function resolve(string $class): string {
        $result = '';
        foreach ((new ReflectionClass($class))->getAttributes(RequiredDependency::class) as $attribute) {
            $result .= $attribute->newInstance()->className;
        }

        return $result;
    }
}

spl_autoload_register(function (string $class): void {
    if ($class === 'Fixture\\DecoratedTarget') {
        require __DIR__ . '/decorated.php';
    }
});

$source = 'return ["Fixture\\DecoratedTarget"];';
$classes = eval($source);
if (class_exists($classes[0])) {
    echo (new AttributeResolver())->resolve($classes[0]);
}
"#,
            ),
        ],
        "entry.php",
    );
    assert_eq!(out, "base");
}

/// Verifies public Reflection name properties share the same populated metadata as `getName()`.
#[test]
fn test_reflection_public_name_properties_use_populated_metadata() {
    let out = compile_and_run(
        r#"<?php
class NamedReflectionTarget {
    public function execute(string $value): void {}
}
$class = new ReflectionClass(NamedReflectionTarget::class);
$method = new ReflectionMethod(NamedReflectionTarget::class, 'execute');
$parameter = $method->getParameters()[0];
echo $class->name . '|' . $method->name . '|' . $method->class . '|' . $parameter->name;
"#,
    );
    assert_eq!(
        out,
        "NamedReflectionTarget|execute|NamedReflectionTarget|value"
    );
}

/// Verifies callable reflectors are assignable to their shared abstract parent and dispatch the
/// inherited contracts to the concrete method reflector implementation.
#[test]
fn test_reflection_method_through_function_abstract_contract() {
    let out = compile_and_run(
        r#"<?php
class AbstractReflectionTarget {
    public function execute(string $value): int { return 1; }
}
function describe(ReflectionFunctionAbstract $reflector): string {
    return $reflector->name . ':' . $reflector->class . ':' . $reflector->getName() . ':' .
        count($reflector->getParameters()) . ':' .
        ($reflector->hasReturnType() ? 'typed' : 'untyped');
}
$method = new ReflectionMethod(AbstractReflectionTarget::class, 'execute');
echo $method instanceof ReflectionFunctionAbstract ? 'base|' : 'bad|';
echo describe($method);
"#,
    );
    assert_eq!(
        out,
        "base|execute:AbstractReflectionTarget:execute:1:typed"
    );
}

/// Verifies reflected instance, static, and function targets materialize ordinary callables that
/// preserve their receiver and declared argument behavior.
#[test]
fn test_reflection_get_closure_materializes_tracked_callable_target() {
    let out = compile_and_run(
        r#"<?php
class ReflectedClosureTarget {
    public function __construct(private int $base) {}
    public function add(int $value): int { return $this->base + $value; }
    public static function twice(int $value): int { return $value * 2; }
}
function reflected_increment(int $value): int { return $value + 1; }
$target = new ReflectedClosureTarget(4);
$instance = (new ReflectionMethod(ReflectedClosureTarget::class, 'add'))->getClosure($target);
$static = (new ReflectionMethod(ReflectedClosureTarget::class, 'twice'))->getClosure();
$function = (new ReflectionFunction('reflected_increment'))->getClosure();
echo $instance(3), ':', $static(5), ':', $function(8);
"#,
    );
    assert_eq!(out, "7:10:9");
}

/// Verifies a callable descriptor supplies ReflectionFunction signature metadata and remains
/// invokable through both the reflector and the Closure returned by `getClosure()`.
#[test]
fn test_reflection_function_accepts_callable_descriptor() {
    let out = compile_and_run(
        r#"<?php
$callable = function (string $name, int $count = 3): string {
    return $name . ':' . $count;
};
$reflection = new ReflectionFunction($callable);
$parameters = $reflection->getParameters();
echo count($parameters), ':', $reflection->getNumberOfRequiredParameters(), ':';
echo $parameters[0]->getName(), ':', $parameters[0]->getType()->getName(), ':';
echo $parameters[1]->getDefaultValue(), ':', $reflection->getReturnType()->getName(), ':';
echo $reflection->invoke('Ada', 4), ':';
$copy = $reflection->getClosure();
echo $copy('Lin');
"#,
    );
    assert_eq!(out, "2:1:name:string:3:string:Ada:4:Lin:3");
}

/// Verifies callable-backed ReflectionFunction metadata preserves binding, scope, and staticity.
#[test]
fn test_reflection_function_reports_bound_closure_metadata() {
    let out = compile_and_run(
        r#"<?php
class ReflectedBoundClosureTarget {
    public function make(): Closure {
        return function (): object { return $this; };
    }
}
$target = new ReflectedBoundClosureTarget();
$reflection = new ReflectionFunction($target->make());
echo $reflection->isAnonymous() ? 'anonymous:' : 'named:';
echo $reflection->isStatic() ? 'static:' : 'bound:';
echo $reflection->isClosure() ? 'closure:' : 'function:';
echo $reflection->getClosureThis() === $target ? 'this:' : 'missing:';
echo $reflection->getClosureScopeClass()->getName(), ':';
echo $reflection->getClosureCalledClass()->getName();
"#,
    );
    assert_eq!(
        out,
        "anonymous:bound:closure:this:ReflectedBoundClosureTarget:ReflectedBoundClosureTarget"
    );
}

/// Verifies runtime string targets use registered function metadata instead of requiring a
/// compile-time literal.
#[test]
fn test_reflection_function_accepts_runtime_string_target() {
    let out = compile_and_run(
        r#"<?php
function reflected_runtime_target(string $value = 'ok'): string {
    return strtoupper($value);
}
$name = 'reflected_runtime_target';
$reflection = new ReflectionFunction($name);
echo $reflection->getName(), ':', $reflection->getNumberOfParameters(), ':';
echo $reflection->getNumberOfRequiredParameters(), ':', $reflection->invoke('ready');
"#,
    );
    assert_eq!(out, "reflected_runtime_target:1:0:READY");
}

/// Verifies a gradual class-name operand is resolved and validated by runtime metadata.
#[test]
fn test_reflection_class_accepts_gradual_runtime_target() {
    let out = compile_and_run(
        r#"<?php
class ReflectedRuntimeClassTarget {}
$name = $argc > 0 ? ReflectedRuntimeClassTarget::class : null;
$reflection = new ReflectionClass($name);
echo $reflection->name, ':', $reflection->getName();
"#,
    );
    assert_eq!(out, "ReflectedRuntimeClassTarget:ReflectedRuntimeClassTarget");
}

/// Verifies an eval-materialized ReflectionClass exposes the native public name property.
#[test]
fn test_eval_materialized_reflection_class_exposes_public_name() {
    let out = compile_and_run(
        r#"<?php
class EvalReflectedRuntimeClassTarget {}
$inside = eval('$reflection = new ReflectionClass("EvalReflectedRuntimeClassTarget"); return $reflection->name;');
$reflection = eval('return new ReflectionClass("EvalReflectedRuntimeClassTarget");');
echo $inside, ':', $reflection->name, ':', $reflection->getName();
"#,
    );
    assert_eq!(
        out,
        "EvalReflectedRuntimeClassTarget:EvalReflectedRuntimeClassTarget:EvalReflectedRuntimeClassTarget"
    );
}

/// Verifies source-backed reflection owners expose file and declaration-line metadata.
#[test]
fn test_reflection_source_location_metadata() {
    let out = compile_and_run(
        r#"<?php
class ReflectionSourceTarget {
    public function execute(): void {}
}
function reflection_source_function(): void {}

$class = new ReflectionClass(ReflectionSourceTarget::class);
$object = new ReflectionObject(new ReflectionSourceTarget());
$method = new ReflectionMethod(ReflectionSourceTarget::class, 'execute');
$function = new ReflectionFunction('reflection_source_function');
echo $class->getFileName() !== false ? 'f' : 'x';
$objectFile = $object->getFileName();
echo is_string($objectFile) && '' !== $objectFile ? 'f' : 'x';
echo $method->getEndLine() >= $method->getStartLine() ? 'e' : 'x';
echo $function->getFileName() !== false ? 'f' : 'x';
"#,
    );
    assert_eq!(out, "ffef");
}

/// Verifies ReflectionObject on `$this` uses the lexical hierarchy to select runtime metadata.
#[test]
fn test_reflection_object_this_source_file_is_non_empty() {
    let out = compile_and_run(
        r#"<?php
class ReflectionThisSourceBase {
    public function sourceFile(): string|bool {
        return (new ReflectionObject($this))->getFileName();
    }
}
final class ReflectionThisSourceChild extends ReflectionThisSourceBase {}
$file = (new ReflectionThisSourceChild())->sourceFile();
echo is_string($file) && '' !== $file ? 'file' : 'missing';
"#,
    );
    assert_eq!(out, "file");
}

/// Verifies a direct runtime ReflectionObject source-file query avoids eagerly
/// materializing unrelated Reflection metadata while preserving the file result.
#[test]
fn test_direct_reflection_object_this_source_file_is_non_empty() {
    let out = compile_and_run(
        r#"<?php
class DirectReflectionSourceBase {
    public function sourceFile(): string|bool {
        return (new ReflectionObject($this))->getFileName();
    }
}
final class DirectReflectionSourceChild extends DirectReflectionSourceBase {}
$file = (new DirectReflectionSourceChild())->sourceFile();
echo is_string($file) && '' !== $file ? 'file' : 'missing';
"#,
    );
    assert_eq!(out, "file");
}

/// Verifies a source-only ReflectionObject query through an interface-typed property.
#[test]
fn test_reflection_object_interface_property_source_file_is_non_empty() {
    let out = compile_and_run(
        r#"<?php
interface ReflectionSourceProvider { public function configure(): void; }
final class ReflectionSourceProviderImpl implements ReflectionSourceProvider {
    public function configure(): void {}
}
final class ReflectionSourceConsumer {
    public function __construct(private ReflectionSourceProvider $subject) {}
    public function sourceFile(): string|bool {
        return (new ReflectionObject($this->subject))->getFileName();
    }
}
$file = (new ReflectionSourceConsumer(new ReflectionSourceProviderImpl()))->sourceFile();
echo is_string($file) && '' !== $file ? 'file' : 'missing';
"#,
    );
    assert_eq!(out, "file");
}

/// Verifies a local ReflectionObject may read both source metadata fields without full members.
#[test]
fn test_reflection_object_local_source_file_and_name_are_available() {
    let out = compile_and_run(
        r#"<?php
interface ReflectionSourceMetadataProvider { public function configure(): void; }
final class ReflectionSourceMetadataProviderImpl implements ReflectionSourceMetadataProvider {
    public function configure(): void {}
}
final class ReflectionSourceMetadataConsumer {
    public function __construct(private ReflectionSourceMetadataProvider $subject) {}
    public function sourceMetadata(): string {
        $reflection = new ReflectionObject($this->subject);
        $file = $reflection->getFileName();
        return (is_string($file) && '' !== $file ? 'file:' : 'missing:').$reflection->name;
    }
}
echo (new ReflectionSourceMetadataConsumer(new ReflectionSourceMetadataProviderImpl()))->sourceMetadata();
"#,
    );
    assert_eq!(out, "file:ReflectionSourceMetadataProviderImpl");
}

/// Verifies the built-in Reflection attribute-filter flag is available as a class constant.
#[test]
fn test_reflection_attribute_instanceof_filter_constant() {
    let out = compile_and_run("<?php echo ReflectionAttribute::IS_INSTANCEOF;");
    assert_eq!(out, "2");
}

/// Verifies writes in the sole fallthrough `else` branch survive guard restoration so a
/// reflected attribute instance keeps its concrete runtime value after `newInstance()`.
#[test]
fn test_reflection_attribute_instance_survives_terminal_guard_chain() {
    let out = compile_and_run(
        r#"<?php
#[Attribute(Attribute::TARGET_PARAMETER)]
class GuardedAttributePayload {
    public function __construct(public string $value) {}
}
class GuardedAttributeOwner {
    public function execute(#[GuardedAttributePayload('ready')] string $argument): void {}
}
$parameter = (new ReflectionMethod(GuardedAttributeOwner::class, 'execute'))->getParameters()[0];
foreach ([1] as $index) {
    if (false) {
        continue;
    } elseif (!$attribute = $parameter->getAttributes(GuardedAttributePayload::class)[0] ?? null) {
        continue;
    } else {
        $attribute = $attribute->newInstance();
    }
    echo $attribute->value;
}
$arguments = ['$argument' => new GuardedAttributePayload('direct')];
foreach ([1] as $index) {
    if (array_key_exists('$'.$parameter->name, $arguments)) {
        $attribute = $arguments['$'.$parameter->name];
        if (!$attribute instanceof GuardedAttributePayload) {
            continue;
        }
    } elseif (!$attribute = $parameter->getAttributes(GuardedAttributePayload::class)[0] ?? null) {
        continue;
    } else {
        $attribute = $attribute->newInstance();
    }
    echo '|'.$attribute->value;
}
"#,
    );
    assert_eq!(out, "ready|direct");
}

/// Pins that reflecting an object whose method parameter names another class stays bounded.
///
/// ⚠️ This does NOT reproduce the open `--web` defect, and must not be mistaken for it: reflecting
/// an object whose parameter is typed `Symfony\Component\DependencyInjection\ContainerBuilder`
/// exhausts the heap identically at `--heap-size` 8 MiB and 1 GiB, where php charges 96 bytes for
/// the same handle. Every attempt to reproduce that locally has passed — including a
/// self-referential class with 40 methods, 120 classes x 12 methods, traits, attributes and
/// associative class constants. The trigger is not class size, cycles, or any feature reproduced
/// here. See `.plans/reflection-slot-reachability.md`; this case only guards the shape from
/// regressing further.
#[test]
fn test_reflection_object_with_class_typed_parameter_stays_bounded() {
    let out = compile_and_run(
        r#"<?php
class Wide {
    public function a(Wide $x, ?Wide $y = null): ?Wide { return null; }
    public function b(Wide $x, ?Wide $y = null): ?Wide { return null; }
    public function c(Wide $x, ?Wide $y = null): ?Wide { return null; }
    public function d(Wide $x, ?Wide $y = null): ?Wide { return null; }
}
class Narrow {
    public function m(Wide $w): void {}
}
$r = new ReflectionObject(new Narrow());
echo $r->getName(), "|", count($r->getMethods());
"#,
    );
    assert_eq!(out, "Narrow|1");
}

/// Verifies `getFileName()` reports the file a class was DECLARED in, not the entry file.
///
/// Declaration-to-file attribution ran after `require` targets were spliced into their parent
/// program, so a class written in another file was either attributed to whichever file included it
/// or — for the entry program's own includes — recorded nowhere at all, at which point
/// `reflection_source_file` fell back to the module source path. The fallback always yields a real
/// existing path, which is what kept this silent: `is_file()` stayed true and Symfony's
/// `getProjectDir()` even recovered, because its walk-up to `composer.json` compensated.
#[test]
fn test_reflection_get_file_name_reports_the_declaring_file() {
    // Driven through the real CLI: the in-process multi-file helper is a hand-rolled pipeline
    // that never populates declaration-to-file attribution at all, so it would observe the
    // fallback no matter what the compiler does.
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                "<?php\nrequire __DIR__ . '/loader.php';\n$o = new \\Inc\\Widget();\n$r = new \\ReflectionObject($o);\necho basename((string) $r->getFileName()), '|', $o->file() === $r->getFileName() ? 'same' : 'different';\n",
            ),
            ("loader.php", "<?php\nrequire __DIR__ . '/Widget.php';\n"),
            (
                "Widget.php",
                "<?php\nnamespace Inc;\nclass Widget { public function file(): string { return __FILE__; } }\nfunction widget_file(): string { return __FILE__; }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "Widget.php|same");
}

/// ReflectionClass created in an inherited method reports the concrete runtime class's file,
/// even when the base method and subclass were compiled from different source units.
#[test]
fn test_reflection_aot_runtime_class_uses_its_declaring_file() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
require __DIR__ . '/Base.php';
require __DIR__ . '/foreign.php';
$target = new ForeignReflectionTarget();
echo basename($target->ownSourceFile()), '|', basename($target->reflectedSourceFile());
"#,
            ),
            (
                "Base.php",
                r#"<?php
abstract class ReflectionSourceBase {
    public function reflectedSourceFile() {
        return (new ReflectionClass($this))->getFileName();
    }
}
"#,
            ),
            (
                "foreign.php",
                "<?php\nclass ForeignReflectionTarget extends ReflectionSourceBase { public function ownSourceFile(): string { return __FILE__; } }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "foreign.php|foreign.php");
}

/// Verifies the deprecated `ReflectionParameter::getClass()` still reports the parameter's class
/// when the program actually calls it.
///
/// This is the over-pruning guard for the slot-reachability gate: it is the test that fails if the
/// gate ever decides a slot is dead while a reachable accessor can still read it.
#[test]
fn test_reflection_parameter_get_class_still_reports_the_parameter_class() {
    let out = compile_and_run(
        r#"<?php
class Dep {}
class Uses {
    public function m(Dep $d): void {}
}
$r = new ReflectionObject(new Uses());
$p = $r->getMethod('m')->getParameters()[0];
$c = $p->getClass();
echo $c === null ? "null" : $c->getName();
"#,
    );
    assert_eq!(out, "Dep");
}

/// Verifies a reflected parameter default reads a constant declared in an autoloaded interface.
///
/// The interface and the class are declared by different runtime `include`s, each running in its
/// own eval context, and only the declaration crosses between contexts. Reading `self::CONST` from
/// the context that reflects then found the declaration with no value behind it, which killed the
/// eval bridge outright; `getConstant()` answered `false` from the same gap. `php -n` prints
/// `generate|1|1|1`.
#[test]
fn test_autoloaded_interface_constant_is_readable_from_another_eval_context() {
    let out = compile_and_run_files(
        &[
            (
                "contract.php",
                r#"<?php
interface AutoloadedConstantContract {
    public const ABSOLUTE_PATH = 1;
}
"#,
            ),
            (
                "target.php",
                r#"<?php
class AutoloadedConstantTarget implements AutoloadedConstantContract {
    public function generate(string $name, int $referenceType = self::ABSOLUTE_PATH): string {
        return $name . $referenceType;
    }
}
"#,
            ),
            (
                "entry.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    if ($class === 'AutoloadedConstantContract') {
        require __DIR__ . '/contract.php';
    }
    if ($class === 'AutoloadedConstantTarget') {
        require __DIR__ . '/target.php';
    }
});

$name = 'AutoloadedConstantTarget';
class_exists($name);
$reflector = new ReflectionClass($name);
$method = $reflector->getMethod('generate');
echo $method->getName();
echo '|' . $method->getParameters()[1]->getDefaultValue();
echo '|' . $reflector->getConstant('ABSOLUTE_PATH');
echo '|' . count($reflector->getMethods());
"#,
            ),
        ],
        "entry.php",
    );
    assert_eq!(out, "generate|1|1|1");
}

/// Verifies a reflector held in a boxed local still reports an autoloaded class's own methods.
///
/// Reassigning the reflector to the `ReflectionMethod` it produced — the shape a container's
/// `getReflectionMethod()` uses — leaves the receiver boxed, which used to run the synthesized
/// reflection body instead of the eval bridge. That body answers from generated class metadata,
/// which holds nothing about a class the interpreter declared at runtime, so the same reflector
/// that answered `hasMethod()` correctly listed no methods and refused `getMethod()`. `php -n`
/// prints `camelCaseOne,plain|true|camelCaseOne|public`.
#[test]
fn test_boxed_reflector_lists_autoloaded_class_methods() {
    let out = compile_and_run_files(
        &[
            (
                "members.php",
                r#"<?php
class AutoloadedMemberTarget {
    public function camelCaseOne(): int { return 1; }
    public function plain(): int { return 2; }
}
"#,
            ),
            (
                "entry.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    if ($class === 'AutoloadedMemberTarget') {
        require __DIR__ . '/members.php';
    }
});

$name = 'AutoloadedMemberTarget';
class_exists($name);
$reflector = new ReflectionClass($name);
$names = [];
foreach ($reflector->getMethods() as $listed) {
    $names[] = $listed->getName();
}
sort($names);
$found = implode(',', $names) . '|' . ($reflector->hasMethod('camelCaseOne') ? 'true' : 'false');
$reflector = $reflector->getMethod('camelCaseOne');
echo $found . '|' . $reflector->getName() . '|' . ($reflector->isPublic() ? 'public' : 'other');
"#,
            ),
        ],
        "entry.php",
    );
    assert_eq!(out, "camelCaseOne,plain|true|camelCaseOne|public");
}

/// Verifies a reflector built inside one runtime include still reports its target in the next.
///
/// A reflector's target is registered per eval context under the object's runtime identity, so one
/// built inside another eval frame — a runtime `include` here — carried no binding in the context
/// that later reflected on it. Every eval-backed handler declined and the synthesized body
/// answered instead, listing nothing for a class the interpreter declared. `php -n` prints
/// `camelCaseOne,plain|camelCaseOne`.
#[test]
fn test_reflector_built_in_another_eval_frame_keeps_its_target() {
    let out = compile_and_run_files(
        &[
            (
                "members.php",
                r#"<?php
class AutoloadedEvalMemberTarget {
    public function camelCaseOne(): int { return 1; }
    public function plain(): int { return 2; }
}
"#,
            ),
            (
                "reflect.php",
                r#"<?php
return new ReflectionClass($name);
"#,
            ),
            (
                "entry.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    if ($class === 'AutoloadedEvalMemberTarget') {
        require __DIR__ . '/members.php';
    }
});

$name = 'AutoloadedEvalMemberTarget';
class_exists($name);
$reflector = require __DIR__ . '/reflect.php';
$names = [];
foreach ($reflector->getMethods() as $listed) {
    $names[] = $listed->getName();
}
sort($names);
echo implode(',', $names) . '|' . $reflector->getMethod('camelCaseOne')->getName();
"#,
            ),
        ],
        "entry.php",
    );
    assert_eq!(out, "camelCaseOne,plain|camelCaseOne");
}

/// `ReflectionEnum::getCases()`/`getCase()` read the lazily filled `__cases` slot.
///
/// The slot was declared as a plain `array`, which stores a boxed Mixed cell, while its filler
/// stores a raw object array; `getCases()` unboxed the raw pointer and answered "float given",
/// and `getCase()` warned "foreach() argument must be of type array|object" and then threw
/// "Suit::S is not a case". Expected output is `php -n` 8.5's.
#[test]
fn test_reflection_enum_get_cases_reads_the_filled_slot() {
    let out = compile_and_run(
        r#"<?php
enum Suit: string { case H = "h"; case S = "s"; const X = 1; }
$e = new ReflectionEnum(Suit::class);
echo $e->getCase("S")->getName(), ":", count($e->getCases()), ":";
try { $e->getCase("X"); } catch (ReflectionException $error) { echo $error->getMessage(), ":"; }
echo json_encode([(new ReflectionEnum(Suit::class))->getCase("H")]), "\n";
"#,
    );
    assert_eq!(out, "S:2:Suit::X is not a case:[{\"name\":\"H\",\"class\":\"Suit\"}]\n");
}

/// A literal `new ReflectionMethod(Trait::class, ...)` or `(Interface::class, ...)` and an object
/// `new ReflectionClass($anonymous)` are answered from compile-time metadata.
///
/// Every reflector construction reserves an eval context, and a body owning one sent any target
/// that was not a CLASS method to the interpreter, which does not know compiled traits ("Method
/// T::m() does not exist") and lost `isAnonymous()` and `returnsReference()` facts.
#[test]
fn test_reflection_of_trait_interface_and_anonymous_targets_stays_compiled() {
    let out = compile_and_run(
        r#"<?php
trait RefT { public function &get(): array { static $a = []; return $a; } public function Helper(): int { return 1; } }
interface RefI { public function &byRef(int $n): array; }
echo (new ReflectionMethod(RefT::class, "HELPER"))->getName(), ":";
echo (new ReflectionMethod(RefT::class, "get"))->returnsReference() ? "y" : "n";
echo (new ReflectionMethod(RefI::class, "byRef"))->returnsReference() ? "y" : "n";
$anon = new class { public function v(): int { return 1; } };
echo (new ReflectionClass($anon))->isAnonymous() ? "A" : "a";
echo (new ReflectionClass(new class {}))->isAnonymous() ? "A" : "a";
echo "\n";
"#,
    );
    assert_eq!(out, "Helper:yyAA\n");
}

/// Builtin call shapes that refused whole vendor files: `ReflectionClass::getConstants($filter)`
/// (league/uri), `http_build_query()` named arguments (league/uri), `copy()` with a stream
/// context (symfony/asset-mapper), and `array_column()` over enum cases and list rows
/// (symfony/asset-mapper, symfony/type-info).
#[test]
fn test_builtin_signatures_that_refused_vendor_files() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
enum T: string { case A = 'a'; case B = 'b'; }
class K { public const A = 1; protected const B = 2; private const C = 3; }
$r = new ReflectionClass(K::class);
echo implode(',', array_keys($r->getConstants())), '|';
echo implode(',', array_keys($r->getConstants(ReflectionClassConstant::IS_PUBLIC))), '|';
echo implode(',', array_keys($r->getConstants(ReflectionClassConstant::IS_PRIVATE))), "\n";
echo http_build_query(data: ['a' => 1, 'b' => 'x y'], arg_separator: ';', encoding_type: PHP_QUERY_RFC3986), "\n";
$src = tempnam(sys_get_temp_dir(), 'cp');
file_put_contents($src, 'hello');
var_dump(copy($src, $src . '.copy', stream_context_create()));
echo file_get_contents($src . '.copy'), "\n";
unlink($src);
unlink($src . '.copy');
echo implode(',', array_column(T::cases(), 'value')), '|';
echo implode(',', array_column([['p', 1], ['q', 2]], 0)), "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "A,B,C|A|C\na=1;b=x%20y\nbool(true)\nhello\na,b|p,q\n"
    );
}
