//! Purpose:
//! End-to-end coverage for expression-level parser and lowering fixes for forms real-world
//! vendor code uses: subscripts over `mixed` call results, `$obj::staticMethod()`, and the PHP 8.x
//! expression forms.
//!
//! Called from:
//! - The native codegen suite (`tests/codegen/mod.rs`).
//!
//! Key details:
//! - Expected output was produced by PHP 8.5 from the same source.
//! - Receivers come from functions returning `mixed`, so no pass can narrow them statically.

use crate::support::*;

/// Verifies indexing a `mixed` value reads string offsets, array elements and `ArrayAccess`
/// offsets like PHP, whether the receiver is a call result or a local (monolog's
/// `$client::VERSION[0]` shape used to print an empty string).
#[test]
fn test_mixed_receiver_subscript_reads_string_array_and_array_access() {
    let out = compile_and_run(
        r#"<?php
        class Box implements ArrayAccess {
            public function offsetExists(mixed $o): bool { return true; }
            public function offsetGet(mixed $o): mixed { return "box[$o]"; }
            public function offsetSet(mixed $o, mixed $v): void {}
            public function offsetUnset(mixed $o): void {}
        }
        function s(): mixed { return '8.1'; }
        function a(): mixed { return ['x' => 1, 2 => 'two', 0 => 'zero']; }
        function o(): mixed { return new Box(); }
        function pick(int $k): mixed { $all = ['abc', [5, 6], new Box()]; return $all[$k]; }
        echo s()[0], '|', s()[2], '|', a()['x'], '|', a()[2], '|', a()[0], '|', o()['q'], "\n";
        echo pick(0)[1], '|', pick(1)[1], '|', pick(2)[7], '|', strlen(s()[1]), "\n";
        $m = s(); echo $m[0], $m[-1], "\n";
        "#,
    );
    assert_eq!(out, "8|1|1|two|zero|box[q]\nb|6|box[7]|1\n81\n");
}

/// Verifies `??` over a `mixed` string offset stays silent and falls back on a miss, and that a
/// `string|array` union receiver also reads string offsets.
#[test]
fn test_mixed_receiver_subscript_silent_and_union_string() {
    let out = compile_and_run(
        r#"<?php
        function s(): mixed { return 'ab'; }
        function u(bool $str): string|array { return $str ? 'xy' : ['p', 'q']; }
        echo s()[1] ?? 'd', '|', s()[5] ?? 'd', '|', u(true)[1], '|', u(false)[1], "\n";
        "#,
    );
    assert_eq!(out, "b|d|y|q\n");
}

/// Verifies the string-offset arm over a `mixed` receiver frees what it allocates: owned call
/// results, locals, `??` misses and boxed keys, read in a loop under heap debug.
#[test]
fn test_mixed_receiver_subscript_string_arm_is_leak_free() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
        function v(int $i): mixed { return $i % 2 ? 'v' . $i . '.0' : ['k' . $i, 'w']; }
        function idx(mixed $k): mixed { return $k; }
        $out = '';
        for ($i = 0; $i < 4; $i++) {
            $m = v($i);
            $out .= v($i)[1] . $m[0] . (v($i)[9] ?? '-') . v($i)[idx(1)] . ',';
        }
        echo $out;
        "#,
    );
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "wk0-w,1v-1,wk2-w,3v-3,");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Verifies an out-of-range offset on a `mixed` string still warns like PHP's string read.
#[test]
fn test_mixed_receiver_subscript_string_miss_warns() {
    let output = compile_and_run_capture(
        r#"<?php
        function v(): mixed { return 'v1'; }
        echo v()[7], "|end";
        "#,
    );
    assert_eq!(output.stdout, "|end");
    assert!(output.stderr.contains("Uninitialized string offset 7"), "{}", output.stderr);
}

/// Regression: the string arm of a `mixed` receiver reads a boxed variable offset again after
/// its cast warning, like the typed string read: a handler that moves `$o` from `1.9` to `2.9`
/// selects `'c'`, as in PHP.
#[test]
fn test_mixed_receiver_subscript_string_offset_reloaded_after_cast_warning() {
    let out = compile_and_run(
        r#"<?php
        function v(): mixed { return 'abc'; }
        $s = v(); $t = 'abc';
        $o = 1.9;
        set_error_handler(function ($no, $msg) use (&$o) { echo "[$msg]"; $o = 2.9; return true; });
        echo $s[$o], "|";
        $o = 1.9;
        echo $t[$o], "|";
        $o = 0;
        echo $s[$o], $s[1], $s[-1], "\n";
        "#,
    );
    assert_eq!(out, "[String offset cast occurred]c|[String offset cast occurred]c|abc\n");
}

