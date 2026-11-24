//! Purpose:
//! The `elephc test` subcommand: finds the test files under a path, resolves the project
//! root, synthesizes the virtual entry that requires them, compiles it with the testing
//! prelude forced and compile-time discovery on, runs the binary, and returns its verdict.
//!
//! Called from:
//! - `crate::entry::run()` through `cli::Command::Test`.
//!
//! Key details:
//! - The entry is VIRTUAL: `<root>/.elephc-test.lfc` is never written. Its directory is what
//!   the Composer autoload registry reads, so `autoload` and `autoload-dev` from the root
//!   `composer.json` resolve test and project classes without a second manifest.
//! - The binary is built under the per-user cache, never beside the sources, in a directory
//!   of its own per invocation (`<cache>/elephc/test/<project hash>/<pid>/`): two concurrent
//!   runs of one project, with different filters, can never run each other's binary. The
//!   directory is removed after the run; one left behind by a compile error (which ends the
//!   process inside the pipeline) or a crash is removed by the next run, which deletes every
//!   sibling whose process is no longer alive.
//! - Compile options after the path are the ordinary ones; output-mode and target flags are
//!   refused up front because the runner always builds and runs a host executable.
//! - Isolation, timeout, filter and the JUnit path are not compiler options: they are baked
//!   into the synthesized driver's `Runner` constructor call by `discover::driver_source`.
//! - The child inherits stdio and the caller's working directory; its exit code is the
//!   command's, and a signal death becomes `128 + signal` with one diagnostic line.

mod args;
mod files;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) use args::{parse_args, TestCommand, HELP};

/// The virtual entry's file name, placed in the project root so the autoload registry and
/// `elephc.toml` discovery both start there. The `.lfc` extension keeps it tagless and
/// extension-enabled; the required test files keep their own per-file profile.
const ENTRY_FILE_NAME: &str = ".elephc-test.lfc";

/// The output stem under the cache directory; the binary is `<dir>/suite`.
const OUTPUT_STEM: &str = "suite";

/// Runs one `elephc test` invocation and returns the process exit code.
pub(crate) fn run(command: TestCommand) -> i32 {
    if command.help {
        print!("{HELP}");
        return 0;
    }
    match run_inner(command) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("elephc test: {error}");
            1
        }
    }
}

/// Discovers the files, builds the suite, runs it, and maps the child's status to a code.
fn run_inner(command: TestCommand) -> Result<i32, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let path = cwd.join(&command.path);
    if !path.exists() {
        return Err(format!("no such path '{}'", command.path));
    }
    let test_files = files::collect_test_files(&path)?;
    if test_files.is_empty() {
        return Err(format!(
            "no test files found under '{}' (looked for *Test.php and *Test.lfc)",
            command.path
        ));
    }
    let root = files::project_root(&path, &cwd);
    let entry_source = files::entry_source(&test_files);

    let logical_entry = root.join(ENTRY_FILE_NAME);
    let logical_entry = logical_entry
        .to_str()
        .ok_or("the project path is not valid UTF-8")?
        .to_string();
    let project_dir = project_build_dir(&root);
    prune_abandoned_builds(&project_dir);
    let build_dir = project_dir.join(std::process::id().to_string());
    std::fs::create_dir_all(&build_dir)
        .map_err(|error| format!("cannot create '{}': {error}", build_dir.display()))?;
    let output_stem = build_dir.join(OUTPUT_STEM);
    let output_stem = output_stem
        .to_str()
        .ok_or("the cache path is not valid UTF-8")?
        .to_string();

    let mut args = vec!["elephc".to_string(), logical_entry];
    args.extend(command.compile_args.iter().cloned());
    let mut config = crate::cli::parse_compile_args(&args);
    crate::entry::apply_project_ini(&mut config);
    crate::entry::emit_ini_override_warnings(&config);
    // The report path is resolved against the caller's directory now, so the suite writes
    // it where the user pointed even though the binary lives under the cache.
    let junit = command.junit.as_deref().map(|path| {
        cwd.join(path)
            .to_str()
            .map(str::to_owned)
            .unwrap_or_else(|| path.to_string())
    });
    let options = crate::testing_prelude::discover::DriverOptions {
        filter: command.filter.clone(),
        isolate: command.isolate,
        timeout: command.timeout,
        junit,
    };
    // A compile error ends the process inside the pipeline with the compiler's own
    // diagnostics and exit code; only a successful build returns here.
    crate::pipeline::compile_test(config, &entry_source, &output_stem, &options);

    let binary = build_dir.join(OUTPUT_STEM);
    let status = Command::new(&binary).status();
    let _ = std::fs::remove_dir_all(&build_dir);
    let status = status.map_err(|error| format!("cannot run '{}': {error}", binary.display()))?;
    Ok(exit_code(status))
}

/// Maps the suite's exit status to this command's: the code as is, or `128 + signal` with
/// a diagnostic when the suite died of a signal (a native fatal, an abort, a kill).
fn exit_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            eprintln!("elephc test: the suite was killed by signal {signal}");
            return 128 + signal;
        }
    }
    1
}

/// The per-project parent of the per-invocation build directories, keyed by a hash of the
/// project root so different projects never share one.
fn project_build_dir(root: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    root.hash(&mut hasher);
    crate::runtime_cache::runtime_cache_dir()
        .join("test")
        .join(format!("{:016x}", hasher.finish()))
}

/// Removes the build directories under `project_dir` whose owning process (the directory's
/// name is its pid) is no longer alive: what a compile error or a crash left behind.
fn prune_abandoned_builds(project_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(project_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(pid) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<i32>().ok())
        else {
            continue;
        };
        if pid != std::process::id() as i32 && !process_is_alive(pid) {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// Whether a process with this pid exists: `kill(pid, 0)` succeeds, or fails only because
/// the process belongs to someone else (`EPERM`).
fn process_is_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    // SAFETY: signal 0 performs only the existence and permission check, delivering nothing.
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the per-invocation build directories of `elephc test`.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - A pid far above any real pid stands for a process that no longer exists.

    use super::*;

    /// Pruning removes directories of dead processes and keeps the live one and anything
    /// that is not a pid-named directory.
    #[test]
    fn prunes_only_directories_of_dead_processes() {
        let project_dir = std::env::temp_dir().join(format!(
            "elephc_test_prune_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&project_dir);
        let dead = project_dir.join("2147483000");
        let live = project_dir.join(std::process::id().to_string());
        let other = project_dir.join("notes");
        for dir in [&dead, &live, &other] {
            std::fs::create_dir_all(dir).unwrap();
        }
        prune_abandoned_builds(&project_dir);
        assert!(!dead.exists(), "a dead process's build directory is removed");
        assert!(live.exists(), "the running process's build directory is kept");
        assert!(other.exists(), "a directory that is not a pid is left alone");
        let _ = std::fs::remove_dir_all(&project_dir);
    }

    /// `kill(pid, 0)` reports this process alive and an impossible pid dead.
    #[test]
    fn process_liveness_check() {
        assert!(process_is_alive(std::process::id() as i32));
        assert!(!process_is_alive(2147483000));
        assert!(!process_is_alive(0));
    }
}
