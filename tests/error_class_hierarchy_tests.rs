//! Purpose:
//! End-to-end tests for PHP's built-in `Error` class hierarchy.
//!
//! Before this increment, `catch (\ArgumentCountError $e)` — and the same for
//! `DivisionByZeroError` and `AssertionError` — failed the COMPILE with
//! `Undefined class: ArgumentCountError`. The catch clause itself was rejected, so no PHP
//! program that handles those errors could be built at all, regardless of whether the error
//! was ever raised. `TypeError`, `ValueError`, `ArithmeticError`, and `UnhandledMatchError`
//! already existed; the three missing names now join them through the SAME mechanism
//! (`inject_builtin_throwables` in `src/types/checker/builtin_types/declarations.rs`), so
//! they inherit the whole Throwable API — `getMessage`, `getCode`, `getFile`, `getLine`,
//! `getPrevious` — transitively from `Error` exactly like `RuntimeException` does from
//! `Exception`.
//!
//! Called from:
//! - `cargo test --test error_class_hierarchy_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp
//!   dir, compile a plain executable, run it, and assert stdout — the same harness style as
//!   `array_result_type_tests` / `opcache_ini_tests`. Host-target only (macOS aarch64 local).
//! - Every expected value was taken from reference PHP 8.5.6 (`php -d xdebug.mode=off`).
//!   The hierarchy itself was verified with `var_dump(class_parents(...))`:
//!   `ArgumentCountError` -> `["TypeError", "Error"]`, `DivisionByZeroError` ->
//!   `["ArithmeticError", "Error"]`, `AssertionError` -> `["Error"]`.
//! - COMPILE-TIME VS RUNTIME DIVERGENCE (by design, asserted below): reference PHP raises
//!   `ArgumentCountError` at RUNTIME for a bad builtin arity, e.g. `opcache_reset(1)`.
//!   elephc is an AOT compiler and rejects that arity at COMPILE time. These tests require
//!   only that the `catch` clause COMPILES and that a genuinely thrown error is catchable —
//!   NOT that elephc defers its static checks. `catch_clause_compiles_while_static_arity_check_still_rejects`
//!   pins both halves so neither can drift silently.
//! - REGRESSION ANCHOR (`intdiv`): `intdiv($a, 0)` used to write a bare
//!   `Fatal error: division by zero` to stderr and exit 1 — uncatchable. It now raises
//!   reference PHP's `DivisionByZeroError` with php-src's `Division by zero` wording. A
//!   regression shows up as `catch (\DivisionByZeroError $e)` never firing.
//! - The zero-divisor operands go through a FUNCTION PARAMETER on purpose. A literal
//!   `intdiv(1, 0)` risks being const-folded, which would probe the optimizer instead of the
//!   codegen guard under test.
//! - Compile-failure assertions filter stderr through `elephc_diagnostics` because the system
//!   linker (GNU `ld` on Linux) emits warnings macOS does not.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Runs the compiler on `source` and returns its raw output.
fn compile_raw(dir: &Path, source: &str, stem: &str) -> std::process::Output {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&php);
    cmd.output().expect("failed to spawn elephc")
}

