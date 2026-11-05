//! Purpose:
//! Regression tests for programs elephc refused while php compiles and runs them, found by rooting
//! every class of the Symfony Demo's `vendor/` in a closed world.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each expected output is what local php prints.

use crate::support::*;

/// A class declaring a method itself owes no `insteadof` for the traits that also bring it:
/// symfony's `DnsResolvingHttpClient` gets `withOptions()` from two of its traits.
#[test]
fn test_a_method_the_class_declares_settles_a_trait_conflict() {
    let out = compile_and_run(
        r#"<?php
trait A { public function m(): string { return 'a'; } }
trait B { public function m(): string { return 'b'; } }
final class Own { use A; use B; public function m(): string { return 'own'; } }
trait Both { use A; use B; public function m(): string { return 'trait'; } }
final class ViaTrait { use Both; }
echo (new Own)->m(), ' ', (new ViaTrait)->m(), "\n";
"#,
    );
    assert_eq!(out, "own trait\n");
}

/// The object behind an interface may declare `__toString()`: php converts it where a `string` is
/// declared, which symfony's `CommandTester` does with an `InputInterface`, and league/uri with a
/// `Stringable|string` parameter.
#[test]
fn test_an_interface_argument_may_convert_to_a_string_parameter() {
    let out = compile_and_run(
        r#"<?php
interface I {}
class S implements I { public function __toString(): string { return 'stringable'; } }
class P { public function __construct(public readonly string $v) {} }
function plain(I $i): string { return (new P($i))->v; }
function either(\Stringable|string $s): string { return (string) $s; }
function viaUnion(I $i): string { return either($i); }
echo plain(new S), ' ', viaUnion(new S), "\n";
"#,
    );
    assert_eq!(out, "stringable stringable\n");
}

