//! Purpose:
//! Pins the behavior of runtime fast paths that replaced slower generic ones: the hashed
//! `new $class(...)` dispatch, in-place property element writes, integer-key hashing, and
//! reused fiber/generator stacks.
//!
//! Called from:
//! - `crate::codegen` test module registration.
//!
//! Key details:
//! - Each fixture checks the answer php gives, so a fast path can only be kept while it agrees.

use crate::support::*;
use std::fs;

/// `new $name(...$args)` dispatches through `crc32(strtolower($name))` once it has enough
/// candidates. Every class must still be reached under any spelling of its name, with named and
/// spread arguments planned per class, and a non-string name must keep the generic path.
#[test]
fn hashed_dynamic_new_reaches_every_candidate() {
    let mut source = String::from("<?php\nnamespace App\\Shapes;\n");
    for index in 0..24 {
        source.push_str(&format!(
            "class C{index:02} {{ public function __construct(public int $a = {index}, public string $b = 'd') {{}} }}\n"
        ));
    }
    source.push_str(
        r#"
function make(mixed $name, array $args): object { return new $name(...$args); }
function makeStr(string $name, array $args): object { return new $name(...$args); }
foreach (['App\Shapes\C00', 'app\shapes\c07', 'APP\SHAPES\C23', 'App\Shapes\c13'] as $i => $name) {
    $o = make($name, ['b' => "x$i"]);
    echo get_class($o), ' ', $o->a, ' ', $o->b, "\n";
    $o = makeStr($name, [$i * 100, 'y']);
    echo get_class($o), ' ', $o->a, ' ', $o->b, "\n";
}
echo get_class(make(\stdClass::class, [])), "\n";
"#,
    );
    let out = compile_and_run(&source);
    assert_eq!(
        out,
        "App\\Shapes\\C00 0 x0\nApp\\Shapes\\C00 0 y\nApp\\Shapes\\C07 7 x1\nApp\\Shapes\\C07 100 y\n\
         App\\Shapes\\C23 23 x2\nApp\\Shapes\\C23 200 y\nApp\\Shapes\\C13 13 x3\nApp\\Shapes\\C13 300 y\n\
         stdClass\n"
    );
}

/// `$this->p[$k] = $v` writes into the property's own container. The value is evaluated first,
/// as php does, so writes it makes to the same property survive; a snapshot taken before the
/// write keeps its old contents; and growth, autovivification and subclasses all publish back.
#[test]
fn in_place_property_write_keeps_php_order_and_snapshots() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public array $items = ['a' => 1];
    protected array $typed;
    public function snapshotThenWrite(): array {
        $prev = $this->items;
        $this->items['b'] = 2;
        $this->items['a'] = 10;
        return $prev;
    }
    public function grow(int $n): int {
        for ($i = 0; $i < $n; $i++) { $this->items["k$i"] = $i; }
        return count($this->items);
    }
    public function selfWritingValue(): void { $this->items['outer'] = $this->addMany(); }
    private function addMany(): int {
        for ($i = 0; $i < 40; $i++) { $this->items["inner$i"] = $i; }
        return 99;
    }
    public function typedAutovivify(): string {
        $this->typed['x'] = 'y';
        $this->typed['z'] = 'w';
        return implode(',', $this->typed);
    }
}
class Sub extends Bag {}
$b = new Bag();
echo json_encode($b->snapshotThenWrite()), json_encode($b->items), "\n";
echo $b->grow(100), "\n";
$b->selfWritingValue();
echo count($b->items), ' ', $b->items['outer'], ' ', $b->items['inner39'], "\n";
echo $b->typedAutovivify(), "\n";
$s = new Sub();
$s->items['sub'] = 's';
$alias = $s->items;
$s->items['sub'] = 't';
echo $alias['sub'], $s->items['sub'], "\n";
"#,
    );
    assert_eq!(out, "{\"a\":1}{\"a\":10,\"b\":2}\n102\n143 99 39\ny,w\nst\n");
}

