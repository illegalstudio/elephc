//! Purpose:
//! Regression coverage for array-valued Reflection constants and profile-dependent defaults.
//!
//! Called from:
//! - The `codegen_tests` integration test binary.
//!
//! Key details:
//! - Exercises native/eval inventories, constant APIs, key normalization and value independence.

use super::*;

/// Parameter dumps keep the already normalized PHP profile keys, including nested arrays.
#[test]
fn test_reflection_constant_array_profile_keys_match_parameter_dump() {
    let source = r#"<?php
class ProfileBox { public function __construct(public array $value) {} }
function profileDefault(array $value = [-5 => 'a', 'b'], array $nested = [[-5 => 'c', 'd']], ProfileBox $box = new ProfileBox([-5 => 'e', 'f'])) {}
$parameter = new ReflectionParameter('profileDefault', 0);
echo json_encode($parameter->getDefaultValue()), '|', $parameter->__toString(), '|';
echo (new ReflectionParameter('profileDefault', 1))->__toString(), '|';
echo (new ReflectionParameter('profileDefault', 2))->__toString();
"#;
    for (profile, key) in [
        (elephc::php_version::PhpVersion::Php82, 0),
        (elephc::php_version::PhpVersion::Php83, -4),
    ] {
        let expected = format!(
            "{{\"-5\":\"a\",\"{key}\":\"b\"}}|Parameter #0 [ <optional> array $value = [-5 => 'a', {key} => 'b'] ]|Parameter #1 [ <optional> array $nested = [[-5 => 'c', {key} => 'd']] ]|Parameter #2 [ <optional> ProfileBox $box = new \\ProfileBox([-5 => 'e', {key} => 'f']) ]"
        );
        assert_eq!(compile_and_run_with_php_version(source, profile), expected);
    }
}

