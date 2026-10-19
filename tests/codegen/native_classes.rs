//! Purpose:
//! End-to-end coverage for PHP core class-likes elephc declares itself: the implicit
//! `UnitEnum`/`BackedEnum` interfaces of enums, `WeakMap`, `ErrorException`, the
//! `Serializable`/`SessionHandlerInterface` contracts and the `ReflectionType` parent.
//!
//! Called from:
//! - `cargo test --test codegen_tests native_classes` through Rust's test harness.
//!
//! Key details:
//! - Expected outputs were generated with PHP 8.5.
//! - `WeakMap` and `ErrorException` come from `core_prelude`; these cases exercise the
//!   collection-independent surface.

use crate::support::*;

/// Every enum implements `UnitEnum`, every backed enum also `BackedEnum`, and both work as
/// `instanceof` targets and parameter types.
#[test]
fn test_enums_implement_unit_and_backed_enum() {
    let out = compile_and_run(
        r#"<?php
enum Suit { case Hearts; }
enum Size: string { case S = 's'; }
function u(UnitEnum $e): string { return get_class($e); }
function b(BackedEnum $e): string { return get_class($e); }
function kind(mixed $v): string { return $v instanceof BackedEnum ? 'b' : ($v instanceof UnitEnum ? 'u' : 'p'); }
echo (Suit::Hearts instanceof UnitEnum ? 1 : 0), (Suit::Hearts instanceof BackedEnum ? 1 : 0);
echo (Size::S instanceof UnitEnum ? 1 : 0), (Size::S instanceof BackedEnum ? 1 : 0), "|";
echo u(Suit::Hearts), u(Size::S), b(Size::S), "|";
echo kind('x'), kind(Size::S), kind(Suit::Hearts), kind(new stdClass()), "\n";
"#,
    );
    assert_eq!(out, "1011|SuitSizeSize|pbup\n");
}

/// An enum satisfies the PARENT of an interface it implements, as a class does. This used to be
/// a checker error (`expects A, got E`) because enum interface lists were not transitive.
#[test]
fn test_enum_satisfies_inherited_interface_parameter() {
    let out = compile_and_run(
        r#"<?php
interface A {}
interface B extends A {}
enum E: int implements B { case X = 1; }
function f(A $a): string { return get_class($a); }
echo f(E::X), (E::X instanceof A ? 'y' : 'n'), "\n";
"#,
    );
    assert_eq!(out, "Ey\n");
}

/// Enum behavior that already worked is unchanged: `cases()`, `from()`, `tryFrom()`, `->name`,
/// `->value` and user interfaces with methods.
#[test]
fn test_enum_existing_surface_is_unchanged() {
    let out = compile_and_run(
        r#"<?php
interface HasLabel { public function label(): string; }
enum Size: string implements HasLabel {
    case S = 's'; case L = 'l';
    public function label(): string { return strtoupper($this->value); }
}
function show(HasLabel $h): string { return $h->label(); }
echo count(Size::cases()), Size::from('l')->name, Size::L->value, show(Size::S);
var_dump(Size::tryFrom('x'));
"#,
    );
    assert_eq!(out, "2LlSNULL\n");
}

/// `WeakMap` with object keys: set/get/exists/unset, `count()`, insertion-ordered iteration
/// yielding the key objects, `null` values, and the PHP errors for missing and non-object keys.
#[test]
fn test_weakmap_minimal_semantics() {
    let out = compile_and_run(
        r#"<?php
$m = new WeakMap();
$a = new stdClass(); $b = new stdClass(); $c = new ArrayObject();
$m[$a] = 'A'; $m[$b] = [1, 2]; $m[$c] = 3;
echo count($m), $m[$a], "|";
echo (isset($m[$a]) ? 1 : 0), (isset($m[new stdClass()]) ? 1 : 0), "|";
unset($m[$b]); unset($m[$b]);
echo count($m), "|";
$m[$a] = 'A2';
foreach ($m as $k => $v) { echo get_class($k), '=', $v, ($k === $a ? '!' : ''), ','; }
$m[$b] = null;
echo "|", (isset($m[$b]) ? 1 : 0), ($m->offsetExists($b) ? 1 : 0), count($m), "|";
try { $m[new stdClass()]; } catch (Error $e) { echo get_class($e), "|"; }
try { $m['s'] = 1; } catch (TypeError $e) { echo $e->getMessage(), "|"; }
echo ($m instanceof Countable ? 1 : 0), ($m instanceof ArrayAccess ? 1 : 0), ($m instanceof IteratorAggregate ? 1 : 0), "\n";
"#,
    );
    assert_eq!(
        out,
        "3A|10|2|stdClass=A2!,ArrayObject=3,|003|Error|WeakMap key must be an object|111\n"
    );
}

