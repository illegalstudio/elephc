//! Purpose:
//! Integration tests for flow-sensitive type narrowing on `is_*` / `instanceof` guards.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixtures exercise scalar/object narrowing (functions and methods), negated guards, the early-return
//!   idiom, `instanceof` narrowing with method dispatch on a runtime-Mixed receiver, `if`/`elseif`
//!   chains, and `: never`-function divergence that keeps the complement after an exhaustive chain.
//!   The guarded variables are untyped parameters that are unions at runtime (heterogeneous calls),
//!   so these tests depend on both the union parameter inference and the narrowing. Outputs match PHP.
//! - Receivers typed by an interface or a parent class and narrowed to a class with a method the
//!   declared type lacks dispatch on the runtime class.

use super::*;

/// Verifies the false edge of `is_object()` removes the object member from a union, allowing an
/// object branch converted to its class name to join the untouched string branch as `string`.
#[test]
fn test_is_object_union_complement_after_branch_assignment() {
    let out = compile_and_run(
        r#"<?php
class NarrowedClassName {}

function normalizedClassName(object|string $value): string {
    if (is_object($value)) {
        $value = $value::class;
    }
    $lookup = [];
    $lookup[$value] = "ok";
    return $lookup[$value];
}

echo normalizedClassName(new NarrowedClassName());
echo normalizedClassName("literal");
"#,
    );
    assert_eq!(out, "okok");
}

/// Verifies a breaking switch case cannot leak assignments into a sibling default branch.
#[test]
fn test_switch_break_keeps_sibling_type_environments_isolated() {
    let out = compile_and_run(
        r#"<?php
function requireString(string $value): string {
    return strtoupper($value);
}

function choose(bool $first): string {
    $value = 'ok';
    switch (true) {
        case $first:
            $value = [1, 2];
            break;
        default:
            return requireString($value);
    }

    return 'first';
}

echo choose(false), ':', choose(true);
"#,
    );
    assert_eq!(out, "OK:first");
}

/// Verifies a switch without a default preserves its unmatched nullable path for list unpacking.
#[test]
fn test_switch_without_default_preserves_nullable_fallthrough() {
    let out = compile_and_run(
        r#"<?php
function consume(?array $entry, int $mode): string {
    if ($entry === null) {
        switch ($mode) {
            case 1:
                return 'returned';
        }
    }

    [$key, $value] = $entry;
    return ($key === null ? 'null' : $key).':'.($value === null ? 'null' : $value);
}

echo consume(null, 0), '|', consume(['k', 'v'], 0);
"#,
    );
    assert_eq!(out, "null:null|k:v");
}

/// Verifies the literal `false` subtype remains callable and uses the normal boolean runtime
/// representation while `int|false` narrows to int after a divergent strict-false guard.
#[test]
fn test_literal_false_type_and_strict_guard_runtime() {
    let out = compile_and_run(
        r#"<?php
        function onlyFalse(false $value): string { return $value === false ? "false" : "bad"; }
        function returnFalse(): false { return false; }
        function requireInt(int|false $value): int {
            if ($value === false) { throw new Exception("false"); }
            return $value;
        }
        echo onlyFalse(returnFalse()), ":", requireInt(9);
        "#,
    );
    assert_eq!(out, "false:9");
}

/// Verifies a builtin `int|false` result keeps the literal-false member precise, allowing the
/// standard divergent guard to leave a plain integer on the fallthrough path.
#[test]
fn test_builtin_false_sentinel_narrows_to_success_type() {
    let out = compile_and_run(
        r#"<?php
        function requirePosition(string $haystack, string $needle): int {
            $position = strpos($haystack, $needle);
            if ($position === false) { throw new Exception("not found"); }
            return $position;
        }
        echo requirePosition("abcdef", "cd");
        "#,
    );
    assert_eq!(out, "2");
}

/// Verifies an unrelated local assignment preserves a stable property narrowing in the same
/// branch; invalidation is scoped to the receiver that was actually rebound.
#[test]
fn test_property_narrowing_survives_unrelated_local_assignment() {
    let out = compile_and_run(
        r#"<?php
        final class NarrowedValue { public function __construct(public string $text) {} }
        final class NarrowedBox {
            public function __construct(public ?NarrowedValue $value) {}
        }
        function readBox(NarrowedBox $box): NarrowedValue {
            if (!$box->value instanceof NarrowedValue) { throw new Exception("missing"); }
            $unrelated = 1;
            return $box->value;
        }
        echo readBox(new NarrowedBox(new NarrowedValue("ok")))->text;
        "#,
    );
    assert_eq!(out, "ok");
}

