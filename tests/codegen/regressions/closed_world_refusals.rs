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