/// The missing-key `Error` names the class and object id exactly as PHP does.
#[test]
fn test_weakmap_missing_key_message() {
    let out = compile_and_run(
        r#"<?php
$m = new WeakMap();
$k = new stdClass();
try { $m[$k]; } catch (Error $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(out, "Object stdClass#2 not contained in WeakMap\n");
}

/// `ErrorException` carries its severity, defaults it to `E_ERROR`, keeps the inherited
/// message/code/previous surface and can be subclassed and caught as an `Exception`.
#[test]
fn test_error_exception_severity() {
    let out = compile_and_run(
        r#"<?php
$e = new ErrorException("boom", 3, E_WARNING);
echo $e->getMessage(), $e->getCode(), $e->getSeverity(), "|";
$d = new ErrorException("d", previous: $e);
echo $d->getSeverity(), $d->getPrevious()->getMessage(), "|";
$n = new ErrorException("n", 0, E_USER_NOTICE, null, null, $d);
echo $n->getSeverity(), "|";
try { throw new ErrorException("t", 0, E_NOTICE); } catch (ErrorException $x) { echo $x->getSeverity(), "|"; }
try { throw new ErrorException("u"); } catch (Exception $x) { echo $x->getMessage(), "|"; }
class MyErr extends ErrorException {}
$m = new MyErr("m", 1, E_USER_ERROR);
echo $m->getSeverity(), ($m instanceof ErrorException ? 'y' : 'n'), "\n";
"#,
    );
    assert_eq!(out, "boom32|1boom|1024|8|u|256y\n");
}

/// `Serializable` and `SessionHandlerInterface` are implementable contracts.
#[test]
fn test_serializable_and_session_handler_interfaces() {
    let out = compile_and_run(
        r#"<?php
class S implements Serializable {
    public function serialize() { return "x"; }
    public function unserialize(string $data) { }
    public function __serialize(): array { return []; }
    public function __unserialize(array $data): void {}
}
class H implements SessionHandlerInterface {
    public function open(string $path, string $name): bool { return true; }
    public function close(): bool { return true; }
    public function read(string $id): string|false { return "r" . $id; }
    public function write(string $id, string $data): bool { return true; }
    public function destroy(string $id): bool { return true; }
    public function gc(int $max_lifetime): int|false { return 0; }
}
function useHandler(SessionHandlerInterface $h): string|false { return $h->read("1"); }
echo (new S() instanceof Serializable ? 1 : 0), useHandler(new H()), "\n";
"#,
    );
    assert_eq!(out, "1r1\n");
}

/// The Reflection type objects share the `ReflectionType` parent.
#[test]
fn test_reflection_type_is_the_common_parent() {
    let out = compile_and_run(
        r#"<?php
function f(?int $x, string|array $y): string { return "" . $x . count((array) $y); }
function d(?ReflectionType $t): string { return $t === null ? "none" : get_class($t); }
foreach ((new ReflectionFunction('f'))->getParameters() as $p) {
    $t = $p->getType();
    echo ($t instanceof ReflectionType ? 1 : 0), d($t), ",";
}
echo "\n";
"#,
    );
    assert_eq!(out, "1ReflectionNamedType,1ReflectionUnionType,\n");
}

/// Introspection alone must not break the link. `get_declared_classes()` or a literal
/// `class_exists('ErrorException')` used to inject (or keep alive) an Exception subclass whose
/// `parent::__construct` named a builtin constructor nobody emitted
/// (`_method___Exception____u__u_construct` undefined).
#[test]
fn test_introspection_does_not_strand_the_exception_constructor() {
    let out = compile_and_run(
        r#"<?php
class A {}
echo get_declared_classes() ? "x" : "y";
echo class_exists('WeakMap') ? 1 : 0, class_exists('ErrorException') ? 1 : 0, "\n";
"#,
    );
    assert_eq!(out, "x11\n");
}

/// A user Exception subclass kept alive only by introspection (no `new`) links too: its
/// `parent::__construct` now registers the builtin constructor as reachable.
#[test]
fn test_parent_constructor_of_introspected_exception_subclass_links() {
    let out = compile_and_run(
        r#"<?php
class X extends Exception { public function __construct() { parent::__construct("a"); } }
echo class_exists('X') ? "hi" : "no", "\n";
"#,
    );
    assert_eq!(out, "hi\n");
}

/// Iteration walks the LIVE insertion order like PHP: an entry added inside the loop is visited,
/// one removed before the cursor reaches it is skipped, and a re-added key goes to the end.
#[test]
fn test_weakmap_iteration_sees_live_modifications() {
    let out = compile_and_run(
        r#"<?php
$m = new WeakMap(); $a = new stdClass; $b = new stdClass; $c = new stdClass;
$m[$a] = 1; $m[$b] = 2;
foreach ($m as $k => $v) { echo $v, ","; if ($v === 1) { $m[$c] = 3; unset($m[$b]); } }
echo "|", count($m), "|";
unset($m[$a]); $m[$a] = 4;
foreach ($m as $k => $v) { echo $v, ","; }
echo "\n";
"#,
    );
    assert_eq!(out, "1,3,|2|3,4,\n");
}

/// `serialize()`/`unserialize()` of a WeakMap throw PHP's exceptions, and a `null` key is the
/// same `TypeError` as any other non-object key.
#[test]
fn test_weakmap_serialization_and_null_key_errors() {
    let out = compile_and_run(
        r#"<?php
$m = new WeakMap(); $m[new stdClass] = 1;
try { echo serialize($m); } catch (Exception $e) { echo $e->getMessage(), "|"; }
try { unserialize('O:7:"WeakMap":0:{}'); } catch (Exception $e) { echo $e->getMessage(), "|"; }
try { $m[null] = 1; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "Serialization of 'WeakMap' is not allowed|Unserialization of 'WeakMap' is not allowed|WeakMap key must be an object\n"
    );
}

/// Internal Core prelude classes (`__ElephcWeakMapIterator`) stay out of
/// `get_declared_classes()`, so instantiating every listed class does not reach them.
#[test]
fn test_declared_classes_hide_internal_core_prelude_classes() {
    let out = compile_and_run(
        r#"<?php
$map = new WeakMap();
foreach (get_declared_classes() as $class) {
    if (stripos($class, '__elephc') === 0) {
        echo "internal: ", $class, "\n";
    }
}
echo in_array('WeakMap', get_declared_classes(), true) ? "WeakMap listed\n" : "WeakMap missing\n";
"#,
    );
    assert_eq!(out, "WeakMap listed\n");
}
