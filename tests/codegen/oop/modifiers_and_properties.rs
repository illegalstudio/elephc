//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP class modifiers and properties, including readonly class constructor initialization, final class instantiates and dispatches methods, and final method dispatches normally without override.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

/// Exercises the retained abstract setter through native, eval and native-parent eval classes.
fn check_asymmetric_eighth_review_abstract_setter_storage(native_parent: Option<bool>) {
    let parent = "abstract class Base { abstract public int $x { get; set; } }";
    let child = r#"abstract class Child extends Base {
    public public(set) readonly int $x;
    public function write(): int { echo "R"; $this->x = 4; return 4; }
    public function invalid(mixed $value): void {
        try { $this->x = $value; echo "bad"; } catch (TypeError $error) { echo $error->getMessage(); }
    }
    public function probe(): void {
        echo $this->write(), ":", isset($this->x) ? "isset" : "unset", ":";
        try { echo $this->x; } catch (Error $error) { echo $error->getMessage(); }
        echo "|", $this->write(), ":", isset($this->x) ? "isset" : "unset";
    }
}
class Grandchild extends Child {}
(new Grandchild())->probe();
echo "|";
(new Grandchild())->invalid("bad");
foreach ((new ReflectionProperty(Grandchild::class, "x"))->getHooks() as $name => $hook) {
    echo "|", $name, ":", $hook->isAbstract() ? "A" : "C", ":", $hook->getDeclaringClass()->getName();
}"#;
    let source = match native_parent {
        None => format!("<?php {parent} {child}"),
        Some(false) => format!("<?php eval('{parent} {child}');"),
        Some(true) => format!("<?php {parent} echo (new ReflectionClass(Base::class))->isAbstract() ? '' : 'bad'; eval('{child}');"),
    };
    assert_eq!(compile_and_run(&source), "R4:unset:Typed property Child::$x must not be accessed before initialization|R4:unset|Base::$x::set(): Argument #1 ($value) must be of type int, string given|set:A:Base");
}

/// Native storage retains a no-op abstract setter instead of initializing the readonly slot.
#[test]
fn test_asymmetric_eighth_review_abstract_setter_native() {
    check_asymmetric_eighth_review_abstract_setter_storage(None);
}

/// Eval storage preserves the original abstract setter and its reflection owner.
#[test]
fn test_asymmetric_eighth_review_abstract_setter_eval() {
    check_asymmetric_eighth_review_abstract_setter_storage(Some(false));
}

/// Native parent metadata keeps an abstract setter when an eval child introduces storage.
#[test]
fn test_asymmetric_eighth_review_abstract_setter_native_parent() {
    check_asymmetric_eighth_review_abstract_setter_storage(Some(true));
}

/// Native abstract setter dispatch is independent of EIR optimization.
#[test]
fn test_asymmetric_eighth_review_abstract_setter_native_no_ir_opt() {
    crate::support::without_ir_opt(|| check_asymmetric_eighth_review_abstract_setter_storage(None));
}

/// Eval abstract setter dispatch is independent of EIR optimization.
#[test]
fn test_asymmetric_eighth_review_abstract_setter_eval_no_ir_opt() {
    crate::support::without_ir_opt(|| check_asymmetric_eighth_review_abstract_setter_storage(Some(false)));
}

/// Native-parent eval abstract setters preserve metadata without EIR optimization.
#[test]
fn test_asymmetric_eighth_review_abstract_setter_native_parent_no_ir_opt() {
    crate::support::without_ir_opt(|| check_asymmetric_eighth_review_abstract_setter_storage(Some(true)));
}

