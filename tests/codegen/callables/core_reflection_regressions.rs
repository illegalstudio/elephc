//! Purpose:
//! Pins metadata ordering, trait defaults, introspection visibility, and reflection value ownership.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_reflection_regression_`.
//!
//! Key details:
//! - Fixtures keep class-name and callable dispatch independent of reflection metadata order.

use crate::support::*;

/// Invalid class-introspection arguments remain catchable through opaque eval, CUF, CUFA, and FCC.
#[test]
fn test_core_eval_class_introspection_type_errors_are_catchable() {
    let methods_error = "get_class_methods(): Argument #1 ($object_or_class) must be an object or a valid class name, string given";
    let vars_error = "get_class_vars(): Argument #1 ($class) must be a valid class name, MissingReviewClass given";
    let mut body = "$methods = get_class_methods(...); $vars = get_class_vars(...);".to_string();
    let mut expected = String::new();
    for (call, error) in [
        ("get_class_methods(\"MissingReviewClass\")", methods_error),
        ("call_user_func(\"get_class_methods\", \"MissingReviewClass\")", methods_error),
        ("call_user_func_array(\"get_class_methods\", [\"MissingReviewClass\"])", methods_error),
        ("$methods(\"MissingReviewClass\")", methods_error),
        ("get_class_vars(\"MissingReviewClass\")", vars_error),
        ("call_user_func(\"get_class_vars\", \"MissingReviewClass\")", vars_error),
        ("call_user_func_array(\"get_class_vars\", [\"class\" => \"MissingReviewClass\"])", vars_error),
        ("$vars(\"MissingReviewClass\")", vars_error),
    ] {
        body.push_str(&format!("try {{ {call}; }} catch (TypeError $error) {{ echo $error->getMessage(), \"|\"; unset($error); }}"));
        expected.push_str(error);
        expected.push('|');
    }
    for (argument, given) in [("42", "int"), ("2.5", "float"), ("true", "bool"), ("null", "null"), ("[]", "array"), ("[\"key\" => 1]", "array")] {
        body.push_str(&format!("try {{ \\GET_CLASS_METHODS({argument}); }} catch (TypeError $error) {{ echo $error->getMessage(), \"|\"; unset($error); }}"));
        expected.push_str(&format!("get_class_methods(): Argument #1 ($object_or_class) must be an object or a valid class name, {given} given|"));
    }
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $source = '{quoted}' . ' // ' . $argc; eval($source); echo 'alive';");
    expected.push_str("alive");
    assert_eq!(compile_and_run(&source), expected);
}

