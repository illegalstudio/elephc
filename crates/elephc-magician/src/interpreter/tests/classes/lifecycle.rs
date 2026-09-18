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
use crate::context::NativeCallableShape;

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

/// Verifies PHP 8.5 clone calls run the hook before applying property overrides.
#[test]
fn execute_program_clone_function_applies_properties_after_hook() {
    let program = parse_fragment(
        br#"class EvalCloneWithBox {
    public string $name;
    public function __construct($name) { $this->name = $name; }
    public function __clone() { echo "hook:"; $this->name = "hooked"; }
}
$first = new EvalCloneWithBox("A");
$second = clone($first, ["name" => "B"]);
echo $first->name; echo ":"; echo $second->name;
return $second->name;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "hook:A:B");
    assert_eq!(values.get(result), FakeValue::String("B".to_string()));
}

/// Verifies every eval callable surface reaches the clone function implementation.
#[test]
fn execute_program_clone_function_supports_callable_surfaces() {
    let program = parse_fragment(
        br#"class EvalCloneCallableBox {
    public string $name;
    public function __construct($name) { $this->name = $name; }
}
$source = new EvalCloneCallableBox("A");
$via_cuf = call_user_func("clone", $source, ["name" => "B"]);
$callable = clone(...);
$via_fcc = $callable($source, ["name" => "C"]);
$via_array = call_user_func_array("clone", [$source, ["name" => "D"]]);
return $source->name . ":" . $via_cuf->name . ":" . $via_fcc->name . ":" . $via_array->name;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.get(result),
        FakeValue::String("A:B:C:D".to_string())
    );
}

/// Verifies clone property overrides may reinitialize readonly slots on the active clone only.
#[test]
fn execute_program_clone_function_reinitializes_readonly_clone_slot() {
    let program = parse_fragment(
        br#"class EvalCloneReadonlyBox {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
    public function copyWith($id) { return clone($this, ["id" => $id]); }
}
$source = new EvalCloneReadonlyBox(1);
$copy = $source->copyWith(2);
return $source->id . ":" . $copy->id;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("1:2".to_string()));
}

/// Verifies each readonly property has one rewrite allowance before and after the clone hook.
#[test]
fn execute_program_clone_function_scopes_readonly_reinitialization_per_property_phase() {
    let program = parse_fragment(
        br#"class EvalCloneReadonlyPhases {
    public readonly int $id;
    public int $trigger {
        set { $this->id = $value; }
    }
    public function __construct($id) { $this->id = $id; }
    public function __clone() { $this->id = 2; }
}
$source = new EvalCloneReadonlyPhases(1);
$copy = clone($source, ["id" => 3]);
echo $copy->id . ":";
try {
    clone($source, ["trigger" => 4, "id" => 5]);
    echo "bad";
} catch (Error $error) {
    echo $error->getMessage();
}
return $source->id;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "3:Cannot modify readonly property EvalCloneReadonlyPhases::$id"
    );
    assert_eq!(values.get(result), FakeValue::Int(1));
}

/// Verifies unary clone releases an owned temporary source after the shallow clone is complete.
#[test]
fn execute_program_clone_expression_releases_temporary_source() {
    let program = parse_fragment(
        br#"class EvalCloneTemporarySource {
    public string $label = "source";
    public function __clone() { $this->label = "clone"; }
    public function __destruct() { echo $this->label . "|"; }
}
$copy = clone new EvalCloneTemporarySource();
echo "after|";
unset($copy);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "source|after|clone|");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies clone overrides reject live references but accept the value after its alias is unset.
#[test]
fn execute_program_clone_function_rejects_only_shared_reference_overrides() {
    let program = parse_fragment(
        br#"class EvalCloneReferenceBox {
    public int $value = 0;
}
$source = new EvalCloneReferenceBox();
$reference = 42;
$properties = ["value" => &$reference];
try {
    clone($source, $properties);
    echo "bad";
} catch (Error $error) {
    echo $error->getMessage() . "|";
}
unset($reference);
$copy = clone($source, $properties);
return $source->value . ":" . $copy->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Cannot assign by reference when cloning with updated properties|"
    );
    assert_eq!(values.get(result), FakeValue::String("0:42".to_string()));
}

/// Verifies clone-created dynamic properties use PHP's deprecation handler and exemptions.
#[test]
fn execute_program_clone_function_dispatches_dynamic_property_deprecations() {
    let program = parse_fragment(
        br#"function eval_clone_dynamic_handler($level, $message) {
    echo $level . ":" . $message . "|";
    return true;
}
class EvalCloneDynamicPlain {}
#[AllowDynamicProperties]
class EvalCloneDynamicAllowed {}
class EvalCloneDynamicChild extends EvalCloneDynamicAllowed {}
set_error_handler("eval_clone_dynamic_handler", E_DEPRECATED);
$plain = clone(new EvalCloneDynamicPlain(), ["value" => 1]);
$allowed = clone(new EvalCloneDynamicAllowed(), ["value" => 2]);
$child = clone(new EvalCloneDynamicChild(), ["value" => 3]);
$standard = clone(new stdClass(), ["value" => 4]);
return $plain->value . ":" . $allowed->value . ":" . $child->value . ":" . $standard->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "8192:Creation of dynamic property EvalCloneDynamicPlain::$value is deprecated|"
    );
    assert_eq!(values.get(result), FakeValue::String("1:2:3:4".to_string()));
    assert!(values.warnings.is_empty());
}

