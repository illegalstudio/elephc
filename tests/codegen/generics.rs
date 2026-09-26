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
