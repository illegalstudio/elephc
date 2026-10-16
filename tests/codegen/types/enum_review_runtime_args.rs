//! Purpose:
//! Regressions for reviewed native enum runtime argument binding and diagnostics.
//!
//! Called from:
//! - The codegen integration harness through `codegen::types`.
//!
//! Key details:
//! - Supplied values are checked before later omissions, including static unpacks.
//! - Collector storage may widen without changing declared hints or argument positions.
//! - Independent IR-off tests preserve the same PHP-visible results.

use crate::support::*;

/// Static unpack elements are evaluated once and type-checked before a later missing parameter.
#[test]
fn test_enum_seventh_review_static_spread_shortage() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $a, int $b): int; }
enum G implements I { case A; public function f(int $a = 1, int $b): int { return $a + $b; } }
function bad(): string { echo 'Y'; return 'no'; }
function good(): int { echo 'Z'; return 3; }
function rejected(I $value): int { return $value->f(...[bad()]); }
function literal(I $value): int { return $value->f(...['no']); }
function missing(I $value): int { return $value->f(...[good()]); }
try { rejected(G::A); } catch (TypeError $e) { echo $e->getMessage(); }
echo '|';
try { literal(G::A); } catch (TypeError $e) { echo $e->getMessage(); }
echo '|';
try { missing(G::A); } catch (ArgumentCountError $e) { echo $e->getMessage(); }
"#);
    assert_eq!(out, "YG::f(): Argument #1 ($a) must be of type int, string given|G::f(): Argument #1 ($a) must be of type int, string given|ZToo few arguments to function G::f(), 1 passed and exactly 2 expected");
}

