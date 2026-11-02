//! Purpose:
//! Argument parsing for `elephc test`: the optional test path, `--filter`, `--no-isolation`,
//! `--timeout`, `--junit`, `--help`, and the pass-through of ordinary compile options, with
//! output-mode and target flags refused.
//!
//! Called from:
//! - `crate::cli::parse_args` when the first argument is exactly `test`.
//!
//! Key details:
//! - Exactly one positional is accepted and it is the test path; a second one is an error
//!   rather than a silently ignored file.
//! - Flags the runner cannot honor (`--emit*`, `--check`, `--target`, `--web`) are rejected
//!   here with a message naming the flag, before the compiler sees them.
//! - Everything else is forwarded verbatim to `cli::parse_compile_args`, which keeps the
//!   ordinary compile diagnostics for a mistyped option.

/// One parsed `elephc test` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestCommand {
    /// The test directory or file, as given; `tests` when omitted.
    pub(crate) path: String,
    /// A case-sensitive substring over `Class::method` that selects the tests to compile.
    pub(crate) filter: Option<String>,
    /// Run every test in a forked child (the default) or in the runner's own process.
    pub(crate) isolate: bool,
    /// Seconds an isolated test may run before it is ended and reported; 0 disables.
    pub(crate) timeout: u32,
    /// Path of the JUnit XML report to write, when requested.
    pub(crate) junit: Option<String>,
    /// Ordinary compile options forwarded to the compiler.
    pub(crate) compile_args: Vec<String>,
    /// `-h` / `--help` was given: print usage and exit.
    pub(crate) help: bool,
}

/// The default per-test timeout, in seconds.
pub(crate) const DEFAULT_TIMEOUT: u32 = 60;

/// The usage text for `elephc test --help` and for argument errors.
pub(crate) const HELP: &str = "Usage: elephc test [PATH] [OPTIONS] [COMPILE OPTIONS]

Compile the test suite under PATH (default: tests) into a native binary and run it.
Test files are *Test.php and *Test.lfc; test classes extend Elephc\\Testing\\TestCase
or PHPUnit\\Framework\\TestCase; tests are public `test*` methods or #[Test] methods.
Composer autoload and autoload-dev are read from the nearest composer.json above PATH.

Options:
  --filter PATTERN        Run only the tests whose Class::method contains PATTERN
  --no-isolation          Run tests in the runner's process instead of a forked child
                          per test (faster, for debuggers; a native fatal then ends the run)
  --timeout SECONDS       Seconds an isolated test may run before it is ended and
                          reported as an error (default 60; 0 disables)
  --junit FILE            Write a JUnit XML report to FILE
  -h, --help              Show this help

Compile options such as --strict-php, --with-<crate>, --heap-size, --php-version,
--define and --ini are forwarded to the compiler. Output modes (--emit-ir, --emit-asm,
--emit, --check), --target and --web are not accepted: the runner always builds and
runs a host executable.

Exit code: the suite's (0 when every test passed), 1 on a compile or usage error.
";

/// Flags the runner refuses, each with the reason shown to the user.
const REFUSED_FLAGS: &[(&str, &str)] = &[
    ("--emit-ir", "the runner always builds and runs a host executable"),
    ("--emit-asm", "the runner always builds and runs a host executable"),
    ("--emit", "the runner always builds and runs a host executable"),
    ("--check", "the runner always builds and runs a host executable"),
    ("--target", "the runner runs the suite on this host"),
    ("--web", "a test suite is not a server"),
    ("--with-web", "a test suite is not a server"),
    ("--web-isolation", "a test suite is not a server"),
];

/// Parses the arguments after `elephc test`.
pub(crate) fn parse_args(args: &[String]) -> Result<TestCommand, String> {
    let mut path: Option<String> = None;
    let mut filter = None;
    let mut isolate = true;
    let mut timeout = DEFAULT_TIMEOUT;
    let mut junit = None;
    let mut compile_args = Vec::new();
    let mut help = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-h" || arg == "--help" {
            help = true;
        } else if arg == "--filter" {
            i += 1;
            let value = args
                .get(i)
                .ok_or("missing pattern after --filter")?;
            filter = Some(value.clone());
        } else if let Some(value) = arg.strip_prefix("--filter=") {
            filter = Some(value.to_string());
        } else if arg == "--no-isolation" {
            isolate = false;
        } else if arg == "--timeout" {
            i += 1;
            let value = args.get(i).ok_or("missing seconds after --timeout")?;
            timeout = parse_timeout(value)?;
        } else if let Some(value) = arg.strip_prefix("--timeout=") {
            timeout = parse_timeout(value)?;
        } else if arg == "--junit" {
            i += 1;
            let value = args.get(i).ok_or("missing file after --junit")?;
            junit = Some(value.clone());
        } else if let Some(value) = arg.strip_prefix("--junit=") {
            junit = Some(value.to_string());
        } else if let Some((flag, reason)) = refused(arg) {
            return Err(format!("{flag} is not accepted by `elephc test`: {reason}"));
        } else if arg.starts_with('-') && arg.len() > 1 {
            compile_args.push(arg.clone());
            // Value-taking compile flags keep their value attached: forward the next
            // argument too so `--heap-size 1048576` or `--define X` stays a pair.
            if takes_value(arg) {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| format!("missing value after {arg}"))?;
                compile_args.push(value.clone());
            }
        } else if path.is_none() {
            path = Some(arg.clone());
        } else {
            return Err(format!(
                "unexpected argument '{arg}': `elephc test` takes one test path"
            ));
        }
        i += 1;
    }
    Ok(TestCommand {
        path: path.unwrap_or_else(|| "tests".to_string()),
        filter,
        isolate,
        timeout,
        junit,
        compile_args,
        help,
    })
}

