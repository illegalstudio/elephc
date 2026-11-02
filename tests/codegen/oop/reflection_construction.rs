//! Purpose:
//! End-to-end codegen tests for ReflectionClass and ReflectionObject
//! construction helpers over reflected class metadata.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Covers inherited ReflectionObject construction helpers that must route
//!   through the same dynamic class-name lowering as ReflectionClass.

use super::*;

/// Verifies the standard ReflectionReference surface accepts an ordinary array element.
#[test]
fn test_reflection_reference_non_reference_element_is_null() {
    let out = compile_and_run(
        r#"<?php
$values = ["plain"];
echo null === ReflectionReference::fromArrayElement($values, 0) ? "null" : "reference";
"#,
    );
    assert_eq!(out, "null");
}

/// Verifies member reflectors accept an object and derive its declaring class at runtime.
#[test]
fn test_reflection_member_constructors_accept_object() {
    let out = compile_and_run_capture(
        r#"<?php
class RuntimeReflectionOwner {
    public const LABEL = "constant";
    public string $value = "property";
    public function run(): void {}
}

$object = new RuntimeReflectionOwner();
echo (new ReflectionMethod($object, "run"))->getName() . ":";
echo (new ReflectionProperty($object, "value"))->getName() . ":";
echo (new ReflectionClassConstant($object, "LABEL"))->getValue();
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "run:value:constant");
}

/// Verifies a parameter DECLARED `?\ReflectionClass` but holding a class-name string is
/// reflected by its value, not folded to its declared class (`ReflectionClass` itself).
#[test]
fn test_reflection_class_of_a_declared_reflection_param_holding_a_string() {
    let out = compile_and_run_capture(
        r#"<?php
final class ProxyNameSource { public function __construct(private string $cls) {} public function getClass(): ?string { return $this->cls; } }
final class ProxyNameDumper {
    public function name(ProxyNameSource $d, ?\ReflectionClass &$class = null): string {
        $class = 'object' !== $d->getClass() ? $d->getClass() : 'stdClass';
        $class = new \ReflectionClass($class);
        return $class->name;
    }
}
function reflected_nullable(?ArrayObject $o, string $fallback): string {
    if ($o === null) { $o = $fallback; }
    return (new ReflectionClass($o))->getName();
}
$c = null;
echo (new ProxyNameDumper())->name(new ProxyNameSource('ArrayObject'), $c), " ", $c->name, " ";
echo reflected_nullable(null, 'Exception'), " ", reflected_nullable(new ArrayObject(), 'x');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "ArrayObject ArrayObject Exception ArrayObject");
}

/// Verifies ReflectionClass resolves a case-insensitive class name held in a runtime string.
#[test]
fn test_reflection_class_accepts_runtime_class_string() {
    let out = compile_and_run_capture(
        r#"<?php
class DynamicReflectionTarget {}
$name = strtolower("DynamicReflectionTarget");
$reflection = new ReflectionClass($name);
echo $reflection->getName();
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "DynamicReflectionTarget");
}

/// Verifies an unknown runtime class name raises a catchable ReflectionException.
#[test]
fn test_reflection_class_runtime_unknown_name_throws() {
    let out = compile_and_run_capture(
        r#"<?php
$name = strtolower("DefinitelyMissing");
try {
    new ReflectionClass($name);
} catch (ReflectionException $error) {
    echo $error->getMessage();
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Class \"definitelymissing\" does not exist");
}

/// Verifies `ReflectionObject` inherits working construction helpers from `ReflectionClass`.
#[test]
fn test_reflection_object_construction_helpers_use_runtime_class() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectObjectConstructBase {}
class ReflectObjectConstructChild extends ReflectObjectConstructBase {
    public function __construct(string $left = "L", string $right = "R") {
        echo $left . $right . "|";
    }
}

$object = new ReflectObjectConstructChild("I", "N");
$ref = new ReflectionObject($object);
$first = $ref->newInstance("A", "B");
echo get_class($first) . ":";
$second = $ref->newInstanceArgs(["right" => "Y", "left" => "X"]);
echo get_class($second) . ":";
$third = $ref->newInstance(right: "N", left: "M");
echo get_class($third) . ":";
$fourth = $ref->newInstanceWithoutConstructor();
echo get_class($fourth);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "IN|AB|ReflectObjectConstructChild:XY|ReflectObjectConstructChild:MN|ReflectObjectConstructChild:ReflectObjectConstructChild"
    );
}