/// A plain program writing a property element links no interpreter: the quiet-fetch scope
/// around the container read exists only for eval-owned receivers.
#[test]
fn property_element_write_without_eval_calls_no_bridge() {
    let dir = make_cli_test_dir("elephc_quiet_fetch_no_bridge");
    let (user_asm, _, _) = compile_source_to_asm_with_options(
        "<?php class A { public array $m = []; function set(string $k): void { $this->m[$k] = 1; } }
        $a = new A(); $a->set('x'); echo count($a->m);",
        &dir,
        8_388_608,
        false,
        false,
    );
    let _ = fs::remove_dir_all(&dir);
    assert!(!user_asm.contains("__elephc_eval_quiet_property_fetch"));
}

/// Powers of two share their low bits under a multiplicative hash; the hash folds its high half
/// in so they do not all probe from one slot. Correctness must not depend on that either way.
#[test]
fn power_of_two_integer_keys_round_trip() {
    let out = compile_and_run(
        r#"<?php
$a = [];
for ($i = 0; $i < 62; $i++) { $a[1 << $i] = $i; }
unset($a[1 << 10], $a[1 << 40]);
$a[1 << 20] = 'twenty';
$sum = 0;
foreach ($a as $k => $v) { if (is_int($v)) { $sum += $v; } }
echo count($a), ' ', $a[1 << 20], ' ', isset($a[1 << 10]) ? 'y' : 'n', ' ', $sum, ' ', array_key_first($a), ' ', array_key_last($a), "\n";
"#,
    );
    assert_eq!(out, "60 twenty n 1821 1 2305843009213693952\n");
}

/// Freed fiber stacks are reused. Sequential and nested generators, and fibers that suspend and
/// resume, must all behave as with fresh stacks.
#[test]
fn reused_fiber_stacks_run_generators_and_fibers() {
    let out = compile_and_run(
        r#"<?php
function inner(int $n): Generator { for ($i = 0; $i < $n; $i++) { yield $i; } }
function outer(int $depth): Generator {
    if ($depth > 0) { yield from outer($depth - 1); }
    yield from inner(3);
}
$sum = 0;
for ($k = 0; $k < 500; $k++) { foreach (inner(2) as $v) { $sum += $v; } }
$nested = 0;
for ($k = 0; $k < 20; $k++) { foreach (outer(20) as $v) { $nested += $v; } }
$fibers = 0;
for ($k = 0; $k < 50; $k++) {
    $f = new Fiber(function (int $x): int { $y = Fiber::suspend($x * 2); return $x + $y; });
    $fibers += $f->start($k);
    $f->resume(1);
    $fibers += $f->getReturn();
}
echo $sum, ' ', $nested, ' ', $fibers, "\n";
"#,
    );
    assert_eq!(out, "500 1260 3725\n");
}

/// realpath() answers from a php-style cache of absolute paths, which every path-changing
/// builtin empties: an unlinked, renamed or removed path must stop resolving at once, a
/// recreated one must resolve again, and a relative path still follows the cwd.
#[test]
fn realpath_cache_follows_path_changes() {
    let out = compile_and_run(
        r#"<?php
$dir = sys_get_temp_dir() . '/elephc_rpcache_' . mt_rand();
@mkdir($dir);
$f = "$dir/a.txt";
file_put_contents($f, 'x');
$r1 = realpath($f);
$r2 = realpath($f);
echo $r1 === $r2 ? "same" : "diff", ' ', basename($r1), "\n";
unlink($f);
var_dump(realpath($f));
file_put_contents($f, 'y');
echo realpath($f) === $r1 ? "back" : "lost", "\n";
rename($f, "$dir/b.txt");
var_dump(realpath($f));
echo basename(realpath("$dir/b.txt")), "\n";
chdir($dir);
echo basename(realpath('b.txt')), "\n";
unlink("$dir/b.txt");
mkdir("$dir/sub");
echo basename(realpath("$dir/sub/../sub")), "\n";
rmdir("$dir/sub");
var_dump(realpath("$dir/sub"));
clearstatcache(true);
chdir('/');
rmdir($dir);
var_dump(realpath($dir));
"#,
    );
    assert_eq!(
        out,
        "same a.txt\nbool(false)\nback\nbool(false)\nb.txt\nb.txt\nsub\nbool(false)\nbool(false)\n"
    );
}

