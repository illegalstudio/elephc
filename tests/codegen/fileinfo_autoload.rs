//! Purpose:
//! Exercises fileinfo activation when only Composer-autoloaded code uses the extension.
//!
//! Called from:
//! - The codegen integration harness on every executable supported target.
//!
//! Key details:
//! - Uses the production CLI and resolver without a forced bridge flag or entry-file reference.
//! - Reuses the managed native cache and removes the fixture even after assertion failures.

use crate::support::*;
use std::path::PathBuf;

/// Owns the Composer fixture until compilation and execution assertions finish.
struct FileinfoAutoloadProject(PathBuf);

impl Drop for FileinfoAutoloadProject {
    /// Removes only this test's project on success and unwinding.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Composer-only object and procedural references must inject and link fileinfo automatically.
#[test]
fn test_fileinfo_composer_autoload_activation() {
    if fileinfo_native::skip("test_fileinfo_composer_autoload_activation") { return; }
    ensure_cli_bridge_staticlibs(&["elephc_fileinfo"]);
    let project = FileinfoAutoloadProject(make_cli_test_dir("elephc_fileinfo_autoload"));
    let dir = &project.0;
    fs::create_dir(dir.join("src")).unwrap();
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::write(dir.join("elephc.toml"), include_str!("../../examples/fileinfo/elephc.toml")).unwrap();
    fs::write(dir.join("elephc.lock"), include_str!("../../examples/fileinfo/elephc.lock")).unwrap();
    fs::write(dir.join("src/MimeProbe.php"), r#"<?php
namespace App;
class MimeProbe {
    public function objectMime(string $data): string|false {
        $f = new \finfo(FILEINFO_MIME_TYPE);
        return $f->buffer($data);
    }
    public function functionMime(string $data): string|false {
        $f = \finfo_open(FILEINFO_MIME_TYPE);
        if ($f === false) { throw new \RuntimeException("Could not open fileinfo"); }
        return \finfo_buffer($f, $data);
    }
}
"#).unwrap();
    fs::write(dir.join("main.php"), r#"<?php
$probe = new App\MimeProbe();
echo $probe->objectMime("hello"), "\n";
echo $probe->functionMime("hello"), "\n";
"#).unwrap();
    let cache = managed_native_cache_root().expect("fileinfo managed native cache");
    let mut command = elephc_cli_command(dir);
    configure_prebuilt_cli_bridges(&mut command);
    let compiled = command.args(["--target", target().as_str(), "main.php"])
        .env("ELEPHC_NATIVE_CACHE", cache).output().expect("run fileinfo compiler");
    assert!(compiled.status.success(), "autoloaded fileinfo compilation failed: {}",
        String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary_with_args(&dir.join("main"), dir, &[]);
    assert!(output.status.success(), "autoloaded fileinfo execution failed: {}",
        String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "text/plain\ntext/plain\n");
}
