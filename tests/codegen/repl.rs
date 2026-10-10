//! Purpose:
//! Exercises the real cached REPL host through the public compiler command.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::repl` on supported desktop hosts.
//!
//! Key details:
//! - Each fixture owns its project and cache and removes both even after an assertion fails.
//! - Tests submit input through stdin; no external PHP installation is required.

use crate::support::{elephc_cli_command, ensure_cli_bridge_staticlibs, make_cli_test_dir};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Output, Stdio};

#[path = "repl_terminal.rs"]
mod terminal;

/// One isolated project and all cache artifacts produced by its REPL sessions.
struct Fixture(PathBuf);

impl Fixture {
    /// Ensures the actual Magician staticlib exists before starting a compiler subprocess.
    fn new() -> Self {
        ensure_cli_bridge_staticlibs(&["elephc_magician"]);
        Self(make_cli_test_dir("elephc_repl"))
    }

    /// Starts a piped session using a stable explicit profile independent of local Composer files.
    fn start(&self, args: &[&str]) -> Child {
        elephc_cli_command(&self.0).args(["repl", "--php-version", "8.5"])
            .args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().expect("start REPL")
    }

    /// Submits one transcript and waits for the native session to finish.
    fn run(&self, source: &str, args: &[&str]) -> Output {
        let mut child = self.start(args);
        child.stdin.take().unwrap().write_all(source.as_bytes()).expect("submit transcript");
        child.wait_with_output().expect("wait for REPL")
    }

    /// Lists only published hosts, excluding locks, runtime objects, and staging files.
    fn hosts(&self) -> Vec<PathBuf> {
        let root = self.0.join("cache-root/elephc/repl");
        fs::read_dir(root).unwrap().filter_map(|entry| {
            let host = entry.unwrap().path().join("host");
            host.is_file().then_some(host)
        }).collect()
    }
}

