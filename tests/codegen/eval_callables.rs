//! Purpose:
//! End-to-end regressions for eval callable bridge dispatch.
//! Covers callable values crossing eval into generated/AOT functions and methods.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_callable` through Rust's test harness.
//!
//! Key details:
//! - Fixtures verify by-reference writeback through string, callable-array, and
//!   first-class callable forms instead of only direct method/function syntax.

use crate::support::compile_and_run;

/// Verifies eval string and first-class AOT function callables preserve by-ref writeback.
#[test]
fn test_eval_aot_function_callables_preserve_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
function eval_aot_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

echo eval('$string = "eval_aot_ref_add";
$a = "2";
echo $string($a, 3) . ":" . gettype($a) . ":" . $a . "|";
$first = eval_aot_ref_add(...);
$b = "4";
echo $first($b, 5) . ":" . gettype($b) . ":" . $b . "|";
$c = "6";
return call_user_func_array($first, [&$c, 7]) . ":" . gettype($c) . ":" . $c;');
"#,
    );

    assert_eq!(out, "5:integer:5|9:integer:9|13:integer:13");
}

/// Verifies eval callable-array AOT methods preserve by-ref writeback.
#[test]
fn test_eval_aot_callable_arrays_preserve_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
class EvalAotCallableArrayRefBox {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }
}

echo eval('$box = new EvalAotCallableArrayRefBox();
$instance = [$box, "bump"];
$a = "2";
echo $instance($a, 3) . ":" . gettype($a) . ":" . $a . "|";
$b = "4";
echo call_user_func_array($instance, [&$b, 5]) . ":" . gettype($b) . ":" . $b . "|";
$static = ["EvalAotCallableArrayRefBox", "add"];
$c = "7";
echo $static($c, 6) . ":" . gettype($c) . ":" . $c . "|";
$d = "8";
return call_user_func_array($static, [&$d, 9]) . ":" . gettype($d) . ":" . $d;');
"#,
    );

    assert_eq!(
        out,
        "15:integer:15|19:integer:19|13:integer:13|17:integer:17"
    );
}

/// Verifies eval first-class AOT method callables preserve by-ref writeback.
#[test]
fn test_eval_aot_first_class_method_callables_preserve_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
class EvalAotFirstClassRefBox {
    public int $base = 20;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }
}

echo eval('$box = new EvalAotFirstClassRefBox();
$method = $box->bump(...);
$a = "2";
echo $method($a, 3) . ":" . gettype($a) . ":" . $a . "|";
$static = EvalAotFirstClassRefBox::add(...);
$b = "4";
echo $static($b, 5) . ":" . gettype($b) . ":" . $b . "|";
$name = "EvalAotFirstClassRefBox::add";
$c = "6";
echo $name($c, 7) . ":" . gettype($c) . ":" . $c . "|";
$d = "8";
return call_user_func_array($static, [&$d, 9]) . ":" . gettype($d) . ":" . $d;');
"#,
    );

    assert_eq!(
        out,
        "25:integer:25|9:integer:9|13:integer:13|17:integer:17"
    );
}

/// Verifies eval-declared callable forms preserve by-ref writeback.
#[test]
fn test_eval_declared_callable_forms_preserve_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
echo eval('function eval_declared_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

class EvalDeclaredRefCallableBox {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }
}

$string = "eval_declared_ref_add";
$a = "2";
echo $string($a, 3) . ":" . gettype($a) . ":" . $a . "|";

$first = eval_declared_ref_add(...);
$b = "4";
echo $first($b, 5) . ":" . gettype($b) . ":" . $b . "|";
$c = "6";
echo call_user_func_array($first, [&$c, 7]) . ":" . gettype($c) . ":" . $c . "|";

$box = new EvalDeclaredRefCallableBox();
$instance = [$box, "bump"];
$d = "8";
echo $instance($d, 4) . ":" . gettype($d) . ":" . $d . "|";
$e = "1";
echo call_user_func_array($instance, [&$e, 5]) . ":" . gettype($e) . ":" . $e . "|";

$static = ["EvalDeclaredRefCallableBox", "add"];
$f = "7";
echo $static($f, 6) . ":" . gettype($f) . ":" . $f . "|";

$closureFunction = Closure::fromCallable("eval_declared_ref_add");
$g = "3";
echo $closureFunction($g, 4) . ":" . gettype($g) . ":" . $g . "|";

