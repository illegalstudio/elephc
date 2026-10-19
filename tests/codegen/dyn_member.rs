//! Purpose:
//! End-to-end coverage for dynamic member and call forms that real-world vendor
//! code uses: runtime method names on named class receivers, first-class callables of
//! arbitrary expressions, callable array literals, dynamic-property and mid-chain appends,
//! foreach place targets and named arguments in dynamic method calls.
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

/// Verifies first-class callables of arbitrary expressions: `($this->callback)(...)`,
/// `$this(...)`, `call()(...)`, `$o->$m(...)` and `($a['k'])(...)`, plus a direct call of an
/// untyped property holding a Closure.
#[test]
fn test_expression_first_class_callables() {
    let out = compile_and_run(
        r#"<?php
        class R {
            public $callback;
            public function __construct() { $this->callback = fn($x = 'd') => "cb:$x"; }
            public function __invoke($a) { return "invoke:$a"; }
            public function get() { return ($this->callback)(...); }
            public function self() { return $this(...); }
            public function factory() { return fn() => fn($y) => "inner:$y"; }
            public function named($m) { return $this->$m(...); }
            public function upper($s) { return strtoupper($s); }
        }
        $r = new R();
        $f = $r->get(); echo $f('a'), "\n";
        $f = $r->self(); echo $f('b'), "\n";
        $fac = $r->factory(); $f = $fac()(...); echo $f('c'), "\n";
        $f = $r->named('upper'); echo $f('d'), "\n";
        $opts = ['decider' => 'strrev'];
        $f = ($opts['decider'])(...); echo $f('abc'), "\n";
        echo ($r->callback)('e'), "\n";
        "#,
    );
    assert_eq!(out, "cb:a\ninvoke:b\ninner:c\nD\ncba\ncb:e\n");
}

/// Verifies `$this->{$property}[] = v` (Mailable.php) appends to the named property, with the
/// right-hand side evaluated before the property is fetched for writing, and that `$this[] = v`
/// / `$obj[] = v` on a user ArrayAccess call `offsetSet(null, v)`.
#[test]
fn test_dynamic_property_append_and_array_access_push() {
    let out = compile_and_run(
        r#"<?php
        class M {
            public array $to = [];
            public array $cc = [];
            public function add($property, $address) {
                $this->{$property}[] = ['address' => $address];
                return $this;
            }
            public function grow($property) { $this->{$property}[] = $this->add($property, 'inner') ? 'outer' : 'no'; return $this; }
        }
        $m = (new M())->add('to', 'a@x')->add('cc', 'b@x')->add('to', 'c@x')->grow('cc');
        echo json_encode([$m->to, $m->cc]), "\n";
        class Coll implements ArrayAccess {
            public $log = '';
            public function offsetExists($o): bool { return false; }
            public function offsetGet($o): mixed { return null; }
            public function offsetSet($o, $v): void { $this->log .= var_export($o, true) . '=' . $v . ';'; }
            public function offsetUnset($o): void { }
            public function add($e) { $this[] = $e; $this['k'] = $e; return $this->log; }
        }
        $c = new Coll();
        $c[] = 'z';
        echo $c->add('x'), "\n";
        "#,
    );
    assert_eq!(
        out,
        "[[{\"address\":\"a@x\"},{\"address\":\"c@x\"}],[{\"address\":\"b@x\"},{\"address\":\"inner\"},\"outer\"]]\nNULL=z;NULL=x;'k'=x;\n"
    );
}