impl Drop for Fixture {
    /// Removes this fixture's exact owned directory on both success and unwinding.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Asserts success while retaining both output streams in any failing diagnostic.
fn stdout(output: &Output) -> String {
    assert!(output.status.success(), "status {}\nstdout: {}\nstderr: {}", output.status,
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout.clone()).unwrap()
}

/// Variables, declarations, references, arrays, and multiline bodies survive successive eval calls.
#[test]
fn test_repl_persistent_eval_session() {
    let fixture = Fixture::new();
    let output = fixture.run(concat!(
        "$n = 10;\n",
        "function twice($value) {\n return $value * 2;\n}\n",
        "twice($n)\n",
        "$alias =& $n;\n",
        "$alias += 3;\n",
        "$n\n",
        "$items = [1,\n2, 3];\n",
        "count($items)\n",
        "class Box { public $value = 7; }\n",
        "$box = new Box();\n",
        "$box->value\n",
        "unset($n);\n",
        "isset($n)\n",
    ), &[]);
    let out = stdout(&output);
    assert!(out.starts_with("int(10)\nint(20)\n"), "{out}");
    assert!(out.contains("int(13)\nint(13)\n"), "{out}");
    assert!(out.contains("int(3)\n"), "{out}");
    assert!(out.contains("object(Box)"), "{out}");
    assert!(out.ends_with("int(7)\nbool(false)\n"), "{out}");
    assert!(!out.contains(">>>"));
    assert!(!fixture.0.join("cache-root/elephc/repl/history").exists());
}

/// Syntax errors and escaping Throwable values preserve committed state and process later input.
#[test]
fn test_repl_recovers_without_replaying_effects() {
    let fixture = Fixture::new();
    let output = fixture.run(concat!(
        "$n = 0;\n",
        "function bump() { global $n; $n++; return $n; }\n",
        "bump()\n",
        "eval('return 42;')\n",
        "$x = ;\n",
        "$n = 8; throw new Exception('recover');\n",
        "$n\n",
        "bump()\n",
    ), &[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "int(0)\nint(1)\nint(42)\nint(8)\nint(9)\n");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("syntax error"), "{error}");
    assert!(error.contains("Exception: recover"), "{error}");
}

/// A misspelled function aborts only its submission and leaves state available to later input.
#[test]
fn test_repl_undefined_functions_keep_session_state() {
    let fixture = Fixture::new();
    let output = fixture.run(concat!(
        "$a = 41;\n",
        "if (isseet($a)) {\n echo 'unexpected';\n} else {\n echo 'unexpected';\n}\n",
        "$a + 1\n",
        "isset($a)\n",
    ), &[]);
    assert_eq!(output.status.code(), Some(1), "a piped session records failed submissions");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "int(41)\nint(42)\nbool(true)\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "Error: Call to undefined function isseet()\n");
}

/// Reported eval failures abort one submission, preserving earlier effects without replay.
#[test]
fn test_repl_recovers_reported_eval_failures() {
    let fixture = Fixture::new();
    let output = fixture.run(concat!(
        "$n = 0;\n",
        "das\n",
        "function bump() { global $n; $n++; return $n; }\n",
        "bump(); break;\n",
        "eval('$broken = ;');\n",
        "new MissingReplClass();\n",
        "trigger_error('recover user fatal', E_USER_ERROR);\n",
        "strlen()\n",
        "call_user_func(new stdClass())\n",
        "$n\n",
        "bump()\n",
        "$items = ['owned' => str_repeat('x', 100)];\n",
        "count($items)\n",
        "unset($items);\n",
        "echo 'alive';\n",
    ), &["--heap-debug"]);
    assert_eq!(output.status.code(), Some(1));
    let out = String::from_utf8_lossy(&output.stdout);
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(out.starts_with("int(0)\nint(1)\nint(2)\n"), "stdout: {out}\nstderr: {error}");
    assert!(out.ends_with("int(1)\nalive"), "{out}");
    for diagnostic in ["unsupported construct", "fragment is invalid",
        "runtime failed", "recover user fatal", "ArgumentCountError", "TypeError"] {
        assert!(error.contains(diagnostic), "missing {diagnostic}: {error}");
    }
    assert!(!error.contains("use-after-free") && !error.contains("double free"), "{error}");
}

/// Nested eval and function frames unwind before later submissions run.
#[test]
fn test_repl_recovers_nested_failures() {
    let fixture = Fixture::new();
    let output = fixture.run(concat!(
        "$n = 0;\n",
        "function fail(&$value) { $value++; eval('das;'); echo 'unreachable'; }\n",
        "fail($n); echo 'unreachable';\n",
        "$n\n",
        "21 * 2\n",
        "function stable($n) { return $n + 1; }\n",
        "stable($n)\n",
        "try { isseet(); } catch (Error $e) { echo 'caught'; }\n",
        "echo 'alive';\n",
    ), &["--heap-debug"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "int(0)\nint(1)\nint(42)\nint(2)\ncaughtalive");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "Error: eval() runtime failed\n");
}

/// Includes, magic directory constants, and project configuration use the launch directory.
#[test]
fn test_repl_project_context_and_multiline_strings() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("included.php"), "<?php $included = 42;").unwrap();
    fs::write(fixture.0.join("elephc.toml"), "[ini]\n\"opcache.revalidate_freq\" = 7\n").unwrap();
    fs::write(fixture.0.join("composer.json"), r#"{"autoload":{"files":["must-not-compile.php"]}}"#).unwrap();
    fs::write(fixture.0.join("must-not-compile.php"), "<?php echo 'unexpected autoload side effect';").unwrap();
    let output = fixture.run(concat!(
        "require_once 'included.php';\n",
        "$included\n",
        "__DIR__ === getcwd()\n",
        "opcache_get_configuration()['directives']['opcache.revalidate_freq']\n",
        "$text = \"first\nsecond\";\n",
        "echo $text;\n",
    ), &[]);
    let out = stdout(&output);
    assert!(out.contains("int(42)\nbool(true)\n"), "{out}");
    assert!(out.contains("int(7)"), "{out}");
    assert!(out.ends_with("first\nsecond"), "{out}");
    assert!(!out.contains("unexpected autoload"), "{out}");
}

