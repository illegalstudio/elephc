//! Purpose:
//! Regression tests for when php-src evaluates stored constant expressions (class constants,
//! property and parameter defaults, trait-collision defaults, enum case values) and so when,
//! how often, and at which line a deprecated constant read inside one reports.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each expected output was produced by PHP 8.5 on the same program. The `eval()` twins use
//!   a handler without lines, since eval reports at the `eval()` call line.

use super::*;

/// Reads inside a compound class constant, property default, or parameter default report at their own line, a bare one at the enclosing line, as `EG(lineno_override)` decides in php-src.
#[test]
fn test_constant_deprecation_lines_follow_php_line_overrides() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m, string $f, int $line): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), " @$line\n"; return true; });
class Z {
    const ZA = [
        MT_RAND_PHP,
        self::ZB];
    const ZB = [
        DATE_RFC7231,
        self::ZC];
    const ZC =
        E_STRICT;
    const ZD =
        self::ZE;
    const ZE =
        DATE_RFC7231;
}
echo "read\n";
Z::ZA;
Z::ZD;
class Y {
    public $p = [
        DATE_RFC7231,
        self::YB];
    public $q =
        MT_RAND_PHP;
    const YB =
        E_STRICT;
    public function m($a = [
        DATE_RFC7231,
        self::YB], $b =
        MT_RAND_PHP) {}
}
echo "new\n";
new Y;
echo "m\n";
(new Y)->m();
function f($x = [
    DATE_RFC7231,
    Z::ZB]) {}
echo "f\n";
f();
"#,
    );
    assert_eq!(
        out,
        r#"read
  Constant MT_RAND_PHP @4
  Constant DATE_RFC7231 @7
  Constant E_STRICT @8
  Constant DATE_RFC7231 @12
new
  Constant E_STRICT @33
  Constant DATE_RFC7231 @21
  Constant MT_RAND_PHP @33
m
  Constant DATE_RFC7231 @28
  Constant MT_RAND_PHP @30
f
  Constant DATE_RFC7231 @37
"#
    );
}

/// A class initialization evaluates the constants of its table php-src has not evaluated yet (own, trait, interface; a trait constant once per using class) before its property defaults.
#[test]
fn test_class_initialization_evaluates_unevaluated_constants_first() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m, string $f, int $line): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), " @$line\n"; return true; });
interface I { const IK = MT_RAND_PHP; }
trait T { const TK = DATE_RFC7231; public static $ts = DATE_RFC7231; }
class P { const PK = DATE_RFC7231; public $p = MT_RAND_PHP; }
class C extends P implements I {
    use T;
    public static $s = E_STRICT;
    const CK = MT_RAND_PHP;
    public $c = DATE_RFC7231;
}
echo "read\n";
C::CK;
echo "new\n";
new C;
trait T2 { const TK = DATE_RFC7231; }
class A2 { use T2; } class B2 { use T2; }
echo "a2\n"; A2::TK; echo "b2\n"; new B2; echo "b2r\n"; B2::TK;
class D { const DK = DATE_RFC7231; public static $d = 1; }
echo "static\n"; D::$d;
class F { const FK = DATE_RFC7231; }
echo "refl\n"; (new ReflectionClass(F::class))->getDefaultProperties();
class G { const GK = DATE_RFC7231; }
echo "nc\n"; (new ReflectionClass(G::class))->newInstanceWithoutConstructor();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"read
  Constant MT_RAND_PHP @12
new
  Constant DATE_RFC7231 @14
  Constant MT_RAND_PHP @14
  Constant DATE_RFC7231 @14
  Constant MT_RAND_PHP @14
  Constant MT_RAND_PHP @14
  Constant DATE_RFC7231 @14
  Constant E_STRICT @14
  Constant DATE_RFC7231 @14
a2
  Constant DATE_RFC7231 @17
b2
  Constant DATE_RFC7231 @17
b2r
static
  Constant DATE_RFC7231 @19
refl
  Constant DATE_RFC7231 @21
nc
  Constant DATE_RFC7231 @23
end
"#
    );
}

/// A class initialization evaluates the constants of its table php-src has not evaluated yet (own, trait, interface; a trait constant once per using class) before its property defaults. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_class_initialization_evaluates_unevaluated_constants_first() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
interface I { const IK = MT_RAND_PHP; }
trait T { const TK = DATE_RFC7231; public static $ts = DATE_RFC7231; }
class P { const PK = DATE_RFC7231; public $p = MT_RAND_PHP; }
class C extends P implements I {
    use T;
    public static $s = E_STRICT;
    const CK = MT_RAND_PHP;
    public $c = DATE_RFC7231;
}
echo "read\n";
C::CK;
echo "new\n";
new C;
trait T2 { const TK = DATE_RFC7231; }
class A2 { use T2; } class B2 { use T2; }
echo "a2\n"; A2::TK; echo "b2\n"; new B2; echo "b2r\n"; B2::TK;
class D { const DK = DATE_RFC7231; public static $d = 1; }
echo "static\n"; D::$d;
class F { const FK = DATE_RFC7231; }
echo "refl\n"; (new ReflectionClass(F::class))->getDefaultProperties();
class G { const GK = DATE_RFC7231; }
echo "nc\n"; (new ReflectionClass(G::class))->newInstanceWithoutConstructor();
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"read
  Constant MT_RAND_PHP
new
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
  Constant MT_RAND_PHP
  Constant DATE_RFC7231
  Constant E_STRICT
  Constant DATE_RFC7231
a2
  Constant DATE_RFC7231
b2
  Constant DATE_RFC7231
b2r
static
  Constant DATE_RFC7231
refl
  Constant DATE_RFC7231
nc
  Constant DATE_RFC7231
end
"#
    );
}

/// A bare `C::K;` statement is the constant's first evaluation and is not optimized away.
#[test]
fn test_bare_class_constant_statement_reports_its_first_evaluation() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m, string $f, int $line): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), " @$line\n"; return true; });
class C {
    const CK = MT_RAND_PHP;
}
echo "read\n";
C::CK;
echo C::CK, "\n";
"#,
    );
    assert_eq!(
        out,
        r#"read
  Constant MT_RAND_PHP @7
1
"#
    );
}

/// A bare `C::K;` statement is the constant's first evaluation and is not optimized away. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_bare_class_constant_statement_reports_its_first_evaluation() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
class C {
    const CK = MT_RAND_PHP;
}
echo "read\n";
C::CK;
echo C::CK, "\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"read
  Constant MT_RAND_PHP
1
"#
    );
}

/// `getDefaultValue()`, `getValue()`, and `getConstant[s]()` evaluate the reflected initializer the way php-src does.
#[test]
fn test_reflection_getters_evaluate_constant_expressions() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m, string $f, int $line): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), " @$line\n"; return true; });
class P {
    public $i = MT_RAND_PHP;
    public static $s = MT_RAND_PHP;
    const K = DATE_RFC7231;
    const L = E_STRICT;
    public function m($a = DATE_RFC7231) {}
}
$rp = new ReflectionProperty(P::class, "i");
$rs = new ReflectionProperty(P::class, "s");
echo "dv1\n";
$rp->getDefaultValue();
$rs->getDefaultValue();
echo "dv2\n";
$rp->getDefaultValue();
echo "new\n";
new P;
echo "dv3\n";
$rp->getDefaultValue();
$rs->getDefaultValue();
class Q { const K = DATE_RFC7231; const L = MT_RAND_PHP; }
$rc = new ReflectionClassConstant(Q::class, "K");
echo "cv1\n";
$rc->getValue();
echo "cv2\n";
$rc->getValue();
echo "rd\n";
echo Q::K === "" ? "" : "", Q::L, "\n";
class R { const K = DATE_RFC7231; const L = MT_RAND_PHP; }
echo "gc\n";
(new ReflectionClass(R::class))->getConstant("L");
echo "gc2\n";
echo R::K === "" ? "" : "", R::L, "\n";
echo "pd\n";
$pm = new ReflectionParameter([P::class, "m"], 0);
$pm->getDefaultValue();
echo "pd2\n";
$pm->getDefaultValue();
function reflectiveFloatDefault(float $value = MT_RAND_PHP): float { return $value; }
$pn = (new ReflectionFunction('reflectiveFloatDefault'))->getParameters()[0];
echo gettype($pn->getDefaultValue()), "\n";
echo gettype(reflectiveFloatDefault()), "\n";
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"dv1
  Constant MT_RAND_PHP @12
  Constant MT_RAND_PHP @13
dv2
  Constant MT_RAND_PHP @15
new
  Constant DATE_RFC7231 @17
  Constant E_STRICT @17
  Constant MT_RAND_PHP @17
  Constant MT_RAND_PHP @17
dv3
  Constant MT_RAND_PHP @20
cv1
  Constant DATE_RFC7231 @24
cv2
rd
  Constant MT_RAND_PHP @28
1
gc
  Constant DATE_RFC7231 @31
  Constant MT_RAND_PHP @31
gc2
1
pd
  Constant DATE_RFC7231 @36
