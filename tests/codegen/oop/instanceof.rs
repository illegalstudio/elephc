//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP instanceof, including instanceof classes and unknown target, instanceof inheritance and interfaces, and instanceof self parent and late static.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests instanceof with known class names, unknown class names, and non-object LHS.
#[test]
fn test_instanceof_classes_and_unknown_target() {
    let out = compile_and_run(
        r#"<?php
class A {}
class B {}
$a = new A();
echo ($a instanceof A) ? "T" : "F";
echo ($a instanceof B) ? "T" : "F";
echo (42 instanceof A) ? "T" : "F";
echo ($a instanceof Missing) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TFFF");
}

/// Verifies an unresolved target gives the true branch dynamic access without inventing a class.
#[test]
fn test_unknown_instanceof_target_uses_dynamic_true_branch_type() {
    let out = compile_and_run(
        r#"<?php
class Candidate {
    public static int $checks = 0;

    public static function make(): Candidate {
        self::$checks++;
        return new Candidate();
    }
}

function inspect(Candidate $value): void {
    if ($value instanceof Missing) {
        echo $value->label;
    }
}

inspect(new Candidate());
if (Candidate::make() instanceof Missing) {
    echo "bad";
}
$candidate = new Candidate();
echo ($candidate instanceof Missing) ? $candidate->missing() : "ok";
echo Candidate::$checks;
"#,
    );
    assert_eq!(out, "ok1");
}

/// Tests instanceof with class inheritance hierarchies and interface implementations.
#[test]
fn test_instanceof_inheritance_and_interfaces() {
    let out = compile_and_run(
        r#"<?php
interface Named {
    public function name();
}

interface Entity extends Named {
    public function id();
}

class Base {}

class User extends Base implements Entity {
    public function name() { return "user"; }
    public function id() { return 1; }
}

$user = new User();
$base = new Base();
echo ($user instanceof User) ? "T" : "F";
echo ($user instanceof Base) ? "T" : "F";
echo ($user instanceof Entity) ? "T" : "F";
echo ($user instanceof Named) ? "T" : "F";
echo ($base instanceof User) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TTTTF");
}

/// Tests instanceof with self, static, and parent keywords in class methods.
#[test]
fn test_instanceof_self_parent_and_late_static() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function check(Base $x) {
        echo ($x instanceof self) ? "S" : "s";
        echo ($x instanceof static) ? "T" : "t";
    }
}

class Child extends Base {
    public function checkParent(Base $x) {
        echo ($x instanceof parent) ? "P" : "p";
    }
}

$base = new Base();
$child = new Child();
$base->check($child);
$child->check($base);
$child->checkParent($child);
"#,
    );
    assert_eq!(out, "STStP");
}

/// Verifies that flow narrowing resolves relative `instanceof` targets before member access.
#[test]
fn test_instanceof_self_and_parent_narrow_to_concrete_classes() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public string $value = "B";

    public function fromSelf($candidate): string {
        if ($candidate instanceof self) {
            return $candidate->value;
        }

        return "x";
    }
}

class Child extends Base {
    public function fromParent($candidate): string {
        if ($candidate instanceof parent) {
            return $candidate->value;
        }

        return "x";
    }
}

$child = new Child();
echo $child->fromSelf($child);
echo $child->fromParent($child);
"#,
    );
    assert_eq!(out, "BB");
}

