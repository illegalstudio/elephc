//! Purpose:
//! Interpreter tests for eval-declared class runtime behavior.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases cover class property semantics that need eval runtime state.

use super::super::*;
use super::support::*;

/// Verifies promoted constructor properties initialize before the constructor body runs.
#[test]
fn execute_program_initializes_constructor_promoted_properties() {
    let program = parse_fragment(
        br#"class EvalPromotedUser {
    public function __construct(public int $id, private string $name = "Ada") {
        $this->id = $this->id + 1;
    }
    public function label() { return $this->id . ":" . $this->name; }
}
$user = new EvalPromotedUser(6);
echo $user->id; echo ":";
return $user->label();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "7:");
    assert_eq!(values.get(result), FakeValue::String("7:Ada".to_string()));
}

/// Verifies promoted readonly properties keep the normal constructor-only write rule.
#[test]
fn execute_program_rejects_promoted_readonly_property_write_after_constructor() {
    let program = parse_fragment(
        br#"class EvalPromotedReadonlyBox {
    public function __construct(public readonly int $id) {}
    public function replace($id) { $this->id = $id; }
}
$box = new EvalPromotedReadonlyBox(7);
echo $box->id;
$box->replace(8);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("promoted readonly property write should fail outside constructor");

    assert_eq!(err, EvalStatus::RuntimeFatal);
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

/// Verifies readonly eval properties reject writes outside the declaring constructor.
#[test]
fn execute_program_rejects_readonly_property_write_after_constructor() {
    let program = parse_fragment(
        br#"class EvalReadonlyBox {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
    public function replace($id) { $this->id = $id; }
}
$box = new EvalReadonlyBox(7);
$box->replace(8);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly property write should fail outside constructor");

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

/// Verifies readonly class instance properties reject writes after construction.
#[test]
fn execute_program_rejects_readonly_class_property_write_after_constructor() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyClassFailBox {
    public int $id;
    public function __construct($id) { $this->id = $id; }
    public function replace($id) { $this->id = $id; }
}
$box = new EvalReadonlyClassFailBox(11);
$box->replace(12);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly class property write should fail outside constructor");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies readonly class static properties remain mutable.
#[test]
fn execute_program_allows_readonly_class_static_property_mutation() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyStaticBox {
    public static int $count = 1;
}
EvalReadonlyStaticBox::$count = 5;
echo EvalReadonlyStaticBox::$count; echo ":";
EvalReadonlyStaticBox::$count = EvalReadonlyStaticBox::$count + 1;
return EvalReadonlyStaticBox::$count;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5:");
    assert_eq!(values.get(result), FakeValue::Int(6));
}

/// Verifies readonly classes may extend readonly parents and use inherited constructors.
#[test]
fn execute_program_allows_readonly_class_extending_readonly_parent() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyParentBase {
    public int $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
readonly class EvalReadonlyParentChild extends EvalReadonlyParentBase {}
$box = new EvalReadonlyParentChild(13);
echo $box->id(); echo ":";
return $box->id();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "13:");
    assert_eq!(values.get(result), FakeValue::Int(13));
}

