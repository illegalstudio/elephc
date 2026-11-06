//! Purpose:
//! End-to-end tests for a wrong argument count inside `eval()`: PHP throws a catchable
//! `ArgumentCountError`, where eval used to stop the program with
//! `Fatal error: eval() runtime failed`.
//!
//! Called from:
//! - `cargo test --test eval_argument_count_tests` through Rust's test harness.
//!
//! Key details:
//! - Every expected line is PINNED FROM REFERENCE PHP 8.5.10, running the same source through
//!   `eval(getenv('REVIEW_CODE'))`, except the user-callable rows, whose PHP text also says
//!   where the call was made (`0 passed in <file>(2) : eval()'d code on line 1 and ...`). Eval
//!   tracks no line for a running statement, so it prints the form PHP uses when no userland
//!   frame made the call (`0 passed and ...`, as for an `array_map` callback). Those rows pin
//!   that form.
//! - The eval'd code comes from an environment variable, so the compiler never sees it: nothing
//!   is folded or checked ahead of time, and every row runs through the eval interpreter.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   the same harness style as `opcache_restrict_api_tests`. Host target only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// A program whose only behaviour is the opaque fragment it evaluates.
const EVAL_ONLY: &str = "<?php\n$code = getenv('REVIEW_CODE');\neval($code);\n";

/// Creates an isolated temp dir unique across parallel test threads and processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
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

/// Compiles `source` as `main.php` in a fresh dir under the PHP 8.5 profile and returns the
/// executable. `extra` carries further CLI arguments such as `--ini` assignments.
fn compile(prefix: &str, source: &str, extra: &[&str]) -> PathBuf {
    let dir = make_test_dir(prefix);
    fs::write(dir.join("main.php"), source).unwrap();
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg(dir.join("main.php"))
        .args(["--php-version", "8.5"])
        .args(extra)
        .output()
        .expect("failed to spawn elephc");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    dir.join("main")
}

/// Runs `binary` with `code` as the eval'd fragment.
fn run(binary: &Path, code: &str) -> Output {
    Command::new(binary)
        .env("REVIEW_CODE", code)
        .output()
        .expect("failed to run compiled binary")
}