pd2
  Constant DATE_RFC7231 @38
  Constant MT_RAND_PHP @41
integer
  Constant MT_RAND_PHP @39
double
end
"#
    );
}

/// `getDefaultValue()`, `getValue()`, and `getConstant[s]()` evaluate the reflected initializer the way php-src does. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_reflection_getters_evaluate_constant_expressions() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
class P {
    public $i = MT_RAND_PHP;
    public static $s = MT_RAND_PHP;
    const K = DATE_RFC7231;
    const L = E_STRICT;
    public function m($a = DATE_RFC7231) {}
}
$rp = new ReflectionProperty(P::class, "i");
$rs = new ReflectionProperty(P::class, "s");
echo "dv1\n";
$rp->getDefaultValue();
$rs->getDefaultValue();
echo "dv2\n";
$rp->getDefaultValue();
echo "new\n";
new P;
echo "dv3\n";
$rp->getDefaultValue();
$rs->getDefaultValue();
class Q { const K = DATE_RFC7231; const L = MT_RAND_PHP; }
$rc = new ReflectionClassConstant(Q::class, "K");
echo "cv1\n";
$rc->getValue();
echo "cv2\n";
$rc->getValue();
echo "rd\n";
echo Q::K === "" ? "" : "", Q::L, "\n";
class R { const K = DATE_RFC7231; const L = MT_RAND_PHP; }
echo "gc\n";
(new ReflectionClass(R::class))->getConstant("L");
echo "gc2\n";
echo R::K === "" ? "" : "", R::L, "\n";
echo "pd\n";
$pm = new ReflectionParameter([P::class, "m"], 0);
$pm->getDefaultValue();
echo "pd2\n";
$pm->getDefaultValue();
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"dv1
  Constant MT_RAND_PHP
  Constant MT_RAND_PHP
dv2
  Constant MT_RAND_PHP
new
  Constant DATE_RFC7231
  Constant E_STRICT
  Constant MT_RAND_PHP
  Constant MT_RAND_PHP
dv3
  Constant MT_RAND_PHP
cv1
  Constant DATE_RFC7231
cv2
rd
  Constant MT_RAND_PHP
1
gc
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
gc2
1
pd
  Constant DATE_RFC7231
pd2
  Constant DATE_RFC7231
end
"#
    );
}

/// `get_class_vars()` initializes the class once and evaluates the static defaults it returns on every call.
#[test]
fn test_get_class_vars_initializes_and_replays_static_defaults() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m, string $f, int $line): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), " @$line\n"; return true; });
class C { public $a = DATE_RFC7231; public static $s = MT_RAND_PHP; }
echo "g1\n"; get_class_vars("C"); echo "g2\n"; get_class_vars("C"); echo "n\n"; new C;
class B { public $a = DATE_RFC7231; private static $p = E_STRICT; public static $s = MT_RAND_PHP; }
class D extends B { public static $t = DATE_RFC7231; }
echo "d1\n"; get_class_vars("D"); echo "d2\n"; get_class_vars("D");
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"g1
  Constant DATE_RFC7231 @3
  Constant MT_RAND_PHP @3
  Constant MT_RAND_PHP @3
g2
  Constant MT_RAND_PHP @3
n
d1
  Constant DATE_RFC7231 @6
  Constant E_STRICT @6
  Constant MT_RAND_PHP @6
  Constant DATE_RFC7231 @6
  Constant DATE_RFC7231 @6
  Constant DATE_RFC7231 @6
  Constant MT_RAND_PHP @6
d2
  Constant DATE_RFC7231 @6
  Constant MT_RAND_PHP @6
end
"#
    );
}

/// `get_class_vars()` initializes the class once and evaluates the static defaults it returns on every call. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_get_class_vars_initializes_and_replays_static_defaults() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
class C { public $a = DATE_RFC7231; public static $s = MT_RAND_PHP; }
echo "g1\n"; get_class_vars("C"); echo "g2\n"; get_class_vars("C"); echo "n\n"; new C;
class B { public $a = DATE_RFC7231; private static $p = E_STRICT; public static $s = MT_RAND_PHP; }
class D extends B { public static $t = DATE_RFC7231; }
echo "d1\n"; get_class_vars("D"); echo "d2\n"; get_class_vars("D");
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"g1
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
  Constant MT_RAND_PHP
g2
  Constant MT_RAND_PHP
n
d1
  Constant DATE_RFC7231
  Constant E_STRICT
  Constant MT_RAND_PHP
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
d2
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
end
"#
    );
}

/// A trait that redeclares a used trait's property evaluates both defaults at its statement.
#[test]
fn test_trait_statement_evaluates_compatible_redeclarations() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m, string $f, int $line): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), " @$line\n"; return true; });
echo "first\n";
trait U { public $a = DATE_RFC7231; public static $s = MT_RAND_PHP; }
trait T { use U; public $a = DATE_RFC7231; public static $s = MT_RAND_PHP; }
echo "second\n";
class C { use T; }
echo "third\n";
new C;
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"first
  Constant DATE_RFC7231 @4
  Constant DATE_RFC7231 @4
  Constant MT_RAND_PHP @4
  Constant MT_RAND_PHP @4
second
third
  Constant DATE_RFC7231 @8
  Constant MT_RAND_PHP @8
end
"#
    );
}

/// A trait that redeclares a used trait's property evaluates both defaults at its statement. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_trait_statement_evaluates_compatible_redeclarations() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
echo "first\n";
trait U { public $a = DATE_RFC7231; public static $s = MT_RAND_PHP; }
trait T { use U; public $a = DATE_RFC7231; public static $s = MT_RAND_PHP; }
echo "second\n";
class C { use T; }
echo "third\n";
new C;
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"first
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
  Constant MT_RAND_PHP
second
third
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
end
"#
    );
}

/// A compile-time-folded conditional reports like a bare constant, interface constants follow table order, a refcounted parameter default reports on every call, and reflection does not consume a parameter's once flag.
#[test]
fn test_folded_conditionals_and_refcounted_parameter_defaults() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m, string $f, int $line): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), " @$line\n"; return true; });
class T1 { const ON = true; const K = self::ON ? DATE_RFC7231 : MT_RAND_PHP; const M = MT_RAND_PHP ?: E_STRICT; const N = null ?? DATE_RFC7231; }
echo "new\n"; new T1;
interface I2 { const A = DATE_RFC7231; }
interface I3 extends I2 { const B = MT_RAND_PHP; }
trait T2 { const C = E_STRICT; }
class Q implements I3 { use T2; const D = DATE_RFC7231; }
echo "q\n"; new Q;
class M { public function m($a = DATE_RFC7231, $b = [MT_RAND_PHP]) {} }
$p = new ReflectionParameter([M::class, "m"], 0);
echo "r1\n"; $p->getDefaultValue();
echo "r2\n"; $p->getDefaultValue();
echo "call\n"; (new M)->m();
echo "call2\n"; (new M)->m();
class LS { const K = DATE_RFC7231; public static function k() { return static::K; } }
class LS2 extends LS { const K = MT_RAND_PHP; }
echo "ls\n"; LS2::k(); LS::k(); LS2::k();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"new
  Constant DATE_RFC7231 @3
  Constant MT_RAND_PHP @2
  Constant DATE_RFC7231 @3
q
  Constant DATE_RFC7231 @8
  Constant E_STRICT @8
  Constant MT_RAND_PHP @8
  Constant DATE_RFC7231 @8
r1
  Constant DATE_RFC7231 @11
r2
  Constant DATE_RFC7231 @12
call
  Constant DATE_RFC7231 @9
  Constant MT_RAND_PHP @9
call2
  Constant MT_RAND_PHP @9
ls
  Constant MT_RAND_PHP @15
  Constant DATE_RFC7231 @15
end
"#
    );
}

/// A compile-time-folded conditional reports like a bare constant, interface constants follow table order, a refcounted parameter default reports on every call, and reflection does not consume a parameter's once flag. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_folded_conditionals_and_refcounted_parameter_defaults() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
class T1 { const ON = true; const K = self::ON ? DATE_RFC7231 : MT_RAND_PHP; const M = MT_RAND_PHP ?: E_STRICT; const N = null ?? DATE_RFC7231; }
echo "new\n"; new T1;
interface I2 { const A = DATE_RFC7231; }
interface I3 extends I2 { const B = MT_RAND_PHP; }
trait T2 { const C = E_STRICT; }
class Q implements I3 { use T2; const D = DATE_RFC7231; }
echo "q\n"; new Q;
class M { public function m($a = DATE_RFC7231, $b = [MT_RAND_PHP]) {} }
$p = new ReflectionParameter([M::class, "m"], 0);
echo "r1\n"; $p->getDefaultValue();
echo "r2\n"; $p->getDefaultValue();
echo "call\n"; (new M)->m();
echo "call2\n"; (new M)->m();
class LS { const K = DATE_RFC7231; public static function k() { return static::K; } }
class LS2 extends LS { const K = MT_RAND_PHP; }
echo "ls\n"; LS2::k(); LS::k(); LS2::k();
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"new
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
  Constant DATE_RFC7231
