//! Purpose:
//! Guards the PHASE at which a usage-gated compatibility prelude is injected. A surface named
//! only by a class the AUTOLOADER brings in must still be declared — the gate runs against the
//! complete closed-world program, not against the entry file.
//!
//! Called from:
//! - `cargo test --test compat_prelude_gate_tests` through Rust's test harness.
//!
//! Key details:
//! - This suite exists because every other prelude test is SINGLE-FILE, and a single-file probe
//!   cannot see the bug. The CLI error prelude was injected in the `web-prelude` phase, whose
//!   gate saw only the 21-line entry script; nothing was injected and the binary died at run
//!   time on `Call to undefined function error_reporting()`. The declaration it needed lived in
//!   a class the autoload pass splices in ~90 lines further down `src/pipeline.rs`, in the
//!   `compat-preludes` phase. Neither the 52 `--web` tests nor the 8 CLI error-surface tests
//!   caught it.
//! - The tell is encoded below as a pair: a `require`d file IS visible to a gate in the early
//!   phase, an AUTOLOADED class is NOT. `a_required_second_file_gates_the_prelude_too` passed
//!   throughout the bug; `an_autoloaded_class_gates_the_prelude` is the one that failed.
//! - The assembly assertions distinguish the two failure modes a run alone confuses: the NAME
//!   appears either way, because the runtime's undefined-function message carries it. Only a
//!   symbol the prelude DECLARES (`__elephc_diag_render`) proves the declaration was compiled.
//! - The error-handling surface is used as the probe because it is the one that broke, but the
//!   property under test belongs to the phase, not to that surface: any usage-gated prelude
//!   injected in `compat-preludes` inherits these assertions.
//! - The assertions are not vacuous, and the neighbouring project proves it rather than a
//!   mutation: on the same three-file shape, the autoloaded class NAMING the surface emits
//!   1 036 342 bytes of assembly with 82 `__elephc_diag_render` hits, and the one naming nothing
//!   emits 161 650 bytes with 0. The counted symbol therefore reaches zero on an input one line
//!   away from the passing case. Every expected string was captured from `php 8.5.10`:
//!   `php -r '$l = error_reporting(); ...'` prints `level=E_ALL handler=yes`.

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

/// Writes a multi-file project into `dir`, creating parent directories for nested paths.
fn write_project(dir: &Path, files: &[(&str, &str)]) {
    for (relative, contents) in files {
        let target = dir.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&target, contents).unwrap();
    }
}

/// Runs elephc over `entry` inside `dir`, with `extra` arguments, and returns its stderr.
fn run_elephc(dir: &Path, entry: &str, extra: &[&str]) -> String {
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(dir)
        .args(extra)
        .arg(entry)
        .output()
        .expect("failed to spawn elephc");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "elephc {extra:?} {entry} failed:\n{stderr}"
    );
    stderr
}

