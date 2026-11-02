//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types enums, including backed enum value and from identity, enum try from and cases, and backed enum from and value.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

/// Implicit builtin factories keep their backing ABI when an explicit interface inherits BackedEnum.
#[test]
fn test_enum_main_builtin_backed_factory_contracts() {
    let out = compile_and_run(r#"<?php
interface Factory extends BackedEnum {}
enum TextValue: string implements Factory { case Word = 'word'; }
enum NumberValue: int implements Factory { case Seven = 7; }
echo TextValue::from('word')->value, ':', NumberValue::from(7)->value, ':',
    (int)(TextValue::tryFrom('missing') === null);
"#);
    assert_eq!(out, "word:7:1");
}

/// Builtin backed-factory contract validation is independent of EIR optimization.
#[test]
fn test_enum_main_builtin_backed_factory_contracts_no_ir_opt() {
    crate::support::without_ir_opt(test_enum_main_builtin_backed_factory_contracts);
}

/// Named partial calls check earlier supplied types but stop at the first missing slot.
#[test]
fn test_enum_sixth_review_named_shortage_precedence() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $a, int $b): int; }
enum G implements I { case A; public function f(int $a = 1, int $b): int { return $a + $b; } }
enum H implements I { case A; public function f(mixed $a = 1, mixed $b = 2): int { return 1; } }
function bad(): string { echo 'Y'; return 'no'; }
function first(I $value): int { return $value->f(a: bad()); }
function second(I $value): int { return $value->f(b: bad()); }
try { first(G::A); } catch (TypeError $e) { echo $e->getMessage(); }
echo '|';
try { second(G::A); } catch (ArgumentCountError $e) { echo $e->getMessage(); }
"#);
    assert_eq!(out, "YG::f(): Argument #1 ($a) must be of type int, string given|YG::f(): Argument #1 ($a) not passed");
}

/// Enum variadic storage stays boxed when an ordinary class shares the interface.
#[test]
fn test_enum_sixth_review_variadic_with_class_implementation() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x): int; }
class C implements I { public function f(int $x): int { return $x; } }
enum V implements I {
    case A;
    public function f(int $x, int ...$rest): int {
        foreach ($rest as $key => $value) { echo $key, '=', $value, ';'; $x += $value; }
        return $x;
    }
}
function invoke(I $value, array $args): int { return $value->f(...$args); }
echo invoke(V::A, ['7', '8']), '|';
echo invoke(V::A, ['x' => 2, 'extra' => '4']);
"#);
    assert_eq!(out, "0=8;15|extra=4;6");
}

/// Diagnostic and weak-binding behavior does not depend on EIR optimization.
#[test]
fn test_enum_sixth_review_unpack_diagnostics_ir_off() {
    assert_eq!(run_enum_sixth_review_ir_off(r#"<?php
interface I { public function f(int $x, int $y = 2): int; }
enum F implements I { case A; public function f(int $x, int $y = 2): int { return $x + $y; } }
function invoke(I $value, array $args): int { return $value->f(...$args); }
function probe(I $value, array $args): void {
    try { echo invoke($value, $args); } catch (Throwable $e) { echo $e->getMessage(); }
    echo '|';
}
probe(F::A, []); probe(F::A, ['y' => 4]); probe(F::A, ['no']); probe(F::A, [null]);
echo invoke(F::A, ['7']), '|', invoke(F::A, [true]), '|', invoke(F::A, [2.0]);
"#),
        "Too few arguments to function F::f(), 0 passed and at least 1 expected|F::f(): Argument #1 ($x) not passed|F::f(): Argument #1 ($x) must be of type int, string given|F::f(): Argument #1 ($x) must be of type int, null given|9|3|4");
}

/// A returned storage alias remains attached after runtime argument binding retires.
#[test]
fn test_enum_sixth_review_reference_storage_alias() {
    let out = compile_and_run(r#"<?php
class Store { public array $values = [0]; }
interface I { public function &f(Store $store, int $x): array; }
enum E implements I {
    case A;
    public function &f(Store $store, int $x = 1): array { $store->values = [$x]; return $store->values; }
}
function invoke(I $value, array $args): int {
    $result = &$value->f(...$args); $result[0] += 10; return $result[0];
}
$store = new Store();
echo invoke(E::A, [$store, 2]), ':', $store->values[0], '|';
echo invoke(E::A, [$store]), ':', $store->values[0];
"#);
    assert_eq!(out, "12:12|11:11");
}

/// Runtime scalar binding covers numeric strings, bools and Stringable objects.
#[test]
fn test_enum_sixth_review_weak_scalar_unpack() {
    let out = compile_and_run(r#"<?php
interface I { public function f(string $x, float $y, bool $z): string; }
enum E implements I {
    case A;
    public function f(string $x, float $y, bool $z): string {
        return $x . ':' . $y . ':' . ($z ? 'T' : 'F');
    }
}
class Text { public function __toString(): string { return 'ok'; } }
function invoke(I $value, array $args): string { return $value->f(...$args); }
echo invoke(E::A, ['z' => 0, 'x' => 8, 'y' => '2.5']), '|';
echo invoke(E::A, [new Text(), '1.5', true]), '|';
try { invoke(E::A, [null, 2.0, false]); } catch (TypeError $e) { echo $e->getMessage(); }
"#);
    assert_eq!(out, "8:2.5:F|ok:1.5:T|E::f(): Argument #1 ($x) must be of type string, null given");
}

/// Runs one regression through the CLI with EIR optimization explicitly disabled.
fn run_enum_sixth_review_ir_off(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_enum_unpack_ir_off");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write enum unpack fixture");
    let result = std::panic::catch_unwind(|| {
        let compiled = elephc_cli_command(&dir).arg("--ir-opt=off").arg(&path).output()
            .expect("compile enum unpack fixture");
        assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
        let output = Command::new(dir.join("main")).output().expect("run enum unpack fixture");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).expect("enum fixture UTF-8")
    });
    let _ = fs::remove_dir_all(&dir);
    result.unwrap_or_else(|failure| std::panic::resume_unwind(failure))
}

/// Returned reference cells survive unpack materialization with optimization disabled.
#[test]
fn test_enum_sixth_review_reference_unpack_ir_off() {
    assert_eq!(run_enum_sixth_review_ir_off(r#"<?php
interface I { public function &f(int $x): array; }
enum H implements I { case A; public function &f(int $x): array { $a = [$x]; return $a; } }
enum E implements I { case A; public function &f(int $x = 1): array { $a = [$x]; return $a; } }
function invoke(I $value, array $args): int { $r = &$value->f(...$args); $r[0] += 10; return $r[0]; }
try { echo invoke(H::A, []); } catch (ArgumentCountError $e) { echo $e->getMessage(); }
echo '|', invoke(H::A, [2]), '|', invoke(H::A, ['x' => 3]), '|', invoke(E::A, []);
"#),
        "Too few arguments to function H::f(), 0 passed and exactly 1 expected|12|13|11");
}

/// Supplied argument validation precedes omissions with optimization disabled too.
#[test]
fn test_enum_sixth_review_shortage_type_precedence_ir_off() {
    assert_eq!(run_enum_sixth_review_ir_off(r#"<?php
interface I { public function f(int $a, int $b): int; }
enum G implements I { case A; public function f(int $a = 1, int $b): int { return $a + $b; } }
function bad(): string { echo 'Y'; return 'no'; }
function good(): int { echo 'Z'; return 3; }
function rejected(I $value): int { return $value->f(bad()); }
function missing(I $value): int { return $value->f(good()); }
try { rejected(G::A); } catch (TypeError $e) { echo $e->getMessage(); }
echo '|';
try { missing(G::A); } catch (ArgumentCountError $e) { echo $e->getMessage(); }
"#),
        "YG::f(): Argument #1 ($a) must be of type int, string given|ZToo few arguments to function G::f(), 1 passed and exactly 2 expected");
}

/// Variadic values and introspection collectors retain actual counts and named keys.
#[test]
fn test_enum_sixth_review_variadic_unpack() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x): int; }
enum V implements I {
    case A;
    public function f(int $x = 3, int ...$rest): int {
        echo func_num_args(), ':';
        foreach ($rest as $key => $value) { echo $key, '=', $value, ';'; $x += $value; }
        return $x;
    }
}
function invoke(I $value, array $args): int { return $value->f(...$args); }
echo invoke(V::A, []), '|', invoke(V::A, ['7', '8', 9]), '|';
echo invoke(V::A, ['x' => 2, 'extra' => '4']);
"#);
    assert_eq!(out, "0:3|3:0=8;1=9;24|1:extra=4;6");
}

/// Name collisions are rejected before earlier supplied parameter type errors.
#[test]
fn test_enum_sixth_review_binding_error_precedence() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x, int $y): int; }
enum F implements I { case A; public function f(int $x, int $y): int { return $x + $y; } }
function invoke(I $value, array $args): int { return $value->f(...$args); }
try { invoke(F::A, ['no', 2, 'y' => 3]); } catch (Error $e) { echo $e->getMessage(); }
echo '|';
try { invoke(F::A, ['no', 'bad' => 3]); } catch (Error $e) { echo $e->getMessage(); }
"#);
    assert_eq!(out, "Named parameter $y overwrites previous argument|Unknown named parameter $bad");
}