/// Parses the `--timeout` value: a non-negative number of seconds, 0 meaning no timeout.
fn parse_timeout(value: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .map_err(|_| format!("--timeout expects a non-negative number of seconds, got '{value}'"))
}

/// Returns the refused flag and its reason when `arg` is one, in its bare or `=value` form.
fn refused(arg: &str) -> Option<(&'static str, &'static str)> {
    REFUSED_FLAGS.iter().copied().find(|(flag, _)| {
        arg == *flag || arg.strip_prefix(flag).is_some_and(|rest| rest.starts_with('='))
    })
}

/// Compile flags that take their value as the NEXT argument (not `--flag=value`).
fn takes_value(arg: &str) -> bool {
    matches!(
        arg,
        "--php-version"
            | "--define"
            | "-D"
            | "--ini"
            | "--link"
            | "-l"
            | "--link-path"
            | "-L"
            | "--framework"
            | "--null-repr"
            | "--regalloc"
            | "--ir-opt"
    )
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for `elephc test` argument parsing: defaults, `--filter` forms, forwarded
    //! compile options with their values, and the refused flags.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Parsing never touches the file system; existence of the path is checked by `run`.

    use super::*;

    /// Converts a slice of literals into owned arguments.
    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    /// No arguments means the `tests` directory, no filter, nothing forwarded.
    #[test]
    fn defaults_to_the_tests_directory() {
        let command = parse_args(&args(&[])).expect("parses");
        assert_eq!(
            command,
            TestCommand {
                path: "tests".into(),
                filter: None,
                isolate: true,
                timeout: DEFAULT_TIMEOUT,
                junit: None,
                compile_args: vec![],
                help: false
            }
        );
    }

    /// The path, both `--filter` spellings, and compile options are separated correctly,
    /// and a value-taking compile flag keeps its value.
    #[test]
    fn separates_path_filter_and_compile_options() {
        let command = parse_args(&args(&[
            "spec",
            "--filter",
            "Alpha::testOne",
            "--strict-php",
            "--define",
            "DEBUG",
            "--heap-size=1048576",
        ]))
        .expect("parses");
        assert_eq!(command.path, "spec");
        assert_eq!(command.filter.as_deref(), Some("Alpha::testOne"));
        assert_eq!(
            command.compile_args,
            args(&["--strict-php", "--define", "DEBUG", "--heap-size=1048576"])
        );
        let command = parse_args(&args(&["--filter=Beta"])).expect("parses");
        assert_eq!(command.filter.as_deref(), Some("Beta"));
        assert_eq!(command.path, "tests");
    }

    /// A second positional, a dangling `--filter`, and a dangling value-taking flag are
    /// errors that name the problem.
    #[test]
    fn rejects_extra_positionals_and_dangling_values() {
        let error = parse_args(&args(&["a", "b"])).unwrap_err();
        assert!(error.contains("one test path"), "{error}");
        let error = parse_args(&args(&["--filter"])).unwrap_err();
        assert!(error.contains("missing pattern after --filter"), "{error}");
        let error = parse_args(&args(&["--define"])).unwrap_err();
        assert!(error.contains("missing value after --define"), "{error}");
    }

    /// Output-mode, target and web flags are refused in both their spellings.
    #[test]
    fn refuses_output_mode_target_and_web_flags() {
        for flag in ["--emit-ir", "--emit-asm", "--emit=cdylib", "--emit", "--check", "--target=linux-x86_64", "--target", "--web", "--with-web", "--web-isolation=pool"] {
            let error = parse_args(&args(&[flag])).unwrap_err();
            assert!(error.contains("is not accepted by `elephc test`"), "{flag}: {error}");
        }
    }

    /// `--no-isolation`, both `--timeout` spellings and both `--junit` spellings are parsed,
    /// and a non-numeric timeout is an error.
    #[test]
    fn parses_isolation_timeout_and_junit() {
        let command = parse_args(&args(&["--no-isolation", "--timeout", "5", "--junit", "r.xml"])).expect("parses");
        assert!(!command.isolate);
        assert_eq!(command.timeout, 5);
        assert_eq!(command.junit.as_deref(), Some("r.xml"));
        let command = parse_args(&args(&["--timeout=0", "--junit=out/r.xml"])).expect("parses");
        assert_eq!(command.timeout, 0);
        assert_eq!(command.junit.as_deref(), Some("out/r.xml"));
        let error = parse_args(&args(&["--timeout", "soon"])).unwrap_err();
        assert!(error.contains("--timeout expects a non-negative number"), "{error}");
        let error = parse_args(&args(&["--junit"])).unwrap_err();
        assert!(error.contains("missing file after --junit"), "{error}");
    }

    /// `--help` is recognized anywhere and wins over everything else.
    #[test]
    fn help_is_recognized_anywhere() {
        assert!(parse_args(&args(&["tests", "--help"])).expect("parses").help);
        assert!(parse_args(&args(&["-h"])).expect("parses").help);
    }
}