/// Compiles `source` to a plain executable and returns its path.
fn compile(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let output = compile_raw(dir, source, stem);
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Compiles `source` expecting FAILURE and returns elephc's own diagnostics from stderr.
fn compile_expecting_failure(dir: &Path, source: &str, stem: &str) -> String {
    let output = compile_raw(dir, source, stem);
    let raw = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "elephc compile unexpectedly SUCCEEDED — the checker over-accepted:\n{raw}"
    );
    elephc_diagnostics(&raw)
}

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than
/// anything elephc emitted: GNU `ld` on Linux reports the static-`getaddrinfo` glibc notes and
/// the `.note.GNU-stack` deprecation, while Apple's linker stays silent. elephc's own lines
/// start with `error`/`warning`, so anchoring on those prefixes isolates its diagnostics — and
/// still surfaces an UNEXPECTED elephc diagnostic, which an allow-list would have hidden.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("error")
                || line.starts_with("Error")
                || line.starts_with("warning")
                || line.starts_with("Warning: ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runs a compiled executable and returns its stdout, asserting a clean exit.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Runs a compiled executable expecting a FATAL exit, returning its combined output.
fn run_binary_expecting_fatal(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert!(
        !output.status.success(),
        "compiled binary unexpectedly exited 0 — the uncaught error did not fatal"
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

// ---------------------------------------------------------------------------
// 1 — the classes EXIST and are catchable by their own name
// ---------------------------------------------------------------------------

/// THE ORIGINAL GAP. Each builtin Error class must be catchable BY ITS OWN NAME.
///
/// `ArgumentCountError`, `DivisionByZeroError`, and `AssertionError` did not exist, so the
/// `catch` clause naming any of them was a compile error before any code ran. The other four
/// rows are the classes that already existed and must keep working.
#[test]
fn every_builtin_error_class_is_catchable_by_its_own_name() {
    let dir = make_test_dir("error_hierarchy_own_name");
    let src = "<?php \
        try { throw new \\Error('a'); } catch (\\Error $e) { echo 'Error:', $e->getMessage(), \"\\n\"; } \
        try { throw new \\TypeError('b'); } catch (\\TypeError $e) { echo 'TypeError:', $e->getMessage(), \"\\n\"; } \
        try { throw new \\ArgumentCountError('c'); } catch (\\ArgumentCountError $e) { echo 'ArgumentCountError:', $e->getMessage(), \"\\n\"; } \
        try { throw new \\ValueError('d'); } catch (\\ValueError $e) { echo 'ValueError:', $e->getMessage(), \"\\n\"; } \
        try { throw new \\ArithmeticError('e'); } catch (\\ArithmeticError $e) { echo 'ArithmeticError:', $e->getMessage(), \"\\n\"; } \
        try { throw new \\DivisionByZeroError('f'); } catch (\\DivisionByZeroError $e) { echo 'DivisionByZeroError:', $e->getMessage(), \"\\n\"; } \
        try { throw new \\AssertionError('g'); } catch (\\AssertionError $e) { echo 'AssertionError:', $e->getMessage(), \"\\n\"; } \
        try { throw new \\UnhandledMatchError('h'); } catch (\\UnhandledMatchError $e) { echo 'UnhandledMatchError:', $e->getMessage(), \"\\n\"; }";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "Error:a\nTypeError:b\nArgumentCountError:c\nValueError:d\n\
         ArithmeticError:e\nDivisionByZeroError:f\nAssertionError:g\nUnhandledMatchError:h\n",
        "each builtin Error class must be throwable and catchable by its own name"
    );
}

/// Every class must be catchable by EACH of its ancestors, and by `Throwable`.
///
/// This is the catch-matching walk through the nominal hierarchy — the thing a bare class
/// declaration does not give you for free. `ArgumentCountError` is the interesting row: it is
/// the only builtin Error subclass that is NOT a direct child of `Error`, so a broken parent
/// link shows up here as `catch (TypeError)` failing to match it.
#[test]
fn each_error_class_is_catchable_by_every_ancestor() {
    let dir = make_test_dir("error_hierarchy_ancestors");
    let src = "<?php \
        try { throw new \\ArgumentCountError('1'); } catch (\\TypeError $e) { echo \"ACE<-TypeError\\n\"; } \
        try { throw new \\ArgumentCountError('2'); } catch (\\Error $e) { echo \"ACE<-Error\\n\"; } \
        try { throw new \\ArgumentCountError('3'); } catch (\\Throwable $e) { echo \"ACE<-Throwable\\n\"; } \
        try { throw new \\DivisionByZeroError('4'); } catch (\\ArithmeticError $e) { echo \"DBZ<-ArithmeticError\\n\"; } \
        try { throw new \\DivisionByZeroError('5'); } catch (\\Error $e) { echo \"DBZ<-Error\\n\"; } \
        try { throw new \\DivisionByZeroError('6'); } catch (\\Throwable $e) { echo \"DBZ<-Throwable\\n\"; } \
        try { throw new \\AssertionError('7'); } catch (\\Error $e) { echo \"AE<-Error\\n\"; } \
        try { throw new \\AssertionError('8'); } catch (\\Throwable $e) { echo \"AE<-Throwable\\n\"; } \
        try { throw new \\TypeError('9'); } catch (\\Error $e) { echo \"TE<-Error\\n\"; } \
        try { throw new \\ValueError('10'); } catch (\\Throwable $e) { echo \"VE<-Throwable\\n\"; }";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "ACE<-TypeError\nACE<-Error\nACE<-Throwable\n\
         DBZ<-ArithmeticError\nDBZ<-Error\nDBZ<-Throwable\n\
         AE<-Error\nAE<-Throwable\nTE<-Error\nVE<-Throwable\n",
        "catch matching must walk the whole nominal ancestor chain"
    );
}

/// NEGATIVE CONTROL: a class must NOT be caught by an unrelated SIBLING.
///
/// Seeding the whole Error branch into the emitted class tables (so codegen-raised errors
/// have descriptors) must not make the runtime match everything against everything. Each row
/// puts the wrong sibling FIRST so an over-broad match would be observable, then a correct
/// wider clause proves the throw really happened and was not swallowed.
#[test]
fn error_classes_are_not_caught_by_unrelated_siblings() {
    let dir = make_test_dir("error_hierarchy_siblings");
    let src = "<?php \
        try { throw new \\ArgumentCountError('a'); } catch (\\ValueError $e) { echo \"BAD:ACE<-ValueError\\n\"; } catch (\\Error $e) { echo \"ok:ACE\\n\"; } \
        try { throw new \\ArgumentCountError('b'); } catch (\\ArithmeticError $e) { echo \"BAD:ACE<-ArithmeticError\\n\"; } catch (\\Error $e) { echo \"ok:ACE2\\n\"; } \
        try { throw new \\DivisionByZeroError('c'); } catch (\\TypeError $e) { echo \"BAD:DBZ<-TypeError\\n\"; } catch (\\Error $e) { echo \"ok:DBZ\\n\"; } \
        try { throw new \\AssertionError('d'); } catch (\\TypeError $e) { echo \"BAD:AE<-TypeError\\n\"; } catch (\\ValueError $e) { echo \"BAD:AE<-ValueError\\n\"; } catch (\\Error $e) { echo \"ok:AE\\n\"; } \
        try { throw new \\TypeError('e'); } catch (\\ArgumentCountError $e) { echo \"BAD:TE<-ACE\\n\"; } catch (\\Error $e) { echo \"ok:TE\\n\"; } \
        try { throw new \\ArithmeticError('f'); } catch (\\DivisionByZeroError $e) { echo \"BAD:AR<-DBZ\\n\"; } catch (\\Error $e) { echo \"ok:AR\\n\"; }";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out, "ok:ACE\nok:ACE2\nok:DBZ\nok:AE\nok:TE\nok:AR\n",
        "an Error subclass must not be caught by a sibling, nor a parent by its own child"
    );
}