/// Literal false rejects coercible scalar lookalikes and reports true distinctly from bool.
#[test]
fn test_enum_seventh_review_false_argument() {
    let out = compile_and_run(r#"<?php
interface I { public function f(false $value): string; }
enum F implements I { case A; public function f(false $value): string { return $value ? 'T' : 'F'; } }
function invoke(I $value, array $args): string { return $value->f(...$args); }
function probe(I $value, array $args): void {
    try { echo invoke(F::A, $args); } catch (TypeError $e) { echo $e->getMessage(); }
    echo '|';
}
probe(F::A, [0]); probe(F::A, [true]); probe(F::A, ['0']); probe(F::A, [null]);
echo invoke(F::A, [false]);
"#);
    assert_eq!(out, "F::f(): Argument #1 ($value) must be of type false, int given|F::f(): Argument #1 ($value) must be of type false, true given|F::f(): Argument #1 ($value) must be of type false, string given|F::f(): Argument #1 ($value) must be of type false, null given|F");
}

/// Iterable accepts arrays and Traversable but rejects scalar values before conversion.
#[test]
fn test_enum_seventh_review_iterable_argument() {
    let out = compile_and_run(r#"<?php
interface I { public function f(iterable $value): int; }
enum E implements I { case A; public function f(iterable $value): int {
    $sum = 0; foreach ($value as $item) { $sum += $item; } return $sum;
} }
function invoke(I $value, array $args): int { return $value->f(...$args); }
try { invoke(E::A, [1]); } catch (TypeError $e) { echo $e->getMessage(); }
echo '|', invoke(E::A, [[1, 2]]), '|', invoke(E::A, [new ArrayIterator([3, 4])]);
"#);
    assert_eq!(out, "E::f(): Argument #1 ($value) must be of type Traversable|array, int given|3|7");
}

/// Equally variadic interface and enum signatures keep count-aware ABI transport after promotion.
#[test]
fn test_enum_seventh_review_equal_variadic_interface() {
    let out = compile_and_run(r#"<?php
interface U { public function f(int $x, int ...$rest): int; }
enum X implements U { case A; public function f(int $x, int ...$rest): int {
    foreach ($rest as $value) { $x += $value; } return $x;
} }
function invoke(U $value, array $args): int { return $value->f(...$args); }
echo X::A->f(1, 2, 3), '|', invoke(X::A, [1, 2, 3]), '|';
echo invoke(X::A, [1, 2, 'extra' => 3]);
"#);
    assert_eq!(out, "6|6|6");
}

/// A promoted equally variadic implementation preserves hidden actual counts as well.
#[test]
fn test_enum_seventh_review_equal_variadic_counts() {
    let out = compile_and_run(r#"<?php
interface U { public function f(int $x, int ...$rest): int; }
enum X implements U { case A; public function f(int $x, int ...$rest): int {
    echo func_num_args(), ':';
    foreach ($rest as $value) { $x += $value; } return $x;
} }
function invoke(U $value, array $args): int { return $value->f(...$args); }
echo X::A->f(1, 2, 3), '|', invoke(X::A, [1, 2, 3]), '|';
echo invoke(X::A, [1, 2, 'extra' => 3]);
"#);
    assert_eq!(out, "3:6|3:6|2:6");
}

/// Optional fixed slots keep a real hidden count when runtime named tails omit those slots.
#[test]
fn test_enum_seventh_review_equal_variadic_optional_counts() {
    let out = compile_and_run(r#"<?php
interface U { public function f(int $x = 1, int ...$rest): int; }
enum X implements U { case A; public function f(int $x = 1, int ...$rest): int {
    echo func_num_args(), ':';
    foreach ($rest as $value) { $x += $value; } return $x;
} }
function invoke(U $value, array $args): int { return $value->f(...$args); }
echo invoke(X::A, []), '|', invoke(X::A, [1, 2]), '|';
echo invoke(X::A, ['extra' => 2]), '|', invoke(X::A, [1, 2, 3]);
"#);
    assert_eq!(out, "0:1|2:3|0:3|3:6");
}

/// Variadic diagnostics use actual positional indexes and do not name the collector parameter.
#[test]
fn test_enum_seventh_review_variadic_diagnostic_position() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x): int; }
enum E implements I { case A; public function f(int $x, int ...$rest): int { return $x; } }
function invoke(I $value, array $args): int { return $value->f(...$args); }
function probe(I $value, array $args): void {
    try { invoke($value, $args); } catch (TypeError $e) { echo $e->getMessage(), '|'; }
}
probe(E::A, [1, 'no']); probe(E::A, [1, 2, 'no']);
probe(E::A, [1, 'extra' => 'no']); probe(E::A, [1, 2, 'extra' => 'no']);
probe(E::A, ['x' => 1, 'foo' => 2, 'bar' => 'no']);
"#);
    assert_eq!(out, "E::f(): Argument #2 must be of type int, string given|E::f(): Argument #3 must be of type int, string given|E::f(): Argument #2 must be of type int, string given|E::f(): Argument #3 must be of type int, string given|E::f(): Argument #2 must be of type int, string given|");
}

/// Omitted optional prefixes still occupy their PHP positions before a named variadic tail.
#[test]
fn test_enum_seventh_review_optional_variadic_diagnostic_position() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x = 1): int; }
enum E implements I { case A; public function f(int $x = 1, int $y = 2, int ...$rest): int { return $x; } }
function invoke(I $value, array $args): int { return $value->f(...$args); }
function probe(I $value, array $args): void {
    try { invoke($value, $args); } catch (TypeError $e) { echo $e->getMessage(), '|'; }
}
probe(E::A, ['foo' => 'no']); probe(E::A, ['x' => 1, 'foo' => 'no']);
probe(E::A, ['y' => 2, 'foo' => 'no']); probe(E::A, [1, 2, 3, 'foo' => 'no']);
"#);
    assert_eq!(out, "E::f(): Argument #3 must be of type int, string given|E::f(): Argument #3 must be of type int, string given|E::f(): Argument #3 must be of type int, string given|E::f(): Argument #4 must be of type int, string given|");
}

