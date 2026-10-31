//! Purpose:
//! Keeps failing CI fixture inputs and assembly available after temporary directories are removed.
//!
//! Called from:
//! - Heap-debug compiler helpers and the assembler runner.
//!
//! Key details:
//! - Writes only in GitHub Actions, under the workspace's target directory.
//! - Best-effort diagnostics never replace the original test failure.

use super::ProgramOutput;
use std::sync::atomic::{AtomicU64, Ordering};

static DIAGNOSTIC_ID: AtomicU64 = AtomicU64::new(0);

/// Preserves failing heap-debug executions before their temporary source directory disappears.
pub(super) fn preserve_heap_failure(source: &str, assembly: &str, output: &ProgramOutput) {
    let leaked = output.stderr.lines().any(|line|
        line.contains("HEAP DEBUG: leak summary:") && !line.ends_with("clean"));
    if !output.success || leaked {
        save_failure("heap", Some(source), assembly,
            &format!("stdout:\n{}\nstderr:\n{}", output.stdout, output.stderr));
    }
}

/// Preserves an assembler's exact stdin, including runtime assembly when that is what failed.
pub(super) fn preserve_assembler_failure(assembly: &str) {
    save_failure("assembler", None, assembly, "Assembler failed; see the job's stderr.");
}

/// Saves one uniquely named diagnostic without making artifact I/O another test requirement.
fn save_failure(kind: &str, source: Option<&str>, assembly: &str, output: &str) {
    if !matches!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true")) { return; }
    let Some(workspace) = std::env::var_os("GITHUB_WORKSPACE") else { return; };
    let directory = std::path::PathBuf::from(workspace).join("target/codegen-diagnostics");
    let id = DIAGNOSTIC_ID.fetch_add(1, Ordering::Relaxed);
    let stem = format!("{kind}-{}-{id}", std::process::id());
    let current = std::thread::current();
    let test = current.name().unwrap_or("unnamed");
    let saved = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&directory)?;
        std::fs::write(directory.join(format!("{stem}.s")), assembly)?;
        if let Some(source) = source {
            std::fs::write(directory.join(format!("{stem}.php")), source)?;
        }
        std::fs::write(directory.join(format!("{stem}.txt")), format!("test: {test}\n{output}"))
    })();
    if let Err(error) = saved { eprintln!("Could not save fixture diagnostics: {error}"); }
}
