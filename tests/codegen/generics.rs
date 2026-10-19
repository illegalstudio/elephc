//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of `array<T>` type
//! arguments, including parameter and return positions, declared locals, and the element
//! types whose ABI differs (register-width int, two-register string, heap pointer).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries; a declared element type must survive to
//!   the callee instead of collapsing to boxed `mixed`, which is the whole point of the
//!   annotation.

use crate::support::*;

/// Compiles `source` through the real CLI with `--emit-asm` and returns the user assembly.
///
/// A fresh process per call: the in-process library codegen path sets process-global state
/// (null repr, compile profile, linked extensions), so a fixture compiled in-process can observe a
/// previous test's settings. The CLI is isolated and deterministic.
fn compile_cli_to_asm(tag: &str, source: &str) -> String {
    let dir = std::env::temp_dir().join(format!("elephc_asm_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let php = dir.join("main.php");
    fs::write(&php, source).unwrap();
    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg(&php)
        .output()
        .expect("run elephc CLI");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let asm = fs::read_to_string(dir.join("main.s")).unwrap();
    let _ = fs::remove_dir_all(&dir);
    asm
}

/// A written `array<mixed>` method parameter is a contract, not a hint sharpened by its first call.
///
/// The method path rewrote its stored signature to the first call's array shape, a rule meant for
/// a bare `array`; since a bare `array` became a union, only a WRITTEN `array<mixed>` still reached
/// it, and the second call was refused with `expects array<int>, got array<string>`. Instance,
/// static and `parent::` dispatch each had their own copy. A bare `array` still keeps its
/// associative shape.
#[test]
fn test_written_array_mixed_method_param_accepts_every_element_type() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Base { public function n(array<mixed> $a): int { return count($a); } }
class K extends Base {
    public function g(array<mixed> $a): int { return count($a); }
    public static function s(array<mixed> $a): int { return count($a); }
    public function viaParent(array<string> $s): int { return parent::n([1, 2]) + parent::n($s); }
    public function keys(array $m): string { return implode(",", array_keys($m)); }
}
function go(): void {
    $ints = [3, 1, 2];
    $strs = ["a", "b"];
    $k = new K();
    $t = 0;
    for ($i = 0; $i < 50; $i++) {
        $t += $k->g($ints) + $k->g($strs) + K::s($ints) + K::s($strs) + $k->viaParent($strs);
    }
    echo $t, "|", count($ints), "|", $k->keys(["x" => 1, "y" => 2]);
}
go();
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "700|3|x,y", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A declared `array<int>` parameter carries its element type into the callee.
#[test]
fn test_array_type_argument_int_param() {
    let out = compile_and_run(
        "<?php function firstOf(array<int> $a): int { return $a[0]; } echo firstOf([7, 8, 9]);",
    );
    assert_eq!(out, "7");
}

/// `string` is the element type whose ABI differs most from `int` (16 bytes, two registers),
/// so it exercises a different storage path than the integer case.
#[test]
fn test_array_type_argument_string_param() {
    let out = compile_and_run(
        "<?php function firstOf(array<string> $a): string { return $a[0]; } echo firstOf([\"a\", \"b\"]);",
    );
    assert_eq!(out, "a");
}

/// Two functions with DIFFERENT declared element types coexist in one program. Without the
/// annotation a single untyped `firstOf` called with both would widen to `array<mixed>`.
#[test]
fn test_array_type_argument_distinct_element_types_coexist() {
    let out = compile_and_run(
        "<?php \
         function firstInt(array<int> $a): int { return $a[0]; } \
         function firstStr(array<string> $a): string { return $a[0]; } \
         echo firstInt([1, 2]), firstStr([\"x\", \"y\"]);",
    );
    assert_eq!(out, "1x");
}

/// `array<T>` is valid in return position and the element type survives a round trip.
#[test]
fn test_array_type_argument_return_position() {
    let out = compile_and_run(
        "<?php function make(): array<int> { return [4, 5, 6]; } $a = make(); echo $a[1];",
    );
    assert_eq!(out, "5");
}

/// A declared `array<int>` local is writable with a conforming element.
#[test]
fn test_array_type_argument_declared_local_accepts_conforming_write() {
    let out = compile_and_run("<?php array<int> $a = [1, 2, 3]; $a[0] = 9; echo $a[0] + $a[2];");
    assert_eq!(out, "12");
}

/// A bare `array` declaration is `array<mixed>` and absorbs a heterogeneous write, so the
/// declared-element contract must not fire for it.
///
/// Regression: this SEGFAULTED before `coerce_typed_assign_value` converted the initializer
/// to the declared element storage. The declaration allocated `array<int>` from `[1,2,3]`,
/// bound the slot as `array<mixed>`, and emitted no `array_to_mixed` between them, so the
/// element header tag still said int while every later load read the slot as boxed. A
/// CONFORMING write crashed too, so the trigger was the declaration, not the widening.
#[test]
fn test_bare_array_declaration_absorbs_widening_write() {
    let out = compile_and_run("<?php array $a = [1, 2, 3]; $a[0] = \"s\"; echo $a[0], $a[1];");
    assert_eq!(out, "s2");
}

/// The same declaration with a CONFORMING element write, which crashed for the same reason.
#[test]
fn test_bare_array_declaration_accepts_conforming_write() {
    let out = compile_and_run("<?php array $a = [1, 2, 3]; $a[0] = 9; echo $a[0], $a[1];");
    assert_eq!(out, "92");
}

/// A declared array with no element write at all was always correct; pinned so the fix
/// cannot regress the path that already worked.
#[test]
fn test_bare_array_declaration_without_write() {
    let out = compile_and_run("<?php array $a = [1, 2, 3]; echo $a[0], $a[2];");
    assert_eq!(out, "13");
}

/// An object element type exercises the heap-pointer representation class.
#[test]
fn test_array_type_argument_object_element() {
    let out = compile_and_run(
        "<?php \
         class Box { public function __construct(public int $v) {} } \
         function firstV(array<Box> $a): int { return $a[0]->v; } \
         echo firstV([new Box(3), new Box(4)]);",
    );
    assert_eq!(out, "3");
}

/// A nullable element type parses and round-trips through the annotation.
#[test]
fn test_array_type_argument_nullable_element() {
    let out = compile_and_run(
        "<?php function firstOf(array<?int> $a) { return $a[0]; } echo firstOf([11, 12]);",
    );
    assert_eq!(out, "11");
}

/// `array<K, V>` selects hash storage and keeps both halves typed: the IR signature is
/// `Heap(Hash) php=array<string, int>` returning a raw `I64`, with no boxing.
#[test]
fn test_assoc_array_type_arguments_param() {
    let out = compile_and_run(
        "<?php \
         function lookup(array<string, int> $m, string $k): int { return $m[$k]; } \
         $ages = [\"alice\" => 30, \"bob\" => 25]; \
         echo lookup($ages, \"alice\"), \",\", lookup($ages, \"bob\");",
    );
    assert_eq!(out, "30,25");
}

/// An integer-keyed associative form is distinct from the indexed form and still works.
#[test]
fn test_assoc_array_type_arguments_int_key() {
    let out = compile_and_run(
        "<?php \
         function lookup(array<int, string> $m, int $k): string { return $m[$k]; } \
         echo lookup([7 => \"seven\", 9 => \"nine\"], 9);",
    );
    assert_eq!(out, "nine");
}

/// `array<K, V>` works in return position, and an empty literal satisfies it.
#[test]
fn test_assoc_array_type_arguments_return_position() {
    let out = compile_and_run(
        "<?php \
         function empty_ok(): array<string, int> { return []; } \
         function match_ok(): array<string, int> { return [\"a\" => 1, \"b\" => 2]; } \
         $m = match_ok(); \
         echo count(empty_ok()), \",\", $m[\"b\"];",
    );
    assert_eq!(out, "0,2");
}

/// An object value type exercises the heap-pointer representation through hash storage.
#[test]
fn test_assoc_array_type_arguments_object_value() {
    let out = compile_and_run(
        "<?php \
         class Point { public function __construct(public int $x) {} } \
         function xOf(array<string, Point> $m, string $k): int { return $m[$k]->x; } \
         echo xOf([\"o\" => new Point(4)], \"o\");",
    );
    assert_eq!(out, "4");
}

/// A declared associative local accepts a conforming write.
#[test]
fn test_assoc_array_declared_local_accepts_conforming_write() {
    let out = compile_and_run(
        "<?php array<string, int> $m = [\"a\" => 1]; $m[\"b\"] = 2; echo $m[\"a\"] + $m[\"b\"];",
    );
    assert_eq!(out, "3");
}

/// One generic template serves three argument types in the same program.
///
/// Each call selects its own monomorphic instantiation — `identity<int>` is `I64 -> I64`,
/// `identity<string>` is `Str -> Str` — so none of them widens the others to boxed `mixed`,
/// which is what an untyped function shared between these call sites would do.
#[test]
fn test_generic_function_instantiates_per_argument_type() {
    let out = compile_and_run(
        "<?php \
         function identity<T>(T $value): T { return $value; } \
         echo identity(5), \"|\", identity(\"hi\"), \"|\", identity(2.5);",
    );
    assert_eq!(out, "5|hi|2.5");
}

/// Inference reaches through an `array<T>` parameter, and the element type survives into the
/// instantiation's signature.
#[test]
fn test_generic_function_infers_through_an_array_parameter() {
    let out = compile_and_run(
        "<?php \
         function firstOf<T>(array<T> $xs): T { return $xs[0]; } \
         echo firstOf([10, 20]), \"|\", firstOf([\"a\", \"b\"]);",
    );
    assert_eq!(out, "10|a");
}

/// Two type parameters bind independently from two argument positions.
#[test]
fn test_generic_function_with_two_type_params() {
    let out = compile_and_run(
        "<?php \
         function pairUp<K, V>(K $k, V $v): string { return $k . \"=\" . $v; } \
         echo pairUp(\"k\", 7), \"|\", pairUp(1, \"v\");",
    );
    assert_eq!(out, "k=7|1=v");
}

/// A template calling another template: instantiating `twice<int>` makes its body call
/// `identity`, which the next fixpoint round instantiates in turn.
#[test]
fn test_generic_template_calling_another_template() {
    let out = compile_and_run(
        "<?php \
         function identity<T>(T $value): T { return $value; } \
         function twice<T>(T $value): T { return identity($value); } \
         echo twice(42), \"|\", twice(\"deep\");",
    );
    assert_eq!(out, "42|deep");
}

/// An object type argument exercises the heap-pointer representation class.
#[test]
fn test_generic_function_with_an_object_type_argument() {
    let out = compile_and_run(
        "<?php \
         class Point { public function __construct(public int $x) {} } \
         function unwrap<T>(T $value): T { return $value; } \
         echo unwrap(new Point(7))->x;",
    );
    assert_eq!(out, "7");
}

/// A type parameter named in the body — a typed local — is substituted per instantiation.
#[test]
fn test_generic_body_typed_local_is_substituted() {
    let out = compile_and_run(
        "<?php \
         function hold<T>(T $value): T { T $held = $value; return $held; } \
         echo hold(7), \"|\", hold(\"seven\");",
    );
    assert_eq!(out, "7|seven");
}

/// One source position inside a template resolves per ENCLOSING instantiation.
///
/// `twice<int>` and `twice<string>` both call `identity` at the same line and column, and they
/// must reach different functions. Keying the call site on the position alone made
/// `twice<string>` invoke `identity<int>`.
#[test]
fn test_one_position_resolves_per_enclosing_instantiation() {
    let out = compile_and_run(
        "<?php \
         function identity<T>(T $value): T { return $value; } \
         function twice<T>(T $value): T { return identity($value); } \
         echo twice(42), \"|\", twice(\"deep\");",
    );
    assert_eq!(out, "42|deep");
}

/// PHPStan `@template` docblocks are the PORTABLE surface and compile to the same monomorphic
/// instantiations as native syntax.
///
/// The annotated file is valid PHP — the annotations are comments — so this is what an existing
/// well-annotated codebase gets with no source change at all.
#[test]
fn test_docblock_template_compiles_like_native_syntax() {
    let out = compile_and_run(
        "<?php\n\
         /**\n\
          * @template T\n\
          * @param array<T> $items\n\
          * @return T\n\
          */\n\
         function firstOf(array $items) { return $items[0]; }\n\
         echo firstOf([1, 2, 3]), \"|\", firstOf([\"a\", \"b\"]);",
    );
    assert_eq!(out, "1|a");
}

/// A bare `@template T` binds through a plain parameter, exactly as `function f<T>(T $v)` does.
#[test]
fn test_docblock_template_binds_a_bare_parameter() {
    let out = compile_and_run(
        "<?php\n\
         /**\n\
          * @template T\n\
          * @param T $value\n\
          * @return T\n\
          */\n\
         function identity($value) { return $value; }\n\
         echo identity(7), \"|\", identity(\"seven\");",
    );
    assert_eq!(out, "7|seven");
}

/// A doc comment carrying no `@template` changes nothing.
///
/// Acting on it would silently re-type every annotated PHP file in the world.
#[test]
fn test_docblock_without_template_is_inert() {
    let out = compile_and_run(
        "<?php\n\
         /**\n\
          * @param int $a\n\
          * @return int\n\
          */\n\
         function twice($a) { return $a * 2; }\n\
         echo twice(21);",
    );
    assert_eq!(out, "42");
}

/// A template that is never called emits no code at all: it is not a function, and stripping
/// it is what keeps its unrepresentable annotations away from lowering.
#[test]
fn test_uncalled_generic_template_emits_nothing() {
    let out = compile_and_run(
        "<?php function unused<T>(T $value): T { return $value; } echo \"ok\";",
    );
    assert_eq!(out, "ok");
}


// --- Generic classes and interfaces -------------------------------------------------
//
// A generic CLASS needs no inference: every mention writes its type arguments, so
// `generics::classes` instantiates it from syntax alone, before the checker runs. What these
// tests pin is that the instantiation is real — two type arguments give two distinct classes
// with their own storage — and that the surface reaches every position a class type occupies.

/// The base case: one generic class, two type arguments, two independent instantiations.
#[test]
fn test_generic_class_two_instantiations_are_distinct_classes() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         $i = new Box<int>(41); \
         $s = new Box<string>(\"ok\"); \
         echo $i->get() + 1, \"|\", $s->get();",
    );
    assert_eq!(out, "42|ok");
}

/// An explicitly declared property typed `T` is substituted, not only the promoted form.
#[test]
fn test_generic_class_declared_property_is_substituted() {
    let out = compile_and_run(
        "<?php \
         class Cell<T> { \
           private T $value; \
           public function __construct(T $value) { $this->value = $value; } \
           public function get(): T { return $this->value; } \
         } \
         echo (new Cell<int>(7))->get();",
    );
    assert_eq!(out, "7");
}

/// A class implements a generic interface at a concrete type without being generic itself.
#[test]
fn test_class_implements_generic_interface_at_a_concrete_type() {
    let out = compile_and_run(
        "<?php \
         interface Entity { public function id(): int; } \
         class User implements Entity { \
           public function __construct(private int $uid) {} \
           public function id(): int { return $this->uid; } \
         } \
         interface Repository<T: Entity> { public function find(int $id): T; } \
         class UserRepository implements Repository<User> { \
           public function find(int $id): User { return new User($id); } \
         } \
         echo (new UserRepository())->find(7)->id();",
    );
    assert_eq!(out, "7");
}

/// A subclass of the bound satisfies it: the check is assignability, not name equality.
#[test]
fn test_generic_class_bound_accepts_a_subclass() {
    let out = compile_and_run(
        "<?php \
         class Animal { public function speak(): string { return \"...\"; } } \
         class Dog extends Animal { public function speak(): string { return \"woof\"; } } \
         class Pen<T: Animal> { \
           public function __construct(private T $a) {} \
           public function noise(): string { return $this->a->speak(); } \
         } \
         echo (new Pen<Dog>(new Dog()))->noise();",
    );
    assert_eq!(out, "woof");
}

/// A trailing parameter falls back to its declared default.
#[test]
fn test_generic_class_type_parameter_default() {
    let out = compile_and_run(
        "<?php \
         class Pair<K, V = string> { \
           public function __construct(private K $k, private V $v) {} \
           public function key(): K { return $this->k; } \
           public function value(): V { return $this->v; } \
         } \
         $p = new Pair<int>(1, \"defaulted\"); \
         echo $p->key(), \":\", $p->value();",
    );
    assert_eq!(out, "1:defaulted");
}

/// `Box<Box<int>>` ends in ONE token: the lexer reads `>>` as PHP's right shift, and the
/// parser is what splits it back into two closes.
#[test]
fn test_generic_class_nested_type_argument_splits_shift_token() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         $outer = new Box<Box<int>>(new Box<int>(5)); \
         echo $outer->get()->get();",
    );
    assert_eq!(out, "5");
}

