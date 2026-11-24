//! Purpose:
//! Runs one `elephc` command line: parses the top-level command and dispatches it to
//! compilation, the REPL, native-dependency management, `extension`, `monitor`, or `test`.
//!
//! Called from:
//! - `src/main.rs`, on the stack established by `crate::compiler_stack::with_compiler_stack`.
//!
//! Key details:
//! - Lives in the library so the binary is a thin wrapper: a binary that declared the
//!   module tree itself compiled the whole compiler a second time, after the library.

use crate::{
    cli, linker, monitor, native_deps, opcache, php_ext, pipeline, project_ini, project_monitoring, repl,
    test_command,
};

/// Runs the command named by the process arguments, as the `elephc` binary's entry point.
///
/// # Inputs
/// - `std::env::args()`: OS-provided arguments, where `args[0]` is the program name.
///
/// # Outputs
/// - Returns `()` when the selected command succeeds without an explicit exit.
/// - Never returns on fatal errors or unhealthy native diagnostics.
///
/// # Side effects
/// - Compile commands read source files and write outputs alongside the source.
/// - Mutating native commands may update project files and the durable native cache.
/// - Emits warnings/errors to stderr, including OPcache `--ini` quantity diagnostics
///   (`emit_ini_override_warnings`) for compile commands.
/// - May create temporary files during assembly and linking.
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if cli::wants_mascotte(&args) {
        cli::print_mascotte();
    }
    match cli::parse_args(&args) {
        cli::Command::Repl(command) => std::process::exit(repl::run(command)),
        cli::Command::Build(mut config) => {
            apply_project_ini(&mut config);
            apply_project_monitoring(&mut config);
            emit_ini_override_warnings(&config);
            pipeline::compile(config);
        }
        cli::Command::Native(command) => run_native(command),
        cli::Command::PhpExt(command) => run_php_ext(command),
        cli::Command::Monitor(command) => std::process::exit(monitor::run(command)),
        cli::Command::Test(command) => std::process::exit(test_command::run(command)),
    }
}

/// Puts the project's `elephc.toml` `[ini]` entries AHEAD of the command line's `--ini` ones,
/// so an `--ini` for the same directive, being the later entry, is the value every consumer
/// reads. A manifest that cannot be used stops the compile: its directives would otherwise be
/// dropped without a word. `elephc repl` and `elephc test` apply it to their own configs too.
pub(crate) fn apply_project_ini(config: &mut cli::CliConfig) {
    match project_ini::project_ini_overrides(std::path::Path::new(&config.filename)) {
        Ok(mut entries) => {
            entries.append(&mut config.ini_overrides);
            config.ini_overrides = entries;
        }
        Err(message) => {
            eprintln!("error: {message}");
            std::process::exit(1);
        }
    }
}

/// Fills what the command line left unset from the project's `elephc.toml` `[monitoring]` and
/// `[apm]` tables (with the entry file's `[entry."<path>"]` overrides), and says what the
/// manifest turned on. A manifest that cannot be used stops the compile, as for `[ini]`.
fn apply_project_monitoring(config: &mut cli::CliConfig) {
    let source = std::path::PathBuf::from(&config.filename);
    let outcome = project_monitoring::discover(&source).and_then(|found| {
        let Some(found) = found else {
            return Ok(Vec::new());
        };
        let (monitoring, apm, warnings) = found.settings.for_entry(&found.root, &source);
        for warning in warnings {
            eprintln!("warning: {}: {warning}", found.manifest.display());
        }
        config.apm_include = apm.include.clone().unwrap_or_default();
        config.apm_exclude = apm.exclude.clone().unwrap_or_default();
        project_monitoring::apply_to_build(config, &monitoring, &apm, &found.manifest.display().to_string())
    });
    match outcome {
        Ok(notes) => {
            if !config.quiet {
                for note in notes {
                    eprintln!("  note: {note}");
                }
            }
        }
        Err(message) => {
            eprintln!("error: {message}");
            std::process::exit(1);
        }
    }
}

/// Executes a parsed native command and maps its captured output to process streams/status.
fn run_native(command: native_deps::NativeCommand) {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("failed to read current directory: {error}");
            std::process::exit(1);
        }
    };
    match native_deps::run_native_command(&command, &cwd) {
        Ok(output) => {
            print!("{}", output.stdout);
            if output.exit_code != 0 {
                std::process::exit(output.exit_code);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

/// Executes a parsed `elephc extension` command, printing its output or error.
fn run_php_ext(command: php_ext::cli::PhpExtCommand) {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("failed to read current directory: {error}");
            std::process::exit(1);
        }
    };
    let locate_engine: php_ext::install::EngineLocator =
        || linker::bridge_archive("elephc_zend").map_err(|error| error.to_string());
    match php_ext::cli::run_php_ext_command(&command, &cwd, locate_engine) {
        Ok(output) => print!("{output}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

/// Prints the startup diagnostics reference PHP would emit for the `--ini` overrides this
/// compile carries, to stderr, before the pipeline runs.
///
/// Reference PHP emits these while REGISTERING the INI entries at startup — a
/// `Warning: Invalid "opcache.max_file_size" setting. Invalid quantity "12abc": unknown
/// multiplier "c", interpreting as "12" for backwards compatibility in Unknown on line 0` for
/// `php -d opcache.max_file_size=12abc`. For elephc the compile IS the registration (the
/// directive values are baked into the binary), so this is where the faithful analogue belongs
/// and the only point at which it is actionable. The value is still STORED either way — see
/// `crate::opcache::directives::parse_ini_quantity` — so without this the misread is silent.
///
/// The `in Unknown on line 0` tail is dropped: it names reference PHP's INI-file position, and
/// elephc's source of the value is a command-line flag, which the compiler's own stderr voice
/// already implies. Nothing is emitted when there are no `--ini` overrides, so the default
/// compile path is byte-identical on stderr.
pub(crate) fn emit_ini_override_warnings(config: &cli::CliConfig) {
    for warning in opcache::directives::ini_override_warnings(
        config.php_version.version_id(),
        &config.ini_overrides,
    ) {
        eprintln!("Warning: {warning}");
    }
}
