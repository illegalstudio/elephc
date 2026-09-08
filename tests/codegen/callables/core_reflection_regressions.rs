//! Purpose:
//! Pins PHP's metadata ordering, trait defaults, and protected introspection visibility.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_reflection_regression_`.
//!
//! Key details:
//! - Fixtures keep class-name and callable dispatch independent of reflection metadata order.

use crate::support::*;

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