/// The whole `Error` branch must stay disjoint from the `Exception` branch.
///
/// PHP splits `Throwable` into `Error` and `Exception`; neither catches the other. Only
/// `catch (Throwable)` sees both.
#[test]
fn error_branch_and_exception_branch_do_not_cross_catch() {
    let dir = make_test_dir("error_hierarchy_branches");
    let src = "<?php \
        try { throw new \\ArgumentCountError('a'); } catch (\\Exception $e) { echo \"BAD:ACE<-Exception\\n\"; } catch (\\Throwable $e) { echo \"ok:ACE\\n\"; } \
        try { throw new \\DivisionByZeroError('b'); } catch (\\RuntimeException $e) { echo \"BAD:DBZ<-RuntimeException\\n\"; } catch (\\Throwable $e) { echo \"ok:DBZ\\n\"; } \
        try { throw new \\RuntimeException('c'); } catch (\\Error $e) { echo \"BAD:RE<-Error\\n\"; } catch (\\Throwable $e) { echo \"ok:RE\\n\"; } \
        try { throw new \\InvalidArgumentException('d'); } catch (\\TypeError $e) { echo \"BAD:IAE<-TypeError\\n\"; } catch (\\Throwable $e) { echo \"ok:IAE\\n\"; }";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out, "ok:ACE\nok:DBZ\nok:RE\nok:IAE\n",
        "the Error and Exception branches of Throwable must not cross-catch"
    );
}

