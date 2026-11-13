//! Purpose:
//! Owns isolated CLI projects for strict semantic fixtures.
//!
//! Called from:
//! - Strict feature tests covering source rejection and native execution.
//!
//! Key details:
//! - Drop removes only this fixture's directory, including after assertion failures.

use super::*;
use std::path::PathBuf;
use std::process::Output;

/// A PHP entry with optional include or Composer/autoload companions.
pub(crate) struct StrictFixture {
    directory: PathBuf,
}

impl StrictFixture {
    /// Creates one task-owned fixture using the established unique CLI directory helper.
    pub(crate) fn new(source: &str) -> Self {
        let fixture = Self { directory: make_cli_test_dir("elephc_strict_semantics") };
        fixture.file("main.php", source);
        fixture
    }

    /// Adds a companion file and any directories contained within this fixture.
    pub(crate) fn file(&self, name: &str, contents: &str) {
        let path = self.directory.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    /// Compiles through the real CLI with explicit semantic options.
    fn compile(&self, flags: &[&str]) -> Output {
        elephc_cli_command(&self.directory).args(flags).arg(self.directory.join("main.php"))
            .output().expect("run strict fixture compiler")
    }

    /// Asserts a compile diagnostic and includes the source when a refusal is missing.
    pub(crate) fn assert_error(&self, flags: &[&str], expected: &str) {
        let output = self.compile(flags);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "strict fixture unexpectedly compiled: {}",
            fs::read_to_string(self.directory.join("main.php")).unwrap());
        assert!(errors.contains(expected), "expected {expected:?}, received: {errors}");
    }

    /// Compiles and executes a fixture, requiring successful compilation and execution.
    pub(crate) fn run(&self, flags: &[&str]) -> String {
        let output = self.compile(flags);
        assert!(output.status.success(), "strict fixture compilation failed: {}",
            String::from_utf8_lossy(&output.stderr));
        let output = run_binary(&self.directory.join("main"), &self.directory);
        assert!(output.status.success(), "strict fixture execution failed: {}",
            String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for StrictFixture {
    /// Removes the exact directory created by this fixture on success and unwinding.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
