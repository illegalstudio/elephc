//! Purpose:
//! Runtime regression tests for native generic templates and typed contracts.
//!
//! Called from:
//! - `cargo test -p elephc-magician generics`.
//!
//! Key details:
//! - Persistent contexts exercise declarations and specialization across separate fragments.

use super::support::*;
use super::super::*;

/// Executes fragments in one context and returns captured output.
fn run(fragments: &[&str]) -> String {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    context.set_global_scope(&mut scope);
    let mut values = FakeOps::default();
    for source in fragments {
        let program = parse_fragment(source.as_bytes()).expect("generic syntax parses");
        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .unwrap_or_else(|error| panic!("{source}: {error:?}; output: {}", values.output));
        values.release(result).expect("release eval result");
    }
    values.output
}

/// Returns before nested generic declarations must leave every declaration family unregistered.
#[test]
fn generics_review_unreachable_function_declarations() {
    for declaration in [
        "class Hidden<T> {}", "interface Hidden<T> {}", "trait Hidden<T> {}",
        "function hidden<T>(T $value): T { return $value; }",
        "class Hidden<out T> { public function put(T $value): void {} }",
    ] {
        let source = format!("function stop() {{ echo 'call|'; return; {declaration} }} stop(); stop(); echo 'done';");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let program = parse_fragment(source.as_bytes()).unwrap();
        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
        values.release(result).unwrap();
        assert_eq!(values.output, "call|call|done", "{declaration}");
        assert!(context.generic_template("Hidden").is_none(), "{declaration}");
        assert!(context.generic_function_template("hidden").is_none(), "{declaration}");
    }
}