/// Callable is another PHP-visible hint that cannot silently skip runtime unpack guards.
#[test]
fn test_enum_seventh_review_callable_argument() {
    let out = compile_and_run(r#"<?php
interface I { public function f(callable $value): string; }
enum E implements I { case A; public function f(callable $value): string { return 'ok'; } }
function invoke(I $value, array $args): string { return $value->f(...$args); }
try { invoke(E::A, [1]); } catch (TypeError $e) { echo $e->getMessage(); }
echo '|', invoke(E::A, [static fn () => 1]), '|', invoke(E::A, ['strlen']);
"#);
    assert_eq!(out, "E::f(): Argument #1 ($value) must be of type callable, int given|ok|ok");
}

/// Repeated guarded calls retain only the bounded singleton storage of the direct controls.
#[test]
fn test_enum_seventh_review_argument_owners() {
    let mut baseline: Option<([usize; 2], [usize; 2])> = None;
    for iterations in [1, 16] {
        let run = |body: &str| compile_and_run_with_heap_debug(&format!(r#"<?php
interface I {{ public function f(int $a, int $b): int; }}
enum G implements I {{ case A; public function f(int $a = 1, int $b): int {{ return $a + $b; }} }}
class Payload {{ public function __destruct() {{ echo 'D'; }} }}
function reject(I $value): int {{ return $value->f(...[new Payload()]); }}
interface J {{ public function f(iterable $value): int; }}
enum E implements J {{ case A; public function f(iterable $value): int {{
    $sum = 0; foreach ($value as $item) {{ $sum += $item; }} return $sum;
}} }}
function iterable(J $value, array $args): int {{ return $value->f(...$args); }}
interface U {{ public function f(int $x, int ...$rest): int; }}
enum X implements U {{ case A; public function f(int $x, int ...$rest): int {{
    foreach ($rest as $item) {{ $x += $item; }} return $x;
}} }}
function variadic(U $value, array $args): int {{ return $value->f(...$args); }}
for ($i = 0; $i < {iterations}; $i++) {{ {body} }}
"#));
        let direct = run("echo G::A->f(1, 2), '|'; $p = new Payload(); unset($p); echo 'T|'; echo E::A->f(new ArrayIterator([3, 4])), '|', X::A->f(1, 2, extra: 3), '|';");
        let result = run("echo G::A->f(1, 2), '|'; try { reject(G::A); } catch (TypeError $e) { echo 'T'; unset($e); } echo '|', iterable(E::A, [new ArrayIterator([3, 4])]), '|', variadic(X::A, [1, 2, 'extra' => 3]), '|';");
        assert!(direct.success && result.success, "{}\n{}", direct.stderr, result.stderr);
        assert_eq!(direct.stdout, "3|DT|7|6|".repeat(iterations));
        assert_eq!(result.stdout, direct.stdout, "{}", result.stderr);
        let retained = |stderr: &str| ["live_blocks=", "live_bytes="].map(|metric| {
            stderr.lines().next().unwrap().split_whitespace()
                .find_map(|field| field.strip_prefix(metric)).unwrap().parse::<usize>().unwrap()
        });
        let direct_retained = retained(&direct.stderr);
        let unpack_retained = retained(&result.stderr);
        assert_eq!(unpack_retained[0], direct_retained[0],
            "retained singleton count: direct {}\nunpack {}", direct.stderr, result.stderr);
        if let Some((direct_baseline, unpack_baseline)) = baseline {
            // The allocator can reuse differently sized singleton blocks across binaries.
            // Each path must still retain no additional blocks or bytes after repeated calls.
            assert_eq!(direct_retained, direct_baseline, "direct: {}", direct.stderr);
            assert_eq!(unpack_retained, unpack_baseline, "unpack: {}", result.stderr);
        } else {
            baseline = Some((direct_retained, unpack_retained));
        }
    }
}

/// Each IR-off regression is independently scheduled under the unchanged CI timeout.
macro_rules! without_opt_test {
    ($name:ident, $case:ident) => {
        /// Verifies this enum argument regression without EIR optimization.
        #[test]
        fn $name() { without_ir_opt($case); }
    };
}

without_opt_test!(test_enum_seventh_review_static_spread_shortage_no_ir_opt, test_enum_seventh_review_static_spread_shortage);
without_opt_test!(test_enum_seventh_review_false_argument_no_ir_opt, test_enum_seventh_review_false_argument);
without_opt_test!(test_enum_seventh_review_iterable_argument_no_ir_opt, test_enum_seventh_review_iterable_argument);
without_opt_test!(test_enum_seventh_review_equal_variadic_interface_no_ir_opt, test_enum_seventh_review_equal_variadic_interface);
without_opt_test!(test_enum_seventh_review_equal_variadic_counts_no_ir_opt, test_enum_seventh_review_equal_variadic_counts);
without_opt_test!(test_enum_seventh_review_equal_variadic_optional_counts_no_ir_opt, test_enum_seventh_review_equal_variadic_optional_counts);
without_opt_test!(test_enum_seventh_review_variadic_diagnostic_position_no_ir_opt, test_enum_seventh_review_variadic_diagnostic_position);
without_opt_test!(test_enum_seventh_review_optional_variadic_diagnostic_position_no_ir_opt, test_enum_seventh_review_optional_variadic_diagnostic_position);
without_opt_test!(test_enum_seventh_review_callable_argument_no_ir_opt, test_enum_seventh_review_callable_argument);
without_opt_test!(test_enum_seventh_review_argument_owners_no_ir_opt, test_enum_seventh_review_argument_owners);