/// The eval bridge's constructor and method dispatch switch on the class id through a search
/// tree once there are more cases than a leaf holds; every class must still be reached.
#[test]
fn eval_bridge_class_id_switch_reaches_every_class() {
    let mut source = String::from("<?php\n");
    for index in 0..20 {
        source.push_str(&format!(
            "class K{index:02} {{ public function __construct(public int $v = 0) {{}} public function get(): int {{ return $this->v + {index}00; }} }}\n"
        ));
    }
    source.push_str(
        r#"
foreach (['K00', 'K07', 'K19', 'K13', 'K08', 'K01'] as $i => $name) {
    $o = eval("return new $name($i);");
    echo get_class($o), ' ', $o->get(), ' ', eval('return $o->get();'), "\n";
}
"#,
    );
    let out = compile_and_run(&source);
    assert_eq!(
        out,
        "K00 0 0\nK07 701 701\nK19 1902 1902\nK13 1303 1303\nK08 804 804\nK01 105 105\n"
    );
}

/// A program without eval builds `new $class(...)` of a builtin exception itself: the message,
/// code and previous reach the object, as does the creation line. The registry fallback it used
/// to reach allocated the object without running the constructor at all.
#[test]
fn dynamic_new_of_a_builtin_exception_runs_its_constructor() {
    let out = compile_and_run(
        r#"<?php
$previous = new LogicException('inner');
$class = $argc === 1 ? 'RuntimeException' : 'LogicException';
$line = __LINE__ + 1;
$e = new $class('outer', 7, $previous);
echo get_class($e), '|', $e->getMessage(), '|', $e->getCode(), '|',
    $e->getPrevious()->getMessage(), '|', $e->getLine() === $line ? 'line' : 'wrong-line', "\n";
$m = new $class('only');
echo $m->getMessage(), '|', $m->getCode(), "\n";
"#,
    );
    assert_eq!(out, "RuntimeException|outer|7|inner|line\nonly|0\n");
}

/// An object-owned reference property built by `new $class` holds a CELL, and its default is
/// written into that cell -- writing it into the slot replaced the cell pointer.
#[test]
fn dynamic_new_writes_reference_property_defaults_through_the_cell() {
    let out = compile_and_run(
        r#"<?php
class RefDefaults {
    private bool $first = true;
    private bool $second = false;
    public function bind(): void { $r = &$this->second; $r = true; }
    public function show(): string { return ($this->first ? "T" : "F") . ($this->second ? "T" : "F"); }
}
$cls = "RefDefaults";
$byname = new $cls();
echo $byname->show(), "|";
$byname->bind();
echo $byname->show(), "\n";
"#,
    );
    assert_eq!(out, "TF|TT\n");
}

/// Compiled code answers "is this receiver eval-owned?" from a per-bucket filter before asking
/// the interpreter's registry, and the free path skips the eval destructor call the same way.
/// Eval-owned objects among thousands of native ones must still dispatch to their overrides,
/// read through their overlay, and run their destructors when php does.
#[test]
fn eval_owner_filter_keeps_dispatch_and_destructors() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public string $name = 'base';
    public function who(): string { return 'native:' . $this->name; }
    public function tag(): string { return $this->who(); }
}
function describe(Base $b): string { return $b->who() . '|' . $b->name . '|' . $b->tag(); }
$natives = [];
for ($i = 0; $i < 3000; $i++) { $natives[] = new Base(); }
eval('class Child extends Base {
    public function __construct() { $this->name = "child"; }
    public function who(): string { return "eval:" . $this->name; }
    public function __destruct() { echo "destruct ", $this->name, "\n"; }
}');
$children = [];
for ($i = 0; $i < 5; $i++) {
    $c = eval('return new Child();');
    $c->name = "c$i";
    $children[] = $c;
}
$natives = [];
echo describe(new Base()), "\n";
foreach ($children as $c) { echo describe($c), "\n"; }
$children[2] = null;
echo "after one\n";
$more = [];
for ($i = 0; $i < 3000; $i++) { $more[] = new Base(); }
$more = [];
$children = [];
echo "after all\n";
$x = eval('return new Child();');
echo describe($x), "\n";
unset($x);
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "native:base|base|native:base\neval:c0|c0|eval:c0\neval:c1|c1|eval:c1\n\
         eval:c2|c2|eval:c2\neval:c3|c3|eval:c3\neval:c4|c4|eval:c4\ndestruct c2\nafter one\n\
         destruct c0\ndestruct c1\ndestruct c3\nafter all\neval:child|child|eval:child\n\
         destruct child\nend\ndestruct c4\n"
    );
}