$closureInstance = Closure::fromCallable([$box, "bump"]);
$h = "2";
echo $closureInstance($h, 6) . ":" . gettype($h) . ":" . $h . "|";

$closureStatic = Closure::fromCallable(["EvalDeclaredRefCallableBox", "add"]);
$i = "5";
echo $closureStatic($i, 8) . ":" . gettype($i) . ":" . $i . "|";

$closureNamedStatic = Closure::fromCallable("EvalDeclaredRefCallableBox::add");
$j = "6";
return call_user_func_array($closureNamedStatic, [&$j, 9]) . ":" . gettype($j) . ":" . $j;');
"#,
    );

    assert_eq!(
        out,
        concat!(
            "5:integer:5|9:integer:9|13:integer:13|22:integer:22|16:integer:16|",
            "13:integer:13|7:integer:7|18:integer:18|13:integer:13|15:integer:15"
        )
    );
}

/// Verifies eval first-class callables are PHP-visible `Closure` objects and remain invokable.
#[test]
fn test_eval_first_class_callables_are_php_closure_objects() {
    let out = compile_and_run(
        r#"<?php
function eval_aot_first_class_object_func(string $value): string {
    return "F" . $value;
}

class EvalAotFirstClassObjectBox {
    public function m(string $value): string {
        return "M" . $value;
    }

    public static function s(string $value): string {
        return "S" . $value;
    }

    public function __invoke(string $value): string {
        return "I" . $value;
    }
}

echo eval('$box = new EvalAotFirstClassObjectBox();
$f = eval_aot_first_class_object_func(...);
$m = $box->m(...);
$s = EvalAotFirstClassObjectBox::s(...);
$class = "EvalAotFirstClassObjectBox";
$ds = $class::s(...);
$i = $box(...);
foreach ([$f, $m, $s, $ds, $i] as $cb) {
    echo is_object($cb) ? "O" : "o";
    echo get_class($cb);
    echo $cb instanceof Closure ? "I" : "i";
    echo is_callable($cb) ? "C" : "c";
    echo "|";
}
return $f("1") . ":" . $m("2") . ":" . $s("3") . ":" . $ds("4") . ":" . $i("5");');
"#,
    );

    assert_eq!(
        out,
        "OClosureIC|OClosureIC|OClosureIC|OClosureIC|OClosureIC|F1:M2:S3:S4:I5"
    );
}

/// Verifies namespaced eval first-class function callables use PHP builtin fallback rules.
#[test]
fn test_eval_first_class_function_callables_follow_namespace_fallback() {
    let out = compile_and_run(
        r#"<?php
namespace EvalFirstClassFallback;

eval('namespace EvalFirstClassFallback;
function strrev($value) {
    return "local:" . $value;
}

$builtin = strlen(...);
$local = strrev(...);
echo $builtin("abcd") . "|";
echo $local("ab") . "|";
echo call_user_func($builtin, "xyz") . "|";
echo call_user_func_array($local, ["cd"]);');
"#,
    );

    assert_eq!(out, "4|local:ab|3|local:cd");
}

/// Verifies eval first-class function callables resolve namespace function imports.
#[test]
fn test_eval_first_class_function_callables_follow_namespace_imports() {
    let out = compile_and_run(
        r#"<?php
eval('namespace EvalFirstClassImport\Lib;
function target($value) {
    return "target:" . $value;
}

namespace EvalFirstClassImport\App;
use function strlen as Len;
use function EvalFirstClassImport\Lib\target as AliasTarget;

$len = Len(...);
$target = AliasTarget(...);
echo $len("abcd") . "|";
echo $target("x") . "|";
echo call_user_func($len, "yz") . "|";
echo call_user_func_array($target, ["q"]);');
"#,
    );

    assert_eq!(out, "4|target:x|2|target:q");
}

/// Verifies callable-array class receivers accept PHP's leading namespace separator.
#[test]
fn test_eval_callable_array_class_receivers_allow_leading_namespace_separator() {
    let out = compile_and_run(
        r#"<?php
class EvalLeadingSlashAotCallableArray {
    public static function stat(string $value): string {
        return "A" . $value;
    }
}

echo eval('class EvalLeadingSlashCallableArray {
    public static function stat($value) {
        return "E" . $value;
    }
}

$eval = ["\\EvalLeadingSlashCallableArray", "stat"];
$aot = ["\\EvalLeadingSlashAotCallableArray", "stat"];
$name = "seed";

echo call_user_func($eval, "a") . "|";
echo $eval("b") . "|";
echo call_user_func_array($eval, ["c"]) . "|";
echo Closure::fromCallable($eval)("d") . "|";
echo is_callable($eval, false, $name) ? $name : "bad";
echo "|";
echo call_user_func($aot, "x") . "|";
echo $aot("y") . "|";
echo call_user_func_array($aot, ["z"]) . "|";
echo Closure::fromCallable($aot)("q");');
"#,
    );

    assert_eq!(
        out,
        "Ea|Eb|Ec|Ed|\\EvalLeadingSlashCallableArray::stat|Ax|Ay|Az|Aq"
    );
}

/// Verifies string callables accept PHP's leading namespace separator.
#[test]
fn test_eval_string_callables_allow_leading_namespace_separator() {
    let out = compile_and_run(
        r#"<?php
class EvalLeadingSlashAotStringCallable {
    public static function stat(string $value): string {
        return "A" . $value;
    }
}

echo eval('class EvalLeadingSlashStringCallable {
    public static function stat($value) {
        return "E" . $value;
    }
}

$builtin = "\\strlen";
$eval = "\\EvalLeadingSlashStringCallable::stat";
$aot = "\\EvalLeadingSlashAotStringCallable::stat";
$name = "seed";

echo $builtin("abcd") . "|";
echo call_user_func($eval, "a") . "|";
echo $eval("b") . "|";
echo call_user_func_array($eval, ["c"]) . "|";
echo Closure::fromCallable($eval)("d") . "|";
echo is_callable($eval, false, $name) ? $name : "bad";
echo "|";
echo call_user_func($aot, "x") . "|";
echo $aot("y") . "|";
echo call_user_func_array($aot, ["z"]) . "|";
echo Closure::fromCallable($aot)("q");');
"#,
    );

    assert_eq!(
        out,
        "4|Ea|Eb|Ec|Ed|\\EvalLeadingSlashStringCallable::stat|Ax|Ay|Az|Aq"
    );
}

/// Verifies eval first-class callables reject invalid method targets at creation time.
#[test]
fn test_eval_first_class_callable_validation_rejects_invalid_method_targets() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFirstClassInvalidTargets {
    private function hidden() {
        return "bad";
    }

    public function inst() {
        return "bad";
    }

    private static function secret() {
        return "bad";
    }

    public static function ok() {
        return "OK";
    }
}

$box = new EvalFirstClassInvalidTargets();
try {
    $box->missing(...);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    $box->hidden(...);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalFirstClassInvalidTargets::missing(...);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalFirstClassInvalidTargets::inst(...);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalFirstClassInvalidTargets::secret(...);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
$ok = EvalFirstClassInvalidTargets::ok(...);
echo $ok();');
"#,
    );

    assert_eq!(
        out,
        "Error:Call to undefined method EvalFirstClassInvalidTargets::missing()|\
Error:Call to private method EvalFirstClassInvalidTargets::hidden() from global scope|\
Error:Call to undefined method EvalFirstClassInvalidTargets::missing()|\
Error:Non-static method EvalFirstClassInvalidTargets::inst() cannot be called statically|\
Error:Call to private method EvalFirstClassInvalidTargets::secret() from global scope|OK"
    );
}

/// Verifies eval first-class callables preserve PHP magic-method fallback.
#[test]
fn test_eval_first_class_callable_validation_preserves_magic_fallback() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFirstClassMagicTargets {
    private function hidden() {
        return "bad";
    }

    private static function secret() {
        return "bad";
    }

    public function __call($name, $args) {
        return "I:" . $name . ":" . $args[0];
    }

    public static function __callStatic($name, $args) {
        return "S:" . $name . ":" . $args[0];
    }
}

$box = new EvalFirstClassMagicTargets();
$hidden = $box->hidden(...);
echo $hidden("a") . "|";
$missing = $box->missing(...);
echo $missing("b") . "|";
$secret = EvalFirstClassMagicTargets::secret(...);
echo $secret("c") . "|";
$staticMissing = EvalFirstClassMagicTargets::missingStatic(...);
echo $staticMissing("d");');
"#,
    );

    assert_eq!(out, "I:hidden:a|I:missing:b|S:secret:c|S:missingStatic:d");
}

