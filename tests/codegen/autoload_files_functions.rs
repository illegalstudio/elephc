//! Purpose:
//! Exercises unqualified function calls in Composer-autoloaded class files that resolve to
//! functions declared by `autoload.files` entries or by the entry program.
//!
//! Called from:
//! - The codegen integration harness on every executable supported target.
//!
//! Key details:
//! - PHP resolves an unqualified call in a namespace to the namespaced function when it exists,
//!   else to the global one; the autoload pass resolves each file separately, so the functions
//!   the rest of the program declares must reach that resolution.
//! - Uses the production CLI and resolver, and removes the fixture even after assertion failures.

use crate::support::*;
use std::path::PathBuf;

/// Owns the Composer fixture until compilation and execution assertions finish.
struct AutoloadFilesProject(PathBuf);

impl Drop for AutoloadFilesProject {
    /// Removes only this test's project on success and unwinding.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A class file calls `autoload.files` functions unqualified: a global one (namespace fallback),
/// a namespaced one from a later `files` entry, and a namespaced one that shadows a global.
#[test]
fn test_autoloaded_class_calls_functions_from_autoload_files() {
    let project = AutoloadFilesProject(make_cli_test_dir("elephc_autoload_files_functions"));
    let dir = &project.0;
    fs::create_dir(dir.join("src")).unwrap();
    fs::write(dir.join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"src/"},"files":["fn.php","more.php"]}}"#).unwrap();
    fs::write(dir.join("fn.php"), r#"<?php
function gfn(int $x): int { return $x + 1; }
function shadowed(): string { return "global"; }
"#).unwrap();
    fs::write(dir.join("more.php"), r#"<?php
namespace App;
function helper(): string { return "ns-" . gfn(1); }
function shadowed(): string { return "app"; }
"#).unwrap();
    fs::write(dir.join("src/A.php"), r#"<?php
namespace App;
final class A {
    public static function go(): string { return gfn(1) . "|" . helper() . "|" . shadowed() . "|" . \shadowed(); }
}
"#).unwrap();
    fs::write(dir.join("main.php"), "<?php\necho App\\A::go(), \"\\n\";\n").unwrap();
    let compiled = elephc_cli_command(dir)
        .args(["--target", target().as_str(), "main.php"])
        .output()
        .expect("run compiler");
    assert!(compiled.status.success(), "autoload files compilation failed: {}",
        String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary_with_args(&dir.join("main"), dir, &[]);
    assert!(output.status.success(), "autoload files execution failed: {}",
        String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "2|ns-2|app|global\n");
}

/// A class file calls functions the entry program declares: one after the call site and one in
/// a `require`d file, both through the namespace fallback to the global function.
#[test]
fn test_autoloaded_class_calls_functions_from_entry_program() {
    let project = AutoloadFilesProject(make_cli_test_dir("elephc_autoload_entry_functions"));
    let dir = &project.0;
    fs::create_dir(dir.join("src")).unwrap();
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::write(dir.join("helpers.php"), "<?php\nfunction inc(): string { return 'included'; }\n")
        .unwrap();
    fs::write(dir.join("src/A.php"), r#"<?php
namespace App;
final class A {
    public static function go(): string { return late() . "|" . inc(); }
}
"#).unwrap();
    fs::write(dir.join("main.php"), r#"<?php
require __DIR__ . '/helpers.php';
echo App\A::go(), "\n";
function late(): string { return 'late'; }
"#).unwrap();
    let compiled = elephc_cli_command(dir)
        .args(["--target", target().as_str(), "main.php"])
        .output()
        .expect("run compiler");
    assert!(compiled.status.success(), "entry functions compilation failed: {}",
        String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary_with_args(&dir.join("main"), dir, &[]);
    assert!(output.status.success(), "entry functions execution failed: {}",
        String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "late|included\n");
}