q
  Constant DATE_RFC7231
  Constant E_STRICT
  Constant MT_RAND_PHP
  Constant DATE_RFC7231
r1
  Constant DATE_RFC7231
r2
  Constant DATE_RFC7231
call
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
call2
  Constant MT_RAND_PHP
ls
  Constant MT_RAND_PHP
  Constant DATE_RFC7231
end
"#
    );
}

/// A backed enum initializes on the first fetch of any constant, `cases()` evaluates the case values only, and `getBackingValue()` is the case's first evaluation. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_backed_enum_initialization_and_case_reflection() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
enum E: string { case B = "b"; const K = MT_RAND_PHP; case A = DATE_RFC7231; case C = DATE_RFC7231 . "x"; }
echo "b\n"; E::B;
echo "a\n"; E::A;
echo "from\n"; E::from("b");
enum F: string { case A = DATE_RFC7231; const K = MT_RAND_PHP; case C = "c"; }
echo "fcases\n"; F::cases();
echo "ffrom\n"; F::tryFrom("zz");
enum G: string { case A = DATE_RFC7231; }
$re = new ReflectionEnumBackedCase(G::class, "A");
echo "bv\n"; $re->getBackingValue();
echo "bv2\n"; $re->getBackingValue(); G::A;
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"b
  Constant MT_RAND_PHP
  Constant DATE_RFC7231
  Constant DATE_RFC7231
a
from
fcases
  Constant DATE_RFC7231
ffrom
  Constant MT_RAND_PHP
bv
  Constant DATE_RFC7231
bv2
end
"#
    );
}

/// `ReflectionAttribute::getArguments()` and `newInstance()` evaluate the attribute's arguments
/// on every call: a bare constant argument reports at the calling line for `getArguments()`
/// and at the attribute's line for `newInstance()`.
#[test]
fn test_attribute_arguments_evaluate_on_every_reflection_call() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m, string $f, int $line): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), " @$line\n"; return true; });
#[Attribute]
class At { public function __construct(public string $v) {} }
#[At(DATE_RFC7231)]
class W {}
$a = (new ReflectionClass(W::class))->getAttributes()[0];
echo "a1\n"; $a->getArguments();
echo "a2\n"; $a->getArguments();
echo "a3\n"; $a->newInstance();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"a1
  Constant DATE_RFC7231 @7
a2
  Constant DATE_RFC7231 @8
a3
  Constant DATE_RFC7231 @4
end
"#
    );
}

/// A parameter default is cached unless the branch it yields or its cast builds a refcounted value, two traits declaring the same property are compared at the class statement, and `get_class_vars()` evaluates an interface's or a trait's own constants.
#[test]
fn test_cached_parameter_defaults_trait_compositions_and_introspected_constants() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m, string $f, int $line): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), " @$line\n"; return true; });
function f1($x = true ? DATE_RFC7231 : [MT_RAND_PHP]) {} echo "f1\n"; f1(); f1();
function f2($x = false ? [DATE_RFC7231] : MT_RAND_PHP) {} echo "f2\n"; f2(); f2();
function f3($x = DATE_RFC7231 ?? [MT_RAND_PHP]) {} echo "f3\n"; f3(); f3();
function f4($x = (array)DATE_RFC7231) {} echo "f4\n"; f4(); f4();
function f5($x = (string)(DATE_RFC7231 . "x")) {} echo "f5\n"; f5(); f5();
function f6($x = (string)DATE_RFC7231) {} echo "f6\n"; f6(); f6();
function f7($x = (int)MT_RAND_PHP) {} echo "f7\n"; f7(); f7();
trait TA { public $a = DATE_RFC7231; public $b = E_STRICT; } trait TB { public $a = DATE_RFC7231; public static $s = MT_RAND_PHP; }
echo "tc\n";
class TC { use TA, TB; }
echo "new\n"; new TC; new TC;
interface IP { const P = DATE_RFC7231; } interface IC extends IP { const C = MT_RAND_PHP; }
echo "gi\n"; get_class_vars("IC"); echo "rd\n"; IC::C; IC::P;
trait TR { const C = DATE_RFC7231; public $a = E_STRICT; public static $s = MT_RAND_PHP; }
echo "gt1\n"; get_class_vars("TR"); echo "gt2\n"; get_class_vars("TR");
trait T5 { const C = DATE_RFC7231; public $a = 1; }
echo "g5\n"; get_class_vars("T5"); get_class_vars("T5");
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"f1
  Constant DATE_RFC7231 @2
f2
  Constant MT_RAND_PHP @3
f3
  Constant DATE_RFC7231 @4
f4
  Constant DATE_RFC7231 @5
  Constant DATE_RFC7231 @5
f5
  Constant DATE_RFC7231 @6
  Constant DATE_RFC7231 @6
f6
  Constant DATE_RFC7231 @7
f7
  Constant MT_RAND_PHP @8
tc
  Constant DATE_RFC7231 @11
  Constant DATE_RFC7231 @11
new
  Constant DATE_RFC7231 @12
  Constant E_STRICT @12
  Constant MT_RAND_PHP @12
gi
  Constant MT_RAND_PHP @14
  Constant DATE_RFC7231 @14
rd
gt1
  Constant DATE_RFC7231 @16
  Constant E_STRICT @16
  Constant MT_RAND_PHP @16
  Constant MT_RAND_PHP @16
gt2
  Constant MT_RAND_PHP @16
g5
  Constant DATE_RFC7231 @18
end
"#
    );
}

/// The same rules in `eval()` (eval reports every notice at the `eval()` line, so lines are not compared).
#[test]
fn test_eval_cached_parameter_defaults_trait_compositions_and_introspected_constants() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
function f1($x = true ? DATE_RFC7231 : [MT_RAND_PHP]) {} echo "f1\n"; f1(); f1();
function f2($x = false ? [DATE_RFC7231] : MT_RAND_PHP) {} echo "f2\n"; f2(); f2();
function f3($x = DATE_RFC7231 ?? [MT_RAND_PHP]) {} echo "f3\n"; f3(); f3();
function f4($x = (array)DATE_RFC7231) {} echo "f4\n"; f4(); f4();
function f5($x = (string)(DATE_RFC7231 . "x")) {} echo "f5\n"; f5(); f5();
function f6($x = (string)DATE_RFC7231) {} echo "f6\n"; f6(); f6();
function f7($x = (int)MT_RAND_PHP) {} echo "f7\n"; f7(); f7();
trait TA { public $a = DATE_RFC7231; public $b = E_STRICT; } trait TB { public $a = DATE_RFC7231; public static $s = MT_RAND_PHP; }
echo "tc\n";
class TC { use TA, TB; }
echo "new\n"; new TC; new TC;
interface IP { const P = DATE_RFC7231; } interface IC extends IP { const C = MT_RAND_PHP; }
echo "gi\n"; get_class_vars("IC"); echo "rd\n"; IC::C; IC::P;
trait TR { const C = DATE_RFC7231; public $a = E_STRICT; public static $s = MT_RAND_PHP; }
echo "gt1\n"; get_class_vars("TR"); echo "gt2\n"; get_class_vars("TR");
trait T5 { const C = DATE_RFC7231; public $a = 1; }
echo "g5\n"; get_class_vars("T5"); get_class_vars("T5");
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"f1
  Constant DATE_RFC7231
f2
  Constant MT_RAND_PHP
f3
  Constant DATE_RFC7231
f4
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f5
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f6
  Constant DATE_RFC7231
f7
  Constant MT_RAND_PHP
tc
  Constant DATE_RFC7231
  Constant DATE_RFC7231
new
  Constant DATE_RFC7231
  Constant E_STRICT
  Constant MT_RAND_PHP
gi
  Constant MT_RAND_PHP
  Constant DATE_RFC7231
rd
gt1
  Constant DATE_RFC7231
  Constant E_STRICT
  Constant MT_RAND_PHP
  Constant MT_RAND_PHP
gt2
  Constant MT_RAND_PHP
g5
  Constant DATE_RFC7231
end
"#
    );
}

/// A class php-src binds at its statement (one with interfaces or traits, or whose parent was not bound while the file compiled) copies its parent's and traits' defaults as they are then, so a default the parent or trait already evaluated does not report again for it; a top-level class with neither, whose parent is bound, binds while the file compiles and reports the parent's defaults itself.
#[test]
fn test_classes_bound_at_their_statement_copy_already_evaluated_defaults() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m, string $f, int $line): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), " @$line\n"; return true; });
interface I {}
class Q { public $q = DATE_RFC7231; }
echo "nq\n"; new Q;
class R extends Q implements I { public $r = MT_RAND_PHP; }
echo "nr\n"; new R;
class P { public $a = DATE_RFC7231; }
echo "np\n"; new P;
class C extends P {}
echo "nc\n"; new C;
trait T { public $t = DATE_RFC7231; public static $ts = MT_RAND_PHP; const TK = E_STRICT; }
class Early { use T; }
echo "gcv\n"; get_class_vars("T");
class U { use T; }
echo "nu\n"; new U; U::TK;
echo "ne\n"; new Early; Early::TK;
class GP { public $g = DATE_RFC7231; }
echo "ngp\n"; new GP;
class PP extends GP implements I { public $p = MT_RAND_PHP; }
class CC extends PP {}
echo "ncc\n"; new CC;
trait TA { public $a = DATE_RFC7231; } trait TB { public $a = DATE_RFC7231; }
echo "gta\n"; get_class_vars("TA");
class TC2 { use TA, TB; }
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"nq
  Constant DATE_RFC7231 @4