/// A generic FUNCTION over a generic class infers its parameter from the argument's class.
///
/// The argument's type is an ordinary object by then — `Box<Box<int>>` — so inference recovers
/// the structure by decoding that name, which is what `instantiated_name` wrote.
#[test]
fn test_generic_function_infers_from_a_generic_class_argument() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         function unwrap<T>(Box<Box<T>> $b): T { return $b->get()->get(); } \
         echo unwrap(new Box<Box<int>>(new Box<int>(5)));",
    );
    assert_eq!(out, "5");
}

/// A generic class composes with `array<T>`: a type argument is a type, not only a class.
#[test]
fn test_generic_class_holding_an_array_type_argument() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         Box<array<int>> $b = new Box<array<int>>([1, 2, 3]); \
         echo count($b->get());",
    );
    assert_eq!(out, "3");
}

/// A generic class nobody mentions emits no code: it is a template, not a class.
#[test]
fn test_uncalled_generic_class_emits_nothing() {
    let out = compile_and_run(
        "<?php class Unused<T> { public function __construct(public T $v) {} } echo \"ok\";",
    );
    assert_eq!(out, "ok");
}

/// A generic class implementing a generic interface at its OWN type parameter.
///
/// The shape a real repository has, and the one that needs two instantiation rounds:
/// `InMemoryRepository<User>` is emitted first, and its `implements Repository<User>` is what
/// asks for the interface's own instantiation.
#[test]
fn test_generic_class_implements_a_generic_interface_at_its_own_parameter() {
    let out = compile_and_run(
        "<?php \
         interface Entity { public function id(): int; } \
         class User implements Entity { \
           public function __construct(private int $uid) {} \
           public function id(): int { return $this->uid; } \
         } \
         interface Repository<T: Entity> { public function find(int $id): T; } \
         class InMemoryRepository<T: Entity> implements Repository<T> { \
           public function __construct(private T $seed) {} \
           public function find(int $id): T { return $this->seed; } \
         } \
         echo (new InMemoryRepository<User>(new User(5)))->find(1)->id();",
    );
    assert_eq!(out, "5");
}

/// An ordinary class can extend an INSTANTIATION, inheriting its concrete member storage.
#[test]
fn test_concrete_class_extends_a_generic_instantiation() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(protected T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         class IntBox extends Box<int> { \
           public function doubled(): int { return $this->get() * 2; } \
         } \
         echo (new IntBox(21))->doubled();",
    );
    assert_eq!(out, "42");
}

/// An instantiated generic interface works as a parameter type, with ordinary dispatch.
#[test]
fn test_instantiated_generic_interface_as_a_parameter_type() {
    let out = compile_and_run(
        "<?php \
         interface Entity { public function id(): int; } \
         class User implements Entity { \
           public function __construct(private int $uid) {} \
           public function id(): int { return $this->uid; } \
         } \
         interface Repository<T: Entity> { public function find(int $id): T; } \
         class UserRepository implements Repository<User> { \
           public function find(int $id): User { return new User($id); } \
         } \
         function lookup(Repository<User> $r, int $id): int { return $r->find($id)->id(); } \
         echo lookup(new UserRepository(), 9);",
    );
    assert_eq!(out, "9");
}

/// A generic class declared inside a namespace: the template is found under its canonical name,
/// and its type parameter is NOT qualified against that namespace.
#[test]
fn test_namespaced_generic_class() {
    let out = compile_and_run(
        "<?php namespace App; \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         echo (new Box<int>(9))->get();",
    );
    assert_eq!(out, "9");
}

/// Static access reaches through a generic class type: a method, a constant, and the name.
///
/// `Box<int>::class` is the INSTANTIATED class's real name, which is also what `get_class()`
/// returns for one of its instances — the two cannot disagree, because there is only one class.
#[test]
fn test_static_access_on_a_generic_class_type() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           const LABEL = \"box\"; \
           public function __construct(private T $v) {} \
           public static function of(T $v): Box<T> { return new Box<T>($v); } \
           public function get(): T { return $this->v; } \
         } \
         echo Box<int>::of(7)->get(), \"|\", Box<string>::LABEL, \"|\", Box<int>::class;",
    );
    assert_eq!(out, "7|box|Box<int>");
}

/// A generic class returning ITSELF at its own parameter: `Box<T>` inside `Box<T>`'s body is
/// concrete only once the class is instantiated, and the copy carries `Box<int>`.
#[test]
fn test_generic_class_method_returns_its_own_instantiation() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
           public function copy(): Box<T> { return new Box<T>($this->v); } \
         } \
         echo (new Box<string>(\"same\"))->copy()->get();",
    );
    assert_eq!(out, "same");
}

/// The recognition of `Name<...>::` must not swallow an ordinary comparison chain, which is the
/// same tokens without the `::`.
#[test]
fn test_comparison_chain_still_compiles() {
    let out = compile_and_run(
        "<?php const A = 1; $b = 2; $c = 3; echo (A < $b && $b > $c) ? \"y\" : \"n\";",
    );
    assert_eq!(out, "n");
}

/// `instanceof` reaches a generic class type, and two instantiations are unrelated classes.
#[test]
fn test_instanceof_a_generic_class_type() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { public function __construct(private T $v) {} } \
         $i = new Box<int>(1); \
         $s = new Box<string>(\"a\"); \
         var_dump($i instanceof Box<int>); \
         var_dump($s instanceof Box<int>);",
    );
    assert_eq!(out, "bool(true)\nbool(false)\n");
}

/// `get_class()` reports the instantiated class, which is its real name.
#[test]
fn test_get_class_reports_the_instantiated_name() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { public function __construct(private T $v) {} } \
         echo get_class(new Box<int>(1)), \"|\", get_class(new Box<string>(\"a\"));",
    );
    assert_eq!(out, "Box<int>|Box<string>");
}

/// The motivating shape: a typed collection. `array<T>` inside a generic class, a builtin
/// interface, and a typed `foreach` over the element type.
#[test]
fn test_generic_class_holding_a_typed_array_implements_countable() {
    let out = compile_and_run(
        "<?php \
         class TypedList<T> implements Countable { \
           private array<T> $items; \
           public function __construct(array<T> $items) { $this->items = $items; } \
           public function count(): int { return count($this->items); } \
           public function first(): T { return $this->items[0]; } \
           public function sum(): int { \
             $total = 0; \
             foreach ($this->items as $item) { $total += $item; } \
             return $total; \
           } \
         } \
         $ints = new TypedList<int>([1, 2, 3]); \
         $words = new TypedList<string>([\"a\", \"bb\"]); \
         echo count($ints), \"|\", $ints->first(), \"|\", $ints->sum(), \"|\", $words->first();",
    );
    assert_eq!(out, "3|1|6|a");
}

/// A SELF-REFERENTIAL generic class: `Node<T>` holds a `?Node<T>` and builds one in its own
/// method. The instantiation fixpoint has to recognize it already made that class, or it asks
/// for a new one every round.
#[test]
fn test_self_referential_generic_class_settles() {
    let out = compile_and_run(
        "<?php \
         class Node<T> { \
           public ?Node<T> $next = null; \
           public function __construct(public T $value) {} \
           public function append(T $value): Node<T> { \
             $tail = new Node<T>($value); \
             $this->next = $tail; \
             return $tail; \
           } \
           public function total(): int { \
             $rest = $this->next === null ? 0 : $this->next->total(); \
             return $this->value + $rest; \
           } \
         } \
         $head = new Node<int>(1); \
         $head->append(2)->append(3); \
         echo $head->total();",
    );
    assert_eq!(out, "6");
}

/// Static state belongs to the INSTANTIATION, because the instantiations are separate classes.
/// This is the one place monomorphization and erasure give different answers.
#[test]
fn test_static_property_is_per_instantiation() {
    let out = compile_and_run(
        "<?php \
         class Counter<T> { \
           public static int $made = 0; \
           public function __construct(T $v) { static::$made++; } \
         } \
         new Counter<int>(1); \
         new Counter<int>(2); \
         new Counter<string>(\"x\"); \
         echo Counter<int>::$made, \"|\", Counter<string>::$made;",
    );
    assert_eq!(out, "2|1");
}

/// A type argument is any type: an enum, a nullable, a union, an associative array.
#[test]
fn test_exotic_type_arguments_instantiate_and_mangle() {
    let out = compile_and_run(
        "<?php \
         enum Status: string { case Ready = \"ready\"; } \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         echo (new Box<Status>(Status::Ready))->get()->value, \"|\", \
              (new Box<int|string>(7))->get(), \"|\", \
              (new Box<array<string, int>>([\"b\" => 2]))->get()[\"b\"];",
    );
    assert_eq!(out, "ready|7|2");
}

/// Namespaced generics with `use` imports: the head and every argument canonicalize, or the
/// mention names a template no declaration matches.
#[test]
fn test_namespaced_generic_class_with_imports() {
    let out = compile_and_run(
        "<?php \
         namespace App\\Model { class User { public function __construct(public int $id) {} } } \
         namespace App\\Support { \
           class Box<T> { \
             public function __construct(private T $v) {} \
             public function get(): T { return $this->v; } \
           } \
         } \
         namespace App\\Run { \
           use App\\Model\\User; \
           use App\\Support\\Box; \
           $b = new Box<User>(new User(9)); \
           echo $b->get()->id, \"|\", get_class($b); \
         }",
    );
    assert_eq!(out, "9|App\\Support\\Box<App\\Model\\User>");
}

/// The cursor idiom over a generic linked list, compiled and run.
///
/// `?Node<int> $c = $head; while ($c !== null) { $c = $c->next; }` needs two things at once: a
/// self-referential generic class, and a declared nullable local that survives the guard's
/// narrowing.
#[test]
fn test_generic_linked_list_cursor_traversal() {
    let out = compile_and_run(
        "<?php \
         class Node<T> { \
           public ?Node<T> $next = null; \
           public function __construct(public T $value) {} \
           public function append(T $value): Node<T> { \
             $tail = new Node<T>($value); \
             $this->next = $tail; \
             return $tail; \
           } \
         } \
         $head = new Node<int>(1); \
         $head->append(2)->append(3); \
         $sum = 0; \
         ?Node<int> $c = $head; \
         while ($c !== null) { $sum += $c->value; $c = $c->next; } \
         $words = new Node<string>(\"a\"); \
         $words->append(\"b\"); \
         $out = \"\"; \
         ?Node<string> $w = $words; \
         while ($w !== null) { $out .= $w->value; $w = $w->next; } \
         echo $sum, \"|\", $out;",
    );
    assert_eq!(out, "6|ab");
}

/// `new Box(41)` with no written type arguments still produces two distinct classes.
#[test]
fn test_inferred_generic_construction_produces_distinct_classes() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         $i = new Box(41); \
         $s = new Box(\"ok\"); \
         echo $i->get() + 1, \"|\", $s->get(), \"|\", get_class($i), \"|\", get_class($s);",
    );
    assert_eq!(out, "42|ok|Box<int>|Box<string>");
}

/// A generic class constructed INSIDE a generic function, at that function's own parameter.
///
/// `generics::classes` cannot resolve this one — `T` means nothing until a call binds it — so
/// the checker answers it while instantiating `wrap<int>` to type the call.
#[test]
fn test_generic_class_constructed_inside_a_generic_function() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         function wrap<T>(T $v): Box<T> { return new Box<T>($v); } \
         echo wrap(1)->get(), wrap(\"a\")->get();",
    );
    assert_eq!(out, "1a");
}

/// A static factory determines the type arguments the same way `new` does.
///
/// PHP allows exactly one `__construct`, so a named factory is how a real codebase offers more
/// than one way to build something — inference that fired only at `new` would miss most of it.
#[test]
fn test_generic_static_factory_infers_and_dispatches() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
           public static function of(T $v): Box<T> { return new Box<T>($v); } \
         } \
         $i = Box::of(7); \
         $s = Box::of(\"x\"); \
         echo $i->get() + 1, \"|\", $s->get(), \"|\", get_class($i), \"|\", get_class($s);",
    );
    assert_eq!(out, "8|x|Box<int>|Box<string>");
}

/// `self` in a type argument is LEXICAL, so the instantiating pass can substitute it — and must,
/// or it emits a class literally named `Box<self>` whose property is typed `Box<self>`.
#[test]
fn test_self_as_a_type_argument_names_the_enclosing_class() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { public function __construct(private T $v) {} } \
         class A { public function wrap(): Box<self> { return new Box<self>($this); } } \
         echo get_class((new A())->wrap());",
    );
    assert_eq!(out, "Box<A>");
}

/// One construction inside a generic function means a different class per instantiation, and
/// resolves to each. The bodies are different AST nodes after splicing; only the KEY collided.
#[test]
fn test_construction_inside_a_generic_function_resolves_per_instantiation() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         function wrap<T>(T $v): Box<T> { return new Box($v); } \
         echo wrap(1)->get(), wrap(\"a\")->get(), \"|\", get_class(wrap(2)), \"|\", get_class(wrap(\"b\"));",
    );
    assert_eq!(out, "1a|Box<int>|Box<string>");
}

/// `catch` is the fifth place PHP names a class, and it discriminates between instantiations:
/// `Err<int>` does not catch an `Err<string>`.
#[test]
fn test_catch_discriminates_between_instantiations() {
    let out = compile_and_run(
        "<?php \
         class Err<T> extends Exception {} \
         try { \
           throw new Err<string>(\"s\"); \
         } catch (Err<int> $e) { \
           echo \"wrong\"; \
         } catch (Err<string> $e) { \
           echo \"right:\", get_class($e); \
         }",
    );
    assert_eq!(out, "right:Err<string>");
}

/// `@template` on a CLASS reaches the same monomorphic instantiations native `class Box<T>`
/// does — the file it is written in is still valid PHP, and `get_class` shows the two classes
/// the one annotated declaration produced.
#[test]
fn test_docblock_template_on_a_class() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/** @template T */\n",
        "class Box {\n",
        "    /** @var T */\n",
        "    private $value;\n",
        "    /** @param T $value */\n",
        "    public function __construct($value) { $this->value = $value; }\n",
        "    /** @return T */\n",
        "    public function get() { return $this->value; }\n",
        "    /** @param T $value */\n",
        "    public function set($value): void { $this->value = $value; }\n",
        "}\n",
        "$a = new Box(41);\n",
        "$a->set($a->get() + 1);\n",
        "$b = new Box(\"hi\");\n",
        "echo $a->get(), $b->get(), \"|\", get_class($a), \"|\", get_class($b);\n",
    ));
    assert_eq!(out, "42hi|Box<int>|Box<string>");
}

/// A member annotation that mentions no type parameter is an ordinary PHPStan annotation and
/// must stay one: `@template` on the class does not promote the whole body into declarations.
#[test]
fn test_docblock_member_without_a_type_parameter_is_not_a_declaration() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/** @template T */\n",
        "class Box {\n",
        "    /** @param T $value */\n",
        "    public function __construct(private $value) {}\n",
        "    /** @param int $times */\n",
        "    public function repeat($times) { return $times; }\n",
        "    /** @return T */\n",
        "    public function get() { return $this->value; }\n",
        "}\n",
        "$b = new Box(\"x\");\n",
        "echo $b->get(), $b->repeat(\"3\");\n",
    ));
    assert_eq!(out, "x3");
}

/// A promoted constructor parameter is also a property. Retyping only the parameter would store
/// an `int` into a `mixed` field and lose the instantiation.
#[test]
fn test_docblock_promoted_constructor_property() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/** @template T */\n",
        "final class Pair {\n",
        "    /**\n",
        "     * @param T $first\n",
        "     * @param T $second\n",
        "     */\n",
        "    public function __construct(private $first, private $second) {}\n",
        "    /** @return T */\n",
        "    public function first() { return $this->first; }\n",
        "    /** @return T */\n",
        "    public function second() { return $this->second; }\n",
        "}\n",
        "$p = new Pair(3, 4);\n",
        "$q = new Pair(\"a\", \"b\");\n",
        "echo $p->first() + $p->second(), $q->first() . $q->second(), \"|\", get_class($p);\n",
    ));
    assert_eq!(out, "7ab|Pair<int>");
}

