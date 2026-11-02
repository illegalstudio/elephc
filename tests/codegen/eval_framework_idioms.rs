//! Purpose:
//! End-to-end regressions for interpreted-code idioms that a large PHP framework leans on:
//! reference captures of by-reference parameters, `instanceof stdClass`, `array_unique()` flags
//! and `unset($$name)`.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_framework_idioms` through Rust's test harness.
//!
//! Key details:
//! - Every fixture compiles PHP to native code, enters the eval bridge, and compares with the
//!   output `php` printed for the same fragment.

use crate::support::compile_and_run;

/// A closure that captures a by-reference PARAMETER by reference and assigns it must leave the
/// enclosing function (and its caller) reading the new value. The capture's write-back target is
/// the caller's variable, so the function's own `$p` kept the replaced -- and by then freed --
/// cell: it read back as empty, and a replaced array made `json_encode()` fatal.
#[test]
fn test_eval_closure_ref_capture_of_a_ref_parameter_writes_through() {
    let out = compile_and_run(
        r#"<?php
eval(<<<'EOT'
function v3(&$p) { $f = function () use (&$p) { $p = 5; }; $f(); echo "inside:", $p, ";"; }
$s = 1; v3($s); echo "outer:", $s, ";";
function v7(&$p) { $f = function () use (&$p) { $p++; }; $f(); $f(); echo "inc:", $p, ";"; }
$s = 10; v7($s); echo "outer:", $s, ";";
function arr(array &$parameters) {
    $f = function () use (&$parameters) { $parameters = ['new' => 1]; };
    $f();
    echo "arr:", json_encode($parameters), ";";
}
$r = ['id' => 7, 'slug' => 'x'];
arr($r);
echo "r:", json_encode($r), ";";
EOT
);
"#,
    );

    assert_eq!(out, "inside:5;outer:5;inc:12;outer:12;arr:{\"new\":1};r:{\"new\":1};");
}

/// `unset($captured[$key])` over a by-reference capture of a by-reference parameter is how
/// `Arr::pull()` reaches `RouteUrlGenerator`; the array must shrink for the function and its
/// caller alike.
#[test]
fn test_eval_unset_through_a_ref_capture_of_a_ref_parameter() {
    let out = compile_and_run(
        r#"<?php
eval(<<<'EOT'
class AA {
    public static function forget(&$array, $keys) { unset($array[$keys]); }
    public static function pull(&$array, $key, $default = null) {
        $value = $array[$key] ?? $default;
        static::forget($array, $key);
        return $value;
    }
}
class Gen {
    public function run(array &$parameters) {
        $f = function ($k) use (&$parameters) {
            if (isset($parameters[$k]) && $parameters[$k] !== "") {
                return AA::pull($parameters, $k);
            }
            return "?";
        };
        return $f("id") . "/" . $f("slug") . "/" . $f("none");
    }
}
$t = new Gen;
$p = ['id' => 7, 'slug' => 'x', 'ref' => 'a b'];
echo $t->run($p), json_encode($p);
EOT
);
"#,
    );

    assert_eq!(out, "7/x/?{\"ref\":\"a b\"}");
}

/// A class an eval fragment declares is carried by a `stdClass` object at run time, which used to
/// make `$obj instanceof stdClass` true for every one of them. `Router::toResponse()` tests it
/// before it renders a view, so a Blade `View` was JSON-encoded as `{}`.
#[test]
fn test_eval_instanceof_stdclass_is_false_for_declared_classes() {
    let out = compile_and_run(
        r#"<?php
eval('class Plain {} class Extending extends stdClass {} class Wrapper { public function __toString(): string { return "w"; } }');
echo eval('return (new Plain) instanceof stdClass ? "std" : "not";'), ",";
echo eval('return (new Wrapper) instanceof stdClass ? "std" : "not";'), ",";
echo eval('return (new Extending) instanceof stdClass ? "std" : "not";'), ",";
echo eval('return (new stdClass) instanceof stdClass ? "std" : "not";'), ",";
echo eval('return (function () {}) instanceof stdClass ? "std" : "not";'), ",";
echo eval('return (new Plain) instanceof \stdClass ? "std" : "not";');
"#,
    );

    assert_eq!(out, "not,not,std,std,not,not");
}