/// `RecursiveIteratorIterator` hands a method it lacks to the iterator of the current level, and
/// answers php's own `Error` when that one lacks it too.
#[test]
fn test_recursive_iterator_iterator_forwards_to_the_inner_iterator() {
    let out = compile_and_run(
        r#"<?php
$dir = sys_get_temp_dir() . '/elephc_forward_' . getmypid();
mkdir($dir . '/sub', 0777, true);
touch($dir . '/sub/f.txt');
$names = [];
foreach ($it = new RecursiveIteratorIterator(new RecursiveDirectoryIterator($dir, RecursiveDirectoryIterator::SKIP_DOTS), RecursiveIteratorIterator::SELF_FIRST) as $item) {
    $names[] = $it->getSubPathname();
}
sort($names);
echo implode(',', $names), "\n";
try { $it->nope(); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
unlink($dir . '/sub/f.txt'); rmdir($dir . '/sub'); rmdir($dir);
"#,
    );
    assert_eq!(out, "sub,sub/f.txt\nError: Call to undefined method RecursiveIteratorIterator::nope()\n");
}

/// In LEAVES_ONLY an element with children is no leaf, also at the maximum depth.
#[test]
fn test_recursive_iterator_iterator_max_depth_skips_what_it_cannot_enter() {
    let out = compile_and_run(
        r#"<?php
$tree = new RecursiveIteratorIterator(new RecursiveArrayIterator([1, [2, [3]], 4]));
$tree->setMaxDepth(1);
echo $tree->getMaxDepth(), ':', implode(',', iterator_to_array($tree, false)), "\n";
$self = new RecursiveIteratorIterator(new RecursiveArrayIterator([1, [2, [3]]]), RecursiveIteratorIterator::CHILD_FIRST);
$self->setMaxDepth(1);
echo count(iterator_to_array($self, false)), "\n";
"#,
    );
    assert_eq!(out, "1:1,2,4\n4\n");
}

/// `match (true)` narrows an `instanceof` arm like a ternary: nette's `Iterables::toIterator()` calls
/// a method on an `iterable` parameter there.
#[test]
fn test_match_true_arm_narrows_an_iterable_to_its_instanceof() {
    let out = compile_and_run(
        r#"<?php
class Pack implements IteratorAggregate {
    public function getIterator(): Iterator { return new ArrayIterator([1, 2]); }
}
class Plain implements Iterator {
    private int $i = 0;
    public function current(): mixed { return 'p' . $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 1; }
}
function toIterator(iterable $iterable): Iterator {
    return match (true) {
        $iterable instanceof Iterator => $iterable,
        $iterable instanceof IteratorAggregate => toIterator($iterable->getIterator()),
        is_array($iterable) => new ArrayIterator($iterable),
        default => throw new LogicException('no'),
    };
}
echo implode(',', iterator_to_array(toIterator(new Pack))), "\n";
echo implode(',', iterator_to_array(toIterator(new Plain))), "\n";
echo implode(',', iterator_to_array(toIterator([7, 8]))), "\n";
"#,
    );
    assert_eq!(out, "1,2\np0\n7,8\n");
}

/// A closure is lowered once per scope it may be bound to. `self::` in the copy for a class without
/// the method is php's run-time `Error`, not a reason to refuse the program.
#[test]
fn test_a_closure_copy_for_a_scope_without_the_method_still_compiles() {
    let out = compile_and_run(
        r#"<?php
class Host {
    private static function only(): string { return 'only'; }
    public function make(): Closure { return function (): string { return self::only() . get_class($this); }; }
}
class Other {}
function rebind(callable $closure): callable { return Closure::bind($closure, new Other, Other::class); }
$f = (new Host)->make();
echo ($argc > 5 ? rebind($f) : $f)(), "\n";
"#,
    );
    assert_eq!(out, "onlyHost\n");
}

/// A class declaring `bindTo()` (symfony's `HttpOptions`) must not turn `$closure->bindTo()` into a
/// call on null: php decides on the object the `mixed` receiver holds.
#[test]
fn test_bind_to_on_a_mixed_receiver_splits_closures_from_objects() {
    let out = compile_and_run(
        r#"<?php
class Options {
    public string $to = '';
    public function bindTo(string $bindto, ?string $unused = null): static { $this->to = $bindto; return $this; }
}
class Target { private int $secret = 5; }
function make($class) {
    static $base = null;
    $base ??= static function ($o) { return $o->secret; };
    return $base->bindTo(null, $class);
}
function bindEither($value, $first, $second) { return $value->bindTo($first, $second); }
echo make('Target')(new Target), "\n";
echo bindEither(new Options, 'addr', 'x')->to, "\n";
echo bindEither(make('Target'), null, 'Target')(new Target), "\n";
"#,
    );
    assert_eq!(out, "5\naddr\n5\n");
}

/// A closure bound to no scope reaches `self::` only as php's run-time `Error`.
#[test]
fn test_a_closure_copy_without_a_scope_still_compiles() {
    let out = compile_and_run(
        r#"<?php
class Host {
    private static function only(): string { return 'only'; }
    public function make(): Closure { return function (): string { return self::only() . get_class($this); }; }
}
function rebind(callable $closure): callable { return Closure::bind($closure, null, null); }
$f = (new Host)->make();
echo ($argc > 5 ? rebind($f) : $f)(), "\n";
"#,
    );
    assert_eq!(out, "onlyHost\n");
}

/// `cmp` takes a 12-bit immediate, and vendor/ alone has more classes than that.
#[test]
fn test_dynamic_new_compares_class_ids_past_the_cmp_immediate() {
    let classes: String = (0..4_200)
        .map(|index| format!("class C{index} extends Base {{ public function name(): string {{ return 'C{index}'; }} }}\n"))
        .collect();
    let source = format!(
        r#"<?php
class Base {{ public function __construct(public int $v = 0) {{}} }}
{classes}
function make(string $name, int $v): Base {{
    if (!is_subclass_of($name, Base::class)) {{ throw new LogicException($name); }}
    return new $name($v);
}}
foreach (['C5', 'C4199'] as $name) {{ $o = make($name, 3); echo get_class($o), ' ', $o->v, "\n"; }}
"#
    );
    assert_eq!(compile_and_run(&source), "C5 3\nC4199 3\n");
}

/// Strings occupy 16-byte slots: `array_unshift()` keeps source order, copy-on-write and the count,
/// on an empty array too. `DumpDataCollector::__destruct()` prepends to `headers_list()`.
#[test]
fn test_array_unshift_prepends_strings() {
    let out = compile_and_run(
        r#"<?php
function prefixed(array $h): array { array_unshift($h, 'x' . count($h)); return $h; }
$a = ['b', 'c'];
$copy = $a;
echo array_unshift($a, 'a'), ' ', implode(',', $a), ' ', implode(',', $copy), "\n";
echo array_unshift($a, 'y', 'z'), ' ', implode(',', $a), "\n";
$grown = [];
for ($i = 0; $i < 40; $i++) { $grown[] = "s$i"; }
for ($i = 0; $i < 40; $i++) { array_unshift($grown, "p$i"); }
echo count($grown), ' ', $grown[0], ' ', $grown[39], ' ', $grown[40], ' ', $grown[79], "\n";
$empty = headers_list();
array_unshift($empty, 'only');
echo implode(',', $empty), ' ', implode(',', prefixed(['k'])), "\n";
"#,
    );
    assert_eq!(out, "3 a,b,c b,c\n5 y,z,a,b,c\n80 p39 p0 s0 s39\nonly x1,k\n");
}
