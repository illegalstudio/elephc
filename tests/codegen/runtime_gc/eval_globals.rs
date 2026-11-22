//! Purpose:
//! Integration tests for global symbol-table access across compiled and evaluated scopes:
//! `$GLOBALS[...]` reads and writes, the `global` keyword's runtime binding and detaching,
//! CLI `argv`/`argc`, and closures bound in runtime-evaluated files that share real globals.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (runtime GC integration suite).
//!
//! Key details:
//! - Bound closures must share real globals without capturing local names.
//! - `global $x` is a runtime binding in php: it can be conditional, it rebinds an existing
//!   local of the same name, and `unset()` on either side detaches only that name.
//! - Expected output matches php for the same fixture; CLI binaries run with no arguments, so
//!   `argv` has one entry.

use crate::support::*;

/// Verifies `$alias =& $value` on a by-reference `int &$value` parameter writes through to the
/// caller's variable (prints "11") and leaves the heap-debug leak summary clean, so the local
/// alias neither steals nor leaks the reference owner.
#[test]
fn test_local_alias_of_reference_parameter_preserves_owner() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function mutateAliasValue(int &$value): void {
    $alias =& $value;
    $alias += 1;
}
$subject = 10;
mutateAliasValue($subject);
echo $subject;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "11");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies `eval()` inside a function writes the function's local `$shared`, while
/// `$GLOBALS["shared"]` in the same eval still reads the global. PHP outputs "seed:eval:seed".
#[test]
fn test_eval_literal_global_reads_do_not_alias_local_variables() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
$GLOBALS['shared'] = 'seed';
function localEval(): void {
    $shared = 'local';
    eval('$shared = "eval"; echo $GLOBALS["shared"];');
    echo ':', $shared;
}
localEval();
echo ':', $GLOBALS['shared'];
"#)],
        "main.php",
    );
    assert_eq!(out, "seed:eval:seed");
}

/// Verifies a `global $shared` alias keeps the original value after `unset($GLOBALS['shared'])`
/// and recreation of that name: the local still reads "old", and writing it no longer reaches the
/// recreated global. PHP outputs "old:new".
#[test]
fn test_global_keyword_alias_survives_global_name_unset_and_recreation() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
$GLOBALS['shared'] = 'old';
function detachGlobalName(): void {
    global $shared;
    unset($GLOBALS['shared']);
    $GLOBALS['shared'] = 'new';
    echo $shared, ':';
    $shared = 'detached';
    echo $GLOBALS['shared'];
}
detachGlobalName();
"#)],
        "main.php",
    );
    assert_eq!(out, "old:new");
}

/// Verifies a `global $shared` executed only under `if ($bind)` is honoured by a later `eval()`
/// in the same function: unbound, the eval writes the local; bound, it writes the global. PHP
/// outputs "seed:eval:seed|eval:eval:eval".
#[test]
fn test_global_keyword_conditional_binding_remains_live_inside_eval() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
$GLOBALS['shared'] = 'seed';
function chooseEval(bool $bind): void {
    $shared = 'local';
    if ($bind) { global $shared; }
    eval('$shared = "eval"; echo $GLOBALS["shared"], ":";');
    echo $shared, ':';
}
chooseEval(false);
echo $GLOBALS['shared'], '|';
chooseEval(true);
echo $GLOBALS['shared'];
"#)],
        "main.php",
    );
    assert_eq!(out, "seed:eval:seed|eval:eval:eval");
}

/// Verifies a conditional `global $shared` in compiled code is decided at run time: unbound, the
/// function reads and writes its local; bound, it reads "seed" and its write reaches the global.
/// PHP outputs "local:seed|seed:changed".
#[test]
fn test_global_keyword_conditional_binding_is_runtime_state() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
$GLOBALS['shared'] = 'seed';
function choose(bool $bind): void {
    $shared = 'local';
    if ($bind) { global $shared; }
    echo $shared, ':';
    $shared = 'changed';
}
choose(false);
echo $GLOBALS['shared'], '|';
choose(true);
echo $GLOBALS['shared'];
"#)],
        "main.php",
    );
    assert_eq!(out, "local:seed|seed:changed");
}