/// `array_unique($items, SORT_REGULAR)` is what `Collection::unique()` calls; the interpreter only
/// knew the one-argument form and failed the call.
#[test]
fn test_eval_array_unique_takes_sort_flags() {
    let out = compile_and_run(
        r#"<?php
echo eval('return json_encode(array_unique([1, "1", 2, 2.0, "a", "A", 3, "3"]));'), "\n";
echo eval('return json_encode(array_unique([1, "1", 2, 2.0, "a", "A", 3, "3"], SORT_REGULAR));'), "\n";
echo eval('return json_encode(array_unique([4, "4", 4.5, "4.5", "b", "b"], SORT_NUMERIC));'), "\n";
echo eval('return json_encode(array_unique(["a", "A", "a"], SORT_STRING));'), "\n";
echo eval('return json_encode(array_unique([[1], [1], [2]], SORT_REGULAR));'), "\n";
echo eval('return json_encode(array_unique(["b" => 1, "a" => 1, "c" => 2], SORT_REGULAR));'), "\n";
"#,
    );

    assert_eq!(
        out,
        "{\"0\":1,\"2\":2,\"4\":\"a\",\"5\":\"A\",\"6\":3}\n\
         {\"0\":1,\"2\":2,\"4\":\"a\",\"5\":\"A\",\"6\":3}\n\
         {\"0\":4,\"2\":4.5,\"4\":\"b\"}\n\
         [\"a\",\"A\"]\n\
         {\"0\":[1],\"2\":[2]}\n\
         {\"b\":1,\"c\":2}\n"
    );
}

/// Blade's anonymous-component prologue ends with `unset($$__key);`, which the interpreter's
/// parser rejected, so every component view died with a parse error.
#[test]
fn test_eval_unset_variable_variable() {
    let out = compile_and_run(
        r#"<?php
eval('$a = 1; $b = 2; $names = ["a", "b"];
foreach ($names as $__key) {
    if ($__key === "a") unset($$__key);
}
echo isset($a) ? "a set" : "a gone", ",", isset($b) ? "b set" : "b gone";
unset(${"b"});
echo ",", isset($b) ? "b set" : "b gone";');
"#,
    );

    assert_eq!(out, "a gone,b set,b gone");
}

/// A by-reference parameter rebound with `$array = &$array[$key]` (how `Arr::set()` walks a dotted
/// key) no longer aliases the caller's variable. The write-back at the end of the call copied the
/// innermost element over the caller's array, so `Arr::set($config, 'a.b', 2)` left
/// `['b' => 2]` -- and a session `put('a.b', ...)` erased the whole session.
#[test]
fn test_eval_by_ref_parameter_rebound_to_a_nested_element_keeps_the_caller_array() {
    let out = compile_and_run(
        r#"<?php
eval(<<<'EOT'
class H {
    public static function nested(&$array, $key, $value) {
        $keys = explode(".", $key);
        foreach ($keys as $i => $key) {
            if (count($keys) === 1) { break; }
            unset($keys[$i]);
            if (! isset($array[$key]) || ! is_array($array[$key])) { $array[$key] = []; }
            $array = &$array[$key];
        }
        $array[array_shift($keys)] = $value;
        return $array;
    }
}
function rebind(&$array) { $array = &$array["a"]; $array["z"] = 1; }
class A {
    public $p = ["keep" => 1];
    public function t($key) { H::nested($this->p, $key, 2); return json_encode($this->p); }
}
$arr = ["keep" => 1];
H::nested($arr, "a.b", 2);
echo json_encode($arr), "\n";
$arr = ["keep" => 1, "a" => ["q" => 0]];
H::nested($arr, "a.b.c", 3);
echo json_encode($arr), "\n";
$a = new A;
echo $a->t("a.b"), "\n";
$arr = ["a" => ["q" => 0], "keep" => 1];
rebind($arr);
echo json_encode($arr), "\n";
EOT
);
"#,
    );

    assert_eq!(
        out,
        "{\"keep\":1,\"a\":{\"b\":2}}\n{\"keep\":1,\"a\":{\"q\":0,\"b\":{\"c\":3}}}\n{\"keep\":1,\"a\":{\"b\":2}}\n{\"a\":{\"q\":0,\"z\":1},\"keep\":1}\n"
    );
}

/// `==` between two arrays compares them by value in interpreted code. The runtime's loose
/// comparison only knew scalar payloads, so `[1] == [1]` was false unless both operands were the
/// same storage -- which made `Collection::unique()` keep every duplicate row.
#[test]
fn test_eval_loose_equality_compares_arrays_by_value() {
    let out = compile_and_run(
        r#"<?php
echo eval('return json_encode([[1] == [1], [1] === [1], ["a" => 1, "b" => 2] == ["b" => 2, "a" => 1], ["a" => 1, "b" => 2] === ["b" => 2, "a" => 1], [[1]] == [[1]], [1] == ["1"], [1] === ["1"], [] == [], [1] != [1], [1] != [2], [] == false, [] == null, [0] == false]);'), "\n";
"#,
    );

    assert_eq!(
        out,
        "[true,true,true,false,true,true,false,true,false,true,true,true,false]\n"
    );
}

/// A static property written from a parameter owns its own reference. `Model::setEventDispatcher
/// ($d) { static::$dispatcher = $d; }` stored the parameter's cell itself, so once the caller's
/// temporary went away the slot read back as null -- or as whatever object reused the memory,
/// which is how Eloquent's event dispatcher turned into the Application.
#[test]
fn test_eval_static_property_write_from_parameter_owns_value() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvX { public $v = 5; }
class M1 { public static $d; public static $e;
  public static function putS($d) { self::$d = $d; }
  public static function putT($d) { static::$e = $d; } }
class Junk { public $a = 1; public $b = 2; }
M1::putS(new EvX);
M1::putT(new EvX);
$junk = [new Junk, new Junk, new Junk];
$z = new EvX;
M1::putS($z);
$z = null;
$junk[] = new Junk;
return get_class(M1::$d) . "|" . get_class(M1::$e) . "|" . M1::$d->v;'), "\n";
"#,
    );

    assert_eq!(out, "EvX|EvX|5\n");
}