/// `@extends` and `@implements` are how an annotated file names the instantiation it inherits,
/// which native syntax writes `extends Holder<int> implements Reader<int>`.
#[test]
fn test_docblock_extends_and_implements() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/** @template T */\n",
        "interface Reader {\n",
        "    /** @return T */\n",
        "    public function read();\n",
        "}\n",
        "/**\n",
        " * @template T\n",
        " * @implements Reader<T>\n",
        " */\n",
        "abstract class Holder implements Reader {\n",
        "    /** @param T $slot */\n",
        "    public function __construct(protected $slot) {}\n",
        "    /** @return T */\n",
        "    public function read() { return $this->slot; }\n",
        "}\n",
        "/**\n",
        " * @extends Holder<int>\n",
        " * @implements Reader<int>\n",
        " */\n",
        "final class IntHolder extends Holder {\n",
        "    public function doubled(): int { return $this->read() * 2; }\n",
        "}\n",
        "$h = new IntHolder(21);\n",
        "echo $h->doubled(), \"|\", $h->read(), \"|\", get_class($h);\n",
    ));
    assert_eq!(out, "42|21|IntHolder");
}

/// `+T` lets an instantiation widen to one over a superclass. The receiver still dispatches to
/// its OWN instantiation's body — `get_class($this)` is what proves it, because two bodies
/// compiled from one template differ in nothing else.
#[test]
fn test_covariant_type_parameter_widens_and_keeps_its_instantiation() {
    let out = compile_and_run(
        "<?php \
         class Animal {} \
         class Dog extends Animal {} \
         class Box<+T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
           public function whoAmI(): string { return get_class($this); } \
         } \
         function inspect(Box<Animal> $b): string { return $b->whoAmI(); } \
         echo inspect(new Box<Animal>(new Animal())), \"|\", inspect(new Box<Dog>(new Dog()));",
    );
    assert_eq!(out, "Box<Animal>|Box<Dog>");
}

/// `-T` goes the other way: a consumer of the supertype stands in for a consumer of the subtype.
#[test]
fn test_contravariant_type_parameter_widens_the_other_way() {
    let out = compile_and_run(
        "<?php \
         class Animal {} \
         class Dog extends Animal {} \
         class Sink<-T> { \
           public function accept(T $v): string { $x = $v; return get_class($this); } \
         } \
         function feed(Sink<Dog> $s): string { return $s->accept(new Dog()); } \
         echo feed(new Sink<Dog>()), \"|\", feed(new Sink<Animal>());",
    );
    assert_eq!(out, "Sink<Dog>|Sink<Animal>");
}

/// Widening composes through nesting, one level at a time.
#[test]
fn test_covariance_composes_through_a_nested_instantiation() {
    let out = compile_and_run(
        "<?php \
         class Animal { public function name(): string { return \"animal\"; } } \
         class Dog extends Animal { public function name(): string { return \"dog\"; } } \
         class Box<+T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
         } \
         function readNested(Box<Box<Animal>> $o): string { return $o->get()->get()->name(); } \
         echo readNested(new Box<Box<Dog>>(new Box<Dog>(new Dog())));",
    );
    assert_eq!(out, "dog");
}

/// A PHP array is a value, so a `T` read out of `array<T>` cannot be written back into the
/// template's storage. The element keeps the enclosing polarity, which is what keeps this legal.
#[test]
fn test_covariant_parameter_may_be_returned_inside_an_array() {
    let out = compile_and_run(
        "<?php \
         class Animal {} \
         class Dog extends Animal {} \
         class Box<+T> { \
           public function __construct(private T $v) {} \
           public function all(): array<T> { return [$this->v]; } \
         } \
         function count_all(Box<Animal> $b): int { return count($b->all()); } \
         echo count_all(new Box<Dog>(new Dog()));",
    );
    assert_eq!(out, "1");
}

/// PHPStan spells variance in the tag; native syntax spells it on the parameter. Both reach the
/// same declaration, so an annotated file widens exactly like a written one.
#[test]
fn test_docblock_template_covariant() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "class Animal {}\n",
        "class Dog extends Animal {}\n",
        "/** @template-covariant T */\n",
        "class Box {\n",
        "    /** @param T $v */\n",
        "    public function __construct(private $v) {}\n",
        "    /** @return T */\n",
        "    public function get() { return $this->v; }\n",
        "    public function whoAmI(): string { return get_class($this); }\n",
        "}\n",
        "function inspect(Box<Animal> $b): string { return $b->whoAmI(); }\n",
        "echo inspect(new Box<Dog>(new Dog()));\n",
    ));
    assert_eq!(out, "Box<Dog>");
}

/// A widened call site takes its vtable slot from the EXPECTED instantiation and indexes the
/// ACTUAL one's table, so the two must number their methods identically.
///
/// Here only `Box<Dog>` is ever constructed. `Box<Animal>` is named in a parameter type alone,
/// so its `__construct` is not live — and a pruner that recompacts each class independently
/// moves `whoAmI` from slot 2 to slot 1 in `Box<Animal>` while `Box<Dog>` keeps it at 2. The
/// call then lands on `Box<Dog>::get`, whose returned object is read as a string, and the
/// program prints heap bytes and exits 0.
///
/// `whoAmI` is declared THIRD on purpose: at slot 0 the shift cannot be observed, which is how
/// the bug hid behind every hand-written probe that happened to construct both.
#[test]
fn test_widening_keeps_vtable_slots_aligned_when_only_one_side_is_constructed() {
    let out = compile_and_run(
        "<?php \
         class Animal {} \
         class Dog extends Animal {} \
         class Box<+T> { \
           public function __construct(private T $v) {} \
           public function get(): T { return $this->v; } \
           public function whoAmI(): string { return get_class($this); } \
         } \
         function inspect(Box<Animal> $b): string { return $b->whoAmI(); } \
         echo inspect(new Box<Dog>(new Dog()));",
    );
    assert_eq!(out, "Box<Dog>");
}

/// A method may declare type parameters of its OWN, bound by the call rather than by the class.
/// Two calls with different argument types produce two instantiations of one template.
#[test]
fn test_generic_method_instantiates_per_argument_type() {
    let out = compile_and_run(
        "<?php \
         class Registry { \
           public function pickOr<U>(U $key, U $fallback): U { $x = $fallback; return $key; } \
         } \
         $r = new Registry(); \
         echo $r->pickOr(7, 0), $r->pickOr(\"a\", \"z\");",
    );
    assert_eq!(out, "7a");
}

/// A generic method on a generic CLASS: `T` is bound when the class is instantiated and `U` when
/// the method is called, so one class instantiation carries several method instantiations.
#[test]
fn test_generic_method_on_a_generic_class() {
    let out = compile_and_run(
        "<?php \
         class Pair<T> { \
           public function __construct(private T $left) {} \
           public function left(): T { return $this->left; } \
           public function withRight<U>(U $right): U { return $right; } \
         } \
         $p = new Pair<int>(1); \
         $q = new Pair<string>(\"a\"); \
         echo $p->left(), $p->withRight(\"two\"), $p->withRight(3), $q->left(), $q->withRight(9);",
    );
    assert_eq!(out, "1two3a9");
}

/// A bound on the method's own type parameter is checked at the CALL, against the class table,
/// and the instantiated body dispatches on the concrete class it was given.
#[test]
fn test_generic_method_bound_dispatches_on_the_concrete_class() {
    let out = compile_and_run(
        "<?php \
         class Entity { public function id(): int { return 42; } } \
         class User extends Entity { public function id(): int { return 7; } } \
         class Repo { public function idOf<E : Entity>(E $e): int { return $e->id(); } } \
         $r = new Repo(); \
         echo $r->idOf(new Entity()), \"|\", $r->idOf(new User());",
    );
    assert_eq!(out, "42|7");
}

/// A bound that names its own parameter — `<T : Cmp<T>>` — is satisfied through the
/// instantiation it describes, on all four paths that check one: a function call, a method
/// call, a written class mention and an inferred construction.
///
/// Each resolved the bound as written, so it asked whether `Word` implements a class called
/// `Cmp<T>` — which nothing declares — and rejected every call. The class path substituted,
/// but only the parameters bound BEFORE the one being checked, which left the same `T`.
#[test]
fn test_f_bounded_bounds_are_substituted_before_they_are_checked() {
    let out = compile_and_run(
        "<?php \
         interface Cmp<T> { public function cmp(T $o): int; } \
         class Word implements Cmp<Word> { \
             public function __construct(public string $s) {} \
             public function cmp(Word $o): int { return strcmp($this->s, $o->s); } \
         } \
         function smaller<T : Cmp<T>>(T $a, T $b): T { return $a->cmp($b) <= 0 ? $a : $b; } \
         class Picker { \
             public function larger<U : Cmp<U>>(U $a, U $b): U { return $a->cmp($b) >= 0 ? $a : $b; } \
         } \
         class Sorted<T : Cmp<T>> { \
             public function __construct(private T $first) {} \
             public function keep(T $v): T { return $v->cmp($this->first) < 0 ? $v : $this->first; } \
         } \
         echo smaller(new Word(\"ab\"), new Word(\"cd\"))->s, \"|\"; \
         echo (new Picker())->larger(new Word(\"ab\"), new Word(\"cd\"))->s, \"|\"; \
         $s = new Sorted<Word>(new Word(\"m\")); \
         echo $s->keep(new Word(\"c\"))->s, \"|\", $s->keep(new Word(\"z\"))->s, \"|\"; \
         $inferred = new Sorted(new Word(\"k\")); \
         echo $inferred->keep(new Word(\"q\"))->s;",
    );
    assert_eq!(out, "ab|cd|c|m|k");
}

/// Vincenzo's report: `>>` is split for `Box<Box<int>>`, so it is split for arrays too. The rule
/// lives in the type parser and knows nothing about what is being closed.
#[test]
fn test_nested_array_type_arguments_split_the_shift_token() {
    let out = compile_and_run(
        "<?php \
         function rows(array<array<int>> $grid): int { return count($grid); } \
         function deep(array<array<array<int>>> $cube): int { return count($cube); } \
         function named(array<string, array<int>> $m): int { return count($m); } \
         echo rows([[1, 2], [3]]), deep([[[1]]]), named([\"a\" => [1, 2]]);",
    );
    assert_eq!(out, "211");
}

/// The shape Vincenzo reached for, and the reason `callable(T): U` exists: a bare `callable`
/// carries no types, so nothing at the call could bind `U`. The declared signature lets the
/// closure's own types do it, and `map` chains to a third type.
#[test]
fn test_typed_callable_infers_a_generic_method_through_a_closure() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(private T $value) {} \
           public function get(): T { return $this->value; } \
           public function map<U>(callable(T): U $f): Box<U> { \
             return new Box<U>($f($this->value)); \
           } \
         } \
         $b = new Box<int>(21); \
         $s = $b->map(fn(int $n): string => \"n=\" . ($n * 2)); \
         $len = $s->map(fn(string $t): int => strlen($t)); \
         echo $s->get(), \"|\", get_class($s), \"|\", $len->get(), \"|\", get_class($len);",
    );
    assert_eq!(out, "n=42|Box<string>|4|Box<int>");
}

/// The same inference on a generic FUNCTION, where both halves of the signature bind.
#[test]
fn test_typed_callable_infers_both_halves_on_a_function() {
    let out = compile_and_run(
        "<?php \
         function applyTyped<T, U>(callable(T): U $f, T $v): U { return $f($v); } \
         echo applyTyped(fn(int $n): string => \"n=\" . $n, 7);",
    );
    assert_eq!(out, "n=7");
}

/// A bare `callable` is untouched by any of this: it stays the type it always was, and a
/// parameter declared with it still accepts any callable.
#[test]
fn test_bare_callable_is_unchanged() {
    let out = compile_and_run(
        "<?php \
         function run(callable $f, int $v): int { return $f($v); } \
         echo run(fn(int $n): int => $n + 1, 41);",
    );
    assert_eq!(out, "42");
}

/// Generic methods and variance meet here, and the meeting used to be a silent miscompile.
///
/// `Holder<Dog>` is called with `tag<int>` while `Holder<Animal>` never is, so the two
/// instantiations carry different method sets. If an instantiated generic method took a vtable
/// slot, the slot after it would land at a different number in each class — and `describe`, read
/// through a widened `Holder<Animal>` parameter, would dispatch to whatever sits at that index in
/// `Holder<Dog>`.
///
/// An instantiated generic method takes no slot: its call site names it exactly, so there is
/// nothing to dispatch on. `assert_instantiation_vtable_slots_aligned` fails the build if that
/// stops being true.
#[test]
fn test_generic_method_instantiations_do_not_shift_sibling_vtable_slots() {
    let out = compile_and_run(
        "<?php \
         class Animal {} \
         class Dog extends Animal {} \
         class Holder<+T> { \
           public function __construct(private T $value) {} \
           public function get(): T { return $this->value; } \
           public function tag<U>(U $label): string { $x = $label; return get_class($this); } \
           public function describe(): string { return get_class($this); } \
         } \
         function inspect(Holder<Animal> $h): string { return $h->describe(); } \
         $dog = new Holder<Dog>(new Dog()); \
         echo $dog->tag(7), \"|\", inspect($dog), \"|\", inspect(new Holder<Animal>(new Animal()));",
    );
    assert_eq!(out, "Holder<Dog>|Holder<Dog>|Holder<Animal>");
}

/// `$i++` inside a loop body must not make `$i` `mixed` in the storage fixed point.
///
/// `AssignedValue::Opaque` means "no statically available RHS" — true of a `foreach` value, false
/// of an increment, whose type is the variable's own. Classing it opaque made `$i` `mixed`, and
/// appending it pinned the array to `array<mixed>`, so a declared `array<int>` could not be
/// satisfied while appending a constant always could.
#[test]
fn test_incrementing_a_counter_does_not_widen_what_is_appended() {
    let out = compile_and_run(
        "<?php \
         function upTo(int $n): array<int> { \
           $out = []; \
           $i = 0; \
           while ($i < $n) { $out[] = $i; $i++; } \
           return $out; \
         } \
         echo upTo(3)[2], count(upTo(3));",
    );
    assert_eq!(out, "23");
}

/// A float counter keeps its own type rather than collapsing, and a string counter stays opaque:
/// PHP's `++` on a string is a string increment, not arithmetic, so widening it is the honest
/// answer there.
#[test]
fn test_incremented_float_counter_keeps_its_element_type() {
    let out = compile_and_run(
        "<?php \
         function halves(int $n): array<float> { \
           $out = []; \
           $f = 0.5; \
           $i = 0; \
           while ($i < $n) { $out[] = $f; $f++; $i++; } \
           return $out; \
         } \
         echo halves(2)[1];",
    );
    assert_eq!(out, "1.5");
}

/// Invoking a declared `callable(T): U` yields `U`, not `mixed`.
///
/// The signature was read for inference at the call site and then dropped, because `CallableSig`
/// resolves to a bare `PhpType::Callable`. An `array<U>` return is the STRICT position that shows
/// it — an argument position would not, since `array<string>` accepts `array<mixed>` coercively.
#[test]
fn test_typed_callable_invocation_yields_its_declared_return() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(private T $value) {} \
           public function mapAll<U>(callable(T): U $f): array<U> { return [$f($this->value)]; } \
         } \
         echo (new Box<int>(21))->mapAll(fn(int $n): string => \"n=\" . $n)[0];",
    );
    assert_eq!(out, "n=21");
}

/// The same on a free function, where the declared signature is the only thing that can say what
/// `$f(…)` returns.
#[test]
fn test_typed_callable_invocation_on_a_function_yields_its_declared_return() {
    let out = compile_and_run(
        "<?php \
         function collect<T, U>(callable(T): U $f, T $v): array<U> { return [$f($v)]; } \
         echo collect(fn(int $n): string => \"n=\" . $n, 7)[0];",
    );
    assert_eq!(out, "n=7");
}

/// An enum implementing a generic interface at a concrete type, end to end.
///
/// The enum carries the arguments on its `implements` clause and nothing else — there is no
/// `enum Suit<T>` — and the instantiation rewrites the clause to name `Labelled<string>`, the
/// class that actually exists after the template is stripped. Leaving the template name there
/// reached codegen as missing interface metadata.
#[test]
fn test_enum_implements_a_generic_interface() {
    let out = compile_and_run(
        "<?php \
         interface Labelled<T> { public function label(): T; } \
         enum Suit: string implements Labelled<string> { \
           case Hearts = 'H'; \
           case Spades = 'S'; \
           public function label(): string { return $this->value; } \
         } \
         function show(Labelled<string> $l): string { return $l->label(); } \
         echo show(Suit::Hearts), show(Suit::Spades);",
    );
    assert_eq!(out, "HS");
}