/// A local the eval scope can reach is stored boxed. Appending to it used to copy the whole
/// array on every `$a[] = $x` and leak the previous copy with its box: 3000 appends ran the
/// 8 MB heap dry. The append must own the array outright and grow it in place.
#[test]
fn append_to_a_boxed_local_grows_in_place() {
    let out = compile_and_run(
        r#"<?php
class Item { public string $name = "item"; }
$items = [];
$before = memory_get_usage();
for ($i = 0; $i < 3000; $i++) { $items[] = new Item(); }
array_push($items, new Item(), new Item());
$grown = memory_get_usage() - $before;
echo count($items), ' ', $items[2999]->name, ' ', $grown < 1000000 ? 'linear' : "quadratic:$grown", "\n";
if ($argc > 5) { eval('class Sub extends Item {}'); }
"#,
    );
    assert_eq!(out, "3002 item linear\n");
}

/// A compiled include first asks the interpreter's include state directly, without an eval
/// context or a copy of the caller's locals: a repeated `include_once` answers `true` there. What
/// it cannot answer must still behave exactly as before -- a plain `require` of an already
/// included file runs it again, `include` sees and keeps the caller's variables, a missing file is
/// `false` -- whichever spelling of the path reaches the file.
#[test]
fn include_fast_path_keeps_php_include_semantics() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
require_once __DIR__ . "/lib.php";
function load(string $f, int $n): mixed { $local = $n * 2; $r = include $f; return [$r, $local]; }
function once(string $f) { $x = 5; $v = include_once $f; return var_export($v, true) . ":$x"; }
function stmt(string $f) { $keep = "k"; require $f; return $keep; }
$dir = __DIR__;
echo helper_from_lib(), "\n";
echo stmt("$dir/lib.php"), "\n";
echo stmt("$dir/../" . basename($dir) . "/lib.php"), "\n";
echo once("$dir/side.php"), "\n";
echo once("$dir/side.php"), "\n";
echo json_encode(load("$dir/data.php", 3)), "\n";
echo json_encode(load("$dir/data.php", 4)), "\n";
echo once("$dir/data.php"), "\n";
echo @once("$dir/missing.php"), "\n";
if ($argc > 9) { eval("echo 1;"); }
"#,
            ),
            (
                "lib.php",
                r#"<?php
if (!function_exists('helper_from_lib')) {
    function helper_from_lib(): string { return "lib"; }
}
echo "lib body ran\n";
"#,
            ),
            ("data.php", "<?php\nreturn [\"answer\" => 42, \"who\" => basename(__FILE__)];\n"),
            ("side.php", "<?php\necho \"side ran\\n\";\n"),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        "lib body ran\nlib\nlib body ran\nk\nlib body ran\nk\nside ran\n1:5\ntrue:5\n\
         [{\"answer\":42,\"who\":\"data.php\"},6]\n[{\"answer\":42,\"who\":\"data.php\"},8]\n\
         true:5\nfalse:5\n"
    );
}

/// `sort()`/`rsort()` of a `mixed` receiver hands an all-scalar list to the native sort, which
/// orders it with php's comparison (numeric strings numerically, case-sensitive strings), and
/// keeps the quadratic prelude loop only for a list holding an array or an object.
#[test]
fn gradual_sort_of_scalars_takes_the_native_sort() {
    let out = compile_and_run(
        r#"<?php
function gradual(int $which): mixed {
    return match ($which) {
        0 => ['getName', 'alpha', 'Zeta', 'isValid', '_private', 'beta'],
        1 => ['10', '9', '1e1', '010', 'x', '2'],
        default => [[2, 1], 'b', 'a', 3],
    };
}
foreach ([0, 1] as $which) {
    $values = gradual($which);
    sort($values);
    echo implode(',', $values), "\n";
    $values = gradual($which);
    rsort($values);
    echo implode(',', $values), "\n";
}
$nested = gradual(2);
sort($nested);
echo json_encode($nested), "\n";
$floats = $argc > 5 ? 'none' : [1.5, -2, '3', 0, true];
sort($floats);
var_export($floats);
echo "\n";
"#,
    );
    assert_eq!(
        out,
        "Zeta,_private,alpha,beta,getName,isValid\nisValid,getName,beta,alpha,_private,Zeta\n\
         2,9,10,1e1,010,x\nx,10,1e1,010,9,2\n[3,\"a\",\"b\",[2,1]]\n\
         array (\n  0 => -2,\n  1 => 0,\n  2 => 1.5,\n  3 => '3',\n  4 => true,\n)\n"
    );
}

