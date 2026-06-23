//! Purpose:
//! Interpreter tests for isset, empty, function/class probes, dynamic constants, and class declarations.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover symbol-table and metadata probes backed by the eval context.

use super::super::*;
use super::support::*;
use std::ffi::c_void;

/// Verifies `isset` distinguishes missing, null, and other falsey values.
#[test]
fn execute_program_isset_distinguishes_missing_null_and_falsey_values() {
    let program = parse_fragment(
        br#"if (isset($missing)) { echo "1"; } else { echo "0"; }
if (isset($nullish)) { echo "1"; } else { echo "0"; }
if (isset($zero)) { echo "1"; } else { echo "0"; }
if (isset($empty)) { echo "1"; } else { echo "0"; }
if (isset($zero, $empty)) { echo "1"; } else { echo "0"; }
if (isset($zero, $nullish)) { echo "1"; } else { echo "0"; }"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let nullish = values.null().expect("create fake null");
    let zero = values.int(0).expect("create fake int");
    let empty = values.string("").expect("create fake string");
    scope.set("nullish", nullish, ScopeCellOwnership::Owned);
    scope.set("zero", zero, ScopeCellOwnership::Owned);
    scope.set("empty", empty, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "001110");
    assert_eq!(values.get(result), FakeValue::Null);
}
/// Verifies `empty` treats missing, null, and falsey values as empty.
#[test]
fn execute_program_empty_uses_php_truthiness_without_missing_warnings() {
    let program = parse_fragment(
        br#"if (empty($missing)) { echo "1"; } else { echo "0"; }
if (empty($nullish)) { echo "1"; } else { echo "0"; }
if (empty($zero)) { echo "1"; } else { echo "0"; }
if (empty($empty_string)) { echo "1"; } else { echo "0"; }
if (empty($zero_string)) { echo "1"; } else { echo "0"; }
if (empty($value)) { echo "1"; } else { echo "0"; }"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let nullish = values.null().expect("create fake null");
    let zero = values.int(0).expect("create fake int");
    let empty_string = values.string("").expect("create fake empty string");
    let zero_string = values.string("0").expect("create fake zero string");
    let value = values.string("x").expect("create fake non-empty string");
    scope.set("nullish", nullish, ScopeCellOwnership::Owned);
    scope.set("zero", zero, ScopeCellOwnership::Owned);
    scope.set("empty_string", empty_string, ScopeCellOwnership::Owned);
    scope.set("zero_string", zero_string, ScopeCellOwnership::Owned);
    scope.set("value", value, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "111110");
    assert_eq!(values.get(result), FakeValue::Null);
}
/// Verifies `isset` and `empty` use PHP offset semantics for array reads.
#[test]
fn execute_program_isset_and_empty_support_array_offsets() {
    let program = parse_fragment(
        br#"$map = [
    "present" => "x",
    "nullish" => null,
    "zero" => 0,
    "empty" => "",
    "child" => ["leaf" => "ok", "null" => null],
];
echo isset($map["present"]) ? "1" : "0";
echo isset($map["nullish"]) ? "1" : "0";
echo isset($map["missing"]) ? "1" : "0";
echo isset($map["zero"]) ? "1" : "0";
echo isset($map["child"]["leaf"]) ? "1" : "0";
echo isset($map["child"]["null"]) ? "1" : "0";
echo isset($map["missing"]["leaf"]) ? "1" : "0";
echo ":";
echo empty($map["present"]) ? "1" : "0";
echo empty($map["nullish"]) ? "1" : "0";
echo empty($map["missing"]) ? "1" : "0";
echo empty($map["zero"]) ? "1" : "0";
echo empty($map["empty"]) ? "1" : "0";
echo empty($map["child"]["leaf"]) ? "1" : "0";
echo empty($map["child"]["null"]) ? "1" : "0";
echo empty($map["missing"]["leaf"]) ? "1" : "0";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1001100:01111011");
    assert_eq!(values.get(result), FakeValue::Null);
}
/// Verifies eval builtin probes see dynamic functions and supported PHP-visible builtins.
#[test]
fn execute_program_function_probes_use_eval_context() {
    let program = parse_fragment(
        br#"function dyn_probe() { return 1; }
echo function_exists("DYN_PROBE") . "x";
echo is_callable("dyn_probe") . "x";
echo function_exists("strlen") . "x";
echo function_exists("native_probe") . "x";
echo function_exists("eval") . "x";
echo function_exists("missing_probe") . "x";"#,
    )
    .expect("parse eval fragment");
    let native = NativeFunction::new(1usize as *mut c_void, fake_native_return_descriptor, 0);
    let mut context = ElephcEvalContext::new();
    assert!(context
        .define_native_function("native_probe", native)
        .is_ok());
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.output, "1x1x1x1xxx");
}
/// Verifies eval `interface_exists()` probes generated interface metadata by callable.
#[test]
fn execute_program_interface_exists_uses_runtime_probe() {
    let program = parse_fragment(
        br#"echo interface_exists("KnownInterface") ? "Y" : "N";
echo interface_exists("knowninterface") ? "Y" : "N";
echo interface_exists("KnownClass") ? "Y" : "N";
echo call_user_func("interface_exists", "KnownInterface") ? "Y" : "N";
echo call_user_func_array("interface_exists", ["interface" => "KnownInterface"]) ? "Y" : "N";
echo interface_exists(interface: "MissingInterface", autoload: false) ? "Y" : "N";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "YYNYYN");
}
/// Verifies eval-declared interfaces are visible to interface symbol probes.
#[test]
fn execute_program_interface_exists_uses_dynamic_interface_table() {
    let program = parse_fragment(
        br#"interface DynEvalIface {}
echo interface_exists("DynEvalIface") ? "Y" : "N";
echo interface_exists("dynevaliface") ? "Y" : "N";
echo class_exists("DynEvalIface") ? "C" : "c";
echo call_user_func("interface_exists", "DynEvalIface") ? "Y" : "N";
echo call_user_func_array("interface_exists", ["interface" => "\DynEvalIface"]) ? "Y" : "N";
$interfaces = get_declared_interfaces();
echo count($interfaces); echo ":"; echo $interfaces[0];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "YYcYY1:DynEvalIface");
}
/// Verifies eval `trait_exists()` and `enum_exists()` probe generated metadata.
#[test]
fn execute_program_class_like_exists_uses_runtime_probe() {
    let program = parse_fragment(
        br#"echo trait_exists("KnownTrait") ? "T" : "t";
echo trait_exists("knowntrait") ? "T" : "t";
echo trait_exists("KnownEnum") ? "T" : "t";
echo enum_exists("KnownEnum") ? "E" : "e";
echo enum_exists("\knownenum") ? "E" : "e";
echo enum_exists("KnownTrait") ? "E" : "e";
echo call_user_func("trait_exists", "KnownTrait") ? "T" : "t";
echo call_user_func_array("enum_exists", ["enum" => "KnownEnum"]) ? "E" : "e";
echo trait_exists(trait: "MissingTrait", autoload: false) ? "T" : "t";
echo enum_exists(enum: "MissingEnum", autoload: false) ? "E" : "e";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "TTtEEeTEte");
}
/// Verifies eval `is_a()` and `is_subclass_of()` dispatch through runtime class metadata.
#[test]
fn execute_program_is_a_relation_uses_runtime_probe() {
    let program = parse_fragment(
            br#"$object = new KnownClass();
echo is_a($object, "KnownClass") ? "Y" : "N";
echo is_subclass_of($object, "KnownClass") ? "Y" : "N";
echo is_subclass_of($object, "ParentClass") ? "Y" : "N";
echo call_user_func("is_a", $object, "ParentClass") ? "Y" : "N";
echo call_user_func_array("is_subclass_of", ["object_or_class" => $object, "class" => "ParentClass"]) ? "Y" : "N";
echo is_a(object_or_class: $object, class: "MissingClass", allow_string: false) ? "Y" : "N";"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "YNYYYN");
}
/// Verifies eval `define()` and `defined()` share a dynamic constant-name table.
#[test]
fn execute_program_define_and_defined_use_dynamic_constant_table() {
    let program = parse_fragment(
        br#"echo define("DynEvalConst", "ok") ? "Y" : "N";
echo DynEvalConst;
echo \DynEvalConst;
echo defined("DynEvalConst") ? "Y" : "N";
echo defined("\\DynEvalConst") ? "Y" : "N";
echo defined("dynevalconst") ? "Y" : "N";
echo define("DynEvalConst", 2) ? "Y" : "N";
echo call_user_func("defined", "DynEvalConst") ? "Y" : "N";
echo call_user_func_array("defined", ["constant_name" => "\\DynEvalConst"]) ? "Y" : "N";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "YokokYYNNYY");
    assert_eq!(
        values.warnings,
        vec![DEFINE_ALREADY_DEFINED_WARNING.to_string()]
    );
}
/// Verifies eval predefined runtime constants are fetchable and cannot be redefined.
#[test]
fn execute_program_reads_predefined_runtime_constants() {
    let program = parse_fragment(
        br#"echo PHP_EOL === "\n" ? "eol" : "bad"; echo ":";
echo (PHP_OS === "Darwin" || PHP_OS === "Linux") ? "os" : "bad"; echo ":";
echo DIRECTORY_SEPARATOR; echo ":";
echo PHP_INT_MAX > 9000000000000000000 ? "int" : "bad"; echo ":";
echo defined("PHP_OS") ? "defined" : "bad"; echo ":";
echo defined("\\PHP_OS") ? "root" : "bad"; echo ":";
echo defined("php_os") ? "bad" : "case"; echo ":";
echo define("PHP_OS", "x") ? "bad" : "locked"; echo ":";
return PHP_INT_MAX;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "eol:os:/:int:defined:root:case:locked:");
    assert_eq!(values.get(result), FakeValue::Int(i64::MAX));
    assert_eq!(
        values.warnings,
        vec![DEFINE_ALREADY_DEFINED_WARNING.to_string()]
    );
}
/// Verifies missing eval dynamic constants fail through runtime status.
#[test]
fn execute_program_missing_constant_fetch_fails() {
    let program = parse_fragment(br#"return MissingEvalConst;"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("missing constant should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
/// Verifies eval class probes use the runtime class-name table.
#[test]
fn execute_program_class_exists_uses_runtime_probe() {
    let program = parse_fragment(
        br#"class DynProbe {}
echo class_exists("DynProbe") ? "Y" : "N";
echo class_exists("\dynprobe") ? "Y" : "N";
echo class_exists("KnownClass") ? "Y" : "N";
echo class_exists("\knownclass") ? "Y" : "N";
echo class_exists(class: "MissingClass", autoload: false) ? "Y" : "N";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "YYYYN");
}
/// Verifies eval `class_alias()` registers dynamic and runtime-visible aliases.
#[test]
fn execute_program_class_alias_registers_aliases() {
    let program = parse_fragment(
        br#"class DynAliasBox {
    public int $x = 1;
    public function __construct($x) { $this->x = $x; }
    public function bump($n) { $this->x = $this->x + $n; return $this->x; }
}
interface DynAliasIface {}
trait DynAliasTrait {}
enum DynAliasEnum: string {
    case Ready = "ready";
}
echo class_alias("DynAliasBox", "DynAliasCopy") ? "alias" : "bad"; echo ":";
echo class_exists("DynAliasCopy") ? "exists" : "bad"; echo ":";
$box = new DynAliasCopy(5);
echo get_class($box); echo ":";
echo $box->bump(2); echo ":";
echo is_a($box, "DynAliasCopy") ? "isa" : "bad"; echo ":";
echo class_alias("DynAliasIface", "DynAliasIfaceCopy") ? "iface-alias" : "bad"; echo ":";
echo interface_exists("DynAliasIfaceCopy") ? "iface-exists" : "bad"; echo ":";
echo class_exists("DynAliasIfaceCopy") ? "bad" : "iface-not-class"; echo ":";
echo is_a("DynAliasIfaceCopy", "DynAliasIface", true) ? "iface-isa" : "bad"; echo ":";
echo (new ReflectionClass("DynAliasIfaceCopy"))->isInterface() ? "iface-reflect" : "bad"; echo ":";
echo class_alias("DynAliasTrait", "DynAliasTraitCopy") ? "trait-alias" : "bad"; echo ":";
echo trait_exists("DynAliasTraitCopy") ? "trait-exists" : "bad"; echo ":";
echo class_exists("DynAliasTraitCopy") ? "bad" : "trait-not-class"; echo ":";
echo is_a("DynAliasTraitCopy", "DynAliasTrait", true) ? "trait-isa" : "bad"; echo ":";
echo class_alias("DynAliasEnum", "DynAliasEnumCopy") ? "enum-alias" : "bad"; echo ":";
echo enum_exists("DynAliasEnumCopy") ? "enum-exists" : "bad"; echo ":";
echo class_exists("DynAliasEnumCopy") ? "enum-class" : "bad"; echo ":";
echo (new ReflectionClass("DynAliasEnumCopy"))->getName(); echo ":";
echo DynAliasEnumCopy::Ready->value; echo ":";
echo class_alias("DynAliasBox", "DynAliasCopy") ? "bad" : "duplicate"; echo ":";
echo class_alias("MissingAliasSource", "MissingAliasTarget") ? "bad" : "missing"; echo ":";
echo call_user_func("class_alias", "DynAliasBox", "DynAliasCall") ? "call" : "bad"; echo ":";
echo class_exists("DynAliasCall") ? "call-exists" : "bad"; echo ":";
echo call_user_func_array("class_alias", ["class" => "KnownClass", "alias" => "KnownAlias"]) ? "aot" : "bad"; echo ":";
$known = new KnownAlias();
echo is_a($known, "KnownAlias") ? "known" : "bad"; echo ":";
echo function_exists("class_alias");
return is_callable("class_alias");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "alias:exists:DynAliasBox:7:isa:iface-alias:iface-exists:iface-not-class:iface-isa:iface-reflect:trait-alias:trait-exists:trait-not-class:trait-isa:enum-alias:enum-exists:enum-class:DynAliasEnum:ready:duplicate:missing:call:call-exists:aot:known:1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `get_declared_*()` lists eval-visible class-like declarations.
#[test]
fn execute_program_get_declared_symbols_reports_eval_declarations() {
    let program = parse_fragment(
        br#"class DeclaredOne {}
class DeclaredTwo {}
class_alias("DeclaredOne", "DeclaredAlias");
$classes = get_declared_classes();
echo count($classes); echo ":";
echo $classes[0]; echo ":";
echo $classes[1]; echo ":";
$call = call_user_func("get_declared_classes");
echo count($call); echo ":";
echo count(get_declared_interfaces()); echo ":";
echo count(call_user_func_array("get_declared_traits", [])); echo ":";
echo function_exists("get_declared_classes");
echo function_exists("get_declared_interfaces");
return is_callable("get_declared_traits");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:DeclaredOne:DeclaredTwo:2:0:0:11");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies duplicate eval-declared class names fail through runtime status.
#[test]
fn execute_program_duplicate_class_declaration_fails() {
    let program = parse_fragment(
        br#"class DynProbeDup {}
class dynprobedup {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values).expect_err("duplicate fails");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