/// Two enums implementing the SAME template at different types get the two distinct interfaces,
/// which is what monomorphization means here.
#[test]
fn test_two_enums_implement_one_template_at_different_types() {
    let out = compile_and_run(
        "<?php \
         interface Labelled<T> { public function label(): T; } \
         enum Suit: string implements Labelled<string> { \
           case Hearts = 'H'; \
           public function label(): string { return $this->value; } \
         } \
         enum Rank: int implements Labelled<int> { \
           case Ace = 1; \
           public function label(): int { return $this->value; } \
         } \
         function word(Labelled<string> $l): string { return $l->label(); } \
         function count_of(Labelled<int> $l): int { return $l->label(); } \
         echo word(Suit::Hearts), count_of(Rank::Ace);",
    );
    assert_eq!(out, "H1");
}

/// `$i++` leaves `int|float`, not `mixed`, and a SECOND increment must still compile.
///
/// PHP promotes an integer at the overflow boundary — `PHP_INT_MAX++` is
/// `float(9.223372036854776E+18)` in php-src and in elephc — so the honest type after an
/// increment is `int|float`. It used to be `mixed`, which is the top type: every value derived
/// from a counter inherited it, and no diagnostic could say what a counter actually held.
///
/// The second `$i++` is the regression this pins: the increment arms accepted `int`, `float`,
/// `string`, `bool`, `null` and `mixed`, so the moment the first increment produced a union the
/// next one reported `Cannot increment/decrement $i`.
#[test]
fn test_incremented_counter_can_be_incremented_again() {
    let out = compile_and_run(
        "<?php \
         function walk(int $n): int { \
           $i = 0; \
           $i++; \
           $i++; \
           while ($i < $n) { $i++; } \
           return $i; \
         } \
         echo walk(5);",
    );
    assert_eq!(out, "5");
}

/// The overflow the `int|float` type describes, at the boundary, outside a loop.
///
/// Storage is unchanged by the new type (`codegen_repr` maps both `mixed` and a union to a
/// boxed cell), which is exactly why the precision costs nothing: this still promotes.
#[test]
fn test_incrementing_past_the_integer_boundary_promotes_to_float() {
    let out = compile_and_run(
        "<?php $i = PHP_INT_MAX; $i++; var_dump($i);",
    );
    assert_eq!(out, "float(9.223372036854776E+18)\n");
}

/// A counter cast at the append satisfies a declared `array<int>`.
///
/// This is the way out the element-storage diagnostic now names. An `int|float` element is
/// boxed storage, so it cannot BE a packed int vector; the cast pins the element type, and for
/// a value that is already an int it costs nothing at runtime.
#[test]
fn test_casting_a_counter_at_the_append_pins_the_element_type() {
    let out = compile_and_run(
        "<?php \
         function upTo(int $n): array<int> { \
           $out = []; \
           for ($i = 0; $i < $n; $i++) { $out[] = (int) $i; } \
           return $out; \
         } \
         echo upTo(3)[2], count(upTo(3));",
    );
    assert_eq!(out, "23");
}

/// An `int|float` counter still binds to an `int` parameter and to a string offset.
///
/// Both positions accepted `mixed` already, so refusing the NARROWER `int|float` would have made
/// the more precise type the stricter one — and it did, until `type_accepts` and the string
/// offset rule learned about the arithmetic union. Several preludes pass a loop counter to an
/// `int` parameter, so the whole image/mysqli/xml surface stopped compiling for one build.
#[test]
fn test_incremented_counter_binds_to_int_parameters_and_string_offsets() {
    let out = compile_and_run(
        "<?php \
         function pick(string $s, int $at): string { return $s[$at]; } \
         function scan(string $s): string { \
           $out = ''; \
           $i = 0; \
           while ($i < 3) { $out .= pick($s, $i); $out .= $s[$i]; $i++; } \
           return $out; \
         } \
         echo scan('abc');",
    );
    assert_eq!(out, "aabbcc");
}

// --- Type arguments WRITTEN at a call site ---
//
// Inference stays the default and covers the common case; writing them is for what inference
// cannot reach — a type parameter no argument position mentions — and for saying something the
// arguments would otherwise decide differently.

/// A type parameter no argument mentions: only the call site can bind it.
///
/// `emptyList<T>(): array<T>` takes nothing, so inference has nothing to work from and reported
/// exactly that. Writing the argument is the answer, and the instantiation it names is an
/// ordinary function from there on.
#[test]
fn test_written_type_argument_binds_what_inference_cannot() {
    let out = compile_and_run(
        "<?php \
         function emptyList<T>(): array<T> { return []; } \
         $l = emptyList<int>(); \
         $l[] = 7; \
         echo count($l), $l[0];",
    );
    assert_eq!(out, "17");
}

/// A written argument OVERRIDES what the call's arguments would have selected.
#[test]
fn test_written_type_argument_overrides_inference() {
    let out = compile_and_run(
        "<?php \
         function identity<T>(T $v): T { return $v; } \
         var_dump(identity<float>(1));",
    );
    assert_eq!(out, "float(1)\n");
}

/// Two written instantiations of one template are two functions, as inference's are.
#[test]
fn test_two_written_instantiations_are_two_functions() {
    let out = compile_and_run(
        "<?php \
         function identity<T>(T $v): T { return $v; } \
         echo identity<int>(4), identity<string>('x');",
    );
    assert_eq!(out, "4x");
}

/// A written argument list may stop early when the remaining parameters declare defaults.
#[test]
fn test_written_type_arguments_fall_back_to_defaults() {
    let out = compile_and_run(
        "<?php \
         function pair<A, B = string>(A $a, B $b): B { return $b; } \
         echo pair<int>(1, 'x');",
    );
    assert_eq!(out, "x");
}

/// Nested arguments decode through the same type grammar the name was built with.
#[test]
fn test_written_type_argument_may_be_generic_itself() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { public function __construct(private T $v) {} \
                        public function get(): T { return $this->v; } } \
         function unwrap<T>(Box<T> $b): T { return $b->get(); } \
         echo unwrap<Box<int>>(new Box<Box<int>>(new Box<int>(7)))->get();",
    );
    assert_eq!(out, "7");
}

/// A generic METHOD takes written arguments the same way, and chains.
#[test]
fn test_written_type_argument_on_a_generic_method() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
           public function __construct(private T $v) {} \
           public function map<U>(callable(T): U $f): Box<U> { return new Box<U>($f($this->v)); } \
           public function get(): T { return $this->v; } \
         } \
         $b = new Box<int>(21); \
         echo $b->map<string>(fn(int $x): string => 'n' . $x * 2)->get();",
    );
    assert_eq!(out, "n42");
}

/// The comparison chain that looks the same must keep parsing as comparisons.
///
/// `identity<int>(41)` is claimed only because php-src REJECTS the comparison reading — `<` is
/// non-associative in PHP 8 — so nothing a PHP program could have meant is taken. An ordinary
/// comparison against a constant, with no `(` after it, is untouched.
#[test]
fn test_written_type_arguments_do_not_steal_comparisons() {
    let out = compile_and_run(
        "<?php \
         const A = 1; const B = 5; \
         $x = 3; \
         var_dump(A < B); \
         var_dump($x < B); \
         var_dump((A < $x) == true);",
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\n");
}

/// `buffer_new<int>(…)` and `ptr_cast<T>(…)` have the same token shape and their own grammar.
#[test]
fn test_written_type_arguments_leave_the_construct_forms_alone() {
    let out = compile_and_run(
        "<?php \
         $b = buffer_new<int>(4); \
         $b[0] = 9; \
         echo $b[0];",
    );
    assert_eq!(out, "9");
}

// --- A BARE template name ---
//
// `function idOf(Repo $r)` used to be `Unknown type: Repo`: the templates are stripped before the
// checker runs, so nothing under that name survives. It is now read as the instantiation the
// declaration itself describes — each parameter at its bound, its default, or `mixed` — and the
// reading is warned about every time, because it is weaker than it looks.

/// A bounded template read at its bound, satisfied by that exact instantiation.
#[test]
fn test_bare_template_name_is_read_at_its_bound() {
    let out = compile_and_run(
        "<?php \
         class Entity { public function id(): int { return 7; } } \
         class Repo<T : Entity> { \
           public function __construct(private T $item) {} \
           public function item(): T { return $this->item; } \
         } \
         function idOf(Repo $r): int { return $r->item()->id(); } \
         echo idOf(new Repo<Entity>(new Entity()));",
    );
    assert_eq!(out, "7");
}

/// The reading that is actually useful: a COVARIANT template accepts its subtype instantiations.
///
/// `Repo<User>` satisfies the bare `Repo` — read as `Repo<Entity>` — only because `out T` says
/// the parameter appears in no input position. On an invariant template the same bare name takes
/// exactly one instantiation, which is what the warning says.
#[test]
fn test_bare_covariant_template_accepts_a_subtype_instantiation() {
    let out = compile_and_run(
        "<?php \
         class Entity { public function id(): int { return 7; } } \
         class User extends Entity { public function id(): int { return 9; } } \
         class Repo<out T : Entity> { \
           public function __construct(private T $item) {} \
           public function item(): T { return $this->item; } \
         } \
         function idOf(Repo $r): int { return $r->item()->id(); } \
         echo idOf(new Repo<User>(new User()));",
    );
    assert_eq!(out, "9");
}

/// With no bound and no default the reading is `mixed`, which is erasure — and it still works on
/// its own terms.
#[test]
fn test_bare_unbounded_template_is_read_as_mixed() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { public function __construct(private T $v) {} \
                        public function get(): T { return $this->v; } } \
         function unwrap(Box $b) { return $b->get(); } \
         echo unwrap(new Box<mixed>(5));",
    );
    assert_eq!(out, "5");
}

/// A DEFAULT stands in where the parameter declares no bound.
#[test]
fn test_bare_template_name_falls_back_to_a_default() {
    let out = compile_and_run(
        "<?php \
         class Tagged<T = string> { public function __construct(private T $v) {} \
                                    public function get(): T { return $this->v; } } \
         function label(Tagged $t): string { return $t->get(); } \
         echo label(new Tagged<string>('hi'));",
    );
    assert_eq!(out, "hi");
}

/// A loop counter reaches a buffer — as the index AND as the element — and a re-walked temp.
///
/// Each of those positions accepted `mixed` while refusing `int|float`, which made the narrower
/// type the stricter one. The three shapes here are the ones the codegen suite found: a buffer
/// element written from a counter, a buffer indexed by one, and a synthesized temp assigned from
/// `$i++` on a body the checker walks more than once (`func_get_arg` desugars to exactly that),
/// plus a bitwise operator, whose integer-operand rule excluded `Float` on purpose and therefore
/// excluded the union too.
#[test]
fn test_counter_reaches_buffer_index_element_and_a_rewalked_temp() {
    let out = compile_and_run(
        "<?php \
         function fill(int $n): int { \
           buffer<int> $buf = buffer_new<int>(4); \
           for ($i = 0; $i < $n; $i++) { $buf[$i] = $i; } \
           $last = $buf[$n - 1] & 7; \
           buffer_free($buf); \
           return $last; \
         } \
         function pick(int $at): int { $i = $at; return func_get_arg($i++); } \
         echo fill(3), pick(1, 7, 8);",
    );
    assert_eq!(out, "27");
}

/// A counter reaches a union expectation, a negation and a packed field.
///
/// Three more gates that accepted `mixed` and refused `int|float`, found by a focused external
/// review that ran every one of them:
///
/// - an expected UNION (`?int`, `int|string`) split the ACTUAL into members and asked whether
///   `int` accepts `float`, which is false — so `?int $x` refused what a bare `int $x` took;
/// - unary negation enumerated the scalars and errored on everything else;
/// - the packed-field guard admitted `mixed` for exactly this value — its comment says "int
///   arithmetic, typed Mixed for its overflow-to-float promotion" — and was not extended when
///   that arithmetic started saying `int|float` precisely.
#[test]
fn test_counter_reaches_union_expectations_negation_and_packed_fields() {
    let out = compile_and_run(
        "<?php \
         packed class Cell { public int $id; } \
         class P { public ?int $x = null; } \
         function f(?int $x): ?int { return $x; } \
         function g(int|string $x): string { return 'v' . $x; } \
         $i = 0; \
         $i++; \
         $p = new P(); \
         $p->x = $i; \
         ?int $j = $i; \
         buffer<Cell> $cells = buffer_new<Cell>(1); \
         $cells[0]->id = $i; \
         echo f($i), $p->x, $j, g($i), -$i, $cells[0]->id; \
         buffer_free($cells);",
    );
    assert_eq!(out, "111v1-11");
}


/// Named arguments bind by NAME, so the call's source order is not the order inference pairs
/// against the declaration.
///
/// `pick(v: "abc", n: 1)` was inferred by position: `T` took `n`'s int, the call resolved to
/// `pick<int>`, and the ordinary argument check — which does understand named arguments — then
/// refused `"abc"` at an `int` parameter. A valid program rejected by its own instantiation.
#[test]
fn test_named_arguments_bind_type_parameters_by_name_not_by_position() {
    let out = compile_and_run(
        r#"<?php
/**
 * @template T
 * @param T $v
 * @return T
 */
function pick(int $n, $v) { return $v; }
echo pick(v: "abc", n: 1), "|", pick(n: 2, v: 7), "|", pick(3, "z");
"#,
    );
    assert_eq!(out, "abc|7|z");
}


/// A type parameter's default may NAME an earlier one (`<T, U = T>`), which is a type only once
/// that one is bound.
///
/// The default was cloned raw, so `U` was handed the name `T` and the declaration failed with
/// `Unknown type: T` — for both the inferred call and the written one, which take different paths
/// to the same omission.
#[test]
fn test_a_dependent_type_parameter_default_is_substituted() {
    let out = compile_and_run(
        r#"<?php
function identity<T, U = T>(T $x): U { return $x; }
echo identity(42), "|", identity<int>(7), "|", identity("z");
"#,
    );
    assert_eq!(out, "42|7|z");
}

/// A VARIADIC parameter is a binding position like any other.
///
/// Its declared element type lives in its own field, so handing inference the fixed parameter
/// types alone left `T` with nothing to determine it and the call was refused as undetermined.
#[test]
fn test_a_variadic_parameter_determines_a_type_parameter() {
    let out = compile_and_run(
        r#"<?php
function first<T>(T ...$values): T { return $values[0]; }
echo first(42), "|", first("a", "b"), "|", first(1, 2, 3);
"#,
    );
    assert_eq!(out, "42|a|1");
}

/// A generic method is INHERITED like any other.
///
/// Its template is recorded under the class that DECLARES it — a template has no signature to
/// copy into the subclass until a call site gives it type arguments — so a lookup that only asked
/// the receiver's own name reported `Undefined method: B::id` for a method the object has.
#[test]
fn test_an_inherited_generic_method_resolves_through_its_declaring_class() {
    let out = compile_and_run(
        r#"<?php
class A {
    public function id<T>(T $x): T { return $x; }
}
class B extends A {}
class C extends B {}
echo (new B())->id(42), "|", (new C())->id("deep"), "|", (new A())->id(7);
"#,
    );
    assert_eq!(out, "42|deep|7");
}

/// The same template, called statically.
///
/// The static path tried generic CLASSES (`Box<int>::of()`, where the class carries the type
/// parameters) and then reported `Undefined method`. A method's own type parameters are a
/// different template, and it is not in the class table either.
#[test]
fn test_a_static_generic_method_resolves_and_instantiates() {
    let out = compile_and_run(
        r#"<?php
class C {
    public static function id<T>(T $x): T { return $x; }
}
class D extends C {}
echo C::id(42), "|", C::id("s"), "|", D::id(9);
"#,
    );
    assert_eq!(out, "42|s|9");
}

/// The method path orders its arguments against the declaration, like the function path.
///
/// It paired `args` by source position, so a named argument bound the wrong parameter's type —
/// and a variadic method parameter determined nothing, for the same missing-field reason the
/// function form had.
#[test]
fn test_generic_method_named_arguments_and_variadics_bind_correctly() {
    let out = compile_and_run(
        r#"<?php
class Pick {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function of(int $n, $v) { return $v; }

    public function head<T>(T ...$xs): T { return $xs[0]; }
}
$p = new Pick();
echo $p->of(v: "abc", n: 1), "|", $p->of(2, "z"), "|", $p->head(5, 6);
"#,
    );
    assert_eq!(out, "abc|z|5");
}