/// Loop control must skip unreachable declarations instead of registering them each iteration.
#[test]
fn generics_review_unreachable_loop_declarations() {
    assert_eq!(run(&[r#"
for ($i = 0; $i < 2; $i++) { echo $i; continue; class Hidden<T> {} }
for ($i = 0; $i < 2; $i++) { break; trait Skipped<T> {} }
echo '|done';
"#]), "01|done");
}

/// Reached nested templates become persistent and still reject a later redeclaration.
#[test]
fn generics_review_reached_nested_declarations() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let program = parse_fragment(b"function install_templates() { class Visible<T> {} } install_templates(); echo get_class(new Visible<int>());").unwrap();
    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    values.release(result).unwrap();
    assert_eq!(values.output, "Visible<int>");
    let repeat = parse_fragment(b"install_templates();").unwrap();
    assert_eq!(execute_program_with_context(&mut context, &repeat, &mut scope, &mut values), Err(EvalStatus::RuntimeFatal));
    assert!(context.generic_template("Visible").is_some());
}

/// Later trait, class-parent, and interface definitions must not make earlier variance promises unsafe.
#[test]
fn generics_review_later_variance_dependencies() {
    for (relation, later) in [
        ("use Put<T>;", "trait Put<T> { public function put(T $value): void {} }"),
        ("", "class Put<T> { public function put(T $value): void {} }"),
        ("", "interface Put<T> { public function put(T $value): void; }"),
    ] {
        let header = if later.starts_with("class") { "extends Put<T>" }
            else if later.starts_with("interface") { "implements Put<T>" } else { "" };
        let method = if later.starts_with("interface") { "public function put(int $value): void {}" } else { "" };
        let source = format!("function install_templates() {{ class Box<out T> {header} {{ {relation} {method} }} {later} }} install_templates(); try {{ new Box<int>(); }} catch (TypeError $error) {{ echo 'variance'; }}");
        assert_eq!(run(&[&source]), "variance", "{later}");
    }
}

/// Later safe traits respect qualified private adaptations when a nested class is specialized.
#[test]
fn generics_review_later_safe_trait_dependencies() {
    assert_eq!(run(&[r#"
function install_templates() {
    class Box<out T> { use Put<T> { Put::put as private; } }
    trait Put<T> { public function put(T $value): void {} }
}
install_templates(); echo get_class(new Box<int>()), '|', get_class(new Box<int>());
class_alias('Box<int>', 'AliasBox'); echo '|', get_class(new AliasBox());
"#]), "Box<int>|Box<int>|Box<int>");
}

/// Aliases and reference parameters must enforce the original typed local before mutation.
#[test]
fn generics_review_reference_local_contracts() {
    assert_eq!(run(&[r#"
array<int> $a = [1]; $b =& $a;
try { $b[0] = 'bad'; } catch (TypeError $e) { echo 'index|'; }
try { $b[] = 'bad'; } catch (TypeError $e) { echo 'append|'; }
try { $b = ['bad']; } catch (TypeError $e) { echo 'replace|'; }
function change(&$v) { $v[0] = 'bad'; }
try { change($b); } catch (TypeError $e) { echo 'call|'; }
function element(&$v) { $v = 'bad'; }
$key = '0';
try { element($b[$key]); } catch (TypeError $e) { echo 'element|'; }
echo $a[0], '|', count($a);
"#]), "index|append|replace|call|element|1|1");
}

/// Interface returns can use a concrete child with the same inherited generic arguments.
#[test]
fn generics_review_inherited_signature_returns() {
    assert_eq!(run(&[r#"
class Box<T> {} class Child<T> extends Box<T> {}
$parent = new Box<int>(); $child = new Child<int>();
interface Factory { public function make(): Box<int>; }
class Maker implements Factory { public function make(): Child<int> { return new Child<int>(); } }
echo get_class((new Maker())->make());
"#]), "Child<int>");
}

/// A generic trait cannot introduce a consumer into a covariant class surface.
#[test]
fn generics_review_trait_variance() {
    assert_eq!(run(&[r#"
trait Mutator<T> { public function put(T $v): void {} }
try { class Box<out T> { use Mutator<T>; } } catch (TypeError $e) { echo 'variance'; }
"#]), "variance");
}

/// Trait aliases and overrides determine the effective public variance surface.
#[test]
fn generics_review_trait_variance_uses_effective_visibility() {
    assert_eq!(run(&[r#"
trait Put<T> { public function put(T $value): void {} }
trait Hidden<T> { private function put(T $value): void {} }
class HiddenBox<out T> { use Put<T> { put as private; } }
class OverrideBox<out T> { use Put<T>; public function put(int $value): void {} }
try { class Exposed<out T> { use Hidden<T> { put as public; } } } catch (TypeError $e) { echo 'exposed'; }
"#]), "exposed");
}

/// Qualified visibility adaptations use the same concrete trait identity as composition.
#[test]
fn generics_review_qualified_trait_visibility() {
    assert_eq!(run(&[r#"
trait Put<T> { public function put(T $value): void {} }
trait Hidden<T> { private function put(T $value): void {} }
class Box<out T> { use Put<T> { pUt::put as private; } }
echo get_class(new Box<int>());
try { class Exposed<out T> { use Hidden<T> { Hidden::put as public; } } }
catch (TypeError $e) { echo '|exposed'; }
try { class Alias<out T> { use Put<T> { Put::put as private; Put::put as public exposed; } } }
catch (TypeError $e) { echo '|alias'; }
"#]), "Box<int>|exposed|alias");
}

/// Qualified selection resolves against all used traits before suppressing an unsafe method.
#[test]
fn generics_review_qualified_trait_selection() {
    assert_eq!(run(&[r#"
trait Get<T> { public function value(): T { throw new Exception('unused'); } }
trait Other<T> { public function value(T $value): T { return $value; } }
class Box<out T> { use Get<T>, Other<T> { Get::value insteadof Other; Get::value as copy; } }
echo get_class(new Box<int>());
try { class Alias<out T> { use Get<T>, Other<T> { Get::value insteadof Other; Other::value as exposed; } } }
catch (TypeError $e) { echo '|alias'; }
"#]), "Box<int>|alias");
}

/// Nested traits preserve qualified visibility changes at every import boundary.
#[test]
fn generics_review_nested_qualified_trait_visibility() {
    assert_eq!(run(&[r#"
trait Put<T> { public function put(T $value): void {} }
trait Inner<T> { use Put<T> { Put::put as private; } }
class Box<out T> { use Inner<T>; }
echo get_class(new Box<int>());
try { class Exposed<out T> { use Inner<T> { Inner::put as public; } } }
catch (TypeError $e) { echo '|exposed'; }
"#]), "Box<int>|exposed");
}

/// Class keyword casing must retain type-parameter evidence from constructor annotations.
#[test]
fn generics_review_docblock_keyword_case() {
    assert_eq!(run(&[r#"
CLASS Box<T> {
    /** @param T $v */ public function __construct(public $v) {}
    /** @return T */ public function get() { return $this->v; }
}

$box = new Box(1); echo get_class($box), '|', $box->get();
"#]), "Box<int>|1");
}

/// Shared storage retains contracts through chained aliases, copies, captures, and escaped references.
#[test]
fn generics_review_reference_contract_lifetimes() {
    assert_eq!(run(&[r#"
array<int> $a = [1]; $b =& $a; $c =& $b; $copy = $a;
$copy[0] = 'copy'; $c[0] = 2; unset($a);
try { $c[] = 'bad'; } catch (TypeError $e) { echo 'chain|'; }
$f = function() use (&$b) { $b = ['bad']; };
try { $f(); } catch (TypeError $e) { echo 'capture|'; }
function escaped() { array<int> $local = [3]; return [&$local]; }
$refs = escaped();
try { $refs[0] = ['bad']; } catch (TypeError $e) { echo 'escape|'; }
$refs[0] = [4]; echo $b[0], '|', $copy[0], '|', $refs[0][0];
"#]), "chain|capture|escape|2|copy|4");
}

/// Every typed alias contributes its own contract, and by-value copies remain independent.
#[test]
fn generics_review_reference_contract_intersection() {
    assert_eq!(run(&[r#"
array<int>|string $wide = [1]; array<int> $narrow = [2]; $narrow =& $wide;
try { $wide = 'bad'; } catch (TypeError $e) { echo 'intersection|'; }
$copy = $wide; $copy = 'ok';
try { array_push($wide, 'bad'); } catch (TypeError $e) { echo 'builtin|'; }
$wide[] = 3; echo $wide[0], '|', $narrow[1], '|', $copy;
"#]), "intersection|builtin|1|3|ok");
}

/// Reference rebinding validates the new source before changing the typed destination.
#[test]
fn generics_review_reference_rebinding_rejects_invalid_sources() {
    assert_eq!(run(&[r#"
array<int> $a = [1]; $b = ['bad'];
try { $a =& $b; } catch (TypeError $e) { echo 'value|'; }
try { $a =& $missing; } catch (TypeError $e) { echo 'undefined|'; }
echo $a[0]; $b[0] = 'independent'; echo '|', $b[0];
"#]), "value|undefined|1|independent");
}

/// Rejected redeclarations leave both existing and newly introduced alias contracts unchanged.
#[test]
fn generics_review_failed_declaration_preserves_reference_contracts() {
    assert_eq!(run(&[r#"
array<int> $a = [1]; $b =& $a;
try { array<string> $a = ['bad']; } catch (TypeError $e) { echo 'declared|'; }
try { array<string> $b = ['bad']; } catch (TypeError $e) { echo 'alias|'; }
$a = [2]; $b[] = 3; echo $a[0], '|', $b[1];
"#]), "declared|alias|2|3");
}

/// A typed local that aliases a global contributes its contract to that shared storage.
#[test]
fn generics_review_global_reference_contracts() {
    assert_eq!(run(&[r#"
$global = [1];
function bind_global() { global $global; array<int> $local = [2]; $local =& $global; }
bind_global();
try { $global[] = 'bad'; } catch (TypeError $e) { echo 'global|'; }
echo $global[0], '|', count($global);
"#]), "global|1|1");
}

/// Static aliases preserve persistent reference storage through valid and rejected assignments.
#[test]
fn generics_review_static_reference_contracts() {
    assert_eq!(run(&[r#"
class Holder { public static $value = null; }
function bind_static() { array<int> $local = [3]; Holder::$value =& $local; }
bind_static();
try { Holder::$value = ['bad']; } catch (TypeError $e) { echo 'static|'; }
try { Holder::$value[0] = 'bad'; } catch (TypeError $e) { echo 'index|'; }
echo Holder::$value[0], '|'; Holder::$value = [4]; echo Holder::$value[0];
"#]), "static|index|3|4");
}

/// Substituted ancestors support cold-cache covariant returns while incompatible slots stay rejected.
#[test]
fn generics_review_inherited_variance_returns() {
    assert_eq!(run(&[r#"
class Animal {} class Dog extends Animal {}
class Box<out T> {} class Child<T> extends Box<T> {}
interface Factory { public function make(): Box<Animal>; }
class Maker implements Factory { public function make(): Child<Dog> { return new Child<Dog>(); } }
echo get_class((new Maker())->make());
"#]), "Child<Dog>");
    for returned in ["Child<string>", "Child<float>"] {
        let source = format!("class Box<T> {{}} class Child<T> extends Box<T> {{}} interface Factory {{ public function make(): Box<int>; }} class Maker implements Factory {{ public function make(): {returned} {{ return new {returned}(); }} }}");
        let program = parse_fragment(source.as_bytes()).unwrap();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values), Err(EvalStatus::RuntimeFatal), "{returned}");
    }
}

/// Nested traits enforce both polarities and preserve constructor/private exclusions and method shadowing.
#[test]
fn generics_review_nested_trait_variance() {
    assert_eq!(run(&[r#"
trait Put<T> { public function put(T $value): void {} }
trait Nested<U> { use Put<U>; }
trait Get<T> { public function get(): T { throw new Exception('unused'); } }
trait State<T> { public T $value; }
trait Hidden<T> { private function put(T $value): void {} public function id<T>(T $value): T { return $value; } }
try { class Bad<out V> { use Nested<V>; } } catch (TypeError $e) { echo 'nested|'; }
try { class BadGet<in V> { use Get<V>; } } catch (TypeError $e) { echo 'return|'; }
try { class BadState<out V> { use State<V>; } } catch (TypeError $e) { echo 'property|'; }
class Good<out V> { use Hidden<V>; public function __construct(V $value) {} }
echo (new Good<int>(1))->id('ok');
"#]), "nested|return|property|ok");
}

/// Mixed-case interfaces and traits retain annotated member types during specialization.
#[test]
fn generics_review_all_classlike_keyword_cases() {
    assert_eq!(run(&[r#"
InTeRfAcE Read<T> { /** @return T */ public function get(); }
TrAiT ReadValue<T> { /** @return T */ public function get() { return $this->value; } }
cLaSs Box<T> implements Read<T> {
    use ReadValue<T>;
    /** @var T */ public $value;
    /** @param T $value */ public function __construct($value) { $this->value = $value; }
}
$box = new Box(5); echo $box->get();
$box->value = 'bad';
try { $box->get(); } catch (TypeError $e) { echo '|return'; }
"#]), "5|return");
}

/// Functions specialize independently for scalars, nested arrays, names, and spreads.
#[test]
fn generics_functions_infer_and_persist() {
    assert_eq!(run(&[
        "function identity<T>(T $value): T { return $value; } function first<T>(array<T> $items): T { return $items[0]; }",
        "echo identity(41), '|', identity('ok'), '|', first([[7]])[0], '|', identity(...['value' => 'spread']);",
    ]), "41|ok|7|spread");
}

/// Explicit arguments and defaults use the same concrete signature as inferred calls.
#[test]
fn generics_written_and_dependent_defaults() {
    assert_eq!(run(&[
        "function same<T, U = T>(T $a, U $b): U { return $b; } echo same<int>(1, 2); function emptyItems<T>(): array<T> { return []; } echo count(emptyItems<string>());",
    ]), "20");
}

/// Written and constructor-inferred class instantiations keep distinct identities and properties.
#[test]
fn generics_classes_have_concrete_identity() {
    assert_eq!(run(&[
        "class Box<T> { public function __construct(private T $value) {} public function get(): T { return $this->value; } }",
        "$i = new Box<int>(41); $s = new Box('ok'); echo $i->get(), '|', $s->get(), '|', get_class($i), '|', get_class($s);",
    ]), "41|ok|Box<int>|Box<string>");
}

/// Conflicting, missing, and excess generic arguments fail as catchable TypeErrors.
#[test]
fn generics_binding_errors_are_catchable() {
    assert_eq!(run(&[
        "function pair<T>(T $a, T $b): T { return $a; } function missing<T>(int $a): int { return $a; }",
        "try { pair(1, 'x'); } catch (TypeError $e) { echo 'conflict|'; } try { missing(1); } catch (TypeError $e) { echo 'missing|'; } try { pair<int, string>(1, 2); } catch (TypeError $e) { echo 'excess'; }",
    ]), "conflict|missing|excess");
}

/// Bounds use the class/interface hierarchy rather than structural resemblance.
#[test]
fn generics_bounds_accept_subclasses() {
    assert_eq!(run(&[
        "class Base { public function id(): int { return 7; } } class Child extends Base {} function id<T: Base>(T $value): int { return $value->id(); } echo id(new Child()); try { id(1); } catch (TypeError $e) { echo '|bound'; }",
    ]), "7|bound");
}

/// Structural array contracts check all values on both argument and return boundaries.
#[test]
fn generics_typed_arrays_validate_elements() {
    assert_eq!(run(&[
        "function ints(array<int> $items): int { return count($items); } echo ints([1, 2]); try { ints(['x']); } catch (TypeError $e) { echo '|argument'; } function bad(): array<int> { return ['x']; } try { bad(); } catch (TypeError $e) { echo '|return'; }",
    ]), "2|argument|return");
}

/// Generic prototypes preserve type parameters in namespaced declarations.
#[test]
fn generics_namespace_imports_survive_specialization() {
    assert_eq!(run(&[
        "namespace Model; class Thing { public function get(): int { return 9; } }",
        "namespace Work; use Model\\Thing; function get<T: Thing>(T $value): int { return $value->get(); } echo get(new Thing());",
    ]), "9");
}

/// Runtime strict PHP mode rejects native generic syntax before any source statements execute.
#[test]
fn generics_native_syntax_rejected_in_strict_mode() {
    let _guard = crate::strict_php_mode::scoped_enable();
    let mut values = FakeOps::default();
    let mut scope = ElephcEvalScope::new();
    let program = parse_fragment(b"echo 'before'; function id<T>(T $v): T { return $v; }").unwrap();
    assert_eq!(execute_program(&program, &mut scope, &mut values), Err(EvalStatus::ParseError));
    assert_eq!(values.output, "");
}

/// A generic method has independent slots, including callable signature inference.
#[test]
fn generics_methods_and_typed_callables() {
    assert_eq!(run(&[
        "class Box<T> { public function __construct(private T $value) {} public function get(): T { return $this->value; } public function map<U>(callable(T): U $f): Box<U> { return new Box<U>($f($this->value)); } public static function id<V>(V $v): V { return $v; } }",
        "$b = new Box('hello'); echo $b->map(function(string $s): int { return strlen($s); })->get(), '|', $b->map<string>(function(string $s): string { return strtoupper($s); })->get(), '|', Box<int>::id<string>('ok');",
    ]), "5|HELLO|ok");
}

/// Portable template annotations specialize functions and member types in both PHP modes.
#[test]
fn generics_docblock_templates_in_strict_mode() {
    for strict in [false, true] {
        let _guard = strict.then(crate::strict_php_mode::scoped_enable);
        let output = run(&[
            "/** @template T */\nclass Box { /** @var T */ private $value; /** @param T $value */ public function __construct($value) { $this->value = $value; } /** @return T */ public function get() { return $this->value; } }\n/**\n * @template U\n * @param U $value\n * @return U\n */\nfunction identity($value) { return $value; }",
            "$b = new Box(7); echo $b->get(), '|', identity('ok');",
        ]);
        assert_eq!(output, "7|ok");
    }
}

/// Substitution visits type positions without renaming methods or constants named like a slot.
#[test]
fn generics_substitution_preserves_non_type_names() {
    assert_eq!(run(&[
        "class User { const T = 'constant'; public function T(): string { return 'method'; } } function label<T>(T $value): string { return $value->T() . User::T . 'T'; } echo label(new User());",
    ]), "methodconstantT");
}

/// Generic interfaces and traits are instantiated before implementing class validation.
#[test]
fn generics_interfaces_and_traits() {
    assert_eq!(run(&[
        "interface Getter<T> { public function get(): T; } trait Stored<T> { public T $value; public function get(): T { return $this->value; } } class IntBox implements Getter<int> { use Stored<int>; public function __construct(int $value) { $this->value = $value; } } echo (new IntBox(9))->get();",
    ]), "9");
}

/// Constructor-local parameters specialize promoted storage independently of class slots.
#[test]
fn generics_constructor_slots_and_shadowing() {
    assert_eq!(run(&[
        "class Item { public function __construct<T>(private T $value) {} public function get() { return $this->value; } } class Pair<T> { public function __construct<U>(private T $left, private U $right) {} public function right() { return $this->right; } } class Shadow<T> { public function __construct<T>(private T $value) {} public function get() { return $this->value; } }",
        "$a = new Item(7); $b = new Pair<int>(1, 'two'); $c = new Shadow<int>('shadow'); echo $a->get(), '|', get_class($a), '|', $b->right(), '|', get_class($b), '|', $c->get(), '|', get_class($c);",
    ]), "7|Item<int>|two|Pair<int, string>|shadow|Shadow<int, string>");
}

/// Imported generic function aliases retain explicitly written types in their own namespace.
#[test]
fn generics_function_aliases_and_enum_methods() {
    assert_eq!(run(&[
        "namespace Lib; function id<T>(T $v): T { return $v; }",
        "namespace App; use function Lib\\id as alias; echo alias<string>('ok');",
    ]), "ok");
    assert_eq!(run(&["enum Pick { case One; public function id<T>(T $v): T { return $v; } } echo Pick::One->id<int>(7);"]), "7");
}

/// Case-insensitive direct and first-class imports bind written slots in persistent contexts.
#[test]
fn generics_mixed_case_function_aliases_and_callables() {
    assert_eq!(run(&[
        "namespace Lib; function Identity<T>(T $v): T { return $v; }",
        "namespace App; use function Lib\\Identity as Alias; echo aLIAS<string>('direct'), '|'; $f = ALias<int>(...); echo $f(42);",
        "namespace Other; echo '|', \\Lib\\IDENTITY<int>(7);",
    ]), "direct|42|7");
    assert_eq!(run(&[
        "function Identity<T>(T $v): T { return $v; }",
        "namespace App; echo iDENTITY<int>(8), '|'; $f = IDentity<string>(...); echo $f('fallback');",
    ]), "8|fallback");
}

/// Variance widens object arguments and keeps AOT's scalar storage restriction.
#[test]
fn generics_variance_accepts_safe_object_widening() {
    assert_eq!(run(&[
        "class Animal {} class Dog extends Animal {} class Box<out T> { public function __construct(private T $value) {} public function get(): T { return $this->value; } } class Sink<in T> { public function put(T $v): void {} } function read(Box<Animal> $box): string { return get_class($box->get()); } function write(Sink<Dog> $sink): string { $sink->put(new Dog()); return 'sink'; } echo read(new Box<Dog>(new Dog())), '|', write(new Sink<Animal>()); function mixedBox(Box<mixed> $box): void {} try { mixedBox(new Box<int>(7)); } catch (TypeError $e) { echo '|storage'; }",
    ]), "Dog|sink|storage");
}

/// Own, inherited, writable-property, and callable polarity violations are rejected.
#[test]
fn generics_variance_declarations_are_checked() {
    for declaration in [
        "class Bad<+T> { public function put(T $value): void {} }",
        "class Bad<+T> { public T $value; }",
        "class Bad<-T> { public function get(): T { return null; } }",
        "class Bad<+T> { public function get(): callable(T): int { return null; } }",
        "class ParentBox<T> { public function put(T $v): void {} } class Bad<+T> extends ParentBox<T> {}",
    ] {
        assert_eq!(run(&[&format!("try {{ {declaration} }} catch (TypeError $e) {{ echo 'variance'; }}")]), "variance");
    }
}

/// Unconditional generic declarations can be referenced before their source position.
#[test]
fn generics_forward_calls_are_registered() {
    assert_eq!(run(&["echo id(7); function id<T>(T $v): T { return $v; }"]), "7");
}

/// Substitution covers local types, nested closure signatures, and subsequent array writes.
#[test]
fn generics_local_and_closure_types_are_preserved() {
    assert_eq!(run(&[
        "function keep<T>(T $v): T { T $held = $v; $f = function(T $arg): T { return $arg; }; array<T> $items = [$held]; try { $held = 'wrong'; } catch (TypeError $e) { echo 'local|'; } try { $items[] = 'wrong'; } catch (TypeError $e) { echo 'append|'; } try { $items[0] = 'wrong'; } catch (TypeError $e) { echo 'index|'; } return $f($items[0]); } echo keep(7);",
    ]), "local|append|index|7");
}

/// Nullable evidence and constructor defaults do not infer from null or empty storage.
#[test]
fn generics_nullable_inference_and_empty_constructor_evidence() {
    assert_eq!(run(&[
        "function id<T>(?T $v): ?T { return $v; } echo id(7); class Box<T = string> { public function __construct(private ?T $v) {} } echo '|', get_class(new Box(null)); class Required<T> { public function __construct(T $v) {} } try { new Required(null); } catch (TypeError $e) { echo '|null'; } try { new Required([]); } catch (TypeError $e) { echo '|empty'; }",
    ]), "7|Box<string>|null|empty");
}

/// Serialized templates preserve lexical imports, type-use indexes, and strict provenance.
#[test]
fn generics_cached_templates_round_trip() {
    let program = parse_fragment(b"namespace App; class Box<T> { public function __construct(private T $v) {} public function get(): T { return $this->v; } } echo (new Box<int>(7))->get();").unwrap();
    let bytes = bincode::serialize(&program).unwrap();
    let loaded: EvalProgram = bincode::deserialize(&bytes).unwrap();
    assert_eq!(loaded, program);
    let mut values = FakeOps::default();
    let mut scope = ElephcEvalScope::new();
    let result = execute_program(&loaded, &mut scope, &mut values).unwrap();
    values.release(result).unwrap();
    assert_eq!(values.output, "7");
    let _guard = crate::strict_php_mode::scoped_enable();
    assert_eq!(execute_program(&loaded, &mut scope, &mut values), Err(EvalStatus::ParseError));
}

/// PHPDoc annotations respect attributes, native class slots, and ignored unsupported grammar.
#[test]
fn generics_docblocks_keep_attributes_and_native_class_scope() {
    assert_eq!(run(&[
        "class Box<T> { /** @param T $value */ public function __construct(private $value) {} /** @return T */ public function get() { return $this->value; } } echo (new Box<int>(7))->get(); /**\n * @template U\n * @param U $value\n * @return U\n */ #[Deprecated] function identity($value) { return $value; } echo '|', identity('ok'); /** @template X of unsupported[] */ function ordinary($v) { return $v; } echo '|', ordinary(8);",
    ]), "7|ok|8");
}

/// Bare type hints and bare inheritance use the declared bound before the default.
#[test]
fn generics_bare_types_and_inheritance_defaults() {
    assert_eq!(run(&[
        "class Animal {} class Dog extends Animal {} class Repo<out T: Animal = Dog> { public function __construct(private T $v) {} public function get(): T { return $this->v; } } function read(Repo $repo): string { return get_class($repo->get()); } interface Name<T = string> { public function name(): T; } class Named implements Name { public function name(): string { return 'named'; } } trait Store<T = int> { public T $v; } trait Nested { use Store; } class Holder { use Nested; } echo read(new Repo<Dog>(new Dog())), '|', (new Named())->name(); $h = new Holder(); $h->v = 7; echo '|', $h->v;",
    ]), "Dog|named|7");
}

/// Ordinary scalar types produced by specialization retain catchable contract failures.
#[test]
fn generics_specialized_scalar_contracts_throw_type_errors() {
    assert_eq!(run(&[
        "function id<T>(T $v): T { return $v; } function bad<T>(T $v): T { return []; } try { id<int>([]); } catch (TypeError $e) { echo 'argument|'; } try { bad(7); } catch (TypeError $e) { echo 'return'; }",
    ]), "argument|return");
}

/// Specialized methods retain the source spelling of magic constants and trait origins.
#[test]
fn generics_magic_constants_keep_source_names() {
    assert_eq!(run(&[
        "function label<T>(T $v): string { return __FUNCTION__; } trait Label { public function label<U>(U $v): string { return __TRAIT__ . ':' . __METHOD__; } } class Box<T> { use Label; public function __construct(T $v) {} public function source(): string { return __CLASS__ . ':' . __METHOD__; } } $b = new Box<int>(7); echo label(7), '|', $b->source(), '|', $b->label<string>('x');",
    ]), "label|Box:Box::source|Label:Label::label");
}

/// Trait adaptation qualifiers select the actual concrete trait, as they do in AOT.
#[test]
fn generics_trait_adaptations_select_specializations() {
    assert_eq!(run(&[
        "trait Label<T> { public function label(T $v): T { return $v; } } trait Other { public function label(int $v): int { return 0; } } class Box { use Label<int>, Other { Label::label insteadof Other; Label::label as renamed; } } $b = new Box(); echo $b->label(7), '|', $b->renamed(8);",
    ]), "7|8");
}

/// Recursive type growth terminates with a catchable error and leaves later calls usable.
#[test]
fn generics_recursive_growth_is_bounded() {
    assert_eq!(run(&[
        "function recurse<T>(T $v): void { recurse([$v]); } try { recurse(7); } catch (TypeError $e) { echo 'depth|'; } function id<T>(T $v): T { return $v; } echo id(8);",
    ]), "depth|8");
}

/// Constructor slots of an ordinary class may also be supplied explicitly.
#[test]
fn generics_written_constructor_slots_of_ordinary_class() {
    assert_eq!(run(&[
        "class Item { public function __construct<T>(public T $value) {} } $item = new Item<string>('ok'); echo $item->value, '|', get_class($item); try { new Item<int>([]); } catch (TypeError $e) { echo '|type'; }",
    ]), "ok|Item<string>|type");
}

/// Static writes create their selected class and static state remains separate per instantiation.
#[test]
fn generics_static_storage_is_per_instantiation() {
    assert_eq!(run(&[
        "class Counter<T> { public static int $made = 0; public function __construct(T $v) { static::$made++; } public static function of(T $v): Counter<T> { return new Counter<T>($v); } } Counter<int>::$made = 5; new Counter<int>(7); new Counter<string>('x'); echo Counter<int>::$made, '|', Counter<string>::$made, '|', get_class(Counter::of(8));",
    ]), "6|1|Counter<int>");
}

/// Native class slots take precedence over conflicting PHPDoc template declarations.
#[test]
fn generics_docblocks_preserve_native_precedence() {
    assert_eq!(run(&[
        "/** @template T */ class Box<U> { /** @var T */ public $ignored; /** @param U $value */ public function __construct(public $value) {} } $box = new Box(7); $box->ignored = 'free'; echo $box->value, '|', $box->ignored;",
    ]), "7|free");
}

/// Inline local annotations remain inert, matching AOT's property-only PHPDoc var handling.
#[test]
fn generics_docblocks_leave_inline_local_var_inert() {
    let _guard = crate::strict_php_mode::scoped_enable();
    assert_eq!(run(&[
        "/** @template T */ class Box { /** @param T $value */ public function __construct(public $value) {} public function inspect(): string { /** @var T $local */ $local = 'free'; return $local; } } echo (new Box(7))->inspect();",
    ]), "free");
}

/// Callable aliases, explicit first-class callables, and named callback arrays specialize lazily.
#[test]
fn generics_callbacks_share_specializations() {
    assert_eq!(run(&[
        "function id<T>(T $v): T { return $v; } $f = id<int>(...); echo $f(7), '|'; $f = 'id'; echo $f('alias'), '|', call_user_func('id', 8), '|', call_user_func_array('id', ['v' => 'named']), '|', is_callable('id');",
    ]), "7|alias|8|named|1");
}

/// Generated-code context calls see templates before any interpreter call instantiated them.
#[test]
fn generics_context_entry_points_specialize_templates() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let program = parse_fragment(b"function id<T>(T $v): T { return $v; } class Box<T> { public const LABEL = 'box'; public static int $made = 0; public function __construct(public T $v) {} public static function make(T $v): Box<T> { return new Box<T>($v); } }").unwrap();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    let integer = values.int(7).unwrap();
    let result = execute_context_function(&mut context, "id", vec![integer], &mut values).unwrap();
    assert_eq!(values.get(result), FakeValue::Int(7));
    let EvalOutcome::Value(label) = execute_context_class_constant_fetch(&mut context, "Box", "LABEL", &mut values).unwrap() else { panic!("expected constant"); };
    assert_eq!(values.get(label), FakeValue::String("box".into()));
    execute_context_static_property_set(&mut context, "Box", "made", integer, &mut values).unwrap();
    let EvalOutcome::Value(made) = execute_context_static_property_get(&mut context, "Box", "made", &mut values).unwrap() else { panic!("expected static value"); };
    assert_eq!(values.get(made), FakeValue::Int(7));
    let EvalOutcome::Value(object) = execute_context_new_object_outcome(&mut context, "Box", vec![integer], &mut values).unwrap() else { panic!("expected object"); };
    let identity = values.object_identity(object).unwrap();
    assert_eq!(context.dynamic_object_class_name(identity).as_deref(), Some("Box<int>"));
    let string = values.string("ok").unwrap();
    let EvalOutcome::Value(object) = execute_context_static_method_call_outcome(&mut context, "Box", "make", vec![string], &mut values).unwrap() else { panic!("expected object"); };
    let identity = values.object_identity(object).unwrap();
    assert_eq!(context.dynamic_object_class_name(identity).as_deref(), Some("Box<string>"));
    let EvalOutcome::Throwable(error) = execute_context_function_outcome(&mut context, "id<int>", vec![string], &mut values).unwrap() else { panic!("expected type error"); };
    assert!(values.object_is_a(error, "TypeError", false).unwrap());
}

/// Function and class template names occupy separate PHP symbol namespaces.
#[test]
fn generics_function_and_class_namespaces_are_separate() {
    assert_eq!(run(&[
        "function Box<T>(T $v): T { return $v; } class Box<T> { public function __construct(public T $v) {} } function Item(): string { return 'function'; } class Item<T> { public function __construct(public T $v) {} } class Plain {} function Plain<T>(T $v): T { return $v; } echo Box(7), '|', (new Box('class'))->v, '|', Item(), '|', (new Item(8))->v, '|', Plain(9);",
    ]), "7|class|function|8|9");
}

/// Ordinary declarations cannot overwrite a retained template of their symbol kind.
#[test]
fn generics_ordinary_redefinitions_are_rejected() {
    for (declaration, duplicate) in [
        ("function id<T>(T $v): T { return $v; }", "function id($v) { return $v; }"),
        ("class Box<T> {}", "class Box {}"),
        ("interface Read<T> {}", "interface Read {}"),
        ("trait Store<T> {}", "trait Store {}"),
    ] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let program = parse_fragment(declaration.as_bytes()).unwrap();
        execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
        let program = parse_fragment(duplicate.as_bytes()).unwrap();
        assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values), Err(EvalStatus::RuntimeFatal));
    }
}

/// Nullable and union array locals accept valid writes and preserve rejected values.
#[test]
fn generics_nullable_and_union_array_local_writes() {
    for ty in ["?array<int>", "array<int>|null", "null|array<int>", "array<int>|string"] {
        let source = format!("{ty} $items = [1]; $items[] = 2; $items[0] = 3; try {{ $items[] = 'bad'; }} catch (TypeError $e) {{ echo 'append|'; }} try {{ $items[0] = 'bad'; }} catch (TypeError $e) {{ echo 'index|'; }} echo $items[0], '|', $items[1], '|', count($items);");
        assert_eq!(run(&[&source]), "append|index|3|2|2", "{ty}");
    }
    assert_eq!(run(&[
        "array<int>|array<string> $items = [1]; try { $items[] = 'bad'; } catch (TypeError $e) { echo 'arm|'; } $items[] = 2; echo count($items); ?array $bare = []; $bare[] = 'ok'; echo '|', $bare[0]; ?array<int> $nullable = null; $nullable[] = 7; echo '|', $nullable[0];",
    ]), "arm|2|ok|7");
}

/// Indexed array writes reject string keys before promotion while numeric strings remain valid.
#[test]
fn generics_indexed_array_local_rejects_associative_key() {
    assert_eq!(run(&[
        "array<int> $items = [1]; try { $items['k'] = 2; } catch (TypeError $e) { echo 'key|'; } $items['0'] = 3; $items[] = 4; echo $items[0], '|', $items[1], '|', count($items); array<string, int> $map = ['k' => 1]; $map['more'] = 2; try { $map[0] = 3; } catch (TypeError $e) { echo '|map'; } echo '|', count($map);",
    ]), "key|3|4|2|map|2");
}

/// Indexed array contracts reject associative values at local, parameter and return boundaries.
#[test]
fn generics_indexed_array_value_contract_rejects_associative_storage() {
    assert_eq!(run(&[
        "function length(array<int> $items): int { return count($items); } function bad(): array<int> { return ['k' => 1]; } try { length(['k' => 1]); } catch (TypeError $e) { echo 'argument|'; } try { bad(); } catch (TypeError $e) { echo 'return|'; } try { array<int> $items = ['k' => 1]; } catch (TypeError $e) { echo 'local'; }",
    ]), "argument|return|local");
}

/// Constructor identity retains class arguments for variance, inference and nested bounds.
#[test]
fn generics_constructor_specializations_preserve_class_variance() {
    assert_eq!(run(&[
        "class Animal {} class Dog extends Animal {} class Box<out T> { public function __construct<U>(private T $value, private U $label) {} public function get(): T { return $this->value; } } function read(Box<Animal> $box): string { return get_class($box->get()); } function relay<V>(Box<V> $box): V { return $box->get(); } function bounded<V: Box<Animal>>(V $box): string { return read($box); } $i = new Box(new Dog(), 1); $s = new Box<Dog>(new Dog(), 'label'); echo read($i), '|', read($s), '|', get_class(relay($i)), '|', bounded($s), '|', get_class($i), '|', get_class($s);",
    ]), "Dog|Dog|Dog|Dog|Box<Dog, int>|Box<Dog, string>");
}

/// Constructor slots leave nested inference, contravariance and invariant rejection intact.
#[test]
fn generics_constructor_slots_preserve_nested_type_contracts() {
    assert_eq!(run(&[
        "class Animal {} class Dog extends Animal {} class Box<out T> { public function __construct<U>(private T $value, private U $label) {} public function get(): T { return $this->value; } } function nested<V>(Box<Box<V>> $box): V { return $box->get()->get(); } $inner = new Box(new Dog(), 1); echo get_class(nested(new Box($inner, 'outer'))); class Sink<in T> { public function __construct<U>(private U $label) {} public function put(T $value): void {} } function write(Sink<Dog> $sink): string { $sink->put(new Dog()); return 'sink'; } echo '|', write(new Sink<Animal>(1)); class Invariant<T> { public function __construct<U>(private T $value, private U $label) {} } function reject(Invariant<Animal> $value): void {} try { reject(new Invariant(new Dog(), 1)); } catch (TypeError $e) { echo '|invariant'; } class Item { public function __construct<U>(public U $value) {} } function item(Item<int> $value): int { return $value->value; } echo '|', item(new Item(7)); try { item(new Item('bad')); } catch (TypeError $e) { echo '|constructor'; }",
    ]), "Dog|sink|invariant|7|constructor");
}

/// Fully written constructor identities work before and after inferred cache entries exist.
#[test]
fn generics_written_class_and_constructor_slots_are_order_independent() {
    for inferred_first in [false, true] {
        let warmup = if inferred_first { "$warm = new Pair<int>(1, 'warm');" } else { "" };
        let source = format!("class Pair<T> {{ public function __construct<U>(public T $left, public U $right) {{}} }} {warmup} $pair = new Pair<int, string>(1, 'two'); echo get_class($pair), '|', $pair->right; try {{ new Pair<int, string>(1, []); }} catch (TypeError $e) {{ echo '|value'; }}");
        assert_eq!(run(&[&source]), "Pair<int, string>|two|value");
    }
    assert_eq!(run(&[
        "class Pair<T> { public function __construct<U>(public T $left, public U $right) {} }",
        "class Child extends Pair<int, string> {} $child = new Child(3, 'child'); echo $child->left, '|', $child->right, '|', get_parent_class($child);",
    ]), "3|child|Pair<int, string>");
}

/// Combined written lists retain dependent constructor defaults, bounds and lexical shadowing.
#[test]
fn generics_written_constructor_slots_preserve_binding_rules() {
    assert_eq!(run(&[
        "class Base {} class Child extends Base {} class Pair<T> { public function __construct<U: Base, V = U>(public T $left, public U $right, public V $other) {} } $pair = new Pair<int, Child>(1, new Child(), new Child()); echo get_class($pair); try { new Pair<int, int>(1, 2, 3); } catch (TypeError $e) { echo '|bound'; } try { new Pair<int, Child, Child, string>(1, new Child(), new Child()); } catch (TypeError $e) { echo '|arity'; } class Shadow<T> { public function __construct<T>(public T $value) {} } $shadow = new Shadow<int, string>('ok'); echo '|', get_class($shadow), '|', $shadow->value;",
    ]), "Pair<int, Child, Child>|bound|arity|Shadow<int, string>|ok");
}

/// Typed array keys use PHP's integer normalization for floats and booleans before validation.
#[test]
fn generics_array_local_accepts_normalized_scalar_keys() {
    for ty in ["array<int>", "array<int, int>", "?array<int>", "array<int>|null"] {
        let source = format!("{ty} $items = [1, 2]; $items[1.0] = 3; $items[true] = 4; $items[false] = 5; $items[-0.0] = 6; $items[2.0] = 7; $items[1.9] = 8; try {{ $items[false] = 'bad'; }} catch (TypeError $e) {{ echo 'value|'; }} echo $items[0], '|', $items[1], '|', $items[2], '|', count($items);");
        assert_eq!(run(&[&source]), "value|6|8|7|3", "{ty}");
    }
    assert_eq!(run(&[
        "array<string, int> $map = ['k' => 1]; try { $map[1.0] = 2; } catch (TypeError $e) { echo 'float|'; } try { $map[false] = 3; } catch (TypeError $e) { echo 'bool|'; } echo count($map);",
    ]), "float|bool|1");
}

/// Null keys validate as empty strings without changing the caller key or array copies.
#[test]
fn generics_array_local_accepts_null_string_keys() {
    assert_eq!(run(&[
        "array<string, int> $map = ['k' => 1]; $original = $map; $key = null; $map[$key] = 2; $map[null] = 3; echo $map[''], '|', count($map), '|', count($original), '|', is_null($key); ?array<string, int> $nullable = null; $nullable[null] = 4; array<string, int>|null $union = null; $union[null] = 5; echo '|', $nullable[''], '|', $union['']; array<int> $indexed = [1]; try { $indexed[null] = 2; } catch (TypeError $e) { echo '|indexed'; } try { $map[null] = 'bad'; } catch (TypeError $e) { echo '|value'; } echo '|', $map[''];",
    ]), "3|2|1|1|4|5|indexed|value|3");
}

/// Scalar union arms reject array writes after evaluating operands and preserve their values.
#[test]
fn generics_non_array_union_writes_preserve_scalars() {
    assert_eq!(run(&[
        "function rhs(&$calls): int { $calls++; return 9; } $calls = 0; array<int>|string $items = 'keep'; try { $items[] = rhs($calls); } catch (Error $e) { echo $e->getMessage(), '|'; } echo $items, '|', $calls; foreach ([true, 7, 1.5] as $value) { array<int>|bool|int|float $scalar = $value; try { $scalar[] = rhs($calls); } catch (Error $e) { echo '|append'; } try { $scalar[0] = rhs($calls); } catch (Error $e) { echo '|index'; } echo '|', $scalar; } echo '|', $calls; array<int>|bool|null $empty = false; $empty[] = 1; $empty = null; $empty[0] = 2; echo '|', $empty[0];",
    ]), "[] operator not supported for strings|keep|1|append|index|1|append|index|7|append|index|1.5|7|2");
}

/// String union arms update bytes through references while preserving by-value copies.
#[test]
fn generics_string_union_offset_writes_preserve_strings() {
    assert_eq!(run(&[
        "array<int>|string $items = 'keep'; $original = $items; $alias =& $items; $items[0] = 9; $alias[-1] = 'X'; $items['01'] = 'Y'; echo $items, '|', $original, '|', $alias; try { $items[] = 1; } catch (Error $e) { echo '|append'; } try { $items['bad'] = 1; } catch (TypeError $e) { echo '|key'; } try { $items[0] = ''; } catch (Error $e) { echo '|empty'; } echo '|', $items; $items[6] = 'Z'; echo '|', $items;",
    ]), "9YeX|keep|9YeX|append|key|empty|9YeX|9YeX  Z");
}

/// Offset writes observe replacements made by either operand before choosing receiver semantics.
#[test]
fn generics_local_offsets_observe_operand_reassignments() {
    assert_eq!(run(&[r#"
function replace(&$target, $next, $result) { $target = $next; return $result; }
array<int>|string|int|null $items = 'ab'; $original = $items;
$items[replace($items, 'xy', 0)] = 'Z'; echo $items, '|', $original;
array<int>|string|int $scalar = 'ab';
try { $scalar[replace($scalar, 7, 0)] = 'Z'; } catch (Error $e) { echo '|scalar'; }
echo '|', $scalar;
array<int>|string|null $append = null;
try { $append[] = replace($append, 'ab', 'value'); } catch (Error $e) { echo '|append'; }
echo '|', $append;
array<int>|string $rhs = 'ab';
$rhs[0] = replace($rhs, 'xy', 'Z'); echo '|', $rhs;
array<int> $indexed = [0];
$indexed[replace($indexed, [1, 2], 0)] = 3; echo '|', $indexed[0], '|', $indexed[1];
array<int> $appended = [0];
$appended[] = replace($appended, [4, 5], 6); echo '|', $appended[0], '|', $appended[1], '|', $appended[2];
array<mixed, int>|string $map = 'ab';
$map[replace($map, [7], '01')] = 8; echo '|', $map[0], '|', $map['01'], '|', count($map);
$map['01'] = replace($map, 'xy', 'Z'); echo '|', $map;
function stage1(&$target, &$order) { $order .= 'i'; $target = 'xy'; return 0; }
function stage2(&$target, &$order) { $order .= 'v'; $target = 'uv'; return 'Z'; }
array<int>|string $ordered = 'ab'; $order = '';
$ordered[stage1($ordered, $order)] = stage2($ordered, $order);
echo '|', $ordered, '|', $order;
"#]), "Zy|ab|scalar|7|append|ab|Zy|3|2|4|5|6|7|8|2|xZ|Zv|iv");
}

/// Failed index and RHS evaluation preserve their reassignments without applying the pending write.
#[test]
fn generics_offset_operand_failures_preserve_live_values() {
    assert_eq!(run(&[r#"
function reject(&$target, $next) { $target = $next; throw new Exception('stop'); }
array<int>|string $items = 'ab';
try { $items[reject($items, 'xy')] = 'Z'; } catch (Exception $e) { echo 'index|'; }
echo $items;
try { $items[0] = reject($items, 'uv'); } catch (Exception $e) { echo '|rhs'; }
echo '|', $items;
"#]), "index|xy|rhs|uv");
}

/// PHP string offsets include vertical tab among leading and trailing numeric whitespace.
#[test]
fn generics_string_offsets_accept_vertical_tab() {
    assert_eq!(run(&[r#"
array<int>|string $items = 'keep';
$items["\v1"] = 'Q'; $items["\v1x"] = 'R'; $items["1\v"] = 'S';
echo $items;
"#]), "kSep");
}

/// Escaped failed receivers retain their class and properties but never run their destructors.
#[test]
fn generics_failed_construction_preserves_escaped_metadata() {
    assert_eq!(run(&[
        "class Saved { public static $value = null; } class Failed<T> { public function __construct(public T $value) { Saved::$value = $this; throw new Exception('failed'); } public function __destruct() { echo 'bad-destructor|'; } } try { new Failed<int>(7); } catch (Exception $e) { echo $e->getMessage(), '|'; } $saved = Saved::$value; Saved::$value = null; echo get_class($saved), '|', $saved->value; unset($saved);",
    ]), "failed|Failed<int>|7");
}

/// Property-default errors consume the partial receiver without calling its destructor.
#[test]
fn generics_failed_property_default_releases_receiver() {
    let program = parse_fragment(b"class Secret { private const VALUE = 1; } class Broken<T> { public $first = 'allocated'; public $value = Secret::VALUE; public function __construct(T $value) {} public function __destruct() { echo 'bad'; } } try { new Broken<int>(3); } catch (Error $e) { echo 'default'; }").unwrap();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    values.release(result).unwrap();
    assert_eq!(values.output, "default");
    let receiver = values.values.keys().find(|identity| context.dynamic_object_class_name(**identity as u64).as_deref() == Some("Broken<int>")).copied().expect("partial receiver was allocated");
    assert_eq!(values.cell_owners.get(&receiver), Some(&0));
}
