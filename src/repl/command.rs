//! Purpose:
//! Parses the small REPL option surface independently from compilation-only modes.
//!
//! Called from:
//! - `crate::cli::parse_args` for an exact first argument of `repl`.
//!
//! Key details:
//! - Compile options are validated by the ordinary CLI before any cache lookup.
//! - Host building is internal and never accepts a caller-supplied PHP program.

pub(crate) const HELP: &str = "Usage: elephc repl [OPTIONS]\n\n\
Run interactive PHP through eval with persistent variables and declarations.\n\
The native session host is compiled on first use and cached under elephc/repl.\n\n\
Options:\n\
  --php-version VERSION  Select PHP compatibility (otherwise use the project profile)\n\
  --strict-php           Hide elephc extensions from evaluated code\n\
  --with-<capability>    Enable an optional eval capability, e.g. regex or pdo\n\
  --ini KEY=VALUE        Override a project INI setting (repeatable)\n\
  --heap-size=BYTES      Set the session heap size\n\
  --heap-debug           Enable runtime heap diagnostics\n\
  --gc-stats             Print runtime GC statistics at exit\n\
  --no-history          Disable history loading and persistence\n\
  --quiet, -q           Hide the banner and cache-build notice\n\
  --help, -h            Show this help\n\n\
Enter PHP without <?php. Expressions display their values; incomplete input\n\
continues at ... . Use :help, :quit, Ctrl-C to cancel input, or Ctrl-D to exit.\n\
Piped input has no prompts or history. Each launch creates a new session.\n";

/// User-facing launch settings and a private compiler-child output destination.
#[derive(Debug, Default)]
pub(crate) struct ReplCommand {
    pub(crate) compile_args: Vec<String>,
    pub(crate) quiet: bool,
    pub(crate) no_history: bool,
    pub(crate) help: bool,
    pub(crate) build_output: Option<String>,
}

/// Accepts only options meaningful for a native host session, leaving values to the compiler.
pub(crate) fn parse_args(args: &[String]) -> Result<ReplCommand, String> {
    if args.iter().any(|arg| matches!(arg.as_str(), "--help" | "-h")) {
        return Ok(ReplCommand { help: true, ..Default::default() });
    }
    let mut command = ReplCommand::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--quiet" | "-q" => command.quiet = true,
            "--no-history" => command.no_history = true,
            "--php-version" | "--ini" => {
                let value = args.next().filter(|value| !value.starts_with('-'))
                    .ok_or_else(|| format!("{arg} requires a value"))?;
                command.compile_args.extend([arg.clone(), value.clone()]);
            }
            "--strict-php" | "--heap-debug" | "--gc-stats" => command.compile_args.push(arg.clone()),
            _ if arg.starts_with("--internal-build-host=") => {
                if command.build_output.is_some() { return Err("duplicate internal build destination".into()); }
                let path = &arg["--internal-build-host=".len()..];
                if !std::path::Path::new(path).is_absolute() { return Err("host build destination must be absolute".into()); }
                command.build_output = Some(path.into());
            }
            _ if arg.starts_with("--php-version=") || arg.starts_with("--heap-size=") || arg.starts_with("--ini=") => {
                command.compile_args.push(arg.clone());
            }
            _ if arg.starts_with("--with-") => {
                let name = &arg[7..];
                if matches!(name, "web" | "monitoring" | "probe" | "instrument") || !crate::cli::with_flag_is_known(name) {
                    return Err(format!("capability '{name}' is not supported by elephc repl"));
                }
                command.compile_args.push(arg.clone());
            }
            _ => return Err(format!("unrecognized REPL option '{arg}'")),
        }
    }
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps compiler output modes and cross-target requests out of a host-only session.
    #[test]
    fn repl_rejects_non_session_options() {
        for arg in ["--target=ios-arm64", "--with-web", "--emit-asm", "file.php", "--with-unknown"] {
            assert!(parse_args(&[arg.into()]).is_err(), "{arg}");
        }
    }

    /// Preserves repeated INI order and accepts only complete option-value pairs.
    #[test]
    fn repl_options_preserve_compile_arguments() {
        let args = ["--quiet", "--no-history", "--php-version", "8.4", "--ini", "a=1", "--ini", "a=2", "--with-regex"];
        let parsed = parse_args(&args.map(String::from)).unwrap();
        assert!(parsed.quiet && parsed.no_history);
        assert_eq!(parsed.compile_args, &args[2..]);
        assert!(parse_args(&["--ini".into()]).is_err());
    }
}