/// Construction and static factories order their arguments against the declaration.
///
/// Both paired the call's SOURCE order against the declared parameters, so a named argument bound
/// the wrong parameter's type: `new Box(v: "abc", n: 1)` took `T` from `n`'s int and then asked
/// for a `Box<int>` it could not build. They were the last two call surfaces still reading their
/// arguments positionally, after the function and method paths were fixed.
#[test]
fn test_generic_construction_and_factories_bind_named_arguments_by_name() {
    let out = compile_and_run(
        r#"<?php
class Box<T> {
    public function __construct(public int $n, private T $v) {}
    public function get(): T { return $this->v; }
    public static function of(int $n, T $v): Box<T> { return new Box<T>($n, $v); }
}
$named = new Box(v: "abc", n: 1);
echo $named->get(), "|", $named->n, "|";
$positional = new Box(2, "z");
echo $positional->get(), "|", $positional->n, "|";
echo Box::of(v: "f", n: 3)->get();
"#,
    );
    assert_eq!(out, "abc|1|z|2|f");
}

/// A property read on an INFERRED instantiation must not report the class undefined.
///
/// `new Box(…)` resolves `Box<string>` in one round and the class is spliced by the next, so
/// within the first the name is legitimately absent from the class table. A method call on the
/// object was already tolerated; a property read reported `Undefined class: Box<string>` for a
/// construction the checker had just resolved itself. Written instantiations never hit it,
/// which is why it survived.
///
/// The two spellings must name DIFFERENT instantiations. Writing `Box<string>` beside an
/// inferred `Box<string>` puts the class in the table before the read is checked, so the
/// deferral is never reached — an earlier version of this test proved nothing for exactly
/// that reason, and its mutation run said so.
#[test]
fn test_property_read_on_an_inferred_instantiation_is_deferred_not_refused() {
    let out = compile_and_run(
        r#"<?php
class Box<T> {
    public function __construct(public int $n, private T $v) {}
    public function get(): T { return $this->v; }
}
$inferred = new Box(1, "abc");
$written = new Box<int>(2, 5);
echo $inferred->n, $inferred->get(), "|", $written->n, $written->get();
"#,
    );
    assert_eq!(out, "1abc|25");
}

/// An ATTRIBUTE between a doc block and its declaration must not drop the annotation.
///
/// `collect` keys a block by the first NONBLANK line after it, and `#[Marker]` is not blank, so
/// the block was filed against the attribute's line while the lookup used the declaration's. The
/// failure is silent rather than loud: with `@return T` gone the same program answers a different
/// number, which is why the attributed and unattributed forms are asserted against each other.
#[test]
fn test_an_attribute_between_a_doc_block_and_its_declaration_keeps_the_annotation() {
    let out = compile_and_run(
        r#"<?php
#[Attribute]
class Marker {}

/**
 * @template T
 */
#[Marker]
class Attributed {
    /** @param T $v */
    public function __construct(private $v) {}
    /** @return T */
    public function get() { return $this->v; }
}

/**
 * @template T
 */
class Plain {
    /** @param T $v */
    public function __construct(private $v) {}
    /** @return T */
    public function get() { return $this->v; }
}
$a = new Attributed<int>(7);
$p = new Plain<int>(7);
echo $a->get() + 1, "|", $p->get() + 1;
"#,
    );
    assert_eq!(out, "8|8");
}

/// Brackets in attribute strings and comments cannot end the group or consume its declaration.
#[test]
fn test_docblock_generics_survive_brackets_inside_attributes() {
    for argument in [
        r#""close ]""#,
        r#"'open ['"#,
        r#""escaped \" ]""#,
        r#""label" /* ] */"#,
        r#"["nested" => ["close ]"]]"#,
    ] {
        let source = format!(
            r#"<?php
/** @template T */
#[Marker(
    {argument}
)]
class Box {{
    /** @param T $value */
    public function __construct(private $value) {{}}
    /** @return T */
    public function get() {{ return $this->value; }}
}}
$box = new Box<int>(7);
echo $box->get() + 1, "|", get_class($box);
"#,
        );
        assert_eq!(compile_and_run(&source), "8|Box<int>", "{argument}");
    }
}

/// Consecutive groups may share the declaration's line and still follow a generic docblock.
#[test]
fn test_docblock_generics_survive_same_line_attribute_groups() {
    let out = compile_and_run(
        r#"<?php
/** @template T */
#[First("]")] #[Second("[")] class Box {
    public function __construct(int $value) {}
}
echo get_class(new Box<int>(7));
"#,
    );
    assert_eq!(out, "Box<int>");
}

// --- Generic traits ---
//
// `trait Holder<T>` is a template like a generic class: each `use Holder<int>` instantiates an
// ordinary trait named `Holder<int>`, and trait flattening then copies it into the class as it
// would any trait. Before, the declaration did not parse at all.

/// One trait used at two type arguments gives two classes with two storages, not one `mixed`.
#[test]
fn test_generic_trait_is_instantiated_per_use() {
    let out = compile_and_run(
        "<?php \
         trait Holder<T> { \
             private T $item; \
             public function put(T $v): void { $this->item = $v; } \
             public function get(): T { return $this->item; } \
         } \
         class IntBox { use Holder<int>; public function doubled(): int { return $this->get() * 2; } } \
         class StrBox { use Holder<string>; public function loud(): string { return strtoupper($this->get()); } } \
         $i = new IntBox(); $i->put(7); \
         $s = new StrBox(); $s->put(\"seven\"); \
         echo $i->get(), \"|\", $s->get(), \"|\", $i->doubled(), \"|\", $s->loud();",
    );
    assert_eq!(out, "7|seven|14|SEVEN");
}

/// A generic trait inside a namespace, used by another generic trait at ITS parameter, with an
/// object type argument — and the heap stays clean across both copies.
///
/// `use Bag<T>` inside `Named<T>` is a mention inside a template, left alone until `Named<int>`
/// is instantiated and names `Bag<int>`; `T` inside a namespace must stay `T`, not `App\T`.
#[test]
fn test_generic_trait_nests_and_resolves_inside_a_namespace() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
namespace App;
class Tag { public function __construct(public string $name) {} }
trait Bag<T> {
    private array<T> $items = [];
    public function add(T $v): void { $this->items[] = $v; }
    public function count(): int { return \count($this->items); }
    public function first(): T { return $this->items[0]; }
}
trait Named<T> {
    use Bag<T>;
    public function describe(): string { return (string) $this->count(); }
}
class Ints { use Named<int>; }
class Tags { use Bag<Tag>; }
$n = new Ints(); $n->add(1); $n->add(2); $n->add(3);
$t = new Tags(); $t->add(new Tag("x")); $t->add(new Tag("y"));
echo $n->describe(), "|", $t->count(), "|", $t->first()->name;
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3|2|x", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An adaptation block names the trait by its written spelling, and still reaches the
/// instantiation once `use Holder<int>` means it.
#[test]
fn test_generic_trait_adaptations_follow_the_instantiation() {
    let out = compile_and_run(
        "<?php \
         trait Holder<T> { \
             private T $item; \
             public function put(T $v): void { $this->item = $v; } \
             public function label(): string { return \"box:\" . $this->item; } \
             public function raw(): T { return $this->item; } \
         } \
         trait Other { public function label(): string { return \"other\"; } } \
         class IntBox { \
             use Holder<int>, Other { \
                 Holder::label insteadof Other; \
                 Other::label as otherLabel; \
                 Holder::raw as protected rawItem; \
             } \
             public function peek(): int { return $this->rawItem(); } \
         } \
         $b = new IntBox(); $b->put(5); \
         echo $b->label(), \"|\", $b->otherLabel(), \"|\", $b->peek();",
    );
    assert_eq!(out, "box:5|other|5");
}

/// A bare `use Holder;` is read as the instantiation the declaration describes, as a bare class
/// template name is.
#[test]
fn test_bare_generic_trait_use_reads_the_declared_instantiation() {
    let out = compile_and_run(
        "<?php \
         trait Holder<T> { \
             private T $item; \
             public function put(T $v): void { $this->item = $v; } \
             public function get(): T { return $this->item; } \
         } \
         class AnyBox { use Holder; } \
         $b = new AnyBox(); $b->put(7); \
         echo $b->get();",
    );
    assert_eq!(out, "7");
}

/// A bound whose argument implements `Stringable` only IMPLICITLY — by declaring `__toString`,
/// with its return type left for the body check to infer — is still satisfied.
///
/// Bounds are now answered before any body is checked, and that interface is not settled then;
/// such an obligation waits for the pass after the bodies instead of being rejected early.
#[test]
fn test_bound_satisfied_by_an_implicit_stringable_waits_for_the_bodies() {
    let out = compile_and_run(
        "<?php \
         class Money { public function __toString() { return \"m\"; } } \
         class Wrap<T : Stringable> { \
             public function __construct(private T $v) {} \
             public function show(): string { return \"<\" . $this->v . \">\"; } \
         } \
         echo (new Wrap<Money>(new Money()))->show();",
    );
    assert_eq!(out, "<m>");
}

/// A generic function called from a method, a static method, a closure in a method and nested
/// closures resolves its instantiation, under `--heap-debug`.
///
/// The checker keyed each call site by the enclosing FREE function, `main` everywhere else, while
/// lowering looked it up under `Class::method` or the closure's own scope. The lookup missed, the
/// call kept naming the stripped template, and codegen refused it as `unsupported language
/// construct identity`. Two instantiations of one generic class share the call's position, which
/// is why the scope is part of the key at all.
#[test]
fn test_generic_function_called_from_methods_and_closures() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function identity<T>(T $v): T { return $v; }
class Holder<T> {
    public function __construct(private T $v) {}
    public function get(): T { return identity($this->v); }
}
class S {
    public static function go(int $x): int { return identity($x); }
    public function viaClosure(int $x): int { $f = fn(int $y): int => identity($y); return $f($x); }
}
$outer = function (int $a): int { $inner = fn(int $b): int => identity($b); return $inner($a); };
echo (new Holder(5))->get(), "|", (new Holder("x"))->get(), "|", S::go(9), "|",
    (new S())->viaClosure(6), "|", $outer(7), "|", (function (int $y): int { return identity($y); })(8);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "5|x|9|6|7|8");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An inferred construction and static factory inside a method, a closure in a method and a
/// trait body are renamed to their instantiation, under `--heap-debug`.
///
/// The checker filed these sites under `main` while the rename walk looked them up under the bare
/// method name, so the construction kept naming the template, every round re-requested it, and
/// the compile ended in "Generic instantiation did not settle after 16 rounds". A trait body is
/// checked once per using class but walked once, so its site resolves through the using classes.
#[test]
fn test_inferred_constructions_inside_methods_closures_and_traits() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box<T> {
    public function __construct(public T $v) {}
    public static function of(T $v): Box<T> { return new Box<T>($v); }
}
class F {
    public function make(): int { $b = new Box(5); return $b->v; }
    public function factory(): string { return Box::of("f")->v; }
    public function inClosure(): int { $f = fn(int $x) => new Box($x); return $f(4)->v; }
}
trait Tr { public function build(int $x): int { return (new Box($x))->v; } }
class C { use Tr; }
class D { use Tr; }
$f = new F();
echo $f->make(), "|", $f->factory(), "|", $f->inClosure(), "|", (new C())->build(1), "|",
    (new D())->build(2);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "5|f|4|1|2");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A static factory on a generic class may return `static` or `self`, called at top level or from
/// another class's method, under `--heap-debug`.
///
/// The return type was resolved in the CALLER's context, so `Box::of(5)` was refused as "Cannot
/// use 'static' as a type outside of a class". It now means the instantiation being inferred.
#[test]
fn test_generic_static_factory_returning_static_or_self() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box<T> {
    public function __construct(public T $v) {}
    public static function of(T $v): static { return new static($v); }
    public static function from(T $v): self { return new self($v); }
}
class F {
    public function a(): int { return Box::from(5)->v; }
    public function b(): string { return Box::from("x")->v; }
}
echo Box::of(3)->v, "|", (new F())->a(), "|", (new F())->b();
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3|5|x");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An inferred generic method called through `?->` is renamed like one called through `->`.
///
/// The walker's nullsafe arm passed the method name through untouched, so the call kept naming the
/// stripped template and lowering reported `unknown method Box<int>::pickOr`.
#[test]
fn test_nullsafe_call_to_an_inferred_generic_method() {
    let out = compile_and_run(
        "<?php \
         class Box<T> { \
             public function __construct(private T $value) {} \
             public function pickOr<U>(U $fallback, U $other): U { return $fallback; } \
         } \
         $b = new Box(41); \
         echo $b?->pickOr(1, 0), \"|\", $b?->pickOr(\"a\", \"b\");",
    );
    assert_eq!(out, "1|a");
}

/// A generic method declared in a trait instantiates per call, for every using class and through
/// a subclass, under `--heap-debug`.
///
/// Method templates were collected from classes only, so a trait's `id<U>` was never a template
/// and `(new C())->id(5)` reported `Undefined method: C::id`. The instantiation is attached to the
/// trait and flattened into each using class with the rest of it.
#[test]
fn test_generic_method_declared_in_a_trait() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
trait T { public function id<U>(U $x): U { return $x; } }
class C { use T; }
class D { use T; }
class E extends C {}
echo (new C())->id(5), "|", (new C())->id("x"), "|", (new D())->id(7), "|", (new E())->id(4);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "5|x|7|4");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A spread array literal binds type parameters by its keys on the function, method and
/// construction paths, as it binds the arguments themselves.
///
/// Inference paired the whole `...[…]` with the first parameter, so `<T>` was reported as not
/// determined, while the shared call planner expands the same spread into named arguments and a
/// non-generic call printed `abc`.
#[test]
fn test_spread_literal_arguments_bind_type_parameters() {
    let out = compile_and_run(
        r#"<?php
function box<T>(int $n, T $v): T { return $v; }
class C { public static function box<T>(int $n, T $v): T { return $v; } }
class Box<T> { public function __construct(public int $n, public T $v) {} }
echo box(...["v" => "abc", "n" => 1]), "|", C::box(...["v" => "def", "n" => 2]), "|",
    (new Box(...["v" => "ghi", "n" => 3]))->v, "|", box(...[4, "jkl"]);
"#,
    );
    assert_eq!(out, "abc|def|ghi|jkl");
}

/// Native constructor type parameters specialize the class and retain promoted storage.
#[test]
fn test_native_constructor_type_parameters_specialize_the_class() {
    let source = "<?php\nclass Box { public function __construct<T>(public T $value) {} }\n\
         $number = new Box(5); $text = new Box(\"x\");\n\
         echo get_class($number), \":\", $number->value, \"|\",\n\
             get_class($text), \":\", $text->value;\n";
    assert_eq!(
        compile_cli_file_and_run_with_flags(source, &[]),
        "Box<int>:5|Box<string>:x"
    );
}

/// A spread classifies its keys as the call planner does and binds `<T>` from the argument the
/// call actually passes.
///
/// An integer-like string key (`"0"`) is positional. Every string key used to become a named
/// argument, so `<T>` was reported undetermined.
#[test]
fn test_spread_keys_bind_type_parameters_like_the_call_planner() {
    let out = compile_and_run(
        r#"<?php
function box<T>(T $v, int $n): string { return $v . ":" . $n; }
class C { public static function box<T>(T $v, int $n): string { return $v . ":" . $n; } }
echo box(...["0" => "abc", "n" => 1]), "|", C::box(...["0" => "def", "n" => 2]);
"#,
    );
    assert_eq!(out, "abc:1|def:2");
}

/// An unpacked array that supplies a positional entry after a named one is refused, as php
/// refuses it, on ordinary and generic calls.
///
/// The planner hoisted positional entries ahead of named ones, so `show(...["a" => "x", 1 => 2])`
/// printed `x:2` where php throws "Cannot use positional argument after named argument during
/// unpacking". The rule is per array: php accepts a SEPARATE later unpack of positional values,
/// and the valid orders still bind as php binds them.
#[test]
fn test_positional_after_named_unpack_is_refused() {
    for call in [
        "function show(string $a, int $b): string { return $a . \":\" . $b; }\necho show(...[\"a\" => \"x\", 1 => 2]);",
        "function show<T>(T $a, int $b): string { return $a . \":\" . $b; }\necho show(...[\"a\" => \"x\", 1 => 2]);",
    ] {
        let error = compile_cli_file_with_flags_expect_failure(&format!("<?php\n{call}\n"), &[]);
        assert!(
            error.contains("cannot use positional argument after named argument during unpacking"),
            "{call}: {error}"
        );
    }
    let out = compile_and_run(
        r#"<?php
function show(string $a, int $b): string { return $a . ":" . $b; }
echo show(...[0 => "x", "b" => 2]), "|", show(...["0" => "y", "b" => 3]), "|",
    show(...["z"], ...["b" => 4]), "|", show(...["a" => "w"], ...[5]);
"#,
    );
    assert_eq!(out, "x:2|y:3|z:4|w:5");
}

