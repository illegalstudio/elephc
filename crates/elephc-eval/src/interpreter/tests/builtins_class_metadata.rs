//! Purpose:
//! Interpreter tests for eval class metadata and relation builtins.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - Eval class declarations expose parent/interface metadata plus class-level
//!   attribute names and supported literal positional args.
//! - Tests verify direct calls, dynamic calls, named arguments, and builtin probes.

use super::super::*;
use super::support::*;

/// Verifies class-relation helpers return empty arrays for known eval classes.
#[test]
fn execute_program_dispatches_class_relation_builtins() {
    let program = parse_fragment(
        br#"class EvalMeta {}
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
echo function_exists("class_implements"); echo function_exists("class_parents");
echo function_exists("class_uses");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "impl:parents:uses:missing:call:named:111");
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
$parents = class_parents("EvalMetaChild");
echo count($parents); echo ":";
echo $parents["EvalMetaBase"]; echo ":";
$call = call_user_func("class_implements", "EvalMetaChild");
echo $call["KnownInterface"]; echo ":";
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
        "1:KnownInterface:1:EvalMetaBase:KnownInterface:EvalMetaBase"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies class attribute helpers expose eval class-level metadata.
#[test]
fn execute_program_dispatches_class_attribute_metadata_builtins() {
    let program = parse_fragment(
        br#"#[Route("/home", -1, true, null)]
#[Tag("first"), Tag("second")]
class EvalAttrMeta {}
$names = class_attribute_names("EvalAttrMeta");
echo count($names); echo ":"; echo $names[0]; echo ":"; echo $names[1]; echo ":"; echo $names[2]; echo ":";
$args = class_attribute_args("EvalAttrMeta", "route");
echo count($args); echo ":"; echo $args[0]; echo ":"; echo $args[1]; echo ":";
echo $args[2] ? "T" : "F"; echo ":"; echo is_null($args[3]) ? "N" : "bad"; echo ":";
$tag = class_attribute_args("evalattrmeta", "Tag");
echo $tag[0]; echo ":";
$missing = class_attribute_args("EvalAttrMeta", "Missing");
echo count($missing); echo ":";
$attrs = class_get_attributes("EvalAttrMeta");
echo count($attrs); echo ":"; echo $attrs[0]->getName(); echo ":";
$attr_args = $attrs[0]->getArguments();
echo count($attr_args); echo ":"; echo $attr_args[0]; echo ":"; echo $attr_args[1]; echo ":";
echo $attr_args[2] ? "T" : "F"; echo ":"; echo is_null($attr_args[3]) ? "N" : "bad"; echo ":";
$tag_attr_args = $attrs[1]->getArguments();
echo $attrs[1]->getName(); echo ":"; echo $tag_attr_args[0]; echo ":";
echo is_null($attrs[0]->newInstance()) ? "N" : "bad"; echo ":";
$call_names = call_user_func("class_attribute_names", "EvalAttrMeta");
echo $call_names[0]; echo ":";
$call_args = call_user_func_array(
    "class_attribute_args",
    ["class_name" => "EvalAttrMeta", "attribute_name" => "Route"]
);
echo $call_args[0]; echo ":";
echo function_exists("class_attribute_names"); echo function_exists("class_get_attributes");
echo function_exists("class_attribute_args");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "3:Route:Tag:Tag:4:/home:-1:T:N:first:0:3:Route:4:/home:-1:T:N:Tag:first:N:Route:/home:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionAttribute::newInstance instantiates eval-declared attribute classes.
#[test]
fn execute_program_instantiates_eval_declared_reflection_attribute() {
    let program = parse_fragment(
        br#"class EvalRoute {
    public $path;
    public $code;
    public $enabled;
    public function __construct($path, $code, $enabled) {
        $this->path = $path;
        $this->code = $code;
        $this->enabled = $enabled;
    }
    public function summary() {
        return $this->path . ":" . $this->code . ":" . ($this->enabled ? "T" : "F");
    }
}
#[EvalRoute("/home", -7, true)]
class EvalRouteTarget {}
$attrs = class_get_attributes("EvalRouteTarget");
$instance = $attrs[0]->newInstance();
echo get_class($instance); echo ":"; echo $instance->summary();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "EvalRoute:/home:-7:T");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass/Method/Property expose eval-declared attribute metadata.
#[test]
fn execute_program_reflects_eval_member_attributes() {
    let program = parse_fragment(
        br#"class EvalMarker {
    public $name;
    public function __construct($name) {
        $this->name = $name;
    }
    public function label() {
        return $this->name;
    }
}
#[EvalMarker("class")]
class EvalReflectTarget {
    #[EvalMarker("method")]
    public function handle() {}
    #[EvalMarker("property")]
    public $id;
}
$class_attrs = (new ReflectionClass("EvalReflectTarget"))->getAttributes();
echo count($class_attrs); echo ":"; echo (new ReflectionClass("EvalReflectTarget"))->getName(); echo ":";
echo $class_attrs[0]->getName(); echo ":"; echo $class_attrs[0]->newInstance()->label(); echo ":";
$method_attrs = (new ReflectionMethod("EvalReflectTarget", "handle"))->getAttributes();
echo count($method_attrs); echo ":"; echo $method_attrs[0]->getName(); echo ":";
echo $method_attrs[0]->getArguments()[0]; echo ":"; echo $method_attrs[0]->newInstance()->label(); echo ":";
$property_attrs = (new ReflectionProperty("EvalReflectTarget", "id"))->getAttributes();
echo count($property_attrs); echo ":"; echo $property_attrs[0]->getName(); echo ":";
echo $property_attrs[0]->getArguments()[0]; echo ":"; echo $property_attrs[0]->newInstance()->label();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:EvalReflectTarget:EvalMarker:class:1:EvalMarker:method:method:1:EvalMarker:property:property"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies unsupported attribute argument metadata remains name-visible but not materializable.
#[test]
fn execute_program_rejects_unsupported_class_attribute_args_metadata() {
    let program = parse_fragment(
        br#"#[Tag($dynamic)]
class EvalUnsupportedAttr {}
$names = class_attribute_names("EvalUnsupportedAttr");
echo count($names); echo ":"; echo $names[0]; echo ":";
class_attribute_args("EvalUnsupportedAttr", "Tag");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("unsupported attribute metadata should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
    assert_eq!(values.output, "1:Tag:");
}
