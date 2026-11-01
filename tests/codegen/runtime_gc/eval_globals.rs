//! Purpose: verify global symbol-table access across compiled and evaluated scopes.
//! Called from: the runtime GC integration suite.
//! Key details: bound closures must share real globals without capturing local names.

use crate::support::*;

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

#[test]
fn test_cli_globals_argv_and_argc_match_process_arguments() {
    let out = compile_cli_files_and_run(
        &[("main.php", "<?php echo count($GLOBALS['argv']), ':', $GLOBALS['argc'];")],
        "main.php",
    );
    assert_eq!(out, "1:1");
}

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