/// A PHPDoc `@template-covariant` marker that the declaration does not keep (a writable
/// property, a parameter) is demoted to invariant with a warning instead of rejecting a file
/// php-src runs.
#[test]
fn test_docblock_covariant_marker_broken_by_the_declaration_is_demoted() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/** @template-covariant TValue */\n",
        "class Box {\n",
        "    /** @var TValue */\n",
        "    protected $value;\n",
        "    /** @param TValue $value */\n",
        "    public function __construct($value) { $this->value = $value; }\n",
        "    /** @param TValue $value */\n",
        "    public function contains($value): bool { return $this->value === $value; }\n",
        "    /** @return TValue */\n",
        "    public function get() { return $this->value; }\n",
        "}\n",
        "$b = new Box(\"a\");\n",
        "echo $b->get(), \" \", var_export($b->contains(\"a\"), true);\n",
    ));
    assert_eq!(out, "a true");
}

/// PHPStan-style type arguments on built-in interfaces (`IteratorAggregate<K, V>`,
/// `ArrayAccess<K, V>`) describe nothing elephc stores and are erased.
#[test]
fn test_docblock_type_arguments_on_builtin_interfaces_are_erased() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/**\n",
        " * @implements IteratorAggregate<string, int>\n",
        " * @implements ArrayAccess<string, int>\n",
        " */\n",
        "class Bag implements IteratorAggregate, ArrayAccess {\n",
        "    private array $items = [];\n",
        "    public function getIterator(): ArrayIterator { return new ArrayIterator($this->items); }\n",
        "    public function offsetExists(mixed $k): bool { return isset($this->items[$k]); }\n",
        "    public function offsetGet(mixed $k): mixed { return $this->items[$k]; }\n",
        "    public function offsetSet(mixed $k, mixed $v): void { $this->items[$k] = $v; }\n",
        "    public function offsetUnset(mixed $k): void {}\n",
        "}\n",
        "$b = new Bag(); $b[\"x\"] = 1; $b[\"y\"] = 2;\n",
        "foreach ($b as $k => $v) { echo $k, \"=\", $v, \";\"; } echo $b[\"y\"];\n",
    ));
    assert_eq!(out, "x=1;y=2;2");
}

/// `foreach` over an instantiation inferred in the same round (`new Bag([...])`) is deferred to
/// the round that splices the class, instead of rejecting the not-yet-declared `Bag<string, int>`.
#[test]
fn test_foreach_over_an_inferred_generic_iterator_aggregate() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/**\n",
        " * @template TKey of array-key\n",
        " * @template TValue\n",
        " * @implements IteratorAggregate<TKey, TValue>\n",
        " */\n",
        "class Bag implements IteratorAggregate {\n",
        "    /** @param array<TKey, TValue> $items */\n",
        "    public function __construct(private array $items) {}\n",
        "    public function getIterator(): ArrayIterator { return new ArrayIterator($this->items); }\n",
        "}\n",
        "foreach (new Bag([\"x\" => 1, \"y\" => 2]) as $k => $v) { echo $k, \"=\", $v, \";\"; }\n",
    ));
    assert_eq!(out, "x=1;y=2;");
}

/// A non-generic class that implements a PHPDoc-generic interface by its bare name still
/// implements it: the bare parent is read as the interface's default instantiation instead of
/// naming the stripped template.
#[test]
fn test_bare_docblock_generic_interface_on_a_plain_class() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/**\n",
        " * @template TKey of array-key\n",
        " * @template TValue\n",
        " */\n",
        "interface Arrayable {\n",
        "    /** @return array<TKey, TValue> */\n",
        "    public function toArray();\n",
        "}\n",
        "class Model implements Arrayable {\n",
        "    public function toArray() { return [\"id\" => 1]; }\n",
        "}\n",
        "function dump(Arrayable $a) { echo count($a->toArray()); }\n",
        "dump(new Model());\n",
    ));
    assert_eq!(out, "1");
}

/// `instanceof` against a PHPDoc interface template is true for a class implementing it bare,
/// and the narrowed guard still evaluates once per object.
#[test]
fn test_instanceof_bare_docblock_generic_interface() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/**\n",
        " * @template TKey of array-key\n",
        " * @template TValue\n",
        " */\n",
        "interface Arrayable {\n",
        "    /** @return array<TKey, TValue> */\n",
        "    public function toArray();\n",
        "}\n",
        "class Model implements Arrayable {\n",
        "    public function toArray() { return [\"id\" => 1]; }\n",
        "}\n",
        "class Other {}\n",
        "echo var_export(new Model() instanceof Arrayable, true), \"|\";\n",
        "function chk($x) { echo var_export($x instanceof Arrayable, true), \"|\"; }\n",
        "chk(new Model()); chk(new Other()); chk(3);\n",
        "$o = new Model(); if ($o instanceof Arrayable) { echo \"yes\"; } else { echo \"no\"; }\n",
    ));
    assert_eq!(out, "true|true|false|false|yes");
}

/// `instanceof` against a PHPDoc interface template is true for a class implementing one of
/// its instantiations (`@implements Arrayable<string, int>`).
#[test]
fn test_instanceof_docblock_generic_interface_through_an_instantiation() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/**\n",
        " * @template TKey of array-key\n",
        " * @template TValue\n",
        " */\n",
        "interface Arrayable {\n",
        "    /** @return array<TKey, TValue> */\n",
        "    public function toArray();\n",
        "}\n",
        "/** @implements Arrayable<string, int> */\n",
        "class Model implements Arrayable {\n",
        "    public function toArray() { return [\"id\" => 1]; }\n",
        "}\n",
        "function chk($x) { echo var_export($x instanceof Arrayable, true), \"|\"; }\n",
        "chk(new Model());\n",
        "$o = new Model(); echo var_export($o instanceof Arrayable, true);\n",
    ));
    assert_eq!(out, "true|true");
}

/// `instanceof` against a generic CLASS template is true for any of its instantiations and for
/// a subclass extending one, PHPDoc or native alike.
#[test]
fn test_instanceof_generic_class_template_matches_every_instantiation() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/**\n",
        " * @template TKey of array-key\n",
        " * @template TValue\n",
        " */\n",
        "class Coll {\n",
        "    /** @param array<TKey, TValue> $items */\n",
        "    public function __construct(public array $items) {}\n",
        "}\n",
        "/** @extends Coll<string, string> */\n",
        "class SColl extends Coll {}\n",
        "class Box<T> { public function __construct(public T $v) {} }\n",
        "function chk($x) { echo var_export($x instanceof Coll, true), \"|\"; }\n",
        "chk(new Coll([\"a\" => 1])); chk(new Coll([1 => \"b\"])); chk(new SColl([\"k\" => \"v\"]));\n",
        "chk(new Box(1));\n",
        "$b = new Box<int>(1); $s = new Box<string>(\"s\");\n",
        "echo var_export($b instanceof Box, true), \"|\", var_export($s instanceof Box, true), \"|\";\n",
        "echo var_export(new Coll([\"z\" => 0]) instanceof Coll, true);\n",
    ));
    assert_eq!(out, "true|true|true|false|true|true|true");
}

/// Erasing a PHPDoc template's `array<TKey, TValue>` on a dispatched method gives back the plain
/// `array` php-src sees, so the method still adapts to an interface declaring `array`.
#[test]
fn test_erased_docblock_assoc_array_matches_a_native_array_contract() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "interface Enum { public function merge(array $xs); }\n",
        "/**\n",
        " * @template TKey of array-key\n",
        " * @template TValue\n",
        " */\n",
        "class Coll implements Enum {\n",
        "    /** @param array<TKey, TValue> $items */\n",
        "    public function __construct(public array $items) {}\n",
        "    /**\n",
        "     * @param array<TKey, TValue> $xs\n",
        "     * @return array<TKey, TValue>\n",
        "     */\n",
        "    public function merge(array $xs) { $o = $xs; foreach ($this->items as $k => $v) { $o[$k] = $v; } return $o; }\n",
        "}\n",
        "/** @extends Coll<string, string> */\n",
        "class SColl extends Coll {}\n",
        "$c = new SColl([\"k\" => \"a\"]);\n",
        "$r = $c->merge([\"p\", \"q\"]); echo implode(\",\", $r), \"|\";\n",
        "function m(Enum $e) { return implode(\",\", $e->merge([1, 2])); } echo m($c);\n",
    ));
    assert_eq!(out, "p,q,a|1,2,a");
}

/// An `instanceof` guard on a generic template keeps the receiver's methods reachable: the
/// value is not narrowed to the template's member-less marker, so the call dispatches.
#[test]
fn test_instanceof_generic_template_guard_keeps_methods_callable() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/**\n",
        " * @template TKey of array-key\n",
        " * @template TValue\n",
        " */\n",
        "interface Arrayable {\n",
        "    /** @return array<TKey, TValue> */\n",
        "    public function toArray();\n",
        "}\n",
        "class Model implements Arrayable {\n",
        "    public function toArray() { return [\"id\" => 1, \"n\" => 2]; }\n",
        "}\n",
        "function f($x) { if ($x instanceof Arrayable) { return count($x->toArray()); } return -1; }\n",
        "echo f(new Model()), \"|\", f(3), \"|\";\n",
        "$o = new Model(); if ($o instanceof Arrayable) { echo implode(\",\", array_keys($o->toArray())); }\n",
    ));
    assert_eq!(out, "2|-1|id,n");
}

/// A CLASS template's marker interface exists only for `instanceof`: reflection sees what php-src
/// sees (no interface `Box`, nothing extra in `class_implements`), `is_subclass_of` holds only
/// for a class extending an instantiation, and an INTERFACE template stays a visible interface.
#[test]
fn test_class_template_marker_is_hidden_from_reflection() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/** @template T */\n",
        "class Box { /** @param T $v */ public function __construct(public $v) {} }\n",
        "/** @extends Box<int> */\n",
        "class IntBox extends Box {}\n",
        "/** @template T */\n",
        "interface Coll { /** @return T */ public function first(); }\n",
        "/** @implements Coll<int> */\n",
        "class IntColl implements Coll { public function first() { return 1; } }\n",
        "$b = new Box(5); $s = new Box(\"s\"); $i = new IntBox(3);\n",
        "var_dump($b instanceof Box, $s instanceof Box, $i instanceof Box, new IntColl() instanceof Coll);\n",
        "var_dump(interface_exists('Box'), is_a($b, 'Box'), is_subclass_of($b, 'Box'), is_subclass_of($i, 'Box'), is_subclass_of('IntBox', 'Box'));\n",
        "var_dump(in_array('Box', get_declared_interfaces()), in_array('Coll', get_declared_interfaces()), interface_exists('Coll'));\n",
        "var_dump(class_implements($b), class_implements($i), class_implements('IntBox'));\n",
    ));
    assert_eq!(
        out,
        concat!(
            "bool(true)\nbool(true)\nbool(true)\nbool(true)\n",
            "bool(false)\nbool(true)\nbool(false)\nbool(true)\nbool(true)\n",
            "bool(false)\nbool(true)\nbool(true)\n",
            "array(0) {\n}\narray(0) {\n}\narray(0) {\n}\n",
        )
    );
}

/// An `instanceof` guard on a template is not a narrowing for the mixed-storage scan either, so
/// reassigning the guarded local inside it is an ordinary store.
#[test]
fn test_instanceof_generic_template_guard_is_not_a_storage_narrowing() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/** @template T */\n",
        "interface Coll { /** @return T */ public function first(); }\n",
        "/** @implements Coll<int> */\n",
        "class IntColl implements Coll { public function first() { return 7; } public function extra() { return \"x\"; } }\n",
        "interface Shape { public function area(); }\n",
        "class Sq implements Shape, Coll { public function area() { return 4; } public function first() { return 9; } }\n",
        "function a($x) { return $x instanceof Coll ? $x->first() : -1; }\n",
        "function b(Shape $x) { if ($x instanceof Coll) { return $x->area(); } return 0; }\n",
        "function c($x) { if (!$x instanceof Coll) { return \"no\"; } return $x->first(); }\n",
        "function d($x) { return $x instanceof Coll && $x->first() > 5 ? \"big\" : \"small\"; }\n",
        "echo a(new IntColl()), a(1), b(new Sq()), c(new Sq()), c(2), d(new IntColl()), d(new Sq()), \"|\";\n",
        "$v = 1; if ($v instanceof Coll) { $v = new IntColl(); } echo $v, \"|\";\n",
        "$w = new IntColl(); if ($w instanceof Coll) { echo $w->extra(); }\n",
    ));
    assert_eq!(out, "7-149nobigbig|1|x");
}

/// An `instanceof` guard on a template keeps an object-typed local's own class, so it still
/// reaches a parameter typed with that class inside and after the guard.
#[test]
fn test_instanceof_generic_template_guard_keeps_the_object_type() {
    let out = compile_and_run(concat!(
        "<?php\n",
        "/** @template T */\n",
        "interface Coll { /** @return T */ public function first(); }\n",
        "/** @implements Coll<int> */\n",
        "class IntColl implements Coll { public $n = 3; public function first() { return 7; } }\n",
        "function take(IntColl $c) { return $c->n; }\n",
        "$w = new IntColl();\n",
        "if ($w instanceof Coll) { $y = $w; echo $y->first(), take($w), $w->n, \"|\"; }\n",
        "echo take($w), \"|\";\n",
        "$arr = [new IntColl(), 5, \"s\"];\n",
        "foreach ($arr as $e) { if ($e instanceof Coll) { echo $e->first() + 1; } else { echo \"-\"; } }\n",
    ));
    assert_eq!(out, "733|3|8--");
}

/// `--report-generics` prints the instantiations a program reached, grouped by template, and
/// prints nothing unless the flag is passed.
#[test]
fn test_report_generics_lists_instantiations() {
    let source = r#"<?php
function idf<T>(T $x): T { return $x; }
class Box<T> { public function __construct(public T $v) {} }
echo idf(1), idf("s");
$b = new Box<int>(7);
echo get_class($b);
"#;
    let reported =
        compile_cli_file_with_flags_capturing_stderr(source, &["--report-generics"]);
    assert!(reported.contains("generics:"), "{reported}");
    assert!(reported.contains("idf<int>"), "{reported}");
    assert!(reported.contains("idf<string>"), "{reported}");
    assert!(reported.contains("Box<int>"), "{reported}");
    let quiet = compile_cli_file_with_flags_capturing_stderr(source, &[]);
    assert!(
        !quiet.contains("generics:"),
        "report must be opt-in: {quiet}"
    );
}

/// A monomorphized free function whose body is T-independent keeps ONE emitted body when its two
/// instantiations differ only by the concrete object class.
///
/// `identity<User>` and `identity<Order>` both lower to `return $x` over an object pointer, so the
/// assembly folding shares their body. Both entry symbols stay addressable, with one entry
/// jumping to the other's body, while each call retains its original behavior.
#[test]
fn test_shape_merge_keeps_one_t_independent_instantiation() {
    let source = r#"<?php
class User {}
class Order {}
function identity<T>(T $x): T { return $x; }
echo get_class(identity(new User())), "|", get_class(identity(new Order()));
"#;
    assert_eq!(compile_and_run(source), "User|Order");

    let user_asm = compile_cli_to_asm("inst", source);
    let user = elephc::names::function_symbol("identity<User>");
    let order = elephc::names::function_symbol("identity<Order>");
    let has_user = user_asm.contains(&format!("{user}:"));
    let has_order = user_asm.contains(&format!("{order}:"));
    assert!(
        one_is_thunk_of_the_other(&user_asm, &user, &order),
        "exactly one identity instantiation body should remain (user={has_user}, order={has_order})"
    );
}

/// A T-independent method on two generic instantiations shares one emitted body.
/// Both vtables keep their own entry symbol, with one entry jumping to the shared body.
#[test]
fn test_shape_merge_keeps_one_t_independent_method() {
    let source = r#"<?php
class Box<T> {
    public function __construct(private T $value) {}
    public function size(): int { return 1; }
}
class User {}
class Order {}
$u = new Box(new User());
$o = new Box(new Order());
echo $u->size(), $o->size(), "|", get_class($u), get_class($o);
"#;
    assert_eq!(compile_and_run(source), "11|Box<User>Box<Order>");

    let user_asm = compile_cli_to_asm("method", source);
    let size_key = elephc::names::php_symbol_key("size");
    let user = elephc::names::method_symbol("Box<User>", &size_key);
    let order = elephc::names::method_symbol("Box<Order>", &size_key);
    let has_user = user_asm.contains(&format!("{user}:"));
    let has_order = user_asm.contains(&format!("{order}:"));
    assert!(
        one_is_thunk_of_the_other(&user_asm, &user, &order),
        "exactly one size method body should remain (user={has_user}, order={has_order})"
    );
}

