//! Purpose:
//! Launches an eval-backed PHP session from a verified, per-user cached native host.
//!
//! Called from:
//! - Top-level `elephc repl` dispatch in `main` and the private host build subprocess.
//!
//! Key details:
//! - Generated source has a logical path in the invoking directory for project discovery.
//! - Only the bootstrap is compiled; all submitted PHP executes through dynamic eval.

mod cache;
mod command;

pub(crate) use command::{parse_args, ReplCommand, HELP};
pub(crate) const HOST_SOURCE: &str = include_str!("host.php");

use std::path::Path;
use std::process::Command;

/// Runs the requested session or builds its host in an isolated compiler subprocess.
pub(crate) fn run(command: ReplCommand) -> i32 {
    if command.help { print!("{HELP}"); return 0; }
    match run_inner(command) {
        Ok(code) => code,
        Err(error) => { eprintln!("elephc repl: {error}"); 1 }
    }
}

/// Resolves project settings before choosing a cache entry and launching fresh PHP state.
fn run_inner(command: ReplCommand) -> Result<i32, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let logical_source = cwd.join(".elephc-repl.php");
    let source = logical_source.to_str().ok_or("REPL project path is not UTF-8")?;
    let mut args = vec!["elephc".to_string(), source.to_string(), "--quiet".to_string()];
    args.extend(command.compile_args.clone());
    let mut config = crate::cli::parse_compile_args(&args);
    crate::apply_project_ini(&mut config);
    // Preloading belongs to a different execution model: it would compile arbitrary PHP
    // into the host and require its whole include graph in the executable cache identity.
    if config.ini_overrides.iter().rev().find(|(key, _)| key == "opcache.preload")
        .is_some_and(|(_, value)| !value.is_empty()) {
        return Err("opcache.preload is not supported in the REPL; use require_once in the session".into());
    }
    crate::emit_ini_override_warnings(&config);
    if let Some(output) = command.build_output {
        crate::pipeline::compile_repl(config, &output);
        return Ok(0);
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let root = crate::runtime_cache::runtime_cache_dir().join("repl");
    let identity = cache::identity(&executable, &cwd, &command.compile_args,
        config.php_version.version_id(), &config.ini_overrides)?;
    let host = cache::prepare(&root, &identity, &executable, &command.compile_args, command.quiet)?;
    let mut child = Command::new(host);
    // Give PHP a stable argv[0] rather than the cache's implementation path.
    use std::os::unix::process::CommandExt;
    child.arg0("elephc repl");
    if command.quiet { child.env("ELEPHC_REPL_QUIET", "1"); }
    else { child.env_remove("ELEPHC_REPL_QUIET"); }
    let history = root.join("history");
    child.env("ELEPHC_REPL_HISTORY", if command.no_history { Path::new("") } else { &history });
    // Replace the launcher so terminal signals and PHP exit statuses reach the caller directly.
    Err(format!("cannot start cached host: {}", child.exec()))
}