nr
  Constant MT_RAND_PHP @6
np
  Constant DATE_RFC7231 @8
nc
  Constant DATE_RFC7231 @10
gcv
  Constant E_STRICT @13
  Constant DATE_RFC7231 @13
  Constant MT_RAND_PHP @13
  Constant MT_RAND_PHP @13
nu
  Constant MT_RAND_PHP @15
ne
  Constant E_STRICT @16
  Constant DATE_RFC7231 @16
  Constant MT_RAND_PHP @16
ngp
  Constant DATE_RFC7231 @18
ncc
  Constant MT_RAND_PHP @21
  Constant MT_RAND_PHP @21
gta
  Constant DATE_RFC7231 @23
  Constant DATE_RFC7231 @24
end
"#
    );
}

/// The same binding rules in `eval()`, where a class php-src binds while compiling the `eval()` code copies its parent's defaults as they were before that code ran. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_classes_bound_at_their_statement_copy_already_evaluated_defaults() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
interface I {}
class Q { public $q = DATE_RFC7231; }
echo "nq\n"; new Q;
class R extends Q implements I { public $r = MT_RAND_PHP; }
echo "nr\n"; new R;
class P { public $a = DATE_RFC7231; }
echo "np\n"; new P;
class C extends P {}
echo "nc\n"; new C;
trait T { public $t = DATE_RFC7231; public static $ts = MT_RAND_PHP; const TK = E_STRICT; }
class Early { use T; }
echo "gcv\n"; get_class_vars("T");
class U { use T; }
echo "nu\n"; new U; U::TK;
echo "ne\n"; new Early; Early::TK;
class GP { public $g = DATE_RFC7231; }
echo "ngp\n"; new GP;
class PP extends GP implements I { public $p = MT_RAND_PHP; }
class CC extends PP {}
echo "ncc\n"; new CC;
trait TA { public $a = DATE_RFC7231; } trait TB { public $a = DATE_RFC7231; }
echo "gta\n"; get_class_vars("TA");
class TC2 { use TA, TB; }
echo "end\n";
PHPCODE
);
eval('class E1 { public $e = MT_RAND_PHP; }');
echo "ne1\n"; eval('new E1;');
eval('class E2 extends E1 {}');
echo "ne2\n"; eval('new E2;');
eval('class E3 extends E1 {}');
echo "ne3\n"; eval('new E3;');
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"nq
  Constant DATE_RFC7231
nr
  Constant MT_RAND_PHP
np
  Constant DATE_RFC7231
nc
  Constant DATE_RFC7231
gcv
  Constant E_STRICT
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
  Constant MT_RAND_PHP
nu
  Constant MT_RAND_PHP
ne
  Constant E_STRICT
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
ngp
  Constant DATE_RFC7231
ncc
  Constant MT_RAND_PHP
  Constant MT_RAND_PHP
gta
  Constant DATE_RFC7231
  Constant DATE_RFC7231
end
ne1
  Constant MT_RAND_PHP
ne2
ne3
end
"#
    );
}