/// Verifies `global $shared` rebinding a local leaves an earlier `$before =& $shared` reference
/// on the old local value, and `unset($shared)` detaches only the local name so a new local
/// write does not touch the global. PHP outputs "local:global:global:new-local".
#[test]
fn test_global_keyword_preserves_prior_alias_and_unset_detaches_local() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
$GLOBALS['shared'] = 'seed';
function detach(): void {
    $shared = 'local';
    $before =& $shared;
    global $shared;
    $shared = 'global';
    echo $before, ':', $GLOBALS['shared'], ':';
    unset($shared);
    $shared = 'new-local';
    echo $GLOBALS['shared'], ':', $shared;
}
detach();
"#)],
        "main.php",
    );
    assert_eq!(out, "local:global:global:new-local");
}

/// Verifies a function can write a string through `global $shared` into a global whose initial
/// top-level assignment was an int; the compiler must not pin the global to `int`. PHP outputs
/// "changed".
#[test]
fn test_global_keyword_allows_replacing_a_known_initial_type() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
$shared = 1;
function replaceGlobal(): void {
    global $shared;
    $shared = 'changed';
}
replaceGlobal();
echo $GLOBALS['shared'];
"#)],
        "main.php",
    );
    assert_eq!(out, "changed");
}

/// Verifies `global $shared` after a local int `$shared = 42` replaces the local binding: the
/// read yields the global "seed", and a later array write lands in the global. PHP outputs
/// "seed:done".
#[test]
fn test_global_keyword_rebinds_a_same_named_local() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
$GLOBALS['shared'] = 'seed';
function replaceGlobal(): void {
    $shared = 42;
    global $shared;
    echo $shared;
    $shared = ['done'];
}
replaceGlobal();
echo ':', $GLOBALS['shared'][0];
"#)],
        "main.php",
    );
    assert_eq!(out, "seed:done");
}

/// Verifies `$GLOBALS['argv']` and `$GLOBALS['argc']` are populated from the process
/// arguments in a CLI build: with no arguments both describe the script alone, "1:1".
#[test]
fn test_cli_globals_argv_and_argc_match_process_arguments() {
    let out = compile_cli_files_and_run(
        &[("main.php", "<?php echo count($GLOBALS['argv']), ':', $GLOBALS['argc'];")],
        "main.php",
    );
    assert_eq!(out, "1:1");
}

/// Verifies `eval()` can replace `argc` with a string and `argv` with an int through `$GLOBALS`,
/// and compiled code then reads the new values instead of its typed process-argument view.
/// PHP outputs "1:1:changed:42".
#[test]
fn test_cli_globals_process_arguments_allow_type_changes_across_eval() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
echo count($GLOBALS['argv']), ':', $GLOBALS['argc'];
eval('$GLOBALS["argc"] = "changed"; $GLOBALS["argv"] = 42;');
echo ':', $GLOBALS['argc'], ':', $GLOBALS['argv'];
"#)],
        "main.php",
    );
    assert_eq!(out, "1:1:changed:42");
}

/// Verifies a scalar global written by `eval()` is visible to compiled code afterwards, and an
/// `unset($GLOBALS[...])` in a second eval removes it for compiled `isset()`. PHP outputs
/// "7:absent".
#[test]
fn test_eval_globals_explicit_barrier_roundtrips_scalar() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
$GLOBALS['scalar'] = 3;
eval('$GLOBALS["scalar"] = 7;');
echo $GLOBALS['scalar'];
eval('unset($GLOBALS["scalar"]);');
echo ':', isset($GLOBALS['scalar']) ? 'present' : 'absent';
"#)],
        "main.php",
    );
    assert_eq!(out, "7:absent");
}

/// Verifies global state is synchronized at each crossing when eval calls a compiled function:
/// the native `global $counter` sees eval's 7, its write of 9 is seen by the rest of the eval,
/// and the outer compiled code also sees 9. PHP outputs "native:7:eval:9:outer:9".
#[test]
fn test_eval_globals_nested_native_call_observes_each_boundary() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
function nativeStep() {
    global $counter;
    echo 'native:', $counter, ':';
    $counter = 9;
}
$GLOBALS['counter'] = 1;
eval('$GLOBALS["counter"] = 7; nativeStep(); echo "eval:", $GLOBALS["counter"], ":";');
echo 'outer:', $GLOBALS['counter'];
"#)],
        "main.php",
    );
    assert_eq!(out, "native:7:eval:9:outer:9");
}