/// Repeated rejected unpacks retain no more than the direct enum singleton baseline.
#[test]
fn test_enum_sixth_review_unpack_owners() {
    let retained = |stderr: &str, metric: &str| stderr.lines().next().unwrap().split_whitespace()
        .find_map(|field| field.strip_prefix(metric)).unwrap().parse::<usize>().unwrap();
    for iterations in [1, 40] {
        let run = |body: &str| compile_and_run_with_heap_debug(&format!(r#"<?php
interface I {{ public function f(int $x): int; }}
enum F implements I {{ case A; public function f(int $x): int {{ return $x; }} }}
class Payload {{ public function __destruct() {{ echo 'D'; }} }}
function args(array $values): array {{ echo 'A'; return $values; }}
function invoke(I $value, array $values): int {{ return $value->f(...args($values)); }}
for ($i = 0; $i < {iterations}; $i++) {{ {body} }}
"#));
        let direct = run("echo F::A->f(4), '|';");
        let result = run("try { invoke(F::A, [new Payload()]); } catch (TypeError $e) { echo 'T'; unset($e); } echo invoke(F::A, [4, new Payload()]), '|';");
        assert!(direct.success && result.success, "{}\n{}", direct.stderr, result.stderr);
        assert_eq!(result.stdout, "ADTAD4|".repeat(iterations));
        for metric in ["live_blocks=", "live_bytes="] {
            assert_eq!(retained(&direct.stderr, metric), retained(&result.stderr, metric),
                "{metric}: direct {}\nunpack {}", direct.stderr, result.stderr);
        }
    }
}

/// Runtime unpacking preserves the selected method's returned reference cell.
#[test]
fn test_enum_sixth_review_reference_unpack() {
    let out = compile_and_run(r#"<?php
interface I { public function &f(int $x): array; }
enum H implements I { case A; public function &f(int $x): array { $a = [$x]; return $a; } }
enum E implements I { case A; public function &f(int $x = 1): array { $a = [$x]; return $a; } }
function invoke(I $value, array $args): int { $r = &$value->f(...$args); $r[0] += 10; return $r[0]; }
try { echo invoke(H::A, []); } catch (ArgumentCountError $e) { echo $e->getMessage(); }
echo '|', invoke(H::A, [2]), '|', invoke(H::A, ['x' => 3]), '|', invoke(E::A, []);
"#);
    assert_eq!(out, "Too few arguments to function H::f(), 0 passed and exactly 1 expected|12|13|11");
}

/// Selected methods retain PHP argument diagnostics and weak scalar parameter binding.
#[test]
fn test_enum_sixth_review_unpack_diagnostics() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x, int $y = 2): int; }
enum F implements I { case A; public function f(int $x, int $y = 2): int { return $x + $y; } }
function invoke(I $value, array $args): int { return $value->f(...$args); }
function probe(I $value, array $args): void {
    try { echo invoke($value, $args); } catch (Throwable $e) { echo $e->getMessage(); }
    echo '|';
}
probe(F::A, []); probe(F::A, ['y' => 4]); probe(F::A, ['no']); probe(F::A, [null]);
echo invoke(F::A, ['7']), '|', invoke(F::A, [true]), '|', invoke(F::A, [2.0]);
"#);
    assert_eq!(out, "Too few arguments to function F::f(), 0 passed and at least 1 expected|F::f(): Argument #1 ($x) not passed|F::f(): Argument #1 ($x) must be of type int, string given|F::f(): Argument #1 ($x) must be of type int, null given|9|3|4");
}

/// Provided argument types are checked before the selected method's positional shortage.
#[test]
fn test_enum_sixth_review_shortage_type_precedence() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $a, int $b): int; }
enum G implements I { case A; public function f(int $a = 1, int $b): int { return $a + $b; } }
function bad(): string { echo 'Y'; return 'no'; }
function good(): int { echo 'Z'; return 3; }
function rejected(I $value): int { return $value->f(bad()); }
function missing(I $value): int { return $value->f(good()); }
try { rejected(G::A); } catch (TypeError $e) { echo $e->getMessage(); }
echo '|';
try { missing(G::A); } catch (ArgumentCountError $e) { echo $e->getMessage(); }
"#);
    assert_eq!(out, "YG::f(): Argument #1 ($a) must be of type int, string given|ZToo few arguments to function G::f(), 1 passed and exactly 2 expected");
}


/// Dynamic PHP arrays unpack elements instead of passing their count to the selected method.
#[test]
fn test_enum_fifth_review_dynamic_unpack() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x): int; }
enum E implements I { case A; public function f(int $x = 10): int { return $x; } }
enum F implements I { case A; public function f(int $x): int { return $x; } }
function invoke(I $value, array $args): int { return $value->f(...$args); }
try { echo invoke(F::A, []); } catch (ArgumentCountError $error) { echo 'missing'; }
echo '|', invoke(F::A, [4]), '|', invoke(F::A, [4, 5]), '|', invoke(E::A, []);
echo '|', invoke(F::A, ['x' => 6]), '|', invoke(F::A, [7 => 8]);
"#);
    assert_eq!(out, "missing|4|4|10|6|8");
}

/// A required parameter after a default makes every preceding positional slot required.
#[test]
fn test_enum_fifth_review_required_after_default() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $a, int $b): int; }
enum E implements I { case A; public function f(int $a = 1, int $b): int { return $a + $b; } }
function argument(): int { echo 'Y'; return 4; }
function invoke(I $value): int { return $value->f(argument()); }
try { echo invoke(E::A); } catch (ArgumentCountError $error) { echo $error->getMessage(); }
echo '|', E::A->f(4, 5);
"#);
    assert_eq!(out, "YToo few arguments to function E::f(), 1 passed and exactly 2 expected|9");
}

/// A hidden backtrace collector does not turn a fixed method into a source variadic.
#[test]
fn test_enum_fifth_review_hidden_collector_bound() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x): int; }
enum E implements I { case A; public function f(int $x = 10): int { return $x; } }
enum F implements I { case A; public function f(int $x): int { return $x; } }
enum V implements I { case A; public function f(int $x, int ...$rest): int { return $x; } }
enum W implements I { case A; public function f(int $x, int $y = 2, int ...$rest): int { return $x; } }
function invoke(I $value): int { return $value->f(); }
debug_print_backtrace();
try { echo invoke(F::A); } catch (ArgumentCountError $error) { echo $error->getMessage(); }
echo '|';
try { echo invoke(V::A); } catch (ArgumentCountError $error) { echo $error->getMessage(); }
echo '|';
try { echo invoke(W::A); } catch (ArgumentCountError $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "Too few arguments to function F::f(), 0 passed and exactly 1 expected|Too few arguments to function V::f(), 0 passed and exactly 1 expected|Too few arguments to function W::f(), 0 passed and at least 1 expected");
}

/// Runtime unpack binding releases source, callback and rejected argument owners exactly once.
#[test]
fn test_enum_fifth_review_unpack_ownership() {
    for iterations in [1, 30] {
        let run = |body: &str| compile_and_run_with_heap_debug(&format!(r#"<?php
interface I {{ public function f(int $x): int; }}
enum E implements I {{ case A; public function f(int $x = 10): int {{ return $x; }} }}
enum F implements I {{ case A; public function f(int $x): int {{ return $x; }} }}
class Payload {{ public function __destruct() {{ echo 'D'; }} }}
function arguments(array $values): array {{ echo 'A'; return $values; }}
function invoke(I $value, array $args): int {{ return $value->f(...arguments($args)); }}
for ($i = 0; $i < {iterations}; $i++) {{ {body} }}
"#));
        let direct = run("echo E::A->f(), '|', F::A->f(4), '|';");
        let unpacked = run("echo invoke(E::A, []), '|'; $bad = ['bad' => new Payload()]; try { invoke(F::A, $bad); } catch (Error $error) { echo 'bad|'; unset($error); } unset($bad); try { invoke(F::A, []); } catch (ArgumentCountError $error) { echo 'missing|'; unset($error); } echo invoke(F::A, [4, new Payload()]), '|';");
        assert!(direct.success && unpacked.success, "{}\n{}", direct.stderr, unpacked.stderr);
        assert_eq!(unpacked.stdout, "A10|Abad|DAmissing|AD4|".repeat(iterations));
        for metric in ["live_blocks=", "live_bytes="] {
            let retained = |stderr: &str| stderr.lines().next().unwrap().split_whitespace()
                .find(|field| field.starts_with(metric)).unwrap()[metric.len()..].parse::<usize>().unwrap();
            assert_eq!(retained(&unpacked.stderr), retained(&direct.stderr), "{metric}: {}\n{}", unpacked.stderr, direct.stderr);
        }
    }
}

/// A missing positional argument throws only for the selected required enum implementation.
#[test]
fn test_enum_fourth_review_missing_positionals() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x): int; }
enum E implements I { case A; public function f(int $x = 10): int { return $x; } }
enum F implements I { case A; public function f(int $x): int { return $x; } }
function invoke(I $value): int { return $value->f(); }
function unpack(I $value): int { return $value->f(...[]); }
echo 'E', invoke(E::A), '|';
try { echo invoke(F::A); } catch (ArgumentCountError $error) { echo $error->getMessage(); }
echo '|E', unpack(E::A), '|';
try { echo unpack(F::A); } catch (ArgumentCountError $error) { echo $error->getMessage(); }
echo '|', F::A->f(8);
"#);
    assert_eq!(out, "E10|Too few arguments to function F::f(), 0 passed and exactly 1 expected|E10|Too few arguments to function F::f(), 0 passed and exactly 1 expected|8");
}