/// A T-SLOT READ does NOT fold, and the reason is honest: reading a typed property carries the
/// uninitialized-access fatal, whose message embeds the instantiated class name
/// (`Typed property Box<User>::$value …`). That message is runtime-observed data, so the two
/// bodies are genuinely different and the emission fold refuses them — the old EIR merge erased
/// the name and folded them, making one instantiation print the other's class in its fatal.
#[test]
fn test_shape_merge_keeps_t_slot_reads_whose_error_data_names_the_class() {
    let source = r#"<?php
class Box<T> {
    public function __construct(private T $value) {}
    public function get(): T { return $this->value; }
}
class User {}
class Order {}
echo get_class((new Box(new User()))->get()), "|", get_class((new Box(new Order()))->get());
"#;
    assert_eq!(compile_and_run(source), "User|Order");

    let user_asm = compile_cli_to_asm("read", source);
    let key = elephc::names::php_symbol_key("get");
    let user = elephc::names::method_symbol("Box<User>", &key);
    let order = elephc::names::method_symbol("Box<Order>", &key);
    assert!(
        user_asm.contains(&format!("{user}:")),
        "the Box<User> get body must stay (its fatal message names the class)"
    );
    assert!(
        user_asm.contains(&format!("{order}:")),
        "the Box<Order> get body must stay (its fatal message names the class)"
    );
    // The CONSTRUCTOR beside it has no class-naming data, so it still folds.
    let ctor_key = elephc::names::php_symbol_key("__construct");
    let ctor_user = elephc::names::method_symbol("Box<User>", &ctor_key);
    let ctor_order = elephc::names::method_symbol("Box<Order>", &ctor_key);
    assert!(
        one_is_thunk_of_the_other(&user_asm, &ctor_user, &ctor_order),
        "the constructor beside the read must still fold into one body"
    );
}

/// A T-SLOT WRITE with a concrete source MERGES: the source is not `Mixed`, so no typed-property
/// guard is emitted and the only class-specific thing is the slot, which is template-identical.
#[test]
fn test_shape_merge_merges_t_slot_writes() {
    let source = r#"<?php
class Box<T> {
    public function __construct(private T $value) {}
    public function set(T $value): void { $this->value = $value; }
}
class User {}
class Order {}
$u = new Box(new User());
$u->set(new User());
$o = new Box(new Order());
$o->set(new Order());
echo get_class($u), "|", get_class($o);
"#;
    assert_eq!(compile_and_run(source), "Box<User>|Box<Order>");

    let user_asm = compile_cli_to_asm("write", source);
    let key = elephc::names::php_symbol_key("set");
    let user = elephc::names::method_symbol("Box<User>", &key);
    let order = elephc::names::method_symbol("Box<Order>", &key);
    let has_user = user_asm.contains(&format!("{user}:"));
    let has_order = user_asm.contains(&format!("{order}:"));
    assert!(
        one_is_thunk_of_the_other(&user_asm, &user, &order),
        "a concrete T-slot write must merge to one body (user={has_user}, order={has_order})"
    );
}

/// A T-SLOT WRITE with a `mixed` source does NOT merge: it runs the typed-property guard, whose
/// accepted-class list is the concrete `T`.
#[test]
fn test_shape_merge_keeps_guarded_t_slot_writes() {
    let source = r#"<?php
class Box<T> {
    public function __construct(private T $value) {}
    public function put(mixed $value): void { $this->value = $value; }
}
class User {}
class Order {}
$u = new Box(new User());
$u->put(new User());
$o = new Box(new Order());
$o->put(new Order());
echo get_class($u), "|", get_class($o);
"#;
    assert_eq!(compile_and_run(source), "Box<User>|Box<Order>");

    let user_asm = compile_cli_to_asm("guardwrite", source);
    let key = elephc::names::php_symbol_key("put");
    let user = elephc::names::method_symbol("Box<User>", &key);
    let order = elephc::names::method_symbol("Box<Order>", &key);
    assert!(
        user_asm.contains(&format!("{user}:")),
        "the user put body must stay (typed-property guard)"
    );
    assert!(
        user_asm.contains(&format!("{order}:")),
        "the order put body must stay (typed-property guard)"
    );
}

/// Identical emitted delegating bodies share storage through an entry thunk.
/// `indirect()` dispatches to `direct()` and preserves each instantiation's return class;
/// folding compares emitted assembly rather than running a transitive EIR merge.
#[test]
fn test_shape_merge_merges_delegating_methods() {
    let source = r#"<?php
class Box<T> {
    public function __construct(private T $value) {}
    public function direct(): T { return $this->value; }
    public function indirect(): T { return $this->direct(); }
}
class User {}
class Order {}
echo get_class((new Box(new User()))->indirect()), "|", get_class((new Box(new Order()))->indirect());
"#;
    assert_eq!(compile_and_run(source), "User|Order");

    let user_asm = compile_cli_to_asm("disp", source);
    let key = elephc::names::php_symbol_key("indirect");
    let user = elephc::names::method_symbol("Box<User>", &key);
    let order = elephc::names::method_symbol("Box<Order>", &key);
    let has_user = user_asm.contains(&format!("{user}:"));
    let has_order = user_asm.contains(&format!("{order}:"));
    assert!(
        one_is_thunk_of_the_other(&user_asm, &user, &order),
        "a delegating method must merge to one body (user={has_user}, order={has_order})"
    );
}

/// A method that calls a callee which is NOT merged does NOT merge: the callee differs across the
/// two classes (it writes a `T` slot from a `mixed` source, so it runs the typed-property guard),
/// so the caller's dispatch would resolve to the wrong class.
#[test]
fn test_shape_merge_keeps_calls_to_unmerged_methods() {
    let source = r#"<?php
class Box<T> {
    public function __construct(private T $value) {}
    public function rawSet(mixed $v): void { $this->value = $v; }
    public function delegate(mixed $v): void { $this->rawSet($v); }
}
class User {}
class Order {}
$u = new Box(new User());
$u->delegate(new User());
$o = new Box(new Order());
$o->delegate(new Order());
echo get_class($u), "|", get_class($o);
"#;
    assert_eq!(compile_and_run(source), "Box<User>|Box<Order>");

    let user_asm = compile_cli_to_asm("callee", source);
    let key = elephc::names::php_symbol_key("delegate");
    let user = elephc::names::method_symbol("Box<User>", &key);
    let order = elephc::names::method_symbol("Box<Order>", &key);
    assert!(
        user_asm.contains(&format!("{user}:")),
        "the user delegate body must stay (its callee is not merged)"
    );
    assert!(
        user_asm.contains(&format!("{order}:")),
        "the order delegate body must stay (its callee is not merged)"
    );
}

/// Identical static helpers share a body, while instance callers retain their distinct callee
/// entry symbols. Emission folding does not transitively canonicalize calls through thunks.
#[test]
fn test_shape_merge_keeps_self_static_calls_to_distinct_entries() {
    let source = r#"<?php
class Box<T> {
    public function __construct(private T $value) {}
    private static function helper(): void { return; }
    public function touch(): void { self::helper(); }
}
class User {}
class Order {}
$u = new Box(new User());
$u->touch();
$o = new Box(new Order());
$o->touch();
echo get_class($u), "|", get_class($o);
"#;
    assert_eq!(compile_and_run(source), "Box<User>|Box<Order>");

    let user_asm = compile_cli_to_asm("selfstatic", source);
    let key = elephc::names::php_symbol_key("helper");
    let user = elephc::names::static_method_symbol("Box<User>", &key);
    let order = elephc::names::static_method_symbol("Box<Order>", &key);
    let has_user = user_asm.contains(&format!("{user}:"));
    let has_order = user_asm.contains(&format!("{order}:"));
    assert!(
        one_is_thunk_of_the_other(&user_asm, &user, &order),
        "helper must merge to one body (user={has_user}, order={has_order})"
    );
    // These instance bodies call different nominal helper entries, so they stay distinct.
    let key = elephc::names::php_symbol_key("touch");
    let user = elephc::names::method_symbol("Box<User>", &key);
    let order = elephc::names::method_symbol("Box<Order>", &key);
    let has_user = user_asm.contains(&format!("{user}:"));
    let has_order = user_asm.contains(&format!("{order}:"));
    assert!(
        has_user && has_order && !one_is_thunk_of_the_other(&user_asm, &user, &order),
        "touch must retain its distinct callee entries (user={has_user}, order={has_order})"
    );
}

/// Instance methods calling the same inherited static implementation have identical emitted
/// bodies. Both real instance entries must remain addressable, with one jumping to the other.
#[test]
fn test_shape_merge_merges_self_static_calls_to_shared_implementation() {
    let source = r#"<?php
class Helper {
    protected static function helper(): void { return; }
}
class Box<T> extends Helper {
    public function __construct(private T $value) {}
    public function touch(): void { self::helper(); }
}
class User {}
class Order {}
$u = new Box(new User());
$u->touch();
$o = new Box(new Order());
$o->touch();
echo get_class($u), "|", get_class($o);
"#;
    assert_eq!(compile_and_run(source), "Box<User>|Box<Order>");

    let asm = compile_cli_to_asm("sharedselfstatic", source);
    let key = elephc::names::php_symbol_key("touch");
    let user = elephc::names::method_symbol("Box<User>", &key);
    let order = elephc::names::method_symbol("Box<Order>", &key);
    assert!(
        asm.contains(&format!("{user}:")) && asm.contains(&format!("{order}:")),
        "both instance method entries must stay addressable"
    );
    assert!(
        one_is_thunk_of_the_other(&asm, &user, &order),
        "touch bodies calling the same static implementation must share one body"
    );
}

/// A body that BOXES its scalar parameter is NOT merged.
///
/// `value_boxing.rs` bakes the runtime value tag from the exact `PhpType` (int 0, bool 3) when a
/// concrete scalar is stored into a `mixed` slot. The tag is not an EIR immediate, so the two
/// bodies compare equal under a scalar shape while emitting different boxing code; the merge keeps
/// scalars exact, which is what this test pins.
#[test]
fn test_shape_merge_keeps_boxing_scalar_bodies() {
    let source = r#"<?php
class Sink<T> {
    public function __construct(private mixed $slot = null) {}
    public function box(T $v): void { $this->slot = $v; }
}
class User {}
class Order {}
$int = new Sink<int>();
$int->box(1);
$bool = new Sink<bool>();
$bool->box(true);
echo get_class($int), "|", get_class($bool);
"#;
    assert_eq!(compile_and_run(source), "Sink<int>|Sink<bool>");

    let user_asm = compile_cli_to_asm("boxscalar", source);
    let key = elephc::names::php_symbol_key("box");
    let int_body = elephc::names::method_symbol("Sink<int>", &key);
    let bool_body = elephc::names::method_symbol("Sink<bool>", &key);
    assert!(
        user_asm.contains(&format!("{int_body}:")),
        "the int boxing body must stay (the runtime tag is baked from the exact PhpType)"
    );
    assert!(
        user_asm.contains(&format!("{bool_body}:")),
        "the bool boxing body must stay (the runtime tag is baked from the exact PhpType)"
    );
}

/// `--report-generics` measures the residual sharing opportunity with the assembly oracle: a
/// scalar instantiation whose lowered body is identical to its sibling is reported as removable.
#[test]
fn test_report_generics_oracle_counts_identical_scalar_bodies() {
    let source = r#"<?php
function id<T>(T $x): T { return $x; }
echo id(1), id(true) ? "y" : "n";
"#;
    let reported = compile_cli_file_with_flags_capturing_stderr(source, &["--report-generics"]);
    assert!(
        reported.contains("shape-oracle: 1 slots lower to identical bodies"),
        "{reported}"
    );
    assert!(reported.contains("1 removable"), "{reported}");
}

/// A body whose scalar is BOXED lowers to different code (a different runtime tag), so the oracle
/// refuses it: the measured opportunity stays at zero rather than promising an unsound merge.
#[test]
fn test_report_generics_oracle_refuses_boxing_scalar_bodies() {
    let source = r#"<?php
class Sink<T> {
    public function __construct(private mixed $slot = null) {}
    public function box(T $v): void { $this->slot = $v; }
}
class User {}
class Order {}
$int = new Sink<int>();
$int->box(1);
$bool = new Sink<bool>();
$bool->box(true);
echo get_class($int), "|", get_class($bool);
"#;
    let reported = compile_cli_file_with_flags_capturing_stderr(source, &["--report-generics"]);
    assert!(
        !reported.contains("Sink::box"),
        "the boxing body must be refused by the oracle: {reported}"
    );
}

/// `false` is a marker the backend reads exactly as `bool` (`codegen_repr` maps it and nothing
/// consults it on its own), so a `Box<false>` instantiation compiles and runs like its `Box<bool>`
/// sibling. Storing the `bool`-typed literal into the `false`-typed promoted slot is accepted by
/// the property-store check for exactly that reason. Neither body folds: the uninitialized
/// typed-property fatal inside the read names its own instantiation, which is real data.
#[test]
fn test_shape_merge_compiles_false_into_bool_property_slot() {
    let source = r#"<?php
class Box<T> {
    public function __construct(private T $value) {}
    public function flag(): int { return $this->value ? 1 : 0; }
}
class User {}
class Order {}
$bool = new Box<bool>(true);
$no = new Box<false>(false);
echo $bool->flag(), $no->flag();
"#;
    assert_eq!(compile_and_run(source), "10");

    let user_asm = compile_cli_to_asm("falsebool", source);
    let key = elephc::names::php_symbol_key("flag");
    let bool_body = elephc::names::method_symbol("Box<bool>", &key);
    let false_body = elephc::names::method_symbol("Box<false>", &key);
    assert!(
        user_asm.contains(&format!("{bool_body}:")),
        "the Box<bool> flag body must stay (its fatal message names the class)"
    );
    assert!(
        user_asm.contains(&format!("{false_body}:")),
        "the Box<false> flag body must stay (its fatal message names the class)"
    );
}

/// A scalar instantiation whose lowered body is identical to its sibling is FOLDED at emission: the
/// survivor keeps the body and the victim's entry becomes a jump to it. The victim keeps its EIR
/// function, so a caller still resolves the instantiation's own parameter types and picks its own
/// coercion — which is what makes the int/bool fold sound where a name redirect would not be.
#[test]
fn test_shape_merge_folds_identical_scalar_bodies() {
    let source = r#"<?php
function id<T>(T $x): T { return $x; }
echo id(1), id(true) ? "y" : "n";
"#;
    assert_eq!(compile_and_run(source), "1y");

    let user_asm = compile_cli_to_asm("fold", source);
    let int_entry = elephc::names::function_symbol("id<int>");
    let bool_entry = elephc::names::function_symbol("id<bool>");
    // Both entries stay addressable.
    assert!(user_asm.contains(&format!("{int_entry}:")), "{int_entry}");
    assert!(user_asm.contains(&format!("{bool_entry}:")), "{bool_entry}");
    // Exactly one of them is the shared body and the other is a jump to it.
    let int_is_thunk = body_is_branch_to(&user_asm, &int_entry, &bool_entry);
    let bool_is_thunk = body_is_branch_to(&user_asm, &bool_entry, &int_entry);
    assert!(
        int_is_thunk ^ bool_is_thunk,
        "one instantiation must be the body and the other a jump (int_thunk={int_is_thunk}, bool_thunk={bool_is_thunk})"
    );
}

/// Whether a function's entry body is exactly a branch to `target` (a folded body's thunk).
fn body_is_branch_to(asm: &str, entry: &str, target: &str) -> bool {
    let lines: Vec<&str> = asm.lines().collect();
    let Some(position) = lines
        .iter()
        .position(|line| line.trim() == format!("{entry}:"))
    else {
        return false;
    };
    lines[position + 1..]
        .iter()
        .map(|line| line.trim())
        .find(|line| !line.is_empty() && !line.starts_with('.') && !line.starts_with('@'))
        .map(|first| first == format!("b {target}") || first == format!("jmp {target}"))
        .unwrap_or(false)
}