/// Verifies clone property failures release the partial clone and reject mangled property names.
#[test]
fn execute_program_clone_function_cleans_up_property_failures() {
    let program = parse_fragment(
        br#"class EvalClonePropertyFailure {
    public string $label = "source";
    public int $trigger {
        set { $this->label = "failed"; throw new RuntimeException("setter"); }
    }
    public function __destruct() { echo $this->label . "|"; }
}
$source = new EvalClonePropertyFailure();
try {
    clone($source, ["trigger" => 1]);
} catch (RuntimeException $error) {
    echo $error->getMessage() . "|";
}
try {
    clone($source, ["\0EvalClonePropertyFailure\0label" => "hidden"]);
} catch (Error $error) {
    echo $error->getMessage() . "|";
}
return $source->label;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "failed|setter|source|Cannot access property starting with \"\\0\"|"
    );
    assert_eq!(values.get(result), FakeValue::String("source".to_string()));
}

/// Verifies a clone whose hook throws is released before the original error is caught.
#[test]
fn execute_program_clone_function_releases_clone_when_hook_throws() {
    let program = parse_fragment(
        br#"class EvalCloneThrowBox {
    public string $name;
    public function __construct($name) { $this->name = $name; }
    public function __clone() { $this->name = "clone"; throw new RuntimeException("boom"); }
    public function __destruct() { echo $this->name . ":"; }
}
$source = new EvalCloneThrowBox("source");
try {
    clone($source);
} catch (RuntimeException $error) {
    echo $error->getMessage() . ":";
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "clone:boom:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies native clone dispatch keeps the declaring class as the generated frame scope.
#[test]
fn execute_program_forwards_aot_clone_declaring_and_called_class_scopes() {
    let program = parse_fragment(
        br#"$box = new KnownClonePublic();
return clone $box;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values)
        .expect("native clone hook should run");

    assert!(matches!(values.get(result), FakeValue::Object(_)));
    assert_eq!(
        values.native_clone_calls,
        vec![(
            "KnownClonePublic".to_string(),
            Some("KnownClonePublic".to_string()),
        )]
    );
}

/// Verifies native clone dispatch materializes a physical hidden collector before bridging.
#[test]
fn execute_program_materializes_aot_clone_hidden_collector() {
    let program = parse_fragment(
        br#"$box = new KnownClonePublic();
return clone $box;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(1);
    assert!(signature.set_param_name(0, ""));
    assert!(signature.set_variadic_index(0));
    signature.set_shape(NativeCallableShape::new(0, 0, false, false));
    assert!(context.define_native_method_signature(
        "KnownClonePublic",
        "__clone",
        signature,
    ));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("native clone hook should receive its hidden collector");

    assert!(matches!(values.get(result), FakeValue::Object(_)));
    assert_eq!(values.native_clone_arg_shapes, vec![(1, Some(0))]);
    assert_eq!(
        values.native_clone_calls,
        vec![(
            "KnownClonePublic".to_string(),
            Some("KnownClonePublic".to_string()),
        )]
    );
}

/// Verifies a public native clone hook inherited by an eval subclass dispatches as that subclass.
#[test]
fn execute_program_forwards_public_native_clone_hook_for_eval_subclass() {
    let program = parse_fragment(
        br#"class EvalCloneNativePublicChild extends KnownClonePublic {}
$box = new EvalCloneNativePublicChild();
return clone $box;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values)
        .expect("public inherited native clone hook should run");

    assert!(matches!(values.get(result), FakeValue::Object(_)));
    assert_eq!(
        values.native_clone_calls,
        vec![(
            "KnownClonePublic".to_string(),
            Some("EvalCloneNativePublicChild".to_string()),
        )]
    );
}

/// Verifies an inherited protected native clone hook keeps the eval subclass as called class.
#[test]
fn execute_program_forwards_eval_subclass_for_inherited_native_clone_hook() {
    let program = parse_fragment(
        br#"class EvalCloneNativeProtectedChild extends KnownCloneProtected {
    public function copy() { return clone $this; }
}
$box = new EvalCloneNativeProtectedChild();
return $box->copy();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values)
        .expect("protected native clone hook should run from child scope");

    assert!(matches!(values.get(result), FakeValue::Object(_)));
    assert_eq!(
        values.native_clone_calls,
        vec![(
            "KnownCloneProtected".to_string(),
            Some("EvalCloneNativeProtectedChild".to_string()),
        )]
    );
}

/// Verifies a private native parent clone hook remains inaccessible to an eval subclass.
#[test]
fn execute_program_rejects_private_native_clone_hook_from_eval_subclass() {
    let program = parse_fragment(
        br#"class EvalCloneNativePrivateChild extends KnownClonePrivate {
    public function copy() { return clone $this; }
}
$box = new EvalCloneNativePrivateChild();
try {
    $box->copy();
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values)
        .expect("private native clone hook should throw a catchable Error");

    assert_eq!(
        values.output,
        "Error:Call to private KnownClonePrivate::__clone() from scope EvalCloneNativePrivateChild"
    );
    assert!(values.native_clone_calls.is_empty());
    assert_eq!(values.get(result), FakeValue::Bool(true));
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