/// Verifies `self::method(...)` inside an instance eval method captures `$this`.
#[test]
fn test_eval_first_class_callable_validation_static_syntax_instance_method_captures_this() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFirstClassStaticSyntaxThis {
    public int $base = 7;

    public function make() {
        $fn = self::add(...);
        return $fn(5);
    }

    public function add($value) {
        return $this->base + $value;
    }

    public static function makeStatic() {
        try {
            self::add(...);
            return "bad";
        } catch (Error $e) {
            return get_class($e) . ":" . $e->getMessage();
        }
    }
}

$box = new EvalFirstClassStaticSyntaxThis();
echo $box->make() . "|";
echo EvalFirstClassStaticSyntaxThis::makeStatic();');
"#,
    );

    assert_eq!(
        out,
        "12|Error:Non-static method EvalFirstClassStaticSyntaxThis::add() cannot be called statically"
    );
}

/// Verifies eval `is_callable()` supports syntax-only probes and callable-name writeback.
#[test]
fn test_eval_is_callable_supports_syntax_only_and_callable_name_writeback() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalCallableNameBox {
    public function method() {}
    private function hidden() {}
    public static function stat() {}
    public function __invoke() {}
}

$box = new EvalCallableNameBox();
$closure = function () {};

$name = "seed";
echo is_callable("\\strlen", false, $name) ? "F:" : "f:";
echo $name . "|";

