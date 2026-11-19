//! Purpose:
//! Runs php-src 8.6 partial-function-application fixtures and focused semantics checks.
//!
//! Called from:
//! - `cargo test --test codegen_tests partial_application_php86`.
//!
//! Key details:
//! - Sources are transcribed from `Zend/tests/partial_application/*.phpt` and compiled with
//!   `--php-version=8.6`, because partial application is syntax new in that profile.
//! - The selected fixtures avoid engine-only reflection surfaces; every case exercises the
//!   closure synthesis, evaluation order, argument forwarding, or scope rules directly.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Compiles and runs one fixture with the PHP 8.6 profile.
fn run86(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php86)
}

/// php-src `rfc_examples_eval_order.phpt`.
#[test]
fn test_pfa_rfc_eval_order_php86() {
    let out = run86(
        r#"<?php
function getArg() {
    print __FUNCTION__ . PHP_EOL;
    return 'hi';
}
function speak(string $who, string $msg) {
    printf("%s: %s\n", $who, $msg);
}
$arrow = static fn($who) => speak($who, getArg());
print "Arnaud\n";
$arrow('Larry');
$partial = speak(?, getArg());
print "Arnaud\n";
$partial('Larry');
"#,
    );
    assert_eq!(out.trim(), "Arnaud\ngetArg\nLarry: hi\ngetArg\nArnaud\nLarry: hi");
}

/// php-src `superfluous_args_are_forwarded.phpt`.
#[test]
fn test_pfa_superfluous_args_php86() {
    let out = run86(
        r#"<?php
function f($a) { var_dump(func_get_args()); }
$f = f(?, ...);
$f(1, 2, 3);
$h = f(?);
$h(1, 2, 3);
function g($a, ...$args) { var_dump(func_get_args()); }
$g = g(?, ...);
$g(1, 2, 3);
"#,
    );
    let expected = r#"array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
}
array(1) {
  [0]=>
  int(1)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
}"#;
    assert_eq!(out.trim(), expected);
}

/// php-src `variation_variadics_004.phpt`: named bindings and defaults.
#[test]
fn test_pfa_variadics_with_named_bindings_php86() {
    let out = run86(
        r#"<?php
function foo(int $day = 1, int $month = 1, int $year = 2005) {
    printf("%04d-%02d-%02d\n", $year, $month, $day);
}
$foo = foo(year: 2006, ...);
$foo(2);
$foo = foo(month: 12, ...);
$bar = $foo(year: 2016, ...);
$foo(2);
$bar(2);
$foo();
$bar();
"#,
    );
    assert_eq!(
        out.trim(),
        "2006-01-02\n2005-12-02\n2016-12-02\n2005-12-01\n2016-12-01"
    );
}

/// php-src `variation_scope_001.phpt`: the closure keeps its method scope.
#[test]
fn test_pfa_method_scope_php86() {
    let out = run86(
        r#"<?php
class PfaScopeFoo {
    public function method($a) {
        printf("%s::%s\n", get_called_class(), __FUNCTION__);
    }
}
$foo = new PfaScopeFoo;
$bar = $foo->method(new stdClass, ...);
$bar();
"#,
    );
    assert_eq!(out.trim(), "PfaScopeFoo::method");
}

/// php-src `this.phpt`: chained partial applications keep the bound object.
#[test]
fn test_pfa_chained_this_php86() {
    let out = run86(
        r#"<?php
class PfaThisFoo {
    public function method($a, $b) { return $this; }
}
$foo = new PfaThisFoo;
$bar = $foo->method(new stdClass, ...);
$baz = $bar(new stdClass, ...);
var_dump($foo === $baz());
"#,
    );
    assert_eq!(out.trim(), "bool(true)");
}

/// php-src `static_vars_001.phpt`: closures share the underlying function's statics.
#[test]
fn test_pfa_static_vars_php86() {
    let out = run86(
        r#"<?php
function pfaStaticFoo($a) {
    static $var = 0;
    ++$var;
    return $var;
}
var_dump(pfaStaticFoo(new stdClass));
$foo = pfaStaticFoo(new stdClass, ...);
var_dump($foo());
"#,
    );
    assert_eq!(out.trim(), "int(1)\nint(2)");
}

/// php-src `variation_nocall_001.phpt`: creating a partial does not call the function.
#[test]
fn test_pfa_creation_does_not_call_php86() {
    let out = run86(
        r#"<?php
function pfaNoCall($a, $b) {}
pfaNoCall(?, new stdClass);
echo "OK";
"#,
    );
    assert_eq!(out.trim(), "OK");
}

/// php-src `gh25007.phpt`: constant-expression partial application and named arguments.
#[test]
fn test_pfa_constant_expression_php86() {
    let out = run86(
        r#"<?php
function pfaConstF($a, $b = 2, $c = 3) { var_dump(func_get_args()); }
const PFA_C1 = pfaConstF(a: 1, b: ?);
const PFA_C2 = pfaConstF(a: 1, c: ?, b: ?);
(PFA_C1)(4);
(PFA_C2)(5, 6);
"#,
    );
    assert_eq!(
        out.trim(),
        "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(4)\n}\narray(3) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(6)\n  [2]=>\n  int(5)\n}"
    );
}

/// php-src `fuzz_006.phpt`: a bound named argument leaves earlier defaults intact.
#[test]
fn test_pfa_named_defaults_php86() {
    let out = run86(
        r#"<?php
function pfaFuzzFoo(int $day = 1, int $month = 1, int $year = 2005) {
    var_dump($day);
}
$foo = pfaFuzzFoo(month: 12, ...);
$foo(year: 2025);
"#,
    );
    assert_eq!(out.trim(), "int(1)");
}

/// Focused semantics: positional, named, method, static, invokable, by-ref, and variadic.
#[test]
fn test_pfa_core_semantics_php86() {
    let out = run86(
        r#"<?php
function pfaAdd(int $a, int $b = 10): int { return $a + $b; }
$f = pfaAdd(5, ?);
echo $f(), '|';
$g = pfaAdd(?, 2);
echo $g(5), '|';
class PfaCore {
    public function m(int $a, int $b): int { return $a * 10 + $b; }
    public static function s(int $a, int $b): int { return $a - $b; }
}
$core = new PfaCore;
$m = $core->m(1, ?);
echo $m(2), '|';
$sm = PfaCore::s(1, ?);
echo $sm(4), '|';
class PfaInvoke { public function __invoke(int $a, int $b): int { return $a + $b; } }
$inv = new PfaInvoke;
$p = $inv(?, 3);
echo $p(4), '|';
function pfaInc(int &$x): void { $x++; }
$inc = pfaInc(?);
$y = 1;
$inc($y);
echo $y, '|';
function pfaNamed(int $i1, string $s2, float $f3, int $m5 = 0): string { return "$i1,$s2,$f3,$m5"; }
$n = pfaNamed(s2: ?, i1: ?, f3: 3.5);
echo $n('two', 1), '|';
$vn = pfaNamed(?, ?, f3: 3.5, ...);
echo $vn(1, 'x', 7), '|';
function pfaVar(int $a, int ...$rest): string { return $a . ':' . implode(',', $rest); }
$v = pfaVar(1, 2, ...);
echo $v(3, 4);
"#,
    );
    assert_eq!(out, "15|7|12|-3|7|2|1,two,3.5,0|1,x,3.5,7|1:2,3,4");
}