/// Runs `code` and asserts successful completion and exact stdout, showing stderr on a mismatch.
fn assert_stdout(binary: &Path, code: &str, expected: &str) {
    let output = run(binary, code);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        expected,
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Wraps each call in a `try` that prints the caught class and message, one line per call.
fn catching(calls: &[&str]) -> String {
    calls
        .iter()
        .map(|call| {
            format!(
                "try {{ {call}; echo \"no error\\n\"; }} catch (TypeError $e) {{ \
                 echo get_class($e), ': ', $e->getMessage(), \"\\n\"; }}\n"
            )
        })
        .collect()
}

/// A direct builtin call with a count PHP refuses throws `ArgumentCountError` (a `TypeError`)
/// with PHP's exact wording, whichever specialised handler owns the name: `str_pad` (optional
/// parameters), `max`/`sprintf` (variadics), `strtotime`/`microtime`/`tmpfile` (contracts with an
/// AOT `arity_error` override, which eval must not print), `mt_rand`/`strtr` (PHP's own
/// "exactly" special cases), `sort` (by-reference), and two OPcache handlers that no registry
/// signature describes.
#[test]
fn direct_builtin_calls_with_a_refused_count_throw_php_argument_count_errors() {
    let binary = compile("eval_arity_direct", EVAL_ONLY, &[]);
    let code = catching(&[
        "strlen()",
        "str_repeat('a')",
        "strlen('a', 'b')",
        "array_sum()",
        "str_pad('a')",
        "max()",
        "sprintf()",
        "strtotime()",
        "microtime(1, 2)",
        "tmpfile(1)",
        "mt_rand(1)",
        "strtr('a')",
        "implode()",
        "sort()",
        "STRLEN()",
        "opcache_is_script_cached()",
        "opcache_get_status(1, 2)",
    ]);
    assert_stdout(
        &binary,
        &code,
        "ArgumentCountError: strlen() expects exactly 1 argument, 0 given\n\
         ArgumentCountError: str_repeat() expects exactly 2 arguments, 1 given\n\
         ArgumentCountError: strlen() expects exactly 1 argument, 2 given\n\
         ArgumentCountError: array_sum() expects exactly 1 argument, 0 given\n\
         ArgumentCountError: str_pad() expects at least 2 arguments, 1 given\n\
         ArgumentCountError: max() expects at least 1 argument, 0 given\n\
         ArgumentCountError: sprintf() expects at least 1 argument, 0 given\n\
         ArgumentCountError: strtotime() expects at least 1 argument, 0 given\n\
         ArgumentCountError: microtime() expects at most 1 argument, 2 given\n\
         ArgumentCountError: tmpfile() expects exactly 0 arguments, 1 given\n\
         ArgumentCountError: mt_rand() expects exactly 2 arguments, 1 given\n\
         ArgumentCountError: strtr() expects exactly 2 arguments, 1 given\n\
         ArgumentCountError: implode() expects at least 1 argument, 0 given\n\
         ArgumentCountError: sort() expects at least 1 argument, 0 given\n\
         ArgumentCountError: strlen() expects exactly 1 argument, 0 given\n\
         ArgumentCountError: opcache_is_script_cached() expects exactly 1 argument, 0 given\n\
         ArgumentCountError: opcache_get_status() expects at most 1 argument, 2 given\n",
    );
}

/// The evaluated-argument spellings reach the same error: `call_user_func`,
/// `call_user_func_array`, a variable call, a spread, and a builtin used as a callback.
#[test]
fn evaluated_builtin_calls_with_a_refused_count_throw_the_same_error() {
    let binary = compile("eval_arity_values", EVAL_ONLY, &[]);
    let code = catching(&[
        "call_user_func('strlen')",
        "call_user_func_array('strlen', [])",
        "call_user_func_array('str_repeat', ['a', 2, 3])",
        "$f = 'strlen'; $f()",
        "strlen(...[])",
        "array_map('strlen', ['a'], ['b'])",
    ]);
    assert_stdout(
        &binary,
        &code,
        "ArgumentCountError: strlen() expects exactly 1 argument, 0 given\n\
         ArgumentCountError: strlen() expects exactly 1 argument, 0 given\n\
         ArgumentCountError: str_repeat() expects exactly 2 arguments, 3 given\n\
         ArgumentCountError: strlen() expects exactly 1 argument, 0 given\n\
         ArgumentCountError: strlen() expects exactly 1 argument, 0 given\n\
         ArgumentCountError: strlen() expects exactly 1 argument, 2 given\n",
    );
}

/// PHP sends every argument before the callee refuses the count, so side effects happen in
/// source order first. A plain variable in a by-reference slot is bound, not read: no
/// `Undefined variable` warning, and the variable is still unset afterwards.
#[test]
fn a_refused_builtin_call_still_evaluates_its_arguments_in_order() {
    let binary = compile("eval_arity_order", EVAL_ONLY, &[]);
    let code = "function g() { echo \"g\"; return 1; }\n\
                function h() { echo \"h\"; return 2; }\n\
                try { strlen(g(), h()); } catch (ArgumentCountError $e) { echo '|', $e->getMessage(), \"\\n\"; }\n\
                try { sort($unset, 1, 2); } catch (ArgumentCountError $e) { echo $e->getMessage(), \"\\n\"; }\n\
                var_dump(isset($unset));\n";
    let output = run(&binary, code);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "gh|strlen() expects exactly 1 argument, 2 given\n\
         sort() expects at most 2 arguments, 3 given\n\
         bool(false)\n",
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("Undefined variable"), "{stderr}");
}

/// Counts PHP accepts are never refused, even where the eval signature states fewer
/// parameters: `mktime` marks all six required and `rand` both, yet PHP takes fewer.
#[test]
fn counts_php_accepts_are_not_refused() {
    let binary = compile("eval_arity_accepted", EVAL_ONLY, &[]);
    assert_stdout(
        &binary,
        "echo mktime(0, 0, 0) > 0 ? 'y' : 'n', is_int(rand()) ? 'i' : 'x', mt_rand(3, 3), \
         strtr('ab', 'a', 'x'), \"\\n\";",
        "yi3xb\n",
    );
}

/// An uncaught refusal ends the program as PHP's uncaught `ArgumentCountError`, not as
/// `eval() runtime failed`, and nothing after it runs.
#[test]
fn an_uncaught_refusal_is_reported_as_an_uncaught_argument_count_error() {
    let binary = compile("eval_arity_uncaught", EVAL_ONLY, &[]);
    let output = run(&binary, "strlen(); echo 'after';");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        all.contains("Uncaught ArgumentCountError: strlen() expects exactly 1 argument, 0 given"),
        "{all}"
    );
    assert!(!all.contains("eval() runtime failed"), "{all}");
    assert!(!all.contains("after"), "{all}");
}