/// Named omissions retain the implementation/parameter diagnostic and release rejected owners.
#[test]
fn test_enum_fourth_review_missing_named_ownership() {
    for iterations in [1, 30] {
        let run = |body: &str| compile_and_run_with_heap_debug(&format!(r#"<?php
interface I {{ public function f(int $x): int; }}
class Payload {{ public function __destruct() {{ echo 'D'; }} }}
enum E implements I {{ case A; public function f(int $x = 1, ?Payload $y = null): int {{ return $x; }} }}
enum F implements I {{ case A; public function f(int $x, ?Payload $y = null): int {{ return $x; }} }}
function argument(): Payload {{ echo 'Y'; return new Payload(); }}
function invoke(I $value): int {{ return $value->f(y: argument()); }}
for ($i = 0; $i < {iterations}; $i++) {{ {body} }}
"#));
        let direct = run("echo E::A->f(y: argument()), '|', F::A->f(4, argument()), '|';");
        let rejected = run("echo invoke(E::A), '|'; try { echo invoke(F::A); } catch (ArgumentCountError $error) { echo $error->getMessage(); unset($error); } echo '|';");
        assert!(direct.success && rejected.success, "{}\n{}", direct.stderr, rejected.stderr);
        assert_eq!(direct.stdout, "YD1|YD4|".repeat(iterations));
        assert_eq!(rejected.stdout, "YD1|YDF::f(): Argument #1 ($x) not passed|".repeat(iterations));
        for metric in ["live_blocks=", "live_bytes="] {
            let retained = |stderr: &str| stderr.lines().next().unwrap().split_whitespace()
                .find(|field| field.starts_with(metric)).unwrap()[metric.len()..].parse::<usize>().unwrap();
            assert_eq!(retained(&rejected.stderr), retained(&direct.stderr), "{metric}: {}\n{}", rejected.stderr, direct.stderr);
        }
    }
}

/// Surplus positionals still release owned temporaries after a shorter implementation returns.
#[test]
fn test_enum_third_review_surplus_argument_ownership() {
    for iterations in [1, 40] {
        let run = |body: &str| compile_and_run_with_heap_debug(&format!(r#"<?php
interface I {{ public function f(int $x = 1): int; }}
class Payload {{ public function __destruct() {{ echo 'D'; }} }}
enum E implements I {{ case A; public function f(int $x = 10, ?Payload $y = null): int {{ return $x; }} }}
enum F implements I {{ case A; public function f(int $x = 20): int {{ return $x; }} }}
function invoke(I $value): int {{ return $value->f(4, new Payload()); }}
for ($i = 0; $i < {iterations}; $i++) {{ {body} }}
"#));
        let direct = run("echo E::A->f(4), F::A->f(4);");
        let adapted = run("echo invoke(E::A), invoke(F::A);");
        assert!(direct.success && adapted.success, "{}\n{}", direct.stderr, adapted.stderr);
        assert_eq!(direct.stdout, "44".repeat(iterations));
        assert_eq!(adapted.stdout, "D4D4".repeat(iterations), "{}", adapted.stderr);
        for metric in ["live_blocks=", "live_bytes="] {
            let retained = |stderr: &str| stderr.lines().next().unwrap().split_whitespace()
                .find(|field| field.starts_with(metric)).unwrap()[metric.len()..].parse::<usize>().unwrap();
            assert_eq!(retained(&adapted.stderr), retained(&direct.stderr), "{metric}: {}\n{}", adapted.stderr, direct.stderr);
        }
    }
}

/// Rejected argument owners add no retention beyond the same direct enum singleton controls.
#[test]
fn test_enum_third_review_rejected_argument_ownership() {
    for iterations in [1, 40] {
        let run = |body: &str| compile_and_run_with_heap_debug(&format!(r#"<?php
interface I {{ public function f(int $x = 1): int; }}
class Payload {{ public function __destruct() {{ echo 'D'; }} }}
enum E implements I {{ case A; public function f(int $x = 10, ?Payload $y = null): int {{ return $x; }} }}
enum F implements I {{ case A; public function f(int $x = 20): int {{ return $x; }} }}
function invoke(I $value): int {{ return $value->f(y: new Payload()); }}
for ($i = 0; $i < {iterations}; $i++) {{ {body} }}
"#));
        let direct = run("echo E::A->f(y: new Payload()), '|', F::A->f(), '|done';");
        assert!(direct.success, "{}", direct.stderr);
        assert_eq!(direct.stdout, "D10|20|done".repeat(iterations));
        let out = run("echo invoke(E::A), '|'; try { echo invoke(F::A); } catch (Error $error) { echo $error->getMessage(); unset($error); } echo '|done';");
        assert!(out.success, "{}", out.stderr);
        assert_eq!(out.stdout, "D10|DUnknown named parameter $y|done".repeat(iterations), "{}", out.stderr);
        for metric in ["live_blocks=", "live_bytes="] {
            let retained = |stderr: &str| stderr.lines().next().unwrap().split_whitespace()
                .find(|field| field.starts_with(metric)).unwrap()[metric.len()..].parse::<usize>().unwrap();
            assert_eq!(retained(&out.stderr), retained(&direct.stderr), "{metric}: {}\n{}", out.stderr, direct.stderr);
        }
    }
}

/// A named extension is checked only on the selected enum, after its argument side effects.
#[test]
fn test_enum_third_review_heterogeneous_named_arguments() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x = 1): int; }
enum E implements I { case A; public function f(int $x = 10, int $y = 2): int { return $x + $y; } }
enum F implements I { case A; public function f(int $x = 20): int { return $x; } }
function argument(): int { echo 'Y'; return 4; }
function invoke(I $value): int { return $value->f(y: argument()); }
echo invoke(E::A), '|';
try { echo invoke(F::A); } catch (Error $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "Y14|YUnknown named parameter $y");
}

/// PHP user methods ignore surplus positionals but still evaluate them in source order.
#[test]
fn test_enum_third_review_heterogeneous_positionals() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x = 1): int; }
enum E implements I { case A; public function f(int $x = 10, int $y = 2): int { return $x + $y; } }
enum F implements I { case A; public function f(int $x = 20): int { return $x; } }
function argument(string $label, int $value): int { echo $label; return $value; }
function invoke(I $value): int { return $value->f(argument('X', 4), argument('Y', 6)); }
function defaults(I $value): int { return $value->f(); }
echo invoke(E::A), '|', invoke(F::A), '|', defaults(E::A), ':', defaults(F::A);
"#);
    assert_eq!(out, "XY10|XY4|12:20");
}

/// A sole variadic implementation absorbs every fixed interface parameter in its EIR adapter.
#[test]
fn test_enum_third_review_variadic_absorbs_fixed() {
    for hook in ["", "debug_print_backtrace();"] {
        let out = compile_and_run(&format!(r#"<?php
interface I {{ public function f(int $x, int $y): int; }}
enum E implements I {{ case A; public function f(int ...$rest): int {{ return count($rest); }} }}
function invoke(I $value): int {{ return $value->f(1, 2); }}
{hook}
echo invoke(E::A);
"#));
        assert_eq!(out, "2");
    }
}

/// Interface fixed prefixes and variadic tails are repacked into one implementation variadic.
#[test]
fn test_enum_third_review_variadic_absorbs_prefix() {
    for hook in ["", "debug_print_backtrace();"] {
        let out = compile_and_run(&format!(r#"<?php
interface I {{ public function f(int $x, int ...$rest): int; }}
enum E implements I {{ case A; public function f(int ...$rest): int {{ return count($rest); }} }}
function invoke(I $value): int {{ return $value->f(1, 2, 3); }}
{hook}
echo invoke(E::A);
"#));
        assert_eq!(out, "3");
    }
}

/// Captured optional adapters forward written arguments, never hidden collector addresses.
#[test]
fn test_enum_second_review_captured_optional_defaults() {
    for hook in ["eval('echo 1;');", "debug_print_backtrace();"] {
        let out = compile_and_run(&format!(r#"<?php
interface I {{ public function f(int $x = 1): int; }}
enum E implements I {{ case A; public function f(int $x = 10, int $y = 2): int {{
    echo $x, ':', $y, '|'; return $x + $y;
}} }}
function invoke(I $value): int {{ return $value->f(); }}
{hook}
echo invoke(E::A);
"#));
        assert_eq!(out, if hook.starts_with("eval") { "110:2|12" } else { "10:2|12" });
    }
}

/// Uncaptured interface calls use the implementation's defaults and optional widening.
#[test]
fn test_enum_second_review_implementation_defaults() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x = 1): int; }
interface Required { public function f(int $x): int; }
enum E implements I, Required {
    case A; public function f(int $x = 10, int $y = 2): int { return $x + $y; }
}
function optional(I $value): int { return $value->f(); }
function required(Required $value): int { return $value->f(); }
echo optional(E::A), ':', required(E::A);
"#);
    assert_eq!(out, "12:12");
}

/// Runtime selection retains each enum's defaults and evaluates supplied arguments once.
#[test]
fn test_enum_second_review_runtime_defaults_and_source_order() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x = 1): int; }
enum E implements I { case A; public function f(int $x = 10, int $y = 2): int { return $x + $y; } }
enum F implements I { case A; public function f(int $x = 20, int $y = 3): int { return $x + $y; } }
function argument(string $label, int $value): int { echo $label; return $value; }
function invoke(I $value): string {
    return $value->f() . ':' . $value->f(y: argument('Y', 5), x: argument('X', 4))
        . ':' . $value->f(...[6, 7]);
}
echo invoke(E::A), '|', invoke(F::A);
"#);
    assert_eq!(out, "YX12:9:13|YX23:9:13");
}

/// Enum selection leaves a known ordinary class on its unchanged interface fallback path.
#[test]
fn test_enum_second_review_class_interface_fallback() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x = 1): int; }
enum E implements I { case A; public function f(int $x = 10, int $y = 2): int { return $x + $y; } }
class C implements I { public function f(int $x = 1): int { return $x; } }
function invoke(I $value): string { return $value->f() . ':' . $value->f(3); }
echo invoke(E::A), '|', invoke(new C());
"#);
    assert_eq!(out, "12:5|1:3");
}

/// Guarded nullable enum interface calls retain void result and implementation-default behavior.
#[test]
fn test_enum_second_review_nullsafe_void_defaults() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x = 1): void; }
enum E implements I { case A; public function f(int $x = 10, int $y = 2): void { echo $x, ':', $y; } }
function invoke(?I $value): void { $value?->f(); }
invoke(null); invoke(E::A);
"#);
    assert_eq!(out, "10:2");
}

/// An optional scalar prefix can consume a required interface's variadic element.
#[test]
fn test_enum_second_review_variadic_optional_prefix() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x, int ...$rest): int; }
enum E implements I { case A; public function f(int $x, int $y = 4, int ...$rest): int {
    foreach ($rest as $value) { $x += $value; } return $x + $y;
} }
function invoke(I $value): string { return $value->f(1) . ':' . $value->f(1, 2, 3); }
echo invoke(E::A);
"#);
    assert_eq!(out, "5:6");
}

/// A pure variadic interface can widen to an optional scalar and a remaining variadic tail.
#[test]
fn test_enum_second_review_variadic_scalar_default() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int ...$values): int; }
enum E implements I { case A; public function f(int $first = 7, int ...$values): int {
    foreach ($values as $value) { $first += $value; } return $first;
} }
function invoke(I $value): string { return $value->f(1, 2) . ':' . $value->f(); }
echo invoke(E::A);
"#);
    assert_eq!(out, "3:7");
}