/// Compact method-name data preserves spelling/order and returns independently writable arrays.
#[test]
fn test_core_class_methods_compact_results_have_independent_storage() {
    let output = compile_and_run(r#"<?php
class CompactMethodNames {
    public function zFirst(): void {}
    protected function hidden(): void {}
    public static function ASecond(): void {}
    public function last(): void {}
}
class NoMethodNames {}
class OneMethodName { public function only(): void {} }
$first = get_class_methods(CompactMethodNames::class);
$second = call_user_func("get_class_methods", CompactMethodNames::class);
$callback = get_class_methods(...);
$third = $callback(CompactMethodNames::class);
$first[0] = "changed";
unset($second[1]);
echo implode(",", $third), "|", implode(",", $first), "|",
    count(get_class_methods(NoMethodNames::class)), "|",
    implode(",", get_class_methods(OneMethodName::class));
"#);
    assert_eq!(output, "zFirst,ASecond,last|changed,ASecond,last|0|only");
}

/// Both reflection setters retain fresh values beyond source-argument cleanup and same-cell writes.
#[test]
fn test_core_eval_reflection_static_setters_keep_assigned_values_alive() {
    let source = r#"<?php
$source = 'class RetainedReflectionStatic { public static $value = null; }
$class = new ReflectionClass("RetainedReflectionStatic");
$property = new ReflectionProperty("RetainedReflectionStatic", "value");
$class->setStaticPropertyValue("value", str_repeat("x", 24));
echo strlen(RetainedReflectionStatic::$value), ":";
$property->setValue(null, ["payload" => [7, 8]]);
$same = RetainedReflectionStatic::$value;
$class->setStaticPropertyValue("value", $same);
unset($same);
echo RetainedReflectionStatic::$value["payload"][1], ":";
$property->setValue(null, 5);
echo RetainedReflectionStatic::$value;' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "24:8:5");
}

/// Standalone traits emit instance defaults before static defaults on direct and callable paths.
#[test]
fn test_core_class_vars_trait_static_first_order_matches_eval() {
    let body = r#"
trait StaticFirstTrait {
    public static int $s = 1;
    public int $x = 2;
    public static int $t = 3;
    public int $y = 4;
}
$direct = get_class_vars(StaticFirstTrait::class);
$callback = get_class_vars(...);
echo implode(",", array_keys($direct)), ":", implode(",", $direct), "|";
echo $callback(StaticFirstTrait::class) === $direct ? "F" : "bad";
echo call_user_func("get_class_vars", StaticFirstTrait::class) === $direct ? "C" : "bad";
"#;
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    for source in [
        format!("<?php {body}"),
        format!("<?php $source = '{quoted}' . ' // ' . $argc; eval($source);"),
    ] {
        assert_eq!(compile_and_run(&source), "x,y,s,t:2,4,1,3|FC");
    }
}

/// Eval subclasses retain native defaults, child overrides, visibility, and instance/static order.
#[test]
fn test_core_class_vars_eval_subclass_includes_native_parent_defaults() {
    let source = r#"<?php
class NativeVarsAncestor {
    public int $base = 1;
    public int $shared = 2;
    protected int $guard = 3;
    private int $hidden = 4;
    public static int $s = 5;
}
$source = 'class EvalVarsMiddle extends NativeVarsAncestor {
    public int $shared = 20;
    public int $child = 6;
    public static int $extra = 7;
}
class EvalVarsLeaf extends EvalVarsMiddle {
    public int $tail = 8;
    public function inspect(): void {
        $vars = get_class_vars(self::class);
        echo count($vars), ":", $vars["guard"], ":", isset($vars["hidden"]) ? "bad" : "private";
    }
}
$vars = get_class_vars(EvalVarsLeaf::class);
echo implode(",", array_keys($vars)), "|", implode(",", $vars), "|";
$callback = get_class_vars(...);
echo $callback(EvalVarsLeaf::class) === $vars ? "F" : "bad";
echo call_user_func("get_class_vars", EvalVarsLeaf::class) === $vars ? "C|" : "bad";
(new EvalVarsLeaf())->inspect();' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "base,shared,child,tail,s,extra|1,20,6,8,5,7|FC|7:3:private");
}

/// Pure eval inheritance uses the same declaration order as AOT without sorting away differences.
#[test]
fn test_core_class_vars_eval_defaults_preserve_inheritance_order() {
    let body = r#"
class OrderedVarsParent { public static int $s = 9; public int $first = 1; public int $shared = 2; }
class OrderedVarsChild extends OrderedVarsParent { public int $shared = 3; public int $last = 4; }
$vars = get_class_vars(OrderedVarsChild::class);
echo implode(",", array_keys($vars)), ":", implode(",", $vars);
"#;
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $source = '{quoted}' . ' // ' . $argc; eval($source);");
    assert_eq!(compile_and_run(&source), "first,shared,last,s:1,3,4,9");
}

/// Private parent slots cannot supply defaults for the same-named visible child property.
#[test]
fn test_core_class_vars_private_parent_shadow_uses_visible_default() {
    let source = r#"<?php
class ShadowDefaultParent { private int $value = 1; }
class ShadowDefaultChild extends ShadowDefaultParent { public int $value = 2; }
class ShadowDefaultGrandchild extends ShadowDefaultChild {}
echo get_class_vars(ShadowDefaultChild::class)['value'], '|';
$name = $argc > 0 ? 'ShadowDefaultChild' : 'ShadowDefaultGrandchild';
echo call_user_func('get_class_vars', $name)['value'], '|';
$callback = get_class_vars(...);
echo $callback(ShadowDefaultGrandchild::class)['value'];
"#;
    assert_eq!(compile_and_run(source), "2|2|2");
}