/// Verifies `ReflectionClass` construction helpers support inferred constructor signatures.
#[test]
fn test_reflection_class_construction_helpers_call_inferred_constructor_signature() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectConstructInferredTarget {
    public function __construct($left, $right = "B") {
        echo $left . $right . "|";
    }
}

$ref = new ReflectionClass(ReflectConstructInferredTarget::class);
$ref->newInstance("A", "C");
$ref->newInstanceArgs(["right" => "Y", "left" => "X"]);
$ref->newInstance(right: "N", left: "M");
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "AC|XY|MN|");
}

/// Verifies `newLazyGhost()` bypasses the target constructor, invokes its initializer once, and
/// accepts an initializer that ignores the supplied object argument.
#[test]
fn test_reflection_class_new_lazy_ghost_initializes_constructorless_object() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectionGhostTarget {
    public string $value = "default";
    public function __construct() { echo "ctor"; }
}

$reflection = new ReflectionClass(ReflectionGhostTarget::class);
$ghost = $reflection->newLazyGhost(static function ($object) {
    $object->value = "ready";
});
echo $ghost->value . "|";
echo (int) (bool) $reflection->newLazyGhost(static fn () => null);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "ready|1");
}

/// Verifies `newLazyProxy()` accepts an object factory and returns its usable object result.
#[test]
fn test_reflection_class_new_lazy_proxy_materializes_factory_result() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectionProxyTarget {
    public string $value = "ready";
}

$reflection = new ReflectionClass(ReflectionProxyTarget::class);
$proxy = $reflection->newLazyProxy(static function (): object {
    echo "factory:";
    return new ReflectionProxyTarget();
});
echo $proxy->value;
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "factory:ready");
}

/// Verifies repeated literal constructors allocate independent objects and member arrays.
#[test]
fn test_reflection_literal_materializer_preserves_fresh_runtime_graphs() {
    let out = compile_and_run_capture(
        r#"<?php
class SharedReflectionTarget {
    public function first(): void {}
    public function second(): void {}
}

$first = new ReflectionClass(SharedReflectionTarget::class);
$second = new ReflectionClass(SharedReflectionTarget::class);
$methods = $first->getMethods();
array_pop($methods);
echo ($first === $second ? "same" : "fresh") . ":";
echo count($methods) . ":" . count($second->getMethods());
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "fresh:1:2");
}

/// Verifies repeated identical literal constructors reuse one generated materializer body.
#[test]
fn test_reflection_literal_materializer_reuses_generated_body() {
    let single_dir = make_cli_test_dir("elephc_reflection_materializer_single");
    let repeated_dir = make_cli_test_dir("elephc_reflection_materializer_repeated");
    let single = r#"<?php
class SharedReflectionTarget {
    public function first(): void {}
    public function second(): void {}
}
$reflection = new ReflectionClass(SharedReflectionTarget::class);
echo count($reflection->getMethods());
"#;
    let repeated = r#"<?php
class SharedReflectionTarget {
    public function first(): void {}
    public function second(): void {}
}
$first = new ReflectionClass(SharedReflectionTarget::class);
$second = new ReflectionClass(SharedReflectionTarget::class);
$third = new ReflectionClass(SharedReflectionTarget::class);
echo count($first->getMethods()) + count($second->getMethods()) + count($third->getMethods());
"#;

    let (single_asm, _, _) =
        compile_source_to_asm_with_options(single, &single_dir, 8_388_608, false, false);
    let (repeated_asm, _, _) =
        compile_source_to_asm_with_options(repeated, &repeated_dir, 8_388_608, false, false);
    let _ = fs::remove_dir_all(&single_dir);
    let _ = fs::remove_dir_all(&repeated_dir);

    let materializer_body_count = |asm: &str| {
        asm.lines()
            .filter(|line| {
                line.trim_end().ends_with(':')
                    && line.contains("reflection_materializer_")
                    && !line.contains("reflection_materializer_done_")
            })
            .count()
    };
    let single_body_count = materializer_body_count(&single_asm);
    let repeated_body_count = materializer_body_count(&repeated_asm);
    assert!(single_body_count > 0);
    assert_eq!(repeated_body_count, single_body_count);
    assert!(
        repeated_asm.len() < single_asm.len() + 100_000,
        "repeated literal constructors grew assembly from {} to {} bytes",
        single_asm.len(),
        repeated_asm.len()
    );
}