/// In `eval()` a class constant, an enum case, and a property default are evaluated when php-src evaluates them: an initializer that throws does so at that moment and again on the next attempt, a backed enum checks its case values when it initializes (not on `cases()`), and the self-reference `Error` names the class as php-src's visited-constant check reaches it.
#[test]
fn test_eval_constant_expressions_evaluate_and_throw_where_php_does() {
    let out = compile_and_run(
        r#"<?php
eval(<<<'PHPCODE'
define("X", 1);
enum E: int { case A = X; case B = self::LOOP; const LOOP = self::LOOP; }
echo "declared\n";
try { var_dump(E::A); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump(E::B); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
enum F: string { case A = 'a' . 'b'; case B = 'a' . 'b'; }
try { var_dump(count(F::cases())); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump(F::A); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump(F::from('ab')); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
define("S", 'str');
enum G: int { case A = S; }
try { var_dump(G::cases()[0]->value); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump(G::A); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
enum H { case A; const C = self::C; }
try { echo H::A->name, "\n"; } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump(H::C); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
class C { const X = self::X; const Y = c::Y; const Z = D::Z; const W = self::LW; const LW = self::LW; const OK = 5; }
class D { const Z = C::Z; }
try { var_dump(C::X); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump(C::Y); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump(C::Z); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump(C::W); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump(C::W); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump(C::OK); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
enum R: int { case A = self::A->value; }
try { var_dump(R::A); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
enum M2: int { case A = 1; case B = 1; case C = 'x'; }
try { var_dump(M2::A); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
class Q { public $p = Loops::P; public static $s = Loops::S; }
class Loops { const P = self::P; const S = self::S; }
echo "Q declared\n";
try { var_dump(new Q); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump(Q::$s); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
$r = new ReflectionClass('C');
echo "reflector built\n";
try { var_dump($r->getConstant('OK')); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"declared
Error: Cannot declare self-referencing constant self::LOOP
Error: Cannot declare self-referencing constant self::LOOP
int(2)
Error: Duplicate value in enum F for cases A and B
Error: Duplicate value in enum F for cases A and B
string(3) "str"
TypeError: Enum case type string does not match enum backing type int
A
Error: Cannot declare self-referencing constant self::C
Error: Cannot declare self-referencing constant self::X
Error: Cannot declare self-referencing constant c::Y
Error: Cannot declare self-referencing constant D::Z
Error: Cannot declare self-referencing constant self::LW
Error: Cannot declare self-referencing constant self::LW
int(5)
Error: Cannot declare self-referencing constant self::A
Error: Duplicate value in enum M2 for cases A and B
Q declared
Error: Cannot declare self-referencing constant self::P
Error: Cannot declare self-referencing constant self::P
reflector built
Error: Cannot declare self-referencing constant self::X
"#
    );
}

/// An `eval()` reflector evaluates nothing when it is built: `getValue()`, `getBackingValue()`, and `getDefaultValue()` evaluate the reflected initializer, raising its notices or its `Error`, when they are called.
#[test]
fn test_eval_reflectors_evaluate_on_getter_calls() {
    let out = compile_and_run(
        r#"<?php
eval(<<<'PHPCODE'
set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
class K { const BAD = self::BAD; const GOOD = DATE_RFC7231; public $p = Loops::P; public $q = [DATE_RFC7231]; public static $s = Loops::S; }
class Loops { const P = self::P; const S = self::S; const L = self::L; }
$c = new ReflectionClassConstant('K', 'BAD');
echo "built c\n";
try { var_dump($c->getValue()); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
$g = new ReflectionClassConstant('K', 'GOOD');
echo "built g\n";
try { var_dump($g->getValue()); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump($g->getValue()); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
$p = new ReflectionProperty('K', 'p');
echo "built p\n";
try { var_dump($p->getDefaultValue()); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
$q = new ReflectionProperty('K', 'q');
try { var_dump($q->getDefaultValue()); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump($q->getDefaultValue()); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
$s = new ReflectionProperty('K', 's');
try { var_dump($s->getDefaultValue()); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
enum BE: string { case A = 'x' . DATE_RFC7231; case B = 'y'; }
$e = new ReflectionEnum('BE');
echo "built e\n";
$cases = $e->getCases();
echo "got cases\n";
try { var_dump($cases[0]->getBackingValue()); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump($cases[0]->getValue() === BE::A); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { echo $cases[1]->getValue()->name, "\n"; } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
function f(BE $x = BE::B, $y = Loops::L) {}
$params = (new ReflectionFunction('f'))->getParameters();
echo "built params\n";
try { var_dump($params[0]->getDefaultValue() === BE::B); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { var_dump($params[1]->getDefaultValue()); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"built c
Error: Cannot declare self-referencing constant self::BAD
built g
  Constant DATE_RFC7231
string(21) "D, d M Y H:i:s \G\M\T"
string(21) "D, d M Y H:i:s \G\M\T"
built p
Error: Cannot declare self-referencing constant self::P
  Constant DATE_RFC7231
array(1) {
  [0]=>
  string(21) "D, d M Y H:i:s \G\M\T"
}
  Constant DATE_RFC7231
array(1) {
  [0]=>
  string(21) "D, d M Y H:i:s \G\M\T"
}
Error: Cannot declare self-referencing constant self::S
built e
got cases
  Constant DATE_RFC7231
string(22) "xD, d M Y H:i:s \G\M\T"
bool(true)
B
built params
bool(true)
Error: Cannot declare self-referencing constant self::L
"#
    );
}

/// A class or trait of an included file binds when the include runs (at the start of the file
/// when php-src binds it while compiling the file, at its statement otherwise), and an
/// anonymous class when its `new` first runs, so each copies its parent's and traits' defaults
/// as they are then.
#[test]
fn test_included_and_anonymous_classes_bind_when_they_run() {
    let out = compile_and_run_files(
        &[
            ("main.php", r#"<?php set_error_handler(function (int $l, string $m, string $f, int $line): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), " @$line\n"; return true; });
class P { public $a = DATE_RFC7231; }
echo "np\n"; new P;
require __DIR__ . '/c.php';
class P3 { public $a = MT_RAND_PHP; }
require __DIR__ . '/c3.php';
interface I4 {}
class P4 { public $a = E_STRICT; }
require __DIR__ . '/c4.php';
trait TA { public $a = DATE_RFC7231; }
trait TB { public $a = DATE_RFC7231; }
echo "gcv\n"; get_class_vars('TA');
require __DIR__ . '/tc.php';
echo "ntc\n"; new TC;
class Q { public $q = MT_RAND_PHP; }
echo "nq\n"; new Q;
echo "anon\n"; new class extends Q {};
class R { public $r = DATE_RFC7231; }
echo "anon2\n";
for ($i = 0; $i < 2; $i++) { new class extends R {}; echo "i$i\n"; }
echo "anon3\n";
for ($i = 0; $i < 2; $i++) { new class { use TA, TB; }; echo "j$i\n"; }
echo "end\n";
"#),
            ("c.php", r#"<?php
class C extends P {}
echo "nc\n"; new C;
"#),
            ("c3.php", r#"<?php
new P3;
class C3 extends P3 {}
echo "nc3\n"; new C3;
"#),
            ("c4.php", r#"<?php
new P4;
class C4 extends P4 implements I4 {}
echo "nc4\n"; new C4;
"#),
            ("tc.php", r#"<?php
class TC { use TA, TB; }
echo "inc\n";
"#),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        r#"np
  Constant DATE_RFC7231 @3
nc
  Constant MT_RAND_PHP @2
nc3
  Constant MT_RAND_PHP @4
  Constant E_STRICT @2
nc4
gcv
  Constant DATE_RFC7231 @12
  Constant DATE_RFC7231 @2
inc
ntc
nq
  Constant MT_RAND_PHP @16
anon
anon2
  Constant DATE_RFC7231 @20
  Constant DATE_RFC7231 @20
i0
i1
anon3
  Constant DATE_RFC7231 @22
j0
j1
end
"#
    );
}

/// A parameter default is cached unless the value it yields is refcounted: for a conditional only the branch it takes counts, so an untaken concatenation does not make every call report.
#[test]
fn test_parameter_defaults_classify_only_the_yielded_branch() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
function f1($x = true ? DATE_RFC7231 : (DATE_RFC7231 . "x")) {}
function f2($x = false ? (DATE_RFC7231 . "x") : DATE_RFC7231) {}
function f3($x = DATE_RFC7231 ?? (DATE_RFC7231 . "x")) {}
function f4($x = DATE_RFC7231 ?: (DATE_RFC7231 . "x")) {}
function f11($x = (string)(true ? DATE_RFC7231 : (DATE_RFC7231 . "x"))) {}
function f14($x = true ? (DATE_RFC7231 . "x") : DATE_RFC7231) {}
foreach (['f1', 'f2', 'f3', 'f4', 'f11', 'f14'] as $f) { echo "$f\n"; $f(); $f(); }
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"f1
  Constant DATE_RFC7231
f2
  Constant DATE_RFC7231
f3
  Constant DATE_RFC7231
f4
  Constant DATE_RFC7231
f11
  Constant DATE_RFC7231
f14
  Constant DATE_RFC7231
  Constant DATE_RFC7231
end
"#
    );
}

/// The same parameter-default caching in `eval()`. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_parameter_defaults_classify_only_the_yielded_branch() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
function f1($x = true ? DATE_RFC7231 : (DATE_RFC7231 . "x")) {}
function f2($x = false ? (DATE_RFC7231 . "x") : DATE_RFC7231) {}
function f3($x = DATE_RFC7231 ?? (DATE_RFC7231 . "x")) {}
function f4($x = DATE_RFC7231 ?: (DATE_RFC7231 . "x")) {}
function f11($x = (string)(true ? DATE_RFC7231 : (DATE_RFC7231 . "x"))) {}
function f14($x = true ? (DATE_RFC7231 . "x") : DATE_RFC7231) {}
foreach (['f1', 'f2', 'f3', 'f4', 'f11', 'f14'] as $f) { echo "$f\n"; $f(); $f(); }
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"f1
  Constant DATE_RFC7231
f2
  Constant DATE_RFC7231
f3
  Constant DATE_RFC7231
f4
  Constant DATE_RFC7231
f11
  Constant DATE_RFC7231
f14
  Constant DATE_RFC7231
  Constant DATE_RFC7231
end
"#
    );
}

/// In `eval()` an undefined constant, a trait constant read through the trait, and a throwing trait initialization throw PHP's `Error`, naming the class as the fetch spells it inside a constant expression and by its declared name in ordinary code; a trait whose initialization threw initializes again on the next `get_class_vars()`.
#[test]
fn test_eval_undefined_and_trait_constants_throw_php_errors() {
    let out = compile_and_run(
        r#"<?php
eval(<<<'PHPCODE'
namespace NS;
class K { const A = NOPE_G; const B = self::NOPE; const C = k::NOPE; }
trait TT { const C = self::C; const D = 1; }
class TU { const Z = self::Z; }
trait TV { const C = TU::Z; }
try { echo NOPE_X; } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { echo \NOPE_Y; } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { echo constant('NOPE_W'); } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { echo K::A; } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { echo K::A; } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { echo K::B; } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { echo K::C; } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { echo k::NOPE; } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { echo tt::D; } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { get_class_vars('NS\TT'); } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { get_class_vars('NS\TT'); } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { get_class_vars('NS\TV'); } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { get_class_vars('NS\TV'); } catch (\Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"Error: Undefined constant "NS\NOPE_X"
Error: Undefined constant "NOPE_Y"
Error: Undefined constant "NOPE_W"
Error: Undefined constant "NS\NOPE_G"
Error: Undefined constant "NS\NOPE_G"
Error: Undefined constant self::NOPE
Error: Undefined constant NS\k::NOPE
Error: Undefined constant NS\K::NOPE
Error: Cannot access trait constant NS\TT::D directly
Error: Cannot access trait constant self::C directly
Error: Cannot access trait constant self::C directly
Error: Cannot declare self-referencing constant self::Z
Error: Cannot declare self-referencing constant self::Z
end
"#
    );
}

/// A reflector `eval()` builds and hands to compiled code evaluates on the getter call there as
/// php-src does: the first evaluation of a constant or enum case, a fresh property or parameter
/// default (a closure's parameter too) on every call, and the constant table on `getConstant[s]()`, with their notices and
/// errors at that moment. (`eval()`; eval reports every notice at the `eval()` line, so lines
/// are not compared.)
#[test]
fn test_eval_reflectors_handed_to_compiled_code_evaluate_on_call() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
$r = eval(<<<'PHPCODE'
class K { const C = DATE_RFC7231; const BAD = self::BAD; public $p = MT_RAND_PHP; public static $s = E_STRICT; }
enum BE: string { case A = 'x' . DATE_RFC7231; case B = 'y'; }
function f(BE $x = BE::B, $y = [DATE_RFC7231]) {}
$cl = function ($z = [MT_RAND_PHP]) {};
return [(new ReflectionFunction($cl))->getParameters()[0], new ReflectionClassConstant('K', 'C'), new ReflectionProperty('K', 'p'), new ReflectionProperty('K', 's'),
    (new ReflectionEnum('BE'))->getCases(), (new ReflectionFunction('f'))->getParameters(), new ReflectionClass('K'),
    new ReflectionClassConstant('K', 'BAD')];
PHPCODE);
echo "built\n";
$cp = $r[0];
echo "closure\n"; echo count($cp->getDefaultValue()), "\n"; echo count($cp->getDefaultValue()), "\n";
$c = $r[1];
echo "c\n"; echo $c->getValue(), "\n"; echo $c->getValue(), "\n";
$p = $r[2];
echo "p\n"; var_dump($p->getDefaultValue()); var_dump($p->getDefaultValue());
$s = $r[3];
echo "s\n"; var_dump($s->getDefaultValue()); var_dump($s->getDefaultValue());
$cases = $r[4];
echo "case\n"; echo $cases[0]->getBackingValue(), "\n"; echo $cases[0]->getValue()->name, "\n"; echo $cases[1]->getValue()->name, "\n";
$params = $r[5];
echo "param\n"; echo $params[0]->getDefaultValue()->name, "\n"; echo count($params[1]->getDefaultValue()), "\n"; echo count($params[1]->getDefaultValue()), "\n";
$k = $r[6];
echo "class\n";
try { $k->getConstants(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { echo $k->getConstant('C'), "\n"; } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
$bad = $r[7];
try { $bad->getValue(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { $bad->getValue(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"built
closure
  Constant MT_RAND_PHP
1
  Constant MT_RAND_PHP
1
c
  Constant DATE_RFC7231
D, d M Y H:i:s \G\M\T
D, d M Y H:i:s \G\M\T
p
  Constant MT_RAND_PHP
int(1)
  Constant MT_RAND_PHP
int(1)
s
  Constant E_STRICT
int(2048)
  Constant E_STRICT
int(2048)
case
  Constant DATE_RFC7231
xD, d M Y H:i:s \G\M\T
A
B
param
B
  Constant DATE_RFC7231
1
  Constant DATE_RFC7231
1
class
Error: Cannot declare self-referencing constant self::BAD
Error: Cannot declare self-referencing constant self::BAD
Error: Cannot declare self-referencing constant self::BAD
Error: Cannot declare self-referencing constant self::BAD
end
"#
    );
}

/// An empty array literal a parameter default yields (also through a conditional) is cached and reports once, while a non-empty array and any array cast, `(array)null` included, report on every call.
#[test]
fn test_empty_array_parameter_defaults_are_cached() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
function f($x = DATE_RFC7231 ? [] : [DATE_RFC7231]) {}
function b($x = (DATE_RFC7231 && false) ? [DATE_RFC7231] : []) {}
function c($x = DATE_RFC7231 ? (array)null : 1) {}
function d($x = [DATE_RFC7231]) {}
function e($x = (array)DATE_RFC7231) {}
foreach (['f', 'b', 'c', 'd', 'e'] as $fn) { echo "$fn\n"; $fn(); $fn(); }
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"f
  Constant DATE_RFC7231
b
  Constant DATE_RFC7231
c
  Constant DATE_RFC7231
  Constant DATE_RFC7231
d
  Constant DATE_RFC7231
  Constant DATE_RFC7231
e
  Constant DATE_RFC7231
  Constant DATE_RFC7231
end
"#
    );
}

/// The same array parameter-default caching in `eval()`. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_empty_array_parameter_defaults_are_cached() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
function f($x = DATE_RFC7231 ? [] : [DATE_RFC7231]) {}
function b($x = (DATE_RFC7231 && false) ? [DATE_RFC7231] : []) {}
function c($x = DATE_RFC7231 ? (array)null : 1) {}
function d($x = [DATE_RFC7231]) {}
function e($x = (array)DATE_RFC7231) {}
foreach (['f', 'b', 'c', 'd', 'e'] as $fn) { echo "$fn\n"; $fn(); $fn(); }
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"f
  Constant DATE_RFC7231
b
  Constant DATE_RFC7231
c
  Constant DATE_RFC7231
  Constant DATE_RFC7231
d
  Constant DATE_RFC7231
  Constant DATE_RFC7231
e
  Constant DATE_RFC7231
  Constant DATE_RFC7231
end
"#
    );
}

/// A parameter default is cached exactly when the value it yields is not refcounted in php-src (concatenations with `""`, casts, array `+`, constants holding arrays or built strings, literal branches of an unfolded conditional).
#[test]
fn test_parameter_default_caching_follows_refcounted_results() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { if ($l === E_DEPRECATED) { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; } return true; });
const N = [1]; const E = []; const S = DATE_RFC7231 . "x"; const T = "lit";
class C { const N = [1]; const E = []; const S = DATE_RFC7231 . "y"; const Q = "a" . "b"; }
enum U { case A; }
function f1($x = DATE_RFC7231 . "") {}
function f2($x = "" . DATE_RFC7231) {}
function f3($x = DATE_RFC7231 . ("" . "")) {}
function f4($x = (array)(DATE_RFC7231 ? [] : 1)) {}
function f5($x = (array)(array)(DATE_RFC7231 ? [] : 1)) {}
function f6($x = [] + [DATE_RFC7231]) {}
function f7($x = DATE_RFC7231 ? ([] + []) : [DATE_RFC7231]) {}
function f8($x = DATE_RFC7231 ? N : 1) {}
function f9($x = DATE_RFC7231 ? C::N : 1) {}
function f10($x = DATE_RFC7231 ? ("a" . "b") : (DATE_RFC7231 . "x")) {}
function f11($x = (string)[DATE_RFC7231]) {}
function f12($x = DATE_RFC7231 ? E : [DATE_RFC7231]) {}
function f13($x = DATE_RFC7231 ? C::E : [DATE_RFC7231]) {}
function f14($x = DATE_RFC7231 ? S : 1) {}
function f15($x = DATE_RFC7231 ? C::S : 1) {}
function f16($x = DATE_RFC7231 ? T : 1) {}
function f17($x = DATE_RFC7231 ? C::Q : 1) {}
function f18($x = (string)E_STRICT . (DATE_RFC7231 ? "" : "")) {}
function f19($x = DATE_RFC7231 ? (string)E_STRICT : 1) {}
function f20($x = DATE_RFC7231 ? (string)MT_RAND_PHP : 1) {}
function f21($x = DATE_RFC7231 ? U::A : 1) {}
function f24($x = DATE_RFC7231 ? (string)1.5 : 1) {}
function f25($x = DATE_RFC7231 ? (string)true : 1) {}
function f27($x = DATE_RFC7231 . 5) {}
function f28($x = DATE_RFC7231 ? 5 . "" : 1) {}
function f29($x = DATE_RFC7231 ? (array)E : 1) {}
function f31($x = DATE_RFC7231 ? (array)N : 1) {}
function f32($x = DATE_RFC7231 ? (DATE_RFC7231 ? [] : []) + [] : 1) {}
function f33($x = DATE_RFC7231 ? (string)(DATE_RFC7231 . "x") : 1) {}
function f34($x = DATE_RFC7231 ? (string)T : 1) {}
foreach (['f1', 'f2', 'f3', 'f4', 'f5', 'f6', 'f7', 'f8', 'f9', 'f10', 'f11', 'f12', 'f13', 'f14', 'f15', 'f16', 'f17', 'f18', 'f19', 'f20', 'f21', 'f24', 'f25', 'f27', 'f28', 'f29', 'f31', 'f32', 'f33', 'f34'] as $fn) { echo "$fn\n"; $fn(); $fn(); }
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"  Constant DATE_RFC7231
f1
  Constant DATE_RFC7231
f2
  Constant DATE_RFC7231
f3
  Constant DATE_RFC7231
f4
  Constant DATE_RFC7231
f5
  Constant DATE_RFC7231
f6
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f7
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f8
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f9
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f10
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f11
  Constant DATE_RFC7231
f12
  Constant DATE_RFC7231
f13
  Constant DATE_RFC7231
f14
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f15
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f16
  Constant DATE_RFC7231
f17
  Constant DATE_RFC7231
f18
  Constant E_STRICT
  Constant DATE_RFC7231
  Constant E_STRICT
  Constant DATE_RFC7231
f19
  Constant DATE_RFC7231
  Constant E_STRICT
  Constant DATE_RFC7231
  Constant E_STRICT
f20
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
f21
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f24
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f25
  Constant DATE_RFC7231
f27
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f28
  Constant DATE_RFC7231
f29
  Constant DATE_RFC7231
f31
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f32
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f33
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f34
  Constant DATE_RFC7231
end
"#
    );
}

/// The same parameter-default caching in `eval()`, with `define()`d constants, where each copy of a trait method, each alias, and each closure keeps its own cache. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_parameter_default_caching_follows_refcounted_results() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { if ($l === E_DEPRECATED) { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; } return true; });
eval(<<<'PHPCODE'
define("N", [1]); define("E", []); define("S", DATE_RFC7231 . "x"); define("T", "lit");
class C { const N = [1]; const E = []; const S = DATE_RFC7231 . "y"; const Q = "a" . "b"; }
enum U { case A; }
function f1($x = DATE_RFC7231 . "") {}
function f2($x = "" . DATE_RFC7231) {}
function f3($x = DATE_RFC7231 . ("" . "")) {}
function f4($x = (array)(DATE_RFC7231 ? [] : 1)) {}
function f5($x = (array)(array)(DATE_RFC7231 ? [] : 1)) {}
function f6($x = [] + [DATE_RFC7231]) {}
function f7($x = DATE_RFC7231 ? ([] + []) : [DATE_RFC7231]) {}
function f8($x = DATE_RFC7231 ? N : 1) {}
function f9($x = DATE_RFC7231 ? C::N : 1) {}
function f10($x = DATE_RFC7231 ? ("a" . "b") : (DATE_RFC7231 . "x")) {}
function f11($x = (string)[DATE_RFC7231]) {}
function f12($x = DATE_RFC7231 ? E : [DATE_RFC7231]) {}
function f13($x = DATE_RFC7231 ? C::E : [DATE_RFC7231]) {}
function f14($x = DATE_RFC7231 ? S : 1) {}
function f15($x = DATE_RFC7231 ? C::S : 1) {}
function f16($x = DATE_RFC7231 ? T : 1) {}
function f17($x = DATE_RFC7231 ? C::Q : 1) {}
function f18($x = (string)E_STRICT . (DATE_RFC7231 ? "" : "")) {}
function f19($x = DATE_RFC7231 ? (string)E_STRICT : 1) {}
function f20($x = DATE_RFC7231 ? (string)MT_RAND_PHP : 1) {}
function f21($x = DATE_RFC7231 ? U::A : 1) {}
function f24($x = DATE_RFC7231 ? (string)1.5 : 1) {}
function f25($x = DATE_RFC7231 ? (string)true : 1) {}
function f27($x = DATE_RFC7231 . 5) {}
function f28($x = DATE_RFC7231 ? 5 . "" : 1) {}
function f29($x = DATE_RFC7231 ? (array)E : 1) {}
function f31($x = DATE_RFC7231 ? (array)N : 1) {}
function f32($x = DATE_RFC7231 ? (DATE_RFC7231 ? [] : []) + [] : 1) {}
function f33($x = DATE_RFC7231 ? (string)(DATE_RFC7231 . "x") : 1) {}
function f34($x = DATE_RFC7231 ? (string)T : 1) {}
foreach (['f1', 'f2', 'f3', 'f4', 'f5', 'f6', 'f7', 'f8', 'f9', 'f10', 'f11', 'f12', 'f13', 'f14', 'f15', 'f16', 'f17', 'f18', 'f19', 'f20', 'f21', 'f24', 'f25', 'f27', 'f28', 'f29', 'f31', 'f32', 'f33', 'f34'] as $fn) { echo "$fn\n"; $fn(); $fn(); }
trait T9 {
    function m($x = DATE_RFC7231) {}
    function c() { $f = function ($x = DATE_RFC7231) {}; $f(); }
}
class A9 { use T9; }
class B9 { use T9; }
class D9 { use T9 { m as k; } }
echo "traits\n";
(new A9)->m(); (new B9)->m(); (new A9)->m();
echo "closures\n";
(new A9)->c(); (new B9)->c(); (new A9)->c();
echo "alias\n";
(new D9)->m(); (new D9)->k(); (new D9)->k();
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"  Constant DATE_RFC7231
f1
  Constant DATE_RFC7231
f2
  Constant DATE_RFC7231
f3
  Constant DATE_RFC7231
f4
  Constant DATE_RFC7231
f5
  Constant DATE_RFC7231
f6
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f7
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f8
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f9
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f10
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f11
  Constant DATE_RFC7231
f12
  Constant DATE_RFC7231
f13
  Constant DATE_RFC7231
f14
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f15
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f16
  Constant DATE_RFC7231
f17
  Constant DATE_RFC7231
f18
  Constant E_STRICT
  Constant DATE_RFC7231
  Constant E_STRICT
  Constant DATE_RFC7231
f19
  Constant DATE_RFC7231
  Constant E_STRICT
  Constant DATE_RFC7231
  Constant E_STRICT
f20
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
f21
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f24
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f25
  Constant DATE_RFC7231
f27
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f28
  Constant DATE_RFC7231
f29
  Constant DATE_RFC7231
f31
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f32
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f33
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant DATE_RFC7231
f34
  Constant DATE_RFC7231
traits
  Constant DATE_RFC7231
  Constant DATE_RFC7231
closures
  Constant DATE_RFC7231
  Constant DATE_RFC7231
alias
  Constant DATE_RFC7231
  Constant DATE_RFC7231
end
"#
    );
}

/// `self::from()` and `static::tryFrom()` inside an enum method initialize the backed enum like a named receiver, so the deprecated constant reports on that call and not on a later `E::from()`.
#[test]
fn test_self_and_static_enum_from_initialize_the_backed_enum() {
    let out = compile_and_run(
        r#"<?php set_error_handler(function (int $l, string $m, string $f, int $line): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), " @$line\n"; return true; });
class P { public $a = DATE_RFC7231; }
echo "np\n"; new P;
trait T { public $t = DATE_RFC7231; }
echo "gcv\n"; get_class_vars("T");
enum E: string {
    case A = "a";
    const C = DATE_RFC7231;
    public static function go(string $v): self { return self::from($v); }
}
enum F: string {
    case A = "a";
    const C = MT_RAND_PHP;
    public static function go(string $v): ?static { return static::tryFrom($v); }
}
echo "go\n"; E::go("a");
echo "from\n"; E::from("a");
echo "fgo\n"; F::go("a");
echo "ffrom\n"; F::from("a");
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"np
  Constant DATE_RFC7231 @3
gcv
  Constant DATE_RFC7231 @5
go
  Constant DATE_RFC7231 @9
from
fgo
  Constant MT_RAND_PHP @14
ffrom
end
"#
    );
}

/// In `eval()` a class or trait declared inside a runtime condition binds at its statement, copying the parent's and trait's defaults as they are then, and `self::from()` / `static::tryFrom()` initialize the backed enum. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_conditional_declarations_and_self_enum_from_bind_and_initialize_when_they_run() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
$on = true;
class P { public $a = DATE_RFC7231; }
echo "np\n"; new P;
if ($on) { class C extends P {} }
echo "nc\n"; new C;
trait T { public $t = DATE_RFC7231; }
echo "gcv\n"; get_class_vars("T");
if ($on) { class U { use T; } }
echo "nu\n"; new U;
enum E: string {
    case A = "a";
    const C = DATE_RFC7231;
    public static function go(string $v): self { return self::from($v); }
}
enum F: string {
    case A = "a";
    const C = MT_RAND_PHP;
    public static function go(string $v): ?static { return static::tryFrom($v); }
}
echo "go\n"; E::go("a");
echo "from\n"; E::from("a");
echo "fgo\n"; F::go("a");
echo "ffrom\n"; F::from("a");
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"np
  Constant DATE_RFC7231
nc
gcv
  Constant DATE_RFC7231
nu
go
  Constant DATE_RFC7231
from
fgo
  Constant MT_RAND_PHP
ffrom
end
"#
    );
}

/// A parameter default's deprecation is raised at the read, before a constructor in the default runs; a class default whose condition only folds at run time reports only the branch it takes.
#[test]
fn test_defaults_report_at_the_read_and_only_taken_branches() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
class A { public function __construct($v) { echo "ctor\n"; } }
function g($x = new A(DATE_RFC7231)) {}
echo "g\n"; g(); g();
class C { public $x = [MT_RAND_PHP][0] ? DATE_RFC7231 : E_STRICT; public $y = [0][0] ?: DATE_RFC7231; public $z = [null][0] ?? MT_RAND_PHP; }
echo "c\n"; new C;
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"g
  Constant DATE_RFC7231
ctor
  Constant DATE_RFC7231
ctor
c
  Constant MT_RAND_PHP
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
end
"#
    );
}

/// The same in `eval()`, where an enum that uses a trait also evaluates its members in declaration order and a default whose later read throws still reports, and reports again on the next call since PHP caches only a successful evaluation. (`eval()`; eval reports every notice at the `eval()` line, so lines are not compared.)
#[test]
fn test_eval_defaults_report_at_the_read_and_only_taken_branches() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
class A { public function __construct($v) { echo "ctor\n"; } }
function g($x = new A(DATE_RFC7231)) {}
echo "g\n"; g(); g();
class C { public $x = [MT_RAND_PHP][0] ? DATE_RFC7231 : E_STRICT; public $y = [0][0] ?: DATE_RFC7231; public $z = [null][0] ?? MT_RAND_PHP; }
echo "c\n"; new C;
trait T {}
enum E: string { use T; const K = E_STRICT; case A = DATE_RFC7231 . "x"; }
echo "e\n"; E::A;
function f($x = DATE_RFC7231 . MISSING_CONST) {}
echo "f\n";
try { f(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { f(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"g
  Constant DATE_RFC7231
ctor
  Constant DATE_RFC7231
ctor
c
  Constant MT_RAND_PHP
  Constant DATE_RFC7231
  Constant DATE_RFC7231
  Constant MT_RAND_PHP
e
  Constant E_STRICT
  Constant DATE_RFC7231
f
  Constant DATE_RFC7231
Undefined constant "MISSING_CONST"
  Constant DATE_RFC7231
Undefined constant "MISSING_CONST"
end
"#
    );
}

/// A property default's `??` whose left side reads a missing key probes it quietly; a parameter default whose condition reads a constant through an array folds, so its cached branch reports once.
#[test]
fn test_null_probe_and_foldable_array_conditions_report_like_php() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { echo "  ", $l === E_DEPRECATED ? substr($m, 0, strpos($m, " is deprecated")) : $m, "\n"; return true; });
class C { public $z = [][0] ?? DATE_RFC7231; }
echo "c\n"; new C;
function f($a = [MT_RAND_PHP][0] ? DATE_RFC7231 : [1]) {}
echo "f\n"; f(); f();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"c
  Constant DATE_RFC7231
f
  Constant MT_RAND_PHP
  Constant DATE_RFC7231
end
"#
    );
}

/// The `??` probe in `eval()` (which does not accept an array dimension read in a parameter default).
#[test]
fn test_eval_null_probe_reports_like_php() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { echo "  ", $l === E_DEPRECATED ? substr($m, 0, strpos($m, " is deprecated")) : $m, "\n"; return true; });
eval(<<<'PHPCODE'
class C { public $z = [][0] ?? DATE_RFC7231; }
echo "c\n"; new C;
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"c
  Constant DATE_RFC7231
end
"#
    );
}

/// In `eval()`, a parameter default whose condition constructs an object runs the constructor once per evaluation and caches its scalar result; a self-referencing constant in a default throws on every call. (`eval()`)
#[test]
fn test_eval_parameter_default_caching_runs_user_code_once_and_survives_cycles() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
eval(<<<'PHPCODE'
class A { public function __construct($v) { echo "ctor\n"; } }
function f($x = (new A(DATE_RFC7231)) ? 1 : 2) { echo $x, "\n"; }
f(); f();
class C { const K = self::K; }
function g($x = C::K) {}
try { g(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { g(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
echo "end\n";
PHPCODE
);
"#,
    );
    assert_eq!(
        out,
        r#"  Constant DATE_RFC7231
ctor
1
  Constant DATE_RFC7231
ctor
1
Cannot declare self-referencing constant self::K
Cannot declare self-referencing constant self::K
end
"#
    );
}

/// A parameter default that constructs an object is evaluated, and reports, on every call that omits it, even when its result is a scalar.
#[test]
fn test_parameter_default_constructing_an_object_is_never_cached() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
class A { public function __construct($v) { echo "ctor\n"; } }
function f($x = (new A(DATE_RFC7231)) ? 1 : 2) { echo $x, "\n"; }
f(); f();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"  Constant DATE_RFC7231
ctor
1
  Constant DATE_RFC7231
ctor
1
end
"#
    );
}

/// A deprecated constant read in an attribute argument reports where php-src reports it: in a
/// function's attribute, `getArguments()` reports the caller's file and line and
/// `newInstance()` the attribute's, for top-level and nested reads alike; in a class's
/// attribute only a top-level global constant does, while nested global constants report their
/// own line, in the declaring file.
#[test]
fn test_attribute_argument_reads_report_php_file_and_line() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
set_error_handler(function (int $level, string $message, string $file, int $line): bool {
    echo "  ", basename($file), ":", $line, " ", substr($message, 0, strpos($message, " is deprecated")), "\n";
    return true;
});
require __DIR__ . '/lib/attrs.php';
#[At(
    DATE_RFC7231,
    [
        DATE_RFC7231,
    ]
)]
function deep() {}
foreach ([new ReflectionFunction('deep'), new ReflectionClass('Deep')] as $r) {
    $a = $r->getAttributes()[0];
    echo get_class($r), " args\n";
    $a->getArguments();
    echo "new\n";
    $a->newInstance();
}
reflect_here();
"#,
            ),
            (
                "lib/attrs.php",
                r#"<?php
#[Attribute(Attribute::TARGET_ALL)] class At { public function __construct(public mixed $v = 0, public mixed $w = 0) {} }
#[At(
    DATE_RFC7231,
    [
        DATE_RFC7231,
    ]
)]
class Deep {}
function reflect_here(): void {
    $a = (new ReflectionClass('Deep'))->getAttributes()[0];
    echo "inc args\n";
    $a->getArguments();
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        r#"ReflectionFunction args
  main.php:17 Constant DATE_RFC7231
  main.php:17 Constant DATE_RFC7231
new
  main.php:7 Constant DATE_RFC7231
  main.php:7 Constant DATE_RFC7231
ReflectionClass args
  main.php:17 Constant DATE_RFC7231
  attrs.php:6 Constant DATE_RFC7231
new
  attrs.php:3 Constant DATE_RFC7231
  attrs.php:6 Constant DATE_RFC7231
inc args
  attrs.php:13 Constant DATE_RFC7231
  attrs.php:6 Constant DATE_RFC7231
"#
    );
}

/// A backed enum's initialization and a reflection constant-table fetch report an int case written `MT_RAND_PHP` and a deprecated constant in declaration order, each once.
#[test]
fn test_enum_case_and_constant_notices_follow_declaration_order() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $level, string $message, string $file, int $line): bool {
    echo "  ", $line, " ", substr($message, 0, strpos($message, " is deprecated")), "\n";
    return true;
});
enum F: int {
    case A = MT_RAND_PHP;
    const K = DATE_RFC7231;
}
enum E: int {
    const K = DATE_RFC7231;
    case A = MT_RAND_PHP;
}
echo "from\n";
F::from(1);
echo "reflect\n";
(new ReflectionEnum('E'))->getConstant('K');
echo "again\n";
F::from(1);
E::A;
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"from
  7 Constant MT_RAND_PHP
  15 Constant DATE_RFC7231
reflect
  17 Constant DATE_RFC7231
  12 Constant MT_RAND_PHP
again
end
"#
    );
}

/// A class initialization or constant evaluation whose error handler throws is not marked done: the next attempt evaluates, and reports, again.
#[test]
fn test_initialization_retries_after_a_throwing_handler() {
    let out = compile_and_run(
        r#"<?php
$throws = true;
set_error_handler(function (int $l, string $m) use (&$throws): bool {
    echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n";
    if ($throws) { $throws = false; throw new Exception("handler"); }
    return true;
});
class C { public $p = DATE_RFC7231; const K = E_STRICT; }
try { new C; } catch (Exception $e) { echo "caught ", $e->getMessage(), "\n"; }
$c = new C;
echo strlen($c->p), "\n";
$throws = true;
try { echo C::K, "\n"; } catch (Exception $e) { echo "caught ", $e->getMessage(), "\n"; }
echo C::K, "\n";
echo C::K, "\n";
"#,
    );
    assert_eq!(
        out,
        r#"  Constant E_STRICT
caught handler
  Constant E_STRICT
  Constant DATE_RFC7231
21
2048
2048
2048
"#
    );
}

/// An eval-built `ReflectionParameter` handed to compiled code evaluates a default that constructs an object once per `getDefaultValue()` call, and not when the reflector is built.
#[test]
fn test_eval_parameter_reflector_runs_a_new_default_once_per_getter_call() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m): bool { echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n"; return true; });
class A { public function __construct($v) { echo "ctor\n"; } }
$p = eval('function f($x = new A(DATE_RFC7231)) {} return new ReflectionParameter("f", 0);');
echo "built\n";
$p->getDefaultValue();
$p->getDefaultValue();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"built
  Constant DATE_RFC7231
ctor
  Constant DATE_RFC7231
ctor
end
"#
    );
}