/// Default expressions use their declaration scope while visibility and following code use the caller.
#[test]
fn test_core_class_vars_defaults_preserve_declaring_scope() {
    let out = compile_and_run(r#"<?php
class DefaultParent {
    public string $owner = self::class;
    public static array $nested = [self::class];
    protected string $hidden = self::class;
}
class DefaultChild extends DefaultParent {
    public string $parentName = parent::class;
}
class DefaultCaller {
    public static function inspect(): void {
        $vars = get_class_vars(DefaultChild::class);
        echo $vars['owner'], ':', $vars['nested'][0], ':', $vars['parentName'], ':';
        echo isset($vars['hidden']) ? 'bad' : 'private', ':', self::class;
    }
}
echo get_class_vars(DefaultParent::class)['owner'], '|';
DefaultCaller::inspect();
"#);
    assert_eq!(out, "DefaultParent|DefaultParent:DefaultParent:DefaultParent:private:DefaultCaller");
}

/// CUF and FCC materialize class and standalone-trait defaults in their lexical declaration scope.
#[test]
fn test_core_class_vars_callable_defaults_preserve_declaring_scope() {
    let out = compile_and_run(r#"<?php
trait DefaultTrait { public string $owner = self::class; }
class DefaultConsumer { use DefaultTrait; }
$vars = call_user_func('get_class_vars', DefaultConsumer::class);
echo $vars['owner'], ':';
$callback = get_class_vars(...);
$traitVars = $callback(DefaultTrait::class);
echo $traitVars['owner'];
"#);
    assert_eq!(out, "DefaultConsumer:DefaultTrait");
}

/// Parent and child scopes can see each other's protected declarations, but outsiders cannot.
#[test]
fn test_core_reflection_regression_protected_visibility_is_bidirectional() {
    let out = compile_and_run(r#"<?php
class VisibilityParent {
    protected int $parentValue = 1;
    public static function inspect(): void {
        $vars = get_class_vars(VisibilityChild::class);
        echo isset($vars['childValue']) ? 'V' : 'bad';
        echo in_array('childMethod', get_class_methods(VisibilityChild::class)) ? 'M' : 'bad';
        echo isset($vars['secret']) ? 'bad' : '-';
    }
}
class VisibilityChild extends VisibilityParent {
    protected int $childValue = 2;
    private int $secret = 3;
    protected function childMethod(): void {}
    public static function inspectParent(): void {
        echo isset(get_class_vars(VisibilityParent::class)['parentValue']) ? 'P' : 'bad';
    }
}
VisibilityParent::inspect();
VisibilityChild::inspectParent();
echo isset(get_class_vars(VisibilityChild::class)['childValue']) ? 'bad' : '-';
"#);
    assert_eq!(out, "VM-P-");
}

/// Standalone traits expose visible instance/static defaults, including nested array values.
#[test]
fn test_core_reflection_regression_trait_property_defaults() {
    let out = compile_and_run(r#"<?php
trait VisibleTrait {
    public int $zebra = 7;
    protected int $hidden = 9;
    public static array $items = [1, 8];
}
$name = VisibleTrait::class;
$vars = call_user_func('get_class_vars', $name);
echo implode(',', array_keys($vars)), ':', $vars['zebra'], ':', $vars['items'][1];
"#);
    assert_eq!(out, "zebra,items:7:8");
}

/// Method lists preserve declaration order, inherited placement, and enum intrinsic spelling.
#[test]
fn test_core_reflection_regression_method_order_and_enum_spelling() {
    let out = compile_and_run(r#"<?php
class OrderedParent { public function parentLast(): void {} }
class OrderedChild extends OrderedParent {
    public function zebra(): void {}
    public static function middle(): void {}
    public function alpha(): void {}
}
trait OrderedTrait {
    public function zebra(): void {}
    public static function middle(): void {}
    public function alpha(): void {}
}
interface OrderedInterface {
    public function zebra(): void;
    public static function middle(): void;
    public function alpha(): void;
}
enum OrderedEnum: int {
    case A = 1;
    public function label(): string { return $this->name; }
}
$methods = get_class_methods(...);
echo implode(',', $methods(OrderedChild::class)), '|';
echo implode(',', get_class_methods(OrderedTrait::class)), '|';
echo implode(',', call_user_func('get_class_methods', OrderedInterface::class)), '|';
echo implode(',', get_class_methods(OrderedEnum::class));
"#);
    assert_eq!(out, "zebra,middle,alpha,parentLast|zebra,middle,alpha|zebra,middle,alpha|label,cases,from,tryFrom");
}

/// Boxed `mixed` class names reach every AOT dispatch form, at an early and a late candidate.
#[test]
fn test_core_class_introspection_accepts_boxed_runtime_class_names() {
    let out = compile_and_run(r#"<?php
class BoxedVarsFirst { public int $a = 1; public function first(): void {} }
class BoxedVarsLate { public int $z = 26; public function last(): void {} }
$names = ["early" => $argc > 0 ? "BoxedVarsFirst" : "BoxedVarsLate", "late" => "BoxedVarsLate", "count" => 2];
$early = $names["early"];
$callback = get_class_vars(...);
echo implode(",", array_keys(get_class_vars($early))), "|";
echo implode(",", array_keys(call_user_func("get_class_vars", $names["late"]))), "|";
echo implode(",", array_keys($callback($names["early"]))), "|";
echo implode(",", array_keys(call_user_func_array("get_class_vars", [$names["late"]]))), "|";
echo implode(",", array_keys(get_class_vars(...[$names["early"]]))), "|";
echo implode(",", get_class_methods($names["late"]));
"#);
    assert_eq!(out, "a|z|a|z|a|last");
}

/// Invalid boxed tags throw PHP's catchable TypeError instead of becoming class-name strings.
#[test]
fn test_core_class_introspection_boxed_tags_reject_non_strings() {
    let out = compile_and_run(r#"<?php
class BoxedTagProbe { public int $a = 1; public function m(): void {} }
$values = ["int" => 7, "bool" => true, "null" => null, "array" => [1], "object" => new BoxedTagProbe(), "name" => "BoxedTagProbe"];
foreach (["int", "bool", "null", "array", "object"] as $key) {
    try { get_class_vars($values[$key]); echo "bad|"; }
    catch (TypeError $error) { echo $error->getMessage(), "|"; }
}
foreach (["int", "bool", "null", "array"] as $key) {
    try { get_class_methods($values[$key]); echo "bad|"; }
    catch (TypeError $error) { echo $error->getMessage(), "|"; }
}
echo implode(",", get_class_methods($values["object"])), "|";
echo implode(",", get_class_methods($values["name"])), "|";
echo implode(",", array_keys(get_class_vars($values["name"])));
"#);
    let vars = "get_class_vars(): Argument #1 ($class) must be of type string, ";
    let methods = "get_class_methods(): Argument #1 ($object_or_class) must be an object or a valid class name, ";
    assert_eq!(
        out,
        format!(
            "{vars}int given|{vars}bool given|{vars}null given|{vars}array given|{vars}object given|\
             {methods}int given|{methods}bool given|{methods}null given|{methods}array given|m|m|a"
        )
    );
}

/// A boxed introspection argument is evaluated once and never frees the caller's own object.
#[test]
fn test_core_class_introspection_boxed_arguments_evaluate_once() {
    let out = compile_and_run(r#"<?php
class BoxedOnceTarget { public int $a = 1; public function m(): void {} }
class BoxedOnceOwner { public string $tag = "kept"; public function owned(): void {} }
function boxedOnceName(): mixed { echo "e"; return "BoxedOnceTarget"; }
echo implode(",", array_keys(get_class_vars(boxedOnceName()))), "|";
$owner = new BoxedOnceOwner();
$boxed = ["owner" => $owner, "count" => 1];
echo implode(",", get_class_methods($boxed["owner"])), "|";
echo $owner->tag, "|", $boxed["owner"]->tag;
"#);
    assert_eq!(out, "ea|owned|kept|kept");
}

/// An owned boxed temporary is retired at the failing call, before its TypeError is catchable.
///
/// The validator releases its published input immediately before the throw, so the destructor
/// of a temporary object argument runs while the same-frame catch is still pending.
#[test]
fn test_core_class_introspection_boxed_temporary_is_retired_before_the_throw() {
    let out = compile_and_run(r#"<?php
class BoxedTimingProbe { public function __destruct() { echo "D"; } }
function boxedTimingTemporary(): mixed { return new BoxedTimingProbe(); }
try { get_class_vars(boxedTimingTemporary()); echo "bad"; }
catch (TypeError $error) { echo "T"; }
echo "|end";
"#);
    assert_eq!(out, "DT|end");
}