/// MULTI-CATCH PRECEDENCE: the FIRST matching clause wins, in source order.
///
/// All four clauses match an `ArgumentCountError`; PHP takes the first. Reordering them must
/// change which one fires, so the two halves together prove the runtime is not silently
/// picking the most specific (or the last) clause.
#[test]
fn multi_catch_selects_the_first_matching_clause_in_source_order() {
    let dir = make_test_dir("error_hierarchy_precedence");
    let src = "<?php \
        try { throw new \\ArgumentCountError('x'); } \
          catch (\\ArgumentCountError $e) { echo \"1:ArgumentCountError\\n\"; } \
          catch (\\TypeError $e) { echo \"1:TypeError\\n\"; } \
          catch (\\Error $e) { echo \"1:Error\\n\"; } \
          catch (\\Throwable $e) { echo \"1:Throwable\\n\"; } \
        try { throw new \\ArgumentCountError('x'); } \
          catch (\\Throwable $e) { echo \"2:Throwable\\n\"; } \
          catch (\\ArgumentCountError $e) { echo \"2:ArgumentCountError\\n\"; } \
        try { throw new \\ArgumentCountError('x'); } \
          catch (\\TypeError $e) { echo \"3:TypeError\\n\"; } \
          catch (\\ArgumentCountError $e) { echo \"3:ArgumentCountError\\n\"; } \
        try { throw new \\DivisionByZeroError('x'); } \
          catch (\\Error $e) { echo \"4:Error\\n\"; } \
          catch (\\ArithmeticError $e) { echo \"4:ArithmeticError\\n\"; }";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out, "1:ArgumentCountError\n2:Throwable\n3:TypeError\n4:Error\n",
        "multi-catch must select the first matching clause in source order, not the narrowest"
    );
}

/// A single `catch` with a UNION type must match every alternative.
#[test]
fn union_catch_matches_each_alternative() {
    let dir = make_test_dir("error_hierarchy_union");
    let src = "<?php \
        function probe(\\Throwable $t): void { \
            try { throw $t; } \
            catch (\\ValueError | \\ArgumentCountError $e) { echo 'union:', get_class($e), \"\\n\"; } \
            catch (\\Throwable $e) { echo 'other:', get_class($e), \"\\n\"; } \
        } \
        probe(new \\ValueError('a')); \
        probe(new \\ArgumentCountError('b')); \
        probe(new \\DivisionByZeroError('c'));";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out, "union:ValueError\nunion:ArgumentCountError\nother:DivisionByZeroError\n",
        "a union catch must match each alternative and nothing else"
    );
}

// ---------------------------------------------------------------------------
// 2 — instanceof and reflection through the hierarchy
// ---------------------------------------------------------------------------

/// `instanceof` must agree with catch matching through the whole chain.
///
/// Reference PHP 8.5.6 output, verbatim.
#[test]
fn instanceof_walks_the_error_hierarchy_like_php() {
    let dir = make_test_dir("error_hierarchy_instanceof");
    let src = "<?php \
        $a = new \\ArgumentCountError('i'); \
        var_dump($a instanceof \\ArgumentCountError, $a instanceof \\TypeError, $a instanceof \\Error, $a instanceof \\Throwable, $a instanceof \\ValueError, $a instanceof \\Exception); \
        $d = new \\DivisionByZeroError('i'); \
        var_dump($d instanceof \\DivisionByZeroError, $d instanceof \\ArithmeticError, $d instanceof \\Error, $d instanceof \\Throwable, $d instanceof \\TypeError); \
        $s = new \\AssertionError('i'); \
        var_dump($s instanceof \\AssertionError, $s instanceof \\Error, $s instanceof \\Throwable, $s instanceof \\ArithmeticError);";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\n\
         bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\n\
         bool(true)\nbool(true)\nbool(true)\nbool(false)\n",
        "instanceof must match reference PHP through the Error hierarchy"
    );
}