/// Verifies that the native `object` pseudo-type dispatches methods by runtime class.
#[test]
fn test_generic_object_parameter_method_dispatch() {
    let out = compile_and_run(
        r#"<?php
final class NamedObject {
    public function name(): string {
        return "ok";
    }
}

function invokeName(object $value): string {
    return $value->name();
}

echo invokeName(new NamedObject());
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that the LHS object expression is evaluated exactly once by calling a
/// factory method with observable side effects.
#[test]
fn test_instanceof_lhs_evaluates_once() {
    let out = compile_and_run(
        r#"<?php
class Item {}

class Factory {
    public $count = 0;

    public function make() {
        $this->count = $this->count + 1;
        return new Item();
    }
}

$factory = new Factory();
echo ($factory->make() instanceof Item) ? "T" : "F";
echo $factory->count;
"#,
    );
    assert_eq!(out, "T1");
}

/// Tests instanceof against mixed-type values and nullable object return types.
#[test]
fn test_instanceof_handles_mixed_and_nullable_object_values() {
    let out = compile_and_run(
        r#"<?php
interface Named {}
class User implements Named {}

function id(mixed $value): mixed {
    return $value;
}

function maybe(bool $flag): ?User {
    if ($flag) {
        return new User();
    }
    return null;
}

$mixedObject = id(new User());
$mixedScalar = id(7);
echo ($mixedObject instanceof User) ? "T" : "F";
echo ($mixedObject instanceof Named) ? "T" : "F";
echo ($mixedScalar instanceof User) ? "T" : "F";
echo (maybe(true) instanceof User) ? "T" : "F";
echo (maybe(false) instanceof User) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TTFTF");
}

/// Tests dynamic instanceof with string variables naming classes and interfaces,
/// including case-insensitive lookup and absolute names.
#[test]
fn test_dynamic_instanceof_string_class_and_interface_targets() {
    let out = compile_and_run(
        r#"<?php
interface Named {}
class User implements Named {}
class Other {}

$user = new User();
$className = "User";
$interfaceName = "Named";
$otherName = "Other";
$lowerName = "user";
$absoluteName = "\\User";
$missing = "Missing";

echo ($user instanceof $className) ? "T" : "F";
echo ($user instanceof $interfaceName) ? "T" : "F";
echo ($user instanceof $otherName) ? "T" : "F";
echo ($user instanceof $lowerName) ? "T" : "F";
echo ($user instanceof $absoluteName) ? "T" : "F";
echo ($user instanceof $missing) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TTFTTF");
}

/// Tests dynamic instanceof with string variables naming namespaced classes.
#[test]
fn test_dynamic_instanceof_namespaced_string_targets() {
    let out = compile_and_run(
        r#"<?php
namespace App;
class User {}

$user = new User();
$localName = "User";
$qualifiedName = "App\\User";
$absoluteName = "\\App\\User";

echo ($user instanceof $localName) ? "T" : "F";
echo ($user instanceof $qualifiedName) ? "T" : "F";
echo ($user instanceof $absoluteName) ? "T" : "F";
"#,
    );
    assert_eq!(out, "FTT");
}

/// Tests dynamic instanceof with a string naming an interface that the object
/// implements transitively through a child interface.
#[test]
fn test_dynamic_instanceof_transitive_interface_string_target() {
    let out = compile_and_run(
        r#"<?php
interface Root {}
interface Child extends Root {}
class User implements Child {}

$user = new User();
$target = "Root";
echo ($user instanceof $target) ? "T" : "F";
"#,
    );
    assert_eq!(out, "T");
}

/// Tests that when the target of dynamic instanceof is an object (not a string),
/// the runtime class of the target object is used for the check.
#[test]
fn test_dynamic_instanceof_object_target_uses_target_runtime_class() {
    let out = compile_and_run(
        r#"<?php
class A {}
class B {}

$a = new A();
$targetA = new A();
$targetB = new B();

echo ($a instanceof $targetA) ? "T" : "F";
echo ($a instanceof $targetB) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TF");
}

/// Tests dynamic instanceof with mixed-type LHS and targets, including null targets
/// from nullable return types.
#[test]
fn test_dynamic_instanceof_mixed_targets_and_scalar_lhs() {
    let out = compile_and_run(
        r#"<?php
class User {}
class Other {}

function id(mixed $value): mixed {
    return $value;
}

function maybe(bool $flag): ?User {
    if ($flag) {
        return new User();
    }
    return null;
}

$target = id("User");
$missingTarget = id("Missing");
$objectTarget = id(new Other());
$object = id(new User());
$scalar = id(7);

echo ($object instanceof $target) ? "T" : "F";
echo ($scalar instanceof $target) ? "T" : "F";
echo ($scalar instanceof $missingTarget) ? "T" : "F";
echo ($scalar instanceof $objectTarget) ? "T" : "F";
echo (maybe(true) instanceof $target) ? "T" : "F";
echo (maybe(false) instanceof $target) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TFFFTF");
}

/// Tests dynamic instanceof where the target is a parenthesized string concatenation.
#[test]
fn test_dynamic_instanceof_parenthesized_expression_target() {
    let out = compile_and_run(
        r#"<?php
class User {}
$user = new User();
$prefix = "Us";
$suffix = "er";
echo ($user instanceof ($prefix . $suffix)) ? "T" : "F";
"#,
    );
    assert_eq!(out, "T");
}

/// Tests dynamic instanceof where the target is a parenthesized class constant expression.
#[test]
fn test_dynamic_instanceof_parenthesized_class_constant_target() {
    let out = compile_and_run(
        r#"<?php
class User {}
$user = new User();
echo ($user instanceof (User::class)) ? "T" : "F";
"#,
    );
    assert_eq!(out, "T");
}

/// Tests a `$this` property as a dynamic class target used by metadata-processing code.
#[test]
fn test_dynamic_instanceof_this_property_target() {
    let out = compile_and_run(
        r#"<?php
class DynamicTargetUser {}
class DynamicTargetHolder {
    public object $target;
    public function __construct(object $target) { $this->target = $target; }
    public function matches(object $value): bool {
        return $value instanceof $this->target;
    }
}
$holder = new DynamicTargetHolder(new DynamicTargetUser());
echo $holder->matches(new DynamicTargetUser()) ? "T" : "F";
"#,
    );
    assert_eq!(out, "T");
}

/// Tests that dynamic instanceof with a non-string, non-object target (integer) fails
/// with a Fatal error when the LHS is an object.
#[test]
fn test_dynamic_instanceof_invalid_target_fails_for_object_lhs() {
    let out = compile_and_run_capture(
        r#"<?php
class User {}
$user = new User();
$target = 42;
echo ($user instanceof $target) ? "T" : "F";
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded: {}", out.stdout);
    assert!(
        out.stderr
            .contains("Fatal error: Class name must be a valid object or a string"),
        "unexpected stderr: {}",
        out.stderr
    );
}

/// Verifies that when an invalid target causes a Fatal error, both the LHS and
/// the target expression are evaluated in source order before the error is raised.
#[test]
fn test_dynamic_instanceof_invalid_target_evaluates_lhs_then_target() {
    let out = compile_and_run_capture(
        r#"<?php
function lhs(): int {
    echo "L";
    return 7;
}

function rhs(): int {
    echo "R";
    return 42;
}

echo (lhs() instanceof (rhs())) ? "T" : "F";
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded: {}", out.stdout);
    assert_eq!(out.stdout, "LR");
    assert!(
        out.stderr
            .contains("Fatal error: Class name must be a valid object or a string"),
        "unexpected stderr: {}",
        out.stderr
    );
}

/// Tests that dynamic instanceof with a null target (from a nullable return) fails
/// with a Fatal error.
#[test]
fn test_dynamic_instanceof_null_object_target_fails() {
    let out = compile_and_run_capture(
        r#"<?php
class User {}

function maybe(bool $flag): ?User {
    if ($flag) {
        return new User();
    }
    return null;
}

$user = new User();
$target = maybe(false);
echo ($user instanceof $target) ? "T" : "F";
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded: {}", out.stdout);
    assert!(
        out.stderr
            .contains("Fatal error: Class name must be a valid object or a string"),
        "unexpected stderr: {}",
        out.stderr
    );
}

/// Tests that dynamic instanceof with an invalid target (integer) fails with a Fatal
/// error when the LHS is a scalar.
#[test]
fn test_dynamic_instanceof_invalid_target_fails_for_scalar_lhs() {
    let out = compile_and_run_capture(
        r#"<?php
$value = 7;
$target = 42;
echo ($value instanceof $target) ? "T" : "F";
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded: {}", out.stdout);
    assert!(
        out.stderr
            .contains("Fatal error: Class name must be a valid object or a string"),
        "unexpected stderr: {}",
        out.stderr
    );
}

/// A guard whose target the receiver's own class cannot satisfy must keep the receiver's
/// properties: `instanceof` is an intersection, not a replacement.
///
/// Symfony's `CompiledUrlMatcherTrait::match()` is the shape. It guards
/// `if (!$this instanceof RedirectableUrlMatcherInterface) { throw … }` and then reads
/// `$this->context->getScheme()`. `CompiledUrlMatcher` does NOT implement that interface -- a
/// subclass does, which is exactly why the guard is written -- so narrowing `$this` to the
/// interface threw the inherited property away and the backend refused the call with
/// `method call receiver for PHP type Int`. The closed world names the subclass that satisfies
/// both halves instead.
#[test]
fn test_sibling_interface_guard_keeps_inherited_properties() {
    let out = compile_and_run(
        r#"<?php
class Ctx {
    public function getMethod(): string { return 'GET'; }
    public function getScheme(): string { return 'https'; }
}

interface Redirectable {
    public function redirect(string $path, string $route): array;
}

class Base {
    public function __construct(protected Ctx $context) {}
}

trait MatcherTrait {
    public function match(string $p): string
    {
        if (!$this instanceof Redirectable) {
            throw new RuntimeException('no');
        }
        // Two reads, because only the SECOND one failed: the first was served by a type the
        // checker had recorded for that exact span, and nothing recorded the second.
        $method = $this->context->getMethod();
        $scheme = $this->context->getScheme();
        return $method . ':' . $scheme . ':' . count($this->redirect($p, 'r'));
    }
}

class Compiled extends Base {
    use MatcherTrait;
}

class RedirectableCompiled extends Compiled implements Redirectable {
    public function redirect(string $path, string $route): array
    {
        return ['_route' => $route, 'path' => $path];
    }
}

echo (new RedirectableCompiled(new Ctx()))->match('/x');
"#,
    );
    assert_eq!(out, "GET:https:2");
}


/// The same guard reaching the LEXICAL scope's private method, whose trailing parameters are
/// optional and by-reference.
///
/// Three passes resolve members independently and all three had to agree. The checker refused the
/// call outright (`unknown method RedirectableCompiled::doMatch`); once it resolved, the IR
/// lowering still found no signature for it, so the two omitted optional arguments were never
/// materialized and codegen reported `2 operands for 4 ABI params`. Symfony's
/// `CompiledUrlMatcherTrait` writes exactly this: `private function doMatch(string $pathinfo,
/// array &$allow = [], array &$allowSchemes = [])`, called as `$this->doMatch($pathinfo)`.
#[test]
fn test_sibling_interface_guard_reaches_the_lexical_scopes_private_method() {
    let out = compile_and_run(
        r#"<?php
interface Redirectable {
    public function redirect(string $path, string $route): array;
}

trait MatcherTrait {
    private bool $matchHost = true;

    private function doMatch(string $p, array &$allow = [], array &$allowSchemes = []): array
    {
        $allow[] = 'GET';
        $allowSchemes[] = 'https';
        return ['p' => $p, 'n' => count($allow) + count($allowSchemes)];
    }

    public function match(string $p): string
    {
        if (!$this instanceof Redirectable) {
            throw new RuntimeException('no');
        }
        $ret = $this->doMatch($p);
        $host = $this->matchHost ? 'H' : 'h';
        return $ret['p'] . '/' . $ret['n'] . ':' . $host . ':' . count($this->redirect($p, 'r'));
    }
}

class Compiled {
    use MatcherTrait;
}

class RedirectableCompiled extends Compiled implements Redirectable {
    public function redirect(string $path, string $route): array
    {
        return ['_route' => $route, 'path' => $path];
    }
}

echo (new RedirectableCompiled())->match('/x');
"#,
    );
    assert_eq!(out, "/x/2:H:2");
}

/// `instanceof` on a local that holds a scalar leaves its type alone on the (dead) true edge.
///
/// Twig compiles `{{ ... ? ' has-error' : '' }}` over a boolean to
/// `($tmp = !$x) && $tmp instanceof Markup ? (string) $tmp : $tmp`; narrowing the `bool` slot to
/// `Markup` made the backend refuse the read ("local load from PHP type Bool as Object"), which
/// failed every Twig form theme block. Expected output is php 8.5's.
#[test]
fn test_instanceof_on_a_scalar_local_keeps_its_storage_type() {
    let out = compile_and_run(
        r#"<?php
class Markup { public function __toString(): string { return "m"; } }
function f($c) { return ((($tmp = !($c["valid"] ?? null)) && $tmp instanceof Markup ? (string) $tmp : $tmp)) ? " has-error" : ""; }
var_dump(f(["valid" => $argc > 5]), f(["valid" => true]));
function g($n) { $t = $n + 1; return $t instanceof Markup ? (string) $t : $t; }
var_dump(g($argc));
"#,
    );
    assert_eq!(out, "string(10) \" has-error\"\nstring(0) \"\"\nint(2)\n");
}

/// `instanceof Closure` recognizes a compiled closure inside a function that owns an eval-bridge
/// context (here through `new ReflectionClass()` over a runtime class name), where the bridge
/// alone sees no object and used to answer false.
#[test]
fn test_instanceof_closure_in_function_with_eval_context() {
    let out = compile_and_run(
        r#"<?php
function build($concrete) {
    if ($concrete instanceof Closure) {
        try {
            return $concrete();
        } finally {
            echo "F";
        }
    }
    try {
        $reflector = new ReflectionClass($concrete);
    } catch (ReflectionException $e) {
        return 'missing';
    }
    return $reflector->getName();
}
echo build(fn () => 'closure'), "|", build('stdClass'), "|", build('NoSuchClassHere'), "\n";
"#,
    );
    assert_eq!(out, "Fclosure|stdClass|missing\n");
}

/// The class operand of `instanceof` may be any variable expression holding a class name:
/// a property of `$this`, an array element read through it, or a static property. The
/// property form failed with `Expected class or interface name after 'instanceof'`, and the
/// static form parsed only `self` and left `::$fallback` behind. Regression for #829.
#[test]
fn test_instanceof_accepts_property_element_and_static_property_targets() {
    let out = compile_and_run(
        r#"<?php
interface Shape {}
final class Square implements Shape {}
final class Circle implements Shape {}

final class Rule
{
    public static string $fallback = '';
    public array $classes = [];

    public function __construct(public string $className) { $this->classes['sq'] = Square::class; }

    public function test(mixed $value): bool
    {
        return $value instanceof $this->className;
    }

    public function testKey(mixed $value, string $key): bool
    {
        return $value instanceof $this->classes[$key];
    }

    public function testStatic(mixed $value): bool
    {
        return $value instanceof self::$fallback;
    }
}

Rule::$fallback = Circle::class;
$rule = new Rule(Shape::class);
var_dump($rule->test(new Square()));
var_dump($rule->test("not an object"));
$square = new Rule(Square::class);
var_dump($square->test(new Circle()));
var_dump($square->testKey(new Square(), 'sq'));
var_dump($square->testStatic(new Circle()));
var_dump(!$rule->test(new Circle()) || $rule->test(new Square()));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(false)\n",
            "bool(false)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
        )
    );
}