/// Reflection property strings quote the decoded default bytes with PHP's printable escapes.
#[test]
fn test_reflection_constant_bytes_keep_property_string_boundaries() {
    let out = compile_and_run(r#"<?php
class Name {}
class PropertyStringDefaults {
    public string $name = Name::class;
    public string $bytes = "\xff\0A\n";
}
echo trim((new ReflectionProperty('PropertyStringDefaults', 'name'))->__toString()), '|';
echo trim((new ReflectionProperty('PropertyStringDefaults', 'bytes'))->__toString());
"#);
    assert_eq!(out, r"Property [ public string $name = 'Name\xEE\x80\x80' ]|Property [ public string $bytes = '\xFF\x00A\n' ]");
}

/// Array property defaults decode PHP bytes and synthesized class names at every depth.
#[test]
fn test_reflection_constant_bytes_keep_property_array_boundaries() {
    let out = compile_and_run(r#"<?php
class Name {}
class PropertyByteDefaults {
    public array $names = [Name::class];
    public static array $staticNames = [Name::class];
    public array $bytes = [-5 => "\xff", "\xc3\xa9" => "old", "é" => ["\xff\0A"], "end"];
    public static array $staticBytes = [-5 => "\xff", "\xc3\xa9" => "old", "é" => ["\xff\0A"], "end"];
}
$object = new PropertyByteDefaults();
echo bin2hex($object->names[0]), '|', bin2hex(PropertyByteDefaults::$staticNames[0]), '|';
echo count($object->bytes), ':', bin2hex($object->bytes[-5]), ':', bin2hex($object->bytes['é'][0]), ':', json_encode(array_keys($object->bytes)), '|';
echo count(PropertyByteDefaults::$staticBytes), ':', bin2hex(PropertyByteDefaults::$staticBytes[-5]), ':', bin2hex(PropertyByteDefaults::$staticBytes['é'][0]), '|';
$reflected = (new ReflectionProperty('PropertyByteDefaults', 'bytes'))->getDefaultValue();
echo count($reflected), ':', bin2hex($reflected[-5]), ':', bin2hex($reflected['é'][0]);
"#);
    assert_eq!(out, "4e616d65ee8080|4e616d65ee8080|3:ff:ff0041:[-5,\"\\u00e9\",-4]|3:ff:ff0041|3:ff:ff0041");
}

/// Scalar default and constant ABI slots carry PHP bytes rather than literal marker text.
#[test]
fn test_reflection_constant_bytes_keep_eval_scalar_boundaries() {
    let out = compile_and_run(r#"<?php
class ScalarName {}
const SCALAR_CLASS_NAME = ScalarName::class;
const BINARY_TEXT = "\xff\0A";
function nameDefault($value = ScalarName::class) { return $value; }
function binaryDefault($value = "\xff\0A") { return $value; }
eval(substr('xecho bin2hex(nameDefault()), "|", bin2hex(SCALAR_CLASS_NAME), "|", bin2hex((new ReflectionParameter("nameDefault", 0))->getDefaultValue()), "|", bin2hex(binaryDefault()), "|", bin2hex(BINARY_TEXT), "|", bin2hex((new ReflectionParameter("binaryDefault", 0))->getDefaultValue());', $argc));
"#);
    assert_eq!(out, "5363616c61724e616d65ee8080|5363616c61724e616d65ee8080|5363616c61724e616d65ee8080|ff0041|ff0041|ff0041");
}

/// Property storage uses the same float keys as mixed-literal numbering and Reflection.
#[test]
fn test_mixed_literal_property_defaults_wrap_float_keys() {
    let out = compile_and_run(r#"<?php
class FloatDefaults {
    public array $positive = [-5 => 'a', 1e20 => 'b', 'c'];
    public static array $negative = [-5 => 'a', -1e20 => 'b', 'c'];
    public array $infinite = [-5 => 'a', 1e999 => 'b', 'c'];
}
$object = new FloatDefaults();
echo json_encode(array_keys($object->positive)), '|';
echo json_encode(array_keys(FloatDefaults::$negative)), '|';
echo json_encode(array_keys($object->infinite)), '|';
echo json_encode(array_keys((new ReflectionProperty('FloatDefaults', 'positive'))->getDefaultValue()));
"#);
    assert_eq!(out, "[-5,7766279631452241920,7766279631452241921]|[-5,-7766279631452241920,-4]|[-5,0,1]|[-5,7766279631452241920,7766279631452241921]");
}

/// Escaped byte strings keep their PHP identity in Reflection, defaults and both inventories.
#[test]
fn test_reflection_constant_arrays_preserve_php_string_bytes() {
    let out = compile_and_run(r#"<?php
const BYTE_VALUES = ["\xc3\xa9" => "old", "é" => "\xff\0A", "\xff" => "raw"];
class ByteConstants { const VALUE = BYTE_VALUES; }
function withBytes($value = ByteConstants::VALUE) { return $value; }
$class = new ReflectionClass(ByteConstants::class);
$value = $class->getConstant('VALUE');
echo count($value), ':', bin2hex($value['é']), ':', bin2hex(array_keys($value)[1]), '|';
$value = (new ReflectionParameter('withBytes', 0))->getDefaultValue();
echo count($value), ':', bin2hex($value['é']), ':', bin2hex(array_keys($value)[1]), '|';
$value = get_defined_constants(true)['user']['BYTE_VALUES'];
echo count($value), ':', bin2hex($value['é']), ':', bin2hex(array_keys($value)[1]), '|';
eval(substr('x$value = BYTE_VALUES; echo count($value), ":", bin2hex($value["é"]), ":", bin2hex(array_keys($value)[1]), "|"; $value = withBytes(); echo count($value), ":", bin2hex($value["é"]), ":", bin2hex(array_keys($value)[1]);', $argc));
"#);
    assert_eq!(out, "2:ff0041:ff|2:ff0041:ff|2:ff0041:ff|2:ff0041:ff|2:ff0041:ff");
}

/// PHP literal decoding must preserve real private-use characters in synthesized class names.
#[test]
fn test_reflection_constant_arrays_preserve_private_use_symbol_names() {
    let out = compile_and_run(r#"<?php
class Name {}
class NamedValues { const VALUE = [Name::class]; }
function withName($value = Name::class) {}
trait Trait { public function run() {} }
class AliasValues { use Trait { run as alias; } }
$class = new ReflectionClass(NamedValues::class);
echo bin2hex($class->getConstant('VALUE')[0]), '|';
echo bin2hex((new ReflectionParameter('withName', 0))->getDefaultValue()), '|';
echo bin2hex((new ReflectionClass(AliasValues::class))->getTraitAliases()['alias']);
echo '|', class_exists(Name::class) ? 'y' : 'n', '|';
echo bin2hex((new ReflectionClass(Name::class))->getName());
"#);
    assert_eq!(out, "4e616d65ee8080|4e616d65ee8080|5472616974ee80803a3a72756e|y|4e616d65ee8080");
}

/// Native and opaque eval inventories retain the parser's negative-key constant arrays.
#[test]
fn test_mixed_literal_constant_inventory_and_eval_keep_negative_keys() {
    let out = compile_and_run(r#"<?php
const X = [-5 => 'a', 'b'];
const NESTED = ['value' => [-5 => 'a', 'b']];
echo json_encode(get_defined_constants(true)['user']['X']), '|';
echo json_encode(get_defined_constants()['NESTED']), '|';
eval(substr("xecho json_encode(X), '|', json_encode(NESTED);", $argc));
"#);
    assert_eq!(out, "{\"-5\":\"a\",\"-4\":\"b\"}|{\"value\":{\"-5\":\"a\",\"-4\":\"b\"}}|{\"-5\":\"a\",\"-4\":\"b\"}|{\"value\":{\"-5\":\"a\",\"-4\":\"b\"}}");
}

/// Normalizing implicit keys keeps source-order side effects under both PHP profiles.
#[test]
fn test_mixed_literal_profile_normalization_preserves_evaluation_order() {
    let source = r#"<?php
function mark(string $value): string { echo $value; return $value; }
$value = [-5 => mark('a'), mark('b'), -5 => mark('c')];
echo '|', json_encode($value);
"#;
    for (profile, expected) in [
        (elephc::php_version::PhpVersion::Php82, "abc|{\"-5\":\"c\",\"0\":\"b\"}"),
        (elephc::php_version::PhpVersion::Php83, "abc|{\"-5\":\"c\",\"-4\":\"b\"}"),
    ] {
        assert_eq!(compile_and_run_with_php_version(source, profile), expected);
    }
}

/// Float-key metadata uses PHP's modulo conversion rather than Rust's saturating cast.
#[test]
fn test_reflection_constant_arrays_wrap_out_of_range_float_keys() {
    let out = compile_and_run(r#"<?php
class FloatKeyConstants {
    const VALUE = [1e20 => 'p', -1e20 => 'n', 9223372036854775808.0 => 'min', 1e100 => 'zero'];
}
function withFloatKeys($value = FloatKeyConstants::VALUE) {}
$class = new ReflectionClass(FloatKeyConstants::class);
$parameter = new ReflectionParameter('withFloatKeys', 0);
echo json_encode(array_keys($class->getConstant('VALUE'))), '|';
echo json_encode(array_keys($parameter->getDefaultValue()));
"#);
    let keys = "[7766279631452241920,-7766279631452241920,-9223372036854775808,0]";
    assert_eq!(out, format!("{keys}|{keys}"));
}

/// Mutating one fetched nested array cannot overwrite stored Reflection constant metadata.
#[test]
fn test_reflection_constant_arrays_keep_fetched_values_independent() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ArrayConstants { const VALUE = [['x' => 1], ['x' => 2]]; }
$class = new ReflectionClass(ArrayConstants::class);
$first = $class->getConstant('VALUE');
$first[0]['x'] = 9;
echo json_encode($first), '|', json_encode($class->getConstant('VALUE')), '|';
$constant = $class->getReflectionConstant('VALUE');
$second = $constant->getValue();
$second[1]['x'] = 8;
echo json_encode($second), '|', json_encode($constant->getValue());
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[{\"x\":9},{\"x\":2}]|[{\"x\":1},{\"x\":2}]|[{\"x\":1},{\"x\":8}]|[{\"x\":1},{\"x\":2}]");
}

/// Verifies that reflection constant metadata folds array constants with PHP's key rules:
/// duplicate keys collapse (first position, last value), spread integer keys renumber from
/// the highest key (negative or not), NAN/INF keys cast to 0, and lists normalize to the
/// packed shape (regression for #1230).
#[test]
fn test_reflection_constant_folder_php_key_rules() {
    let out = compile_and_run_capture(
        r#"<?php
const A = [1, 2];
const B = [3];
const DUP = [1 => "a", 1 => "b"];
const S = [0, ...DUP];
const NSTR = ["2" => "a", "b"];
const NEG = [-5 => "a", "b"];
const NEGSPREAD = [-5 => 1, ...["a" => 2, 0 => 3]];
const EXPL = [0 => "a", "b"];
const NANINF = [INF => 1, NAN => 2];
const SPREADONLY = [1, ...[5, 6]];
function gA($x = A) {}
function gB($x = B) {}
function gS($x = S) {}
function gNSTR($x = NSTR) {}
function gNEG($x = NEG) {}
function gNEGSPREAD($x = NEGSPREAD) {}
function gEXPL($x = EXPL) {}
function gNANINF($x = NANINF) {}
function gSPREADONLY($x = SPREADONLY) {}
// One literal constructor call per name: the AOT checker requires string literals for
// reflection owner constructors (dynamic-name lookup is a separate, later gap).
$p = new ReflectionParameter("gA", 0);
echo "A:", json_encode($p->getDefaultValue()), " list=", array_is_list($p->getDefaultValue()) ? 1 : 0, "\n";
$p = new ReflectionParameter("gB", 0);
echo "B:", json_encode($p->getDefaultValue()), " list=", array_is_list($p->getDefaultValue()) ? 1 : 0, "\n";
$p = new ReflectionParameter("gS", 0);
echo "S:", json_encode($p->getDefaultValue()), " list=", array_is_list($p->getDefaultValue()) ? 1 : 0, "\n";
$p = new ReflectionParameter("gNSTR", 0);
echo "NSTR:", json_encode($p->getDefaultValue()), " list=", array_is_list($p->getDefaultValue()) ? 1 : 0, "\n";
$p = new ReflectionParameter("gNEG", 0);
echo "NEG:", json_encode($p->getDefaultValue()), " list=", array_is_list($p->getDefaultValue()) ? 1 : 0, "\n";
$p = new ReflectionParameter("gNEGSPREAD", 0);
echo "NEGSPREAD:", json_encode($p->getDefaultValue()), " list=", array_is_list($p->getDefaultValue()) ? 1 : 0, "\n";
$p = new ReflectionParameter("gEXPL", 0);
echo "EXPL:", json_encode($p->getDefaultValue()), " list=", array_is_list($p->getDefaultValue()) ? 1 : 0, "\n";
$p = new ReflectionParameter("gNANINF", 0);
echo "NANINF:", json_encode($p->getDefaultValue()), " list=", array_is_list($p->getDefaultValue()) ? 1 : 0, "\n";
$p = new ReflectionParameter("gSPREADONLY", 0);
echo "SPREADONLY:", json_encode($p->getDefaultValue()), " list=", array_is_list($p->getDefaultValue()) ? 1 : 0, "\n";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "A:[1,2] list=1\nB:[3] list=1\nS:[0,\"b\"] list=1\nNSTR:{\"2\":\"a\",\"3\":\"b\"} list=0\nNEG:{\"-5\":\"a\",\"-4\":\"b\"} list=0\nNEGSPREAD:{\"-5\":1,\"a\":2,\"-4\":3} list=0\nEXPL:[\"a\",\"b\"] list=1\nNANINF:[2] list=1\nSPREADONLY:[1,5,6] list=1\n"
    );
}

/// Verifies that parameters defaulted to array-valued constants, global or class,
/// reflect the default the way PHP does (regression for #1230).
#[test]
fn test_reflection_parameter_array_valued_constant_defaults() {
    let out = compile_and_run_capture(
        r#"<?php
const ARR = [1, 2, 3];
const ASSOC = ["a" => 1, "1" => 2];
const SCALAR = 7;
const NESTED = [1, ["deep"], "s", null, ["m" => true]];

class ConstHolder {
    public const LIST = [10, 20];
    public const MAP = ["k" => 5];
    public const SCALAR = 9;
}

function withGlobalArray($x = ARR) { return 0; }
function withGlobalAssoc($x = ASSOC) { return 0; }
function withGlobalScalar($x = SCALAR) { return 0; }
function withNested($x = NESTED) { return 0; }
function withClassList($x = ConstHolder::LIST) { return 0; }
function withClassMap($x = ConstHolder::MAP) { return 0; }
function withClassScalar($x = ConstHolder::SCALAR) { return 0; }

$p = new ReflectionParameter("withGlobalArray", 0);
echo "ga: ", $p->isDefaultValueAvailable() ? "y" : "n", " ", $p->isDefaultValueConstant() ? $p->getDefaultValueConstantName() : "-", " ", json_encode($p->getDefaultValue()), "\n";
$p = new ReflectionParameter("withGlobalAssoc", 0);
echo "go: ", $p->isDefaultValueAvailable() ? "y" : "n", " ", $p->isDefaultValueConstant() ? $p->getDefaultValueConstantName() : "-", " ", json_encode($p->getDefaultValue()), "\n";
$p = new ReflectionParameter("withGlobalScalar", 0);
echo "gs: ", $p->isDefaultValueAvailable() ? "y" : "n", " ", $p->isDefaultValueConstant() ? $p->getDefaultValueConstantName() : "-", " ", json_encode($p->getDefaultValue()), "\n";
$p = new ReflectionParameter("withNested", 0);
echo "ne: ", $p->isDefaultValueAvailable() ? "y" : "n", " ", $p->isDefaultValueConstant() ? $p->getDefaultValueConstantName() : "-", " ", json_encode($p->getDefaultValue()), "\n";
$p = new ReflectionParameter("withClassList", 0);
echo "cl: ", $p->isDefaultValueAvailable() ? "y" : "n", " ", $p->isDefaultValueConstant() ? $p->getDefaultValueConstantName() : "-", " ", json_encode($p->getDefaultValue()), "\n";
$p = new ReflectionParameter("withClassMap", 0);
echo "cm: ", $p->isDefaultValueAvailable() ? "y" : "n", " ", $p->isDefaultValueConstant() ? $p->getDefaultValueConstantName() : "-", " ", json_encode($p->getDefaultValue()), "\n";
$p = new ReflectionParameter("withClassScalar", 0);
echo "cs: ", $p->isDefaultValueAvailable() ? "y" : "n", " ", $p->isDefaultValueConstant() ? $p->getDefaultValueConstantName() : "-", " ", json_encode($p->getDefaultValue()), "\n";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "ga: y ARR [1,2,3]\ngo: y ASSOC {\"a\":1,\"1\":2}\ngs: y SCALAR 7\nne: y NESTED [1,[\"deep\"],\"s\",null,{\"m\":true}]\ncl: y ConstHolder::LIST [10,20]\ncm: y ConstHolder::MAP {\"k\":5}\ncs: y ConstHolder::SCALAR 9\n"
    );
}

/// Verifies that `ReflectionClass::getConstant()`, `getConstants()`, and
/// `ReflectionClassConstant::getValue()` reflect array-valued constants,
/// renumbering mixed-spread integer keys the way PHP does (regression for #1230).
#[test]
fn test_reflection_constant_apis_return_array_values() {
    let out = compile_and_run_capture(
        r#"<?php
const OTHER = ["b" => 7, 1 => 8];
class ReflectArrayConstHolder {
    public const LIST = [1, 2, 3];
    public const MAP = ["a" => 1, "1" => 2];
    public const NESTED = [1, ["deep"], "s", null, ["m" => true]];
    public const MIXED = ["first" => 1, ...OTHER, 9];
    public const SCALAR = 7;
}
$ref = new ReflectionClass(ReflectArrayConstHolder::class);
echo json_encode($ref->getConstant("LIST")), "\n";
echo json_encode($ref->getConstant("MAP")), "\n";
echo json_encode($ref->getConstant("NESTED")), "\n";
echo json_encode($ref->getConstant("MIXED")), "\n";
$all = $ref->getConstants();
echo count($all), ":", json_encode($all["MIXED"]), "\n";
$c = $ref->getReflectionConstant("MIXED");
echo $c ? json_encode($c->getValue()) : "false", "\n";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "[1,2,3]\n{\"a\":1,\"1\":2}\n[1,[\"deep\"],\"s\",null,{\"m\":true}]\n{\"first\":1,\"b\":7,\"0\":8,\"1\":9}\n5:{\"first\":1,\"b\":7,\"0\":8,\"1\":9}\n{\"first\":1,\"b\":7,\"0\":8,\"1\":9}\n"
    );
}

/// Reflection folds a constant array's implicit integer keys with the selected PHP profile's rule:
/// PHP 8.2 restarts at 0 after a negative key, PHP 8.3+ continues at max + 1.
#[test]
fn test_reflection_constant_array_implicit_keys_follow_php_profile() {
    let source = r#"<?php
class NegativeKeyConstHolder {
    public const VALUE = [-5 => "a", "b"];
}
$class = new ReflectionClass(NegativeKeyConstHolder::class);
$constant = $class->getConstant("VALUE");
$constants = $class->getConstants();
echo PHP_VERSION, " ", json_encode($constant), " ", json_encode($constants["VALUE"]), "\n";
"#;
    let php82 = compile_and_run_with_php_version(source, elephc::php_version::PhpVersion::Php82);
    assert_eq!(
        php82,
        "8.2.0 {\"-5\":\"a\",\"0\":\"b\"} {\"-5\":\"a\",\"0\":\"b\"}\n"
    );

    let php83 = compile_and_run_with_php_version(source, elephc::php_version::PhpVersion::Php83);
    assert_eq!(
        php83,
        "8.3.0 {\"-5\":\"a\",\"-4\":\"b\"} {\"-5\":\"a\",\"-4\":\"b\"}\n"
    );
}

/// Pins the known limitation that a parameter defaulted to an enum case reflects as having
/// no default (PHP reports the default; the enum-case default form is a later gap). A case
/// nested in an array constant is a separate, later gap and is not exercised here: the
/// base already fatals registering such a constant with the eval bridge.
#[test]
fn test_reflection_parameter_enum_case_defaults_report_unavailable() {
    let out = compile_and_run_capture(
        r#"<?php
enum PinEnum { case Hearts; }
function withEnumCase($x = PinEnum::Hearts) {}
echo (new ReflectionParameter("withEnumCase", 0))->isDefaultValueAvailable() ? "y" : "n", "\n";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "n\n");
}

/// A default spelled `[-5 => "a", "b"]` (which the parser keeps as a mixed literal, because the
/// bare entry's key depends on the PHP profile) materializes and reflects exactly like the hash
/// PHP 8.5 makes of it: property and static property defaults, method and function parameter
/// defaults called directly, through `call_user_func()` and through a first-class callable, and
/// `ReflectionParameter`/`ReflectionProperty::getDefaultValue()`. Regression for #1340.
#[test]
fn test_mixed_literal_defaults_materialize_and_reflect() {
    let out = compile_and_run(
        r#"<?php
class C {
    public array $p = [-5 => "a", "b"];
    public static array $s = [-5 => "a", "b"];
    public function m(array $x = [-5 => "a", "b"]): array { return $x; }
}
function f(array $x = [-5 => "a", "b"]): array { return $x; }
var_dump((new C())->p, C::$s, (new C())->m(), f());
var_dump((new ReflectionFunction('f'))->getParameters()[0]->getDefaultValue());
var_dump((new ReflectionMethod('C', 'm'))->getParameters()[0]->getDefaultValue());
var_dump((new ReflectionProperty('C', 'p'))->getDefaultValue());
var_dump(call_user_func('f'));
$fn = f(...);
var_dump($fn());
"#,
    );
    let pair = "array(2) {\n  [-5]=>\n  string(1) \"a\"\n  [-4]=>\n  string(1) \"b\"\n}\n";
    assert_eq!(out, pair.repeat(9));
}