/// `class_exists`, `get_parent_class`, `class_parents`, `is_a`, and `is_subclass_of` must
/// report the reference hierarchy. `class_parents()` ordering is child-first, as in PHP.
#[test]
fn class_reflection_reports_the_reference_hierarchy() {
    let dir = make_test_dir("error_hierarchy_reflection");
    let src = "<?php \
        var_dump(class_exists('ArgumentCountError'), class_exists('DivisionByZeroError'), class_exists('AssertionError')); \
        $a = new \\ArgumentCountError('x'); \
        echo get_parent_class($a), \"\\n\"; \
        var_dump(is_a($a, 'TypeError'), is_a($a, 'Error'), is_a($a, 'Throwable'), is_subclass_of($a, 'TypeError'), is_subclass_of($a, 'ValueError')); \
        var_dump(class_parents('ArgumentCountError')); \
        var_dump(class_parents('DivisionByZeroError')); \
        var_dump(class_parents('AssertionError'));";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\n\
         TypeError\n\
         bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\n\
         array(2) {\n  [\"TypeError\"]=>\n  string(9) \"TypeError\"\n  [\"Error\"]=>\n  string(5) \"Error\"\n}\n\
         array(2) {\n  [\"ArithmeticError\"]=>\n  string(15) \"ArithmeticError\"\n  [\"Error\"]=>\n  string(5) \"Error\"\n}\n\
         array(1) {\n  [\"Error\"]=>\n  string(5) \"Error\"\n}\n",
        "class reflection must report reference PHP's Error hierarchy"
    );
}

// ---------------------------------------------------------------------------
// 3 — the inherited Throwable API
// ---------------------------------------------------------------------------

/// The new classes must REUSE `Exception`'s Throwable accessors rather than reimplement them.
///
/// `getMessage`/`getCode`/`getPrevious` are declared once on `Error` and inherited; this pins
/// the three-argument constructor round-trip, including a cross-class `$previous` chain.
/// `getFile()`/`getLine()` are exercised for shape only — see
/// `get_file_and_get_line_match_the_existing_exception_behavior` for why they are not
/// compared against reference values.
#[test]
fn throwable_accessors_round_trip_through_the_inherited_api() {
    let dir = make_test_dir("error_hierarchy_accessors");
    let src = "<?php \
        $prev = new \\ValueError('inner', 7); \
        $outer = new \\ArgumentCountError('outer', 42, $prev); \
        echo $outer->getMessage(), '|', $outer->getCode(), '|', get_class($outer), \"\\n\"; \
        echo get_class($outer->getPrevious()), '|', $outer->getPrevious()->getMessage(), '|', $outer->getPrevious()->getCode(), \"\\n\"; \
        var_dump($outer->getPrevious()->getPrevious()); \
        $bare = new \\DivisionByZeroError('only-message'); \
        echo $bare->getMessage(), '|', $bare->getCode(), \"\\n\"; \
        var_dump($bare->getPrevious()); \
        $empty = new \\AssertionError(); \
        var_dump($empty->getMessage(), $empty->getCode());";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "outer|42|ArgumentCountError\n\
         ValueError|inner|7\n\
         NULL\n\
         only-message|0\n\
         NULL\n\
         string(0) \"\"\nint(0)\n",
        "the new Error classes must inherit Exception's Throwable accessors unchanged"
    );
}

/// `getFile()` / `getLine()` must behave the SAME on a new Error class as on the existing
/// `Exception`, whatever that behavior is.
///
/// This is a PARITY-WITH-ELEPHC test, not a parity-with-PHP test. The requirement was to
/// reuse the existing Exception accessors, so the contract asserted here is that a
/// `DivisionByZeroError` and an `Exception` thrown from the same construct report the same
/// SHAPE — `getFile()` a string, `getLine()` an int, and the two classes agreeing. Pinning
/// the reference file path would make this test depend on the temp dir, and pinning
/// reference line numbers would freeze an accessor that elephc fills in from a different
/// mechanism than php-src's.
#[test]
fn get_file_and_get_line_match_the_existing_exception_behavior() {
    let dir = make_test_dir("error_hierarchy_file_line");
    let src = "<?php \
        $exc = new \\Exception('a'); \
        $err = new \\DivisionByZeroError('b'); \
        var_dump(is_string($exc->getFile()), is_string($err->getFile()), $exc->getFile() === $err->getFile()); \
        var_dump(is_int($exc->getLine()), is_int($err->getLine()));";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\n",
        "getFile()/getLine() on a new Error class must behave exactly as on Exception"
    );
}

// ---------------------------------------------------------------------------
// 4 — throwing from userland, across function boundaries and namespaces
// ---------------------------------------------------------------------------

/// A userland `throw` must unwind out of a nested call and still match by ancestor.
///
/// A throw raised in the SAME statement as its `try` can be lowered locally; crossing a
/// function boundary forces the real unwinder and the runtime class-descriptor walk.
#[test]
fn userland_throw_unwinds_across_a_function_boundary() {
    let dir = make_test_dir("error_hierarchy_unwind");
    let src = "<?php \
        function inner(string $which): void { \
            if ($which === 'ace') { throw new \\ArgumentCountError('from-inner'); } \
            if ($which === 'dbz') { throw new \\DivisionByZeroError('from-inner'); } \
            throw new \\AssertionError('from-inner'); \
        } \
        function middle(string $which): void { inner($which); echo \"UNREACHABLE\\n\"; } \
        foreach (['ace', 'dbz', 'other'] as $which) { \
            try { middle($which); } \
            catch (\\TypeError $e) { echo 'TypeError:', get_class($e), ':', $e->getMessage(), \"\\n\"; } \
            catch (\\ArithmeticError $e) { echo 'ArithmeticError:', get_class($e), ':', $e->getMessage(), \"\\n\"; } \
            catch (\\Error $e) { echo 'Error:', get_class($e), ':', $e->getMessage(), \"\\n\"; } \
        }";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "TypeError:ArgumentCountError:from-inner\n\
         ArithmeticError:DivisionByZeroError:from-inner\n\
         Error:AssertionError:from-inner\n",
        "a userland throw must unwind across frames and match by ancestor"
    );
}