/// Compiles a multi-file project and runs the resulting executable, returning `(stdout, stderr)`.
///
/// The entry must be a top-level `<stem>.php`, so the executable lands at `<dir>/<stem>`.
fn compile_and_run_project(prefix: &str, files: &[(&str, &str)], entry: &str) -> (String, String) {
    let dir = make_test_dir(prefix);
    write_project(&dir, files);
    run_elephc(&dir, entry, &[]);

    let stem = entry.strip_suffix(".php").expect("entry must end in .php");
    let output = Command::new(dir.join(stem))
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

/// Compiles a multi-file project with `--emit-asm` and returns the emitted assembly.
fn emit_project_asm(prefix: &str, files: &[(&str, &str)], entry: &str) -> String {
    let dir = make_test_dir(prefix);
    write_project(&dir, files);
    run_elephc(&dir, entry, &["--emit-asm"]);
    let stem = entry.strip_suffix(".php").expect("entry must end in .php");
    fs::read_to_string(dir.join(format!("{stem}.s"))).expect("emitted assembly")
}

/// A PSR-4 manifest mapping `App\` to `src/`.
const MANIFEST: &str = r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#;

/// A class that NAMES the error-handling surface. Nothing outside this file mentions it.
const REPORTER_USING_SURFACE: &str = r#"<?php
namespace App;

class Reporter
{
    public function report(): string
    {
        $level = error_reporting();
        $installed = function_exists('set_error_handler') ? 'yes' : 'no';

        return 'level=' . ($level === E_ALL ? 'E_ALL' : (string) $level)
            . " handler={$installed}\n";
    }
}
"#;

/// The same class with the surface removed, for the pay-for-use control.
const REPORTER_WITHOUT_SURFACE: &str = r#"<?php
namespace App;

class Reporter
{
    public function report(): string
    {
        return "level=none handler=none\n";
    }
}
"#;

/// An entry that names NOTHING about the surface — it only constructs the autoloaded class.
const ENTRY_AUTOLOADED: &str = r#"<?php
$reporter = new App\Reporter();
echo $reporter->report();
"#;

/// A class that registers a shutdown function and exits. Nothing outside this file mentions
/// `register_shutdown_function`, and the entry that uses it is three lines long.
///
/// This is the shape `Symfony\Component\ErrorHandler\ErrorHandler::register()` has: an
/// autoloaded class calls `register_shutdown_function(self::handleFatalError(...))`, and the
/// console entry point that reaches it says nothing about shutdown functions at all.
const RUNNER_USING_SHUTDOWN: &str = r#"<?php
namespace App;

class Runner
{
    public function run(): void
    {
        register_shutdown_function(static function (): void { echo "shutdown\n"; });
        echo "run\n";
        exit(5);
    }
}
"#;

/// An entry that only constructs the autoloaded runner.
const ENTRY_RUNNER: &str = r#"<?php
$runner = new App\Runner();
$runner->run();
"#;

/// A class that reads `error_get_last()` from inside its own shutdown function.
///
/// This is `Symfony\Component\ErrorHandler\ErrorHandler` reproduced in miniature, and it is
/// the exact shape that took the `--web` route gate from 7/7 to 0/7: an AUTOLOADED, NAMESPACED
/// class registers a shutdown callback, and that callback calls the BARE global
/// `error_get_last()`. It exercises two independent gates at once, which is why it is one
/// fixture rather than two:
///
/// - the PHASE gate — the entry names nothing, so the prelude is injected only if the gate
///   runs after `autoload-run` (see `an_autoloaded_class_gates_the_prelude`);
/// - the NAMESPACE gate — inside `namespace App;` a bare `error_get_last()` resolves to
///   `App\error_get_last` unless the name is in `name_resolver`'s
///   `canonical_compat_prelude_function_name` list. Without that entry this compiles and then
///   dies at run time, while `register_shutdown_function` one line above resolves fine.
const GUARD_USING_LAST_ERROR: &str = r#"<?php
namespace App;

class Guard
{
    public static function arm(): void
    {
        register_shutdown_function([self::class, 'onShutdown']);
        trigger_error('from-autoloaded', E_USER_WARNING);
    }

    public static function onShutdown(): void
    {
        $error = error_get_last();
        echo 'last=', $error === null ? 'null' : $error['message'], "\n";
        error_clear_last();
        echo 'cleared=', error_get_last() === null ? 'null' : 'set', "\n";
    }
}
"#;

/// An entry that names nothing about errors or shutdown — it only calls the autoloaded class.
const ENTRY_GUARD: &str = r#"<?php
App\Guard::arm();
echo "body\n";
"#;

/// A namespaced class calling the BARE global `var_export()`, with a `: string` return.
///
/// The return type is load-bearing: `var_export()` is elephc-PHP whose two-mode body is typed
/// `string|null`, and only `name_resolver::rewrite_var_export_return_flag` narrows a literal-flag
/// call to the `: string` helper. A method declared `: string` therefore fails to compile unless
/// the bare namespaced name folded to the global one AND that rewrite fired.
const EXPORTER_USING_VAR_EXPORT: &str = r#"<?php
namespace App;

class Exporter
{
    public function render(): string
    {
        return var_export([1, 2], true);
    }
}
"#;

/// An entry that names `var_export` itself, so the early-phase gate injects the prelude.
const ENTRY_EXPORTER: &str = r#"<?php
echo 'entry:', var_export([0], true), "\n";
$exporter = new App\Exporter();
echo 'autoloaded:', $exporter->render(), "\n";
"#;

/// Verifies an autoloaded class may read `error_get_last()` from its own shutdown function.
///
/// php 8.5.10 on the same source (with `src/Guard.php` `require`d rather than autoloaded,
/// which changes nothing it prints) reports, with diagnostics on stderr:
///
/// ```text
/// body
/// last=from-autoloaded
/// cleared=null
/// ```
///
/// `body` first because the shutdown callback runs after the top level; `last=from-autoloaded`
/// because the record survives from the raise site into the shutdown function, which is the
/// whole reason Symfony calls it there.
#[test]
fn an_autoloaded_shutdown_function_reads_the_last_error() {
    let (stdout, _) = compile_and_run_project(
        "compatgate_lasterror",
        &[
            ("module.json", MANIFEST),
            ("src/Guard.php", GUARD_USING_LAST_ERROR),
            ("main.php", ENTRY_GUARD),
        ],
        "main.php",
    );

    assert_eq!(stdout, "body\nlast=from-autoloaded\ncleared=null\n");
}

/// THE REGRESSION. The entry names nothing; the class the autoloader brings in names everything.
///
/// Before the injection moved to the `compat-preludes` phase this compiled cleanly and died at
/// run time with `Call to undefined function error_reporting()`, because the gate ran before
/// `autoload-run` had spliced `src/Reporter.php` into the program.
#[test]
fn an_autoloaded_class_gates_the_prelude() {
    let (stdout, stderr) = compile_and_run_project(
        "compatgate_autoloaded",
        &[
            ("module.json", MANIFEST),
            ("src/Reporter.php", REPORTER_USING_SURFACE),
            ("main.php", ENTRY_AUTOLOADED),
        ],
        "main.php",
    );

    assert_eq!(stdout, "level=E_ALL handler=yes\n");
    assert!(
        stderr.is_empty(),
        "an autoloaded caller must not warn:\n{stderr}"
    );
}

/// THE TELL, as a test. A `require`d second file is visible to the gate in EITHER phase, which
/// is exactly why a two-file probe built this way proves nothing about the phase and why the
/// bug survived. Kept green on purpose: it is the control for the test above, and a future
/// change that breaks this one breaks something else entirely.
#[test]
fn a_required_second_file_gates_the_prelude_too() {
    let (stdout, stderr) = compile_and_run_project(
        "compatgate_required",
        &[
            (
                "lib.php",
                "<?php\nfunction probe(): string { return error_reporting() === E_ALL ? \"ok\\n\" : \"wrong\\n\"; }\n",
            ),
            (
                "main.php",
                "<?php\nrequire __DIR__ . '/lib.php';\necho probe();\n",
            ),
        ],
        "main.php",
    );

    assert_eq!(stdout, "ok\n");
    assert!(
        stderr.is_empty(),
        "a required caller must not warn:\n{stderr}"
    );
}

/// The declaration is really COMPILED, not merely named.
///
/// The bare function name appears in the assembly either way — the runtime's undefined-function
/// message carries it — so a name grep cannot tell "never injected" from "injected but broken".
/// `__elephc_diag_render` is emitted by the prelude itself, so its presence is the proof.
#[test]
fn the_autoloaded_gate_emits_the_declaration_not_just_the_name() {
    let asm = emit_project_asm(
        "compatgate_asm_using",
        &[
            ("module.json", MANIFEST),
            ("src/Reporter.php", REPORTER_USING_SURFACE),
            ("main.php", ENTRY_AUTOLOADED),
        ],
        "main.php",
    );

    assert!(
        asm.contains("__elephc_diag_render"),
        "the prelude's own symbol must be present when an autoloaded class names the surface"
    );
}

/// PAY-FOR-USE survives the later phase. Moving the gate after `autoload-run` widens what it can
/// see, and the failure mode of widening a gate is that it stops being a gate: a program whose
/// autoloaded classes name nothing must still carry none of it.
#[test]
fn an_autoloaded_class_that_names_nothing_still_pays_nothing() {
    let files_using: [(&str, &str); 3] = [
        ("module.json", MANIFEST),
        ("src/Reporter.php", REPORTER_USING_SURFACE),
        ("main.php", ENTRY_AUTOLOADED),
    ];
    let files_bare: [(&str, &str); 3] = [
        ("module.json", MANIFEST),
        ("src/Reporter.php", REPORTER_WITHOUT_SURFACE),
        ("main.php", ENTRY_AUTOLOADED),
    ];

    let using = emit_project_asm("compatgate_asm_pfu_using", &files_using, "main.php");
    let bare = emit_project_asm("compatgate_asm_pfu_bare", &files_bare, "main.php");

    for symbol in [
        "error_reporting",
        "__elephc_diag_dispatch",
        "__elephc_diag_render",
    ] {
        assert!(
            !bare.contains(symbol),
            "no autoloaded class names the surface, so {symbol} must be absent"
        );
    }
    assert!(
        bare.len() < using.len(),
        "the bare project must be smaller: {} vs {}",
        bare.len(),
        using.len()
    );
}

/// The same phase property for `register_shutdown_function`, with the exit path attached.
///
/// Two things have to hold at once and only a multi-file project shows both: the gate must see
/// the name through the AUTOLOADER (the entry spells nothing), and the drain must run on the
/// `exit(5)` the autoloaded method performs — `exit()` does not run `finally` blocks, so a
/// wrapper-based drain would produce `run` alone. php 8.5.10 on the same three files prints
/// `run` then `shutdown` and exits 5.
#[test]
fn an_autoloaded_class_gates_the_shutdown_prelude() {
    let dir = make_test_dir("compatgate_shutdown");
    write_project(
        &dir,
        &[
            ("module.json", MANIFEST),
            ("src/Runner.php", RUNNER_USING_SHUTDOWN),
            ("main.php", ENTRY_RUNNER),
        ],
    );
    run_elephc(&dir, "main.php", &[]);

    let output = Command::new(dir.join("main"))
        .output()
        .expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert_eq!(stdout, "run\nshutdown\n", "got {stdout:?}");
    assert_eq!(
        output.status.code(),
        Some(5),
        "the status the autoloaded method asked for must survive the drain"
    );
}

/// The RESOLVER half of the same family, which is a different fault from the phase.
///
/// `var_export` is injected in its own phase BEFORE `autoload-run`, so an entry that names it
/// gets the prelude. An autoloaded class in `namespace App;` calling the bare `var_export()`
/// still died with `Call to undefined function App\var_export()` — measured — because autoloaded
/// files are name-resolved individually and nothing gave `App\var_export` PHP's global fallback.
/// The fix is one entry in `name_resolver::canonical_compat_prelude_function_name`, the same one
/// `ini_get_all` needed.
///
/// The `: string` return in the fixture is what distinguishes "resolved" from "resolved and
/// rewritten": without the literal-flag rewrite the call is typed `string|null` and the method
/// does not compile at all.
///
/// STILL BROKEN, deliberately not asserted here: a project where ONLY the autoloaded class names
/// `var_export` and the entry does not. The early gate then injects nothing and the same call
/// dies with `Call to undefined function var_export()` — the name now folds, the declaration is
/// absent. A second, post-`autoload-run` injection site would declare it but could not re-run
/// the literal-flag rewrite, so a `: string` method would trade a run-time error for a
/// compile-time one. That is a real fix and a larger one.
#[test]
fn an_autoloaded_class_reaches_var_export_through_the_global_fallback() {
    let (stdout, stderr) = compile_and_run_project(
        "compatgate_var_export",
        &[
            ("module.json", MANIFEST),
            ("src/Exporter.php", EXPORTER_USING_VAR_EXPORT),
            ("main.php", ENTRY_EXPORTER),
        ],
        "main.php",
    );

    assert_eq!(
        stdout,
        "entry:array (\n  0 => 0,\n)\nautoloaded:array (\n  0 => 1,\n  1 => 2,\n)\n",
        "got {stdout:?}"
    );
    assert!(stderr.is_empty(), "no warning is expected:\n{stderr}");
}

/// An autoloaded class that builds a document with the classic DOM classes and parses HTML with
/// `loadHTML()` flags, the shape of Symfony's `XmlDescriptor` and Termwind's `HtmlRenderer`.
const DESCRIBER_USING_DOM: &str = r#"<?php
namespace App;

class Describer
{
    public function describe(array $routes): string
    {
        $dom = new \DOMDocument('1.0', 'UTF-8');
        $dom->appendChild($root = $dom->createElement('routes'));
        foreach ($routes as $name => $path) {
            $root->appendChild($route = $dom->createElement('route'));
            $route->setAttribute('name', $name);
            $route->appendChild(new \DOMText($path));
        }
        $dom->formatOutput = true;
        return $dom->saveXML();
    }

    public function texts(string $html): string
    {
        $dom = new \DOMDocument();
        $dom->loadHTML('<?xml encoding="UTF-8">' . $html, LIBXML_NOERROR | LIBXML_HTML_NODEFDTD);
        $found = [];
        foreach ($dom->getElementsByTagName('b') as $b) {
            $found[] = $b->nodeValue;
        }
        return implode(',', $found);
    }
}
"#;

/// An entry that names nothing about DOM -- it only calls the autoloaded class.
const ENTRY_DESCRIBER: &str = r#"<?php
$d = new App\Describer();
echo $d->describe(['home' => '/', 'blog' => '/blog/{slug}']);
echo $d->texts('<p><b>one</b> and <b>two</b></p>'), "\n";
"#;

/// The classic DOM classes are injected after autoload expansion: the entry names no DOM class,
/// the autoloaded vendor class does, and the program must still build, serialize and parse.
///
/// Expected output captured from php 8.5.10 running the same two files with the class
/// `require`d instead of autoloaded.
#[test]
fn an_autoloaded_class_gates_the_classic_dom_prelude() {
    let (stdout, stderr) = compile_and_run_project(
        "compatgate_dom",
        &[
            ("module.json", MANIFEST),
            ("src/Describer.php", DESCRIBER_USING_DOM),
            ("main.php", ENTRY_DESCRIBER),
        ],
        "main.php",
    );

    assert_eq!(
        stdout,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<routes>\n  <route name=\"home\">/</route>\n  <route name=\"blog\">/blog/{slug}</route>\n</routes>\none,two\n",
        "got {stdout:?}"
    );
    assert!(stderr.is_empty(), "no warning is expected:\n{stderr}");
}

/// Pay-for-use: a program that never names a DOM class carries none of the surface, and one that
/// does carries it. `DOMDocument` is a symbol only the prelude declares.
#[test]
fn a_program_that_never_names_dom_pays_nothing() {
    let without = emit_project_asm(
        "compatgate_dom_without",
        &[("main.php", "<?php echo \"hello\\n\";\n")],
        "main.php",
    );
    let with = emit_project_asm(
        "compatgate_dom_with",
        &[("main.php", "<?php $d = new DOMDocument(); echo $d->saveXML();\n")],
        "main.php",
    );

    assert_eq!(without.matches("DOMDocument").count(), 0, "an unused DOM surface was compiled");
    assert!(with.matches("DOMDocument").count() > 0, "the named DOM surface was not compiled");
}
