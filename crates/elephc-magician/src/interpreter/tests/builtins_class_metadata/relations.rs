//! Purpose:
//! Interpreter tests for class relations, visible members, and class variables.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Eval and AOT class targets share relation-builtin behavior.
//! - Visibility-sensitive OOP probes run through the fake interpreter runtime.

use super::super::super::*;
use super::super::support::*;

/// Verifies class-relation helpers handle eval class and trait targets.
#[test]
fn execute_program_dispatches_class_relation_builtins() {
    let program = parse_fragment(
        br#"class EvalMeta {}
trait EvalMetaInnerTrait {}
trait EvalMetaOuterTrait {
    use EvalMetaInnerTrait;
}
$object = new EvalMeta();
$implements = class_implements("EvalMeta");
echo is_array($implements) && count($implements) === 0 ? "impl" : "bad"; echo ":";
$parents = class_parents($object);
echo is_array($parents) && count($parents) === 0 ? "parents" : "bad"; echo ":";
$uses = class_uses("EvalMeta");
echo is_array($uses) && count($uses) === 0 ? "uses" : "bad"; echo ":";
echo class_implements("MissingMeta") === false ? "missing" : "bad"; echo ":";
$call = call_user_func("class_implements", "EvalMeta");
echo is_array($call) && count($call) === 0 ? "call" : "bad"; echo ":";
$named = call_user_func_array("class_parents", ["object_or_class" => "EvalMeta"]);
echo is_array($named) && count($named) === 0 ? "named" : "bad"; echo ":";
$trait_uses = class_uses("EvalMetaOuterTrait");
echo $trait_uses["EvalMetaInnerTrait"]; echo ":";
class_alias("EvalMetaOuterTrait", "EvalMetaOuterTraitAlias");
$alias_uses = class_uses("EvalMetaOuterTraitAlias");
echo $alias_uses["EvalMetaInnerTrait"]; echo ":";
echo function_exists("class_implements"); echo function_exists("class_parents");
echo function_exists("class_uses");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "impl:parents:uses:missing:call:named:EvalMetaInnerTrait:EvalMetaInnerTrait:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval-declared parent and interface metadata is exposed to relation builtins.
#[test]
fn execute_program_reports_eval_class_relation_metadata() {
    let program = parse_fragment(
        br#"class EvalMetaBase {}
class EvalMetaChild extends EvalMetaBase implements KnownInterface {}
$object = new EvalMetaChild();
$implements = class_implements($object);
echo count($implements); echo ":";
echo $implements["KnownInterface"]; echo ":";
echo $implements["Traversable"]; echo ":";
$parents = class_parents("EvalMetaChild");
echo count($parents); echo ":";
echo $parents["EvalMetaBase"]; echo ":";
$call = call_user_func("class_implements", "EvalMetaChild");
echo $call["KnownInterface"]; echo ":";
echo $call["Traversable"]; echo ":";
$named = call_user_func_array("class_parents", ["object_or_class" => $object]);
echo $named["EvalMetaBase"];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "2:KnownInterface:Traversable:1:EvalMetaBase:KnownInterface:Traversable:EvalMetaBase"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies generated/AOT parent and interface metadata is exposed to relation builtins.
#[test]
fn execute_program_reports_aot_class_relation_metadata() {
    let program = parse_fragment(
        br#"$implements = class_implements("KnownClass");
echo count($implements); echo ":";
echo $implements["KnownInterface"]; echo ":";
$parents = class_parents("KnownClass");
echo count($parents); echo ":";
echo $parents["ParentClass"]; echo ":";
$call = call_user_func("class_implements", "KnownClass");
echo $call["KnownInterface"]; echo ":";
$interfaceParents = class_implements("KnownInterface");
echo $interfaceParents["Traversable"]; echo ":";
$uses = class_uses("KnownClass");
echo count($uses); echo ":"; echo $uses["KnownTrait"]; echo ":";
$traitUses = class_uses("KnownTrait");
echo $traitUses["KnownInnerTrait"]; echo ":";
$named = call_user_func_array("class_parents", ["object_or_class" => "KnownClass"]);
echo $named["ParentClass"]; echo ":";
class_alias("KnownClass", "KnownAlias");
$aliasImplements = class_implements("KnownAlias");
echo $aliasImplements["KnownInterface"]; echo ":";
$aliasParents = class_parents("KnownAlias");
echo $aliasParents["ParentClass"]; echo ":";
$aliasUses = class_uses("KnownAlias");
echo $aliasUses["KnownTrait"];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    assert!(context.define_native_class_parent("KnownClass", "ParentClass"));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result =
        execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:KnownInterface:1:ParentClass:KnownInterface:Traversable:1:KnownTrait:KnownInnerTrait:ParentClass:KnownInterface:ParentClass:KnownTrait"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies PHP OOP introspection builtins follow eval visibility and scope rules.
#[test]
fn execute_program_dispatches_oop_introspection_builtins() {
    let program = parse_fragment(
        br#"class EvalOopIntrospectBase {
    private $baseSecret = "bp";
    protected $baseProtected = "bq";
    public $basePublic = "br";
    private function basePrivate() {}
    protected function baseProtectedMethod() {}
    public function basePublicMethod() {}
    public function parentView() {
        $vars = get_object_vars($this);
        ksort($vars);
        echo implode(",", array_keys($vars));
    }
}
class EvalOopIntrospectChild extends EvalOopIntrospectBase {
    private $childSecret = "cp";
    protected $childProtected = "cq";
    public $childPublic = "cr";
    private function childPrivate() {}
    protected function childProtectedMethod() {}
    public function childPublicMethod() {}
    public function childView() {
        $methods = get_class_methods($this);
        sort($methods);
        echo implode(",", $methods); echo "|";
        $vars = get_object_vars($this);
        ksort($vars);
        echo implode(",", array_keys($vars));
    }
}
$object = new EvalOopIntrospectChild();
$object->dynamic = "dyn";
echo method_exists("EvalOopIntrospectChild", "basePrivate") ? "bad" : "noParentPrivateMethod"; echo ":";
echo method_exists($object, "basePrivate") ? "objectParentPrivateMethod" : "bad"; echo ":";
echo method_exists("EvalOopIntrospectChild", "baseProtectedMethod") ? "classProtectedMethod" : "bad"; echo ":";
echo property_exists("EvalOopIntrospectChild", "baseSecret") ? "bad" : "noParentPrivateProperty"; echo ":";
echo property_exists($object, "baseSecret") ? "bad" : "noObjectParentPrivateProperty"; echo ":";
echo property_exists($object, "dynamic") ? "dynamicProperty" : "bad"; echo ":";
$methods = get_class_methods("EvalOopIntrospectChild");
sort($methods);
echo implode(",", $methods); echo ":";
$vars = get_object_vars($object);
ksort($vars);
echo implode(",", array_keys($vars)); echo ":";
$object->childView(); echo ":";
$object->parentView(); echo ":";
echo call_user_func("method_exists", $object, "childPrivate") ? "callMethod" : "bad"; echo ":";
echo call_user_func_array("property_exists", ["property" => "dynamic", "object_or_class" => $object]) ? "namedProperty" : "bad"; echo ":";
echo function_exists("method_exists"); echo function_exists("property_exists");
echo function_exists("get_class_methods"); echo function_exists("get_object_vars");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "noParentPrivateMethod:objectParentPrivateMethod:classProtectedMethod:noParentPrivateProperty:noObjectParentPrivateProperty:dynamicProperty:basePublicMethod,childPublicMethod,childView,parentView:basePublic,childPublic,dynamic:baseProtectedMethod,basePublicMethod,childPrivate,childProtectedMethod,childPublicMethod,childView,parentView|baseProtected,basePublic,childProtected,childPublic,childSecret,dynamic:baseProtected,basePublic,baseSecret,childProtected,childPublic,dynamic:callMethod:namedProperty:1111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies `get_class_vars()` materializes visible defaults for eval class-like metadata.
#[test]
fn execute_program_dispatches_get_class_vars_builtin() {
    let program = parse_fragment(
        br#"trait EvalClassVarsTrait {
    public $traitPublic = "tp";
    protected $traitProtected = "tq";
}
enum EvalClassVarsBacked: int { case Ready = 1; }
class EvalClassVarsBase {
    public $basePublic = "bp";
    protected $baseProtected = "bq";
    private $basePrivate = "bs";
    public static $baseStatic = "static";
    public int $typed;
}
class EvalClassVarsChild extends EvalClassVarsBase {
    use EvalClassVarsTrait;
    public $childPublic = "cp";
    protected $childProtected = "cq";
    private $childPrivate = "cs";
    public function childView() {
        $vars = get_class_vars(self::class);
        ksort($vars);
        foreach ($vars as $name => $value) {
            echo $name . "=" . (is_null($value) ? "null" : $value) . "|";
        }
    }
    public function baseView() {
        $vars = get_class_vars(EvalClassVarsBase::class);
        ksort($vars);
        foreach ($vars as $name => $value) {
            echo $name . "=" . (is_null($value) ? "null" : $value) . "|";
        }
    }
}
$outside = get_class_vars("EvalClassVarsChild");
ksort($outside);
foreach ($outside as $name => $value) { echo $name . "=" . (is_null($value) ? "null" : $value) . "|"; }
echo ":";
(new EvalClassVarsChild())->childView();
echo ":";
(new EvalClassVarsChild())->baseView();
echo ":";
$trait = call_user_func("get_class_vars", "EvalClassVarsTrait");
ksort($trait);
foreach ($trait as $name => $value) { echo $name . "=" . (is_null($value) ? "null" : $value) . "|"; }
echo ":";
$enum = call_user_func_array("get_class_vars", ["class" => "EvalClassVarsBacked"]);
ksort($enum);
foreach ($enum as $name => $value) { echo $name . "=" . (is_null($value) ? "null" : $value) . "|"; }
echo ":";
echo function_exists("get_class_vars") ? "F" : "f";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "basePublic=bp|baseStatic=static|childPublic=cp|traitPublic=tp|typed=null|:baseProtected=bq|basePublic=bp|baseStatic=static|childPrivate=cs|childProtected=cq|childPublic=cp|traitProtected=tq|traitPublic=tp|typed=null|:baseProtected=bq|basePublic=bp|baseStatic=static|typed=null|:traitPublic=tp|:name=null|value=null|:F"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