/// Inside a namespace, the new classes must resolve fully-qualified and through `use`.
///
/// PHP resolves an UNQUALIFIED class name inside a namespace against that namespace, so
/// `\App\ArgumentCountError` is a different (nonexistent) name; the `use` import is what
/// binds the global one. Getting this wrong would make namespaced Symfony-shaped code either
/// fail to compile or catch nothing.
#[test]
fn namespaced_code_resolves_the_new_error_classes() {
    let dir = make_test_dir("error_hierarchy_namespace");
    let src = "<?php \
        namespace App; \
        use ArgumentCountError; \
        use DivisionByZeroError; \
        try { throw new \\ArgumentCountError('fq'); } catch (\\ArgumentCountError $e) { echo 'fq:', $e->getMessage(), \"\\n\"; } \
        try { throw new ArgumentCountError('imported'); } catch (ArgumentCountError $e) { echo 'imported:', get_class($e), ':', $e->getMessage(), \"\\n\"; } \
        try { throw new DivisionByZeroError('dbz'); } catch (\\ArithmeticError $e) { echo 'dbz:', get_class($e), \"\\n\"; }";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "fq:fq\nimported:ArgumentCountError:imported\ndbz:DivisionByZeroError\n",
        "namespaced code must resolve the new Error classes fully-qualified and via use"
    );
}

/// A `finally` block must still run when the unwinder carries one of the new classes.
#[test]
fn finally_runs_while_a_new_error_class_unwinds() {
    let dir = make_test_dir("error_hierarchy_finally");
    let src = "<?php \
        function probe(): string { \
            try { throw new \\ArgumentCountError('f'); } \
            catch (\\TypeError $e) { echo \"caught\\n\"; return 'from-catch'; } \
            finally { echo \"finally\\n\"; } \
        } \
        echo probe(), \"\\n\";";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out, "caught\nfinally\nfrom-catch\n",
        "finally must run while one of the new Error classes unwinds"
    );
}

/// A user class extending one of the new builtin Error classes must be catchable by every
/// builtin ancestor.
#[test]
fn user_subclass_of_a_new_error_class_is_catchable_by_its_builtin_ancestors() {
    let dir = make_test_dir("error_hierarchy_user_subclass");
    let src = "<?php \
        class AppArgError extends \\ArgumentCountError {} \
        try { throw new AppArgError('u1'); } catch (AppArgError $e) { echo \"own\\n\"; } \
        try { throw new AppArgError('u2'); } catch (\\ArgumentCountError $e) { echo \"ACE\\n\"; } \
        try { throw new AppArgError('u3'); } catch (\\TypeError $e) { echo \"TE\\n\"; } \
        try { throw new AppArgError('u4'); } catch (\\Error $e) { echo \"E\\n\"; } \
        try { throw new AppArgError('u5'); } catch (\\Throwable $e) { echo 'T:', get_class($e), ':', $e->getMessage(), \"\\n\"; } \
        try { throw new AppArgError('u6'); } catch (\\ValueError $e) { echo \"BAD\\n\"; } catch (\\Error $e) { echo \"ok-not-ValueError\\n\"; } \
        $u = new AppArgError('i'); \
        var_dump($u instanceof \\ArgumentCountError, $u instanceof \\TypeError, $u instanceof \\Error, $u instanceof \\ValueError);";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "own\nACE\nTE\nE\nT:AppArgError:u5\nok-not-ValueError\n\
         bool(true)\nbool(true)\nbool(true)\nbool(false)\n",
        "a user subclass must be catchable by every builtin ancestor of its parent"
    );
}

// ---------------------------------------------------------------------------
// 5 — uncaught shape
// ---------------------------------------------------------------------------