/// A class that only inherits a deprecated interface constant reports it on initialization; a throwing handler's exception is caught and the next `new` reports again.
#[test]
fn test_interface_constant_initialization_retries_after_a_throwing_handler() {
    let out = compile_and_run(
        r#"<?php
$throws = true;
set_error_handler(function (int $l, string $m) use (&$throws): bool {
    echo "  ", substr($m, 0, strpos($m, " is deprecated")), "\n";
    if ($throws) { $throws = false; throw new Exception("handler"); }
    return true;
});
interface IfaceDep { const K = E_STRICT; }
class OnlyIface implements IfaceDep {}
class Child extends OnlyIface {}
try { new OnlyIface; } catch (Exception $e) { echo "caught ", $e->getMessage(), "\n"; }
try { new OnlyIface; echo "made\n"; } catch (Exception $e) { echo "caught2 ", $e->getMessage(), "\n"; }
new Child;
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"  Constant E_STRICT
caught handler
  Constant E_STRICT
made
end
"#
    );
}

/// A backed enum reports its `MT_RAND_PHP` cases before the constants it inherits from an interface.
#[test]
fn test_enum_cases_report_before_interface_constants() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m, string $f, int $line): bool {
    echo "  ", $line, " ", substr($m, 0, strpos($m, " is deprecated")), "\n";
    return true;
});
interface I { const K = DATE_RFC7231; }
enum E: int implements I { case A = MT_RAND_PHP; }
enum F: int implements I { const L = 1; case A = MT_RAND_PHP; }
echo "e\n"; E::from(1);
echo "f\n"; F::from(1);
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"e
  7 Constant MT_RAND_PHP
  9 Constant DATE_RFC7231
f
  8 Constant MT_RAND_PHP
end
"#
    );
}

/// A backed enum reports its `MT_RAND_PHP` case before a constant imported from a trait declared earlier, then the constants it inherits from an interface.
#[test]
fn test_enum_cases_report_before_trait_constants() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $l, string $m, string $f, int $line): bool {
    echo "  ", $line, " ", substr($m, 0, strpos($m, " is deprecated")), "\n";
    return true;
});
trait T { const K = E_STRICT; }
interface I { const J = DATE_RFC7231; }
enum E: int implements I { use T; case A = MT_RAND_PHP; }
echo "from\n";
E::from(1);
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        r#"from
  8 Constant MT_RAND_PHP
  10 Constant E_STRICT
  10 Constant DATE_RFC7231
end
"#
    );
}
