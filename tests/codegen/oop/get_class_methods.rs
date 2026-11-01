//! Purpose:
//! Integration tests for the natively compiled `get_class_methods()`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Visibility follows the CALLING scope at run time: public only from outside a class, a
//!   class's own private methods from that class, protected ones from any related class, and an
//!   ancestor's private method from that ancestor's own scope.
//! - A class only the interpreter knows goes to the eval bridge when it is linked.
//! - Expected output is php's own (`php -n`) for the same program.

use super::*;

/// Verifies scope-dependent visibility, php's method order, enums, a mixed value and a
/// case-insensitive class-name string.
///
/// Before the native lowering the call either did not exist (a static "undefined function") or,
/// with the eval bridge linked, went to the interpreter, which cannot see the compiled caller's
/// scope and so never listed a private or protected method.
#[test]
fn test_get_class_methods_follows_the_calling_scope() {
    let out = compile_and_run(
        r###"<?php
interface Shape { public function area(): float; }
abstract class Base implements Shape {
    public function describe() {}
    protected function helper() {}
    private function secret() {}
    public static function make() {}
    public function fromBase($o) { return implode(',', get_class_methods($o)); }
}
class Circle extends Base {
    public function area(): float { return 1.0; }
    protected function helper() {}
    private function own() {}
    public function fromCircle($o) { return implode(',', get_class_methods($o)); }
}
class Stranger { public function look($o) { return implode(',', get_class_methods($o)); } }
enum Suit: string { case H = 'H'; public function color() {} const X = 1; }

$c = new Circle();
echo 'global: ', implode(',', get_class_methods($c)), "\n";
echo 'base scope: ', $c->fromBase($c), "\n";
echo 'circle scope: ', $c->fromCircle($c), "\n";
echo 'stranger: ', (new Stranger())->look($c), "\n";
echo 'enum: ', implode(',', get_class_methods(Suit::H)), "\n";
echo 'enum str: ', implode(',', get_class_methods('Suit')), "\n";
$mixed = $argc > 5 ? 'Stranger' : $c;
echo 'mixed: ', implode(',', get_class_methods($mixed)), "\n";
$lower = 'circle';
echo 'lower: ', implode(',', get_class_methods($lower)), "\n";
try {
    get_class_methods('NoSuchClass');
} catch (TypeError $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}
"###,
    );
    assert_eq!(
        out,
        "global: area,fromCircle,describe,make,fromBase\n\
         base scope: area,helper,fromCircle,describe,secret,make,fromBase\n\
         circle scope: area,helper,own,fromCircle,describe,make,fromBase\n\
         stranger: area,fromCircle,describe,make,fromBase\n\
         enum: color,cases,from,tryFrom\n\
         enum str: color,cases,from,tryFrom\n\
         mixed: area,fromCircle,describe,make,fromBase\n\
         lower: area,fromCircle,describe,make,fromBase\n\
         TypeError: get_class_methods(): Argument #1 ($object_or_class) must be an object or a valid class name, string given\n"
    );
}

/// Verifies the same answers with the eval bridge linked, and that a class only eval declared
/// is still answered, through the bridge.
///
/// This is the `--web` configuration Symfony and Twig run under. The bridge used to answer every
/// call, so a method's own scope never saw its class's private or protected methods.
#[test]
fn test_get_class_methods_with_the_eval_bridge_linked() {
    let out = compile_and_run(
        r###"<?php
trait T { public function tm() {} }
class Base { public function baseOne() {} protected function prot() {} private function priv() {} public static function st() {} }
class Kid extends Base {
    use T;
    public function own() {} private function kidPriv() {}
    public function inside() { return get_class_methods($this); }
}
$k = new Kid();
echo implode(',', get_class_methods($k)), "\n";
echo implode(',', $k->inside()), "\n";
eval('class EvalOnly { public function e1() {} private function e2() {} }');
$name = $argc > 5 ? 'Base' : 'EvalOnly';
echo implode(',', get_class_methods($name)), "\n";
"###,
    );
    assert_eq!(
        out,
        "own,inside,tm,baseOne,st\nown,kidPriv,inside,tm,baseOne,prot,st\ne1\n"
    );
}

/// Verifies Exception and Error declare `__wakeup()` after `__construct()`, as php lists them,
/// and that a subclass may override it with php's tentative `void` return type.
///
/// The builtin Throwable classes lacked the method, so a subclass's method list skipped it and
/// `method_exists($e, '__wakeup')` answered false.
#[test]
fn test_get_class_methods_lists_the_throwable_wakeup() {
    let out = compile_and_run(
        r###"<?php
class MyEx extends RuntimeException { public function extra() {} }
class MyErr extends Error { public function __wakeup(): void {} }
echo implode(',', get_class_methods(new MyEx())), "\n";
echo implode(',', get_class_methods(new MyErr())), "\n";
var_dump(method_exists(new Exception(), '__wakeup'));
"###,
    );
    assert_eq!(
        out,
        "extra,__construct,__wakeup,getMessage,getCode,getFile,getLine,getTrace,getPrevious,getTraceAsString,__toString\n\
         __wakeup,__construct,getMessage,getCode,getFile,getLine,getTrace,getPrevious,getTraceAsString,__toString\n\
         bool(true)\n"
    );
}