/// Verifies appends in the middle of a write chain (SendGridHandler.php, cmap.php): every `[]`
/// writes into a fresh element.
#[test]
fn test_mid_chain_append_writes() {
    let out = compile_and_run(
        r#"<?php
        function send(array $to, $from) {
            $body = [];
            $body['personalizations'] = [];
            $body['from'] = ['email' => $from];
            foreach ($to as $recipient) {
                $body['personalizations'][]['to'][]['email'] = $recipient;
            }
            return json_encode($body);
        }
        echo send(['a@x', 'b@x'], 'me@x'), "\n";
        $segments = [];
        $segments[][] = [0xFFFF, null];
        $segments[][] = 7;
        echo json_encode($segments), "\n";
        class H { public $b = []; function f() { $this->b['x'][][] = 1; $this->b['x'][]['k'] = 2; return $this->b; } }
        echo json_encode((new H())->f()), "\n";
        "#,
    );
    assert_eq!(
        out,
        "{\"personalizations\":[{\"to\":[{\"email\":\"a@x\"}]},{\"to\":[{\"email\":\"b@x\"}]}],\"from\":{\"email\":\"me@x\"}}\n[[[65535,null]],[7]]\n{\"x\":[[1],{\"k\":2}]}\n"
    );
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

/// Verifies semi-reserved keyword member names after a dynamic receiver (`$factory::new()`,
/// Eloquent Factory.php) and PHP's trailing-dot float default `0.` (fidry/cpu-core-counter).
#[test]
fn test_dynamic_receiver_keyword_member_and_trailing_dot_float() {
    let out = compile_and_run(
        r#"<?php
        class UserFactory {
            public static function new($n = 1) { return new static(); }
            public function count() { return 'factory:' . static::class; }
            const LIST = 'list';
        }
        function load(
            ?float $systemLoadAverage = 0.
        ): string { return var_export($systemLoadAverage, true); }
        $factory = UserFactory::class;
        echo $factory::new()->count(), "\n";
        $obj = new UserFactory();
        echo $obj::new(2)->count(), "\n";
        echo $factory::LIST, "\n";
        echo load(), ' ', load(2.5), "\n";
        "#,
    );
    assert_eq!(out, "factory:UserFactory\nfactory:UserFactory\nlist\n0.0 2.5\n");
}

/// Verifies `$this->{$property}[] = v` on UNTYPED `[]`-default properties (Mailable.php:769,
/// including a slot only a subclass declares: a runtime-named write of a boxed
/// value boxes every untyped slot the name could reach.
#[test]
fn test_dynamic_property_append_on_untyped_properties() {
    let out = compile_and_run(
        r#"<?php
        class Base { public $to = []; public function add($p, $v) { $this->{$p}[] = $v; } }
        class Mail extends Base { public $cc = []; public $subject = 'x'; }
        $m = new Mail; $m->add('to', 'a'); $m->add('cc', ['address' => 'b']); $m->add('to', 'c');
        echo json_encode([$m->to, $m->cc, $m->subject]), "\n";
        $m->subject = 'y'; echo $m->subject, count($m->to), "\n";
        class D { public $data = []; }
        $d = new D; $n = 'data';
        foreach ([1, 2] as $d->{$n}[]) {}
        echo json_encode($d->data), "\n";
        "#,
    );
    assert_eq!(out, "[[\"a\",\"c\"],[{\"address\":\"b\"}],\"x\"]\ny2\n[1,2]\n");
}

/// Verifies a mid-chain append into an object place throws an explicit elephc
/// `Error` instead of calling `offsetSet(null, [...])`, which PHP never does there.
#[test]
fn test_mid_chain_append_into_object_throws_explicit_error() {
    let out = compile_and_run(
        r#"<?php
        class Bag implements ArrayAccess {
            public array $d = [];
            public function offsetExists(mixed $o): bool { return false; }
            public function offsetGet(mixed $o): mixed { return null; }
            public function offsetSet(mixed $o, mixed $v): void { $this->d[] = $v; }
            public function offsetUnset(mixed $o): void {}
        }
        $bb = new Bag;
        try { $bb[][] = 5; } catch (Error $e) { echo $e->getMessage(), "\n"; }
        try { $bb[]['k'] = 6; } catch (Error $e) { echo 'again', "\n"; }
        echo json_encode($bb->d), "\n";
        "#,
    );
    assert_eq!(
        out,
        "elephc: a write through `[]` into an object (`$obj[][...] = v`) is not supported: PHP fetches the new element with offsetGet(); write the element explicitly\nagain\n[]\n"
    );
}

/// Verifies `static::{$n}()` inside an instance method reaches instance methods through `$this`,
/// and still refuses them from a static context.
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

/// Verifies a callable array held in an untyped property is called and converted at run time,
/// and `$obj[] = v` reaches a user override of ArrayObject::offsetSet.
#[test]
fn test_property_callable_array_and_array_object_override_push() {
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
        class MyAO extends ArrayObject { public function offsetSet(mixed $k, mixed $v): void { echo "MyAO set(" . var_export($k, true) . ")\n"; parent::offsetSet($k, $v); } }
        $m = new MyAO([]); $m[] = 'z'; $m['k'] = 'w'; echo json_encode($m->getArrayCopy()), "\n";
        "#,
    );
    assert_eq!(out, "invoke abc def\nMyAO set(NULL)\nMyAO set('k')\n{\"0\":\"z\",\"k\":\"w\"}\n");
}