/// Eval-declared functions, methods and closures name themselves and the counts, like PHP.
/// The `in <file> on line <n>` clause is absent (see the module docs); the named-argument form
/// carries no location in PHP either, so it matches exactly.
#[test]
fn user_callables_missing_an_argument_report_php_messages() {
    let binary = compile("eval_arity_user", EVAL_ONLY, &[]);
    let code = format!(
        "function f($a) {{}}\n\
         function k($a, $b, ...$c) {{}}\n\
         function f2($a, $b) {{}}\n\
         class A {{ function m($a, $b = 1) {{}} static function s($a) {{}} }}\n\
         class B extends A {{}}\n\
         $c = function ($a, $b) {{}};\n\
         {}",
        catching(&[
            "f()",
            "k(1)",
            "f2(b: 1)",
            "(new A)->m()",
            "(new B)->m()",
            "A::s()",
            "$c(1)",
            "call_user_func('f')",
            "array_map('f2', [1])",
        ])
    );
    assert_stdout(
        &binary,
        &code,
        "ArgumentCountError: Too few arguments to function f(), 0 passed and exactly 1 expected\n\
         ArgumentCountError: Too few arguments to function k(), 1 passed and exactly 2 expected\n\
         ArgumentCountError: f2(): Argument #1 ($a) not passed\n\
         ArgumentCountError: Too few arguments to function A::m(), 0 passed and at least 1 expected\n\
         ArgumentCountError: Too few arguments to function A::m(), 0 passed and at least 1 expected\n\
         ArgumentCountError: Too few arguments to function A::s(), 0 passed and exactly 1 expected\n\
         ArgumentCountError: Too few arguments to function {closure:MAIN(3) : eval()'d code:6}(), 1 passed and exactly 2 expected\n\
         ArgumentCountError: Too few arguments to function f(), 0 passed and exactly 1 expected\n\
         ArgumentCountError: Too few arguments to function f2(), 1 passed and exactly 2 expected\n"
            .replace("MAIN", &binary.with_extension("php").to_string_lossy())
            .as_str(),
    );
}

/// A compiled function called from eval with too few arguments throws too, positionally and by
/// name. Its name is the lowercase key the program registers it under.
#[test]
fn compiled_functions_called_from_eval_throw_argument_count_errors() {
    let source = "<?php\nfunction nat($a, $b = 2) { return $a + $b; }\necho nat(1), \"\\n\";\n\
                  $code = getenv('REVIEW_CODE');\neval($code);\n";
    let binary = compile("eval_arity_native", source, &[]);
    let code = catching(&["nat()", "nat(b: 3)", "call_user_func('nat')"]);
    assert_stdout(
        &binary,
        &format!("{code}echo nat(1, 1), nat(a: 5), \"\\n\";"),
        "3\n\
         ArgumentCountError: Too few arguments to function nat(), 0 passed and at least 1 expected\n\
         ArgumentCountError: nat(): Argument #1 ($a) not passed\n\
         ArgumentCountError: Too few arguments to function nat(), 0 passed and at least 1 expected\n\
         27\n",
    );
}

/// The refused OPcache counts, every caught `ArgumentCountError` pinned from PHP 8.5.10.
const OPCACHE_COUNT_REFUSALS: &str =
    "ArgumentCountError: opcache_reset() expects exactly 0 arguments, 1 given\n\
     ArgumentCountError: opcache_reset() expects exactly 0 arguments, 1 given\n\
     ArgumentCountError: opcache_reset() expects exactly 0 arguments, 1 given\n\
     ArgumentCountError: opcache_is_script_cached() expects exactly 1 argument, 0 given\n\
     ArgumentCountError: opcache_is_script_cached() expects exactly 1 argument, 0 given\n\
     ArgumentCountError: opcache_invalidate() expects at least 1 argument, 0 given\n";

/// Builds the eval'd fragment that calls the OPcache names with counts PHP refuses, directly
/// and through `call_user_func()` / `call_user_func_array()`.
fn opcache_count_refusal_calls() -> String {
    catching(&[
        "opcache_reset(1)",
        "call_user_func('opcache_reset', 1)",
        "call_user_func_array('opcache_reset', [1])",
        "opcache_is_script_cached()",
        "call_user_func_array('opcache_is_script_cached', [])",
        "opcache_invalidate()",
    ])
}