/// Named and runtime-name boxed receiver writes retire unused string setter arguments.
#[test]
fn test_asymmetric_eighth_review_abstract_setter_argument_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
abstract class Base { abstract public string $x { set; } }
abstract class Child extends Base { public public(set) readonly string $x; }
class Grandchild extends Child {}
function named(mixed $object, mixed $value): void { $object->x = $value; }
function dynamic(mixed $object, mixed $value, string $name): void { $object->{$name} = $value; }
for ($i = 0; $i < 16; $i++) {
    $object = new Grandchild(); $value = str_repeat('x', 2);
    named($object, $value); dynamic($object, $value, 'x');
    echo isset($object->x) ? 'bad' : 'U';
    unset($object, $value);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "U".repeat(16), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Abstract setter argument retirement is also balanced without IR optimization.
#[test]
fn test_asymmetric_eighth_review_abstract_setter_argument_owners_no_ir_opt() {
    crate::support::without_ir_opt(test_asymmetric_eighth_review_abstract_setter_argument_owners);
}

/// A broader storage type neither broadens the setter nor leaks rejected fresh arguments.
#[test]
fn test_asymmetric_eighth_review_abstract_setter_union_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
abstract class Base { abstract public int $x { set; } }
abstract class Child extends Base {
    public public(set) readonly int|string $x;
    public function probe(): void {
        try { $this->x = str_repeat('b', 2); echo 'bad'; }
        catch (TypeError $error) { echo $error->getMessage(), '|'; }
        $this->x = 4;
        echo isset($this->x) ? 'bad' : 'U';
    }
}
class Grandchild extends Child {}
for ($i = 0; $i < 8; $i++) { $object = new Grandchild(); $object->probe(); unset($object); }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Base::$x::set(): Argument #1 ($value) must be of type int, string given|U".repeat(8), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Broader readonly storage keeps balanced setter rejection with IR optimization disabled.
#[test]
fn test_asymmetric_eighth_review_abstract_setter_union_owners_no_ir_opt() {
    crate::support::without_ir_opt(test_asymmetric_eighth_review_abstract_setter_union_owners);
}

/// An abstract child may add readonly storage while its inherited setter stays abstract.
#[test]
fn test_asymmetric_latest_review_abstract_set_readonly_storage() {
    assert_eq!(compile_and_run(r#"<?php
abstract class Base { abstract public int $x { get; set; } }
abstract class Child extends Base { public public(set) readonly int $x; }
class Grandchild extends Child {}
echo (new ReflectionClass(Child::class))->isAbstract() ? 'Y' : 'bad';
echo (new ReflectionClass(Grandchild::class))->isAbstract() ? 'bad' : 'N';
"#), "YN");
}

/// Checks the readonly exception against eval or generated abstract getter/setter metadata.
fn check_asymmetric_latest_review_abstract_set_readonly(native_parent: bool) {
    let parent = "abstract class Base { abstract public int $x { get; set; } }";
    let child = r#"abstract class Child extends Base { public public(set) readonly int $x; }
class Grandchild extends Child {}
echo (new ReflectionClass(Child::class))->isAbstract() ? "Y" : "bad";
echo (new ReflectionClass(Grandchild::class))->isAbstract() ? "bad" : "N";"#;
    let source = if native_parent {
        format!("<?php {parent} echo (new ReflectionClass(Base::class))->isAbstract() ? '' : 'bad'; eval('{child}');")
    } else {
        format!("<?php eval('{parent} {child}');")
    };
    assert_eq!(compile_and_run(&source), "YN");
}

/// Eval-defined abstract getter/setter parents permit readonly storage on an abstract child.
#[test]
fn test_asymmetric_latest_review_abstract_set_readonly_eval_parent() {
    check_asymmetric_latest_review_abstract_set_readonly(false);
}

/// Native abstract getter/setter metadata grants the same readonly exception to eval.
#[test]
fn test_asymmetric_latest_review_abstract_set_readonly_native_parent() {
    check_asymmetric_latest_review_abstract_set_readonly(true);
}

/// An abstract readonly child cannot narrow the native abstract parent's public setter.
#[test]
fn test_asymmetric_latest_review_native_abstract_setter_narrowing() {
    let out = compile_and_run_capture(r#"<?php
abstract class Base { abstract public int $x { get; set; } }
echo (new ReflectionClass(Base::class))->isAbstract() ? 'before' : 'bad';
eval('abstract class Child extends Base { public readonly int $x; } echo "bad";');
"#);
    assert!(!out.success, "{}", out.stdout);
    assert_eq!(out.stdout, "before", "{}", out.stderr);
    assert!(out.stderr.contains("Fatal error: eval() runtime failed"), "{}", out.stderr);
}

/// Exercises one readonly addition with one compilation per independently scheduled case.
fn check_asymmetric_fifth_review_eval_readonly_addition(parent: &str, child: &str, native_parent: bool) {
    let parent = format!("class Base {{ {parent} }}");
    let source = if native_parent {
        format!("<?php {parent} $base = new Base(); echo 'before'; eval('class Child extends Base {{ {child} }} echo \"bad\";');")
    } else {
        format!("<?php echo 'before'; eval('{parent} class Child extends Base {{ {child} }} echo \"bad\";');")
    };
    let out = compile_and_run_capture(&source);
    assert!(!out.success, "accepted readonly addition: {source}");
    assert_eq!(out.stdout, "before", "{source}");
    assert!(out.stderr.contains("Fatal error: eval() runtime failed"), "{}", out.stderr);
}

/// Public readonly additions fail for eval parents.
#[test]
fn test_asymmetric_fifth_review_eval_cannot_add_public_readonly() {
    check_asymmetric_fifth_review_eval_readonly_addition("public int $x;", "public readonly int $x;", false);
}

/// Public readonly additions fail for generated parent metadata.
#[test]
fn test_asymmetric_fifth_review_native_cannot_add_public_readonly() {
    check_asymmetric_fifth_review_eval_readonly_addition("public int $x;", "public readonly int $x;", true);
}

/// Protected readonly additions cannot bypass the invariant through matching setter access.
#[test]
fn test_asymmetric_fifth_review_eval_cannot_add_protected_readonly() {
    check_asymmetric_fifth_review_eval_readonly_addition("protected int $x;", "protected readonly int $x;", false);
}

/// Generated protected parents retain the same readonly invariant in eval.
#[test]
fn test_asymmetric_fifth_review_native_cannot_add_protected_readonly() {
    check_asymmetric_fifth_review_eval_readonly_addition("protected int $x;", "protected readonly int $x;", true);
}

/// Explicit public(set) does not make a readonly addition legal.
#[test]
fn test_asymmetric_fifth_review_eval_cannot_add_public_set_readonly() {
    check_asymmetric_fifth_review_eval_readonly_addition("public int $x;", "public public(set) readonly int $x;", false);
}

/// Explicit public(set) cannot change readonly status inherited from a generated parent.
#[test]
fn test_asymmetric_fifth_review_native_cannot_add_public_set_readonly() {
    check_asymmetric_fifth_review_eval_readonly_addition("public int $x;", "public public(set) readonly int $x;", true);
}

/// Constructor promotion obeys the same readonly invariant as declared properties.
#[test]
fn test_asymmetric_fifth_review_eval_cannot_add_promoted_readonly() {
    check_asymmetric_fifth_review_eval_readonly_addition("public int $x;", "public function __construct(public public(set) readonly int $x) {}", false);
}

/// Constructor promotion cannot change readonly status inherited from a generated parent.
#[test]
fn test_asymmetric_fifth_review_native_cannot_add_promoted_readonly() {
    check_asymmetric_fifth_review_eval_readonly_addition("public int $x;", "public function __construct(public public(set) readonly int $x) {}", true);
}

/// A concrete virtual getter remains a concrete property for readonly invariance in eval.
#[test]
fn test_asymmetric_ci_eval_virtual_getter_cannot_add_readonly() {
    check_asymmetric_fifth_review_eval_readonly_addition("public int $x { get => 1; }", "public readonly int $x;", false);
}

/// Generated virtual getter metadata does not grant the abstract-property exception.
#[test]
fn test_asymmetric_ci_native_virtual_getter_cannot_add_readonly() {
    check_asymmetric_fifth_review_eval_readonly_addition("public int $x { get => 1; }", "public readonly int $x;", true);
}

/// Checks declared and promoted readonly implementations of one abstract getter contract.
fn check_asymmetric_ci_abstract_getter_readonly(native_parent: bool) {
    let parent = "abstract class Base { abstract public int $value { get; } }";
    let child = r#"class Child extends Base {
    public readonly int $value;
    public function __construct(int $value) { $this->value = $value; }
}
class Promoted extends Base { public function __construct(public readonly int $value) {} }
echo (new Child(42))->value, ":", (new Promoted(7))->value;"#;
    let source = if native_parent {
        format!("<?php {parent} echo (new ReflectionClass(Base::class))->isAbstract() ? '' : 'bad'; eval('{child}');")
    } else {
        format!("<?php eval('{parent} {child}');")
    };
    assert_eq!(compile_and_run(&source), "42:7");
}

/// Eval-defined abstract getter contracts accept readonly backed implementations.
#[test]
fn test_asymmetric_ci_abstract_getter_readonly_eval_parent() {
    check_asymmetric_ci_abstract_getter_readonly(false);
}

/// Generated abstract getter contracts accept readonly eval implementations.
#[test]
fn test_asymmetric_ci_abstract_getter_readonly_native_parent() {
    check_asymmetric_ci_abstract_getter_readonly(true);
}

/// Private-set readonly clone refusals omit the readonly word and retain the source value.
#[test]
fn test_asymmetric_second_review_private_readonly_clone_message() {
    for eval in [false, true] {
        let body = r#"class V {
    public private(set) readonly int $ro;
    public function __construct() { $this->ro = 1; }
}
$source = new V();
try { clone($source, ["ro" => 9]); }
catch (Error $error) { echo $error->getMessage(), "|", $source->ro; }
unset($source);"#;
        let source = if eval { format!("<?php eval('{body}');") }
            else { format!("<?php {body}") };
        let out = compile_and_run_with_heap_debug(&source);
        assert!(out.success, "{}", out.stderr);
        assert_eq!(out.stdout, "Cannot modify private(set) property V::$ro from global scope|1");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// Legal eval child readonly redeclarations keep initialization and empty slots distinct.
#[test]
fn test_asymmetric_followup_eval_redeclared_readonly_initialization() {
    let out = compile_and_run_with_heap_debug(r#"<?php
eval('class ParentBox { public public(set) readonly int $id; }
class ChildBox extends ParentBox {
    public public(set) readonly int $id;
    public function __construct() { $this->id = 7; }
}
class UninitializedBox extends ParentBox { public public(set) readonly int $id; }
$box = new ChildBox();
$empty = new UninitializedBox();
echo $box->id, ":", isset($empty->id) ? "bad" : "empty";
unset($empty, $box);');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "7:empty");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eval reports initialized readonly modification before its implicit protected setter.
#[test]
fn test_asymmetric_followup_eval_readonly_error_precedence() {
    let out = compile_and_run_with_heap_debug(r#"<?php
eval('class EvalBox {
    public readonly int $id;
    public function __construct() { $this->id = 1; }
}
$box = new EvalBox();
try { $box->id = 2; echo "bad"; }
catch (Error $error) { echo $error->getMessage(), ":", $box->id; }');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Cannot modify readonly property EvalBox::$id:1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A failed global clone override cannot bypass readonly set access or corrupt its source.
#[test]
fn test_asymmetric_review_eval_clone_readonly_set_access() {
    let out = compile_and_run_with_heap_debug(r#"<?php
eval('class EvalCloneRestrictedReadonly {
    public readonly int $id;
    public function __construct() { $this->id = 1; }
}
$source = new EvalCloneRestrictedReadonly();
try { clone($source, ["id" => 3]); echo "bad"; }
catch (Error $error) { echo "blocked:", $source->id; }');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "blocked:1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Private(set) contributes implicit finality only when it restricts a wider read visibility.
#[test]
fn test_asymmetric_review_private_setter_reflection_finality() {
    let out = compile_and_run(r#"<?php
class NativePrivateSetter {
    public private(set) int $public = 1;
    protected private(set) int $protected = 2;
    private private(set) int $private = 3;
    public function __construct(public private(set) int $promoted = 4) {}
}
echo (new ReflectionProperty(NativePrivateSetter::class, 'public'))->isFinal() ? '1' : '0';
echo (new ReflectionProperty(NativePrivateSetter::class, 'protected'))->isFinal() ? '1' : '0';
echo (new ReflectionProperty(NativePrivateSetter::class, 'private'))->isFinal() ? '1' : '0';
echo (new ReflectionProperty(NativePrivateSetter::class, 'promoted'))->isFinal() ? '1' : '0';
eval('class EvalPrivateSetter {
    public private(set) int $public = 1;
    protected private(set) int $protected = 2;
    private private(set) int $private = 3;
    public function __construct(public private(set) int $promoted = 4) {}
}
foreach (["public", "protected", "private", "promoted"] as $name) {
    $property = new ReflectionProperty("EvalPrivateSetter", $name);
    echo $property->isFinal() ? "1" : "0";
}');
"#);
    assert_eq!(out, "11011101");
}

/// An eval child may add a restricted setter to a generated getter-only virtual property.
#[test]
fn test_asymmetric_review_eval_child_of_native_virtual_getter() {
    let out = compile_and_run(r#"<?php
class NativeGetterOnly { public int $x { get => 1; } }
$base = new NativeGetterOnly();
echo $base->x, ':';
eval('class EvalWritableChild extends NativeGetterOnly {
    public protected(set) int $x { get => 2; set { } }
}
echo (new EvalWritableChild())->x;');
"#);
    assert_eq!(out, "1:2");
}

/// Skipping ordinary accessor-method variance does not bypass the native property's type contract.
#[test]
fn test_asymmetric_review_eval_child_rejects_changed_native_virtual_type() {
    let error = compile_and_run_expect_failure(r#"<?php
class NativeTypedGetter { public int $x { get => 1; } }
$base = new NativeTypedGetter();
echo $base->x;
eval('class EvalWrongGetter extends NativeTypedGetter {
    public protected(set) string $x { get => "bad"; set { } }
}');
"#);
    assert!(error.contains("Fatal error: eval() runtime failed"), "{error}");
}

/// A restricted physical child slot satisfies a virtual abstract getter without a setter contract.
#[test]
fn test_asymmetric_review_abstract_getter_normal_storage() {
    let out = compile_and_run(r#"<?php
abstract class AbstractGetter { abstract public int $x { get; } }
class ConcreteStorage extends AbstractGetter { public protected(set) int $x = 2; }
echo (new ConcreteStorage())->x;
"#);
    assert_eq!(out, "2");
}

/// Explicit public(set) overrides readonly's implicit protected write visibility in reflection.
#[test]
fn test_asymmetric_property_readonly_explicit_public_set_reflection() {
    let out = compile_and_run(r#"<?php
class NativeSet {
    public public(set) readonly int $explicit;
    public readonly int $implicit;
}
$explicit = new ReflectionProperty(NativeSet::class, 'explicit');
$implicit = new ReflectionProperty(NativeSet::class, 'implicit');
echo $explicit->isProtectedSet() ? 'protected' : 'public', ':', $explicit->getModifiers(), "\n";
echo $implicit->isProtectedSet() ? 'protected' : 'public', ':', $implicit->getModifiers(), "\n";
eval('class EvalSet { public public(set) readonly int $explicit; public readonly int $implicit; }
$explicit = new ReflectionProperty("EvalSet", "explicit");
$implicit = new ReflectionProperty("EvalSet", "implicit");
echo $explicit->isProtectedSet() ? "protected" : "public", ":", $explicit->getModifiers(), "\n";
echo $implicit->isProtectedSet() ? "protected" : "public", ":", $implicit->getModifiers(), "\n";');
"#);
    assert_eq!(out, "public:129\nprotected:2177\npublic:129\nprotected:2177\n");
}

/// Eval reflection of native readonly slots obeys authoritative exported setter flags.
#[test]
fn test_asymmetric_review_eval_native_public_set_readonly_metadata() {
    let out = compile_and_run(r#"<?php
class NativeSet {
    public public(set) readonly int $explicit;
    public readonly int $implicit;
}
eval('$explicit = new ReflectionProperty("NativeSet", "explicit");
$implicit = new ReflectionProperty("NativeSet", "implicit");
echo $explicit->getModifiers(), ":", ($explicit->isProtectedSet() ? "1" : "0"), ":";
echo $implicit->getModifiers(), ":", ($implicit->isProtectedSet() ? "1" : "0");');
"#);
    assert_eq!(out, "129:0:2177:1");
}

/// Native property filters distinguish explicit public(set) from implicit readonly setters.
#[test]
fn test_asymmetric_review_protected_set_filter_matches_modifiers() {
    let out = compile_and_run(r#"<?php
class NativeSet {
    public public(set) readonly int $explicit;
    public readonly int $implicit;
}
$class = new ReflectionClass(NativeSet::class);
$properties = $class->getProperties(ReflectionProperty::IS_PROTECTED_SET);
echo count($properties), ":";
foreach ($properties as $property) {
    echo $property->getName(), ":", ($property->getModifiers() & ReflectionProperty::IS_PROTECTED_SET);
}
"#);
    assert_eq!(out, "1:implicit:2048");
}

/// Verifies that a `readonly` class permits property initialization inside its constructor.
/// The property is assigned in `__construct` and read back via `$user->id`.
#[test]
fn test_readonly_class_constructor_initialization() {
    let out = compile_and_run(
        r#"<?php
readonly class User {
    public $id;

    public function __construct($id) {
        $this->id = $id;
    }
}

$user = new User(42);
echo $user->id;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that a `final` class can be instantiated and that method calls on the
/// resulting object dispatch correctly (no vtable override possible).
#[test]
fn test_final_class_instantiates_and_dispatches_methods() {
    let out = compile_and_run(
        r#"<?php
final class Receipt {
    public $code = 41;

    public function next() {
        return $this->code + 1;
    }
}

$receipt = new Receipt();
echo $receipt->next();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that a `final` method on a base class is callable on a child instance
/// and does not permit overriding. The child defines a separate method to confirm
/// the child object is fully functional.
#[test]
fn test_final_method_dispatches_normally_without_override() {
    let out = compile_and_run(
        r#"<?php
class Base {
    final public function label() {
        return "base";
    }
}

class Child extends Base {
    public function suffix() {
        return "child";
    }
}

$child = new Child();
echo $child->label();
echo ":";
echo $child->suffix();
"#,
    );
    assert_eq!(out, "base:child");
}

/// Verifies that a `final` property on a base class is readable by a child instance
/// and that a method on the child can read and augment the property value.
#[test]
fn test_final_property_reads_normally_without_override() {
    let out = compile_and_run(
        r#"<?php
class Base {
    final public $value = 40;

    public function value() {
        return $this->value + 2;
    }
}

class Child extends Base {
    public function label() {
        return "answer:";
    }
}

$child = new Child();
echo $child->label();
echo $child->value();
"#,
    );
    assert_eq!(out, "answer:42");
}

/// Verifies typed instance properties with mixed initialization strategies:
/// constructor-only assignment, class-level defaults, nullable with null default,
/// and that reading then assigning via `$user->email` produces the expected output.
#[test]
fn test_typed_properties_defaults_constructor_assignment_and_nullable() {
    let out = compile_and_run(
        r#"<?php
class User {
    public int $id;
    public string $name = "Ada";
    public ?string $email = null;

    public function __construct($id) {
        $this->id = $id;
    }

    public function label() {
        return $this->name . ":" . $this->id;
    }
}

$user = new User(42);
echo $user->label();
echo ":";
echo is_null($user->email);
$user->email = "ada@example.test";
echo ":";
echo $user->email;
"#,
    );
    assert_eq!(out, "Ada:42:1:ada@example.test");
}

/// Verifies that accessing a typed instance property before it is initialized
/// throws a catchable `Error` that `catch(\Error $e)` can observe.
#[test]
fn test_uninitialized_typed_instance_property_is_fatal() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $value;
}

$box = new Box();
try {
    echo $box->value;
} catch (\Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert!(
        out.contains("Typed property Box::$value must not be accessed before initialization"),
        "{out}"
    );
}

/// Verifies that explicitly assigning `0` to a typed instance property constitutes
/// valid initialization and the property reads back as `0`.
#[test]
fn test_typed_instance_property_initialized_to_zero_reads_normally() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $value;
}

$box = new Box();
$box->value = 0;
echo $box->value;
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies that an inherited private typed property without a default starts
/// uninitialized on instances of the child class, so `isset()` can initialize it.
#[test]
fn test_inherited_private_typed_property_without_default_starts_uninitialized() {
    let out = compile_and_run(
        r#"<?php
class ParentBox {
    private string $path;

    public function path(): string {
        if (!isset($this->path)) {
            $this->path = "parent";
        }

        return $this->path;
    }
}

class ChildBox extends ParentBox {}

echo (new ChildBox())->path();
"#,
    );
    assert_eq!(out, "parent");
}

/// Verifies that cloning a child preserves the uninitialized marker of a
/// private typed property declared by an ancestor.
#[test]
fn test_clone_preserves_transitively_inherited_private_typed_property_marker() {
    let out = compile_and_run(
        r#"<?php
class CloneRoot {
    private string $path;

    public function path(): string {
        if (!isset($this->path)) {
            $this->path = "cloned";
        }

        return $this->path;
    }
}

class CloneMiddle extends CloneRoot {}
class CloneChild extends CloneMiddle {}

$copy = clone new CloneChild();
echo $copy->path();
"#,
    );
    assert_eq!(out, "cloned");
}

/// Verifies that a dynamic-evaluation context preserves uninitialized markers
/// when it constructs an AOT class with a private typed ancestor property.
#[test]
fn test_eval_new_preserves_transitively_inherited_private_typed_property_marker() {
    let out = compile_and_run(
        r#"<?php
class EvalRoot {
    private string $path;

    public function path(): string {
        if (!isset($this->path)) {
            $this->path = "evaluated";
        }

        return $this->path;
    }
}

class EvalMiddle extends EvalRoot {}
class EvalChild extends EvalMiddle {}

eval('$value = new EvalChild(); echo $value->path();');
"#,
    );
    assert_eq!(out, "evaluated");
}

/// Verifies that a private typed property without a default remains
/// uninitialized through multiple inheritance levels until its declaring class initializes it.
#[test]
fn test_transitively_inherited_private_typed_property_without_default_starts_uninitialized() {
    let out = compile_and_run(
        r#"<?php
class RootBox {
    private string $path;

    public function path(): string {
        if (!isset($this->path)) {
            $this->path = "root";
        }

        return $this->path;
    }
}

class MiddleBox extends RootBox {}
class ChildBox extends MiddleBox {}

echo (new ChildBox())->path();
"#,
    );
    assert_eq!(out, "root");
}

/// Verifies that a closure-created child instance preserves transitive private
/// typed-property initialization metadata through the dynamic callable path.
#[test]
fn test_closure_created_transitive_private_typed_property_starts_uninitialized() {
    let out = compile_and_run(
        r#"<?php
class RootBox {
    private string $path;

    public function path(): string {
        if (!isset($this->path)) {
            $this->path = "closure";
        }

        return $this->path;
    }
}

class MiddleBox extends RootBox {}
class ChildBox extends MiddleBox {}

$factory = static fn (): ChildBox => new ChildBox();
echo $factory()->path();
"#,
    );
    assert_eq!(out, "closure");
}

/// A compiled first-class callable stored behind `mixed` must retain its
/// descriptor when a dynamic receiver requires a callable/Closure parameter.
#[test]
fn test_dynamic_method_callable_parameter_preserves_first_class_descriptor() {
    let out = compile_and_run(
        r#"<?php
interface Processor {
    public function getEnv(string $prefix, string $name, Closure $getEnv): string;
}

class ProcessorImpl implements Processor {
    public function getEnv(string $prefix, string $name, Closure $getEnv): string {
        return $getEnv instanceof Closure ? 'closure' : 'wrong';
    }
}

class ParentHolder {
    private Closure $getEnv;
    private Processor $processor;

    public function __construct() {
        $this->processor = new ProcessorImpl();
    }

    protected function processor(): mixed {
        return $this->processor;
    }

    protected function getEnv(string $name): string {
        $this->getEnv ??= $this->getEnv(...);
        $processor = $this->processor();

        return $processor->getEnv('default', $name, $this->getEnv);
    }
}

class ChildHolder extends ParentHolder {
    public function run(): string {
        return $this->getEnv('ok');
    }
}

eval('');
echo (new ChildHolder())->run();
"#,
    );
    assert_eq!(out, "closure");
}

/// A native first-class method callable stored in a typed property remains a Closure when a
/// dynamically evaluated receiver validates its declared parameter type.
#[test]
fn test_eval_method_closure_parameter_accepts_native_property_callable() {
    let out = compile_and_run(
        r#"<?php
class Holder {
    private Closure $getEnv;

    private function native(string $name): string {
        return $name;
    }

    public function run(): string {
        $this->getEnv ??= $this->native(...);
        $processor = eval('return new class {
            public function getEnv(string $prefix, string $name, Closure $getEnv): string {
                return $getEnv instanceof Closure ? $getEnv($name) : "wrong";
            }
        };');

        return $processor->getEnv('default', 'ok', $this->getEnv);
    }
}

echo (new Holder())->run();
"#,
    );
    assert_eq!(out, "ok");
}

/// A private typed property declared by an AOT parent remains parent-owned when an eval-created
/// child invokes that parent method. The child cannot override the private slot, so `??=` must
/// initialize and later read the native property rather than an eval overlay entry.
#[test]
fn test_eval_subclass_reads_parent_private_closure_slot_natively() {
    let out = compile_and_run(
        r#"<?php
class NativeBase {
    private Closure $callback;

    protected function resolve(string $name): string {
        $this->callback ??= static fn (string $value): string => $value;
        return ($this->callback)($name);
    }
}

$child = eval('return new class extends NativeBase {
    public function run(): string { return $this->resolve("ok"); }
};');
echo $child->run();
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies lazy source-directory discovery across transitive inheritance,
/// reflection, and a closure-created object without depending on a fixture path.
#[test]
fn test_transitive_private_typed_property_lazy_reflection_directory_initialization() {
    let out = compile_cli_files_and_run(
        &[
            ("project-root.marker", "root\n"),
            (
                "src/RootBox.php",
                r#"<?php
namespace App;

function pass_through(mixed $value): mixed {
    return $value;
}

trait ParentStorage {
    private array $entries = [];
}

trait ChildStorage {
    private array $entries = [];
}

class RootBox {
    protected array $bundles = [];
    protected mixed $container = null;
    protected bool $booted = false;
    protected ?float $startTime = null;
    private string $directory;

    public function __construct(
        protected string $environment,
        protected bool $debug,
    ) {}

    public function directory(): string {
        if (!isset($this->directory)) {
            $file = pass_through((new \ReflectionObject($this))->getFileName());
            if (!is_file($file)) {
                return "missing";
            }
            $directory = dirname($file);
            while (!is_file($directory . "/project-root.marker")) {
                if ($directory === dirname($directory)) {
                    return "root";
                }
                $directory = dirname($directory);
            }
            $this->directory = $directory;
        }

        return $this->directory;
    }
}
"#,
            ),
            (
                "src/MiddleBox.php",
                "<?php\nnamespace App;\nclass MiddleBox extends RootBox { use ParentStorage; private ?string $warmup = null; private int $counter = 0; private bool $reset = false; }\n",
            ),
            (
                "src/ChildBox.php",
                "<?php\nnamespace App;\nclass ChildBox extends MiddleBox { use ChildStorage; }\n",
            ),
            (
                "entry/main.php",
                "<?php\nrequire __DIR__ . '/../src/RootBox.php';\nrequire __DIR__ . '/../src/MiddleBox.php';\nrequire __DIR__ . '/../src/ChildBox.php';\n$factory = static fn (): \\App\\ChildBox => new \\App\\ChildBox('dev', true);\n$directory = $factory()->directory();\necho is_file($directory . '/project-root.marker') ? 'ready' : 'wrong';\n",
            ),
        ],
        "entry/main.php",
    );
    assert_eq!(out, "ready");
}

/// Verifies a dynamic include preserves the lexical class scope for a private method bridge.
#[test]
fn test_dynamic_include_preserves_private_method_scope() {
    let out = compile_cli_files_and_run(
        &[
            (
                "entry.php",
                r#"<?php
function scope_identity(mixed $value): mixed { return $value; }

trait PrivateScopeAfterDynamicInclude {
    private string $includePath;

    private function privateLabel(array &$visited, array &$visiting = []): string {
        $visited['called'] = true;
        $visiting['entered'] = true;
        return 'private-ok';
    }

    private function includeThenCallPrivate(): string {
        $path = $this->includePath;
        require $path;
        $self = scope_identity($this);
        $visited = [];
        $label = $self->privateLabel($visited);
        $copy = $visited;
        $copy['copied'] = true;
        return $label . '-' . $visited['called'] . '-' . $copy['copied'];
    }
}

class ScopeCarrier {
    use PrivateScopeAfterDynamicInclude;

    public function __construct(string $path) { $this->includePath = $path; }
    public function run(): string { return $this->includeThenCallPrivate(); }
}

final class ScopeCarrierChild extends ScopeCarrier {}

echo (new ScopeCarrierChild(__DIR__.'/payload.php'))->run();
"#,
            ),
            ("payload.php", "<?php $included = true;\n"),
        ],
        "entry.php",
    );
    assert_eq!(out, "private-ok-1-1");
}

/// Verifies a dynamically included class retains constructor parameters from a protected method.
#[test]
fn test_dynamic_include_class_constructor_keeps_associative_parameters() {
    let out = compile_cli_files_and_run(
        &[
            (
                "entry.php",
                r#"<?php
$path = __DIR__ . '/definition.php';
require $path;
$class = 'ParameterCarrier';
$carrier = new $class();
echo (($carrier->defaults()['kernel.debug'] ?? false) ? 'd' : 'x') . ($carrier->parameter('kernel.debug') ? 'p' : 'q');
"#,
            ),
            (
                "definition.php",
                r#"<?php
class ParameterCarrier {
    protected array $parameters = [];

    public function __construct() {
        $this->parameters = $this->getDefaultParameters();
    }

    protected function getDefaultParameters(): array {
        return ['kernel.debug' => true];
    }

    public function defaults(): array {
        return $this->getDefaultParameters();
    }

    public function parameter(string $name): mixed {
        if (array_key_exists($name, $this->parameters) && '.' !== ($name[0] ?? '')) {
            return $this->parameters[$name];
        }

        return null;
    }
}
"#,
            ),
        ],
        "entry.php",
    );
    assert_eq!(out, "dp");
}

/// Verifies an associative map returned by a dynamic require reaches typed AOT method arguments.
///
/// The method receives a PHP `array` contract while the runtime value is hash-backed. Its nested
/// traversal and by-reference accumulator must retain the associative storage rather than
/// reinterpreting it as an indexed array.
#[test]
fn test_dynamic_require_assoc_map_survives_typed_method_arguments() {
    let out = compile_cli_files_and_run(
        &[
            (
                "entry.php",
                r#"<?php
class BundleResolver {
    public function resolve(array $bundles): array {
        $resolved = [];
        foreach ($bundles as $class => $envs) {
            $this->append($class, $envs, $bundles, $resolved);
        }

        return $resolved;
    }

    private function append(string $class, array $envs, array $bundles, array &$resolved, array &$visiting = []): void {
        $visiting[$class] = true;
        if (!isset($bundles[$class])) {
            return;
        }
        $resolved[$class] = $envs;
    }
}

$path = __DIR__ . '/bundles.php';
$bundles = is_file($path) ? require $path : [];
$resolved = (new BundleResolver())->resolve($bundles);
echo $resolved['Fixture\\Bundle']['all'] ? 'resolved' : 'missing';
"#,
            ),
            (
                "bundles.php",
                "<?php\nreturn ['Fixture\\\\Bundle' => ['all' => true]];\n",
            ),
        ],
        "entry.php",
    );
    assert_eq!(out, "resolved");
}

/// Verifies a dynamic subclass updates a protected property used by an AOT parent method.
#[test]
fn test_dynamic_subclass_syncs_redeclared_protected_parent_property() {
    let out = compile_cli_files_and_run(
        &[
            (
                "entry.php",
                r#"<?php
class ParameterBase {
    protected array $parameters = [];

    public function parameter(string $name): mixed {
        return $this->parameters[$name] ?? null;
    }
}

$path = __DIR__ . '/definition.php';
require $path;
$class = 'ParameterCarrier';
echo (new $class())->parameter('kernel.debug') ? 'ok' : 'wrong';
"#,
            ),
            (
                "definition.php",
                r#"<?php
class ParameterCarrier extends ParameterBase {
    protected array $parameters = [];

    public function __construct() {
        $this->parameters = ['kernel.debug' => true];
    }
}
"#,
            ),
        ],
        "entry.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies an eval barrier preserves an inherited boolean `$this` property.
///
/// `$this` is not assignable by PHP code, including dynamically included code. The following
/// trait method must therefore keep its concrete object representation across the eval barrier
/// and read the inherited boolean without routing the raw scalar through `__rt_mixed_cast_bool`.
#[test]
fn test_dynamic_include_preserves_inherited_bool_this_property_truthiness() {
    let out = compile_cli_files_and_run(
        &[
            (
                "entry.php",
                r#"<?php
trait DynamicBarrierBoolTrait {
    protected function checkDynamicBarrierBool(): void {
        $path = str_replace('/fixture/', '/fixture/', __DIR__ . '/fixture/declaration.php');
        require $path;

        if ($this->debug && !defined('DYNAMIC_BARRIER_BOOL_DISABLED')) {
            echo 'ok';
        }
    }
}

class DynamicBarrierBoolBase {
    public function __construct(protected bool $debug) {}
}

class DynamicBarrierBoolChild extends DynamicBarrierBoolBase {
    use DynamicBarrierBoolTrait;

    public function run(): void {
        $this->checkDynamicBarrierBool();
    }
}

(new DynamicBarrierBoolChild(true))->run();
"#,
            ),
            ("fixture/declaration.php", "<?php\nfunction dynamic_barrier_marker(): void {}\n"),
        ],
        "entry.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies a namespaced dynamically included class reads its protected associative property.
#[test]
fn test_namespaced_dynamic_class_reads_protected_associative_property() {
    let out = compile_cli_files_and_run(
        &[
            (
                "entry.php",
                r#"<?php
$path = __DIR__ . '/definition.php';
require $path;
$class = 'Generated\\ParameterCarrier';
echo (new $class())->parameter('kernel.debug') ? 'ok' : 'wrong';
"#,
            ),
            (
                "definition.php",
                r#"<?php
namespace Generated;

class ParameterCarrier {
    protected array $parameters = [];

    public function __construct() {
        $this->parameters = ['kernel.debug' => true];
    }

    public function parameter(string $name): mixed {
        if (array_key_exists($name, $this->parameters) && '.' !== ($name[0] ?? '')) {
            return $this->parameters[$name];
        }

        return null;
    }
}
"#,
            ),
        ],
        "entry.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies a generated-style dynamic getter finds values in its protected parameter map.
#[test]
fn test_dynamic_generated_style_parameter_getter_reads_protected_map() {
    let out = compile_cli_files_and_run(
        &[
            (
                "entry.php",
                r#"<?php
$path = __DIR__ . '/definition.php';
require $path;
$class = 'Generated\\ParameterCarrier';
echo (new $class())->getParameter('kernel.debug') ? 'ok' : 'wrong';
"#,
            ),
            (
                "definition.php",
                r#"<?php
namespace Generated;

class ParameterCarrier {
    private const DEPRECATED_PARAMETERS = [];
    private const NONEMPTY_PARAMETERS = [];
    private array $loadedDynamicParameters = [];
    private array $dynamicParameters = [];
    protected array $parameters = [];

    public function __construct(private array $buildParameters = [], protected string $containerDir = __DIR__) {
        $this->parameters = $this->getDefaultParameters();
    }

    protected function getDefaultParameters(): array {
        return [
            'kernel.project_dir' => dirname(__DIR__, 2),
            'kernel.environment' => 'dev',
            'kernel.debug' => true,
            'kernel.bundles' => [
                'FrameworkBundle' => 'FrameworkBundle',
            ],
            'kernel.bundles_metadata' => [
                'FrameworkBundle' => [
                    'path' => dirname(__DIR__),
                    'namespace' => 'Framework',
                ],
            ],
            '.known_environments' => ['prod', 'dev', 'test'],
            'kernel.logs_dir' => dirname(__DIR__) . '/log',
            'kernel.http_method_override' => false,
            'session.storage.options' => [
                'cookie_secure' => 'auto',
                'cookie_httponly' => true,
            ],
        ];
    }

    public function getParameter(string $name): mixed {
        if (isset(self::DEPRECATED_PARAMETERS[$name])) {
            return null;
        }

        if (array_key_exists($name, $this->buildParameters)) {
            return $this->buildParameters[$name];
        }

        if (isset($this->loadedDynamicParameters[$name])) {
            return $this->loadedDynamicParameters[$name]
                ? $this->dynamicParameters[$name]
                : null;
        }

        if (array_key_exists($name, $this->parameters) && '.' !== ($name[0] ?? '')) {
            return $this->parameters[$name];
        }

        return null;
    }
}
"#,
            ),
        ],
        "entry.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies a statically typed AOT call dispatches to a runtime-defined subclass override.
#[test]
fn test_typed_aot_method_call_dispatches_to_dynamic_subclass_override() {
    let out = compile_and_run(
        r#"<?php
class BaseMessage {
    public function label(): string {
        return 'base';
    }
}

function label_for(BaseMessage $message): string {
    return $message->label();
}

$definition = 'namespace Generated; class Message extends \\BaseMessage { public function label(): string { return \'dynamic\'; } }';
eval($definition);
$class = 'Generated\\Message';
echo label_for(new $class()) . label_for(new BaseMessage());
"#,
    );
    assert_eq!(out, "dynamicbase");
}

/// Verifies that accessing an uninitialized typed static property throws a
/// catchable `Error` that `catch(\Error $e)` can observe.
#[test]
fn test_uninitialized_typed_static_property_is_fatal() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public static int $value;
}

try {
    echo Box::$value;
} catch (\Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert!(
        out.contains("Typed static property Box::$value must not be accessed before initialization"),
        "{out}"
    );
}

/// `??`, `isset()` and `empty()` must NOT hit the fatal above — PHP answers the default,
/// `false` and `true` for an uninitialized typed static property without raising.
///
/// The instance path has branched on `Op::PropInitialized` for a while; the static guard is
/// emitted straight into the read instead, so the lowering had nothing to branch on and all
/// three constructs raised. `Op::StaticPropInitialized` is that missing operation. The
/// resolution is exercised through every receiver form, because each resolves the declaring
/// class differently: `Named::`, `self::`, `static::` and `parent::`. The private slot is here
/// because the probe enforces visibility where the Reflection one it reuses does not.
#[test]
fn test_coalesce_isset_and_empty_suppress_an_uninitialized_typed_static_property() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public static int $b;
}
class Kid extends Base {
    public static int $k;
    private static ?string $priv;

    public static function viaSelf(): string { return self::$k ?? "d"; }
    public static function viaStatic(): string { return static::$k ?? "d"; }
    public static function viaParent(): string { return parent::$b ?? "d"; }
    public static function viaPriv(): string { return self::$priv ?? "d"; }
    public static function probe(): string { return isset(self::$k) ? "y" : "n"; }
    public static function blank(): string { return empty(self::$k) ? "y" : "n"; }
    public static function fill(): string { self::$k ??= 7; return (string) self::$k; }
}
echo Kid::$k ?? "d", Kid::viaSelf(), Kid::viaStatic(), Kid::viaParent(), Kid::viaPriv();
echo "|", Kid::probe(), Kid::blank();
echo "|", Kid::fill(), Kid::viaSelf(), Kid::probe(), Kid::blank();
"#,
    );
    assert_eq!(out, "ddddd|ny|77yn");
}

/// The controls for the probe above. An INITIALIZED slot keeps its own value even when that
/// value is falsy (`isset` true, `empty` true on `0`); a slot explicitly set to null and an
/// UNTYPED slot are plain null from the start, so they never reach the probe and answer the
/// way they always did.
#[test]
fn test_static_property_probe_leaves_initialized_and_untyped_slots_alone() {
    let out = compile_and_run(
        r#"<?php
class Z {
    public static int $zero;
    public static ?string $nul = null;
    public static $untyped;
}
Z::$zero = 0;
echo Z::$zero ?? "d", isset(Z::$zero) ? "y" : "n", empty(Z::$zero) ? "y" : "n";
echo "|", Z::$nul ?? "d", isset(Z::$nul) ? "y" : "n", empty(Z::$nul) ? "y" : "n";
echo "|", Z::$untyped ?? "d", isset(Z::$untyped) ? "y" : "n", empty(Z::$untyped) ? "y" : "n";
"#,
    );
    assert_eq!(out, "0yy|dny|dny");
}

/// Verifies that accessing an uninitialized typed property and catching `Error`
/// allows continued execution after the property is initialized (issue #339).
#[test]
fn test_uninitialized_property_catch_then_continue() {
    let out = compile_and_run(
        r#"<?php
class C { public int $x; }
$c = new C();
try { echo $c->x; } catch (\Error $e) { echo "e"; }
$c->x = 5;
echo $c->x;
"#,
    );
    assert_eq!(out, "e5");
}

/// Verifies that catching `Exception` (not `Error`) does NOT catch the
/// uninitialized typed property access (issue #339).
#[test]
fn test_uninitialized_property_catch_exception_does_not_match() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class C { public int $x; }
$c = new C();
try { echo $c->x; } catch (\Exception $e) { echo "caught"; }
echo "end";
"#,
    );
    // Asserting the CLASS is what this test is actually about: the `catch (\Exception $e)` must
    // not match because an uninitialized typed property raises an `Error`. The previous
    // assertion only looked for the word "uncaught", so a fatal of any class would have passed —
    // including the very `Exception` the test exists to rule out.
    //
    // Byte-identical to reference PHP 8.5.6 up to its ` in <file>:<line>` suffix, which elephc
    // cannot emit (see issue #660).
    assert!(
        err.contains(
            "Fatal error: Uncaught Error: Typed property C::$x must not be accessed before initialization"
        ),
        "{err}"
    );
}
/// Verifies a typed static property assigned zero remains distinguishable from uninitialized.
#[test]
fn test_typed_static_property_initialized_to_zero_reads_normally() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public static int $value;
}

Box::$value = 0;
echo Box::$value;
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies typed instance and static property defaults resolve class-like constants only after
/// class, parent, and interface metadata is complete.
#[test]
fn test_typed_property_defaults_resolve_scoped_constants() {
    let out = compile_and_run(
        r#"<?php
interface Defaults {
    public const VALUE = 41;
}

class ParentBox {
    protected const OFFSET = 1;
}

class Box extends ParentBox implements Defaults {
    private const LOCAL = 40;

    public int $interfaceValue = Defaults::VALUE;
    public int $selfValue = self::LOCAL;
    public int $parentValue = parent::OFFSET;
    public static int $staticValue = Defaults::VALUE;
}

$box = new Box();
echo $box->interfaceValue, ":", $box->selfValue, ":", $box->parentValue, ":", Box::$staticValue;
"#,
    );
    assert_eq!(out, "41:40:1:41");
}

/// Verifies scoped constants nested in literal property defaults are resolved for instance and
/// static storage, including an instance selected by a runtime class string.
#[test]
fn test_array_property_defaults_resolve_nested_scoped_constants() {
    let out = compile_and_run(
        r#"<?php
class DefaultMap {
    private const CREATED = 'c';
    private const ALIGN = 'left';

    public array $values = [self::CREATED => 1, 'align' => self::ALIGN];
    public static array $staticValues = [self::CREATED => 2, 'align' => self::ALIGN];
}

function instantiate(string $class): object {
    return new $class();
}

$direct = new DefaultMap();
$dynamic = instantiate('DefaultMap');
echo $direct->values['c'], ':', $dynamic->values['align'], ':';
echo DefaultMap::$staticValues['c'], ':', DefaultMap::$staticValues['align'];
"#,
    );
    assert_eq!(out, "1:left:2:left");
}

/// Verifies that a nullable typed static property with an explicit `= null` default
/// is considered initialized (`is_null()` returns true), and that a typed static
/// property without a default remains uninitialized and throws a catchable Error;
/// without a try/catch the no-handler fast path still reports the specific
/// fatal diagnostic (issue #339).
#[test]
fn test_nullable_static_property_default_null_is_initialized() {
    let out = compile_and_run(
        r#"<?php
class WithDefault {
    public static ?int $value = null;
}
echo is_null(WithDefault::$value);
"#,
    );
    assert_eq!(out, "1");

    let err = compile_and_run_expect_failure(
        r#"<?php
class WithoutDefault {
    public static ?int $value;
}

echo WithoutDefault::$value;
"#,
    );
    assert!(
        err.contains("Fatal error: Typed static property WithoutDefault::$value must not be accessed before initialization"),
        "{err}"
    );
}

/// Verifies a nullable integer narrowed by a null check can populate an integer static property.
#[test]
fn test_narrowed_nullable_int_static_property_assignment() {
    let out = compile_and_run(
        r#"<?php
class StaticIntTarget {
    public static int $value = 0;

    public static function assign(?int $value): void {
        if (null !== $value) {
            self::$value = $value;
        }
    }
}
StaticIntTarget::assign(7);
echo StaticIntTarget::$value;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies that an untyped instance property with a `= null` default is strictly
/// null (`=== null` is true, `== null` is true) and that `var_dump` emits `NULL`.
#[test]
fn test_untyped_null_property_default_is_strictly_null() {
    let out = compile_and_run(
        r#"<?php
class A { public $x = null; }
$a = new A();
var_dump($a->x);
echo is_null($a->x) ? "y" : "n", "\n";
echo ($a->x === null) ? "y" : "n", "\n";
echo ($a->x !== null) ? "y" : "n", "\n";
echo ($a->x == null) ? "y" : "n", "\n";
"#,
    );
    assert_eq!(out, "NULL\ny\ny\nn\ny\n");
}

/// Verifies that an untyped static property with a `= null` default is strictly
/// null (`=== null` is true, `== null` is true) and that `var_dump` emits `NULL`.
#[test]
fn test_untyped_static_null_property_default_is_strictly_null() {
    let out = compile_and_run(
        r#"<?php
class A { public static $x = null; }
var_dump(A::$x);
echo is_null(A::$x) ? "y" : "n", "\n";
echo (A::$x === null) ? "y" : "n", "\n";
echo (A::$x !== null) ? "y" : "n", "\n";
echo (A::$x == null) ? "y" : "n", "\n";
"#,
    );
    assert_eq!(out, "NULL\ny\ny\nn\ny\n");
}

/// Verifies that assigning `null` to an untyped instance or static property that
/// previously held a non-null value results in a strictly-null value
/// (`=== null` is true).
#[test]
fn test_untyped_property_assignment_to_null_is_strictly_null() {
    let out = compile_and_run(
        r#"<?php
class A {
    public $x = 1;
    public static $y = 1;
}
$a = new A();
$a->x = null;
A::$y = null;
echo is_null($a->x) ? "y" : "n", "\n";
echo ($a->x === null) ? "y" : "n", "\n";
echo is_null(A::$y) ? "y" : "n", "\n";
echo (A::$y === null) ? "y" : "n", "\n";
"#,
    );
    assert_eq!(out, "y\ny\ny\ny\n");
}

/// Verifies that a static property on a `readonly` class is mutable
/// (readonly only affects instance properties, not static ones).
#[test]
fn test_readonly_class_static_property_is_mutable() {
    let out = compile_and_run(
        r#"<?php
readonly class Counter {
    public static int $count = 0;
}
Counter::$count = 5;
echo Counter::$count;
Counter::$count = Counter::$count + 1;
echo ":";
echo Counter::$count;
"#,
    );
    assert_eq!(out, "5:6");
}

/// Verifies that a static property on an `abstract readonly` class is mutable,
/// matching the behaviour of plain `readonly` classes.
#[test]
fn test_readonly_abstract_class_static_property_is_mutable() {
    let out = compile_and_run(
        r#"<?php
abstract readonly class Counter {
    public static int $count = 0;
}
Counter::$count = 7;
echo Counter::$count;
Counter::$count = Counter::$count + 1;
echo ":";
echo Counter::$count;
"#,
    );
    assert_eq!(out, "7:8");
}

/// Verifies that a static property inherited through a `readonly` child class
/// remains mutable and that both base and child share the same static slot.
#[test]
fn test_readonly_inherited_static_property_remains_mutable() {
    let out = compile_and_run(
        r#"<?php
readonly class Base {
    public static int $shared = 1;
}
readonly class Child extends Base {
}
Child::$shared = 42;
echo Base::$shared;
echo ":";
echo Child::$shared;
"#,
    );
    assert_eq!(out, "42:42");
}

/// End-to-end smoke test using the checked-in `examples/final-classes/main.php`
/// fixture. Verifies the example compiles, runs, and produces `"invoice:42\n"`.
#[test]
fn test_example_final_classes_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/final-classes/main.php"));
    assert_eq!(out, "invoice:42\n");
}

/// End-to-end smoke test using the checked-in `examples/typed-properties/main.php` fixture.
///
/// The trailing rows cover array-literal defaults on NULLABLE and union properties, in both the
/// keyed and positional spellings (issue #688) — declaring such a class used to be a compile
/// error on its own, before any read.
///
/// The expectation is verbatim host PHP 8.5.10 output for the whole example.
#[test]
fn test_example_typed_properties_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/typed-properties/main.php"));
    assert_eq!(
        out,
        concat!(
            "Ada:42\n",
            "repository: UserRepository\n",
            "missing email\n",
            "query: 1/name\n",
            "tags: new, featured\n",
            "meta: web\n",
            "body is null: yes\n",
            "query after clear: null\n",
        )
    );
}

/// Verifies PHP 8.4 asymmetric visibility at runtime: a `public private(set)` property is
/// writable from inside the class and readable from outside.
#[test]
fn test_asymmetric_visibility_internal_write_external_read() {
    let out = compile_and_run(
        "<?php
        class Counter {
            public private(set) int $value = 0;
            public function increment(): void { $this->value = $this->value + 1; }
        }
        $c = new Counter();
        $c->increment();
        $c->increment();
        echo $c->value;
        ",
    );
    assert_eq!(out, "2");
}

/// Verifies that a subclass may write a `protected(set)` property inherited from its parent.
#[test]
fn test_asymmetric_visibility_protected_set_subclass_write() {
    let out = compile_and_run(
        "<?php
        class Base { public protected(set) string $name = \"base\"; }
        class Derived extends Base {
            public function rename(string $n): void { $this->name = $n; }
        }
        $d = new Derived();
        $d->rename(\"derived\");
        echo $d->name;
        ",
    );
    assert_eq!(out, "derived");
}

/// Verifies PHP 8.4 asymmetric visibility on promoted constructor properties (issue #823): the
/// declaring class writes a `private(set)` / `protected private(set)` property, a subclass writes
/// a `protected(set)` one, `readonly` combines with a `(set)` modifier, and outside code reads.
/// Expected output copied from PHP 8.5.
#[test]
fn test_promoted_asymmetric_visibility_writes_and_reads() {
    let out = compile_and_run(
        "<?php
        final class Subject { public function __construct(private(set) string $value) {} }
        class Point {
            public function __construct(
                public private(set) int $x,
                public protected(set) int $y,
                protected private(set) string $tag = \"p\",
                public private(set) readonly int $id = 7,
                protected(set) readonly string $label = \"lbl\",
            ) {}
            public function move(int $dx): void { $this->x = $this->x + $dx; }
            public function tag(): string { return $this->tag; }
            public function retag(string $t): void { $this->tag = $t; }
        }
        class Point3 extends Point {
            public function shiftY(int $dy): void { $this->y = $this->y + $dy; }
        }
        $s = new Subject(\"hello\");
        echo $s->value, \"\\n\";
        $p = new Point3(1, 2);
        $p->move(10);
        $p->shiftY(5);
        $p->retag(\"q\");
        echo $p->x, \" \", $p->y, \" \", $p->tag(), \" \", $p->id, \" \", $p->label, \"\\n\";
        ",
    );
    assert_eq!(out, "hello\n11 7 q 7 lbl\n");
}

/// Verifies that Reflection reports the same asymmetric-visibility flags for a promoted
/// constructor property as PHP does for the declared equivalent. Expected output copied from
/// PHP 8.5.
#[test]
fn test_promoted_asymmetric_visibility_reflection_flags() {
    let out = compile_and_run(
        "<?php
        class Point {
            public function __construct(
                public private(set) int $x,
                public protected(set) int $y,
                protected private(set) string $tag = \"p\",
                public private(set) readonly int $id = 7,
            ) {}
        }
        function show(string $name, ReflectionProperty $r): void {
            echo $name, \": privateSet=\", var_export($r->isPrivateSet(), true),
                \" protectedSet=\", var_export($r->isProtectedSet(), true),
                \" promoted=\", var_export($r->isPromoted(), true),
                \" modifiers=\", $r->getModifiers(), \"\\n\";
        }
        show(\"x\", new ReflectionProperty(Point::class, \"x\"));
        show(\"y\", new ReflectionProperty(Point::class, \"y\"));
        show(\"tag\", new ReflectionProperty(Point::class, \"tag\"));
        show(\"id\", new ReflectionProperty(Point::class, \"id\"));
        ",
    );
    assert_eq!(
        out,
        "x: privateSet=true protectedSet=false promoted=true modifiers=4129\n\
         y: privateSet=false protectedSet=true promoted=true modifiers=2049\n\
         tag: privateSet=true protectedSet=false promoted=true modifiers=4130\n\
         id: privateSet=true protectedSet=false promoted=true modifiers=4257\n"
    );
}

/// Compiles and runs the checked-in `examples/asymmetric-visibility/main.php` fixture, which
/// models an account whose balance is publicly readable but only privately writable, plus a
/// transfer whose promoted constructor properties carry the same asymmetric visibility.
#[test]
fn test_example_asymmetric_visibility_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/asymmetric-visibility/main.php"));
    assert_eq!(
        out,
        "balance: 120\ninsufficient funds\nbalance: 120\ntransfer TX-1: 35\n"
    );
}

/// Verifies an untyped property whose default is a constant takes the constant's type: a global
/// `const`, `self::`, `parent::` and named class constants, an array constant, and constants
/// inside an array literal. The syntactic guess typed a class constant `string` and a global one
/// `int`, so `public $k = self::K;` with `K = 9` failed the build (`prop_set` of an int into a
/// string slot), and `public $l = self::L;` had no literal form at all.
#[test]
fn test_constant_valued_untyped_property_defaults_take_the_constant_type() {
    let out = compile_and_run(
        r#"<?php
const GREETING = 'hi';
class P { const S = 'str'; const F = 1.5; }
class B extends P {
    const K = 9;
    const L = [1, 2];
    public $g = GREETING;
    public $s = self::S;
    public $f = parent::F;
    public $k = self::K;
    public $l = self::L;
    public $arr = [self::K, self::S];
    public static $sk = self::K;
    public $named = P::S;
}
$b = new B();
echo json_encode([$b->g, $b->s, $b->f, $b->k, $b->l, $b->arr, B::$sk, $b->named]), "\n";
$b->k = 'now a string';
var_dump($b->k);
"#,
    );
    assert_eq!(out, "[\"hi\",\"str\",1.5,9,[1,2],[9,\"str\"],9,\"str\"]\nstring(12) \"now a string\"\n");
}

/// Verifies an untyped scalar property written a value of another type keeps both: the default
/// until the write, the written value after, and the default again on a fresh object. The slot
/// used to be retyped to the written value, so propinit stored the `9` of `public $i = 9;` into a
/// `string` slot and the program did not build.
#[test]
fn test_untyped_scalar_property_written_another_type_holds_both() {
    let out = compile_and_run(
        r#"<?php
class B { public $i = 9; public $f = 1.5; public $s = "a"; public $b = true; public $i2 = 0; public $f2 = 0.0; }
$b = new B();
var_dump($b->i, $b->f, $b->s, $b->b, $b->i2, $b->f2);
$b->i = "str"; $b->f = 3; $b->s = 5; $b->b = "x"; $b->i2 = 2.5; $b->f2 = 7;
var_dump($b->i, $b->f, $b->s, $b->b, $b->i2, $b->f2);
$c = new B();
var_dump($c->i, $c->f, $c->s, $c->b, $c->i2, $c->f2);
"#,
    );
    assert_eq!(
        out,
        "int(9)\nfloat(1.5)\nstring(1) \"a\"\nbool(true)\nint(0)\nfloat(0)\n\
         string(3) \"str\"\nint(3)\nint(5)\nstring(1) \"x\"\nfloat(2.5)\nint(7)\n\
         int(9)\nfloat(1.5)\nstring(1) \"a\"\nbool(true)\nint(0)\nfloat(0)\n"
    );
}

/// Verifies an enum case is a property default: untyped (boxed), static through an enum
/// constant, and nullable-typed, each the case's own singleton. There was no default form for a
/// case, so `public $mode = Mode::Slow;` failed the build in `object_new` and a static one in the
/// static-property initializer.
#[test]
fn test_enum_case_property_defaults() {
    let out = compile_and_run(
        r#"<?php
enum Mode { case Fast; case Slow; }
enum Level: int { case Low = 1; case High = 2; const DEFAULT = self::High; }
class Cfg { public $mode = Mode::Slow; public static $lvl = Level::DEFAULT; private ?Level $opt = Level::Low; public function opt() { return $this->opt; } }
$c = new Cfg();
var_dump($c->mode, Cfg::$lvl, $c->opt());
var_dump(spl_object_id($c->mode) === spl_object_id(Mode::Slow));
"#,
    );
    assert_eq!(out, "enum(Mode::Slow)\nenum(Level::High)\nenum(Level::Low)\nbool(true)\n");
}