/// Verifies every truthy fact from a nested `&&` prefix reaches the final operand.
#[test]
fn test_nested_short_circuit_prefix_preserves_property_instanceof_narrowing() {
    let out = compile_and_run(
        r#"<?php
interface Prototype { public function ready(): bool; }
class ConcretePrototype implements Prototype {
    public string $value = 'ok';
    public string $other = 'initial';
    public function ready(): bool { return true; }
}
class PrototypeHolder {
    public function __construct(private Prototype $prototype) {}
    public function empty(): bool {
        $other = new ConcretePrototype();
        if ($this->prototype instanceof ConcretePrototype
            && $this->prototype->ready()
            && array_key_exists('value', ['value' => true])) {
            current([$this->prototype]);
            $other->other = 'changed';
            return $this->prototype->value === 'ok';
        }
        return false;
    }
}
echo (new PrototypeHolder(new ConcretePrototype()))->empty() ? 'ok' : 'bad';
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `is_int` narrowing in a function: the then-branch uses the value as an int and the
/// else-branch as a string, with the parameter being `int|string` across the two call sites.
#[test]
fn test_is_int_narrowing_function_then_else() {
    let out = compile_and_run(
        r#"<?php
        function f($x): void {
            if (is_int($x)) { echo "int:", $x, "\n"; } else { echo "str:", $x, "\n"; }
        }
        f(5);
        f("hi");
        "#,
    );
    assert_eq!(out, "int:5\nstr:hi\n");
}

/// Verifies `is_int` narrowing on an instance-method parameter feeding a typed `int` property:
/// the narrowed value is stored into `int $a`, while non-int calls are ignored.
#[test]
fn test_is_int_narrowing_method_into_typed_property() {
    let out = compile_and_run(
        r#"<?php
        class Bar {
            public int $a = 0;
            public function set($x): void { if (is_int($x)) { $this->a = $x; } }
        }
        $o = new Bar();
        $o->set(7);
        $o->set("ignored");
        echo $o->a;
        "#,
    );
    assert_eq!(out, "7");
}

/// Verifies a negated guard (`!is_int`) narrows the else-path (fallthrough) to int.
#[test]
fn test_negated_is_int_guard_narrows_fallthrough() {
    let out = compile_and_run(
        r#"<?php
        function f($x): string { if (!is_int($x)) { return "notint"; } return "int:" . $x; }
        echo f(5), "|", f("hi");
        "#,
    );
    assert_eq!(out, "int:5|notint");
}

/// Verifies `is_string` narrowing lets the guarded value be used by a string builtin.
#[test]
fn test_is_string_narrowing_allows_strlen() {
    let out = compile_and_run(
        r#"<?php
        function f($x): int { if (is_string($x)) { return strlen($x); } return -1; }
        echo f("abc"), "|", f(5);
        "#,
    );
    assert_eq!(out, "3|-1");
}

/// Verifies a type predicate narrows the local written by its assignment-expression operand.
#[test]
fn test_is_string_narrows_assignment_expression_target() {
    let out = compile_and_run(
        r#"<?php
function available() {}
function check($candidate): void {
    if (is_string($name = $candidate)) {
        echo function_exists($name) ? 'yes' : 'no';
    }
}
check('available');
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies the early-return idiom: a guard with no `else` whose body always returns narrows the
/// statements after the `if` to the complement type.
#[test]
fn test_early_return_guard_narrows_remainder() {
    let out = compile_and_run(
        r#"<?php
        function f($x): string { if (!is_string($x)) { return "no"; } return "len" . strlen($x); }
        echo f("abc"), "|", f(5);
        "#,
    );
    assert_eq!(out, "len3|no");
}

/// Verifies `instanceof` narrowing lets a method be called on the narrowed object, dispatched on
/// the runtime class of a value that is `Foo|int` at runtime.
#[test]
fn test_instanceof_narrowing_allows_method_call() {
    let out = compile_and_run(
        r#"<?php
        class Foo { public function ts(): int { return 42; } }
        function g($x): int { if ($x instanceof Foo) { return $x->ts(); } return -1; }
        echo g(new Foo()), "|", g(5);
        "#,
    );
    assert_eq!(out, "42|-1");
}

/// Verifies `instanceof` narrowing picks one class out of a union of two object classes and
/// dispatches its method correctly, with the non-matching class taking the else-path.
#[test]
fn test_instanceof_narrowing_two_object_union() {
    let out = compile_and_run(
        r#"<?php
        class A { public function name(): string { return "A"; } }
        class B { public function name(): string { return "B"; } }
        function pick($x): string { if ($x instanceof A) { return $x->name(); } return "notA"; }
        echo pick(new A()), "|", pick(new B());
        "#,
    );
    assert_eq!(out, "A|notA");
}

/// Verifies a parameter typed by an interface, narrowed by `instanceof` to a class, can call a
/// method only the class declares; the other branch still calls the interface method.
#[test]
fn test_instanceof_narrowed_interface_receiver_calls_class_method() {
    let out = compile_and_run(
        r#"<?php
        interface Shape { public function name(): string; }
        final class Square implements Shape {
            public function name(): string { return "square"; }
            public function side(): int { return 3; }
        }
        final class Circle implements Shape { public function name(): string { return "circle"; } }
        function describe(Shape $shape): string {
            if ($shape instanceof Square) { return $shape->name() . " " . $shape->side(); }
            return $shape->name();
        }
        echo describe(new Square()), "|", describe(new Circle());
        "#,
    );
    assert_eq!(out, "square 3|circle");
}

/// Verifies a parameter typed by a parent class, narrowed by `instanceof` to a subclass, can call
/// a method only the subclass declares.
#[test]
fn test_instanceof_narrowed_parent_receiver_calls_subclass_method() {
    let out = compile_and_run(
        r#"<?php
        class Animal { public function kind(): string { return "animal"; } }
        class Dog extends Animal { public function bark(): string { return "woof"; } }
        function speak(Animal $animal): string {
            if ($animal instanceof Dog) { return $animal->bark(); }
            return $animal->kind();
        }
        echo speak(new Dog()), "|", speak(new Animal());
        "#,
    );
    assert_eq!(out, "woof|animal");
}

/// Verifies the early-return form narrows an interface-typed receiver for the rest of the body.
#[test]
fn test_negated_instanceof_early_return_narrows_interface_receiver() {
    let out = compile_and_run(
        r#"<?php
        interface Shape { public function name(): string; }
        final class Square implements Shape {
            public function name(): string { return "square"; }
            public function side(): int { return 4; }
        }
        final class Circle implements Shape { public function name(): string { return "circle"; } }
        function side(Shape $shape): int {
            if (!$shape instanceof Square) { return 0; }
            return $shape->side();
        }
        echo side(new Square()), "|", side(new Circle());
        "#,
    );
    assert_eq!(out, "4|0");
}

/// Verifies a local declared as an interface can be narrowed to a concrete implementation,
/// dispatched through that implementation, and returned through the interface without losing it.
#[test]
fn test_instanceof_narrowing_reuses_nominal_object_storage() {
    let out = compile_and_run(
        r#"<?php
        interface NamedValue { public function name(): string; }
        class DetailedValue implements NamedValue {
            public function name(): string { return "base"; }
            public function detail(): string { return "detail"; }
        }
        function inspect(NamedValue $value): NamedValue {
            if ($value instanceof DetailedValue) { echo $value->detail(), "|"; }
            return $value;
        }
        echo inspect(new DetailedValue())->name();
        "#,
    );
    assert_eq!(out, "detail|base");
}

/// Verifies a gradual receiver resolves `instanceof` interface metadata with PHP's
/// case-insensitive type-name rules before dispatching the narrowed object.
#[test]
fn test_gradual_interface_boundary_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
        interface CaseContract { public function value(): string; }
        class CaseImplementation implements CaseContract {
            public function value(): string { return "ok"; }
        }
        function narrowCase(mixed $value): string {
            if ($value instanceof casecontract) { return $value->value(); }
            return "missing";
        }
        echo narrowCase(new CaseImplementation());
        "#,
    );
    assert_eq!(out, "ok");
}

/// Verifies the full overload pattern: an `is_int` guard stores the int into a typed property,
/// while the else-branch calls a method on the narrowed object (dispatched on its runtime class).
#[test]
fn test_overload_pattern_int_or_object() {
    let out = compile_and_run(
        r#"<?php
        class Foo { public function ts(): int { return 42; } }
        class Bar {
            public int $a = 0;
            public int $b = 0;
            public function set($x): void {
                if (is_int($x)) { $this->a = $x; } else { $this->b = $x->ts(); }
            }
        }
        $o = new Bar();
        $o->set(5);
        $o->set(new Foo());
        echo $o->a, "|", $o->b;
        "#,
    );
    assert_eq!(out, "5|42");
}

/// Tests flow-sensitive narrowing across an `if` / `elseif` / `else` chain.
/// Each clause should see the appropriate narrowed (or complement) type.
#[test]
fn test_elseif_narrowing_chain() {
    let out = compile_and_run(
        r#"<?php
        function describe($x): string {
            if (is_int($x)) {
                return "int:" . ($x + 1);
            } elseif (is_string($x)) {
                return "str:" . $x;
            }
            return "other";
        }
        echo describe(41), "|", describe("hi"), "|", describe(3.14);
        "#,
    );
    assert_eq!(out, "int:42|str:hi|other");
}

/// Tests that a branch ending in a `: never` function call lets the code after an
/// if/elseif chain (with no final else) keep the complement type. After the chain `$x`
/// must be narrowed to `int`, which the `: int` return type requires; without `never`
/// detection `$x` would still be `mixed` and `return $x` would be rejected (a `mixed`
/// value does not satisfy an `int` return). The `mixed` parameter avoids the method-call
/// route, which would not distinguish the cases since `mixed` receivers dispatch
/// dynamically.
#[test]
fn test_elseif_chain_with_never_divergence() {
    let out = compile_and_run(
        r#"<?php
        function fail(): never {
            throw new \Exception("boom");
        }

        function classify(mixed $x): int {
            if (is_string($x)) {
                return 0;
            } elseif (!is_int($x)) {
                fail();
            }
            // All clauses diverge and there is no else, so reaching here means every guard
            // was false => $x must be an int.
            return $x;
        }

        echo classify("hi"), "|", classify(41);
        "#,
    );
    assert_eq!(out, "0|41");
}

/// Regression: a non-diverging clause *before* a diverging type guard must not leave the
/// variable narrowed after the `if`. The `$flag` branch reaches the trailing statement
/// without ever evaluating the `instanceof` guard, so `$x` must stay `mixed` (where `+` is
/// allowed) rather than being narrowed to `Box` (where arithmetic is rejected). A rule that
/// kept the complement whenever only the last clause diverged would fail to compile here.
#[test]
fn test_narrowing_not_kept_when_earlier_clause_falls_through() {
    let out = compile_and_run(
        r#"<?php
        class Box {}
        function f(mixed $x, bool $flag): void {
            if ($flag) {
                echo "";
            } elseif (!($x instanceof Box)) {
                return;
            }
            echo $x + 1;
        }
        f(41, true);
        "#,
    );
    assert_eq!(out, "42");
}

/// Regression: when different clauses narrow different variables, every narrowed variable
/// must be restored after the `if`, not only the first. The non-diverging `$x` clause means
/// the trailing statement can run with `$y` unconstrained, so `$y` must stay `mixed` rather
/// than leaking the `Box` narrowing from its diverging clause. A single-slot restore would
/// leave `$y` as `Box` and reject the arithmetic.
#[test]
fn test_narrowing_restores_all_narrowed_variables() {
    let out = compile_and_run(
        r#"<?php
        class Box {}
        function f(mixed $x, mixed $y): void {
            if ($x instanceof Box) {
                echo "";
            } elseif (!($y instanceof Box)) {
                return;
            }
            echo $y + 1;
        }
        f(new Box(), 7);
        "#,
    );
    assert_eq!(out, "8");
}

/// Verifies a null guard whose body returns before unreachable trailing code still narrows the
/// `?array` value to `Array` for the list unpack after the `if` (issue #590 shape 1). The former
/// last-statement-only model saw the dead `echo` instead of the terminal `return` and dropped the
/// narrowing.
#[test]
fn test_narrow_after_terminal_return_before_unreachable_code() {
    let out = compile_and_run(
        r#"<?php
        function consume(?array $entry): string {
            if ($entry === null) {
                return "empty";
                echo "unreachable";
            }
            [$key, $value] = $entry;
            return $key . "=" . $value;
        }
        echo consume(["a", "b"]), "|", consume(null);
        "#,
    );
    assert_eq!(out, "a=b|empty");
}

/// Verifies a null guard whose nested `if` terminates on every branch (return / throw) narrows the
/// `?array` value for the following list unpack (issue #590 shape 2).
#[test]
fn test_narrow_after_nested_if_all_branches_terminate() {
    let out = compile_and_run(
        r#"<?php
        function consume(?array $entry, bool $flag): string {
            if ($entry === null) {
                if ($flag) {
                    return "flag";
                } else {
                    throw new Exception("missing");
                }
            }
            [$key, $value] = $entry;
            return $key . "=" . $value;
        }
        echo consume(["a", "b"], true);
        "#,
    );
    assert_eq!(out, "a=b");
}

/// Verifies `exit()` and `die()` participate in the shared structural traversal when both paths of
/// a nested `if` diverge. The old shallow checker overlay did not inspect the inner branches.
#[test]
fn test_narrow_after_nested_if_exit_and_die_paths() {
    let out = compile_and_run(
        r#"<?php
        function consume(?array $entry, bool $flag): string {
            if ($entry === null) {
                if ($flag) {
                    exit(1);
                } else {
                    die(2);
                }
            }
            [$key, $value] = $entry;
            return $key . "=" . $value;
        }
        echo consume(["a", "b"], true);
        "#,
    );
    assert_eq!(out, "a=b");
}

/// Verifies checker-known `never` calls are threaded through nested structural analysis and can
/// combine with an ordinary terminal branch.
#[test]
fn test_narrow_after_nested_if_never_and_return_paths() {
    let out = compile_and_run(
        r#"<?php
        function fail(string $message): never { throw new Exception($message); }
        function consume(?array $entry, bool $flag): string {
            if ($entry === null) {
                if ($flag) {
                    fail("missing");
                } else {
                    return "empty";
                }
            }
            [$key, $value] = $entry;
            return $key . "=" . $value;
        }
        echo consume(["a", "b"], true);
        "#,
    );
    assert_eq!(out, "a=b");
}

/// Verifies a statically infinite loop without a reachable `break` is non-fallthrough for
/// post-guard narrowing, matching the shared function-exit analysis.
#[test]
fn test_narrow_after_statically_infinite_loop() {
    let out = compile_and_run(
        r#"<?php
        function consume(?array $entry): string {
            if ($entry === null) {
                while (true) {
                    continue;
                }
            }
            [$key, $value] = $entry;
            return $key . "=" . $value;
        }
        echo consume(["a", "b"]);
        "#,
    );
    assert_eq!(out, "a=b");
}

/// Verifies a `do`/`while` whose mandatory first iteration exits through a checker-known `never`
/// call is non-fallthrough even when its condition is false.
#[test]
fn test_narrow_after_do_while_body_never_call() {
    let out = compile_and_run(
        r#"<?php
        function fail(string $message): never { throw new Exception($message); }
        function consume(?array $entry): string {
            if ($entry === null) {
                do {
                    fail("missing");
                } while (false);
            }
            [$key, $value] = $entry;
            return $key . "=" . $value;
        }
        echo consume(["a", "b"]);
        "#,
    );
    assert_eq!(out, "a=b");
}

/// Verifies a null guard whose nested `switch` exits on its single case and its `default` narrows
/// the `?array` value for the following list unpack.
#[test]
fn test_narrow_after_nested_switch_all_paths_terminate() {
    let out = compile_and_run(
        r#"<?php
        function consume(?array $entry, int $mode): string {
            if ($entry === null) {
                switch ($mode) {
                    case 1:
                        return "one";
                    default:
                        throw new Exception("missing");
                }
            }
            [$key, $value] = $entry;
            return $key . "=" . $value;
        }
        echo consume(["a", "b"], 1);
        "#,
    );
    assert_eq!(out, "a=b");
}

/// Verifies a nested exhaustive `switch` combines a checker-known `never` call with shared
/// `exit()` termination and still preserves the post-guard complement.
#[test]
fn test_narrow_after_nested_switch_never_and_exit_paths() {
    let out = compile_and_run(
        r#"<?php
        function fail(string $message): never { throw new Exception($message); }
        function consume(?array $entry, int $mode): string {
            if ($entry === null) {
                switch ($mode) {
                    case 1:
                        fail("missing");
                    default:
                        exit(2);
                }
            }
            [$key, $value] = $entry;
            return $key . "=" . $value;
        }
        echo consume(["a", "b"], 1);
        "#,
    );
    assert_eq!(out, "a=b");
}

/// Verifies a null guard whose nested `try`/`catch` exits on both the try and the catch bodies
/// narrows the `?array` value for the following list unpack.
#[test]
fn test_narrow_after_nested_try_all_paths_terminate() {
    let out = compile_and_run(
        r#"<?php
        function consume(?array $entry): string {
            if ($entry === null) {
                try {
                    throw new Exception("inner");
                } catch (Throwable $e) {
                    return "caught";
                }
            }
            [$key, $value] = $entry;
            return $key . "=" . $value;
        }
        echo consume(["a", "b"]), "|", consume(null);
        "#,
    );
    assert_eq!(out, "a=b|caught");
}

/// Verifies checker-known `never` divergence is propagated through `try` analysis while the catch
/// path uses shared `die()` termination.
#[test]
fn test_narrow_after_nested_try_never_and_die_paths() {
    let out = compile_and_run(
        r#"<?php
        function fail(string $message): never { throw new Exception($message); }
        function consume(?array $entry): string {
            if ($entry === null) {
                try {
                    fail("missing");
                } catch (Throwable $e) {
                    die(2);
                }
            }
            [$key, $value] = $entry;
            return $key . "=" . $value;
        }
        echo consume(["a", "b"]);
        "#,
    );
    assert_eq!(out, "a=b");
}

/// Verifies a null guard ending in `continue` before unreachable trailing code keeps the `?array`
/// narrowing for the list unpack later in the same loop body. `continue` cannot reach the following
/// statement even though it does not exit the function, so the complement is still sound.
#[test]
fn test_narrow_after_continue_before_unreachable_code() {
    let out = compile_and_run(
        r#"<?php
        function row(int $n): ?array {
            if ($n < 0) { return null; }
            return ["k" . $n, "v" . $n];
        }
        $out = "";
        foreach ([1, -1, 2] as $n) {
            $entry = row($n);
            if ($entry === null) {
                continue;
                echo "unreachable";
            }
            [$key, $value] = $entry;
            $out .= $key . "=" . $value . ";";
        }
        echo $out;
        "#,
    );
    assert_eq!(out, "k1=v1;k2=v2;");
}

/// Verifies shared `exit()` termination still fires when the call is not the block's last
/// statement, allowing the structural block scan to ignore unreachable trailing code.
#[test]
fn test_narrow_after_exit_before_unreachable_code() {
    let out = compile_and_run(
        r#"<?php
        function consume(?array $entry): string {
            if ($entry === null) {
                exit(1);
                echo "unreachable";
            }
            [$key, $value] = $entry;
            return $key . "=" . $value;
        }
        echo consume(["a", "b"]);
        "#,
    );
    assert_eq!(out, "a=b");
}

/// Verifies checker-known `never` divergence still fires when the call precedes unreachable
/// trailing code. The checker supplies the function-table lookup to the shared structural model.
#[test]
fn test_narrow_after_never_call_before_unreachable_code() {
    let out = compile_and_run(
        r#"<?php
        function fail(string $m): never { throw new Exception($m); }
        function consume(?array $entry): string {
            if ($entry === null) {
                fail("empty");
                echo "unreachable";
            }
            [$key, $value] = $entry;
            return $key . "=" . $value;
        }
        echo consume(["a", "b"]);
        "#,
    );
    assert_eq!(out, "a=b");
}

/// Verifies the classic singleton: a nullable static property narrowed by an `=== null` guard
/// whose then-branch assigns it is non-null on both merge paths, so the `: S` return checks.
#[test]
fn test_nullable_static_property_singleton_narrows_after_if_assign() {
    let out = compile_and_run(
        r#"<?php
class S {
    private static ?S $inst = null;
    public int $v = 7;
    public static function get(): S {
        if (self::$inst === null) { self::$inst = new S(); }
        return self::$inst;
    }
}
echo S::get()->v, S::get()->v;
"#,
    );
    assert_eq!(out, "77");
}

/// Verifies `!isset(self::$p)` narrows the same way `self::$p === null` does.
#[test]
fn test_nullable_static_property_singleton_narrows_after_isset_guard() {
    let out = compile_and_run(
        r#"<?php
class S {
    private static ?S $inst = null;
    public int $v = 7;
    public static function get(): S {
        if (!isset(self::$inst)) { self::$inst = new S(); }
        return self::$inst;
    }
}
echo S::get()->v;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies `self::$p ??= new S();` leaves the static property non-null for the following return.
#[test]
fn test_nullable_static_property_narrows_after_null_coalescing_assign() {
    let out = compile_and_run(
        r#"<?php
class S {
    private static ?S $inst = null;
    public int $v = 7;
    public static function get(): S {
        self::$inst ??= new S();
        return self::$inst;
    }
}
echo S::get()->v;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies the early-return singleton shape: `!== null` narrows the guarded return, and the
/// assignment after the `if` narrows the fall-through return.
#[test]
fn test_nullable_static_property_narrows_after_strict_not_null_early_return() {
    let out = compile_and_run(
        r#"<?php
class S {
    private static ?S $inst = null;
    public int $v = 7;
    public static function get(): S {
        if (self::$inst !== null) { return self::$inst; }
        self::$inst = new S();
        return self::$inst;
    }
}
echo S::get()->v, S::get()->v;
"#,
    );
    assert_eq!(out, "77");
}

/// Verifies the same narrowing on an INSTANCE property, through both the guarded-assign and the
/// `??=` shapes.
///
/// The two shapes live on separate classes on purpose: `$obj->prop ??= <expr>` currently
/// miscompiles when the property is ALREADY set (an unrelated, pre-existing EIR gap that also
/// reproduces without any narrowing), so each `??=` here runs exactly once on a null property.
#[test]
fn test_nullable_instance_property_lazy_initialization_narrows() {
    let out = compile_and_run(
        r#"<?php
class Node { public int $v = 3; }
class Holder {
    private ?Node $n = null;
    public function get(): Node {
        if ($this->n === null) { $this->n = new Node(); }
        return $this->n;
    }
}
class Lazy {
    private ?Node $n = null;
    public function get(): Node {
        $this->n ??= new Node();
        return $this->n;
    }
}
$h = new Holder();
echo $h->get()->v, $h->get()->v;
$l = new Lazy();
echo $l->get()->v;
"#,
    );
    assert_eq!(out, "333");
}

/// Verifies the branch join also fires when both arms assign: the merged fact is the union of the
/// two branch-exit types, not the declared nullable type.
#[test]
fn test_nullable_static_property_join_across_both_branches() {
    let out = compile_and_run(
        r#"<?php
class S {
    private static ?S $inst = null;
    public int $v = 7;
    public static function get(): S {
        if (self::$inst === null) { self::$inst = new S(); } else { self::$inst = new S(); }
        return self::$inst;
    }
}
echo S::get()->v;
"#,
    );
    assert_eq!(out, "7");
}

/// A guard on a `readonly` property of `$this` survives a call: `$this->plugin->name()` inside
/// `if ($this->plugin instanceof Sold)` cannot write `$this->plugin` again, and `$this` cannot
/// be rebound, so a later `$this->plugin->count()` still reads it as `Sold`. Every call used to
/// drop all property narrowings, and the second read failed with "Undefined method:
/// Plugin::count".
#[test]
fn test_readonly_property_narrowing_survives_a_call() {
    let out = compile_and_run(
        r#"<?php
interface Plugin { public function name(): string; }
final readonly class Sold implements Plugin
{
    public function name(): string { return "sold"; }
    public function count(): int { return 7; }
}
final readonly class Other implements Plugin { public function name(): string { return "other"; } }
final readonly class Report
{
    public function __construct(private Plugin $plugin) {}
    public function line(): string
    {
        if ($this->plugin instanceof Sold) {
            return $this->plugin->name() . " " . $this->plugin->count();
        }
        return $this->plugin->name();
    }
}
echo (new Report(new Sold()))->line(), "|", (new Report(new Other()))->line(), "\n";
"#,
    );
    assert_eq!(out, "sold 7|other\n");
}

/// Verifies an assignment in a truthy branch cannot change the type seen by its else sibling.
#[test]
fn test_if_else_sibling_type_environments_are_isolated() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class Replacement {}

function mapUnlessReplaced(array $values, bool $replace): array {
    if ($replace) {
        $values = new Replacement();
        return [];
    } else {
        return array_map(static fn (mixed $value): mixed => $value, $values);
    }
}

echo count(mapUnlessReplaced([1, 2], false));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a local assigned on only one path stays gradual after the branch join.
#[test]
fn test_conditionally_initialized_local_is_gradual_after_if() {
    let out = compile_and_run(
        r#"<?php
function conditionalCount(bool $enabled): int {
    if ($enabled) {
        $values = [1, 2];
    }

    if ($enabled) {
        return count($values);
    }

    return 0;
}

echo conditionalCount(true), conditionalCount(false);
"#,
    );
    assert_eq!(out, "20");
}

/// Verifies a by-reference output created on a short-circuit arm remains gradual afterwards.
#[test]
fn test_short_circuit_by_ref_output_is_gradual_after_expression() {
    let out = compile_and_run(
        r#"<?php
function fillOutput(?array &$output = null): bool {
    $output = [1, 2, 3];
    return true;
}

function conditionalOutput(bool $enabled): int {
    $filled = $enabled && fillOutput($values);
    if ($filled) {
        return count($values);
    }
    return 0;
}

echo conditionalOutput(true), conditionalOutput(false);
"#,
    );
    assert_eq!(out, "30");
}

/// Verifies an OR operand sees the non-null fact required by its execution path.
#[test]
fn test_nullable_post_decrement_is_narrowed_by_short_circuit_guard() {
    let out = compile_and_run(
        r#"<?php
function retry(?int $attempts): int {
    while (null === $attempts || $attempts--) {
        return null === $attempts ? 0 : 1;
    }
    return -1;
}

echo retry(2);
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies an AND-controlled loop body sees a numeric lower-bound receiver as non-null.
#[test]
fn test_nullable_pre_decrement_is_narrowed_by_numeric_lower_bound() {
    let out = compile_and_run(
        r#"<?php
function bounded(?int $limit): int {
    while (1 < $limit && true) {
        --$limit;
        return 1;
    }
    return 0;
}

echo bounded(null), ':', bounded(2);
"#,
    );
    assert_eq!(out, "0:1");
}

/// Verifies a strict-false guard narrows the local assigned inside the comparison itself.
#[test]
fn test_strict_false_guard_narrows_assignment_target() {
    let out = compile_and_run(
        r#"<?php
function maybeIndex(bool $ok): int|false { return $ok ? 7 : false; }
function requireIndex(bool $ok): int {
    if (false === $index = maybeIndex($ok)) { return -1; }
    return $index;
}

echo requireIndex(true), ':', requireIndex(false);
"#,
    );
    assert_eq!(out, "7:-1");
}

/// Verifies nullsafe method dispatch short-circuits a boxed gradual null receiver.
#[test]
fn test_nullsafe_method_call_on_mixed_receiver() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class GradualReceiver {
    public function value(): string {
        return 'yes';
    }
}

function receiver(bool $present): mixed {
    return $present ? new GradualReceiver() : null;
}

echo receiver(true)?->value() ?? 'no';
echo receiver(false)?->value() ?? 'no';
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "yesno");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a bare `$this` receiver narrowed by `instanceof` can cross an interface-typed call
/// boundary while retaining the same runtime object.
#[test]
fn test_this_instanceof_narrowing_passes_interface_argument() {
    let out = compile_and_run(
        r#"<?php
interface Named { public function name(): string; }
abstract class Base {
    public function describe(): string {
        if ($this instanceof Named) {
            return readName($this);
        }
        return "base";
    }
}
final class Child extends Base implements Named {
    public function name(): string { return "child"; }
}
function readName(Named $value): string { return $value->name(); }
echo (new Child())->describe();
"#,
    );
    assert_eq!(out, "child");
}

/// Verifies trait methods preserve `instanceof` syntax while narrowing their concrete `$this`
/// receiver after trait composition.
#[test]
fn test_trait_this_instanceof_narrowing_passes_interface_argument() {
    let out = compile_and_run(
        r#"<?php
interface Labeled { public function label(): string; }
trait DescribesLabel {
    public function describe(): string {
        if ($this instanceof Labeled) {
            return readLabel($this);
        }
        return "none";
    }
}
final class LabeledValue implements Labeled {
    use DescribesLabel;
    public function label(): string { return "value"; }
}
function readLabel(Labeled $value): string { return $value->label(); }
echo (new LabeledValue())->describe();
"#,
    );
    assert_eq!(out, "value");
}

/// Verifies a trait ternary can read a property declared only by the concrete class proven by
/// its positive `$this instanceof` branch.
#[test]
fn test_trait_this_instanceof_ternary_narrows_property_receiver() {
    let out = compile_and_run(
        r#"<?php
trait ReadsOptionalParent {
    public function parentName(): ?string {
        return $this instanceof ParentAware ? $this->parentName : null;
    }
}

class PlainValue {
    use ReadsOptionalParent;
}

class ParentAware {
    use ReadsOptionalParent;
    public ?string $parentName = "root";
}

echo (new PlainValue())->parentName() ?? "none";
echo ":";
echo (new ParentAware())->parentName();
"#,
    );
    assert_eq!(out, "none:root");
}

/// Verifies `is_countable()` narrows an iterable inside a ternary without claiming every
/// Traversable is countable. Arrays take the guarded `count()` edge; a plain iterator takes null.
#[test]
fn test_is_countable_narrows_iterable_inside_ternary() {
    let out = compile_and_run(
        r#"<?php
final class Sequence implements IteratorAggregate {
    public function getIterator(): Traversable { yield 1; }
}
final class CountedSequence implements IteratorAggregate, Countable {
    public function getIterator(): Traversable { yield 1; }
    public function count(): int { return 9; }
}
function guarded_count(iterable $values): ?int {
    return is_countable($values) ? count($values) : null;
}
echo guarded_count([1, 2, 3]), "|";
var_dump(guarded_count(new Sequence()));
echo guarded_count(new CountedSequence());
"#,
    );
    assert_eq!(out, "3|NULL\n9");
}

/// Verifies an unguarded iterable remains runtime-checked: a non-Countable Traversable raises a
/// catchable TypeError instead of being rejected statically or silently counted by iteration.
#[test]
fn test_count_non_countable_iterable_raises_type_error() {
    let out = compile_and_run(
        r#"<?php
final class StreamValues implements IteratorAggregate {
    public function getIterator(): Traversable { yield 1; }
}
function unguarded_count(iterable $values): int { return count($values); }
try {
    echo unguarded_count(new StreamValues());
} catch (TypeError $error) {
    echo "type-error";
}
"#,
    );
    assert_eq!(out, "type-error");
}

/// Verifies `is_numeric()` admits a checked numeric string to subsequent arithmetic.
#[test]
fn test_is_numeric_guard_allows_string_arithmetic() {
    let out = compile_and_run(
        r#"<?php
function scaled_number(string $value): mixed {
    if (!is_numeric($value)) {
        throw new InvalidArgumentException('not numeric');
    }
    $value *= 1024;
    return $value;
}
echo scaled_number('2.5');
"#,
    );
    assert_eq!(out, "2560");
}

/// Verifies a negated `instanceof` return narrows the fallthrough closure invocation path.
#[test]
fn test_negated_instanceof_early_return_narrows_union() {
    let out = compile_and_run(
        r#"<?php
class NonceNarrowingProbe {
    private function resolveNonce(string|Closure|null $nonce): ?string {
        if (!$nonce instanceof Closure) {
            return $nonce;
        }
        if (!is_string(($value = $nonce()) ?? '')) {
            throw new LogicException('invalid');
        }
        return $value;
    }

    public function dump(): void {
        echo $this->resolveNonce('x'), ':', $this->resolveNonce(fn () => 'y'), ':';
        var_dump($this->resolveNonce(null));
    }
}
(new NonceNarrowingProbe())->dump();
"#,
    );
    assert_eq!(out, "x:y:NULL\n");
}