$name = "seed";
echo is_callable("missing_eval_callable_name", false, $name) ? "bad:" : "M:";
echo $name . "|";

$name = "seed";
echo is_callable("missing_eval_callable_name", true, $name) ? "S:" : "s:";
echo $name . "|";

$name = "seed";
echo is_callable([$box, "method"], false, $name) ? "O:" : "o:";
echo $name . "|";

$name = "seed";
echo is_callable([$box, "hidden"], false, $name) ? "bad:" : "P:";
echo $name . "|";

$name = "seed";
echo is_callable(["NoSuchEvalCallable", "missing"], true, $name) ? "A:" : "a:";
echo $name . "|";

$name = "seed";
echo is_callable(["NoSuchEvalCallable", "missing"], false, $name) ? "bad:" : "N:";
echo $name . "|";

$name = "seed";
echo is_callable($box, false, $name) ? "I:" : "i:";
echo $name . "|";

$name = "seed";
echo is_callable($closure, false, $name) ? "C:" : "c:";
echo $name . "|";

$name = "seed";
echo is_callable(value: [$box, "method"], callable_name: $name) ? "NO:" : "no:";
echo $name . "|";

$probe = "is_callable";
$name = "seed";
echo $probe(["NoSuchDynamicCallable", "missing"], true, $name) ? "D:" : "d:";
echo $name;');
"#,
    );

    assert_eq!(
        out,
        "F:\\strlen|\
M:missing_eval_callable_name|\
S:missing_eval_callable_name|\
O:EvalCallableNameBox::method|\
P:EvalCallableNameBox::hidden|\
A:NoSuchEvalCallable::missing|\
N:NoSuchEvalCallable::missing|\
I:EvalCallableNameBox::__invoke|\
C:Closure::__invoke|\
NO:EvalCallableNameBox::method|\
D:NoSuchDynamicCallable::missing"
    );
}

/// Verifies eval callable arrays resolve `self`, `static`, and `parent` in method scope.
#[test]
fn test_eval_special_class_callable_arrays_follow_method_scope() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalSpecialCallableArrayBase {
    public static function parentStatic(): string {
        return "P:" . get_called_class();
    }

    public function parentInstance(): string {
        return "PI:" . get_class($this);
    }
}

class EvalSpecialCallableArrayChild extends EvalSpecialCallableArrayBase {
    public static function selfStatic(): string {
        return "S:" . get_called_class();
    }

    public function selfInstance(): string {
        return "I:" . get_class($this);
    }

    public static function mapStatic(int $value): string {
        return "MS" . $value . ":" . get_called_class();
    }

    public function mapInstance(int $value): string {
        return "MI" . $value . ":" . get_class($this);
    }

    public function keepValue(int $value): bool {
        return $value > 1;
    }

    public function reduceValue(string $carry, int $value): string {
        return $carry . $value . ":" . get_class($this) . ";";
    }

    public function walkValue(string &$value, int $key) {
        $value = $value . $key . ":" . get_class($this);
    }

    public function compareDesc(int $left, int $right): int {
        return $right - $left;
    }

