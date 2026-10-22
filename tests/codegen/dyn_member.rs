//! Purpose:
//! End-to-end coverage for dynamic member and call forms: runtime method names on named class
//! receivers, callable array literals, untyped property callees, foreach place targets and named
//! arguments in dynamic method calls.
//!
//! Called from:
//! - The native codegen suite (`tests/codegen/mod.rs`).
//!
//! Key details:
//! - Expected output was produced by PHP 8.5 from the same source.

use crate::support::*;

/// Verifies `static::{expr}()` (Carbon Units.php), `self::{$n}()`, `C::{...}()` and
/// `parent::{...}()` keep the late static binding of the named call, and that an unknown name
/// throws PHP's `Call to undefined method` with the called class.
#[test]
fn test_scoped_computed_method_name_keeps_late_static_binding() {
    let out = compile_and_run(
        r#"<?php
        class Base {
            public $localMonthOverflow = null;
            public static function shouldOverflowMonth() { return 'base:' . static::class; }
            public function inst($x) { return 'inst:' . $x . ':' . get_class($this); }
            public function pick($unit) {
                return $this->{'local' . $unit . 'Overflow'} ?? static::{'shouldOverflow' . $unit}();
            }
            public function viaSelf($n) { return self::{$n}('s'); }
            public static function viaClass($n) { return Base::{'should' . $n}(); }
        }
        class Child extends Base {
            public static function shouldOverflowMonth() { return 'child:' . static::class; }
            public function viaParent() { return parent::{'shouldOverflow' . 'Month'}(); }
        }
        $c = new Child();
        echo $c->pick('Month'), "\n";
        $c->localMonthOverflow = 'local';
        echo $c->pick('Month'), "\n";
        echo $c->viaSelf('inst'), "\n";
        echo Child::viaClass('OverflowMonth'), "\n";
        echo $c->viaParent(), "\n";
        try { $c->pick('Year'); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "child:Child\nlocal\ninst:s:Child\nbase:Base\nbase:Child\nError: Call to undefined method Child::shouldOverflowYear()\n"
    );
}

/// Verifies `self::$method($a, $b)` calls the method NAMED by the local (IpUtils.php), not a
/// static property, and that the static property of the same name is still readable.
#[test]
fn test_self_variable_method_call_uses_local_name() {
    let out = compile_and_run(
        r#"<?php
        class IpUtils {
            public static $method = 'notThis';
            public static function checkIp4($ip, $range) { return "v4($ip,$range)"; }
            public static function checkIp6($ip, $range) { return "v6($ip,$range)"; }
            public static function notThis() { return 'wrong'; }
            public static function check($ip, $range) {
                $method = substr_count($ip, ':') > 1 ? 'checkIp6' : 'checkIp4';
                if (self::$method($ip, $range)) {
                    return self::$method($ip, $range);
                }
                return 'no';
            }
        }
        echo IpUtils::check('1.2.3.4', '1.2.3.0/24'), "\n";
        echo IpUtils::check('::1', '::1/128'), "\n";
        echo IpUtils::$method, "\n";
        "#,
    );
    assert_eq!(out, "v4(1.2.3.4,1.2.3.0/24)\nv6(::1,::1/128)\nnotThis\n");
}

/// Verifies the per-arm runtime rules of a runtime method name: `$this` forwarding for an
/// instance method, visibility with the `__callStatic` fallback, arity, a static context, and a
/// static method only a subclass declares under `static::`.
#[test]
fn test_scoped_dynamic_call_arm_rules() {
    let out = compile_and_run(
        r#"<?php
        class A {
            public function inst($x = 'i') { return "inst:$x:" . get_class($this); }
            private static function priv() { return 'priv'; }
            public static function two($a, $b) { return "two:$a$b"; }
            public static function who() { return static::class; }
            public static function __callStatic($n, $args) { return "magic:$n(" . implode(',', $args) . ')'; }
            public function viaSelf($m, $x = null, $y = null) {
                return $x === null ? self::$m() : ($y === null ? self::$m($x) : self::$m($x, $y));
            }
            public static function viaStatic($m) { return static::{$m}(); }
            public static function viaStaticNoThis($m) { return self::$m(); }
        }
        class B extends A {
            public static function onlyB() { return 'onlyB:' . static::class; }
            public function viaParent($m) { return parent::$m('p'); }
        }
        class Other { public static function call($m) { return A::$m(); } }
        $b = new B();
        echo $b->viaSelf('inst', 'x'), "\n";
        echo $b->viaSelf('priv'), "\n";
        echo $b->viaSelf('WHO'), "\n";
        echo $b->viaSelf('nope', 1, 2), "\n";
        echo $b->viaParent('inst'), "\n";
        echo B::viaStatic('who'), "\n";
        echo Other::call('priv'), "\n";
        foreach ([fn() => $b->viaSelf('two', 1), fn() => A::viaStaticNoThis('inst'), fn() => B::viaStatic('onlyB'), fn() => Other::call('inst')] as $f) {
            try { echo $f(), "\n"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
        }
        $m = 5;
        try { echo A::$m(); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "inst:x:B\npriv\nB\nmagic:nope(1,2)\ninst:p:B\nB\nmagic:priv()\n\
         ArgumentCountError: Too few arguments to function A::two(), 1 passed and exactly 2 expected\n\
         Error: Non-static method A::inst() cannot be called statically\n\
         onlyB:B\n\
         Error: Non-static method A::inst() cannot be called statically\n\
         Error: Method name must be a string\n"
    );
}

/// Verifies a subclass-only static method reached through `static::{$name}()` whose body
/// observes the called class runs only when the called class is the declaring subclass, and
/// throws an explicit elephc `Error` instead of binding `static` to the wrong class.
#[test]
fn test_scoped_dynamic_call_subclass_late_static_binding_is_refused_per_arm() {
    let out = compile_and_run(
        r#"<?php
        class A { public static function run($m) { return static::{$m}(); } }
        class B extends A { public static function made() { return 'made:' . static::class; } }
        class C extends B {}
        echo B::run('made'), "\n";
        try { echo C::run('made'), "\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
        try { echo A::run('who'), "\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "made:B\nelephc: calling B::made() through `static::` with a runtime method name from a subclass of B is not supported because it depends on late static binding; name the method\nCall to undefined method A::who()\n"
    );
}

/// Verifies a direct call on a callable array literal (RegisterListenersPass.php).
#[test]
fn test_callable_array_literal_direct_call() {
    let out = compile_and_run(
        r#"<?php
        class Sub {
            public static function getSubscribedEvents() { return ['a' => 'onA', 'b' => 'onB']; }
            public function hello($n) { return "hello $n"; }
        }
        class Pass {
            private static $subscriber = 'Sub';
            public function run() {
                foreach ([self::$subscriber, 'getSubscribedEvents']() as $eventName => $params) {
                    echo $eventName, '=', $params, "\n";
                }
                echo [new Sub(), 'hello']('x'), "\n";
            }
        }
        (new Pass())->run();
        "#,
    );
    assert_eq!(out, "a=onA\nb=onB\nhello x\n");
}

/// Verifies foreach key/value targets that are places (var-dumper Data.php), assigned value
/// first, then key, as PHP does; and an append value target.
#[test]
fn test_foreach_place_targets() {
    let out = compile_and_run(
        r#"<?php
        class Stub { public $class; public $position; public $list = []; }
        $stub = new Stub();
        foreach (['A' => 1, 'B' => 2] as $stub->class => $stub->position) {
            echo $stub->class, ':', $stub->position, "\n";
        }
        echo $stub->class, $stub->position, "\n";
        $arr = [];
        foreach ([10, 20] as $arr['k'] => $arr['v']) { echo json_encode($arr), "\n"; }
        foreach (['p', 'q'] as $stub->list[]) {}
        echo json_encode($stub->list), "\n";
        "#,
    );
    assert_eq!(out, "A:1\nB:2\nB2\n{\"v\":10,\"k\":0}\n{\"v\":20,\"k\":1}\n[\"p\",\"q\"]\n");
}

/// Verifies named arguments in a dynamic method call (Query/Builder.php) bind against the
/// runtime-resolved method, including an unpacked list, an unknown name and an unpacked string
/// key that a named argument would overwrite.
#[test]
fn test_dynamic_method_call_named_arguments() {
    let out = compile_and_run(
        r#"<?php
        class Q {
            public function where($column, $op = '=', $value = null, $boolean = 'and') {
                return "where($column $op " . var_export($value, true) . " $boolean)";
            }
            public function whereIn($column, $values, $boolean = 'and', $not = false) {
                return "whereIn($column " . json_encode($values) . " $boolean " . var_export($not, true) . ")";
            }
        }
        function addArrayOfWheres($query, $column, $boolean, $method) {
            foreach ($column as $key => $value) {
                echo $query->{$method}(...array_values($value), boolean: $boolean), "\n";
            }
        }
        addArrayOfWheres(new Q(), [['a', '>', 1], ['b', '<', 2]], 'or', 'where');
        $q = new Q(); $m = 'whereIn';
        echo $q->$m('x', [3], not: true), "\n";
        try { $q->$m('x', [3], nope: 1); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
        try { $q->{'where'}(...['column' => 'c', 'boolean' => 'x'], boolean: 'y'); } catch (Error $e) { echo $e->getMessage(), "\n"; }
        echo $q->{'where'}(...['column' => 'c'], value: 5), "\n";
        "#,
    );
    assert_eq!(
        out,
        "where(a > 1 or)\nwhere(b < 2 or)\nwhereIn(x [3] and true)\nError: Unknown named parameter $nope\nNamed parameter $boolean overwrites previous argument\nwhere(c = 5 and)\n"
    );
}

/// Verifies `static::{$n}()` inside an instance method reaches instance methods through `$this`
///, and still refuses them from a static context.
#[test]
fn test_static_computed_name_reaches_instance_methods() {
    let out = compile_and_run(
        r#"<?php
        class P {
            public function im() { return "im " . static::class . " " . get_class($this); }
            public static function callIt($n) { try { return static::{$n}(); } catch (Error $e) { return get_class($e) . ": " . $e->getMessage(); } }
            public function fromInst($n) { return static::{$n}(); }
            public function fromSelf($n) { return self::{$n}(); }
        }
        class Q extends P { public function im() { return "Q::im " . static::class; } }
        echo P::callIt('im'), "\n";
        echo (new Q)->fromInst('im'), "\n";
        echo (new Q)->fromSelf('im'), "\n";
        "#,
    );
    assert_eq!(
        out,
        "Error: Non-static method P::im() cannot be called statically\nQ::im Q\nim Q Q\n"
    );
}

/// Verifies PHP's `ArgumentCountError` text when a named-argument container leaves a required
/// parameter unbound: the callee name comes from the descriptor.
#[test]
fn test_named_argument_container_missing_argument_message() {
    let out = compile_and_run(
        r#"<?php
        class Q {
            public function w($col, $op = '=') { return "w($col,$op)"; }
            public static function s($x, $y) { return "s"; }
        }
        function f($a, $b = 1) { return "f$a$b"; }
        $q = new Q; $m = 'w';
        try { echo $q->{$m}(op: '<'), "\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
        foreach ([['f', ['b' => 2]], [['Q', 's'], ['y' => 1]], [fn($z) => 1, ['k' => 1]]] as [$cb, $args]) {
            try { echo call_user_func_array($cb, $args), "\n"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
        }
        "#,
    );
    assert_eq!(
        out,
        "ArgumentCountError: Q::w(): Argument #1 ($col) not passed\nArgumentCountError: f(): Argument #1 ($a) not passed\nArgumentCountError: Q::s(): Argument #1 ($x) not passed\nError: Unknown named parameter $k\n"
    );
}

/// Verifies a callable array and a Closure held in untyped properties are called at run time.
#[test]
fn test_property_callable_array_and_closure_callees() {
    let out = compile_and_run(
        r#"<?php
        class Inv { public function __invoke($a, $b = 'def') { return "invoke $a $b"; } }
        class T {
            public $cb;
            public function __construct($cb) { $this->cb = $cb; }
            public function run($a) { return ($this->cb)($a); }
        }
        $t = new T([new Inv, '__invoke']);
        echo $t->run('abc'), "\n";
        class R {
            public $callback;
            public function __construct() { $this->callback = fn($x = 'd') => "cb:$x"; }
        }
        $r = new R();
        echo ($r->callback)('e'), "\n";
        "#,
    );
    assert_eq!(out, "invoke abc def\ncb:e\n");
}

/// Regression: a plain variable passed by name to a runtime method name reaches a by-reference
/// parameter (`$o->$m(x: $n)` increments `$n`), while by-value parameters still see a copy.
#[test]
fn test_dynamic_method_named_argument_binds_by_reference() {
    let out = compile_and_run(
        r#"<?php
        class O {
            public function bump(&$x, $by = 1) { $x += $by; return $x; }
            public function val($a, $b = 'B') { return $a . $b; }
        }
        $o = new O; $m = 'bump'; $v = 'val';
        $n = 1; $k = 5;
        echo $o->$m(x: $n, by: $k), " ", $n, " ", $k, "\n";
        $s = 'a'; echo $o->$v(b: $s, a: 'z'), " ", $s, "\n";
        $p = 10; echo $o->$m(...[], x: $p), " ", $p, "\n";
        $copy = $n; $copy++; echo $n, " ", $copy, "\n";
        "#,
    );
    assert_eq!(out, "6 6 5\nza a\n11 11\n6 7\n");
}

/// Regression: a variable passed by name to a runtime method name keeps the references the
/// caller already holds: a by-reference write reaches `$alias =& $n` too, a by-value call
/// leaves the binding alone (a later `$n = ...` is still seen through `$alias`), and the same
/// holds inside a function and for a string or object value.
#[test]
fn test_dynamic_method_named_argument_keeps_existing_references() {
    let out = compile_and_run(
        r#"<?php
        class O {
            public function bump(&$x, $by = 1) { $x += $by; return $x; }
            public function val($a, $b = 'B') { return $a . $b; }
            public function suffix(&$s) { $s .= '!'; }
        }
        $o = new O; $m = 'bump'; $v = 'val'; $sfx = 'suffix';
        $n = 1; $alias =& $n;
        echo $o->$m(x: $n), " ", $n, " ", $alias, "\n";
        $w = $o->$v(a: $n); $n = 7; echo $w, " ", $alias, "\n";
        $k = 3; echo $o->$m(by: $k, x: $n), " ", $n, " ", $alias, " ", $k, "\n";
        $s = 'hi'; $t =& $s; $o->$sfx(s: $s); echo $s, $t, "\n";
        function f(O $o, string $m) { $q = 1; $r =& $q; $o->$m(x: $q, by: 4); return "$q $r"; }
        echo f($o, 'bump'), "\n";
        "#,
    );
    assert_eq!(out, "2 2 2\n2B 7\n10 10 10 3\nhi!hi!\n5 5\n");
}

/// Regression: a runtime name that is itself `__callStatic` / `__call` calls that method
/// directly with the supplied arguments; only an unmatched name takes the magic fallback.
#[test]
fn test_scoped_dynamic_call_explicit_magic_method_names() {
    let out = compile_and_run(
        r#"<?php
        class C {
            public static function __callStatic($name, $args) { echo "cs:", $name, ":", count($args), "\n"; }
            public function __call($name, $args) { echo "c:", $name, ":", count($args), "\n"; }
            public static function run() { $m = '__callStatic'; C::$m('given', []); static::$m('given', []); }
            public function inst() { $m = '__call'; self::$m('given', []); }
        }
        C::run();
        (new C)->inst();
        $m = 'other';
        C::$m(1, 2);
        "#,
    );
    assert_eq!(out, "cs:given:0\ncs:given:0\nc:given:0\ncs:other:2\n");
}

/// Regression: `self::$m()`, `static::$m()` and `C::$m()` arms that throw `ArgumentCountError`
/// still evaluate the supplied arguments first, as PHP does.
#[test]
fn test_scoped_dynamic_call_too_few_arguments_evaluates_supplied_arguments() {
    let out = compile_and_run(
        r#"<?php
        function sideEffect() { echo "side\n"; return 1; }
        class C {
            public static function two($a, $b) { return $a . $b; }
            public static function run() {
                $m = 'two';
                try { self::$m(sideEffect()); } catch (ArgumentCountError $e) { echo "ACE\n"; }
                try { static::$m(sideEffect()); } catch (ArgumentCountError $e) { echo "ACE\n"; }
            }
        }
        class Base { public static function run() { $m = 'only'; try { static::$m(sideEffect()); } catch (ArgumentCountError $e) { echo "ACE\n"; } } }
        class Sub extends Base { public static function only($a, $b) { return 1; } }
        C::run();
        Sub::run();
        $m = 'two';
        try { C::$m(sideEffect()); } catch (ArgumentCountError $e) { echo "ACE\n"; }
        "#,
    );
    assert_eq!(out, "side\nACE\nside\nACE\nside\nACE\nside\nACE\n");
}
