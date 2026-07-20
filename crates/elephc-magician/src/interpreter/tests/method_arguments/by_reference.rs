//! Purpose:
//! Interpreter tests for eval method by-reference binding and writeback across
//! variables, arrays, object/static properties, and nested elements.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Invalid temporary and readonly targets retain their fatal behavior.

use super::super::super::*;
use super::super::support::*;

/// Verifies eval-declared instance, static, and constructor methods write back by-reference args.
#[test]
fn execute_program_writes_back_eval_method_by_ref_args() {
    let program = parse_fragment(
        br#"class EvalByRefMethodBox {
    public function __construct(&$value) {
        $value = $value . "-ctor";
    }
    public function change(&$value) {
        $value = $value . "-method";
    }
    public static function changeStatic(&$value) {
        $value = $value . "-static";
    }
}
$ctor = "A";
$box = new EvalByRefMethodBox($ctor);
$box->change($ctor);
EvalByRefMethodBox::changeStatic($ctor);
$named = "B";
$box->change(value: $named);
echo $ctor; echo ":";
return $named;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A-ctor-method-static:");
    assert_eq!(values.get(result), FakeValue::String("B-method".to_string()));
}

/// Verifies eval-declared by-reference method parameters reject temporary values.
#[test]
fn execute_program_rejects_eval_method_by_ref_temporary_arg() {
    let program = parse_fragment(
        br#"class EvalByRefMethodBox {
    public function change(&$value) {
        $value = "changed";
    }
}
$box = new EvalByRefMethodBox();
$box->change("literal");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("literal cannot satisfy a by-reference parameter");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies typed eval-declared by-reference method params write back entry coercions.
#[test]
fn execute_program_writes_back_eval_method_by_ref_type_coercion() {
    let program = parse_fragment(
        br#"class EvalByRefTypedMethodBox {
    public function coerce(int &$value) {}
}
$value = "3";
$box = new EvalByRefTypedMethodBox();
$box->coerce($value);
return $value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(3));
}

/// Verifies eval-declared by-reference variadics write back mutated captured elements.
#[test]
fn execute_program_writes_back_eval_method_by_ref_variadic_elements() {
    let program = parse_fragment(
        br#"class EvalByRefVariadicMethodBox {
    public function change(&...$items) {
        $items[0] = $items[0] . "-first";
        $items["named"] = $items["named"] . "-named";
    }
    public function rebind(&...$items) {
        $items = [];
    }
}
$box = new EvalByRefVariadicMethodBox();
$first = "A";
$named = "B";
$box->change($first, named: $named);
$box->rebind($first);
echo $first; echo ":";
return $named;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A-first:");
    assert_eq!(values.get(result), FakeValue::String("B-named".to_string()));
}

/// Verifies eval-declared by-reference method params write back array-element lvalues.
#[test]
fn execute_program_writes_back_eval_method_by_ref_array_elements() {
    let program = parse_fragment(
        br#"class EvalByRefArrayElementMethodBox {
    public function set(&$value, $next) {
        $value = $next;
    }
    public function variadic(&...$items) {
        $items[0] = "variadic";
    }
}
$box = new EvalByRefArrayElementMethodBox();
$items = ["k" => "old"];
$box->set($items["k"], "changed");
$box->set($missing["new"], "created");
$box->variadic($items["k"]);
echo $items["k"]; echo ":";
return $missing["new"];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "variadic:");
    assert_eq!(values.get(result), FakeValue::String("created".to_string()));
}

/// Verifies eval-declared by-reference method params write back object-property lvalues.
#[test]
fn execute_program_writes_back_eval_method_by_ref_object_properties() {
    let program = parse_fragment(
        br#"class EvalByRefPropertyChanger {
    public function set(&$value, $next) {
        $value = $next;
    }
    public function variadic(&...$items) {
        $items[0] = "variadic";
    }
}
class EvalByRefPublicPropertyBox {
    public string $value = "old";
}
class EvalByRefPrivatePropertyBox {
    private string $value = "private";
    public function update($changer) {
        $name = "value";
        $changer->set($this->{$name}, "secret");
        return $this->value;
    }
}
$changer = new EvalByRefPropertyChanger();
$public = new EvalByRefPublicPropertyBox();
$changer->set($public->value, "changed");
$changer->variadic($public->value);
$name = "value";
$changer->set($public->{$name}, "dynamic");
echo $public->value; echo ":";
$private = new EvalByRefPrivatePropertyBox();
return $private->update($changer);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "dynamic:");
    assert_eq!(values.get(result), FakeValue::String("secret".to_string()));
}