/// The OPcache names with the prelude compiled in. The prelude is injected only when the
/// program names a function, and then eval resolves that name to a native function whose
/// userland binding would accept surplus arguments; PHP's internal-function count still wins on
/// every spelling. `$argc` keeps the referencing branch alive but never taken.
///
/// The prelude-free control is a separate test: each binary is a full eval-linked compile.
#[test]
fn opcache_functions_refuse_counts_like_php() {
    let with_prelude = "<?php\nif ($argc > 99) { opcache_reset(); \
                        var_dump(opcache_is_script_cached(__FILE__), opcache_invalidate(__FILE__)); }\n\
                        $code = getenv('REVIEW_CODE');\neval($code);\n";
    let binary = compile("eval_arity_opcache", with_prelude, &["--ini", "opcache.enable_cli=1"]);
    assert_stdout(&binary, &opcache_count_refusal_calls(), OPCACHE_COUNT_REFUSALS);
}

/// The same refused counts without the prelude, where eval answers from its own fallback.
#[test]
fn opcache_functions_refuse_counts_like_php_in_the_fallback() {
    let fallback = compile("eval_arity_opcache_fallback", EVAL_ONLY, &[]);
    assert_stdout(&fallback, &opcache_count_refusal_calls(), OPCACHE_COUNT_REFUSALS);
}

/// Spread and named arguments to the OPcache names bind like PHP's internal functions, with the
/// binary's OPcache configuration either way: the CLI default, where eval answers from its own
/// fallback, and `opcache.enable_cli=1`, where an opaque `eval()` reaches the native
/// declarations.
///
/// PR #968 review: the direct fallback took a spread as ONE argument, so `opcache_reset(...[1])`
/// and `opcache_invalidate(...[])` were refused with a fatal and `opcache_get_status(...[true,
/// false])` was accepted. And the native declaration is a userland function to the binder,
/// which accepted surplus spread arguments, printed userland's `Too few arguments` text, and
/// turned an unknown or overwriting name into a fatal. Every row is MEASURED on PHP 8.5.10.
#[test]
fn opcache_spread_and_named_arguments_bind_like_php() {
    let fallback = compile("eval_arity_opcache_spread_fallback", EVAL_ONLY, &[]);
    let native = compile(
        "eval_arity_opcache_spread_native",
        EVAL_ONLY,
        &["--ini", "opcache.enable_cli=1"],
    );
    let code: String = [
        "opcache_reset(...[1])",
        "opcache_get_status(...[true, false])",
        "opcache_invalidate(...[])",
        "opcache_is_script_cached(...['/a', '/b'])",
        "call_user_func('opcache_reset', ...[1])",
        "opcache_invalidate(force: true)",
        "opcache_get_status(foo: 1)",
        "opcache_invalidate('/a', filename: '/b')",
        "var_dump(opcache_invalidate(...['filename' => '/nope']))",
    ]
    .iter()
    .map(|call| {
        format!(
            "try {{ {call}; }} catch (Error $e) {{ \
             echo get_class($e), ': ', $e->getMessage(), \"\\n\"; }}\n"
        )
    })
    .collect();
    let expected = "ArgumentCountError: opcache_reset() expects exactly 0 arguments, 1 given\n\
                    ArgumentCountError: opcache_get_status() expects at most 1 argument, 2 given\n\
                    ArgumentCountError: opcache_invalidate() expects at least 1 argument, 0 given\n\
                    ArgumentCountError: opcache_is_script_cached() expects exactly 1 argument, 2 given\n\
                    ArgumentCountError: opcache_reset() expects exactly 0 arguments, 1 given\n\
                    ArgumentCountError: opcache_invalidate(): Argument #1 ($filename) not passed\n\
                    Error: Unknown named parameter $foo\n\
                    Error: Named parameter $filename overwrites previous argument\n\
                    bool(false)\n";
    assert_stdout(&fallback, &code, expected);
    assert_stdout(&native, &code, expected);
    // A named argument reaches the parameter it names: `include_scripts: false` is the status
    // array without its `scripts` key in the live binary, and `false` in the disabled one.
    let named = "$s = opcache_get_status(include_scripts: false); \
                 echo is_array($s) ? (isset($s['scripts']) ? 'scripts' : 'no scripts') : 'false', \"\\n\";";
    assert_stdout(&fallback, named, "false\n");
    assert_stdout(&native, named, "no scripts\n");
}

