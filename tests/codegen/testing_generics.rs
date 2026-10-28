//! Purpose:
//! Regresses generic class specialization reached only from native test methods.
//!
//! Called from:
//! - `cargo test --test codegen_tests testing_generics`.
//!
//! Key details:
//! - The real CLI appends a terminating test driver before specialization runs.
//! - Generic construction is reached exclusively through discovered tests.

use crate::support::*;

/// Runs a standalone native suite and removes its artifacts before checking the result.
fn run_generic_suite(source: &str, files: &[(&str, &str)]) -> (String, String, i32) {
    let dir = make_cli_test_dir("elephc_cli_test_generics");
    fs::write(dir.join("CellTest.lfc"), source).unwrap();
    for (name, contents) in files {
        fs::write(dir.join(name), contents).unwrap();
    }
    let result = elephc_cli_command(&dir)
        .args(["test", "CellTest.lfc"])
        .output();
    fs::remove_dir_all(&dir).expect("remove native generic suite artifacts");
    let output = result.expect("run native generic suite");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

/// A namespaced buffer owner used only by a test retains its concrete constructor and methods.
#[test]
fn test_cli_test_generic_cell_specialization() {
    let (stdout, stderr, code) = run_generic_suite(
        r#"namespace Ivory\Tests\Memory;
use Elephc\Testing\TestCase;
final class Cell<T> {
    private buffer<T> $data;
    public function __construct() { $this->data = buffer_new<T>(1); }
    public function set(T $value): void { $this->data[0] = $value; }
    public function get(): T { return $this->data[0]; }
}
final class LocalCellTest extends TestCase {
    public function testCell(): void {
        $cell = new Cell<int>();
        $cell->set(17);
        $this->assertSameInt(17, $cell->get());
    }
}
"#,
        &[],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert_eq!(stdout, "ok   Ivory\\Tests\\Memory\\LocalCellTest::testCell (1 assertion)\n\nOK (1 test, 1 assertion)\n");
    assert!(!stderr.contains("Unreachable code"), "{stderr}");
}

/// Autoloaded owners retain inferred and transitively requested class specializations.
#[test]
fn test_cli_test_generic_autoloaded_inferred_specializations() {
    let (stdout, stderr, code) = run_generic_suite(
        r#"namespace App\Tests;
use Elephc\Testing\TestCase;
use App\Owner;
final class CellTest extends TestCase {
    public function testOwners(): void {
        $number = new Owner(17);
        $text = new Owner('hello');
        $this->assertSameInt(17, $number->get());
        $this->assertSameString('hello', $text->get());
    }
}
"#,
        &[
            ("composer.json", r#"{"autoload":{"psr-4":{"App\\":"./"}}}"#),
            ("Owner.lfc", r#"namespace App;
final class Cell<T> {
    private T $value;
    public function __construct(T $value) { $this->value = $value; }
    public function get(): T { return $this->value; }
}
final class Owner<T> {
    private Cell<T> $cell;
    public function __construct(T $value) { $this->cell = new Cell<T>($value); }
    public function get(): T { return $this->cell->get(); }
}
"#),
        ],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert_eq!(stdout, "ok   App\\Tests\\CellTest::testOwners (2 assertions)\n\nOK (1 test, 2 assertions)\n");
    assert!(!stderr.contains("Unreachable code"), "{stderr}");
}