/// Verifies eval-declared by-reference method params write back static-property lvalues.
#[test]
fn execute_program_writes_back_eval_method_by_ref_static_properties() {
    let program = parse_fragment(
        br#"class EvalByRefStaticPropertyChanger {
    public function set(&$value, $next) {
        $value = $next;
    }
    public function pair(&$left, &$right) {
        $left = "left";
        $right = "right";
        return $left;
    }
}
class EvalByRefStaticPropertyBox {
    public static string $value = "old";
    public static string $other = "second";
    public static string $third = "third";
    private static string $secret = "private";
    public static function updatePrivate($changer) {
        $changer->set(self::$secret, "secret");
        return self::$secret;
    }
}
$changer = new EvalByRefStaticPropertyChanger();
$changer->set(EvalByRefStaticPropertyBox::$value, "changed");
echo $changer->pair(EvalByRefStaticPropertyBox::$value, EvalByRefStaticPropertyBox::$value);
echo ":";
echo EvalByRefStaticPropertyBox::$value; echo ":";
$class = "EvalByRefStaticPropertyBox";
$changer->set($class::$other, "dynamic");
$name = "third";
$changer->set($class::${$name}, "name");
echo EvalByRefStaticPropertyBox::$other; echo ":";
echo EvalByRefStaticPropertyBox::$third; echo ":";
return EvalByRefStaticPropertyBox::updatePrivate($changer);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "right:right:dynamic:name:");
    assert_eq!(values.get(result), FakeValue::String("secret".to_string()));
}

/// Verifies by-reference method params write back array elements stored in properties.
#[test]
fn execute_program_writes_back_eval_method_by_ref_property_array_elements() {
    let program = parse_fragment(
        br#"class EvalByRefPropertyArrayElementChanger {
    public function set(&$value, $next) {
        $value = $next;
    }
    public function pair(&$left, &$right) {
        $left = "left";
        $right = "right";
        return $left;
    }
}
class EvalByRefPropertyArrayElementBox {
    public $items = ["first" => "old", "same" => "same"];
    public $other = null;
    public static $staticItems = ["first" => "static-old", "same" => "static-same"];
}
$changer = new EvalByRefPropertyArrayElementChanger();
$box = new EvalByRefPropertyArrayElementBox();
$changer->set($box->items["first"], "changed");
$name = "items";
$changer->set($box->{$name}["dynamic"], "dynamic");
$changer->set($box->other["created"], "created");
echo $box->items["first"]; echo ":";
echo $box->items["dynamic"]; echo ":";
echo $box->other["created"]; echo ":";
echo $changer->pair($box->items["same"], $box->items["same"]); echo ":";
echo $box->items["same"]; echo ":";
$changer->set(EvalByRefPropertyArrayElementBox::$staticItems["first"], "static");
$class = "EvalByRefPropertyArrayElementBox";
$staticName = "staticItems";
$changer->set($class::${$staticName}["dynamic"], "static-dynamic");
echo EvalByRefPropertyArrayElementBox::$staticItems["first"]; echo ":";
echo EvalByRefPropertyArrayElementBox::$staticItems["dynamic"]; echo ":";
return $changer->pair(
    EvalByRefPropertyArrayElementBox::$staticItems["same"],
    EvalByRefPropertyArrayElementBox::$staticItems["same"]
) . ":" . EvalByRefPropertyArrayElementBox::$staticItems["same"];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "changed:dynamic:created:right:right:static:static-dynamic:");
    assert_eq!(values.get(result), FakeValue::String("right:right".to_string()));
}

/// Verifies eval-declared by-reference method params keep property access restrictions.
#[test]
fn execute_program_rejects_invalid_eval_method_by_ref_object_property_targets() {
    let private_program = parse_fragment(
        br#"class EvalByRefPrivatePropertyFailChanger {
    public function set(&$value) {
        $value = "bad";
    }
}
class EvalByRefPrivatePropertyFailBox {
    private string $value = "private";
}
$changer = new EvalByRefPrivatePropertyFailChanger();
$box = new EvalByRefPrivatePropertyFailBox();
$changer->set($box->value);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&private_program, &mut scope, &mut values)
        .expect_err("private property by-ref target should fail from global scope");
    assert_eq!(err, EvalStatus::UncaughtThrowable);

    let readonly_program = parse_fragment(
        br#"class EvalByRefReadonlyPropertyFailChanger {
    public function set(&$value) {
        $value = "bad";
    }
}
class EvalByRefReadonlyPropertyFailBox {
    public readonly string $value;
    public function __construct($changer) {
        $this->value = "old";
        $changer->set($this->value);
    }
}
new EvalByRefReadonlyPropertyFailBox(new EvalByRefReadonlyPropertyFailChanger());"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&readonly_program, &mut scope, &mut values)
        .expect_err("readonly property by-ref target should fail as an indirect modification");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}