    public function replaceMatch(array $matches): string {
        return "R" . $matches[0] . ":" . get_class($this);
    }

    public function run(): string {
        $out = "";
        $name = "seed";
        $out .= is_callable(["self", "selfStatic"], false, $name) ? "Y:" . $name . ":" : "N:";
        $out .= call_user_func(["self", "selfStatic"]) . "|";

        $name = "seed";
        $out .= is_callable(["static", "selfStatic"], false, $name) ? "Y:" . $name . ":" : "N:";
        $out .= call_user_func(["static", "selfStatic"]) . "|";

        $name = "seed";
        $out .= is_callable(["parent", "parentStatic"], false, $name) ? "Y:" . $name . ":" : "N:";
        $out .= call_user_func(["parent", "parentStatic"]) . "|";

        $name = "seed";
        $out .= is_callable(["self", "selfInstance"], false, $name) ? "Y:" . $name . ":" : "N:";
        $out .= call_user_func(["self", "selfInstance"]) . "|";

        $name = "seed";
        $out .= is_callable(["parent", "parentInstance"], false, $name) ? "Y:" . $name . ":" : "N:";
        $out .= call_user_func(["parent", "parentInstance"]) . "|";

        $fromInstance = Closure::fromCallable(["self", "selfInstance"]);
        $out .= $fromInstance() . "|";

        $fromStatic = Closure::fromCallable(["parent", "parentStatic"]);
        $out .= $fromStatic();

        $out .= "|" . implode(",", array_map(["static", "mapStatic"], [3]));
        $out .= "|" . implode(",", array_map(["self", "mapInstance"], [4]));
        $out .= "|" . implode(",", array_filter([1, 2], ["self", "keepValue"]));
        $out .= "|" . array_reduce([1, 2], ["self", "reduceValue"], "");

        $walk = ["x"];
        array_walk($walk, ["self", "walkValue"]);
        $out .= "|" . $walk[0];

        $sort = [1, 3, 2];
        usort($sort, ["self", "compareDesc"]);
        $out .= "|" . implode(",", $sort);

        $out .= "|" . preg_replace_callback("/a/", ["self", "replaceMatch"], "a");

        try {
            $direct = ["self", "selfStatic"];
            $out .= "|" . $direct();
        } catch (Error $e) {
            $out .= "|" . get_class($e) . ":" . $e->getMessage();
        }

        return $out;
    }
}

return (new EvalSpecialCallableArrayChild())->run();');
"#,
    );

    assert_eq!(
        out,
        "Y:self::selfStatic:S:EvalSpecialCallableArrayChild|\
Y:static::selfStatic:S:EvalSpecialCallableArrayChild|\
Y:parent::parentStatic:P:EvalSpecialCallableArrayChild|\
Y:self::selfInstance:I:EvalSpecialCallableArrayChild|\
Y:parent::parentInstance:PI:EvalSpecialCallableArrayChild|\
I:EvalSpecialCallableArrayChild|\
P:EvalSpecialCallableArrayChild|\
MS3:EvalSpecialCallableArrayChild|\
MI4:EvalSpecialCallableArrayChild|\
2|\
1:EvalSpecialCallableArrayChild;2:EvalSpecialCallableArrayChild;|\
x0:EvalSpecialCallableArrayChild|\
3,2,1|\
Ra:EvalSpecialCallableArrayChild|\
Error:Class \"self\" not found"
    );
}

