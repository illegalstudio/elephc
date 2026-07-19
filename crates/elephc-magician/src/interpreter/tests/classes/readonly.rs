//! Purpose:
//! Interpreter tests for readonly properties, readonly classes, and dynamic-
//! property restrictions.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Constructor initialization and all post-initialization write paths are
//!   checked independently.

use super::super::super::*;
use super::super::support::*;

/// Verifies promoted readonly properties throw Error outside their constructor.
#[test]
fn execute_program_promoted_readonly_property_write_after_constructor_throws_error() {
    let program = parse_fragment(
        br#"class EvalPromotedReadonlyBox {
    public function __construct(public readonly int $id) {}
    public function replace($id) { $this->id = $id; }
}
$box = new EvalPromotedReadonlyBox(7);
echo $box->id;
try {
    $box->replace(8);
    echo "bad";
} catch (Error $e) {
    echo ":"; echo get_class($e); echo ":"; echo $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "7:Error:Cannot modify readonly property EvalPromotedReadonlyBox::$id"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies readonly eval properties can be initialized inside their constructor.
#[test]
fn execute_program_initializes_readonly_property_in_constructor() {
    let program = parse_fragment(
        br#"class EvalReadonlyBox {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
$box = new EvalReadonlyBox(7);
echo $box->id(); echo ":";
return $box->id();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "7:");
    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies direct reads of uninitialized typed eval properties throw catchable PHP errors.
#[test]
fn execute_program_rejects_uninitialized_typed_property_reads() {
    let program = parse_fragment(
        br#"class EvalTypedReadBox {
    public int $typed;
    public ?int $nullable;
    public ?int $defaultNull = null;
    public $plain;
}
$box = new EvalTypedReadBox();
try {
    echo $box->typed;
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    echo $box->nullable;
} catch (Error $e) {
    echo $e->getMessage();
}
echo "|";
echo is_null($box->defaultNull) ? "default-null" : "bad";
echo "|";
echo is_null($box->plain) ? "plain-null" : "bad";
echo "|";
$box->typed = 0;
echo $box->typed;
echo "|";
unset($box->typed);
try {
    echo $box->typed;
} catch (Error $e) {
    echo $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Typed property EvalTypedReadBox::$typed must not be accessed before initialization|\
Typed property EvalTypedReadBox::$nullable must not be accessed before initialization|\
default-null|plain-null|0|\
Typed property EvalTypedReadBox::$typed must not be accessed before initialization"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies readonly eval properties throw Error on writes outside the declaring constructor.
#[test]
fn execute_program_readonly_property_write_after_constructor_throws_error() {
    let program = parse_fragment(
        br#"class EvalReadonlyBox {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
    public function replace($id) { $this->id = $id; }
}
$box = new EvalReadonlyBox(7);
try {
    $box->replace(8);
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Cannot modify readonly property EvalReadonlyBox::$id"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies readonly eval properties must declare a type like PHP requires.
#[test]
fn execute_program_rejects_untyped_readonly_properties() {
    let explicit = parse_fragment(
        br#"class EvalReadonlyUntypedBox {
    public readonly $value;
}"#,
    )
    .expect("parse explicit readonly property");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&explicit, &mut scope, &mut values)
        .expect_err("explicit readonly property without type should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);

    let readonly_class = parse_fragment(
        br#"readonly class EvalReadonlyClassUntypedBox {
    public $value;
}"#,
    )
    .expect("parse readonly class property");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&readonly_class, &mut scope, &mut values)
        .expect_err("readonly class property without type should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies readonly classes make instance properties readonly implicitly.
#[test]
fn execute_program_initializes_readonly_class_property_in_constructor() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyClassBox {
    public int $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
$box = new EvalReadonlyClassBox(11);
echo $box->id(); echo ":";
return $box->id();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "11:");
    assert_eq!(values.get(result), FakeValue::Int(11));
}

/// Verifies readonly class instance properties throw Error on writes after construction.
#[test]
fn execute_program_readonly_class_property_write_after_constructor_throws_error() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyClassFailBox {
    public int $id;
    public function __construct($id) { $this->id = $id; }
    public function replace($id) { $this->id = $id; }
}
$box = new EvalReadonlyClassFailBox(11);
try {
    $box->replace(12);
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Cannot modify readonly property EvalReadonlyClassFailBox::$id"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies readonly classes throw Error on dynamic property creation without a magic setter.
#[test]
fn execute_program_readonly_class_dynamic_property_creation_throws_error() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyDynamicFailBox {
    public int $id;
    public function __construct($id) { $this->id = $id; }
}
$box = new EvalReadonlyDynamicFailBox(11);
try {
    $box->dynamic = 12;
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Cannot create dynamic property EvalReadonlyDynamicFailBox::$dynamic"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies readonly classes may still handle missing property writes through `__set()`.
#[test]
fn execute_program_allows_readonly_class_magic_set_for_missing_properties() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyMagicSetBox {
    public function __set($name, $value) {
        echo $name; echo ":"; echo $value;
    }
}
$box = new EvalReadonlyMagicSetBox();
$box->dynamic = 12;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "dynamic:12");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies readonly classes reject PHP's global dynamic-property marker attribute.
#[test]
fn execute_program_rejects_allow_dynamic_properties_on_readonly_class() {
    let program =
        parse_fragment(br#"#[\AllowDynamicProperties] readonly class EvalReadonlyAllowDynamic {}"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("AllowDynamicProperties cannot apply to readonly classes");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies namespaced non-builtin attributes do not trigger the readonly-class marker rule.
#[test]
fn execute_program_allows_namespaced_allow_dynamic_properties_on_readonly_class() {
    let program = parse_fragment(
        br#"namespace EvalReadonlyAttrNs;
#[AllowDynamicProperties] readonly class Box {}
echo class_attribute_names("EvalReadonlyAttrNs\Box")[0];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "EvalReadonlyAttrNs\\AllowDynamicProperties");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