/// Verifies readonly class inheritance requires matching readonly status.
#[test]
fn execute_program_rejects_readonly_class_extending_non_readonly_parent() {
    let program = parse_fragment(
        br#"class EvalReadonlyParentMismatchBase {}
readonly class EvalReadonlyParentMismatchChild extends EvalReadonlyParentMismatchBase {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly class cannot extend non-readonly parent");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies anonymous eval classes instantiate, reuse their synthetic class, and reflect as anonymous.
#[test]
fn execute_program_instantiates_anonymous_class_expressions() {
    let program = parse_fragment(
        br#"interface EvalAnonRuntimeLabel {
    function label();
}
class EvalAnonRuntimeBase {
    protected string $prefix;
    public function __construct($prefix) { $this->prefix = $prefix; }
}
function eval_anon_make($prefix) {
    return new class($prefix) extends EvalAnonRuntimeBase implements EvalAnonRuntimeLabel {
        public function label() { return $this->prefix . ":anon"; }
    };
}
$first = eval_anon_make("A");
$second = eval_anon_make("B");
echo $first->label(); echo ":";
echo $second->label(); echo ":";
echo get_class($first) === get_class($second) ? "same" : "different"; echo ":";
$ref = new ReflectionClass(get_class($first));
echo $ref->isAnonymous() ? "anonymous" : "named"; echo ":";
echo $ref->implementsInterface("EvalAnonRuntimeLabel") ? "iface" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A:anon:B:anon:same:anonymous:iface");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies a get-only property hook computes a virtual eval property.
#[test]
fn execute_program_reads_eval_property_get_hook() {
    let program = parse_fragment(
        br#"class EvalHookPerson {
    public string $first = "Ada";
    public string $last = "Lovelace";
    public string $full {
        get => $this->first . " " . $this->last;
    }
}
$person = new EvalHookPerson();
echo $person->full;
return $person->full;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada Lovelace");
    assert_eq!(
        values.get(result),
        FakeValue::String("Ada Lovelace".to_string())
    );
}

/// Verifies get/set property hooks can use the raw backing slot from inside accessors.
#[test]
fn execute_program_routes_eval_property_get_and_set_hooks() {
    let program = parse_fragment(
        br#"class EvalHookName {
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
$name = new EvalHookName();
$name->value = "Ada";
echo $name->value;
return $name->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada!");
    assert_eq!(values.get(result), FakeValue::String("Ada!".to_string()));
}

/// Verifies get-only property hooks reject writes outside a set accessor.
#[test]
fn execute_program_rejects_write_to_get_only_eval_property_hook() {
    let program = parse_fragment(
        br#"class EvalHookReadOnly {
    public int $answer {
        get => 42;
    }
}
$box = new EvalHookReadOnly();
$box->answer = 7;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("get-only property hook write should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval subclasses inherit parent property hooks.
#[test]
fn execute_program_inherits_eval_property_hooks() {
    let program = parse_fragment(
        br#"class EvalHookBase {
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
class EvalHookChild extends EvalHookBase {
    public function shout() { return $this->value . "?"; }
}
$box = new EvalHookChild();
$box->value = "Ada";
echo $box->value; echo ":";
return $box->shout();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada!:");
    assert_eq!(values.get(result), FakeValue::String("Ada!?".to_string()));
}

/// Verifies eval interface property hook contracts are enforced through inheritance.
#[test]
fn execute_program_accepts_interface_property_hook_contracts() {
    let program = parse_fragment(
        br#"interface EvalHookContract {
    public string $value { get; set; }
}
interface EvalNamedHookContract extends EvalHookContract {
    public string $name { get; }
}
class EvalHookContractBox implements EvalNamedHookContract {
    public string $name = "box";
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
$box = new EvalHookContractBox();
$box->value = "Ada";
echo $box->name; echo ":";
echo $box->value;
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "box:Ada!");
    assert_eq!(values.get(result), FakeValue::String("Ada!".to_string()));
}

/// Verifies a normal public mutable property satisfies an eval interface get/set contract.
#[test]
fn execute_program_accepts_plain_property_for_interface_hook_contracts() {
    let program = parse_fragment(
        br#"interface EvalPlainHookContract {
    public string $value { get; set; }
}
class EvalPlainHookContractBox implements EvalPlainHookContract {
    public string $value = "Ada";
}
$box = new EvalPlainHookContractBox();
echo $box->value; echo ":";
$box->value = "Grace";
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada:");
    assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
}

/// Verifies a get-only hook cannot satisfy a writable eval interface contract.
#[test]
fn execute_program_rejects_get_only_hook_for_interface_set_contract() {
    let program = parse_fragment(
        br#"interface EvalHookSetContract {
    public int $answer { get; set; }
}
class EvalHookGetOnlyContractBox implements EvalHookSetContract {
    public int $answer {
        get => 42;
    }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("get-only hook should fail writable interface contract");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies readonly properties cannot satisfy writable eval interface contracts.
#[test]
fn execute_program_rejects_readonly_property_for_interface_set_contract() {
    let program = parse_fragment(
        br#"interface EvalReadonlyHookContract {
    public int $id { get; set; }
}
class EvalReadonlyHookContractBox implements EvalReadonlyHookContract {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly property should fail writable interface contract");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies concrete eval subclasses satisfy abstract property hook contracts.
#[test]
fn execute_program_accepts_abstract_property_hook_contracts() {
    let program = parse_fragment(
        br#"abstract class EvalAbstractHookBase {
    abstract public string $value { get; set; }
}
class EvalAbstractHookBox extends EvalAbstractHookBase {
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
$box = new EvalAbstractHookBox();
$box->value = "Ada";
echo $box->value;
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada!");
    assert_eq!(values.get(result), FakeValue::String("Ada!".to_string()));
}

/// Verifies normal mutable properties satisfy abstract get/set hook contracts.
#[test]
fn execute_program_accepts_plain_property_for_abstract_hook_contracts() {
    let program = parse_fragment(
        br#"abstract class EvalPlainAbstractHookBase {
    abstract public string $value { get; set; }
}
class EvalPlainAbstractHookBox extends EvalPlainAbstractHookBase {
    public string $value = "Ada";
}
$box = new EvalPlainAbstractHookBox();
echo $box->value; echo ":";
$box->value = "Grace";
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada:");
    assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
}

/// Verifies concrete eval subclasses must declare inherited abstract properties.
#[test]
fn execute_program_rejects_missing_abstract_property_hook_contract() {
    let program = parse_fragment(
        br#"abstract class EvalMissingAbstractHookBase {
    abstract public string $value { get; }
}
class EvalMissingAbstractHookBox extends EvalMissingAbstractHookBase {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("missing abstract property should fail concrete subclass");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies abstract final eval properties are rejected while parsing.
#[test]
fn parse_fragment_rejects_final_abstract_property_hook_contract() {
    let err = parse_fragment(
        br#"abstract class EvalFinalAbstractHookBase {
    abstract final public string $value { get; }
}"#,
    )
    .expect_err("final abstract property should fail");

    assert_eq!(err, EvalParseError::UnsupportedConstruct);
}

/// Verifies readonly properties cannot satisfy abstract writable hook contracts.
#[test]
fn execute_program_rejects_readonly_property_for_abstract_set_contract() {
    let program = parse_fragment(
        br#"abstract class EvalReadonlyAbstractHookBase {
    abstract public int $id { get; set; }
}
class EvalReadonlyAbstractHookBox extends EvalReadonlyAbstractHookBase {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly property should fail abstract writable contract");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies abstract trait property hook contracts are enforced after trait expansion.
#[test]
fn execute_program_enforces_trait_abstract_property_hook_contracts() {
    let program = parse_fragment(
        br#"trait EvalTraitNeedsName {
    abstract protected string $name { get; }
    public function label() { return $this->name; }
}
class EvalTraitNameBox {
    use EvalTraitNeedsName;
    protected string $name = "Ada";
}
$box = new EvalTraitNameBox();
echo $box->label();
return $box->label();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada");
    assert_eq!(values.get(result), FakeValue::String("Ada".to_string()));
}
