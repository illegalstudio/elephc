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

/// Saves source and assembly when a heap-clean execution produces incorrect PHP output.
pub(crate) fn preserve_stdout_mismatches(
    source: &str, assembly: &str, outputs: &[ProgramOutput], expected: &[&str],
) {
    let mut report = String::new();
    for (index, (output, expected)) in outputs.iter().zip(expected).enumerate() {
        if output.stdout != *expected {
            report.push_str(&format!("mode={index}\nstdout:\n{}\nexpected:\n{}\nstderr:\n{}\n",
                output.stdout, expected, output.stderr));
        }
    }
    if !report.is_empty() { save_failure("stdout", Some(source), assembly, &report); }
}

/// Preserves failing heap-debug executions before their temporary source directory disappears.
pub(super) fn preserve_heap_failure(source: &str, assembly: &str, output: &ProgramOutput) {
    if heap_execution_failed(output) {
        save_failure("heap", Some(source), assembly,
            &format!("stdout:\n{}\nstderr:\n{}", output.stdout, output.stderr));
    }
}

/// Keeps every failed argument run while saving their shared source and assembly only once.
pub(super) fn preserve_heap_failures(
    source: &str, assembly: &str, outputs: &[ProgramOutput], argvs: &[&[&str]],
) {
    let mut report = String::new();
    for (index, output) in outputs.iter().enumerate() {
        if heap_execution_failed(output) {
            report.push_str(&format!("mode={index} arguments={:?}\nstdout:\n{}\nstderr:\n{}\n",
                argvs.get(index), output.stdout, output.stderr));
        }
    }
    if !report.is_empty() { save_failure("heap", Some(source), assembly, &report); }
}

/// Recognizes process errors and non-clean heap reports without changing test assertions.
fn heap_execution_failed(output: &ProgramOutput) -> bool {
    !output.success || output.stderr.lines().any(|line|
        line.contains("HEAP DEBUG: leak summary:") && !line.ends_with("clean"))
}

/// Saves a failed manual native ABI fixture before its directory guard removes the binary.
pub(super) fn preserve_native_fixture_failure(assembly: &str, output: &std::process::Output) {
    if output.status.success() { return; }
    save_failure("native", None, assembly, &format!(
        "status:{}\nstdout:\n{}\nstderr:\n{}", output.status,
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)));
}

/// Preserves an assembler's exact stdin, including runtime assembly when that is what failed.
pub(super) fn preserve_assembler_failure(assembly: &str) {
    save_failure("assembler", None, assembly, "Assembler failed; see the job's stderr.");
}

/// Saves corrupt output before UTF-8 decoding aborts the ordinary fixture result path.
pub(super) fn preserve_invalid_utf8_output(assembly: &str, output: &std::process::Output) {
    let stdout = &output.stdout[..output.stdout.len().min(256)];
    let stderr = &output.stderr[..output.stderr.len().min(4096)];
    save_failure("utf8", None, assembly, &format!(
        "stdout bytes (first 256): {stdout:?}\nstderr (first 4096 bytes):\n{}",
        String::from_utf8_lossy(stderr)));
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