/// Regression: `$class::m()` falls back to the runtime class's `__callStatic` when it has no
/// `m` (inherited handlers keep late static binding, spreads are unpacked) or when `m` is not
/// accessible from the calling scope, while a class without the handler still throws.
#[test]
fn test_dynamic_receiver_static_call_falls_back_to_call_static() {
    let out = compile_and_run(
        r#"<?php
        class Base {
            public static function __callStatic($n, $a) { return static::class . ":$n:" . implode(',', $a); }
            private static function sec() { return 'sec'; }
            public static function inside(string $c) { return $c::sec(); }
        }
        class Kid extends Base {}
        class Plain { public static function f() { return 'f'; } }
        $k = 'Kid'; $b = 'Base'; $p = 'Plain';
        echo $k::anything(1, 2), "\n";
        $xs = [3, 4]; echo $b::spread(...$xs), "\n";
        echo $b::sec(), "|", Base::inside('Base'), "\n";
        $o = new Kid; echo $o::viaObject('z'), "\n";
        echo $p::f(), "\n";
        try { echo $p::nope(); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "Kid:anything:1,2\nBase:spread:3,4\nBase:sec:|sec\nKid:viaObject:z\nf\nError: Call to undefined method Plain::nope()\n"
    );
}

/// Verifies `$receiver::method()` dispatches on the receiver's runtime class: statics resolve
/// with late static binding, `$this::inst()` runs on `$this`, an instance method from outside
/// throws PHP's "cannot be called statically" Error, and an unknown method is a catchable Error.
#[test]
fn test_dynamic_receiver_static_method_call() {
    let out = compile_and_run(
        r#"<?php
        class A {
            public static function hi($x) { return "A:$x"; }
            public static function who() { return static::class; }
            public static function log($m): void { echo "log:$m\n"; }
            public function inst() { return 'inst:' . static::class; }
            public function viaThis() { return $this::inst() . '/' . $this::who(); }
        }
        class B extends A { public static function hi($x) { return "B:$x"; } }
        function m(mixed $r) { return $r::hi('m') . '/' . $r::who(); }
        $a = new A; $b = new B; $c = 'b';
        echo $a::hi(1), '|', $b::hi(2), '|', $c::hi(3), '|', $b::who(), "\n";
        echo m($b), '|', m('A'), '|', $b->viaThis(), "\n";
        var_dump($b::log('x'));
        try { $a::inst(); } catch (\Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
        try { $a::nope(); } catch (\Error $e) { echo $e->getMessage(), "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "A:1|B:2|B:3|B\nB:m/B|A:m/A|inst:B/B\nlog:x\nNULL\n\
         Error: Non-static method A::inst() cannot be called statically\n\
         Call to undefined method A::nope()\n"
    );
}

/// Verifies the variable-method-name form (`$cls::$m()`) keeps its existing callable path.
/// Control: already passes before this change.
#[test]
fn test_dynamic_receiver_variable_method_name_still_works() {
    let out = compile_and_run(
        r#"<?php
        class A { public static function hi() { return 'hi'; } }
        $c = 'A'; $m = 'hi';
        echo $c::$m(), "\n";
        "#,
    );
    assert_eq!(out, "hi\n");
}

/// Verifies a by-reference argument reached through `$receiver::method()` throws a catchable
/// Error in that arm instead of silently passing a copy.
#[test]
fn test_dynamic_receiver_static_call_by_ref_argument_throws() {
    let out = compile_and_run(
        r#"<?php
        class A { public static function bump(&$n) { $n++; } }
        $b = new A; $n = 1;
        try { $b::bump($n); echo $n; } catch (\Error $e) { echo 'E:', $n; }
        "#,
    );
    assert_eq!(out, "E:1");
}

/// Verifies `--strict-php` accepts `$obj::method()` although it desugars to an internal call.
#[test]
fn test_dynamic_receiver_static_call_is_accepted_by_strict_php() {
    let out = compile_cli_file_and_run_with_flags(
        r#"<?php
        class Shell { public static function v() { return 'v1'; } }
        function v($shell) { return $shell::v(); }
        echo v(new Shell()), v('Shell');
        "#,
        &["--strict-php"],
    );
    assert_eq!(out, "v1v1");
}

/// Verifies PHP's `new` class-name references: `new static::$prop(...)` (late static binding),
/// `new self::$prop`, `new C::$prop`, `new $obj->prop(...)` and `new $obj->map['k'](...)`.
#[test]
fn test_new_with_dynamic_class_name_reference() {
    let out = compile_and_run(
        r#"<?php
        class Q { public function __construct(public string $v = "q") {} }
        class R extends Q {}
        class M2 {
            public static $builder = Q::class;
            public static function make($x) { return new static::$builder($x); }
            public static function bare() { return new self::$builder; }
        }
        class M3 extends M2 { public static $builder = R::class; }
        class P { public $cls = R::class; public $map = ['q' => Q::class]; }
        $p = new P;
        echo get_class(M2::make('a')), M2::make('a')->v, '|', get_class(M3::make('b')), '|', M2::bare()->v, "\n";
        echo get_class(new $p->cls('c')), '|', (new $p->map['q']('d'))->v, '|', get_class(new M2::$builder), "\n";
        $names = ['Q']; echo get_class(new $p->cls), (new $names[0])->v, "\n";
        "#,
    );
    assert_eq!(out, "Qa|R|q\nR|d|Q\nRq\n");
}

/// Verifies `new Foo` / `new $class(...)` keep compiling as before.
/// Control: already passes before this change.
#[test]
fn test_new_plain_forms_still_work() {
    let out = compile_and_run(
        r#"<?php
        class Q { public function __construct(public string $v = "q") {} }
        $c = 'Q';
        echo (new Q)->v, (new $c('x'))->v, new Q('y')->v, "\n";
        "#,
    );
    assert_eq!(out, "qxy\n");
}

/// Verifies `$class::CONST` / `$class::$prop` apply PHP visibility from the calling scope, and
/// that an unrelated class's private static of the same name does not break the dispatch.
#[test]
fn test_dynamic_member_read_respects_visibility() {
    let out = compile_and_run(
        r#"<?php
        class A { private const P = 'priv'; protected const R = 'prot';
          protected static $sp = 'sp';
          public static function inside($c) { return $c::P . $c::R . $c::$sp; } }
        class K { private static $cache = 'k'; public static function get() { return self::$cache; } }
        class B { public static $cache = 'b'; }
        function g($c, $n) {
            try { return $n === 'P' ? $c::P : ($n === 'R' ? $c::R : $c::$sp); }
            catch (\Error $e) { return 'E:' . $e->getMessage(); }
        }
        echo A::inside('A'), "\n", g('A', 'P'), "\n", g(new A, 'R'), "\n", g('A', 'S'), "\n";
        $c = 'B'; echo $c::$cache, K::get(), "\n";
        "#,
    );
    assert_eq!(
        out,
        "privprotsp\nE:Cannot access private constant A::P\nE:Cannot access protected constant A::R\n\
         E:Cannot access protected property A::$sp\nbk\n"
    );
}

/// Verifies `$this::$p` reads the runtime class's own static (a subclass redeclaring it).
#[test]
fn test_this_static_property_uses_runtime_class() {
    let out = compile_and_run(
        r#"<?php
        class A { const X = 'A'; public static $p = 'pa';
          public function f() { $t = $this; return $this::X . $t::X . $this::$p . $t::$p; } }
        class B extends A { const X = 'B'; public static $p = 'pb'; }
        echo (new A)->f(), '|', (new B)->f(), "\n";
        "#,
    );
    assert_eq!(out, "AApapa|BBpbpb\n");
}

/// Verifies `$obj::m()` applies method visibility from the calling scope (PHP's messages), and
/// that a private static inherited from the declaring scope is callable through a subclass.
#[test]
fn test_dynamic_static_call_respects_visibility() {
    let out = compile_and_run(
        r#"<?php
        class A { private static function sec() { return 'secret'; } protected static function pro() { return 'prot'; }
          public static function callSec($o) { return $o::sec() . $o::pro(); } }
        class B extends A {}
        $b = new B; $a = new A;
        echo A::callSec(new B), "\n";
        try { echo $a::sec(), "\n"; } catch (\Error $e) { echo 'E:', $e->getMessage(), "\n"; }
        try { echo $b::pro(), "\n"; } catch (\Error $e) { echo 'E:', $e->getMessage(), "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "secretprot\nE:Call to private method A::sec() from global scope\n\
         E:Call to protected method A::pro() from global scope\n"
    );
}

/// Verifies candidates with a different arity do not break `$x::m(...)`: surplus arguments
/// are evaluated and ignored like PHP, too few throw ArgumentCountError, literal spreads count.
#[test]
fn test_dynamic_static_call_arity_per_candidate() {
    let out = compile_and_run(
        r#"<?php
        function t($v) { echo "eval($v) "; return $v; }
        class A { public static function f($a, $b = 'd') { return "$a$b"; } }
        class B { public static function f(int $a) { return $a * 2; } }
        class C { public static function f($a, $b) { return "C$a$b"; } }
        function r($o) { echo "recv "; return $o; }
        echo r('A')::f(t(1), t(2)), "\n";
        echo r(new B)::f(t(3), t(4)), "\n";
        foreach (['A', 'B'] as $c) { echo $c::f(5), "\n"; }
        echo r('A')::f(...[7, 8]), "\n";
        try { $c = 'C'; echo $c::f(1), "\n"; } catch (\ArgumentCountError $e) { echo get_class($e), "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "recv eval(1) eval(2) 12\nrecv eval(3) eval(4) 6\n5d\n10\nrecv 78\nArgumentCountError\n"
    );
}

/// Regression: a `$class::m()` arm that throws `ArgumentCountError` for too few arguments
/// still evaluates the supplied arguments first, in order, as PHP does.
#[test]
fn test_dynamic_static_call_too_few_arguments_evaluates_supplied_arguments() {
    let out = compile_and_run(
        r#"<?php
        class C { public static function f($a, $b) { return $a . $b; } }
        function side(int $n) { echo "side$n\n"; return $n; }
        $c = 'C';
        try { $c::f(side(1)); } catch (\ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "side1\nToo few arguments to function C::f(), 1 passed and exactly 2 expected\n"
    );
}

/// Verifies a class-name string loses exactly one leading backslash, like PHP.
#[test]
fn test_dynamic_class_name_strips_one_leading_backslash() {
    let out = compile_and_run(
        r#"<?php
        namespace App\Sub;
        class Kid { const V = 'kid'; }
        function r(string $c) { try { return $c::V; } catch (\Error $e) { return 'E'; } }
        echo r('\App\Sub\Kid'), '|', r('\\\\App\Sub\Kid'), "\n";
        "#,
    );
    assert_eq!(out, "kid|E\n");
}

/// Verifies `new $this->prop(...)` and its chains.
#[test]
fn test_new_with_this_property_class_reference() {
    let out = compile_and_run(
        r#"<?php
        class Q { public string $v; public function __construct($v = 'q') { $this->v = $v; } }
        class R extends Q {}
        class M { public $o; public $arr = ['k' => R::class];
          public function inst() { return new $this->o('i'); } public function bare() { return new $this->arr['k']; } }
        $m = new M; $m->o = 'R';
        echo get_class($m->inst()), $m->inst()->v, get_class($m->bare()), "\n";
        "#,
    );
    assert_eq!(out, "RiR\n");
}

/// Verifies an instance method that a subclass overrides, reached through `$obj::m()` from a
/// related class, is refused at compile time instead of running the wrong body.
#[test]
fn test_dynamic_static_call_overridden_instance_method_is_refused() {
    let error = compile_cli_file_with_flags_expect_failure(
        r#"<?php
        class A { public function hi() { return 'A'; } public function via(A $o) { return $o::hi(); } }
        class B extends A { public function hi() { return 'B'; } }
        echo (new A)->via(new A);
        "#,
        &[],
    );
    assert!(error.contains("because a subclass overrides it"), "{error}");
}

/// Verifies a variadic parameter never counts as required in `$x::m(...)` arity checks.
/// Control: already passes before this change.
#[test]
fn test_dynamic_static_call_variadic_is_not_required() {
    let out = compile_and_run(
        r#"<?php
        class D { public static function f($a, $b, $c = "z") { return "D$a$b$c"; } }
        class E { public static function f($a, ...$r) { return "E$a" . count($r); } }
        class F { public static function f($a, $b = "y", ...$r) { return "F$a$b" . count($r); } }
        class G { public static function f(...$r) { return "G" . count($r); } }
        foreach (['E', 'F', 'G'] as $c) { echo $c::f(1), $c::f(1, 2, 3), "|"; }
        echo "\n";
        "#,
    );
    assert_eq!(out, "E10E12|F1y0F121|G1G3|\n");
}

/// Verifies a by-reference candidate only affects its own arm: other receivers at the same site
/// compile and run, the by-ref class works when the site does not pass the by-ref argument, and
/// passing it throws a catchable Error in that arm only (Nette's `Image::fromString(.., &$type)`).
#[test]
fn test_dynamic_static_call_by_ref_candidate_is_arm_local() {
    let out = compile_and_run(
        r#"<?php
        class Img { public static function fromString($s, &$type = null) { $type = 't'; return "img:$s"; } }
        class Uid { public static function fromString($v) { return "uid:$v"; } }
        function make(string $c, $v) { return $c::fromString($v); }
        function make2(string $c, $v) { try { $t = null; return $c::fromString($v, $t); } catch (\Error $e) { return 'E'; } }
        echo make('Uid', 'a'), '|', make('Img', 'b'), '|', make2('Img', 'c'), "\n";
        "#,
    );
    assert_eq!(out, "uid:a|img:b|E\n");
}

/// Verifies an inherited private static whose body depends on late static binding in any
/// form (`static::`, `new static`, `instanceof static`, a forwarding `self::`/`parent::` call,
/// even Carbon's `static::$lastErrors = ...`) throws in its arm instead of binding `static` to
/// the declaring class; a body free of it runs normally.
#[test]
fn test_dynamic_static_call_inherited_private_late_static() {
    let out = compile_and_run(
        r#"<?php
        class A0 { public static function who() { return static::class; } }
        class P extends A0 {
          protected static $last = '';
          public static function run(string $m) { $o = new static(); try { return $o::$m(); } catch (\Error $e) { return 'E'; } }
          public static function a() { $o = new static(); try { return $o::viaSelf(); } catch (\Error $e) { return 'E'; } }
          public static function b() { $o = new static(); try { return $o::viaParent(); } catch (\Error $e) { return 'E'; } }
          public static function c() { $o = new static(); try { return $o::viaInstanceof(); } catch (\Error $e) { return 'E'; } }
          public static function d() { $o = new static(); try { return $o::viaProp(); } catch (\Error $e) { return 'E'; } }
          public static function e() { $o = new static(); try { return $o::plain(); } catch (\Error $e) { return 'E'; } }
          private static function viaSelf() { return self::helper(); }
          private static function helper() { return static::class; }
          private static function viaParent() { return parent::who(); }
          private static function viaInstanceof() { $o = new K; return $o instanceof static ? 'y' : 'n'; }
          private static function viaProp() { static::$last = 'x'; return 'set'; }
          private static function plain() { return 'plain'; }
        }
        class K extends P {}
        echo K::a(), K::b(), K::c(), K::d(), K::e(), "\n";
        "#,
    );
    assert_eq!(out, "EEEEplain\n");
}

/// Verifies a non-literal spread that may fill a by-reference parameter throws in that arm.
#[test]
fn test_dynamic_static_call_spread_into_by_ref_throws() {
    let out = compile_and_run(
        r#"<?php
        class Img { public static function f($s, &$t = null) { $t = "T"; return "img:$s"; } }
        $c = "Img"; $a = ["s", "orig"];
        try { echo $c::f(...$a); } catch (\Error $e) { echo 'E'; }
        echo $a[1], "\n";
        "#,
    );
    assert_eq!(out, "Eorig\n");
}

/// Verifies `unset($class::$prop[...])` is refused at compile time.
#[test]
fn test_dynamic_static_property_unset_is_refused() {
    let error = compile_cli_file_with_flags_expect_failure(
        r#"<?php
        class C { public static $p = ['n' => 1]; }
        $c = 'C'; unset($c::$p['n']);
        "#,
        &[],
    );
    assert!(error.contains("Unsetting through a dynamic class"), "{error}");
}

/// Verifies `new static::$classes[static::class]($m)` (Eloquent `HasCollection`) and
/// `new $this->class(...)` with a reserved word as the property name build the class the chain
/// names at runtime.
#[test]
fn test_new_with_extended_class_name_reference() {
    let out = compile_and_run(
        r#"<?php
class Pivot { public function __construct(public string $tag = 'p') {} }
class Other extends Pivot {}
class Collection { public function __construct(public array $items = []) {} }
class Model
{
    protected static array $classes = [];
    public string $class = Other::class;
    public function viaProp() { return new $this->class('k'); }
    public static function boot(): void { static::$classes[static::class] = Collection::class; }
    public function collect(array $m) { return new static::$classes[static::class]($m); }
}
$m = new Model();
echo get_class($m->viaProp()), ' ', $m->viaProp()->tag, "\n";
Model::boot();
echo count($m->collect([1, 2, 3])->items), "\n";
"#,
    );
    assert_eq!(out, "Other k\n3\n");
}
