//! Purpose:
//! Interpreter tests for anonymous classes, cloning, clone visibility, and
//! destructor execution.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Lifecycle hooks are checked at their PHP-visible invocation boundaries.

use super::super::super::*;
use super::super::support::*;

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

/// Verifies readonly anonymous eval classes initialize and reject property writes.
#[test]
fn execute_program_instantiates_readonly_anonymous_class_expressions() {
    let program = parse_fragment(
        br#"$box = new readonly class("frozen") {
    public function __construct(public string $label) {}
};
echo $box->label; echo ":";
try {
    $box->label = "bad";
    echo "bad";
} catch (Error $e) {
    echo get_class($e);
}
return $box->label;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "frozen:Error");
    assert_eq!(values.get(result), FakeValue::String("frozen".to_string()));
}

/// Verifies eval object cloning copies properties before running `__clone()`.
#[test]
fn execute_program_clones_eval_object_and_runs_clone_hook() {
    let program = parse_fragment(
        br#"class EvalCloneRuntimeBox {
    public string $name;
    public function __construct($name) { $this->name = $name; }
    public function __clone() { $this->name = $this->name . ":clone"; }
}
$first = new EvalCloneRuntimeBox("A");
$second = clone $first;
echo $first->name; echo ":";
echo $second->name;
$second->name = "B";
return $first->name . ":" . $second->name;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A:A:clone");
    assert_eq!(values.get(result), FakeValue::String("A:B".to_string()));
}

/// Verifies private `__clone()` can be invoked from inside the declaring eval class.
#[test]
fn execute_program_allows_private_clone_hook_inside_declaring_class() {
    let program = parse_fragment(
        br#"class EvalCloneRuntimePrivateBox {
    public string $name = "A";
    private function __clone() { $this->name = $this->name . ":copy"; }
    public function copy() { return clone $this; }
}
$first = new EvalCloneRuntimePrivateBox();
$second = $first->copy();
echo $first->name; echo ":";
echo $second->name;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A:A:copy");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval-declared `__destruct()` runs for explicit unset and discarded temporaries.
#[test]
fn execute_program_runs_eval_destructor_on_final_release() {
    let program = parse_fragment(
        br#"class EvalDestructRuntimeBox {
    public string $name;
    public function __construct($name) { $this->name = $name; }
    public function __destruct() { echo "drop:" . $this->name . ":"; }
}
$box = new EvalDestructRuntimeBox("A");
unset($box);
new EvalDestructRuntimeBox("B");
echo "after";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "drop:A:drop:B:after");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies private `__clone()` throws Error through a global clone expression.
#[test]
fn execute_program_private_clone_hook_outside_declaring_class_throws_error() {
    let program = parse_fragment(
        br#"class EvalCloneRuntimePrivateFail {
    private function __clone() {}
}
$box = new EvalCloneRuntimePrivateFail();
try {
    clone $box;
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
        "Error:Call to private EvalCloneRuntimePrivateFail::__clone() from global scope"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