const INTEGER_SWITCH_SOURCE: &str = r#"<?php
function dense(int $v): string {
    switch ($v) {
        case -3: return 'm3';
        case -2: return 'm2';
        case 0: return 'z';
        case 1: return 'one';
        case 2:
        case 3: return 'two-three';
        case 4: $x = 'four'; // falls through
        case 5: return ($x ?? '') . 'five';
        case 7: return 'seven';
        case 1: return 'dup-one';
        case 9: return 'nine';
        case 10: return 'ten';
        default: return 'default';
    }
}
function sparse(int $v): string {
    switch ($v) {
        case 1: return 'a';
        case 1000: return 'b';
        case 1000000: return 'c';
        case -5: return 'd';
        case 42: return 'e';
        case 7: return 'f';
        case 99: return 'g';
        case 123456789: return 'h';
        default: return 'x';
    }
}
function wide(int $v): int {
    switch ($v) {
        case PHP_INT_MIN: return 1;
        case 0: return 2;
        case PHP_INT_MAX: return 3;
        default: return 0;
    }
}
$out = [];
foreach ([-4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, PHP_INT_MIN, PHP_INT_MAX] as $v) {
    $out[] = dense($v);
}
echo implode(',', $out), "\n";
$out = [];
foreach ([1, 1000, 1000000, -5, 42, 7, 99, 123456789, 0, 2] as $v) {
    $out[] = sparse($v);
}
echo implode(',', $out), "\n";
echo wide(PHP_INT_MIN), wide(0), wide(PHP_INT_MAX), wide(5), "\n";

#[Attribute]
class Tag { public function __construct(public string $name, public int $weight = 1) {} }
#[Tag('alpha')] class A1 {}
#[Tag('beta', weight: 2)] class A2 {}
#[Tag('gamma', 3)] class A3 {}
#[Tag('delta')] class A4 {}
#[Tag('eps', 5)] class A5 {}
#[Tag('zeta')] class A6 {}
#[Tag('eta', weight: 7)] class A7 {}
#[Tag('theta')] class A8 {}
#[Tag('iota', 9)] class A9 {}
foreach (['A9', 'A1', 'A5', 'A2', 'A7', 'A3', 'A8', 'A4', 'A6'] as $class) {
    $attr = (new ReflectionClass($class))->getAttributes()[0];
    $tag = $attr->newInstance();
    echo $class, ':', $tag->name, '/', $tag->weight, '/', json_encode($attr->getArguments()), "\n";
}
"#;

/// A dense integer `switch` dispatches through one jump table instead of a compare per case: both
/// ends of the range and the gaps take the default, a repeated value keeps its first case, and a
/// fall-through still falls through. A sparse or two-ended switch keeps the compare chain.
/// `ReflectionAttribute::newInstance()`/`getArguments()` switch over every attribute factory the
/// program declares, which the chain walked one compare at a time.
#[test]
fn dense_integer_switch_dispatches_through_a_jump_table() {
    assert_eq!(
        compile_and_run(INTEGER_SWITCH_SOURCE),
        "default,m3,m2,default,z,one,two-three,two-three,fourfive,five,default,seven,default,nine,\
         ten,default,default,default\na,b,c,d,e,f,g,h,x,x\n1230\n\
         A9:iota/9/[\"iota\",9]\nA1:alpha/1/[\"alpha\"]\nA5:eps/5/[\"eps\",5]\n\
         A2:beta/2/{\"0\":\"beta\",\"weight\":2}\nA7:eta/7/{\"0\":\"eta\",\"weight\":7}\n\
         A3:gamma/3/[\"gamma\",3]\nA8:theta/1/[\"theta\"]\nA4:delta/1/[\"delta\"]\n\
         A6:zeta/1/[\"zeta\"]\n"
    );
    let dir = make_cli_test_dir("elephc_integer_switch_table");
    let (user_asm, _runtime_asm, _libraries) =
        compile_source_to_asm_with_options(INTEGER_SWITCH_SOURCE, &dir, 8_388_608, false, false);
    let _ = fs::remove_dir_all(&dir);
    let tables = |name: &str| {
        user_asm
            .split(&format!("@fn name={name} "))
            .nth(1)
            .unwrap_or_else(|| panic!("{name} must be emitted"))
            .split("@endfn")
            .next()
            .unwrap_or_else(|| panic!("{name} must be terminated"))
            .lines()
            .filter(|line| line.contains("_switch_table_") && line.trim_end().ends_with(':'))
            .count()
    };
    assert_eq!(tables("dense"), 1);
    assert_eq!(tables("sparse"), 0);
    assert_eq!(tables("wide"), 0);
    assert_eq!(tables("ReflectionAttribute::newInstance"), 1);
}