/// Whether one candidate's entry body is exactly a branch to the other's (a folded body's thunk).
///
/// Both entry symbols stay addressable: the survivor carries the shared body and the folded one is
/// a jump, which is what lets a caller keep resolving the instantiation's own parameter types.
fn one_is_thunk_of_the_other(asm: &str, entry_a: &str, entry_b: &str) -> bool {
    body_is_branch_to(asm, entry_a, entry_b) || body_is_branch_to(asm, entry_b, entry_a)
}

/// A dynamic property access on a `mixed` (runtime-typed) receiver does NOT fold: its
/// private-property error message names the LEXICAL class (`Cannot access private property
/// Wrap<Order>::$value`), which is instantiation-specific data. The constructor beside it folds.
#[test]
fn test_shape_merge_keeps_dynamic_access_bodies_whose_error_data_names_the_class() {
    let source = r#"<?php
class Wrap<T> {
    public function __construct(private T $value) {}
    public function pluck(mixed $row, string $key): mixed { return $row->$key; }
}
class Row { public function __construct(public int $a = 1) {} }
class User {}
class Order {}
$row = new Row();
echo (new Wrap(new User()))->pluck($row, "a"), "|", (new Wrap(new Order()))->pluck($row, "a");
"#;
    assert_eq!(compile_and_run(source), "1|1");

    let user_asm = compile_cli_to_asm("dyn", source);
    let key = elephc::names::php_symbol_key("pluck");
    let user = elephc::names::method_symbol("Wrap<User>", &key);
    let order = elephc::names::method_symbol("Wrap<Order>", &key);
    assert!(
        user_asm.contains(&format!("{user}:")),
        "the Wrap<User> pluck body must stay (its error message names the lexical class)"
    );
    assert!(
        user_asm.contains(&format!("{order}:")),
        "the Wrap<Order> pluck body must stay (its error message names the lexical class)"
    );
}

/// `unset` on a `T` slot resolves through the receiver's static type to a template-identical slot
/// with no class-naming data, so `drop` folds. `isset` does not: its typed-property read carries
/// the uninitialized-access fatal, whose message names the instantiation.
#[test]
fn test_shape_merge_folds_unset_but_keeps_isset() {
    let source = r#"<?php
class Box<T> {
    public function __construct(private T $value) {}
    public function has(): bool { return isset($this->value); }
    public function drop(): void { unset($this->value); }
}
class User {}
class Order {}
$u = new Box(new User());
$o = new Box(new Order());
echo $u->has() ? "y" : "n", $o->has() ? "y" : "n";
$u->drop();
$o->drop();
echo $u->has() ? "y" : "n";
"#;
    assert_eq!(compile_and_run(source), "yyn");

    let user_asm = compile_cli_to_asm("isset", source);
    let has_key = elephc::names::php_symbol_key("has");
    let has_user = elephc::names::method_symbol("Box<User>", &has_key);
    let has_order = elephc::names::method_symbol("Box<Order>", &has_key);
    assert!(
        user_asm.contains(&format!("{has_user}:")) && user_asm.contains(&format!("{has_order}:")),
        "isset bodies must stay split (their fatal message names the class)"
    );
    let drop_key = elephc::names::php_symbol_key("drop");
    let drop_user = elephc::names::method_symbol("Box<User>", &drop_key);
    let drop_order = elephc::names::method_symbol("Box<Order>", &drop_key);
    assert!(
        one_is_thunk_of_the_other(&user_asm, &drop_user, &drop_order),
        "unset bodies have no class-naming data and must fold to one body"
    );
}

/// An append to a `T`-element array property does NOT fold: the read-for-write carries the
/// uninitialized-typed-property fatal, whose message embeds the instantiated class name.
#[test]
fn test_shape_merge_keeps_property_append_bodies_whose_error_data_names_the_class() {
    let source = r#"<?php
class Bag<T> {
    public function __construct(private array $items = []) {}
    public function add(T $item): void { $this->items[] = $item; }
    public function count(): int { return count($this->items); }
}
class User {}
class Order {}
$b1 = new Bag<User>();
$b1->add(new User());
$b1->add(new User());
$b2 = new Bag<Order>();
$b2->add(new Order());
echo $b1->count(), $b2->count();
"#;
    assert_eq!(compile_and_run(source), "21");

    let user_asm = compile_cli_to_asm("append", source);
    for method in ["add", "count"] {
        let key = elephc::names::php_symbol_key(method);
        let user = elephc::names::method_symbol("Bag<User>", &key);
        let order = elephc::names::method_symbol("Bag<Order>", &key);
        assert!(
            user_asm.contains(&format!("{user}:")) && user_asm.contains(&format!("{order}:")),
            "{method} bodies must stay split (their fatal message names the class)"
        );
    }
}

/// A property access on a receiver whose static type is a TYPE ARGUMENT resolves the same property
/// name through a DIFFERENT class per instantiation, and different classes place it at different
/// offsets — so it must never share a body.
#[test]
fn test_shape_merge_refuses_type_argument_property_reads() {
    let source = r#"<?php
class Dog { public int $legs = 11; }
class Cat { public int $weight = 22; public int $legs = 33; }
function readLegs<T>(T $x): int { return $x->legs; }
$d = new Dog();
$c = new Cat();
echo readLegs($d), "|", readLegs($c);
"#;
    assert_eq!(compile_and_run(source), "11|33");

    let user_asm = compile_cli_to_asm("readlegs", source);
    assert!(
        user_asm.contains(&format!(
            "{}:",
            elephc::names::function_symbol("readLegs<Dog>")
        )),
        "the Dog body must stay addressable"
    );
    assert!(
        user_asm.contains(&format!(
            "{}:",
            elephc::names::function_symbol("readLegs<Cat>")
        )),
        "the Cat body must stay addressable"
    );
}

/// The write form is worse than the read: a shared store would land in the WRONG FIELD of the
/// other instantiation's class.
#[test]
fn test_shape_merge_refuses_type_argument_property_writes() {
    let source = r#"<?php
class Dog { public int $legs = 0; }
class Cat { public int $weight = 0; public int $legs = 0; }
function setLegs<T>(T $x, int $v): void { $x->legs = $v; }
$d = new Dog();
setLegs($d, 11);
$c = new Cat();
setLegs($c, 33);
echo $d->legs, "|", $c->legs, ",", $c->weight;
"#;
    assert_eq!(compile_and_run(source), "11|33,0");
}

/// The same hole through a method: `$this->v->legs` reads a property of a TYPE-ARGUMENT receiver.
#[test]
fn test_shape_merge_refuses_method_type_argument_property_reads() {
    let source = r#"<?php
class Dog { public int $legs = 11; }
class Cat { public int $weight = 22; public int $legs = 33; }
class Box<T> {
    public function __construct(private T $v) {}
    public function legs(): int { return $this->v->legs; }
}
echo (new Box(new Dog()))->legs(), "|", (new Box(new Cat()))->legs();
"#;
    assert_eq!(compile_and_run(source), "11|33");
}

/// A non-generic subclass whose parent is a folded-away instantiation must still resolve the
/// method: its inherited implementation map has to reach the survivor's symbol.
#[test]
fn test_shape_merge_redirects_inherited_method_implementations() {
    let source = r#"<?php
class Dog {}
class Order {}
class Pen<T> { public function size(): int { return 7; } }
$u = new Pen<Dog>();
echo $u->size();
class BigPen extends Pen<Order> {}
$b = new BigPen();
echo "|", $b->size();
"#;
    assert_eq!(compile_and_run(source), "7|7");
}

/// Function-static storage is per-instantiation and its symbol is derived from the function's own
/// name, which the body erasure would fold into one counter.
#[test]
fn test_shape_merge_keeps_independent_function_static_storage() {
    let source = r#"<?php
class A {}
class B {}
function sequence<T>(T $x): int {
    static $n = 0;
    return ++$n;
}
$a = new A();
$b = new B();
echo sequence($a), sequence($b), sequence($a);
"#;
    assert_eq!(compile_and_run(source), "112");
}

/// Builtin lowering that consults the function's own lexical class (here: the object-vars
/// visibility projection) is nominal, so the method must not share a body across instantiations.
#[test]
fn test_shape_merge_refuses_lexical_class_builtin_lowering() {
    let source = r#"<?php
class A { private int $hidden = 1; }
class B { private int $hidden = 2; }
class Box<T> {
    public function __construct(private T $value) {}
    public function visible(): int {
        return count(get_object_vars($this));
    }
}
echo (new Box(new A()))->visible(), "|", (new Box(new B()))->visible();
"#;
    assert_eq!(compile_and_run(source), "1|1");
}

/// The body erasure must not swallow a sibling callee whose mangled name merely has the
/// candidate's entry as a PREFIX: `foobar` embeds `foo`, so a substring replace would normalize
/// two distinct call operands to the same text and fold both `foo` instantiations into one body
/// that calls the wrong `foobar` — `Box<bool>::foo` would answer `integer`.
#[test]
fn test_fold_keeps_distinct_sibling_callee_symbols() {
    let source = r#"<?php
class Box<T> {
    public static function foo(T $x): string {
        return self::foobar($x);
    }
    public static function foobar(T $x): string {
        return gettype($x);
    }
}
echo Box<int>::foo(1), "|", Box<bool>::foo(true);
"#;
    assert_eq!(
        compile_cli_file_and_run_with_flags(source, &["--no-ir-opt"]),
        "integer|boolean"
    );
}

/// A `false`-declared property slot only accepts a false payload. A boxed mixed argument is
/// refused at the CALL (php's argument `TypeError` names what was given), a direct bool store
/// is refused at the property (`Cannot assign true to property … of type false`), and neither
/// ever reaches the slot with a `true` payload.
#[test]
fn test_false_property_slot_rejects_a_true_payload() {
    let source = r#"<?php
class C {
    public function __construct(public false $v) {}
}
function wrap(mixed $x): C {
    return new C($x);
}
$c = wrap(false);
var_dump($c->v);
try {
    $d = wrap(true);
    var_dump($d->v);
} catch (TypeError $e) {
    echo "TypeError: ", $e->getMessage();
}
"#;
    assert_eq!(
        compile_and_run(source),
        "bool(false)\nTypeError: C::__construct(): Argument #1 ($v) must be of type false, true given"
    );
}

/// Counters embed PER-NAME state the isolated fold comparison cannot see, so a diagnostics build
/// must keep every body unfolded — otherwise both call sites would attribute to the survivor and
/// the victim would report nothing.
#[test]
fn test_counters_keep_per_instantiation_bodies_and_attribution() {
    let source = r#"<?php
function idf<T>(T $x): T { return $x; }
echo idf(1), idf(true);
"#;
    let dir = make_cli_test_dir("elephc_fold_counters");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();

    let compile = elephc_cli_command(&dir)
        .arg("--no-ir-opt")
        .arg("--counters")
        .arg(&php_path)
        .output()
        .expect("run elephc CLI");
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run the counters binary");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "11");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("idf<int> 1"), "{stderr}");
    assert!(stderr.contains("idf<bool> 1"), "{stderr}");
    assert!(
        !stderr.contains("idf<bool> 2"),
        "the survivor must not absorb the victim's calls: {stderr}"
    );
}

/// The source map discovers functions through `@fn`/`@endfn` regions; a folded body is a real
/// function whose range is one jump, so both instantiations must stay in the map.
#[test]
fn test_source_map_records_folded_entries() {
    let source = r#"<?php
function idf<T>(T $x): T { return $x; }
echo idf(1), idf(true);
"#;
    let dir = make_cli_test_dir("elephc_fold_source_map");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg("--source-map")
        .arg(&php_path)
        .output()
        .expect("run elephc CLI");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let map = fs::read_to_string(dir.join("main.map")).expect("source map sidecar");
    assert!(map.contains("\"idf<int>\""), "{map}");
    assert!(map.contains("\"idf<bool>\""), "{map}");
}

/// A one-line doc block parses ONE segment — its opening tag (`@deprecated` here, unread).
/// The generic tags on such a compact line are its description and do not bind; write the tags
/// on separate lines. The described `@return` cannot retype the bound the real `@return` line
/// sets (first read wins), which is what this program's success now shows.
#[test]
fn test_docblock_compact_line_reads_only_its_opening_tag() {
    let out = compile_and_run(r#"<?php
/** @deprecated @template T of int @param T $v @return T */
function f($v) { return $v; }
echo f("bad"), "|", f(7);
"#);
    assert_eq!(out, "bad|7");
}

/// The self-symbol erasure checks BOTH symbol boundaries: a sibling callee whose mangled name
/// ENDS with the candidate's entry (callee `xf` beside candidate `f`) must keep its own call
/// operand, or the folded body answers the survivor's callee — `f(true)` answering `integer`.
#[test]
fn test_fold_keeps_distinct_suffix_sibling_callees() {
    let source = r#"<?php
function xf<T>(T $x): string {
    $acc = 0;
    for ($i = 0; $i < 3; $i++) { $acc = $acc + $i; }
    return gettype($x) . ":" . $acc;
}
function f<T>(T $x): string {
    $out = "";
    for ($j = 0; $j < 2; $j++) { $out = $out . xf($x); }
    return $out;
}
echo f(1), "|", f(true);
"#;
    assert_eq!(
        compile_and_run(source),
        "integer:3integer:3|boolean:3boolean:3"
    );
}

/// The requested `get`-calling-`widget` shape: `widget`'s mangled symbol ENDS with `get`'s, so
/// the erasure must not turn `get<bool>`'s call into the survivor's `widget<int>`.
#[test]
fn test_fold_keeps_widget_calls_from_get() {
    let source = r#"<?php
function widget<T>(T $x): string { return gettype($x); }
function get<T>(T $x): string { return widget($x); }
echo get(1), "|", get(true);
"#;
    assert_eq!(compile_and_run(source), "integer|boolean");
}

/// A `false`-declared constructor parameter accepts ONLY the exact false bool: a boxed mixed
/// argument is refused at the CALL with php's argument `TypeError` naming the runtime type of
/// what was given — never the bool truthiness cast, which would fold `0`, `""` and `[]`.
#[test]
fn test_false_param_refuses_a_boxed_mixed_argument() {
    let source = r#"<?php
class C { public function __construct(public false $v) {} }
function wrap(mixed $x): C { return new C($x); }
foreach ([false, true, 0, 1, "", "false", []] as $x) {
    try { $c = wrap($x); echo "stored: "; var_dump($c->v); }
    catch (TypeError $e) { echo "TypeError\n"; }
}
try { wrap(0); } catch (TypeError $e) { echo $e->getMessage(); }
"#;
    assert_eq!(
        compile_and_run(source),
        "stored: bool(false)\nTypeError\nTypeError\nTypeError\nTypeError\nTypeError\nTypeError\n\
         C::__construct(): Argument #1 ($v) must be of type false, int given"
    );
}

/// A `false`-declared REFERENCE property enforces its payload on the reference-store path too:
/// `true` through the property slot throws php's `TypeError` instead of storing. (Writes
/// through a reference ALIAS taken from the property remain a pre-existing gap: reference
/// cells carry no declared type, on main and here alike.)
#[test]
fn test_false_reference_property_rejects_a_true_store() {
    let source = r#"<?php
class C { public function __construct(public false &$v) {} }
$x = false;
$c = new C($x);
function setRef(mixed $value, C $target): void { $target->v = $value; }
try { setRef(true, $c); echo "stored\n"; }
catch (TypeError $e) { echo "TypeError\n"; }
var_dump($c->v);
"#;
    assert_eq!(compile_and_run(source), "TypeError\nbool(false)\n");
}

/// PHP 8.2's standalone `false` parses as a DECLARED property type (not only the promoted
/// constructor form): the checker refuses a statically-true store, the runtime guard refuses a
/// boxed mixed that is not the exact false bool, and a stored false reads back.
#[test]
fn test_declared_false_property_type_round_trips() {
    let source = r#"<?php
class C { public false $v; }
$c = new C();
function w(mixed $x, C $c): void { $c->v = $x; }
try { w(false, $c); echo "false ok: "; var_dump($c->v); } catch (TypeError $e) { echo "false TypeError\n"; }
try { w(true, $c); echo "true ok\n"; } catch (TypeError $e) { echo "true TypeError\n"; }
try { w(0, $c); echo "zero ok\n"; } catch (TypeError $e) { echo "zero TypeError\n"; }
var_dump($c->v);
"#;
    let error = compile_cli_file_with_flags_expect_failure(
        r#"<?php
class C { public false $v; }
$c = new C();
$c->v = true;
"#,
        &[],
    );
    assert!(
        error.contains("expects false, got bool"),
        "a statically-true store is a compile-time type error: {error}"
    );
    assert_eq!(
        compile_and_run(source),
        "false ok: bool(false)\ntrue TypeError\nzero TypeError\nbool(false)\n"
    );
}
