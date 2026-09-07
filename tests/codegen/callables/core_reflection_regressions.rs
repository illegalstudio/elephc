//! Purpose:
//! Pins PHP's metadata ordering, trait defaults, and protected introspection visibility.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_reflection_regression_`.
//!
//! Key details:
//! - Fixtures keep class-name and callable dispatch independent of reflection metadata order.

use crate::support::*;

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