/// Warm launches reuse the same executable, start fresh state, and recover from corrupted bytes.
#[test]
fn test_repl_cache_reuse_integrity_and_profile() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    assert_eq!(stdout(&fixture.run("$n = 12;\n", &[])), "int(12)\n");
    let hosts = fixture.hosts();
    assert_eq!(hosts.len(), 1);
    let original = fs::metadata(&hosts[0]).unwrap();
    assert_eq!(stdout(&fixture.run("isset($n)\n", &[])), "bool(false)\n");
    assert_eq!(fs::metadata(&hosts[0]).unwrap().ino(), original.ino(), "warm session rebuilt its host");
    fs::write(&hosts[0], b"damaged cache executable").unwrap();
    assert_eq!(stdout(&fixture.run("6 * 7\n", &[])), "int(42)\n");
    assert!(fs::metadata(&hosts[0]).unwrap().len() > 100);
    assert!(stdout(&fixture.run("PHP_VERSION\n", &["--php-version=8.4"])).contains("8.4.0"));
    assert_eq!(fixture.hosts().len(), 2);
    for host in fixture.hosts() { assert!(!host.parent().unwrap().join("build").exists()); }
}

/// Strict mode reaches eval while compiler-owned extern declarations remain valid in the host.
#[test]
fn test_repl_strict_php_and_heap_ownership() {
    let fixture = Fixture::new();
    let output = fixture.run(concat!(
        "function_exists('ptr_null')\n",
        "$s = str_repeat('x', 100);\n",
        "$items = ['owned' => $s];\n",
        "$alias = $items;\n",
        "$items['owned'] = 'new';\n",
        "strlen($alias['owned'])\n",
        "unset($s, $items, $alias);\n",
    ), &["--strict-php", "--heap-debug"]);
    let out = stdout(&output);
    assert!(out.starts_with("bool(false)\n"), "{out}");
    assert!(out.ends_with("int(100)\n"), "{out}");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!error.contains("use-after-free") && !error.contains("double free"), "{error}");
}

/// Competing first launches serialize publication and both execute a complete native host.
#[test]
fn test_repl_concurrent_cache_build() {
    let fixture = Fixture::new();
    let mut first = fixture.start(&[]);
    let mut second = fixture.start(&[]);
    first.stdin.take().unwrap().write_all(b"21 * 2\n").unwrap();
    second.stdin.take().unwrap().write_all(b"20 + 22\n").unwrap();
    assert_eq!(stdout(&first.wait_with_output().unwrap()), "int(42)\n");
    assert_eq!(stdout(&second.wait_with_output().unwrap()), "int(42)\n");
    assert_eq!(fixture.hosts().len(), 1);
}

/// EOF distinguishes incomplete input from a normal exit and explicit PHP exit keeps its status.
#[test]
fn test_repl_eof_commands_and_exit_status() {
    let fixture = Fixture::new();
    let incomplete = fixture.run("function missing() {\n", &[]);
    assert_eq!(incomplete.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&incomplete.stderr).contains("incomplete input"));
    assert_eq!(stdout(&fixture.run(":help\n:quit\necho 'unreachable';\n", &[])), concat!(
        "Enter PHP without <?php. Expressions display their result.\n",
        "Continue incomplete code at ...; Ctrl-C cancels the current input.\n",
        ":help shows this help; :quit or Ctrl-D exits.\n",
        "Eval errors return to the prompt; PHP exit() ends the session.\n",
    ));
    assert_eq!(fixture.run("exit(7);\n", &[]).status.code(), Some(7));
}
