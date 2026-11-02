//! Purpose:
//! Regressions for the defects the PR #2205 review confirmed, each pinned to php 8.5's output and
//! a clean heap.
//!
//! Called from:
//! - `tests/codegen/regressions.rs`.
//!
//! Key details:
//! - Every expected output is the local `php -n` 8.5 output of the same program.
//! - The eval programs pass their code through a variable, which is the dynamic eval path.

use crate::support::*;

/// Eval `unserialize()` honours php's nesting limit: the `unserialize_max_depth` default of 4096,
/// the `max_depth` option (0 unbounded), and php's TypeError/ValueError for a bad option. A
/// 200000-deep payload used to overflow the Rust stack (SIGSEGV) where php returns false, and every
/// array built before a failure leaked.
#[test]
fn test_eval_unserialize_bounds_nesting_like_php() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$code = <<<'PHP'
function nest(int $n): string { return str_repeat('a:1:{i:0;', $n) . 'i:1;' . str_repeat('}', $n); }
function option_error($opt): string {
    try { unserialize('a:1:{i:0;i:1;}', ['max_depth' => $opt]); return 'no error'; }
    catch (Throwable $e) { return get_class($e) . ': ' . $e->getMessage(); }
}
return [
    is_array(@unserialize(nest(4096))),
    @unserialize(nest(4097)) === false,
    @unserialize(nest(3), ['max_depth' => 2]) === false,
    is_array(@unserialize(nest(2), ['max_depth' => 2])),
    @unserialize('O:8:"stdClass":1:{s:1:"a";a:1:{i:0;i:1;}}', ['max_depth' => 1]) === false,
    @unserialize(nest(200000)) === false,
    option_error('2'),
    option_error(-1),
    option_error(null),
];
PHP;
foreach (eval($code) as $line) { var_export($line); echo "\n"; }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "true\ntrue\ntrue\ntrue\ntrue\ntrue\n'TypeError: unserialize(): Option \"max_depth\" must be of type int, string given'\n'ValueError: unserialize(): Option \"max_depth\" must be greater than or equal to 0'\n'TypeError: unserialize(): Option \"max_depth\" must be of type int, null given'\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eval `serialize()`/`unserialize()` release the cells they read and build: each array element
/// leaked its key and value cells, a packed list rebuild leaked four per level, an option read
/// about nine, and a compiled object's property array was never released.
#[test]
fn test_eval_serialize_and_unserialize_release_every_temporary() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$codes = [
    'return unserialize("a:1:{i:0;i:1;}") === false;',
    'return unserialize("a:2:{s:1:\"k\";s:1:\"v\";i:5;a:1:{i:0;b:1;}}");',
    'return @unserialize("a:2:{i:0;i:1;i:1;");',
    'return unserialize("a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}");',
    'return serialize([1, "a" => [true, null], 2 => 1.5]);',
    'return unserialize(serialize((object) ["a" => [1, 2], "b" => "x"]));',
    'return serialize((object) ["a" => [1, 2], "b" => "x"]);',
    'return unserialize("a:1:{i:0;i:1;}", ["allowed_classes" => false, "max_depth" => 3]);',
];
foreach ($codes as $code) {
    var_dump(eval($code));
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\narray(2) {\n  [\"k\"]=>\n  string(1) \"v\"\n  [5]=>\n  array(1) {\n    [0]=>\n    bool(true)\n  }\n}\nbool(false)\narray(3) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n  [2]=>\n  int(3)\n}\nstring(52) \"a:3:{i:0;i:1;s:1:\"a\";a:2:{i:0;b:1;i:1;N;}i:2;d:1.5;}\"\nobject(stdClass)#1 (2) {\n  [\"a\"]=>\n  array(2) {\n    [0]=>\n    int(1)\n    [1]=>\n    int(2)\n  }\n  [\"b\"]=>\n  string(1) \"x\"\n}\nstring(65) \"O:8:\"stdClass\":2:{s:1:\"a\";a:2:{i:0;i:1;i:1;i:2;}s:1:\"b\";s:1:\"x\";}\"\narray(1) {\n  [0]=>\n  int(1)\n}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$r = &$x[0]`, `$r = &$o->p` and writes through `$r` inside eval keep php's semantics and
/// leave the heap balanced: a bind leaked the element read and its key, a write through the
/// reference leaked the assigned value.
#[test]
fn test_eval_element_and_property_references_release_their_cells() {
    let out = compile_and_run_with_heap_debug(r#"<?php
echo json_encode(eval('$x = [1, 2]; $r = &$x[0]; $s = &$x[1]; $r = 9; $s = "str" . "ing"; return $x;')), "\n";
echo json_encode(eval('$x = ["a" => 1]; $r = &$x["a"]; $r = 9; unset($r); return $x;')), "\n";
echo json_encode(eval('$o = new stdClass; $o->p = 1; $r = &$o->p; $r = 5; return $o->p;')), "\n";
echo json_encode(eval('$x = [1, 2]; $r = &$x[0]; unset($x); return $r;')), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[9,\"string\"]\n{\"a\":9}\n5\n1\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A third `ini_set()` on a shared directive, after an unknown one, returns false as php does.
/// The INI identity table still held a 14-byte record at the address the new `"6"` reused, refused
/// it, and the native coordinator exited the process with status 1 and no diagnostic.
#[test]
fn test_ini_set_survives_a_reused_ini_string_allocation() {
    // `ini_set()` is a CLI-pipeline prelude, which the library fixtures do not inject.
    let stdout = compile_cli_file_and_run_with_flags(r#"<?php
var_dump(ini_set('max_input_vars', '5'));
echo "a\n";
var_dump(ini_set('no_such', '1'));
echo "b\n";
var_dump(ini_set('max_input_vars', '6'));
echo "c\n";
var_dump(ini_set('max_input_nesting_level', '2'));
var_dump(ini_set('display_errors', '1'));
var_dump(ini_set('no_such_either', 'x'));
var_dump(ini_set('max_input_vars', '7'));
echo "done\n";
"#, &[]);
    assert_eq!(stdout, "bool(false)\na\nbool(false)\nb\nbool(false)\nc\nbool(false)\nstring(1) \"1\"\nbool(false)\nbool(false)\ndone\n");
}

/// Compiled code entering an eval-declared object publishes its call site to Magician, which
/// reads it as the C symbol `php_site` (`_php_site` on Mach-O, `php_site` on ELF). This links the
/// real, non-test Magician archive on every target: Linux builds used to fail on an undefined
/// `php_site`. A call from the ENTRY file publishes a zero file descriptor, which was read as "no
/// site", so the frame of `Base::where()` named the line of the `eval()` (9) instead of the call.
#[test]
fn test_compiled_calls_into_eval_objects_report_their_call_site() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Base {
    public function where(): string {
        $frame = (new Exception())->getTrace()[0];
        return $frame['class'] . '->' . $frame['function'] . ' @' . ($frame['file'] === __FILE__ ? 'main' : $frame['file']) . ':' . $frame['line'];
    }
}
$declare = 'class Child extends Base {}';
eval($declare);
$class = 'Child';
$child = new $class();
echo $child->where(), "\n";
function viaFunction(Base $b): string {
    return $b->where();
}
echo viaFunction($child), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Base->where @main:12\nBase->where @main:14\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A PHPDoc `@template` on an INTERFACE method is a comment to php. Magician turns docblock
/// templates into its native generic syntax, which interface signatures did not accept, so
/// Symfony's `ContainerBagInterface` was a parse error when loaded at run time (a `--web` build
/// without preload served no request at all). And a PHPDoc template never fails a call:
/// `resolveValue('abc')` against `of array<array|scalar>|scalar` used to throw a TypeError.
#[test]
fn test_eval_phpdoc_templates_are_never_a_php_error() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$code = <<<'PHP'
interface Resolver
{
    /**
     * @template TValue of array<array|scalar>|scalar
     *
     * @param TValue $value
     *
     * @psalm-return (TValue is scalar ? array|scalar : array<array|scalar>)
     */
    public function resolveValue(mixed $value): mixed;
}
final class Upper implements Resolver
{
    /**
     * @template TValue of array<array|scalar>|scalar
     *
     * @param TValue $value
     */
    public function resolveValue(mixed $value): mixed
    {
        return is_string($value) ? strtoupper($value) : $value;
    }
}
/**
 * @template T of array<int|string>|scalar
 * @param T $v
 * @return T
 */
function same(mixed $v): mixed { return $v; }
PHP;
eval($code);
$class = 'Upper';
$r = new $class();
var_dump($r->resolveValue('abc'), $r->resolveValue([1, 2]), same(7), same(['a']));
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "string(3) \"ABC\"\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\nint(7)\narray(1) {\n  [0]=>\n  string(1) \"a\"\n}\n");
}

/// A `Closure::bind($fn, null, null)` site whose closure is not traced plans a NO-scope variant of
/// every rebindable closure. In a program with an eval context, a `self::$cache` read or `unset()`
/// in such a variant propagated the receiver's resolution error and refused the whole build;
/// Symfony's `Router` closures (`unset(self::$cache[$cache->getPath()])`) stopped a `--web` build
/// with preload. php raises `Cannot access "self" when no class scope is active` only if the access
/// runs, so the variant lowers to that throw and the program compiles and behaves as php does.
#[test]
fn test_unscoped_closure_variants_with_self_static_access_compile() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Router {
    private static array $cache = ['a' => 1, 'b' => 2, 'c' => 3];
    private int $hits = 0;
    public function dropper(): Closure {
        return function (string $key) {
            $this->hits++;
            if (method_exists($this, 'probe' . $key)) { echo 'never'; }
            $before = count(self::$cache);
            unset(self::$cache[$key]);
            return $before . ':' . implode(',', array_keys(self::$cache)) . '#' . $this->hits;
        };
    }
}
function unscope(array $closures): array {
    return array_map(fn ($c) => Closure::bind($c, null, null), $closures);
}
$code = 'return 40 + 2;';
echo eval($code), "\n";
$drop = (new Router())->dropper();
echo $drop('b'), "\n";
echo unscope([fn () => 'unscoped'])[0](), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "42\n3:a,c#1\nunscoped\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Every write through an eval element reference -- plain, concatenating, compound -- releases
/// the cell it stores. The variable's entry owns its reference when the element kept its own
/// (`array_set` retains): a borrowed entry leaked one cell per write, a release right after the
/// write freed the only copy when the target had no storage. Still open, and both worse before:
/// a reference left bound when the eval'd script ends keeps its cell, and a nested-element
/// reference (`&$x['a']['b']`) leaks per bind.
#[test]
fn test_eval_writes_through_element_references_release_their_cells() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$code = <<<'PHP'
$x = ['s' => 'v', 't' => 'u'];
for ($i = 0; $i < 3; $i++) {
    $e = &$x['s'];
    $e = 'z' . $i;
    $e = $e . '!';
    $e .= 'e';
    unset($e);
    $f = &$x['t'];
    $f .= 'f';
    unset($f);
}
echo json_encode($x), "\n";
PHP;
eval($code);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"s\":\"z2!e\",\"t\":\"ufff\"}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A write through a reference to a STATIC property keeps the class's own storage: its entry only
/// borrows the value. Owning it there freed what the class still held, and the second iteration
/// read a dead cell (`Cannot access offset of type string on string` where php prints `abbb`).
#[test]
fn test_eval_compound_writes_through_static_property_references_keep_the_class_value() {
    let out = compile_and_run(r#"<?php
$code = <<<'PHP'
class Hits { public static array $hits = ['x' => 'a']; }
for ($i = 0; $i < 3; $i++) {
    $s = &Hits::$hits;
    $s['x'] .= 'b';
    unset($s);
}
echo Hits::$hits['x'], "\n";
PHP;
eval($code);
"#);
    assert_eq!(out, "abbb\n");
}

/// Eval `unserialize()` reads php's legacy `C:` payload. Every one used to fail with `Error at
/// offset`, and GMP's `C:3:"GMP":...` form only worked while eval routed `unserialize` through the
/// compiled helper. The object is built without its constructor and handed the bytes through
/// `Serializable::unserialize()` (or an internal class's hidden hook); a class without one warns
/// and keeps the bare object, an unknown or disallowed one is incomplete, a thrown exception
/// propagates, and a short payload warns like php-src.
#[test]
fn test_eval_unserialize_reads_legacy_serializable_payloads() {
    let out = compile_and_run_capture(r#"<?php
$code = <<<'PHP'
class Legacy implements Serializable {
    public $v = 'unset';
    public function __construct() { echo "constructor ran\n"; }
    public function serialize() { return $this->v; }
    public function unserialize($d) { $this->v = "got:$d"; }
    public function __serialize(): array { return []; }
    public function __unserialize(array $d): void {}
}
class Plain { public $x = 1; }
class Boom implements Serializable {
    public function serialize() { return ''; }
    public function unserialize($d) { throw new RuntimeException("boom $d"); }
    public function __serialize(): array { return []; }
    public function __unserialize(array $d): void {}
}
set_error_handler(function ($no, $msg) { echo "warning: $msg\n"; return true; });
echo unserialize('C:6:"Legacy":3:{abc}')->v, "\n";
$pair = unserialize('a:2:{i:0;C:6:"Legacy":1:{q}i:1;r:2;}');
var_dump($pair[0] === $pair[1], $pair[1]->v);
echo get_class(unserialize('C:5:"Plain":3:{abc}')), "\n";
var_dump(is_object(unserialize('C:7:"Unknown":3:{abc}')));
var_dump(unserialize('C:6:"Legacy":3:{abc}', ['allowed_classes' => false]) instanceof Legacy);
try { unserialize('C:4:"Boom":2:{zz}'); } catch (RuntimeException $e) { echo "caught:", $e->getMessage(), "\n"; }
var_dump(unserialize('C:6:"Legacy":5:{abc}'));
PHP;
eval($code);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "got:abc\nbool(true)\nstring(5) \"got:q\"\nwarning: Class Plain has no unserializer\nPlain\nwarning: Class __PHP_Incomplete_Class has no unserializer\nbool(true)\nwarning: Class __PHP_Incomplete_Class has no unserializer\nbool(false)\ncaught:boom zz\nwarning: Insufficient data for unserializing - 5 required, 4 present\nwarning: unserialize(): Error at offset 16 of 20 bytes\nbool(false)\n");
}

/// `isset($defs[$c])` on a local an `eval()` widens released its table twice: once in the probe
/// (main's projection lease) and once by its caller. The next `$defs[$c]` then read a freed table
/// and warned `Undefined array key`, so an autoloader that `eval()`s `$defs[$class]` after an
/// `isset()` check never declared the class.
#[test]
fn test_isset_on_a_widened_local_releases_its_table_once() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function run($c) {
    $defs = ['k\A' => 'echo "ran A\n";', 'k\B' => 'echo "ran B\n";'];
    if (isset($defs[$c])) {
        eval($defs[$c]);
    }
    echo isset($defs['k\B']) ? "B kept\n" : "B lost\n";
    eval($defs[$c]);
}
run('k\A');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ran A\nB kept\nran A\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A `$this` narrowed to a subclass (`if (!$this instanceof Redir) { throw ... }`) still reaches
/// the private method its ancestor's trait declares. The narrowed-receiver dispatch only looked
/// at the subclass's own method table, which never copies an inherited private method, so the
/// call went to the runtime lookup and died with `Call to undefined method`: Symfony's
/// `CompiledUrlMatcherTrait::match()` answered every 404 with a 500.
#[test]
fn test_narrowed_this_reaches_an_ancestors_private_trait_method() {
    let out = compile_and_run_capture(r#"<?php
interface Redir {}
class Ctx { public function getMethod() { return 'GET'; } }
trait MatcherTrait {
    public function match(string $pathinfo): array {
        $allow = $allowSchemes = [];
        if ($ret = $this->doMatch($pathinfo, $allow, $allowSchemes)) {
            return $ret;
        }
        if ($allow) {
            throw new RuntimeException('not allowed');
        }
        if (!$this instanceof Redir) {
            throw new RuntimeException('no routes');
        }
        if (!\in_array($this->context->getMethod(), ['HEAD', 'GET'], true)) {
        } elseif ('' !== $trimmedPathinfo = rtrim($pathinfo, '/')) {
            $pathinfo = $trimmedPathinfo === $pathinfo ? $pathinfo.'/' : $trimmedPathinfo;
            if ($ret = $this->doMatch($pathinfo, $allow, $allowSchemes)) {
                return $ret;
            }
        }
        throw new RuntimeException("No routes found for \"$pathinfo\".");
    }
    private function doMatch(string $pathinfo, array &$allow = [], array &$allowSchemes = []): array
    {
        $allow = $allowSchemes = [];
        return $pathinfo === '/a' ? ['_route' => 'a'] : [];
    }
}
class Matcher { use MatcherTrait; public $context; public function __construct() { $this->context = new Ctx(); } }
class Redirectable extends Matcher implements Redir {}
$m = new Redirectable();
echo $m->match('/a')['_route'], "\n";
try { $m->match('/missing'); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "a\nNo routes found for \"/missing/\".\n");
}

/// A string-keyed function static (`static $checkers = []` written as `$checkers[$k][$i] ??= ...`)
/// is declared as a hash, and the local fact after its initializer must say so. The fact took the
/// initializer's `array<never>`, so the nested write converted the slot's boxed value as a packed
/// array and EIR validation refused the program: Twig's `CoreExtension::getAttribute()` property
/// checker cache. A null initializer still gets boxed storage.
#[test]
fn test_string_keyed_static_keeps_its_hash_type_for_nested_writes() {
    let out = compile_and_run_capture(r#"<?php
function checker(string $c, string $p): \Closure { return static fn ($o) => $c . ':' . $p; }
class X {}
function cached($object, $item) {
    static $checkers = [];
    return ($checkers[$object::class][$item] ??= checker($object::class, $item))($object);
}
function nothing() {
    static $v = null;
    $v = $v === null ? new X() : $v;
    return get_class($v);
}
echo cached(new X, 'p'), ' ', cached(new X, 'q'), "\n";
echo nothing(), nothing(), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "X:p X:q\nXX\n");
}

/// A `\u{...}` escape names a codepoint, never a raw source byte: `"\u{e000}"` is the three bytes
/// EE 80 80. The lexer kept a scalar in U+E000..U+E0FF as its raw-byte marker, so the escape came
/// out as one 0x00 byte and a callable string naming a function spelled with that character
/// resolved to nothing.
#[test]
fn test_unicode_escapes_in_the_byte_marker_range_encode_the_codepoint() {
    let out = compile_and_run_capture(
        "<?php\necho bin2hex(\"\\u{e000}|\\u{e0ff}|\\u{e9}|\\u{1F600}\"), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ee80807cee83bf7cc3a97cf09f9880\n");
}

/// Live heap blocks a heap-debug run reports at exit.
fn live_blocks(stderr: &str) -> usize {
    if stderr.contains("HEAP DEBUG: leak summary: clean") {
        return 0;
    }
    stderr
        .split("live_blocks=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|count| count.parse().ok())
        .unwrap_or(usize::MAX)
}

/// Writes through a reference to a by-value call result (`$x = &f()`, php binds a copy) and `??=`
/// through an element reference. The variable's entry owns the reference it stores whenever the
/// target kept none of its own (a non-reference `Cell`) or retained its own (an element): the
/// three-iteration loops below leaked 21 and 19 blocks before. RATCHET, not a clean check: 9 and
/// 3 blocks are still leaked by other paths of these statements, and the bound only goes down.
#[test]
fn test_eval_call_result_and_coalesce_reference_writes_release_their_cells() {
    let call = compile_and_run_with_heap_debug(r#"<?php
set_error_handler(function ($no, $message) { return true; });
$code = <<<'PHP'
function by_value_src() { return 'v'; }
for ($i = 0; $i < 3; $i++) { $c = &by_value_src(); $c = $c . $i; $c .= '!'; echo $c, ' '; unset($c); }
PHP;
eval($code);
"#);
    assert!(call.success, "{}", call.stderr);
    assert_eq!(call.stdout, "v0! v1! v2! ");
    assert!(live_blocks(&call.stderr) <= 9, "{}", call.stderr);
    let coalesce = compile_and_run_with_heap_debug(r#"<?php
$code = <<<'PHP'
$x = ['u' => null];
for ($i = 0; $i < 3; $i++) { $e = &$x['u']; $e ??= 'd' . $i; echo $e, ' '; $e = null; unset($e); }
echo json_encode($x);
PHP;
eval($code);
"#);
    assert!(coalesce.success, "{}", coalesce.stderr);
    assert_eq!(coalesce.stdout, "d0 d1 d2 {\"u\":null}");
    assert!(live_blocks(&coalesce.stderr) <= 3, "{}", coalesce.stderr);
}