/// A function body is not a compile-time constant position, even when it runs while a constant or
/// property default is being evaluated. An autoloader that resolves a class named in a default
/// uses `static::` freely; the "not allowed in compile-time constants" refusal leaked into it.
#[test]
fn test_eval_autoloader_runs_outside_constant_context() {
    let out = compile_and_run(
        r#"<?php
echo eval('class Loader {
    protected static $ns = "Dep";
    public function load($name) {
        if (static::$ns && str_starts_with($name, static::$ns)) {
            eval("class $name { const V = 3; }");
            return true;
        }
        return false;
    }
}
spl_autoload_register([new Loader, "load"]);
class Holder { const X = DepOne::V; public static $y = DepTwo::V; public $z = DepThree::V; }
return Holder::X . Holder::$y . (new Holder)->z;'), "\n";
"#,
    );

    assert_eq!(out, "333\n");
}

/// `new static::$prop(...)` instantiates the class NAMED by a static property. The interpreter's
/// parser read it as `new static` and took `$prop` for an undefined variable, so Eloquent's
/// `new static::$builder($query)` could never build a query builder.
#[test]
fn test_eval_new_static_property_instantiates_named_class() {
    let out = compile_and_run(
        r#"<?php
echo eval('class Bld { public $q; public function __construct($q) { $this->q = $q; } }
class Bld2 extends Bld {}
class M { protected static string $builder = Bld::class; public static $other = "Bld2";
  public function make($q) { return new static::$builder($q); }
  public static function make2($q) { return new self::$other($q); }
  public static function make3($q) { return new static::$other($q); }
  protected static array $byClass = [];
  public function indexed($q) { static::$byClass[static::class] ??= Bld2::class; return new static::$byClass[static::class]($q); } }
class N extends M { protected static string $builder = Bld2::class; }
return get_class((new M)->make(1)) . get_class((new N)->make(2)) . get_class(M::make2(3)) . get_class(N::make3(4)) . (new N)->make(7)->q . get_class((new N)->indexed(5)) . (new M)->indexed(6)->q;'), "\n";
"#,
    );

    assert_eq!(out, "BldBld2Bld2Bld27Bld26\n");
}

/// `preg_match()` honours `PREG_UNMATCHED_AS_NULL`: every group is reported, an unmatched one as
/// `null` rather than an omitted trailing key or an empty string. brick/math's number parser reads
/// `$matches['point']` and `=== null` on exactly that, so a compiled `BigNumber::of('7')` died with
/// `Undefined array key "point"` -- and with it every Laravel `between:` validation.
#[test]
fn test_preg_match_unmatched_as_null_reports_every_group() {
    let out = compile_and_run(
        r##"<?php
$re = '/^(?<sign>[\-\+])?(?<integral>[0-9]++)?(?<point>\.)?(?<fractional>[0-9]++)?(?:[eE](?<exponent>[\-\+]?[0-9]++))?\z/';
foreach (["7", "-1.5", "2e3", ""] as $s) {
    $r = preg_match($re, $s, $m, PREG_UNMATCHED_AS_NULL);
    echo $r, json_encode($m), "|";
    $r = preg_match($re, $s, $m);
    echo $r, json_encode($m), "\n";
}
preg_match('/(a)(b)?(c)?/', 'a', $m, PREG_UNMATCHED_AS_NULL);
echo json_encode($m), "\n";
preg_match('/(a)(b)?(c)?/', 'a', $m);
echo json_encode($m), "\n";
"##,
    );

    let expected = [
        r#"1{"0":"7","sign":null,"1":null,"integral":"7","2":"7","point":null,"3":null,"fractional":null,"4":null,"exponent":null,"5":null}|1{"0":"7","sign":"","1":"","integral":"7","2":"7"}"#,
        r#"1{"0":"-1.5","sign":"-","1":"-","integral":"1","2":"1","point":".","3":".","fractional":"5","4":"5","exponent":null,"5":null}|1{"0":"-1.5","sign":"-","1":"-","integral":"1","2":"1","point":".","3":".","fractional":"5","4":"5"}"#,
        r#"1{"0":"2e3","sign":null,"1":null,"integral":"2","2":"2","point":null,"3":null,"fractional":null,"4":null,"exponent":"3","5":"3"}|1{"0":"2e3","sign":"","1":"","integral":"2","2":"2","point":"","3":"","fractional":"","4":"","exponent":"3","5":"3"}"#,
        r#"1{"0":"","sign":null,"1":null,"integral":null,"2":null,"point":null,"3":null,"fractional":null,"4":null,"exponent":null,"5":null}|1[""]"#,
        r#"["a","a",null,null]"#,
        r#"["a","a"]"#,
    ]
    .join("\n")
        + "\n";
    assert_eq!(out, expected);
}

/// `array_push($all, ...$more)` and `array_unshift($all, ...$more)` unpack their spread in
/// interpreted code. The direct by-reference path refused any spread argument ("unsupported
/// NamespacedCall array_push() expression"), which is how Laravel's `MessageBag::all()` flattens
/// validation errors, so every failed validation died while rendering its messages.
#[test]
fn test_eval_array_push_and_unshift_accept_spread_arguments() {
    let out = compile_and_run(
        r#"<?php
echo eval('$all = ["a"];
foreach ([["b", "c"], [], ["d"]] as $chunk) { array_push($all, ...$chunk); }
array_push($all, "e", ...["f", "g"], ...["h"]);
$n = array_unshift($all, ...["x", "y"]);
function gen() { yield 1; yield 2; }
$nums = [0];
$count = array_push($nums, ...gen());
return json_encode([$all, $n, $nums, $count]);'), "\n";
"#,
    );

    assert_eq!(
        out,
        "[[\"x\",\"y\",\"a\",\"b\",\"c\",\"d\",\"e\",\"f\",\"g\",\"h\"],10,[0,1,2],3]\n"
    );
}

/// `is_a()` / `is_subclass_of()` on a boxed value inside a function that owns an eval context still
/// honour `$allow_string`. The eval-bridge probe only knows OBJECTS, so a boxed class NAME was
/// "not an object" there and `is_a($name, JsonResource::class, true)` came back false -- Laravel's
/// resource collections refused every resource class.
#[test]
fn test_is_a_with_boxed_class_name_and_eval_context_honours_allow_string() {
    let out = compile_and_run(
        r#"<?php
namespace T;
class JR { }
class PR extends JR { }
class H {
    public $c;
    public function __construct($c) { $this->c = $c; }
    public function check() {
        eval('$unused = 1;');
        $v = $this->c;
        return json_encode([
            is_a($v, JR::class, true), is_a($v, JR::class), is_subclass_of($v, JR::class),
            is_a(new PR, JR::class), is_subclass_of(new PR, JR::class), is_a($v, JR::class, false),
            is_subclass_of($v, JR::class, false), is_a("nope", JR::class, true),
        ]);
    }
}
echo (new H(PR::class))->check(), "\n";
"#,
    );

    assert_eq!(out, "[true,false,true,true,true,false,false,false]\n");
}

/// Arguments of `new $class(...)` resolve names in the enclosing namespace like any other
/// expression. They were left untouched, so `new $cls(PostResource::class)` inside a namespace
/// handed the callee the bare `PostResource` -- and Laravel's
/// `AnonymousResourceCollection` then refused it ("must collect instances of JsonResource").
#[test]
fn test_new_dynamic_arguments_are_namespace_resolved() {
    let out = compile_and_run(
        r#"<?php
namespace App\Res;
class JR { }
class PR extends JR { }
class Holder { public $collects; public function __construct($c, $tag = "t") { $this->collects = $c . "|" . $tag; } }
$cls = 'App\Res\Holder';
$h = new $cls(PR::class, JR::class);
$g = new $cls(PR::class);
echo $h->collects, " ", $g->collects, " ", var_export(is_a(explode("|", $h->collects)[0], JR::class, true), true), "\n";
"#,
    );

    assert_eq!(out, "App\\Res\\PR|App\\Res\\JR App\\Res\\PR|t true\n");
}

/// A string held in a variable can name `ucfirst`, `ucwords`, `mb_strtolower` or `mb_strtoupper`.
/// Those builtins had no run-time-selectable wrapper, so `$function($value)` died with `Call to
/// undefined function <dynamic>()` -- which is how Laravel's `Pluralizer::matchCase()` picks the
/// casing function, so every `Str::plural()` aborted the process.
#[test]
fn test_runtime_string_callable_selects_case_builtins() {
    let out = compile_and_run(
        r#"<?php
function match_case($value, $comparison) {
    $functions = ['mb_strtolower', 'mb_strtoupper', 'ucfirst', 'ucwords'];
    foreach ($functions as $function) {
        if ($function($comparison) === $comparison) {
            return $function($value);
        }
    }
    return $value;
}
// The eval bridge embeds the iconv bridge, which is what lets a run-time name reach `mb_*`.
eval($argv[1] ?? 'return 1;');
echo mb_strtolower('X'), '|', match_case('posts', 'post'), '|', match_case('posts', 'POST'), '|', match_case('posts', 'Post'), '|', match_case('posts', 'Blog Post'), "\n";
"#,
    );

    assert_eq!(out, "x|posts|POSTS|Posts|Posts\n");
}

/// A closure that captures its own result array by reference and calls itself keeps every level's
/// writes. The outer activation held a stale copy of the captured cell and wrote it over what its
/// inner activations had stored, so `Arr::dot()` returned only the top level of the array.
#[test]
fn test_eval_recursive_closure_keeps_shared_by_ref_capture() {
    let out = compile_and_run(
        r#"<?php
echo eval('$results = [];
$flatten = function ($data, $prefix) use (&$results, &$flatten) {
    foreach ($data as $key => $value) {
        if (is_array($value) && ! empty($value)) { $flatten($value, $prefix . $key . "."); }
        else { $results[$prefix . $key] = $value; }
    }
};
$flatten(["z" => 0, "a" => ["b" => 1, "c" => ["d" => 2]], "e" => 3], "");
$count = 0;
$tick = function () use (&$count, &$tick) { $count++; if ($count < 4) { $tick(); } $count += 10; };
$tick();
return json_encode($results) . "|" . $count;'), "\n";
"#,
    );

    assert_eq!(out, "{\"z\":0,\"a.b\":1,\"a.c.d\":2,\"e\":3}|44\n");
}

/// `compact()` inside interpreted code gathers the running scope's variables. It was an
/// undefined function there, so a schema builder that wrote `compact('type', 'name')` made every
/// column's `type` empty.
#[test]
fn test_eval_compact_gathers_scope_variables() {
    let out = compile_and_run(
        r#"<?php
echo eval('class B { public function col($type, $name, array $extra = []) { return array_merge(compact("type", "name"), $extra); }
  public function str($column, $length = null) { $length = $length ?: 255; return $this->col("string", $column, compact("length")); } }
function f() { $a = 1; $b = [2]; $c = "x"; return compact("a", ["b", ["c", "missing"]]); }
$b = new B;
return json_encode([$b->str("title"), f(), $b->col("t", "n", ["k" => true])]);'), "\n";
"#,
    );

    assert_eq!(
        out,
        "[{\"type\":\"string\",\"name\":\"title\",\"length\":255},{\"a\":1,\"b\":[2],\"c\":\"x\"},{\"type\":\"t\",\"name\":\"n\",\"k\":true}]\n"
    );
}

/// A by-reference `int` parameter lent to a method on a gradual receiver keeps its cell's
/// representation. In a program that can declare methods at run time the callee's by-reference
/// shape is unknown, and re-binding `int &$cursor` to `mixed` read the caller's raw int as a box
/// pointer: the next `++$cursor` stored a heap address into it. Symfony's YAML lexer reached it
/// through `isset($this->line[$cursor])`, whose `ArrayAccess` arm is such a call.
#[test]
fn test_by_ref_int_param_lent_to_a_gradual_method_keeps_its_cell() {
    let out = compile_and_run(
        r#"<?php
class Q { public function take($k) { return $k; } }
class Lexer {
    private string $line = "ab'cd'";
    public function quoted(int &$cursor): string {
        $out = '';
        for (; strlen($this->line) > $cursor; ++$cursor) {
            if ("'" === $this->line[$cursor]) {
                ++$cursor;
                if (isset($this->line[$cursor]) && "'" === $this->line[$cursor]) { continue; }
                return $out;
            }
            $out .= $this->line[$cursor];
        }
        return "malformed";
    }
}
function lend(mixed $o, int &$cursor): void { $o->take($cursor); ++$cursor; }
$c = 3; echo (new Lexer)->quoted($c), ":", $c, "\n";
$n = 1; lend(new Q, $n); echo $n, "\n";
eval('echo "eval\n";');
"#,
    );

    assert_eq!(out, "cd:6\n2\neval\n");
}

/// A declared `iterable` parameter reached through a `mixed` receiver gets the array or
/// Traversable payload, not the box. In a program that can run eval, `$this` inside a closure is
/// gradual, so Symfony's table generator called `fillCells(iterable $row)` through the candidate
/// dispatch, which handed over the boxed cell; `foreach` read it as null and `about` failed.
#[test]
fn test_iterable_parameter_through_a_gradual_receiver_gets_the_payload() {
    let out = compile_and_run(
        r#"<?php
class Rows implements IteratorAggregate {
    public function __construct(private Closure $generator) {}
    public function getIterator(): Traversable { return ($this->generator)(); }
}
class Table {
    private function fill(iterable $row): iterable { $n = []; foreach ($row as $c) { $n[] = strtoupper($c); } return $n; }
    public function build(array $rows): Rows {
        return new Rows(function () use ($rows): Traversable {
            foreach ($rows as $row) { yield $this->fill($row); }
        });
    }
}
function viaMixed(mixed $o, mixed $row) { return $o->fill2($row); }
class Plain { public function fill2(iterable $row): string { return implode(",", is_array($row) ? $row : iterator_to_array($row)); } }
foreach ((new Table)->build([['a', 'b'], ['c']]) as $cells) { echo implode(",", $cells), "|"; }
echo viaMixed(new Plain, ['x', 'y']), "|", viaMixed(new Plain, new ArrayIterator(['z'])), "\n";
eval('echo "eval\n";');
"#,
    );

    assert_eq!(out, "A,B|C|x,y|z\neval\n");
}