/// Verifies a runtime-named push in a class whose other methods write its properties directly
/// (Mailable's `tag()`, `metadata()`, `with()` next to `setAddress()`): no slot is
/// degraded, so the direct pushes and keyed writes still compile and behave like PHP.
#[test]
fn test_dynamic_property_push_next_to_direct_property_writes() {
    let out = compile_and_run(
        r#"<?php
        class Mailable {
            public $to = []; public $tags = []; public $metadata = []; public $callbacks = []; public $viewData = [];
            public function tag($v) { $this->tags[] = $v; return $this; }
            public function metadata($k, $v) { $this->metadata[$k] = $v; return $this; }
            public function with($k, $v) { $this->viewData[$k] = $v; return $this; }
            public function withSymfony($cb) { $this->callbacks[] = $cb; return $this; }
            public function setAddress($a, $property) { $this->{$property}[] = $a; return $this; }
        }
        $m = new Mailable;
        $m->setAddress(['address' => 'x'], 'to'); $m->tag('t1'); $m->metadata('k', 'v'); $m->with('w', 1); $m->withSymfony(fn() => 1);
        echo json_encode([$m->to, $m->tags, $m->metadata, $m->viewData, count($m->callbacks)]), "\n";
        class Fill { public $to = []; public $cc = [];
            public function fill(array $data) { foreach ($data as $k => $v) { $this->{$k} = $v; } }
            public function direct() { $this->to[] = 'd'; }
            public function read() { return count($this->to); } }
        $f = new Fill; $f->direct(); echo $f->read(), "\n";
        "#,
    );
    assert_eq!(out, "[[{\"address\":\"x\"}],[\"t1\"],{\"k\":\"v\"},{\"w\":1},1]\n1\n");
}

/// Verifies `$this->{$p}[] = v` on string, int, object, readonly, private and subclass-only slots,
/// and `$mixed[] = v` on scalars: PHP's Errors instead of a silent no-op.
#[test]
fn test_dynamic_property_push_on_non_array_slots_throws_php_errors() {
    let out = compile_and_run(
        r#"<?php
        class P { public $s = 'str'; public $n = 5; public $o; private $hidden = [];
          public function __construct() { $this->o = new stdClass; }
          public function add($p, $v) { try { $this->{$p}[] = $v; echo "ok\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; } } }
        class C extends P { public $only = []; }
        $c = new C;
        foreach (['s', 'n', 'o', 'only', 'only'] as $k) { $c->add($k, 1); }
        echo json_encode($c->only), "\n";
        function f($x) { try { $x[] = 1; return json_encode($x); } catch (Error $e) { return get_class($e) . ": " . $e->getMessage(); } }
        echo f('s'), "\n", f(5), "\n", f([]), "\n", f(null), "\n";
        "#,
    );
    assert_eq!(
        out,
        "Error: [] operator not supported for strings\nError: Cannot use a scalar value as an array\nError: Cannot use object of type stdClass as array\nok\nok\n[1,1]\nError: [] operator not supported for strings\nError: Cannot use a scalar value as an array\n[1]\n[1]\n"
    );
}

/// Verifies Mailable::setAddress's shape: a runtime-named push followed by a
/// runtime-named array write back into the same slot, next to direct writes of other slots. The
/// array write is stored with the selected slot's static name, so the refined slots keep their
/// representation.
#[test]
fn test_mailable_set_address_runtime_named_push_and_write() {
    let out = compile_and_run(
        r#"<?php
        class Mailable {
            public $to = []; public $cc = []; public $tags = []; public $metadata = [];
            protected function setAddress($address, $name = null, $property = 'to') {
                $this->{$property}[] = ['address' => $address, 'name' => $name];
                $this->{$property} = array_values($this->{$property});
                return $this;
            }
            public function to($a, $n = null) { return $this->setAddress($a, $n, 'to'); }
            public function cc($a, $n = null) { return $this->setAddress($a, $n, 'cc'); }
            public function tag($v) { $this->tags[] = $v; return $this; }
            public function metadata($k, $v) { $this->metadata[$k] = $v; return $this; }
        }
        $m = new Mailable;
        $m->to('a@x', 'A'); $m->to('b@x', 'B'); $m->cc('c@x'); $m->tag('t1'); $m->metadata('k', 'v');
        echo json_encode([$m->to, $m->cc, $m->tags, $m->metadata]), "\n";
        "#,
    );
    assert_eq!(
        out,
        "[[{\"address\":\"a@x\",\"name\":\"A\"},{\"address\":\"b@x\",\"name\":\"B\"}],[{\"address\":\"c@x\",\"name\":null}],[\"t1\"],{\"k\":\"v\"}]\n"
    );
}
