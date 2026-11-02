//! Purpose:
//! End-to-end tests for PHP's error/exception-handling surface in a PLAIN (non-`--web`)
//! binary: that `error_reporting()`, the `set_error_handler()` stack, `trigger_error()` and
//! the exception-handler stack exist at all, that an engine-raised diagnostic reaches the
//! installed handler, and that the pay-for-use gate keeps an unrelated program from growing.
//!
//! Called from:
//! - `cargo test --test error_handling_surface_tests` through Rust's test harness.
//!
//! Key details:
//! - This surface was declared ONLY by the `--web` prelude, so a plain CLI build answered
//!   `Call to undefined function error_reporting()` — `tests/web_session_tests.rs` had the
//!   `--web` half of every assertion below and nothing had the CLI half. The declarations now
//!   live in `src/error_handling_prelude.rs` and both SAPIs take them from there.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp
//!   dir, compile a plain executable, run it, and assert stdout AND stderr — the same harness
//!   style as `php_version_surface_tests` / `function_exists_tests`. Host-target only.
//! - Every expected string here was captured from `php 8.5.10` on the same source. The one
//!   deliberate divergence is `trigger_error()`'s `$file`/`$line`, which report the ENTRY file
//!   and line 0 rather than the call site: the prelude is built as AST with no source position
//!   of its own (`error_handling_prelude::e_magic_line`). The tests therefore assert the
//!   handler's `$severity`/`$message` for `trigger_error()` and leave its location alone.

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
        let mut exe = std::env::current_exe().expect("failed to resolve current test binary");
        exe.pop();
        if exe.ends_with("deps") {
            exe.pop();
        }
        exe.join("elephc").to_string_lossy().into_owned()
    })
}

