//! Purpose:
//! Retains bounded output when a compiled CLI fixture stops making progress.
//!
//! Called from:
//! - Inline HTML and resource-id eval integration tests.
//!
//! Key details:
//! - Compilation keeps its existing budget; only the small generated program is timed.
//! - Both pipes are drained concurrently, retaining at most 64 KiB from each stream.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

static DIAGNOSTIC_ID: AtomicU64 = AtomicU64::new(0);

/// Preserves a stalled CLI fixture's source and assembly for remote diagnosis in GitHub Actions.
fn preserve_timeout(program: &Path, stdout: &[u8], stderr: &[u8]) {
    if !matches!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true")) { return; }
    let Some(workspace) = std::env::var_os("GITHUB_WORKSPACE") else { return; };
    let directory = std::path::PathBuf::from(workspace).join("target/codegen-diagnostics");
    let id = DIAGNOSTIC_ID.fetch_add(1, Ordering::Relaxed);
    let stem = format!("cli-timeout-{}-{id}", std::process::id());
    let saved = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&directory)?;
        for extension in ["php", "s"] {
            let source = program.with_extension(extension);
            if source.is_file() {
                std::fs::copy(&source, directory.join(format!("{stem}.{extension}")))?;
            }
        }
        let current = std::thread::current();
        let test = current.name().unwrap_or("unnamed");
        std::fs::write(directory.join(format!("{stem}.txt")), format!(
            "test: {test}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(stdout), String::from_utf8_lossy(stderr)))
    })();
    if let Err(error) = saved { eprintln!("Could not save CLI fixture diagnostics: {error}"); }
}

/// Drains a pipe without allowing a looping fixture to exhaust the test runner's memory.
fn capture(mut pipe: impl Read + Send + 'static) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            let count = match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => panic!("failed to read fixture output: {error}"),
            };
            let retained = count.min((64 * 1024usize).saturating_sub(output.len()));
            output.extend_from_slice(&chunk[..retained]);
        }
        output
    })
}

/// Runs a small compiled fixture and includes partial output if it exceeds 30 seconds.
pub fn output(command: &mut Command) -> Output {
    let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        .expect("failed to run compiled fixture");
    let stdout = capture(child.stdout.take().expect("fixture stdout pipe"));
    let stderr = capture(child.stderr.take().expect("fixture stderr pipe"));
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("failed to poll compiled fixture") { break status; }
        if started.elapsed() >= Duration::from_secs(30) {
            timed_out = true;
            let _ = child.kill();
            break child.wait().expect("failed to reap timed-out fixture");
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout.join().expect("fixture stdout reader");
    let stderr = stderr.join().expect("fixture stderr reader");
    if timed_out { preserve_timeout(Path::new(command.get_program()), &stdout, &stderr); }
    assert!(!timed_out, "compiled fixture exceeded 30 seconds\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&stdout), String::from_utf8_lossy(&stderr));
    Output { status, stdout, stderr }
}