/// Verifies a SPREAD argument reaches the ordinary two-argument constructor instead of the
/// deprecated one-argument `"Class::method"` form.
///
/// `args.len() == 1` counted a spread as one argument, so `new ReflectionMethod(...$pair)` was
/// sent to the string-name validator and rejected with "first argument must be a string method
/// name" — although the spread expands to `[$object, "method"]` at run time. Symfony's
/// `ControllerEvent::getControllerReflector()` is written exactly this way, and it is one of the
/// last three constructs standing between the generated DI container and an AOT compile.
///
/// The assertion is that the spread form and the written-out form agree; `getName()` is what both
/// are asked for, because that is the part of the reflector Symfony reads.
#[test]
fn test_reflection_method_accepts_a_spread_argument_pair() {
    let out = compile_and_run_capture(
        r#"<?php
class SpreadReflectionOwner {
    public function run(string $a): string { return "ran:" . $a; }
}

$object = new SpreadReflectionOwner();
$pair = [$object, "run"];
$names = ["SpreadReflectionOwner", "run"];

echo (new ReflectionMethod(...$pair))->getName() . ":";
echo (new ReflectionMethod(...$names))->getName() . ":";
echo (new ReflectionMethod($object, "run"))->getName();
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "run:run:run");
}


/// `(string) $attribute` renders what PHP's `ReflectionAttribute::__toString()` renders.
///
/// There was no `__toString` at all, so the cast raised `Object of class ReflectionAttribute
/// could not be converted to string`. Symfony's
/// `Config/Resource/ReflectionClassResource::generateSignature()` writes `[$a->getName(),
/// (string) $a]` for every attribute it tracks, inside `isFresh()` for `url_matching_routes.php`
/// — BEFORE routing — so every route died the moment the app grew a controller with `#[Route]`.
///
/// The rendering is a precomputed `__string` slot, like every other reflection class's. It has to
/// be filled on BOTH materialization paths: the compile-time emitter in
/// `lower_inst::builtins::attributes`, and `__elephc_eval_reflection_attribute_new`, which is the
/// one that actually runs whenever the eval bridge is on — a first fix that filled only the former
/// left the slot empty and returned `""`.
///
/// Covers the shapes that differ: no arguments (one line, no block), a positional and a named
/// argument, and the escaping, which is NOT `var_export`'s — the backslash is escaped, the single
/// quote is not.
///
/// Oracle: `php -n` prints the asserted text.
#[test]
fn test_reflection_attribute_renders_php_tostring() {
    let out = compile_and_run(
        r#"<?php
#[\Attribute(\Attribute::TARGET_ALL)]
class Route {
    public function __construct(public string $path = '/', public ?string $name = null) {}
}
#[Route('/hello', name: 'hello')]
class WithArgs {}
#[Route]
class Bare {}
#[Route("q'uote and \\ back")]
class Escaped {}
foreach ((new ReflectionClass('WithArgs'))->getAttributes() as $a) { echo (string) $a; }
foreach ((new ReflectionClass('Bare'))->getAttributes() as $a) { echo (string) $a; }
foreach ((new ReflectionClass('Escaped'))->getAttributes() as $a) { echo (string) $a; }
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Attribute [ Route ] {\n",
            "  - Arguments [2] {\n",
            "    Argument #0 [ '/hello' ]\n",
            "    Argument #1 [ name = 'hello' ]\n",
            "  }\n}\n",
            "Attribute [ Route ]\n",
            "Attribute [ Route ] {\n",
            "  - Arguments [1] {\n",
            "    Argument #0 [ 'q'uote and \\\\ back' ]\n",
            "  }\n}\n",
        )
    );
}

