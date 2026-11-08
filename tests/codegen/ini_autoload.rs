//! Purpose:
//! Exercises the OPcache/INI and version preludes when only Composer-autoloaded code calls them.
//!
//! Called from:
//! - The codegen integration harness on every executable supported target.
//!
//! Key details:
//! - The entry file names no INI or OPcache function; the autoloaded class reads and writes a
//!   directive and asks the cache, the way symfony/config and symfony/var-exporter do.
//! - The expected output is PHP 8.5's for the same project under the CLI SAPI.
//! - Uses the production CLI and resolver, and removes the fixture even after assertion failures.

use crate::support::*;
use std::path::PathBuf;

/// Owns the Composer fixture until compilation and execution assertions finish.
struct IniAutoloadProject(PathBuf);

impl Drop for IniAutoloadProject {
    /// Removes only this test's project on success and unwinding.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Composer-only `ini_*`, `opcache_*` and `php_sapi_name` calls keep their prelude functions.
#[test]
fn test_ini_and_opcache_composer_autoload_activation() {
    let project = IniAutoloadProject(make_cli_test_dir("elephc_ini_autoload"));
    let dir = &project.0;
    fs::create_dir(dir.join("src")).unwrap();
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::write(dir.join("src/Settings.php"), r#"<?php
namespace App;
final class Settings {
    public static function report(): void {
        var_dump(\ini_get("display_errors"));
        var_dump(\ini_set("display_errors", "0"));
        var_dump(\ini_get("display_errors"));
        \ini_restore("display_errors");
        var_dump(\ini_get("display_errors"));
        var_dump(\ini_get("opcache.enable_cli"));
        var_dump(\is_array(\ini_get_all("date")));
        var_dump(\opcache_get_status());
        var_dump(\function_exists('opcache_reset'));
        var_dump(\php_sapi_name());
    }
}
"#).unwrap();
    fs::write(dir.join("main.php"), "<?php\nApp\\Settings::report();\n").unwrap();
    let compiled = elephc_cli_command(dir)
        .args(["--target", target().as_str(), "main.php"])
        .output()
        .expect("run compiler");
    assert!(compiled.status.success(), "autoloaded INI compilation failed: {}",
        String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary_with_args(&dir.join("main"), dir, &[]);
    assert!(output.status.success(), "autoloaded INI execution failed: {}",
        String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "string(1) \"1\"\nstring(1) \"1\"\nstring(1) \"0\"\nstring(1) \"1\"\nstring(1) \"0\"\n\
         bool(true)\nbool(false)\nbool(true)\nstring(3) \"cli\"\n",
    );
}
