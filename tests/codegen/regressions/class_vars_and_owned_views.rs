//! Purpose:
//! Regression tests for the native `get_class_vars()` and `compact()`, and for the ownership
//! defects found while building them: a null first store into an empty hash, owned string
//! temporaries viewed by the trim family and `substr`, a branch-promoted hash local returned under
//! `: array`, and fresh results presumed to alias their arguments.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected output was captured from php 8.5 running the same source.

use crate::support::*;

/// `get_class_vars()` reports DEFAULT values (statics included, never their current value),
/// instance properties before static ones, own declarations before inherited ones, and filters
/// non-public properties by the calling scope; an unknown name is a catchable `TypeError`.
#[test]
fn test_get_class_vars_matches_php() {
    let out = compile_and_run(
        r#"<?php
const K = 7;
class P { public $a = 1; protected $b = [2]; private $c = "c"; public static $s = 5; private static $ps = 6; public ?int $n = null; public int $u; public $k = K + 1;
  static function inside() { return get_class_vars(static::class); }
  static function self() { return get_class_vars("P"); } }
class Q extends P { public $q = "q"; public $a = 10; protected $b2 = 3; static function own() { return get_class_vars("Q"); } }
P::$s = 99;
echo json_encode(get_class_vars("Q")), "\n";
echo json_encode(P::inside()), "\n";
echo json_encode(Q::inside()), "\n";
echo json_encode(P::self()), "\n";
echo json_encode(Q::own()), "\n";
$n = "q";
echo json_encode(get_class_vars(strtoupper($n))), "\n";
try { get_class_vars("Nope"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(out, "{\"q\":\"q\",\"a\":10,\"n\":null,\"u\":null,\"k\":8,\"s\":5}\n{\"a\":1,\"b\":[2],\"c\":\"c\",\"n\":null,\"u\":null,\"k\":8,\"s\":5,\"ps\":6}\n{\"q\":\"q\",\"a\":10,\"b2\":3,\"b\":[2],\"c\":\"c\",\"n\":null,\"u\":null,\"k\":8,\"s\":5,\"ps\":6}\n{\"a\":1,\"b\":[2],\"c\":\"c\",\"n\":null,\"u\":null,\"k\":8,\"s\":5,\"ps\":6}\n{\"q\":\"q\",\"a\":10,\"b2\":3,\"b\":[2],\"n\":null,\"u\":null,\"k\":8,\"s\":5}\n{\"q\":\"q\",\"a\":10,\"n\":null,\"u\":null,\"k\":8,\"s\":5}\nget_class_vars(): Argument #1 ($class) must be a valid class name, Nope given\n");
}

/// Eloquent's shape: a bare call inside a namespace on `static::class`, read by key; trait
/// properties count as the using class's own, and a trait answers for its own declarations.
#[test]
fn test_get_class_vars_in_a_namespace_through_static_class() {
    let out = compile_and_run(
        r#"<?php
namespace App\Models;
trait HasStamps { protected $stampFormat = 'Y-m-d'; public static $booted = false; }
abstract class Model {
    use HasStamps;
    public $timestamps = true;
    protected $table = null;
    protected $casts = ['id' => 'int', 'meta' => self::class];
    const KIND = 'model';
    protected $kind = self::KIND . '!';
    public static function usesTimestamps(): bool {
        $class = static::class;
        return get_class_vars($class)['timestamps'];
    }
    public function vars() { return get_class_vars(static::class); }
}
class Post extends Model { public $timestamps = false; protected $table = 'posts'; }
class Tag extends Model {}
var_dump(Post::usesTimestamps(), Tag::usesTimestamps());
echo json_encode(get_class_vars(Post::class)), "\n";
echo json_encode((new Tag())->vars()), "\n";
echo json_encode(get_class_vars('\App\Models\HasStamps')), "\n";
try { get_class_vars("Nope"); } catch (\TypeError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$total = 0;
for ($i = 0; $i < 50; $i++) { $total += count(get_class_vars(Post::class)); }
var_dump($total);
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\n{\"timestamps\":false,\"booted\":false}\n{\"timestamps\":true,\"table\":null,\"casts\":{\"id\":\"int\",\"meta\":\"App\\\\Models\\\\Model\"},\"kind\":\"model!\",\"stampFormat\":\"Y-m-d\",\"booted\":false}\n{\"booted\":false}\nTypeError: get_class_vars(): Argument #1 ($class) must be a valid class name, Nope given\nint(100)\n");
}

/// The declared `get_class_vars` serves callable strings, which answer for the global scope.
#[test]
fn test_get_class_vars_as_a_callable_string() {
    let out = compile_and_run(
        r#"<?php
class P { public $a = 1; protected $b = 2; public static $s = 3; }
echo json_encode(call_user_func('get_class_vars', 'P')), "\n";
echo json_encode(array_map('get_class_vars', ['P'])), "\n";
$f = 'get_class_vars';
echo json_encode($f('P')), "\n";
echo function_exists('get_class_vars') ? "yes" : "no", "\n";
"#,
    );
    assert_eq!(out, "{\"a\":1,\"s\":3}\n[{\"a\":1,\"s\":3}]\n{\"a\":1,\"s\":3}\nyes\n");
}

/// `compact()` with literal names (nested arrays, duplicates, a null value, `\compact`, inside a
/// namespace and a method) builds php's array from the caller's variables.
#[test]
fn test_compact_with_literal_names() {
    let out = compile_and_run(
        r#"<?php
namespace App\Http;

function f() { $a = "x"; $b = [1]; $c = compact("a", "b"); return count($c) . json_encode($c); }
echo f(), "\n";
function g() { $a = "x"; $b = [1]; $n = null; return json_encode(compact(["a", ["b"]], "n", "a")); }
echo g(), "\n";
class Ctl { public function show(int $id) { $title = "T" . $id; $items = [1, 2]; return \compact('title', 'items', 'id'); } }
echo json_encode((new Ctl())->show(7)), "\n";
$top = "t";
echo json_encode(compact('top')), "\n";
"#,
    );
    assert_eq!(out, "2{\"a\":\"x\",\"b\":[1]}\n{\"a\":\"x\",\"b\":[1],\"n\":null}\n{\"title\":\"T7\",\"items\":[1,2],\"id\":7}\n{\"top\":\"t\"}\n");
}

/// `$a[1][] = 7` on `[[1, 2], [3]]` vivifies in one arm of a lowering-time branch; the
/// container's conversion to `array<mixed>` happened only there, so the other arm read raw
/// inner-array pointers as boxed cells and `$a[0]` printed `float(2.0E-323)`.
#[test]
fn test_nested_append_into_an_int_list_keeps_its_siblings() {
    let out = compile_and_run(
        r#"<?php
$a = [[1, 2], [3]];
$a[1][] = 7;
var_dump($a);
function f() { $b = [[1, 2], [3]]; $b[1][] = 7; $b[0][] = 8; return json_encode($b); }
echo f(), "\n";
$c = [["x"], []];
$c[1][] = "y";
$c[0][1] = "z";
echo json_encode($c), "\n";
"#,
    );
    assert_eq!(out, "array(2) {\n  [0]=>\n  array(2) {\n    [0]=>\n    int(1)\n    [1]=>\n    int(2)\n  }\n  [1]=>\n  array(2) {\n    [0]=>\n    int(3)\n    [1]=>\n    int(7)\n  }\n}\n[[1,2,8],[3,7]]\n[[\"x\",\"z\"],[\"y\"]]\n");
}

/// A yielded temporary (`yield new D()`, `yield [new D()]`) is owned by the generator alone, so its
/// destructor runs when the generator moves past it or is destroyed, as in php.
#[test]
fn test_yielded_temporaries_are_destroyed_when_the_generator_lets_go() {
    let out = compile_and_run(
        r#"<?php
class D { public function __construct(public string $n) {} public function __destruct() { echo "[~", $this->n, "]"; } }
function g() { yield new D("a"); yield "k" => new D("b"); yield [new D("c")]; echo "<end>"; }
foreach (g() as $k => $v) { echo is_array($v) ? "arr " : $v->n . " "; }
echo "\n";
$gen = g();
echo $gen->current()->n, "\n";
unset($gen);
echo "\nafter\n";
function h() { $x = new D("x"); yield $x; yield $x; }
foreach (h() as $v) { echo $v->n; }
echo "\n";
"#,
    );
    assert_eq!(out, "a [~a]b [~b]arr <end>\na\n[~a]\nafter\n[~c]xx\n[~x]");
}

/// `$v = []; $v['k'] = null;` types the hash's values as null, which the backend refused to
/// store (`hash_set value PHP type Void`).
#[test]
fn test_first_null_store_into_an_empty_hash() {
    let out = compile_and_run(
        r#"<?php
function f(bool $s, string $k) {
    $vars = [];
    if ($s) { $vars[$k] = null; $vars['b'] = true; $vars['c'] = "x"; }
    else { $vars['z'] = [1]; }
    return $vars;
}
var_dump(f(true, "a"), f(false, "a"));
function g() { $v = []; $v['n'] = null; return $v; }
var_dump(g());
"#,
    );
    assert_eq!(out, "array(3) {\n  [\"a\"]=>\n  NULL\n  [\"b\"]=>\n  bool(true)\n  [\"c\"]=>\n  string(1) \"x\"\n}\narray(1) {\n  [\"z\"]=>\n  array(1) {\n    [0]=>\n    int(1)\n  }\n}\narray(1) {\n  [\"n\"]=>\n  NULL\n}\n");
}

/// Owned string temporaries viewed by `ltrim`/`trim`/`rtrim`/`substr` are released once the view
/// is copied, a returned acquired copy is not persisted twice, a local promoted to a hash in
/// one branch and returned under `: array` releases its boxed slot, and the owned arguments of
/// `str_pad`/`str_repeat`/`date`, whose results never alias them, are released.
#[test]
fn test_owned_string_views_and_branch_promoted_hash_returns_are_leak_free() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f(bool $b): array { $v = []; if ($b) { $v["a"] = 1; } return $v; }
function g(mixed $c): string { return ltrim($c, "x"); }
function h(mixed $c): string { return substr($c, 1); }
function k($c) { return trim($c) . rtrim($c, "x"); }
function p($c) { return str_pad($c, 6, "-") . str_repeat($c, 2) . date("Y", 86400 * 400); }
function run(): void {
    $n = 0;
    $s = "";
    for ($i = 0; $i < 5; $i++) {
        $n += count(f($i % 2 == 0));
        $s = g("xxP") . h("ab") . k(" xPx ") . p("ab");
    }
    echo $n, "|", $s, "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3|PbxPx xPx ab----abab1971\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// `__toString` results inside a concatenation, a loop that rewrites its foreach key in a branch
/// before keying a hash returned under `: array` (`array_combine`'s prelude shape), and the
/// owned arguments of `ucwords`/`wordwrap` are all released.
#[test]
fn test_tostring_concat_branchy_loop_hash_and_word_helpers_are_leak_free() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public function __construct(private string $s) {} public function __toString(): string { return "<" . $this->s . ">"; } }
function keyed(array $keys): array {
    $result = [];
    foreach ($keys as $key) {
        if (!is_int($key) && !is_string($key)) { $key = (string) $key; }
        $result[$key] = strlen($key);
    }
    return $result;
}
function words($s) { return ucwords($s) . "|" . wordwrap($s, 3, "\n", true); }
function run(): void {
    $out = "";
    for ($i = 0; $i < 5; $i++) {
        $t = new T("v" . $i);
        $out = "x" . $t . "|" . count(array_combine(["a", "b"], ["x", "y"])) . "|" . json_encode(keyed(["k", "kk"])) . "|" . words("ab cd");
    }
    echo $out, "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "x<v4>|2|{\"k\":1,\"kk\":2}|Ab Cd|ab\ncd\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Callable invocations release what they own: the boxed argument of a directly called closure
/// that returns its parameter, a closure passed to a function that returns the callback's result,
/// and the box a descriptor invoker builds for a `mixed` parameter from a variable argument.
#[test]
fn test_closure_and_descriptor_invocations_are_leak_free() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function apply(callable $c, $v) { return $c($v); }
function each_key(array $items, callable $cb): string {
    $out = "";
    foreach ($items as $k => $v) { $out .= $cb($v, $k); }
    return $out;
}
function direct(): int { $c = fn($k) => $k; return $c(1) + $c(2); }
function run(): void {
    $r = "";
    for ($i = 0; $i < 5; $i++) {
        $n = $i;
        $r = apply(fn($k) => $k, $n) . "|" . direct() . "|"
            . each_key(["a" => 1, "b" => 2], fn($v, $k) => $k . $v) . "|"
            . apply("strtoupper", "x" . $i);
    }
    echo $r, "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "4|3|a1b2|X4\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// `foreach` over an `IteratorAggregate` (a concrete class, a temporary, an interface-typed
/// parameter, a generator-returning `getIterator()`, a `break`) releases the iterator it asked for,
/// and a temporary aggregate is destroyed before the loop as in php.
#[test]
fn test_foreach_over_iterator_aggregates_releases_the_iterator() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Coll implements IteratorAggregate, Countable {
    public function __construct(private array $items) {}
    public function getIterator(): Iterator { return new ArrayIterator($this->items); }
    public function count(): int { return count($this->items); }
    public function __destruct() { echo "[d", count($this->items), "]"; }
}
class Gen implements IteratorAggregate {
    public function getIterator(): Generator { yield "x" => 1; yield "y" => 2; }
}
function sum(IteratorAggregate $a): int { $s = 0; foreach ($a as $v) { $s += $v; } return $s; }
function run(): void {
    foreach (new Coll(["a" => 1, "b" => 2]) as $k => $v) { echo "$k=$v "; }
    echo "\n";
    $c = new Coll([3, 4]);
    foreach ($c as $v) { echo $v, " "; }
    foreach ($c as $k => $v) { if ($k === 0) { break; } }
    echo count($c), "\n";
    for ($i = 0; $i < 3; $i++) { foreach (new Gen() as $k => $v) { echo "$k$v "; } }
    echo "\n", sum(new Coll([5, 6])), "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "[d2]a=1 b=2 \n3 4 2\nx1 y2 x1 y2 x1 y2 \n[d2]11\n[d2]");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// `iterator_to_array()` releases each `key()` box after the insert copies the key, and generator
/// yields of fresh arrays hand their reference to the generator.
#[test]
fn test_iterator_to_array_and_yielded_arrays_are_leak_free() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function g() { yield "a" => 1; yield 5 => "five"; yield "07" => [7]; }
function run(): void {
    $out = "";
    for ($i = 0; $i < 5; $i++) {
        $out = json_encode(iterator_to_array(new ArrayIterator(["x" . $i => 1, "y" => [2], 3 => "z"])))
            . json_encode(iterator_to_array(g())) . json_encode(iterator_to_array(g(), false));
    }
    echo $out, "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "{\"x4\":1,\"y\":[2],\"3\":\"z\"}{\"a\":1,\"5\":\"five\",\"07\":[7]}[1,\"five\",[7]]\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// `dirname()` and `basename()` return a slice of their path. Over a `string|false` or untyped
/// argument the path is the copy a cast made, which was released BEFORE the result was copied:
/// `dirname()` handed back freed bytes (its first eight overwritten by the allocator), which is
/// how Symfony's `getProjectDir()` lost its project directory. The result is now persisted first,
/// and the cast's copy is still released, so the loop stays leak-free.
#[test]
fn test_dirname_and_basename_of_a_cast_path_are_copied_before_release() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function d($x) { return \dirname($x) . '|' . \basename($x); }
function run(int $argc): void {
    $out = "";
    for ($i = 0; $i < 5; $i++) {
        $p = $argc > 5 ? false : '/some/long/path/' . $i . '/file.php';
        $w = $argc > 5 ? false : 'abcdefghijklmnop.php';
        $out = \dirname($p) . ' ' . \dirname($p, 2) . ' ' . d($p) . ' '
            . \basename($w) . ' ' . \basename($w, '.php') . ' ' . \dirname($w);
    }
    echo $out, "\n";
}
run($argc);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "/some/long/path/4 /some/long/path /some/long/path/4|file.php abcdefghijklmnop.php abcdefghijklmnop .\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// A call's string appended to a declared `array` property is owned by the array afterwards.
///
/// The append boxes a call result by taking the string over, and the property path released it
/// once more after the write: `$this->paths[] = rtrim($path, '/\\')` -- Twig's
/// `FilesystemLoader::addPath()` -- stored freed bytes, so no template was ever found, and the
/// heap debugger reported a double free at exit.
#[test]
fn test_call_string_appended_to_an_array_property_stays_owned_by_the_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class L {
    private array $paths = [];
    public function add(string $path): void { $this->paths[] = rtrim($path, '/\\'); }
    public function add2(string $path): void { $this->paths[] = $path; }
    public function paths(): array { return $this->paths; }
}
$a = new L();
$a->add('/x/y/');
$a->add2('/z');
$a->add(str_repeat('ab', 3) . '/');
echo count($a->paths()), ' ', json_encode($a->paths()), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3 [\"\\/x\\/y\",\"\\/z\",\"ababab\"]\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// The same append one level down, through an untyped property: `$this->paths[$ns][] =
/// rtrim(...)` is Twig's exact `FilesystemLoader::addPath()`. It goes through a hidden Mixed
/// local whose append also took the string over and then released it.
#[test]
fn test_call_string_appended_to_a_nested_untyped_property_element_survives() {
    let out = compile_and_run(
        r#"<?php
class L {
    protected $paths = [];
    public function setPaths($paths, string $namespace = '__main__'): void {
        if (!\is_array($paths)) { $paths = [$paths]; }
        $this->paths[$namespace] = [];
        foreach ($paths as $path) { $this->addPath($path, $namespace); }
    }
    public function addPath(string $path, string $namespace = '__main__'): void {
        $this->paths[$namespace][] = rtrim($path, '/\\');
    }
    public function prependPath(string $path, string $namespace = '__main__'): void {
        $path = rtrim($path, '/\\');
        if (!isset($this->paths[$namespace])) {
            $this->paths[$namespace][] = $path;
        } else {
            array_unshift($this->paths[$namespace], $path);
        }
    }
    public function getPaths(string $namespace = '__main__'): array { return $this->paths[$namespace] ?? []; }
}
$a = new L();
$a->addPath('/tpl/');
$a->addPath(str_repeat('x', 3) . '/', 'ns');
$a->prependPath('/first/');
$a->setPaths(['/s1/', '/s2'], 'other');
echo json_encode($a->getPaths()), ' ', json_encode($a->getPaths('ns')), ' ', json_encode($a->getPaths('other')), "\n";
"#,
    );
    assert_eq!(out, "[\"\\/first\",\"\\/tpl\"] [\"xxx\"] [\"\\/s1\",\"\\/s2\"]\n");
}
