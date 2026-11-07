//! Purpose:
//! `elephc usage-record <manifest> -- <command>…`: runs a php command with a recorder that appends
//! to a usage manifest what php loads, asks the autoloader for, and includes.
//!
//! Called from:
//! - `crate::cli::parse_args()` (the `usage-record` selector) and `crate::cli_entry`.
//!
//! Key details:
//! - The recorder is a php file written next to the manifest and injected with
//!   `auto_prepend_file` through an extra ini directory (`PHP_INI_SCAN_DIR`), so any php process
//!   the command starts (a console, `php -S`, a test runner) records, its children included.
//! - Every process appends its lines at shutdown under an exclusive lock; the compiler reads the
//!   manifest with duplicates folded (`closed_world::UsageManifest`). The header is written once,
//!   when the manifest does not exist yet, so successive runs (cold caches, warm caches, other
//!   scenarios) add up.
//! - The recorder is run by php, never parsed by elephc.

use std::path::{Path, PathBuf};
use std::process;

/// Usage text of the subcommand.
pub const HELP: &str = "\
Usage: elephc usage-record <manifest> -- <command> [args...]

Runs <command> with a php recorder that appends to <manifest> every class php loads or asks its
autoloader for, and every file it includes. Run it once per scenario the build must cover, with
cold and warm caches; then build with --closed-world --usage-manifest <manifest>.
";

/// The recorder php runs before each script of the recorded command.
const RECORDER: &str = r#"<?php
// elephc usage recorder: appends to ELEPHC_USAGE_RECORD what this php process used.
(static function (): void {
    $out = getenv('ELEPHC_USAGE_RECORD');
    if ($out === false || $out === '') {
        return;
    }
    $asked = [];
    spl_autoload_register(static function (string $class) use (&$asked): void {
        $asked[$class] = true;
    }, true, true);
    register_shutdown_function(static function () use ($out, &$asked): void {
        $lines = [];
        foreach (array_keys($asked) as $name) {
            $lines[] = 'class ' . $name;
        }
        foreach (array_merge(get_declared_classes(), get_declared_interfaces(), get_declared_traits()) as $name) {
            if (!(new \ReflectionClass($name))->isInternal()) {
                $lines[] = 'class ' . $name;
            }
        }
        foreach (get_included_files() as $file) {
            $lines[] = 'file ' . $file;
        }
        file_put_contents($out, implode("\n", array_unique($lines)) . "\n", FILE_APPEND | LOCK_EX);
    });
})();
"#;

/// One validated `elephc usage-record` invocation.
pub struct UsageRecordCommand {
    manifest: PathBuf,
    command: Vec<String>,
}

/// Parses `<manifest> -- <command> [args…]`.
pub fn parse_args(args: &[String]) -> Result<UsageRecordCommand, String> {
    let manifest = args.first().filter(|arg| !arg.starts_with('-')).ok_or("missing <manifest>")?;
    if args.get(1).map(String::as_str) != Some("--") {
        return Err("expected `--` before the command to record".to_string());
    }
    let command = args[2..].to_vec();
    if command.is_empty() {
        return Err("missing the command to record".to_string());
    }
    Ok(UsageRecordCommand { manifest: manifest.into(), command })
}

/// Runs the command with the recorder and returns its exit status.
pub fn run(command: UsageRecordCommand) -> i32 {
    match record(&command) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("elephc usage-record: {error}");
            1
        }
    }
}

fn record(command: &UsageRecordCommand) -> Result<i32, String> {
    let manifest = absolute(&command.manifest)?;
    if !manifest.exists() {
        std::fs::write(&manifest, format!("{}\n", crate::closed_world::USAGE_MANIFEST_HEADER))
            .map_err(|err| format!("cannot create {}: {err}", manifest.display()))?;
    }
    let dir = std::env::temp_dir().join(format!("elephc-usage-record-{}", process::id()));
    let ini_dir = dir.join("ini");
    std::fs::create_dir_all(&ini_dir).map_err(|err| format!("cannot create {}: {err}", ini_dir.display()))?;
    let recorder = dir.join("recorder.php");
    std::fs::write(&recorder, RECORDER).map_err(|err| format!("cannot write the recorder: {err}"))?;
    std::fs::write(ini_dir.join("zz-elephc-usage.ini"), format!("auto_prepend_file={}\n", recorder.display()))
        .map_err(|err| format!("cannot write the recorder ini: {err}"))?;
    // An empty entry keeps the scan directory php was built with.
    let scan = match std::env::var("PHP_INI_SCAN_DIR") {
        Ok(existing) if !existing.is_empty() => format!("{existing}:{}", ini_dir.display()),
        _ => format!(":{}", ini_dir.display()),
    };
    let status = process::Command::new(&command.command[0])
        .args(&command.command[1..])
        .env("PHP_INI_SCAN_DIR", scan)
        .env("ELEPHC_USAGE_RECORD", &manifest)
        .status()
        .map_err(|err| format!("cannot run {}: {err}", command.command[0]));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(status?.code().unwrap_or(1))
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|dir| dir.join(path))
        .map_err(|err| format!("cannot resolve {}: {err}", path.display()))
}