/// A class constant or global constant as an attribute argument (`#[\Attribute(\Attribute::
/// TARGET_METHOD)]`, symfony/console's `Attribute\Interact`), read through a `ReflectionClass`
/// built from a runtime name. The compiled class metadata marked the whole argument list
/// unsupported, and constructing the reflector died with a bare `eval() runtime failed` -- the
/// Symfony --web worker at boot. The reference now crosses by name and is folded when read.
#[test]
fn test_runtime_named_reflection_reads_constant_attribute_arguments() {
    let out = compile_and_run(
        r#"<?php
const LIMIT = 3;
class K { const V = 7; }
#[\Attribute(\Attribute::TARGET_METHOD)]
class Marker { }
#[\Attribute(K::V)]
class Keyed { }
#[\Attribute(LIMIT)]
class Limited { }
function args(string $name): string {
    $r = new ReflectionClass($name);
    return $r->getName() . json_encode($r->getAttributes()[0]->getArguments());
}
foreach (['Marker', 'Keyed', 'Limited'] as $name) {
    echo args($argc > 5 ? 'X' : $name), "|";
}
if ($argc > 5) { eval('echo 1;'); }
"#,
    );
    assert_eq!(out, "Marker[4]|Keyed[7]|Limited[3]|");
}

/// The built-in `Attribute` class constants fold to their php 8.5 values, alone or combined with
/// `|`, in a program WITHOUT eval. The class-level argument capture had its own copy of the folding
/// that kept `\Attribute::TARGET_CLASS` symbolic; the post-pass found no `Attribute` schema to
/// resolve it against and dropped the whole attribute, so `getAttributes()` was empty for every
/// `#[\Attribute(\Attribute::TARGET_CLASS)]` class -- the way Symfony declares its attributes.
#[test]
fn test_builtin_attribute_flags_fold_alone_and_combined() {
    let out = compile_and_run(
        r#"<?php
#[\Attribute(\Attribute::TARGET_CLASS)]
class A1 { }
#[\Attribute(\Attribute::TARGET_METHOD)]
class A2 { }
#[\Attribute(\Attribute::TARGET_CLASS | \Attribute::IS_REPEATABLE)]
class A3 { }
#[\Attribute(\Attribute::TARGET_ALL ^ \Attribute::TARGET_CONSTANT)]
class A4 { }
foreach (['A1', 'A2', 'A3', 'A4'] as $name) {
    $r = new ReflectionClass($argc > 5 ? 'X' : $name);
    $a = $r->getAttributes();
    echo $name, count($a), count($a) ? json_encode($a[0]->getArguments()) : '', ' ';
}
"#,
    );
    assert_eq!(out, "A11[1] A21[4] A31[129] A41[63] ");
}

/// php runs a lazy ghost's initializer on the object's FIRST USE, so a ghost nothing touches never
/// raises what its initializer would have. Symfony's container hands out such ghosts for services
/// a command never needs; one whose initializer failed (`uri_signer` over an empty
/// `kernel.secret`) aborted `debug:event-dispatcher`, which php completes.
#[test]
fn test_lazy_ghost_whose_initializer_fails_does_not_raise_unless_used() {
    let out = compile_and_run(
        r#"<?php
class Signer {
    public string $secret = '';
    public function __construct(string $secret) {
        if ($secret === '') { throw new InvalidArgumentException('empty secret'); }
        $this->secret = $secret;
    }
}
$r = new ReflectionClass(Signer::class);
$bad = $r->newLazyGhost(function (Signer $g) { $g->__construct(''); });
echo get_class($bad), "|";
$good = $r->newLazyGhost(function (Signer $g) { $g->__construct('k'); });
echo $good->secret;
"#,
    );
    assert_eq!(out, "Signer|k");
}