/// Asserts that named OPcache callable arguments bind like direct calls in `binary`: the
/// variable, `call_user_func()`, `call_user_func_array()` and first-class routes, their caught
/// errors, and left-to-right evaluation of the named arguments.
fn assert_opcache_named_callable_binding(binary: &Path) {
    let code: String = [
        "$f = 'opcache_invalidate'; var_dump($f(filename: '/missing', force: true))",
        "var_dump(call_user_func_array('opcache_invalidate', ['force' => true, 'filename' => '/missing']))",
        "var_dump(call_user_func('opcache_invalidate', filename: '/missing', force: true))",
        "var_dump(call_user_func(force: true, filename: '/missing', callback: 'opcache_invalidate'))",
        "$f = opcache_invalidate(...); var_dump($f(...['filename' => '/missing', 'force' => true]))",
        "$f = 'opcache_get_status'; $f(foo: 1)",
        "call_user_func_array('opcache_get_status', ['foo' => 1])",
        "call_user_func('opcache_get_status', foo: 1)",
        "$f = 'opcache_invalidate'; $f(force: true)",
        "call_user_func_array('opcache_invalidate', ['force' => true])",
        "$f = 'opcache_invalidate'; $f('/a', filename: '/b')",
        "call_user_func_array('opcache_invalidate', [0 => '/a', 'filename' => '/b'])",
        "call_user_func('opcache_invalidate', callback: 'opcache_reset')",
        "call_user_func(filename: '/missing')",
    ]
    .iter()
    .map(|call| {
        format!(
            "try {{ {call}; }} catch (Error $e) {{ \
             echo get_class($e), ': ', $e->getMessage(), \"\\n\"; }}\n"
        )
    })
    .collect();
    let expected = "bool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\n\
                    Error: Unknown named parameter $foo\n\
                    Error: Unknown named parameter $foo\n\
                    Error: Unknown named parameter $foo\n\
                    ArgumentCountError: opcache_invalidate(): Argument #1 ($filename) not passed\n\
                    ArgumentCountError: opcache_invalidate(): Argument #1 ($filename) not passed\n\
                    Error: Named parameter $filename overwrites previous argument\n\
                    Error: Named parameter $filename overwrites previous argument\n\
                    Error: Named parameter $callback overwrites previous argument\n\
                    ArgumentCountError: call_user_func() expects at least 1 argument, 0 given\n";
    assert_stdout(binary, &code, expected);
    let ordered = "function mark($label) { echo $label; return '/missing'; } \
                   $f = 'opcache_invalidate'; \
                   var_dump($f(force: mark('force '), filename: mark('filename ')));";
    assert_stdout(binary, ordered, "force filename bool(false)\n");
}

/// Named OPcache callable arguments use the same internal binding as direct calls when eval
/// answers from its own fallback (the CLI-default binary), including caught errors.
///
/// The native-dispatch case is a separate test: each binary is a full eval-linked compile.
#[test]
fn opcache_named_callable_arguments_use_internal_binding() {
    let fallback = compile("eval_opcache_named_fallback", EVAL_ONLY, &[]);
    assert_opcache_named_callable_binding(&fallback);
}

/// The same binding when `opcache.enable_cli=1` makes the opaque source reach the native
/// declarations.
#[test]
fn opcache_named_callable_arguments_use_internal_binding_natively() {
    let native = compile(
        "eval_opcache_named_native",
        EVAL_ONLY,
        &["--ini", "opcache.enable_cli=1"],
    );
    assert_opcache_named_callable_binding(&native);
}

/// User functions that take OPcache names: a by-name signature and a by-reference parameter.
const OPCACHE_USER_DECLARATIONS: &str = r#"
function opcache_get_status(string $message, string $suffix = ''): string {
    return $message . $suffix;
}
function opcache_reset(int &$counter): int { $counter += 1; return $counter; }
"#;

/// Calls the user declarations on the direct, variable, first-class and `call_user_func` routes.
const OPCACHE_USER_DECLARATION_CALLS: &str = r#"
echo opcache_get_status(suffix: ':direct', message: 'own'), "\n";
$f = 'opcache_get_status';
echo $f(suffix: ':variable', message: 'own'), "\n";
echo call_user_func_array($f, ['suffix' => ':array', 'message' => 'own']), "\n";
echo call_user_func($f, suffix: ':callback', message: 'own'), "\n";
$f = opcache_get_status(...);
echo $f(suffix: ':first-class', message: 'own'), "\n";
echo opcache_get_status('own', ':surplus', 'ignored'), "\n";
$counter = 1;
echo opcache_reset($counter), ':', $counter, "\n";
$f = 'opcache_reset';
echo $f(counter: $counter), ':', $counter, "\n";
$f = opcache_reset(...);
echo $f(counter: $counter), ':', $counter, "\n";
echo call_user_func_array('opcache_reset', ['counter' => &$counter]), ':', $counter, "\n";
echo call_user_func('opcache_reset', counter: $counter), ':', $counter, "\n";
"#;