/// Verifies a `Closure::bind`-ed static loader defined in a runtime-evaluated file guards
/// `require` with `$GLOBALS['loaded_units'][$key]`, sharing the array the compiled main script
/// reads and unsets: the unit runs once, is skipped on the repeat, and runs again after the
/// compiled `unset()`. PHP outputs "1:seen:2".
#[test]
fn test_eval_bound_closure_global_once_guard_shares_compiled_storage() {
    let out = run_once_guard(r#"<?php
function makeUnitLoader() {
    return Closure::bind(static function ($key, $file) {
        if (empty($GLOBALS['loaded_units'][$key])) {
            $GLOBALS['loaded_units'][$key] = true;
            require $file;
        }
    }, null, null);
}
"#, "<?php $GLOBALS['unit_runs'] += 1;");
    assert_eq!(out, "1:seen:2");
}

/// Control for `test_eval_bound_closure_global_once_guard_shares_compiled_storage`: the same
/// once-guard spelled with `global $loaded_units` / `global $unit_runs` instead of `$GLOBALS[...]`,
/// so a failure in only one of the two isolates the access spelling. PHP outputs "1:seen:2".
#[test]
fn test_eval_bound_closure_global_keyword_control_shares_compiled_storage() {
    let out = run_once_guard(r#"<?php
function makeUnitLoader() {
    return Closure::bind(static function ($key, $file) {
        global $loaded_units;
        if (empty($loaded_units[$key])) {
            $loaded_units[$key] = true;
            require $file;
        }
    }, null, null);
}
"#, "<?php global $unit_runs; $unit_runs += 1;");
    assert_eq!(out, "1:seen:2");
}

/// Builds and runs the once-guard fixture: `main.php` seeds the globals, includes `factory.php`
/// through a variable path (so it runs in the runtime evaluator), calls the returned loader
/// twice for `unit.php`, unsets the guard key, calls it again, and prints
/// `unit_runs:seen|missing:unit_runs`. Returns the program's stdout.
fn run_once_guard(factory: &str, unit: &str) -> String {
    compile_cli_files_and_run(
        &[
            ("main.php", r#"<?php
$GLOBALS['loaded_units'] = [];
$GLOBALS['unit_runs'] = 0;
$factoryFile = 'factory';
include __DIR__ . '/' . $factoryFile . '.php';
$loadUnit = makeUnitLoader();
$loadUnit('one', __DIR__ . '/unit.php');
$loadUnit('one', __DIR__ . '/unit.php');
echo $GLOBALS['unit_runs'], ':', isset($GLOBALS['loaded_units']['one']) ? 'seen' : 'missing';
unset($GLOBALS['loaded_units']['one']);
$loadUnit('one', __DIR__ . '/unit.php');
echo ':', $GLOBALS['unit_runs'];
"#),
            ("factory.php", factory),
            ("unit.php", unit),
        ],
        "main.php",
    )
}

/// A function declared by `eval()` inside a helper function retains its context and the
/// program-global scope that owns `$argv`/`$argc`. The CLI request must finalize that context at
/// exit even though `main` itself holds no eval context, so the heap stays clean.
#[test]
fn test_eval_function_declaration_in_helper_releases_retained_context_at_exit() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function declareInHelper(bool $run): void {
    $code = $run ? 'function retainedByEval($v) { return $v + 1; }' : '';
    eval($code);
}
declareInHelper($argc > 0);
echo 'ok';
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ok");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A literal eval fragment that writes a string global bound by `global $g` hands its result back
/// after the native-binding clear, which reuses the eval temp cell: the result used to be read
/// from that cell, so the clear's value view was released as the result and the global's string
/// was freed under its owner.
#[test]
fn test_literal_eval_string_write_to_function_global_survives_the_binding_clear() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
$shared = 'seed';
function writeThroughEval(): void {
    global $shared;
    eval('$shared = "eval";');
    echo $shared, ':';
}
writeThroughEval();
echo $shared;
"#)],
        "main.php",
    );
    assert_eq!(out, "eval:eval");
}