/// Trait variadic requirements compare scalar element hints rather than collector storage.
#[test]
fn test_enum_second_review_trait_variadic_scalar_default() {
    let out = compile_and_run(r#"<?php
trait T { abstract public function f(int ...$values): int; }
enum E { use T; case A; public function f(int $first = 7, int ...$values): int {
    foreach ($values as $value) { $first += $value; } return $first;
} }
echo E::A->f(1, 2), ':', E::A->f();
"#);
    assert_eq!(out, "3:7");
}

/// Captured frames compare declared parameters and still materialize optional defaults.
#[test]
fn test_enum_oct9_captured_optional_interface() {
    for hook in ["eval('echo 1;');", "debug_print_backtrace();"] {
        let out = compile_and_run(&format!(r#"<?php
interface I {{ public function f(int $x): int; }}
enum E implements I {{ case A; public function f(int $x, int $y = 2): int {{ return $x + $y; }} }}
function invoke(I $value): int {{ return $value->f(1); }}
function invoke_extra(I $value): int {{ return $value->f(1, 4); }}
{hook}
echo invoke(E::A), ':', invoke_extra(E::A);
"#));
        assert_eq!(out, if hook.starts_with("eval") { "13:5" } else { "3:5" });
    }
}

/// A source variadic can widen a captured interface signature without exposing hidden slots.
#[test]
fn test_enum_oct9_captured_variadic_interface() {
    for hook in ["eval('echo 1;');", "debug_print_backtrace();"] {
        let out = compile_and_run(&format!(r#"<?php
interface I {{ public function f(int $x): int; }}
enum E implements I {{ case A; public function f(int $x, int ...$rest): int {{
    foreach ($rest as $value) {{ $x += $value; }} return $x;
}} }}
function invoke(I $value): int {{ return $value->f(1); }}
function invoke_extra(I $value): int {{ return $value->f(1, 5); }}
{hook}
echo E::A->f(1, 5), ':', invoke(E::A), ':', invoke_extra(E::A);
"#));
        assert_eq!(out, if hook.starts_with("eval") { "16:1:6" } else { "6:1:6" });
    }
}

/// An added reference return is dereferenced for a by-value interface entry.
#[test]
fn test_enum_oct9_added_reference_return() {
    for (parameters, expected) in [("", "1"), ("int $extra = 1", "2")] {
        let setup = if parameters.is_empty() { "" } else { "$result[0] += $extra;" };
        let out = compile_and_run(&format!(r#"<?php
interface I {{ public function f(): array; }}
enum E implements I {{ case A; public function &f({parameters}): array {{
    $result = [1]; {setup} return $result;
}} }}
function invoke(I $value): int {{ $result = $value->f(); return $result[0]; }}
echo invoke(E::A);
"#));
        assert_eq!(out, expected);
    }
    for hook in ["", "eval('echo 1;');", "debug_print_backtrace();"] {
        let out = compile_and_run(&format!(r#"<?php
interface I {{ public function f(int ...$values): array; }}
enum E implements I {{ case A; public function &f(int ...$values): array {{ return $values; }} }}
function invoke(I $value): string {{ $result = $value->f(1, 2); return $result[0] . ':' . $result[1]; }}
{hook}
echo invoke(E::A);
"#));
        assert_eq!(out, if hook.starts_with("eval") { "11:2" } else { "1:2" });
    }
}

/// Value-return adapters add no retention relative to a direct reference-returning method call.
#[test]
fn test_enum_oct9_added_reference_value_ownership() {
    for iterations in [1, 40] {
        let run = |receiver: &str| compile_and_run_with_heap_debug(&format!(r#"<?php
interface I {{ public function f(): array; }}
enum E implements I {{ case A; public function &f(int $extra = 1): array {{
    $result = [1 + $extra]; return $result;
}} }}
function invoke({receiver} $value): int {{ $result = $value->f(); return $result[0]; }}
for ($i = 0; $i < {iterations}; $i++) {{ echo invoke(E::A); }}
"#));
        let direct = run("E");
        let adapted = run("I");
        assert!(direct.success && adapted.success, "{}\n{}", direct.stderr, adapted.stderr);
        assert_eq!(adapted.stdout, "2".repeat(iterations));
        assert_eq!(adapted.stdout, direct.stdout);
        for metric in ["live_blocks=", "live_bytes="] {
            let retained = |stderr: &str| stderr.lines().next().unwrap().split_whitespace()
                .find_map(|field| field.strip_prefix(metric)).unwrap().parse::<usize>().unwrap();
            assert_eq!(retained(&direct.stderr), retained(&adapted.stderr),
                "direct: {}\nadapter: {}", direct.stderr, adapted.stderr);
        }
    }
}

/// Optional interface adapters preserve a returned reference and its caller mutation.
#[test]
fn test_enum_followup_interface_reference_return() {
    let out = compile_and_run(r#"<?php
interface I { public function &f(): array; }
enum E implements I {
    case A;
    public function &f(int $extra = 1): array {
        $a = [1]; $a[0] += $extra; return $a;
    }
}
function update(I $value): int {
    $r = &$value->f(); $r[0] += 10; return $r[0];
}
echo E::A->name, ':', update(E::A);
"#);
    assert_eq!(out, "A:12");
}

/// Reference returns and reference parameters share the original managed caller cell.
#[test]
fn test_enum_followup_interface_reference_return_alias() {
    let out = compile_and_run(r#"<?php
namespace Adapter;
interface I { public function &f(int &$value): int; }
enum E implements I {
    case A;
    public function &f(int &$value, int $extra = 2): int {
        $value += $extra; return $value;
    }
}
function update(I $enum, int &$value): int {
    $r = &$enum->f($value); $r += 10; return $r;
}
$value = 3;
$root = &$value;
echo update(E::A, $value), ':', $value;
"#);
    assert_eq!(out, "15:15");
}

/// Repeated adapters retain no more storage than the same direct interface reference return.
#[test]
fn test_enum_followup_interface_reference_return_ownership() {
    let run = |iterations, optional| {
        let parameters = if optional { "array $extra = [2]" } else { "" };
        let setup = if optional { "" } else { "$extra = [2];" };
        compile_and_run_with_heap_debug(&format!(r#"<?php
interface I {{ public function &f(): array; }}
enum E implements I {{
    case A;
    public function &f({parameters}): array {{
        {setup} $value = [1]; $value[0] += $extra[0]; return $value;
    }}
}}
function update(I $enum): int {{
    $r = &$enum->f(); $r[0] += 10; return $r[0];
}}
for ($i = 0; $i < {iterations}; $i++) {{ echo update(E::A), ':'; }}
"#))
    };
    // The non-adapted interface path already retains returned storage at shutdown.
    // This regression bounds added adapter/default retention, not that separate lifetime gap.
    for iterations in [1, 40] {
        let baseline = run(iterations, false);
        let adapted = run(iterations, true);
        assert!(baseline.success && adapted.success, "{}\n{}", baseline.stderr, adapted.stderr);
        assert_eq!(adapted.stdout, "13:".repeat(iterations));
        assert_eq!(baseline.stdout, adapted.stdout);
        for metric in ["live_blocks=", "live_bytes="] {
            let retained = |stderr: &str| stderr.lines().next().unwrap()
                .split_whitespace().find_map(|field| field.strip_prefix(metric))
                .unwrap().parse::<usize>().unwrap();
            assert_eq!(retained(&baseline.stderr), retained(&adapted.stderr),
                "baseline: {}\nadapter: {}", baseline.stderr, adapted.stderr);
        }
    }
}

/// Repeated interface calls retire structured default owners instead of accumulating them.
#[test]
fn test_enum_followup_interface_default_ownership() {
    let run = |iterations| compile_and_run_with_heap_debug(&format!(r#"<?php
interface I {{ public function f(int $x): string; }}
enum E implements I {{
    case A;
    public function f(int $x, string $prefix = 'v', array $values = [2]): string {{
        return $prefix . $values[0] . $x;
    }}
}}
function render(I $value): string {{ return $value->f(1); }}
for ($i = 0; $i < {iterations}; $i++) {{ echo render(E::A); }}
"#));
    let once = run(1);
    let repeated = run(100);
    assert!(once.success && repeated.success, "{}\n{}", once.stderr, repeated.stderr);
    assert_eq!(once.stdout, "v21");
    assert_eq!(repeated.stdout, "v21".repeat(100));
    for metric in ["live_blocks=", "live_bytes="] {
        let retained = |stderr: &str| stderr.lines().next().unwrap()
            .split_whitespace().find_map(|field| field.strip_prefix(metric))
            .unwrap().parse::<usize>().unwrap();
        assert_eq!(retained(&once.stderr), retained(&repeated.stderr), "{}", repeated.stderr);
    }
}

/// EIR adapters materialize structured defaults and keep caller reference arguments attached.
#[test]
fn test_enum_followup_interface_structured_defaults_and_references() {
    let out = compile_and_run(r#"<?php
interface Data { public function f(int $x): string; }
enum Value implements Data {
    case A;
    const DATA = ['v'];
    public function f(int $x, string $prefix = 'ok', array $data = self::DATA): string {
        return $prefix . $data[0] . $x;
    }
}
function render(Data $value): string { return $value->f(2); }
interface Ref { public function f(int &$x): int; }
enum RefValue implements Ref {
    case A;
    public function f(int &$x, int $extra = 2): int { $x += $extra; return $x; }
}
function update(Ref $value, int &$x): int { return $value->f($x); }
$x = 3;
echo render(Value::A), ':', update(RefValue::A, $x), ':', $x;
"#);
    assert_eq!(out, "okv2:5:5");
}

/// Enum interface widening preserves direct and interface-dispatched optional argument calls.
#[test]
fn test_enum_followup_optional_interface_calls() {
    let out = compile_and_run(r#"<?php
interface I { public function f(int $x): int; }
enum Extra implements I { case A; public function f(int $x, int $y = 2): int { return $x + $y; } }
enum Optional implements I { case A; public function f(int $x = 3): int { return $x; } }
interface S { public static function f(int $x): int; }
enum StaticExtra implements S { case A; public static function f(int $x = 4, int $y = 5): int { return $x + $y; } }
function call_interface(I $value): int { return $value->f(1); }
echo Extra::A->f(1), ':', Optional::A->f(), ':', StaticExtra::f(), ':', call_interface(Extra::A);
"#);
    assert_eq!(out, "3:3:9:3");
}

/// Trait magic constants retain trait identity while binding class identity to each consumer.
#[test]
fn test_enum_review_trait_magic_constant_values() {
    let out = compile_and_run(r#"<?php
namespace MagicEnum;
trait T { const N = __CLASS__; const T = __TRAIT__; const BOTH = [__CLASS__, __TRAIT__]; }
trait Middle { use T; }
enum E { use Middle; case A; const OWN = __CLASS__; }
enum F { use T; case B; }
echo E::N, ':', E::T, ':', E::BOTH[0], ':', E::BOTH[1], ':', E::OWN, ':', F::N;
"#);
    assert_eq!(out, "MagicEnum\\E:MagicEnum\\T:MagicEnum\\E:MagicEnum\\T:MagicEnum\\E:MagicEnum\\F");
}

/// Enum abstract trait contracts accept final self returns and legal optional parameter widening.
#[test]
fn test_enum_review_legal_trait_signatures() {
    let out = compile_and_run(r#"<?php
trait StaticReturn { abstract public function f(): static; }
enum E { use StaticReturn; case A; public function f(): self { return $this; } }
trait Parameters { abstract public function f(int $x): int; }
enum Extra { use Parameters; case A; public function f(int $x, int $y = 0): int { return $x + $y; } }
enum Optional { use Parameters; case A; public function f(int $x = 1): int { return $x; } }
enum ProtectedBody {
    use Parameters { f as protected; }
    case A;
    protected function f(int $x): int { return $x; }
    public function call(): int { return $this->f(1); }
}
echo E::A->f()->name, ':', Extra::A->f(1), ':', Optional::A->f(), ':', ProtectedBody::A->call();
"#);
    assert_eq!(out, "A:1:1:1");
}

/// Enums validate transitive interface and nested adapted trait contracts without changing dispatch.
#[test]
fn test_enum_review_transitive_interface_and_nested_trait_contracts() {
    let out = compile_and_run_with_heap_debug(r#"<?php
trait Requirement { abstract public function f(int $x): int; }
trait Adapted { use Requirement { f as protected; } }
interface ParentContract { public function label(): string; }
interface Contract extends ParentContract { public static function make(): static; }
enum E implements Contract {
    use Adapted;
    case A;
    protected function f(int $x = 1, int $extra = 2): int { return $x + $extra; }
    public function label(): string { return 'value:' . $this->f(); }
    public static function make(): self { return self::A; }
}

function describe(ParentContract $value): string { return $value->label(); }
echo describe(E::make()), ':', E::A instanceof ParentContract ? 'yes' : 'no';
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "value:3:yes");
    // A bare native enum case already retains its singleton at shutdown. Keep this
    // frontend regression bounded by that control without claiming singleton cleanup.
    let baseline = compile_and_run_with_heap_debug("<?php enum E { case A; } echo E::A->name;");
    assert!(baseline.success, "{}", baseline.stderr);
    for metric in ["live_blocks=", "live_bytes="] {
        let retained = |stderr: &str| stderr.lines().next().unwrap()
            .split_whitespace().find_map(|field| field.strip_prefix(metric))
            .unwrap().parse::<usize>().unwrap();
        assert!(retained(&out.stderr) <= retained(&baseline.stderr), "{}", out.stderr);
    }
}


/// Trait and direct enum constants bind lexical receivers before reflection and native lowering.
#[test]
fn test_enum_trait_constant_lexical_receivers() {
    let out = compile_and_run(r#"<?php
trait LexicalEnumValues { const A = 1; const B = self::A + 2; const N = self::class; }
enum LexicalEnum { case Ready; use LexicalEnumValues; const OWN = self::A + 3; }
echo LexicalEnum::B, "|", LexicalEnum::N, "|", LexicalEnum::OWN, "|";
echo (new ReflectionClass(LexicalEnum::class))->getConstant("B");
"#);
    assert_eq!(out, "3|LexicalEnum|4|3");
}

/// A concrete enum method may widen a required parameter without losing a relative return hint.
#[test]
fn test_enum_trait_abstract_requirement_accepts_parameter_widening() {
    let out = compile_and_run(r#"<?php
trait EnumRequirement {
    abstract public function accept(int $value): self;
}
trait EnumRequirementMiddle { use EnumRequirement; }
enum EnumImplementation {
    case Ready;
    use EnumRequirementMiddle;
    public function accept(mixed $value): self { return $this; }
}
echo EnumImplementation::Ready->accept(7)->name;
"#);
    assert_eq!(out, "Ready");
}

/// A never-returning implementation is a valid covariant narrowing of an abstract return type.
#[test]
fn test_enum_trait_abstract_requirement_accepts_never_return() {
    let out = compile_and_run(r#"<?php
trait NeverEnumRequirement { abstract public function stop(): int; }
enum NeverEnumImplementation {
    case Ready;
    use NeverEnumRequirement;
    public function stop(): never { throw new Error("stop"); }
}
try { NeverEnumImplementation::Ready->stop(); } catch (Error $error) { echo "caught"; }
"#);
    assert_eq!(out, "caught");
}

/// Class-name constant expressions survive enum schema construction as string backing values.
#[test]
fn test_backed_enum_values_accept_named_class_constant_expressions() {
    let out = compile_and_run(r#"<?php
namespace EnumNames;
class Payload {}
use EnumNames\Payload as Alias;
enum Kind: string { case Plain = Alias::class; case Handler = \EnumNames\Payload::class . "Handler"; }
echo Kind::Plain->value, "|", Kind::Handler->value;
"#);
    assert_eq!(out, "EnumNames\\Payload|EnumNames\\PayloadHandler");
}

/// Flattened trait constants remain readable and reflect after the enum's own declarations.
#[test]
fn test_enum_trait_constants_preserve_read_and_reflection_order() {
    let out = compile_and_run(r#"<?php
trait Values { public const TC = 2; }
enum Mode { use Values; public const OWN = 1; case Active; }
echo Mode::TC, "\n";
echo implode(',', array_keys((new ReflectionClass(Mode::class))->getConstants())), "\n";
"#);
    assert_eq!(out, "2\nOWN,Active,TC\n");
}

/// Reflection retains finality for both instance and static enum methods.
#[test]
fn test_enum_final_methods_keep_reflection_flags() {
    let out = compile_and_run(r#"<?php
enum Mode {
    case Active;
    final public function label(): string { return 'active'; }
    final public static function code(): int { return 1; }
    public function ordinary(): int { return 2; }
}
echo (new ReflectionMethod(Mode::class, 'label'))->isFinal() ? 'final' : 'open', "\n";
echo (new ReflectionMethod(Mode::class, 'code'))->isFinal() ? 'final' : 'open', "\n";
echo (new ReflectionMethod(Mode::class, 'ordinary'))->isFinal() ? 'final' : 'open', "\n";
"#);
    assert_eq!(out, "final\nfinal\nopen\n");
}

/// Verifies `Enum` can name an enum and remain usable in type hints and scoped access.
#[test]
fn test_enum_soft_keyword_name_across_runtime_contexts() {
    let out = compile_and_run(
        r#"<?php
enum Enum { case X; }
function describe(Enum $value): string {
    return Enum::class . ":" . $value->name;
}
echo describe(Enum::X);
"#,
    );
    assert_eq!(out, "Enum:X");
}

/// Verifies backed enum with `int` underlying type: `->value` returns the integer case value
/// and `Color::from(2)` resolves to `Color::Green` by identity comparison.
#[test]
fn test_backed_enum_value_and_from_identity() {
    let out = compile_and_run(
        "<?php
        enum Color: int {
            case Red = 1;
            case Green = 2;
            case Blue = 3;
        }
        echo Color::Red->value;
        echo PHP_EOL;
        $c = Color::from(2);
        echo $c === Color::Green;
        ",
    );
    assert_eq!(out, "1\n1");
}

/// Regression: an enum used as a class property / promoted-constructor-param TYPE
/// must resolve. Previously failed with "Unknown type: Tag" because enum names were
/// not pre-declared before the class schema pass resolved member type annotations.
#[test]
fn test_enum_as_promoted_constructor_param_type() {
    let out = compile_and_run(
        "<?php
        enum Tag: string {
            case Div = 'div';
            case Span = 'span';
        }
        final class Element {
            public function __construct(private Tag $tag, private string $text) {}
            public function render(): string {
                return '<' . $this->tag->value . '>' . $this->text . '</' . $this->tag->value . '>';
            }
        }
        echo (new Element(Tag::Div, 'hi'))->render();
        ",
    );
    assert_eq!(out, "<div>hi</div>");
}

/// Verifies an unused function accepts an enum case as the default for an enum-typed parameter.
#[test]
fn test_unused_function_accepts_enum_case_parameter_default() {
    let out = compile_and_run(
        r#"<?php
enum Level: string {
    case Low = "low";
    case High = "high";
}
function unused_level(Level $level = Level::Low): string {
    return $level->value;
}
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies method and promoted-constructor parameters materialize enum case defaults.
#[test]
fn test_method_and_promoted_constructor_accept_enum_case_defaults() {
    let out = compile_and_run(
        r#"<?php
enum Level: string {
    case Low = "low";
    case High = "high";
}
final class Config {
    public function __construct(public Level $level = Level::Low) {}
    public function value(Level $level = Level::High): string {
        return $level->value;
    }
}
$config = new Config();
echo $config->level->value;
echo ":";
echo $config->value();
"#,
    );
    assert_eq!(out, "low:high");
}

/// Verifies `self::` and `parent::` defaults resolve in their declaring method contexts.
#[test]
fn test_method_enum_case_defaults_resolve_relative_receivers() {
    let out = compile_and_run(
        r#"<?php
enum Level: string {
    case Low = "low";
    case High = "high";

    public function value(self $level = self::Low): string {
        return $level->value;
    }
}
class LevelDefaults {
    public const DEFAULT_LEVEL = Level::High;
}
class ChildDefaults extends LevelDefaults {
    public function value(Level $level = parent::DEFAULT_LEVEL): string {
        return $level->value;
    }
}
echo Level::High->value();
echo ":";
echo (new ChildDefaults())->value();
"#,
    );
    assert_eq!(out, "low:high");
}

/// Verifies closure signatures accept and materialize enum case parameter defaults.
#[test]
fn test_closure_accepts_enum_case_parameter_default() {
    let out = compile_and_run(
        r#"<?php
enum Level: string {
    case Low = "low";
    case High = "high";
}
$value = function (Level $level = Level::Low): string {
    return $level->value;
};
echo $value();
"#,
    );
    assert_eq!(out, "low");
}

/// Verifies `Color::tryFrom(99)` returns `null` for an unknown value (with null coalescing to `Color::Red`),
/// `Color::cases()` returns all cases, and case index `1` is `Color::Green` by identity.
#[test]
fn test_enum_try_from_and_cases() {
    let out = compile_and_run(
        "<?php
        enum Color: int {
            case Red = 1;
            case Green = 2;
        }
        $picked = Color::tryFrom(99) ?? Color::Red;
        echo $picked === Color::Red;
        echo PHP_EOL;
        $cases = Color::cases();
        echo count($cases);
        echo PHP_EOL;
        echo $cases[1] === Color::Green;
        ",
    );
    assert_eq!(out, "1\n2\n1");
}

/// Verifies string-backed enum: `Status::from("live")` resolves to `Status::Live` by identity,
/// and `Status::Live->value` returns the `"live"` string.
#[test]
fn test_string_backed_enum_from_and_value() {
    let out = compile_and_run(
        "<?php
        enum Status: string {
            case Draft = \"draft\";
            case Live = \"live\";
        }
        echo Status::from(\"live\") === Status::Live;
        echo PHP_EOL;
        echo Status::Live->value;
        ",
    );
    assert_eq!(out, "1\nlive");
}

/// Verifies pure (unit) enum: `Suit::cases()` returns all cases and `Suit::Hearts === $cases[0]` by identity.
#[test]
fn test_pure_enum_cases_identity() {
    let out = compile_and_run(
        "<?php
        enum Suit {
            case Hearts;
            case Clubs;
        }
        $cases = Suit::cases();
        echo count($cases);
        echo PHP_EOL;
        echo $cases[0] === Suit::Hearts;
        ",
    );
    assert_eq!(out, "2\n1");
}

/// Verifies enum case objects expose PHP's readonly `name` property directly and inside methods.
#[test]
fn test_enum_case_name_property_and_method() {
    let out = compile_and_run(
        "<?php
        enum Suit {
            case Hearts;
            case Clubs;
            public function label(): string {
                return $this->name;
            }
        }
        echo Suit::Hearts->name;
        echo '|';
        echo Suit::Clubs->label();
        ",
    );
    assert_eq!(out, "Hearts|Clubs");
}

/// Verifies enum methods can be imported from traits and run with `$this` bound to the case.
#[test]
fn test_enum_uses_trait_method() {
    let out = compile_and_run(
        "<?php
        trait HasEnumLabel {
            public function label(): string {
                return $this->name;
            }
        }
        enum Suit {
            use HasEnumLabel;
            case Hearts;
            case Clubs;
        }
        echo Suit::Hearts->label();
        echo '|';
        echo Suit::Clubs->label();
        ",
    );
    assert_eq!(out, "Hearts|Clubs");
}

/// Verifies enum trait adaptations support `insteadof` conflict resolution and aliases.
#[test]
fn test_enum_trait_insteadof_and_alias() {
    let out = compile_and_run(
        "<?php
        trait PrimaryEnumLabel {
            public function label(): string {
                return 'P:' . $this->name;
            }
        }
        trait SecondaryEnumLabel {
            public function label(): string {
                return 'S:' . $this->name;
            }
        }
        enum Mode {
            use PrimaryEnumLabel, SecondaryEnumLabel {
                PrimaryEnumLabel::label insteadof SecondaryEnumLabel;
                SecondaryEnumLabel::label as secondaryLabel;
            }
            case Active;
        }
        echo Mode::Active->label();
        echo '|';
        echo Mode::Active->secondaryLabel();
        ",
    );
    assert_eq!(out, "P:Active|S:Active");
}

/// Verifies that `Color::from(99)` throws a catchable `ValueError` with PHP's
/// invalid backing-value message.
#[test]
fn test_enum_from_int_failure_throws_value_error() {
    let out = compile_and_run(
        "<?php
        enum Color: int {
            case Red = 1;
        }
        try {
            Color::from(99);
        } catch (ValueError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "ValueError:99 is not a valid backing value for enum Color"
    );
}

/// Verifies that a missing string-backed enum value is quoted in the catchable
/// `ValueError` message, matching PHP's backed-enum contract.
#[test]
fn test_enum_from_string_failure_throws_value_error() {
    let out = compile_and_run(
        "<?php
        enum Status: string {
            case Draft = \"draft\";
        }
        try {
            Status::from(\"live\");
        } catch (ValueError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "ValueError:\"live\" is not a valid backing value for enum Status"
    );
}

/// Compiles and runs the checked-in `examples/enums/main.php` fixture and asserts stdout includes
/// user-declared enum output, the `->name`/`->value` case introspection loop, the builtin
/// `SortDirection` helper result, and the enum-case property defaults.
///
/// The last row is the #566 section: a declared property, a promoted one and a static one all
/// defaulted to an enum case, plus `=== Level::Low` on the declared one. The identity check is
/// the part that matters — a default that allocated a fresh object, or stored the case slot
/// before it was materialized, would still print the right `->name`.
#[test]
fn test_example_enums_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/enums/main.php"));
    assert_eq!(
        out,
        "1\n2\n3\nRed=1 Green=2 Blue=3 \nDefault=default Match=match MATCH=upper-match \nDESC\n\
         Low High High same"
    );
}

/// Verifies `Color::tryFrom(2)` returns a non-null value and `Color::tryFrom(99)` returns `null`,
/// confirmed with `is_null()`.
#[test]
fn test_enum_try_from_is_null_on_missing_value() {
    let out = compile_and_run(
        "<?php
        enum Color: int {
            case Red = 1;
            case Green = 2;
        }
        echo is_null(Color::tryFrom(2)) ? \"null\" : \"found\";
        echo \"|\";
        echo is_null(Color::tryFrom(99)) ? \"null\" : \"found\";
        ",
    );
    assert_eq!(out, "found|null");
}

/// Verifies `Color::tryFrom(99)` stored in an untyped variable is `null` and `Color::tryFrom(1)` is not null,
/// confirmed through separate variables echoed via `is_null()`.
#[test]
fn test_enum_try_from_is_null_through_nullable_variable() {
    let out = compile_and_run(
        "<?php
        enum Color: int {
            case Red = 1;
        }
        $missing = Color::tryFrom(99);
        $present = Color::tryFrom(1);
        echo is_null($missing) ? \"null\" : \"found\";
        echo \"|\";
        echo is_null($present) ? \"null\" : \"found\";
        ",
    );
    assert_eq!(out, "null|found");
}

/// Verifies `Color::tryFrom(99)` stored in a nullable typed local `?Color` is `null`
/// and `Color::tryFrom(1)` in `?Color` resolves to `Color::Red`.
#[test]
fn test_nullable_enum_typed_local_accepts_try_from_result() {
    let out = compile_and_run(
        "<?php
        enum Color: int {
            case Red = 1;
        }
        ?Color $missing = Color::tryFrom(99);
        ?Color $present = Color::tryFrom(1);
        echo is_null($missing) ? \"null\" : \"found\";
        echo \"|\";
        echo $present === Color::Red ? \"red\" : \"other\";
        ",
    );
    assert_eq!(out, "null|red");
}

/// Verifies namespaced enum case resolution: `RenderMode::Split` is accessible directly in
/// the namespace where it is declared, via `use` imports, and through a static method that
/// receives it as a parameter. Asserts both "local" and "import" paths work correctly.
#[test]
fn test_namespaced_enum_cases_resolve_inside_namespace_and_imports() {
    let out = compile_and_run(
        r#"<?php
namespace Showcases\Doom\App;

enum RenderMode {
    case Map2D;
    case World3D;
    case Split;
}

class Config {
    public static function defaultMode() {
        return RenderMode::Split;
    }
}

namespace Showcases\Doom\Render;

use Showcases\Doom\App\RenderMode;

class Renderer {
    public static function isSplit($mode): bool {
        return $mode === RenderMode::Split;
    }
}

namespace Showcases\Doom;

use Showcases\Doom\App\Config;
use Showcases\Doom\App\RenderMode;
use Showcases\Doom\Render\Renderer;

echo Config::defaultMode() === RenderMode::Split ? "local" : "bad";
echo "|";
echo Renderer::isSplit(RenderMode::Split) ? "import" : "bad";
"#,
    );
    assert_eq!(out, "local|import");
}

/// Verifies PHP 8.6's builtin `SortDirection` unit enum exposes both singleton
/// cases through direct access, `cases()`, `enum_exists()`, and class-like introspection.
#[test]
fn test_builtin_sort_direction_cases_and_introspection() {
    let out = compile_and_run(
        "<?php
        $cases = SortDirection::cases();
        echo count($cases);
        echo '|';
        echo $cases[0] === SortDirection::Ascending ? 'A' : 'bad';
        echo '|';
        echo $cases[1] === SortDirection::Descending ? 'D' : 'bad';
        echo '|';
        echo enum_exists('sortdirection', false) ? 'enum' : 'missing';
        echo '|';
        echo class_exists('SortDirection', false) ? 'class' : 'missing';
        ",
    );
    assert_eq!(out, "2|A|D|enum|class");
}

/// Verifies builtin `SortDirection` can be used in parameter and return type
/// declarations and can drive a `match` expression over enum case singletons.
#[test]
fn test_builtin_sort_direction_typed_function_return_and_match() {
    let out = compile_and_run(
        "<?php
        function default_direction(): SortDirection {
            return SortDirection::Ascending;
        }

        function sort_keyword(SortDirection $direction): string {
            return match ($direction) {
                SortDirection::Ascending => 'ASC',
                SortDirection::Descending => 'DESC',
            };
        }

        echo sort_keyword(default_direction());
        echo '|';
        echo sort_keyword(SortDirection::Descending);
        ",
    );
    assert_eq!(out, "ASC|DESC");
}

/// Verifies namespaced code can reference the global builtin `SortDirection`
/// through PHP class-like name rules: imports and fully-qualified names work.
#[test]
fn test_builtin_sort_direction_resolves_from_namespaced_code() {
    let out = compile_and_run(
        r#"<?php
namespace App;

use SortDirection;

function is_ascending(SortDirection $direction): bool {
    return $direction === \SortDirection::Ascending;
}

echo is_ascending(SortDirection::Ascending) ? "import" : "bad";
echo "|";
echo \SortDirection::Descending === SortDirection::Descending ? "fqcn" : "bad";
"#,
    );
    assert_eq!(out, "import|fqcn");
}

/// Verifies builtin `SortDirection` cases can be used as enum case constants
/// and then compared by singleton identity.
#[test]
fn test_builtin_sort_direction_case_constant() {
    let out = compile_and_run(
        "<?php
        const DEFAULT_DIRECTION = SortDirection::Descending;
        echo DEFAULT_DIRECTION === SortDirection::Descending ? 'ok' : 'bad';
        ",
    );
    assert_eq!(out, "ok");
}

/// Verifies that a pure enum can declare an instance method that uses `$this` for identity.
#[test]
fn test_enum_instance_method() {
    let out = compile_and_run(
        "<?php
        enum Suit {
            case Hearts;
            case Spades;
            public function label(): string {
                return $this === Suit::Hearts ? \"H\" : \"S\";
            }
        }
        echo Suit::Hearts->label() . Suit::Spades->label();
        ",
    );
    assert_eq!(out, "HS");
}

/// Verifies that an enum instance method can `match` on `$this` (the canonical enum-method form).
#[test]
fn test_enum_method_match_on_this() {
    let out = compile_and_run(
        "<?php
        enum Suit {
            case Hearts; case Diamonds; case Clubs; case Spades;
            public function color(): string {
                return match($this) {
                    Suit::Hearts, Suit::Diamonds => \"red\",
                    Suit::Clubs, Suit::Spades => \"black\",
                };
            }
        }
        echo Suit::Hearts->color() . \"-\" . Suit::Spades->color();
        ",
    );
    assert_eq!(out, "red-black");
}

/// Verifies that a backed enum method can read `$this->value`.
#[test]
fn test_enum_method_reads_backing_value() {
    let out = compile_and_run(
        "<?php
        enum Power: int {
            case Low = 1;
            case High = 10;
            public function label(): string { return $this->name . ':' . ($this->value * 2); }
        }
        echo Power::High->label();
        ",
    );
    assert_eq!(out, "High:20");
}

/// Verifies that a static enum method (a factory) dispatches and returns a case.
#[test]
fn test_enum_static_method() {
    let out = compile_and_run(
        "<?php
        enum Color {
            case Red;
            case Green;
            public static function fallback(): self { return Color::Red; }
        }
        echo Color::fallback() === Color::Red ? \"ok\" : \"no\";
        ",
    );
    assert_eq!(out, "ok");
}

/// Verifies that an enum can implement an interface and be used through it.
#[test]
fn test_enum_implements_interface() {
    let out = compile_and_run(
        "<?php
        interface HasLabel { public function label(): string; }
        enum Suit implements HasLabel {
            case Hearts;
            public function label(): string { return \"hearts\"; }
        }
        function describe(HasLabel $h): string { return $h->label(); }
        echo describe(Suit::Hearts);
        ",
    );
    assert_eq!(out, "hearts");
}

/// Verifies that an enum instance method can read `$this->name` (and `$this->value`), dispatching
/// on the case singleton. Previously `$this->name` inside a method was unsupported.
#[test]
fn test_enum_method_reads_this_name() {
    let out = compile_and_run(
        "<?php
        enum Suit: string {
            case Hearts = \"h\";
            case Spades = \"s\";
            public function describe(): string {
                return $this->name . \"=\" . $this->value;
            }
        }
        echo Suit::Hearts->describe();
        echo PHP_EOL;
        echo Suit::Spades->describe();
        ",
    );
    assert_eq!(out, "Hearts=h\nSpades=s");
}

/// Verifies that an enum method can reference a class constant via `self::`.
#[test]
fn test_enum_method_uses_self_constant() {
    let out = compile_and_run(
        "<?php
        enum Scale {
            case One;
            const FACTOR = 5;
            public function compute(): int { return self::FACTOR * 3; }
        }
        echo Scale::One->compute();
        ",
    );
    assert_eq!(out, "15");
}

/// Compiles and runs the checked-in `examples/enum-methods/main.php` fixture, covering instance
/// methods (`match($this)`, `$this->value`, `self::CONST`), a static factory, and `implements`.
#[test]
fn test_example_enum_methods_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/enum-methods/main.php"));
    assert_eq!(out, "red/black\ndiamonds\nblack\n52\nclubs\nSuit\n");
}

/// Verifies a pure (unit) enum case exposes the read-only `->name` property holding the
/// case identifier (issue #330). This is the minimal reproduction from the bug report.
#[test]
fn test_pure_enum_name_property() {
    let out = compile_and_run(
        "<?php
        enum E { case A; }
        echo E::A->name;
        ",
    );
    assert_eq!(out, "A");
}

/// Verifies a backed enum case exposes both `->name` (case identifier) and `->value`
/// (backing value), and that the two are distinct properties.
#[test]
fn test_backed_enum_name_and_value() {
    let out = compile_and_run(
        "<?php
        enum Code: int {
            case Ok = 1;
            case Err = 2;
        }
        echo Code::Err->name;
        echo PHP_EOL;
        echo Code::Err->value;
        ",
    );
    assert_eq!(out, "Err\n2");
}

/// Verifies a string-backed enum case `->name` returns the case identifier, not the
/// backing string value (`Status::Live->name` is `"Live"`, not `"live"`).
#[test]
fn test_string_backed_enum_name_distinct_from_value() {
    let out = compile_and_run(
        "<?php
        enum Status: string {
            case Draft = \"draft\";
            case Live = \"live\";
        }
        echo Status::Live->name;
        echo PHP_EOL;
        echo Status::Live->value;
        ",
    );
    assert_eq!(out, "Live\nlive");
}

/// Verifies `->name` reads correctly when the case singleton is aliased through a local
/// variable and when retrieved from the `cases()` array, exercising the string property
/// through assignment and array storage of the shared singleton.
#[test]
fn test_enum_name_through_variable_and_cases() {
    let out = compile_and_run(
        "<?php
        enum Suit { case Hearts; case Clubs; }
        $x = Suit::Clubs;
        echo $x->name;
        echo PHP_EOL;
        $cases = Suit::cases();
        echo $cases[0]->name;
        echo $cases[1]->name;
        ",
    );
    assert_eq!(out, "Clubs\nHeartsClubs");
}

/// Verifies `->name` works inside string interpolation alongside `->value`, matching PHP's
/// `"{$case->name}"` behavior.
#[test]
fn test_enum_name_in_interpolation() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 9; }
        $l = Level::High;
        echo \"name={$l->name} value={$l->value}\";
        ",
    );
    assert_eq!(out, "name=High value=9");
}

/// Regression for #349: an int-backed enum's `from()` accepts a PHP numeric string
/// (e.g. `"1"`), coercing it to the integer backing value and returning the matching
/// case — instead of rejecting the string argument at compile time.
#[test]
fn test_backed_int_enum_from_numeric_string() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        echo Level::from(\"1\")->name;
        echo Level::from(\"2\")->name;
        ",
    );
    assert_eq!(out, "LowHigh");
}

/// Regression for #349: PHP's coercive int parameter rules accept signed,
/// whitespace-padded, decimal-float, and exponent-form numeric strings for
/// int-backed enum `from()`, with float strings truncated toward zero.
#[test]
fn test_backed_int_enum_from_numeric_string_coercion_forms() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Zero = 0; case One = 1; case Thousand = 1000; }
        echo Level::from(\" 1 \")->name, \"|\";
        echo Level::from(\"+1\")->name, \"|\";
        echo Level::from(\"1.0\")->name, \"|\";
        echo Level::from(\"1.5\")->name, \"|\";
        echo Level::from(\"1e3\")->name, \"|\";
        echo Level::from(\".5\")->name;
        ",
    );
    assert_eq!(out, "One|One|One|One|Thousand|Zero");
}

/// Regression for #349: `tryFrom()` on an int-backed enum coerces a numeric string and
/// returns the matching case, or `null` when the coerced value matches no case.
#[test]
fn test_backed_int_enum_tryfrom_numeric_string() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        echo Level::tryFrom(\"1\")->name;
        $miss = Level::tryFrom(\"3\");
        echo $miss === null ? \"null\" : $miss->name;
        ",
    );
    assert_eq!(out, "Lownull");
}

/// Regression for #349: a numeric string that coerces to an int with no matching case
/// throws a `ValueError` with PHP's backing-value message (like an integer argument would).
#[test]
fn test_backed_int_enum_from_unmatched_numeric_string_throws_value_error() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        try {
            Level::from(\"3\");
        } catch (ValueError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(out, "ValueError:3 is not a valid backing value for enum Level");
}

/// Regression for #349: negative numeric strings still coerce successfully before
/// enum lookup, so a missing negative backing value raises `ValueError`, not `TypeError`.
#[test]
fn test_backed_int_enum_from_negative_numeric_string_throws_value_error() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        try {
            Level::from(\"-1\");
        } catch (ValueError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "ValueError:-1 is not a valid backing value for enum Level"
    );
}

/// Regression for #349: a non-numeric string passed to an int-backed enum's `from()`
/// throws a `TypeError` at runtime with PHP's exact argument-type message, matching
/// PHP's coercive-typing behavior instead of being accepted or rejected at compile time.
#[test]
fn test_backed_int_enum_from_non_numeric_string_throws_type_error() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        try {
            Level::from(\"x\");
        } catch (TypeError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "TypeError:Level::from(): Argument #1 ($value) must be of type int, string given"
    );
}

/// Regression for #349: enum int-parameter coercion must reject libc `strtod`
/// extensions (`0x` hex floats, `INF`/`INFINITY`, and `NAN`) as `TypeError`, even
/// though the general numeric-string probe used by loose comparison accepts them.
#[test]
fn test_backed_int_enum_from_rejects_strtod_only_numeric_forms() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        function mark(string $value) {
            try {
                Level::from($value);
                echo \"ok\";
            } catch (TypeError $e) {
                echo \"T\";
            } catch (ValueError $e) {
                echo \"V\";
            }
            echo \"|\";
        }
        mark(\"0x1\");
        mark(\"0X1\");
        mark(\"INF\");
        mark(\"inf\");
        mark(\"+inf\");
        mark(\"Infinity\");
        mark(\"NAN\");
        mark(\"nan\");
        mark(\"1abc\");
        mark(\"\");
        ",
    );
    assert_eq!(out, "T|T|T|T|T|T|T|T|T|T|");
}

/// Regression for #349: repeatedly coercing a heap-owned numeric string through an
/// int-backed enum `from()` in a loop and storing the reassigned result must keep the
/// case singleton's refcount balanced (it is a persistent, program-lifetime object).
/// Before the fix the returned singleton was under-retained and freed after a few
/// iterations, corrupting the heap free-list and crashing. Exercises the ownership path,
/// not just single-shot coercion.
#[test]
fn test_backed_int_enum_from_numeric_string_in_loop_keeps_singleton_alive() {
    let out = compile_and_run(
        "<?php
        enum L: int { case A = 1; case B = 2; }
        $acc = 0;
        for ($n = 0; $n < 50; $n++) {
            $t = str_repeat(\"2\", 1);
            $c = L::from($t);
            $acc += $c->value;
        }
        echo $acc;
        ",
    );
    assert_eq!(out, "100");
}

/// Regression for #349: `tryFrom()` also throws a `TypeError` (not `null`) for a
/// non-numeric string argument, matching PHP.
#[test]
fn test_backed_int_enum_tryfrom_non_numeric_string_throws_type_error() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        try {
            Level::tryFrom(\"x\");
        } catch (TypeError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "TypeError:Level::tryFrom(): Argument #1 ($value) must be of type int, string given"
    );
}

/// Regression for #349: `tryFrom()` shares the same int-parameter coercion probe as
/// `from()`, so strtod-only forms such as `NAN` throw `TypeError` instead of returning
/// `null` or entering enum backing-value lookup.
#[test]
fn test_backed_int_enum_tryfrom_rejects_strtod_only_numeric_forms() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        try {
            Level::tryFrom(\"NAN\");
        } catch (TypeError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "TypeError:Level::tryFrom(): Argument #1 ($value) must be of type int, string given"
    );
}

/// Regression for #449: an int-backed enum's `from()` accepts a `Mixed` argument — here the
/// value of a `foreach` over a string array — coercing each element to the backing value.
/// Before the fix this failed to compile with "backing input PHP type Mixed".
#[test]
fn test_backed_int_enum_from_mixed_foreach_value() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        foreach ([\"1\", \"2\", \"1\"] as $v) {
            echo Level::from($v)->name;
        }
        ",
    );
    assert_eq!(out, "LowHighLow");
}

/// Regression for #449: an untyped (`Mixed`) parameter passed to `from()` coerces on its
/// runtime tag — a numeric string and an integer both resolve to the matching case.
#[test]
fn test_backed_int_enum_from_mixed_untyped_param() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        function pick($x) { return Level::from($x)->name; }
        echo pick(\"1\"), pick(2), pick(\"2\");
        ",
    );
    assert_eq!(out, "LowHighHigh");
}

/// Verifies keyword-named enum cases remain distinct by exact case, resolve only through their
/// declared spelling, and expose that spelling through the PHP `->name` property.
#[test]
fn test_keyword_named_enum_cases_preserve_case_and_name() {
    let out = compile_and_run(
        "<?php
        enum KeywordCase: string {
            case Default = 'default';
            case DEFAULT = 'upper-default';
            case Match = 'match';
            case MATCH = 'upper-match';
            case Print = 'print';
        }
        foreach (KeywordCase::cases() as $case) {
            echo $case->name, '=', $case->value, ';';
        }
        ",
    );
    assert_eq!(
        out,
        "Default=default;DEFAULT=upper-default;Match=match;MATCH=upper-match;Print=print;"
    );
}

/// Regression for #449: a heterogeneous (`Mixed`) array mixing coercible and non-coercible
/// values dispatches per element — integers and numeric strings resolve, a non-numeric
/// string and an array each throw `TypeError` with PHP's runtime-type message.
#[test]
fn test_backed_int_enum_from_mixed_per_element_dispatch() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        function pick($x) {
            try {
                return Level::from($x)->name;
            } catch (TypeError $e) {
                return $e->getMessage();
            }
        }
        foreach ([1, \"2\", \"x\", [9]] as $v) {
            echo pick($v), \"|\";
        }
        ",
    );
    assert_eq!(
        out,
        "Low|High|Level::from(): Argument #1 ($value) must be of type int, string given|Level::from(): Argument #1 ($value) must be of type int, array given|"
    );
}

/// Regression for #449: `tryFrom()` on a `Mixed` argument coerces and returns the matching
/// case, or `null` when the coerced value matches no case.
#[test]
fn test_backed_int_enum_tryfrom_mixed() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        function pick($x) { $r = Level::tryFrom($x); return $r === null ? \"null\" : $r->name; }
        echo pick(\"2\"), pick(\"9\"), pick(1);
        ",
    );
    assert_eq!(out, "HighnullLow");
}

/// Issue #566: a DIRECTLY declared typed property may default to an enum case, and gets the
/// canonical singleton — not a fresh object, and not null.
///
/// Constructor promotion already worked, because its default is evaluated as a parameter
/// default before the assignment; a plain `public Level $level = Level::Low;` had no path at
/// all. The checker rejected it at schema time (enum cases do not exist yet there, so
/// `Level::Low` typed as `Str`), and behind that the EIR property-initialization path had no
/// form for it either — `object_new for default value of property $level`.
///
/// `===` is the assertion that matters: a default that allocated a new object, or that stored
/// the still-null lazy slot, would print the right `->name` and still be a different value
/// from `Level::Low` everywhere else in the program. The singleton is materialized lazily, so
/// the fourth row — a default written before the case's first use anywhere — is the one that
/// catches storing the unmaterialized slot.
///
/// All three declaration forms are here together, and asserted to agree with each other:
/// static, instance, and promoted. Every expectation is the host PHP 8.5.10 output.
#[test]
fn test_enum_case_default_on_every_property_form_is_the_singleton() {
    let out = compile_and_run(
        r#"<?php
enum Level
{
    case Low;
    case High;
}

enum Backed: string
{
    case Alpha = 'a';
}

class Config
{
    public static Level $shared = Level::High;
    public Level $level = Level::Low;
    public Backed $backed = Backed::Alpha;

    public function __construct(public Level $promoted = Level::High) {}
}

$c = new Config();
var_dump(Config::$shared === Level::High);
var_dump($c->level === Level::Low);
var_dump($c->level === Level::High);
var_dump($c->promoted === Level::High);
var_dump($c->level === $c->promoted);
var_dump($c->level->name);
var_dump($c->backed->value);

$c->level = Level::High;
var_dump($c->level === Level::High);
Config::$shared = Level::Low;
var_dump(Config::$shared === Level::Low);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "bool(false)\n",
            "bool(true)\n",
            "bool(false)\n",
            "string(3) \"Low\"\n",
            "string(1) \"a\"\n",
            "bool(true)\n",
            "bool(true)\n",
        )
    );
}


/// An enum-case default makes its slot a new OWNER of the singleton, so it must retain it.
///
/// Storing the borrowed global without an incref is not a leak but its opposite: the first
/// thing that releases the slot — a property reassignment, an object's cleanup, a web worker's
/// static teardown — consumes the global's only reference and frees a case that is still
/// reachable by name. Lazy materialization then hands the freed block to the NEXT case, so two
/// cases end up sharing one object; this is the same under-retention that #349 fixed for an
/// ordinary `Enum::Case` read, reached through the default paths instead.
///
/// The loop is what makes it observable. Pre-fix, 500 iterations of
/// `$c = new Config(); $c->level = Level::High;` left `Level::Low->name` reading back `""`
/// from the reused block, while `Level::Low === Level::Low` still answered `true` — identity
/// survives a dangling pointer, so `->name` is the assertion that catches it.
///
/// All three default forms churn here, because each has its own store site: the instance path
/// and the promoted one go through `emit_property_default`, the static one through
/// `emit_static_property_default_value`.
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_enum_case_defaults_retain_the_singleton_across_reassignment() {
    let out = compile_and_run(
        r#"<?php
enum Level
{
    case Low;
    case High;
}

class Config
{
    public static Level $shared = Level::Low;
    public Level $level = Level::Low;

    public function __construct(public Level $promoted = Level::Low) {}
}

function churn(int $n): void
{
    for ($i = 0; $i < $n; $i++) {
        $config = new Config();
        $config->level = Level::High;
        $config->promoted = Level::High;
        Config::$shared = Level::High;
        Config::$shared = Level::Low;
    }
}

churn(500);
var_dump(Level::Low->name);
var_dump(Level::High->name);
$fresh = new Config();
var_dump($fresh->level === Level::Low);
var_dump($fresh->promoted === Level::Low);
var_dump(Config::$shared === Level::Low);
var_dump($fresh->level->name);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "string(3) \"Low\"\n",
            "string(4) \"High\"\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "string(3) \"Low\"\n",
        )
    );
}