const REFLECTION_PROPERTY_SOURCE: &str = r#"<?php
class Base {
    protected int $shared = 1;
}
class Resp extends Base {
    public array $headers = ['a' => 1];
    private string $name = 'x';
    protected ?int $code = null;
    public static int $count = 3;

    public function literal(): string {
        static $r;
        $r ??= new \ReflectionProperty(self::class, 'headers');
        return $r->getName();
    }

    public static function byName(self $o, string $name, mixed $value): void {
        static $cache;
        $r = $cache[$name] ??= new \ReflectionProperty(self::class, $name);
        $r->setValue($o, $value);
    }

    public static function describe(string $name): string {
        try {
            $r = new \ReflectionProperty(self::class, $name);
            return $r->getName() . ':' . $r->class . ':' . ($r->isStatic() ? 's' : 'i')
                . ($r->isPrivate() ? 'P' : '') . ($r->isProtected() ? 'R' : '');
        } catch (\ReflectionException $e) {
            return 'E:' . $e->getMessage();
        }
    }
}
$o = new Resp();
echo $o->literal(), "\n";
Resp::byName($o, 'headers', ['b' => 2]);
Resp::byName($o, 'code', 7);
Resp::byName($o, 'name', 'y');
Resp::byName($o, 'code', 8);
echo json_encode($o->headers), "\n";
echo (new ReflectionProperty(Resp::class, 'code'))->getValue($o), "\n";
echo (new ReflectionProperty(Resp::class, 'name'))->getValue($o), "\n";
foreach (['headers', 'name', 'code', 'count', 'shared', 'missing', 'Headers'] as $p) {
    echo Resp::describe($p), "\n";
}
"#;

/// A `ReflectionProperty` naming a compiled class is built from compile-time metadata: a literal
/// property directly, a runtime name by matching it against the declared properties. Only a name
/// the class does not declare reaches the interpreter, which raises php's exception. Every
/// Reflection construction used to go through the interpreter once the lowering had reserved an
/// eval context for it -- `Response::setHeaders()` and `Request::setProperty()` on each request.
#[test]
fn reflection_property_of_a_compiled_class_is_built_natively() {
    assert_eq!(
        compile_and_run(REFLECTION_PROPERTY_SOURCE),
        "headers\n{\"b\":2}\n8\ny\nheaders:Resp:i\nname:Resp:iP\ncode:Resp:iR\ncount:Resp:s\n\
         shared:Base:iR\nE:Property Resp::$missing does not exist\n\
         E:Property Resp::$Headers does not exist\n"
    );
    let dir = make_cli_test_dir("elephc_reflection_property_native");
    let (user_asm, _runtime_asm, _libraries) =
        compile_source_to_asm_with_options(REFLECTION_PROPERTY_SOURCE, &dir, 8_388_608, false, false);
    let _ = fs::remove_dir_all(&dir);
    let body = |name: &str| {
        user_asm
            .split(&format!("@fn name={name} "))
            .nth(1)
            .unwrap_or_else(|| panic!("{name} must be emitted"))
            .split("@endfn")
            .next()
            .unwrap_or_else(|| panic!("{name} must be terminated"))
            .to_string()
    };
    let interpreted = |body: &str| body.matches("eval_reflection_new_object").count();
    assert_eq!(interpreted(&body("Resp::literal")), 0, "a literal member is metadata");
    let by_name = body("Resp::byName");
    assert_eq!(interpreted(&by_name), 1, "only the unmatched name may reach the interpreter");
    assert!(by_name.contains("__rt_str_eq"), "the runtime name is matched natively");
}