/// Compiles `source` to a plain (non-`--web`) executable and returns its path.
fn compile_cli(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Runs a compiled executable and returns `(stdout, stderr)`.
///
/// Both channels matter here: a handler that runs but does not suppress leaves stdout right and
/// stderr wrong, and a diagnostic that is merely swallowed leaves stderr right and stdout wrong.
fn run_binary(bin: &Path) -> (String, String) {
    let output = Command::new(bin)
        .output()
        .expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "compiled binary exited non-zero:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    (stdout, stderr)
}

/// Compiles and runs `source`, returning `(stdout, stderr)`.
fn compile_and_run(prefix: &str, source: &str) -> (String, String) {
    let dir = make_test_dir(prefix);
    let bin = compile_cli(&dir, source, "app");
    run_binary(&bin)
}

/// Verifies the whole surface EXISTS off `--web`, which is the gap this suite was written for:
/// before the shared prelude, `function_exists()` answered false for all seven and calling one
/// was `Call to undefined function`.
///
/// `error_reporting()`'s default is asserted against php's own: `php -r 'var_dump(error_reporting()
/// === E_ALL);'` prints `bool(true)` on 8.5.10 with no ini overrides.
#[test]
fn the_error_handling_surface_exists_in_a_plain_cli_build() {
    let source = r#"<?php
foreach ([
    'error_reporting',
    'set_error_handler',
    'get_error_handler',
    'restore_error_handler',
    'trigger_error',
    'error_get_last',
    'error_clear_last',
    'set_exception_handler',
    'restore_exception_handler',
] as $name) {
    echo $name, '=', function_exists($name) ? '1' : '0', "\n";
}
echo 'mask=', error_reporting() === E_ALL ? 'E_ALL' : 'other', "\n";
"#;
    let (stdout, _) = compile_and_run("errsurface_exists", source);
    for name in [
        "error_reporting",
        "set_error_handler",
        "get_error_handler",
        "restore_error_handler",
        "trigger_error",
        "error_get_last",
        "error_clear_last",
        "set_exception_handler",
        "restore_exception_handler",
    ] {
        assert!(
            stdout.contains(&format!("{name}=1\n")),
            "{name} must exist in a plain CLI build: {stdout:?}"
        );
    }
    assert!(
        stdout.contains("mask=E_ALL\n"),
        "error_reporting() must default to E_ALL as php does: {stdout:?}"
    );
}

/// The CLI twin of `web_session_tests::set_error_handler_receives_a_warning_raised_by_compiled_code`.
///
/// A diagnostic raised by COMPILED code (reading an undefined array key) must reach the
/// installed handler with php's message and `E_WARNING`; returning true must suppress the
/// default display entirely; and after `restore_error_handler()` the same diagnostic must be
/// rendered in php's full ` in FILE on line N` form.
///
/// php 8.5.10 on this source prints, with `display_errors` to stdout suppressed for clarity:
/// `Warning: Undefined array key "absent-two" in <file> on line 9`.
#[test]
fn a_warning_raised_by_compiled_code_reaches_a_cli_handler() {
    let dir = make_test_dir("errsurface_invoked");
    let source = r#"<?php
set_error_handler(function (int $severity, string $message, string $file, int $line): bool {
    echo "HANDLER[$severity]:$message|";
    return true;
});
$a = ['k' => 'v'];
$first = $a['absent-one'];
restore_error_handler();
$second = $a['absent-two'];
echo 'body-after';
"#;
    let bin = compile_cli(&dir, source, "app");
    let (stdout, stderr) = run_binary(&bin);

    assert!(
        stdout.contains(r#"HANDLER[2]:Undefined array key "absent-one"|"#),
        "the handler must receive the engine-raised warning, with E_WARNING and php's message: {stdout:?}"
    );
    assert!(
        stdout.ends_with("body-after"),
        "the handler must not disturb the program's own output: {stdout:?}"
    );
    assert!(
        !stdout.contains("absent-two"),
        "a diagnostic raised after restore_error_handler() must not reach the handler: {stdout:?}"
    );
    assert!(
        !stderr.contains("absent-one"),
        "a handler returning true suppresses the default display entirely: {stderr:?}"
    );
    assert!(
        stderr.contains(r#"Warning: Undefined array key "absent-two" in "#)
            && stderr.contains("on line 9"),
        "once the handler is restored the diagnostic is displayed with php's file and line: {stderr:?}"
    );
}

/// The handler shape production code actually installs: an ARRAY callable plus a level mask.
///
/// Frameworks register `set_error_handler([$handler, 'handleError'], $mask)`, not a closure, and
/// a closure-only test proves less than it looks — the callable travels out of a static array
/// inside `__elephc_error_handler_state`, which is the exact position where a direct `$h(...)`
/// invoke lowers to "mixed value is not callable".
///
/// php 8.5.10 prints `array callable handler saw: 2:Undefined array key 1`.
#[test]
fn an_array_callable_handler_with_a_level_mask_receives_the_warning() {
    let source = r#"<?php
class Collector
{
    public array $seen = [];

    public function handleError(int $severity, string $message): bool
    {
        $this->seen[] = "$severity:$message";
        return true;
    }
}

$collector = new Collector();
set_error_handler([$collector, 'handleError'], E_ALL);

$a = [0 => 'zero'];
$x = $a[1];

echo count($collector->seen) === 1
    ? "array callable handler saw: {$collector->seen[0]}\n"
    : "array callable handler saw NOTHING\n";
"#;
    let (stdout, stderr) = compile_and_run("errsurface_arraycallable", source);
    assert_eq!(
        stdout, "array callable handler saw: 2:Undefined array key 1\n",
        "an array callable handler must receive the warning exactly as php's does: {stdout:?}"
    );
    assert!(
        !stderr.contains("Undefined array key"),
        "a handler returning true suppresses the default display: {stderr:?}"
    );
}

/// The SECOND argument of `set_error_handler()` is a level mask, and a diagnostic outside it
/// must bypass the handler and be displayed.
///
/// Verified against php 8.5.10: with `set_error_handler($h, E_NOTICE)` the `E_WARNING` from an
/// undefined array key is NOT handed to `$h` and is displayed.
#[test]
fn a_level_mask_that_excludes_the_diagnostic_leaves_the_default_display() {
    let source = r#"<?php
set_error_handler(function (int $severity, string $message): bool {
    echo "HANDLED|";
    return true;
}, E_NOTICE);
$a = ['k' => 'v'];
$x = $a['absent'];
echo 'done';
"#;
    let (stdout, stderr) = compile_and_run("errsurface_mask", source);
    assert_eq!(
        stdout, "done",
        "a level the handler's mask excludes must not reach it: {stdout:?}"
    );
    assert!(
        stderr.contains(r#"Warning: Undefined array key "absent" in "#),
        "and it must still be displayed: {stderr:?}"
    );
}

/// `trigger_error()` reaches the handler with the level it was given, and renders php's default
/// form when no handler took it.
///
/// php 8.5.10 prints `Warning: triggered-by-user in <file> on line N` for the second call; the
/// prelude's rendering has no source position of its own, so only the prefix and the message are
/// asserted (see this file's header).
#[test]
fn trigger_error_dispatches_to_a_handler_and_renders_by_default() {
    let source = r#"<?php
set_error_handler(function (int $severity, string $message): bool {
    echo "USER[$severity]:$message|";
    return true;
});
echo trigger_error('handled-by-user', E_USER_WARNING) ? "true|" : "false|";
restore_error_handler();
trigger_error('triggered-by-user', E_USER_WARNING);
echo 'done';
"#;
    let (stdout, stderr) = compile_and_run("errsurface_trigger", source);
    assert_eq!(
        stdout, "USER[512]:handled-by-user|true|done",
        "trigger_error() must reach the handler with E_USER_WARNING and return its verdict: {stdout:?}"
    );
    assert!(
        stderr.contains("Warning: triggered-by-user"),
        "with no handler, trigger_error() renders php's default display: {stderr:?}"
    );
}

/// The exception-handler stack: `set_exception_handler()` returns the previous handler and
/// `restore_exception_handler()` pops it.
///
/// php 8.5.10 prints `NSt` for this source (`null`, then the string handler name).
#[test]
fn the_exception_handler_stack_tracks_set_and_restore() {
    let source = r#"<?php
function h1($e) {}
function h2($e) {}
echo set_exception_handler('h1') === null ? 'N' : 'S';
echo set_exception_handler('h2') === 'h1' ? 'S' : 'X';
echo restore_exception_handler() ? 't' : 'f';
"#;
    let (stdout, _) = compile_and_run("errsurface_exc_stack", source);
    assert_eq!(
        stdout, "NSt",
        "set/restore_exception_handler must report the previous handler as php does: {stdout:?}"
    );
}

/// THE POLYFILL SHAPE. A program that guards its own declaration with `function_exists()` must
/// still BUILD.
///
/// `if (!function_exists('trigger_error')) { function trigger_error(…) { … } }` is ordinary
/// library code. php never takes the branch, and neither does a compiled binary — but elephc
/// hoists the declaration out of the guard regardless, so injecting the prelude's copy next to
/// it produced `error: symbol '_fn_trigger_u_error' is already defined` from the assembler.
/// The same source failed a `--web` LINK the same way before the filter existed, so this is the
/// CLI half of a `--web` defect, not a new one.
///
/// php 8.5.10 prints `handler:512:from-source` / `done`, i.e. the built-in wins and the
/// polyfill is never defined.
#[test]
fn a_function_exists_guarded_polyfill_still_builds() {
    let source = r#"<?php
if (!function_exists('trigger_error')) {
    function trigger_error(string $message, int $level = E_USER_NOTICE): bool
    {
        echo "POLYFILL:$message\n";
        return true;
    }
}
set_error_handler(function (int $s, string $m): bool {
    echo "handler:$s:$m\n";
    return true;
});
trigger_error('from-source', E_USER_WARNING);
echo "done\n";
"#;
    let (stdout, _) = compile_and_run("errsurface_polyfill", source);
    assert_eq!(
        stdout, "handler:512:from-source\ndone\n",
        "a guarded polyfill must build, and the program's own declaration must win: {stdout:?}"
    );
}

/// Verifies `error_get_last()` / `error_clear_last()` reproduce php's whole recording rule.
///
/// The expectation is php's own output on BYTE-IDENTICAL source, not a hand-written guess.
/// php 8.5.10, `php -d display_errors=stderr egl_surface.php 2>/dev/null`:
///
/// ```text
/// exists=11
/// initial=null
/// keys=type,message,file,line
/// type=1024
/// message=notice-one
/// type2=512
/// cleared=null
/// kept=kept
/// refused=refused
/// masked=masked
/// ```
///
/// Each line pins one rule that a plausible wrong implementation gets wrong:
///
/// - `initial=null` — nothing recorded before the first diagnostic.
/// - `keys=` — the four keys, IN php's ORDER. A record built as a different shape still
///   satisfies `$error['type']` and fails here.
/// - `type=1024` / `type2=512` — `E_USER_NOTICE` and `E_USER_WARNING` reach the record with
///   their own levels, so `trigger_error()` is not flattened to one severity.
/// - `cleared=null` — `error_clear_last()` returns the slot to its never-written state, not to
///   an empty array.
/// - `kept=kept` — THE TRAP. A handler that returns true records nothing AND does not clear
///   what was already there; `kept=swallowed` means the write point is wrong, `kept=` (empty)
///   means it clears.
/// - `refused=refused` — a handler returning false falls through to the internal handler,
///   which records.
/// - `masked=masked` — `error_reporting(0)` suppresses the DISPLAY only. `masked=` means the
///   mask gate was treated as "nothing happened".
#[test]
fn error_get_last_reproduces_phps_recording_rule() {
    let source = r#"<?php
echo 'exists=', function_exists('error_get_last') ? '1' : '0', function_exists('error_clear_last') ? '1' : '0', "\n";
echo 'initial=', error_get_last() === null ? 'null' : 'set', "\n";
trigger_error('notice-one');
$e = error_get_last();
echo 'keys=', implode(',', array_keys($e)), "\n";
echo 'type=', $e['type'], "\n";
echo 'message=', $e['message'], "\n";
trigger_error('warn-one', E_USER_WARNING);
echo 'type2=', error_get_last()['type'], "\n";
error_clear_last();
echo 'cleared=', error_get_last() === null ? 'null' : 'set', "\n";
trigger_error('kept');
set_error_handler(function ($n, $s, $f, $l) { return true; });
trigger_error('swallowed');
restore_error_handler();
echo 'kept=', error_get_last()['message'], "\n";
set_error_handler(function ($n, $s, $f, $l) { return false; });
trigger_error('refused', E_USER_WARNING);
restore_error_handler();
echo 'refused=', error_get_last()['message'], "\n";
error_clear_last();
error_reporting(0);
trigger_error('masked', E_USER_WARNING);
error_reporting(E_ALL);
echo 'masked=', error_get_last()['message'], "\n";
"#;
    let (stdout, _) = compile_and_run("errsurface_getlast", source);
    assert_eq!(
        stdout,
        "exists=11\n\
         initial=null\n\
         keys=type,message,file,line\n\
         type=1024\n\
         message=notice-one\n\
         type2=512\n\
         cleared=null\n\
         kept=kept\n\
         refused=refused\n\
         masked=masked\n",
        "error_get_last() must match php 8.5.10 on this source"
    );
}

/// Verifies a diagnostic raised by COMPILED code records php's `file` and `line`.
///
/// An ENGINE diagnostic is the case where the raise site publishes its own location
/// (`_php_diagnostic_file` / `_php_diagnostic_line`), and the native dispatcher records it with
/// the message, so this pins that the location reaches the record rather than being dropped or
/// swapped with the message.
///
/// php 8.5.10 on this exact source prints all four lines identically:
///
/// ```text
/// type=2
/// message=Undefined array key "absent"
/// file=1
/// line=4
/// ```
#[test]
fn an_engine_diagnostic_records_php_s_file_and_line() {
    let source = r#"<?php
set_error_handler(function ($n, $s, $f, $l) { return false; });
$a = ['k' => 'v'];
$value = $a['absent'];
restore_error_handler();
$e = error_get_last();
echo 'type=', $e['type'], "\n";
echo 'message=', $e['message'], "\n";
echo 'file=', $e['file'] === __FILE__ ? '1' : '0', "\n";
echo 'line=', $e['line'], "\n";
"#;
    let (stdout, _) = compile_and_run("errsurface_getlast_loc", source);
    assert_eq!(
        stdout,
        "type=2\nmessage=Undefined array key \"absent\"\nfile=1\nline=4\n",
        "an engine diagnostic's record must carry php's type, message, file and line"
    );
}

/// Verifies the record rule for ENGINE diagnostics and every handler outcome.
///
/// The record was dropped by the origin/main merge: main made the warning dispatcher and
/// `trigger_error()` native and nothing wrote the record any more, so every line below printed
/// `null`. Expected output is php 8.5.10's stdout on this exact source (`php -n`, display to
/// stderr). Each line pins one rule: `@` and `error_reporting(0)` silence the display only; a
/// handler returning `true` or `null` takes the diagnostic and leaves the PREVIOUS record; a
/// handler returning `false`, or one whose mask excludes the level, falls through and records.
/// The eval'd warning's `file` is left out: php names an `eval()'d code` pseudo-file there and
/// elephc publishes the host file, a separate location gap.
#[test]
fn error_get_last_records_every_untaken_engine_diagnostic() {
    let source = r#"<?php
function show(string $label): void {
    $e = error_get_last();
    if ($e === null) {
        echo $label, ": null\n";
        return;
    }
    echo $label, ': ', $e['type'], '|', $e['message'], '|', $e['file'] === __FILE__ ? 'F' : 'other', '|', $e['line'], "\n";
}
$a = ['k' => 'v'];
$x = @$a['at'];
show('at');
error_reporting(0);
$x = $a['masked'];
error_reporting(E_ALL);
show('masked');
set_error_handler(function ($n, $s, $f, $l) { return true; });
$x = $a['taken'];
restore_error_handler();
show('true');
set_error_handler(function ($n, $s, $f, $l) { return null; });
$x = $a['null'];
restore_error_handler();
show('null');
set_error_handler(function ($n, $s, $f, $l) { return false; });
$x = @$a['false'];
restore_error_handler();
show('false');
set_error_handler(function ($n, $s, $f, $l) { return true; }, E_NOTICE);
$x = @$a['outside'];
restore_error_handler();
show('outside');
@trigger_error('deprecated', E_USER_DEPRECATED);
show('trigger');
$s = 'abc';
$y = @$s[10];
show('offset');
eval('$b = []; $z = @$b["eval"];');
$e = error_get_last();
echo 'eval-warning: ', $e['type'], '|', $e['message'], '|', $e['line'], "\n";
eval('@trigger_error("eval-trigger", E_USER_WARNING);');
$e = error_get_last();
echo 'eval-trigger: ', $e['type'], '|', $e['message'], "\n";
$z = @$a['compiled'];
$e = eval('return error_get_last();');
echo 'eval-reads: ', $e['message'], "\n";
eval('error_clear_last();');
show('eval-cleared');
"#;
    let (stdout, _) = compile_and_run("errsurface_getlast_engine", source);
    assert_eq!(
        stdout,
        "at: 2|Undefined array key \"at\"|F|11\n\
         masked: 2|Undefined array key \"masked\"|F|14\n\
         true: 2|Undefined array key \"masked\"|F|14\n\
         null: 2|Undefined array key \"masked\"|F|14\n\
         false: 2|Undefined array key \"false\"|F|26\n\
         outside: 2|Undefined array key \"outside\"|F|30\n\
         trigger: 16384|deprecated|F|33\n\
         offset: 2|Uninitialized string offset 10|F|36\n\
         eval-warning: 2|Undefined array key \"eval\"|1\n\
         eval-trigger: 512|eval-trigger\n\
         eval-reads: Undefined array key \"compiled\"\n\
         eval-cleared: null\n",
        "error_get_last() must match php 8.5.10 on this source"
    );
}

/// PAY-FOR-USE. A program that never mentions the prelude surface must not carry its bodies.
///
/// `error_reporting()`, the handler stacks, `trigger_error()`, the warning dispatcher and the
/// last-error record are native: they live in the shared runtime every program links, which is
/// what lets an engine warning reach a handler at all. What stays pay-for-use is the AST prelude
/// (`error_handling_prelude`), and this is the assertion its injection gate exists for: a trivial
/// script must not contain `error_get_last()` / `error_clear_last()` bodies, and the same script
/// naming `error_get_last()` must, and must be larger.
#[test]
fn a_program_that_names_nothing_emits_no_error_handling_code() {
    let dir = make_test_dir("errsurface_payforuse");
    let emit_asm = |stem: &str, source: &str| -> String {
        let php = dir.join(format!("{}.php", stem));
        fs::write(&php, source).unwrap();
        let output = Command::new(elephc_bin())
            .env("XDG_CACHE_HOME", dir.join("cache-root"))
            .current_dir(&dir)
            .arg("--emit-asm")
            .arg(&php)
            .output()
            .expect("failed to spawn elephc");
        assert!(
            output.status.success(),
            "elephc --emit-asm failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::read_to_string(dir.join(format!("{}.s", stem))).expect("emitted assembly")
    };

    let bare = emit_asm("bare", "<?php echo 1;\n");
    let using = emit_asm("using", "<?php echo 1; var_dump(error_get_last());\n");

    // The prelude bodies' symbols, without the platform's leading underscore. The record they
    // read is written natively by every diagnostic, so the gate is the surface NAME: a program
    // that raises warnings but never asks for the last error carries no reader.
    for symbol in ["fn_error_u_get_u_last", "fn_error_u_clear_u_last"] {
        assert!(
            !bare.contains(symbol),
            "a program that names no error function must not carry {symbol}"
        );
    }
    assert!(
        using.contains("fn_error_u_get_u_last"),
        "the prelude reader is injected for a program that names error_get_last()"
    );
    assert!(
        bare.len() < using.len(),
        "the bare program must be smaller: {} vs {}",
        bare.len(),
        using.len()
    );
}
