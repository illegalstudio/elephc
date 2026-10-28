//! Purpose:
//! Regression tests for the source identity of autoloaded class files (issue #1445): statements
//! at the same line and column of two autoloaded files must stay distinct checker map keys, and
//! a diagnostic raised inside an autoloaded file must name that file.
//!
//! Called from:
//! - `cargo test --test codegen_tests_oop autoload_span_identity` through Rust's test harness.
//!
//! Key details:
//! - Autoloaded files used to be lexed with the entry file's source identity, so span-keyed
//!   checker results (the readonly write guard, binding decisions, builtin call types) leaked
//!   from one class file to the statement at the same position in another.
//! - The LFC repro and the diagnostic tests go through the real CLI pipeline; the other runtime
//!   fixtures use the in-process multi-file harness, which runs the same autoload pass.

use crate::support::*;

/// The Composer manifest every fixture here shares: `App\` maps to `src/`.
const COMPOSER_PSR4: &str = r#"{"autoload": {"psr-4": {"App\\": "src/"}}}"#;

/// Writes `files` into a fresh CLI test directory and returns that directory.
fn write_project(prefix: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = make_cli_test_dir(prefix);
    for (path, source) in files {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("project directory should be created");
        }
        fs::write(path, source).expect("project source should be written");
    }
    dir
}

/// Compiles `entry` of a written project through the CLI and returns the native program's stdout.
fn compile_project_with_cli_and_run(
    prefix: &str,
    files: &[(&str, &str)],
    entry: &str,
) -> String {
    let dir = write_project(prefix, files);
    let entry_path = dir.join(entry);
    let compile = elephc_cli_command(&dir)
        .arg(&entry_path)
        .output()
        .expect("elephc CLI should run");
    assert!(
        compile.status.success(),
        "compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = run_binary(&entry_path.with_extension(""), &dir);
    let stdout = String::from_utf8(output.stdout).expect("program output should be UTF-8");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_dir_all(dir);
    assert!(output.status.success(), "program failed: {stderr}\nstdout: {stdout}");
    stdout
}

/// Runs `elephc --check` on `entry` of a written project and returns the stderr of the failure.
fn check_project_with_cli_expect_failure(
    prefix: &str,
    files: &[(&str, &str)],
    entry: &str,
) -> String {
    let dir = write_project(prefix, files);
    let check = elephc_cli_command(&dir)
        .arg("--check")
        .arg(dir.join(entry))
        .output()
        .expect("elephc CLI should run");
    let stderr = String::from_utf8_lossy(&check.stderr).into_owned();
    let _ = fs::remove_dir_all(dir);
    assert!(!check.status.success(), "the check should fail, got success: {stderr}");
    stderr
}

/// The exact Ivory repro of #1445: a readonly initialization at `Fixed.lfc:9:9` must not guard
/// the ordinary property write at `Counter.lfc:9:9`.
///
/// Both class files were lexed with the entry file's source identity, so the checker's readonly
/// guard, keyed by span, also fired on the `Counter` write and the program died with
/// "Cannot modify readonly property App\Fixed::$value" instead of printing `2`.
#[test]
fn test_autoloaded_readonly_write_does_not_guard_a_same_position_write_in_another_class() {
    let out = compile_project_with_cli_and_run(
        "elephc_autoload_span_readonly_lfc",
        &[
            ("composer.json", COMPOSER_PSR4),
            (
                "src/Fixed.lfc",
                r#"namespace App;

final class Fixed
{
    private readonly int $value;

    public function __construct(int $value)
    {
        $this->value = $value;
    }
}
"#,
            ),
            (
                "src/Counter.lfc",
                r#"namespace App;

final class Counter
{
    private int $total = 0;

    public function add(int $amount): int
    {
        $this->total = $this->total + $amount;
        return $this->total;
    }
}
"#,
            ),
            (
                "main.lfc",
                r#"use App\Counter;
use App\Fixed;

$fixed = new Fixed(1);
$counter = new Counter();
echo $counter->add(2), "\n";
"#,
            ),
        ],
        "main.lfc",
    );
    assert_eq!(out, "2\n");
}

/// The same leak in PHP class files, the other way round: the plain write is autoloaded FIRST,
/// and the readonly class still rejects a second write of its own property.
#[test]
fn test_autoloaded_plain_write_and_readonly_write_at_one_position_keep_their_own_guards() {
    let out = compile_and_run_files(
        &[
            ("composer.json", COMPOSER_PSR4),
            (
                "src/Counter.php",
                "<?php\nnamespace App;\nfinal class Counter {\n    private int $total = 0;\n    public function add(int $amount): int {\n        $this->total = $this->total + $amount;\n        return $this->total;\n    }\n}\n",
            ),
            (
                "src/Fixed.php",
                "<?php\nnamespace App;\nfinal class Fixed {\n    private readonly int $value;\n    public function set(int $value): int {\n        $this->value = $value;\n        return $this->value;\n    }\n}\n",
            ),
            (
                "main.php",
                "<?php\n$counter = new App\\Counter();\necho $counter->add(2), \":\", $counter->add(3), \":\";\n$fixed = new App\\Fixed();\necho $fixed->set(7), \":\";\ntry {\n    $fixed->set(8);\n} catch (Error $e) {\n    echo $e->getMessage();\n}\necho \"\\n\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "2:5:7:Cannot modify readonly property App\\Fixed::$value\n");
}

/// Two autoloaded class files whose retyping assignments sit at the same line and column are
/// told apart, so each file's binding decision lowers only its own statement.
///
/// With one shared source identity both `$x = "..."` retypes were the same `(span, name)` key and
/// the checker refused the program with "Cannot re-bind $x here" (the autoload part of #778).
#[test]
fn test_autoloaded_retypes_at_the_same_position_compile_and_run() {
    let out = compile_and_run_files(
        &[
            ("composer.json", COMPOSER_PSR4),
            (
                "src/A.php",
                "<?php\nnamespace App;\nclass A {\n    public function f() { $x = 0; echo $x; $x = \"s\"; echo $x, \"\\n\"; }\n}\n",
            ),
            (
                "src/B.php",
                "<?php\nnamespace App;\nclass B {\n    public function f() { $x = 1; echo $x; $x = \"t\"; echo $x, \"\\n\"; }\n}\n",
            ),
            ("main.php", "<?php\n(new App\\A())->f();\n(new App\\B())->f();\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "0s\n1t\n");
}

/// A checker error inside an autoloaded class file names that file. It used to print a bare
/// `error[7:16]`, which read as a position in the entry file.
#[test]
fn test_checker_error_in_an_autoloaded_file_names_the_file() {
    let stderr = check_project_with_cli_expect_failure(
        "elephc_autoload_span_diagnostic",
        &[
            ("composer.json", COMPOSER_PSR4),
            (
                "src/Broken.lfc",
                "namespace App;\n\nfinal class Broken\n{\n    public function run(): int\n    {\n        return new Missing();\n    }\n}\n",
            ),
            ("main.lfc", "use App\\Broken;\n\necho (new Broken())->run(), \"\\n\";\n"),
        ],
        "main.lfc",
    );
    assert!(
        stderr.contains("src/Broken.lfc:7:16]: Undefined class: App\\Missing"),
        "the diagnostic should name the autoloaded file: {stderr}"
    );
}
