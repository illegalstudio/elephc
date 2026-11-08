//! Purpose:
//! Exercises the core WeakMap/WeakReference prelude when only Composer-autoloaded code uses it.
//!
//! Called from:
//! - The codegen integration harness on every executable supported target.
//!
//! Key details:
//! - The entry file never names `WeakMap` or `WeakReference`; the autoloaded class types a
//!   promoted property with both, the way revolt/event-loop's `DriverSuspension` does.
//! - Uses the production CLI and resolver, and removes the fixture even after assertion failures.

use crate::support::*;
use std::path::PathBuf;

/// Owns the Composer fixture until compilation and execution assertions finish.
struct WeakMapAutoloadProject(PathBuf);

impl Drop for WeakMapAutoloadProject {
    /// Removes only this test's project on success and unwinding.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Composer-only `WeakMap` / `WeakReference` types and constructions keep their prelude classes.
#[test]
fn test_weak_map_composer_autoload_activation() {
    let project = WeakMapAutoloadProject(make_cli_test_dir("elephc_weak_map_autoload"));
    let dir = &project.0;
    fs::create_dir(dir.join("src")).unwrap();
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::write(dir.join("src/Registry.php"), r#"<?php
namespace App;
final class Registry {
    private ?\WeakReference $last = null;
    public function __construct(private readonly \WeakMap $seen = new \WeakMap()) {}
    public function remember(object $key, string $label): void {
        $this->seen[$key] = $label;
        $this->last = \WeakReference::create($key);
    }
    public function count(): int { return \count($this->seen); }
    public function lastIs(object $key): bool { return $this->last?->get() === $key; }
}
"#).unwrap();
    fs::write(dir.join("main.php"), r#"<?php
$registry = new App\Registry();
$a = new stdClass();
$registry->remember($a, "a");
$registry->remember(new stdClass(), "temporary");
echo $registry->count(), $registry->lastIs($a) ? "|last" : "|other", "\n";
"#).unwrap();
    let compiled = elephc_cli_command(dir)
        .args(["--target", target().as_str(), "main.php"])
        .output()
        .expect("run compiler");
    assert!(compiled.status.success(), "autoloaded WeakMap compilation failed: {}",
        String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary_with_args(&dir.join("main"), dir, &[]);
    assert!(output.status.success(), "autoloaded WeakMap execution failed: {}",
        String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1|other\n");
}