/// An UNCAUGHT builtin Error must produce the SAME fatal shape elephc already produces for
/// an uncaught `Exception`.
///
/// The unwinder now NAMES the class and its message — `Fatal error: Uncaught TypeError: boom` —
/// so each row asserts its own class rather than one generic string. That is what this test
/// originally wanted: its earlier revision asserted a shared `Fatal error: uncaught exception`
/// precisely because no class name was available, which meant a row could have thrown the WRONG
/// class and still passed. The `Exception` row stays as the control.
///
/// Reference PHP additionally appends ` in <file>:<line>` and a stack trace; elephc emits
/// everything up to that suffix, so these assert a prefix (see issue #660).
#[test]
fn uncaught_new_error_classes_use_the_existing_uncaught_fatal_shape() {
    for (stem, class) in [
        ("uncaught_exception", "Exception"),
        ("uncaught_type_error", "TypeError"),
        ("uncaught_ace", "ArgumentCountError"),
        ("uncaught_dbz", "DivisionByZeroError"),
        ("uncaught_assertion", "AssertionError"),
    ] {
        let dir = make_test_dir(&format!("error_hierarchy_{}", stem));
        let src = format!("<?php throw new \\{}('boom');", class);
        let out = run_binary_expecting_fatal(&compile(&dir, &src, "app"));
        assert!(
            out.contains(&format!("Fatal error: Uncaught {class}: boom")),
            "uncaught {class} must name its own class and message, got: {out:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 6 — a genuinely RAISED runtime error: intdiv() by zero
// ---------------------------------------------------------------------------

/// REGRESSION ANCHOR: `intdiv($a, 0)` must raise a CATCHABLE `DivisionByZeroError`.
///
/// This is the one place where elephc already had a runtime failure path whose PHP class is
/// unambiguous. It previously wrote a bare `Fatal error: division by zero` to stderr and
/// exited 1, so `catch (\DivisionByZeroError $e)` could never fire even once the class
/// existed. The message is php-src's own wording, verified against reference PHP 8.5.6.
///
/// The divisor comes from a PARAMETER so the zero is not const-folded away before codegen.
#[test]
fn intdiv_by_zero_raises_a_catchable_division_by_zero_error() {
    let dir = make_test_dir("error_hierarchy_intdiv");
    let src = "<?php \
        function own(int $a, int $b): void { \
            try { echo intdiv($a, $b), \"\\n\"; } \
            catch (\\DivisionByZeroError $e) { echo 'own:', get_class($e), ':', $e->getMessage(), \"\\n\"; } \
        } \
        function arith(int $a, int $b): void { \
            try { echo intdiv($a, $b), \"\\n\"; } \
            catch (\\ArithmeticError $e) { echo 'arith:', get_class($e), ':', $e->getMessage(), \"\\n\"; } \
        } \
        function any(int $a, int $b): void { \
            try { echo intdiv($a, $b), \"\\n\"; } \
            catch (\\Throwable $e) { echo 'any:', get_class($e), ':', $e->getMessage(), \"\\n\"; } \
        } \
        function wrong(int $a, int $b): void { \
            try { echo intdiv($a, $b), \"\\n\"; } \
            catch (\\TypeError $e) { echo \"BAD:TypeError\\n\"; } \
            catch (\\Error $e) { echo 'err:', get_class($e), \"\\n\"; } \
        } \
        own(7, 0); arith(7, 0); any(7, 0); wrong(7, 0); \
        own(7, 2); arith(9, 3);";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "own:DivisionByZeroError:Division by zero\n\
         arith:DivisionByZeroError:Division by zero\n\
         any:DivisionByZeroError:Division by zero\n\
         err:DivisionByZeroError\n\
         3\n3\n",
        "intdiv() by zero must raise PHP's catchable DivisionByZeroError, not a bare fatal"
    );
}

/// `intdiv(PHP_INT_MIN, -1)` keeps raising a plain `ArithmeticError`, NOT a
/// `DivisionByZeroError`.
///
/// Reference PHP distinguishes the two: the overflow case is the parent class. This pins that
/// the new subclass did not swallow the sibling path.
#[test]
fn intdiv_overflow_still_raises_the_parent_arithmetic_error() {
    let dir = make_test_dir("error_hierarchy_intdiv_overflow");
    let src = "<?php \
        function probe(int $a, int $b): void { \
            try { echo intdiv($a, $b), \"\\n\"; } \
            catch (\\DivisionByZeroError $e) { echo \"BAD:DivisionByZeroError\\n\"; } \
            catch (\\ArithmeticError $e) { echo 'overflow:', get_class($e), ':', $e->getMessage(), \"\\n\"; } \
        } \
        probe(PHP_INT_MIN, -1);";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "overflow:ArithmeticError:Division of PHP_INT_MIN by -1 is not an integer\n",
        "intdiv() overflow must stay an ArithmeticError, not become a DivisionByZeroError"
    );
}

// ---------------------------------------------------------------------------
// 7 — the compile-time-vs-runtime divergence, stated as a test
// ---------------------------------------------------------------------------

/// THE POINT OF THIS INCREMENT, and the divergence that comes with it.
///
/// FIRST HALF — the `catch` clause must COMPILE. `catch (\ArgumentCountError $e)` used to be
/// rejected at compile time with `Undefined class: ArgumentCountError`, so no program that
/// handled the error could be built at all. The arity-CORRECT call now compiles and runs.
///
/// SECOND HALF — elephc deliberately does NOT defer its static arity check. Reference PHP
/// raises `ArgumentCountError: opcache_reset() expects exactly 0 arguments, 1 given` at
/// RUNTIME; elephc is an AOT compiler and rejects `opcache_reset(1)` at COMPILE time. That is
/// a defensible divergence and is NOT a bug: the requirement was that the catch clause
/// compiles and that a genuinely thrown error is catchable, not that a statically provable
/// arity error becomes a runtime throw. This half fails loudly if someone "fixes" it by
/// weakening the static check.
#[test]
fn catch_clause_compiles_while_static_arity_check_still_rejects() {
    let dir = make_test_dir("error_hierarchy_divergence");

    let ok_src = "<?php \
        try { opcache_reset(); echo \"reset\\n\"; } \
        catch (\\ArgumentCountError $e) { echo 'caught: ', $e->getMessage(), \"\\n\"; }";
    let out = run_binary(&compile(&dir, ok_src, "ok"));
    assert_eq!(
        out, "reset\n",
        "a catch (\\ArgumentCountError) clause must compile and not interfere with the call"
    );

    let bad_src = "<?php \
        try { opcache_reset(1); } \
        catch (\\ArgumentCountError $e) { echo 'caught: ', $e->getMessage(), \"\\n\"; }";
    let diagnostics = compile_expecting_failure(&dir, bad_src, "bad");
    assert!(
        diagnostics.contains("expects 0 arguments, got 1"),
        "the AOT arity check must still reject opcache_reset(1) at COMPILE time, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.contains("Undefined class"),
        "the catch clause itself must no longer be an undefined-class error, got: {diagnostics:?}"
    );
}

// ---------------------------------------------------------------------------
// 8 — the classes are no longer seeded into every program
// ---------------------------------------------------------------------------

/// ArgumentCountError, AssertionError and UnhandledMatchError must survive being reached
/// through a name the compiler can only read at RUNTIME.
///
/// These three used to be written into every program's class metadata unconditionally, on the
/// theory that a runtime helper might materialize one with no EIR class reference to hang the
/// id off. None can: the authority is the class-id symbol table in
/// `codegen_support::runtime::data::user`, a helper stamps `[obj+0]` from a `_*_class_id`
/// symbol, and none of the three has one — elephc rejects a bad builtin arity at compile time,
/// `assert()` is unimplemented, and an unmatched `match` ends in a fatal terminator rather than
/// a throw. So the seeding went, and the classes now ride in only when something names them.
///
/// WHICH MAKES `new $cls` THE INTERESTING ROW, and the reason this test exists rather than
/// leaning on the by-name coverage above. The same shape is what the Reflection gate got wrong
/// once already: a class reached through a name that is a VALUE is invisible to a scan looking
/// for the name as syntax. `new $cls` is covered by a different mechanism
/// (`referenced_dynamic_object_new_class_names`), and this pins that it still is.
///
/// A miss is not silent — the catch walk meets a `-2` parent id and aborts — but it aborts in
/// whatever program tripped it, which is a worse place to find out than here.
#[test]
fn deseeded_error_classes_survive_construction_through_a_runtime_name() {
    let dir = make_test_dir("error_hierarchy_deseeded");
    let src = "<?php \
        foreach (['ArgumentCountError', 'AssertionError', 'UnhandledMatchError'] as $cls) { \
            $e = new $cls('dyn'); \
            echo get_class($e), '|', get_parent_class($e), \"\\n\"; \
        }";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "ArgumentCountError|TypeError\n\
         AssertionError|Error\n\
         UnhandledMatchError|Error\n",
        "a de-seeded builtin Error class must still be constructible through a runtime name"
    );
}