/// What the user declarations print on every route, the by-reference counter included.
const OPCACHE_USER_DECLARATION_OUTPUT: &str =
    "own:direct\nown:variable\nown:array\nown:callback\nown:first-class\n\
     own:surplus\n2:2\n3:3\n4:4\n5:5\n6:5\n";

/// Compiles the user declarations natively with the given extra CLI arguments, runs the calls
/// through an opaque `eval()`, asserts the pinned output and removes the fixture directory.
fn assert_native_opcache_user_declarations(ini: &[&str]) {
    let source =
        format!("<?php\n{OPCACHE_USER_DECLARATIONS}\n$code = getenv('REVIEW_CODE'); eval($code);");
    let binary = compile("eval_opcache_native_user", &source, ini);
    assert_stdout(&binary, OPCACHE_USER_DECLARATION_CALLS, OPCACHE_USER_DECLARATION_OUTPUT);
    fs::remove_dir_all(binary.parent().unwrap()).unwrap();
}

/// User declarations with OPcache names retain their signatures and reference parameters on
/// direct, variable, first-class and call_user_func routes in the CLI-default binary.
///
/// The configured binary and the eval-declared case are separate tests: each is a full
/// eval-linked compile.
#[test]
fn opcache_user_declarations_keep_their_callable_semantics() {
    assert_native_opcache_user_declarations(&[]);
}

/// The same native user declarations keep their semantics with `opcache.enable_cli=1`.
#[test]
fn opcache_user_declarations_keep_their_callable_semantics_with_the_cache_enabled() {
    assert_native_opcache_user_declarations(&["--ini", "opcache.enable_cli=1"]);
}

/// The same declarations made inside the eval'd fragment keep their semantics too.
#[test]
fn opcache_user_declarations_made_in_eval_keep_their_callable_semantics() {
    let binary = compile("eval_opcache_eval_user", EVAL_ONLY, &[]);
    assert_stdout(
        &binary,
        &format!("{OPCACHE_USER_DECLARATIONS}\n{OPCACHE_USER_DECLARATION_CALLS}"),
        OPCACHE_USER_DECLARATION_OUTPUT,
    );
    fs::remove_dir_all(binary.parent().unwrap()).unwrap();
}

/// Every argument an eval'd OPcache call evaluates is released, on the direct route and on the
/// callable ones.
///
/// PR #968 review: the direct handlers read their arguments with `eval_expr` and dropped them,
/// so ten `opcache_get_status(str_repeat("x", 70000))` calls left 20 blocks (700560 bytes)
/// live, where the same call through `call_user_func()` left none. The CLI-default binary is
/// the one whose eval answers from its own handlers, so it is the one this pins.
#[test]
fn opcache_arguments_evaluated_in_eval_are_released() {
    let binary = compile("eval_opcache_argument_release", EVAL_ONLY, &["--heap-debug"]);
    let code = "for ($i = 0; $i < 10; $i++) {\n\
                    opcache_get_status(str_repeat('x', 70000));\n\
                    opcache_is_script_cached(str_repeat('y', 70000));\n\
                    opcache_invalidate(str_repeat('z', 70000), true);\n\
                    call_user_func('opcache_get_status', str_repeat('x', 70000));\n\
                    $f = 'opcache_compile_file';\n\
                    $f(str_repeat('w', 70000));\n\
                    call_user_func_array('opcache_invalidate', [str_repeat('v', 70000), true]);\n\
                    $f = 'opcache_invalidate';\n\
                    $f(force: true, filename: str_repeat('n', 70000));\n\
                    call_user_func_array($f, ['force' => true, 'filename' => str_repeat('a', 70000)]);\n\
                    call_user_func($f, filename: str_repeat('c', 70000));\n\
                    $f = 'opcache_get_status';\n\
                    try { $f(unknown: str_repeat('e', 70000)); } catch (Error $e) {}\n\
                    try { call_user_func_array($f, ['unknown' => str_repeat('r', 70000)]); } catch (Error $e) {}\n\
                }\n\
                echo \"done\\n\";";
    let output = run(&binary, code);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "done\n", "stderr: {stderr}");
    assert!(
        stderr.contains("HEAP DEBUG: leak summary: clean"),
        "an OPcache call's evaluated argument leaked:\n{stderr}"
    );
}