/// Verifies `Closure::fromCallable()` normalizes eval string and array callables to Closure objects.
#[test]
fn test_eval_closure_from_callable_normalizes_string_and_array_callables() {
    let out = compile_and_run(
        r#"<?php
function eval_from_callable_fn(int $value): string {
    return "F" . $value;
}

class EvalFromCallableBox {
    public function m(int $value): string {
        return "M" . $value;
    }

    public static function s(int $value): string {
        return "S" . $value;
    }
}

echo eval('$box = new EvalFromCallableBox();
$callbacks = [
    Closure::fromCallable("eval_from_callable_fn"),
    Closure::fromCallable([$box, "m"]),
    Closure::fromCallable(["EvalFromCallableBox", "s"]),
    Closure::fromCallable("EvalFromCallableBox::s"),
];
foreach ($callbacks as $index => $cb) {
    echo is_object($cb) ? "O" : "o";
    echo get_class($cb);
    echo $cb instanceof Closure ? "I" : "i";
    echo is_callable($cb) ? "C" : "c";
    echo $cb($index + 1);
    echo "|";
}');
"#,
    );

    assert_eq!(
        out,
        "OClosureICF1|OClosureICM2|OClosureICS3|OClosureICS4|"
    );
}

/// Verifies eval string callback APIs reject missing functions with PHP TypeErrors.
#[test]
fn test_eval_callable_validation_missing_string_callables_raise_type_errors() {
    let out = compile_and_run(
        r#"<?php
echo eval('try {
    call_user_func("MiSsInG_Eval_Callback");
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    call_user_func_array("OtherMissingEvalCallback", []);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    Closure::fromCallable("ThirdMissingEvalCallback");
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );

    assert_eq!(
        out,
        "TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, function \"MiSsInG_Eval_Callback\" not found or invalid function name|\
TypeError:call_user_func_array(): Argument #1 ($callback) must be a valid callback, function \"OtherMissingEvalCallback\" not found or invalid function name|\
TypeError:Failed to create closure from callable: function \"ThirdMissingEvalCallback\" not found or invalid function name"
    );
}

/// Verifies `Closure::fromCallable()` rejects invalid object and method targets.
#[test]
fn test_eval_callable_validation_closure_from_callable_rejects_invalid_targets() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFromCallablePlain {}
class EvalFromCallableMissing {}
class EvalFromCallablePrivate {
    private function hidden() {
        return "bad";
    }
}
class EvalFromCallableInstance {
    public function inst() {
        return "bad";
    }
}

try {
    Closure::fromCallable(new EvalFromCallablePlain());
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    Closure::fromCallable([new EvalFromCallableMissing(), "MiSsInG"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    Closure::fromCallable([new EvalFromCallablePrivate(), "hidden"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    Closure::fromCallable(["EvalFromCallableInstance", "inst"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );

    assert_eq!(
        out,
        "TypeError:Failed to create closure from callable: no array or string given|\
TypeError:Failed to create closure from callable: class EvalFromCallableMissing does not have a method \"MiSsInG\"|\
TypeError:Failed to create closure from callable: cannot access private method EvalFromCallablePrivate::hidden()|\
TypeError:Failed to create closure from callable: non-static method EvalFromCallableInstance::inst() cannot be called statically"
    );
}

/// Verifies `Closure::fromCallable()` preserves by-ref writeback for AOT call targets.
#[test]
fn test_eval_closure_from_callable_preserves_aot_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
function eval_from_callable_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

class EvalFromCallableRefBox {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }
}

echo eval('$box = new EvalFromCallableRefBox();
$function = Closure::fromCallable("eval_from_callable_ref_add");
$a = "2";
echo $function($a, 3) . ":" . gettype($a) . ":" . $a . "|";
$instance = Closure::fromCallable([$box, "bump"]);
$b = "4";
echo $instance($b, 5) . ":" . gettype($b) . ":" . $b . "|";
$static = Closure::fromCallable(["EvalFromCallableRefBox", "add"]);
$c = "7";
echo $static($c, 6) . ":" . gettype($c) . ":" . $c . "|";
$named = Closure::fromCallable("EvalFromCallableRefBox::add");
$d = "8";
return call_user_func_array($named, [&$d, 9]) . ":" . gettype($d) . ":" . $d;');
"#,
    );

    assert_eq!(
        out,
        "5:integer:5|19:integer:19|13:integer:13|17:integer:17"
    );
}

/// Verifies `Closure::fromCallable()` values can cross eval into AOT callable parameters.
#[test]
fn test_eval_closure_from_callable_values_pass_to_aot_callable_params() {
    let out = compile_and_run(
        r##"<?php
function eval_closure_bridge_suffix(string $value): string {
    return $value . "!";
}

class EvalClosureBridgeTarget {
    public static function suffix(string $value): string {
        return $value . "?";
    }

    public function instanceSuffix(string $value): string {
        return $value . "~";
    }

    public function __invoke(string $value): string {
        return $value . "#";
    }
}

class EvalClosureBridgeBox {
    public $value = "";

    public function __construct(callable $callback) {
        $this->value = $callback("C");
    }

    public function apply(callable $callback) {
        return $callback("M");
    }

    public static function applyStatic(callable $callback) {
        return $callback("S");
    }
}

echo eval('$target = new EvalClosureBridgeTarget();
$cases = [
    Closure::fromCallable("eval_closure_bridge_suffix"),
    Closure::fromCallable([EvalClosureBridgeTarget::class, "suffix"]),
    Closure::fromCallable("EvalClosureBridgeTarget::suffix"),
    Closure::fromCallable([$target, "instanceSuffix"]),
    Closure::fromCallable($target),
];
$out = [];
foreach ($cases as $callback) {
    $box = new EvalClosureBridgeBox($callback);
    $out[] = $box->value . ":" . $box->apply($callback) . ":" .
        EvalClosureBridgeBox::applyStatic($callback);
}
return implode("|", $out);');
"##,
    );

    assert_eq!(
        out,
        "C!:M!:S!|C?:M?:S?|C?:M?:S?|C~:M~:S~|C#:M#:S#"
    );
}

/// Verifies eval dynamic callable descriptors preserve AOT caller by-ref variables.
#[test]
fn test_eval_dynamic_callable_params_write_back_aot_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
function eval_dynamic_callable_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

class EvalDynamicCallableRefBridgeBox {
    public string $value = "";

    public function __construct(callable $callback) {
        $value = 2;
        $this->value = $callback($value, 3) . ":" . gettype($value) . ":" . $value;
    }

    public function apply(callable $callback): string {
        $value = 4;
        return $callback($value, 5) . ":" . gettype($value) . ":" . $value;
    }

    public static function applyStatic(callable $callback): string {
        $value = 6;
        return $callback($value, 7) . ":" . gettype($value) . ":" . $value;
    }
}

echo eval('$callback = Closure::fromCallable("eval_dynamic_callable_ref_add");
$box = new EvalDynamicCallableRefBridgeBox($callback);
return $box->value . "|" . $box->apply($callback) . "|" .
    EvalDynamicCallableRefBridgeBox::applyStatic($callback);');
"#,
    );

    assert_eq!(out, "5:integer:5|9:integer:9|13:integer:13");
}

/// Verifies `Closure::call()` rebinds `fromCallable()` method closures to a same-class receiver.
#[test]
fn test_eval_closure_from_callable_call_rebinds_same_class_method_and_invokable_targets() {
    let out = compile_and_run(
        r#"<?php
function eval_from_callable_call_fn(): string {
    return "bad";
}

class EvalFromCallableCallBox {
    public int $base = 0;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int $value): int {
        return $value + 1;
    }
}

echo eval('$original = new EvalFromCallableCallBox();
$original->base = 10;
$bound = new EvalFromCallableCallBox();
$bound->base = 20;

$method = Closure::fromCallable([$original, "bump"]);
$a = "2";
echo $method->call($bound, $a, 3) . ":" . gettype($a) . ":" . $a . "|";

$invoke = Closure::fromCallable($original);
$b = "4";
echo $invoke->call($bound, $b, 5) . ":" . gettype($b) . ":" . $b . "|";

$function = Closure::fromCallable("eval_from_callable_call_fn");
echo is_null($function->call($bound)) ? "F" : "f"; echo "|";

$static = Closure::fromCallable(["EvalFromCallableCallBox", "add"]);
echo is_null($static->call($bound, 1)) ? "S" : "s";');
"#,
    );

    assert_eq!(out, "25:integer:25|29:integer:29|F|S");
}

/// Verifies `Closure::bindTo()` persists rebinding for method and invokable callable targets.
#[test]
fn test_eval_closure_bind_from_callable_persists_method_and_invokable_targets() {
    let out = compile_and_run(
        r#"<?php
class EvalFromCallableBindBox {
    public int $base = 0;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int $value): int {
        return $value + 1;
    }
}

echo eval('$original = new EvalFromCallableBindBox();
$original->base = 10;
$bound = new EvalFromCallableBindBox();
$bound->base = 20;

$rawMethod = Closure::fromCallable([$original, "bump"]);
$method = $rawMethod->bindTo($bound);
$a = "2";
echo $method($a, 3) . ":" . gettype($a) . ":" . $a . "|";

$rawInvoke = Closure::fromCallable($original);
$invoke = $rawInvoke->bindTo($bound);
$b = "4";
echo call_user_func_array($invoke, [&$b, 5]) . ":" . gettype($b) . ":" . $b . "|";

$static = Closure::fromCallable(["EvalFromCallableBindBox", "add"]);
echo is_null($static->bindTo($bound)) ? "S" : "s";');
"#,
    );

    assert_eq!(out, "25:integer:25|29:integer:29|S");
}
