//! Purpose:
//! Interpreter tests for eval property get/set hooks and their type rules.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Tests cover by-reference getters, short setters, mixed-case access, and
//!   parameter validation.

use super::super::super::*;
use super::super::support::*;

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

/// Verifies by-reference get hook syntax routes through the concrete eval get accessor.
#[test]
fn execute_program_reads_eval_by_ref_get_property_hook() {
    let program = parse_fragment(
        br#"class EvalByRefGetHookPerson {
    public string $first = "Ada";
    public string $last = "Lovelace";
    public string $full {
        &get => $this->first . " " . $this->last;
    }
}
$person = new EvalByRefGetHookPerson();
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

/// Verifies short set hooks assign their expression result into the raw backing slot.
#[test]
fn execute_program_routes_eval_short_set_property_hooks() {
    let program = parse_fragment(
        br#"class EvalShortSetHookName {
    public string $value {
        get => $this->value;
        set => trim($value);
    }
}
class EvalShortSetHookLabel {
    public string $text {
        get => $this->text;
        set(string $raw) => strtoupper($raw);
    }
}
$name = new EvalShortSetHookName();
$name->value = "  Ada  ";
echo "[" . $name->value . "]:";
$label = new EvalShortSetHookLabel();
$label->text = "hi";
echo $label->text;
return $label->text;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "[Ada]:HI");
    assert_eq!(values.get(result), FakeValue::String("HI".to_string()));
}

/// Verifies explicit set-hook parameter types are contravariant with the property type.
#[test]
fn execute_program_validates_eval_property_set_hook_parameter_types() {
    let valid_program = parse_fragment(
        br#"class EvalWideSetHookParam {
    public string $value {
        get => $this->value;
        set(mixed $raw) => $raw;
    }
}
$box = new EvalWideSetHookParam();
$box->value = "Ada";
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&valid_program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("Ada".to_string()));

    for source in [
        br#"class EvalNarrowSetHookParam {
    public mixed $value {
        set(string $raw) => $raw;
    }
}"#
        .as_slice(),
        br#"class EvalNullableSetHookParam {
    public ?string $value {
        set(string $raw) => $raw;
    }
}"#
        .as_slice(),
    ] {
        let program = parse_fragment(source).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let err = execute_program(&program, &mut scope, &mut values)
            .expect_err("incompatible set-hook parameter type should fail");
        assert_eq!(err, EvalStatus::RuntimeFatal);
    }
}

/// Verifies nullsafe reads and mixed-case names still route through eval property hooks.
#[test]
fn execute_program_routes_eval_nullsafe_and_mixed_case_property_hooks() {
    let program = parse_fragment(
        br#"class EvalNullsafeHookPerson {
    public string $first = "Ada";
    public string $last = "Lovelace";
    public string $full {
        get => $this->first . " " . $this->last;
    }
}
class EvalMixedCaseHookBox {
    private int $store = 0;
    public int $Total {
        get { return $this->store; }
    }
    public function set(int $value) { $this->store = $value; }
}
function eval_hook_describe($person) {
    return $person?->full ?? "(none)";
}
$person = new EvalNullsafeHookPerson();
$box = new EvalMixedCaseHookBox();
$box->set(5);
echo eval_hook_describe($person) . "|" . eval_hook_describe(null) . "|" . $box->Total;
return $box->Total;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada Lovelace|(none)|5");
    assert_eq!(values.get(result), FakeValue::Int(5));
}
