//! Purpose:
//! Parses and runs the `elephc extension` command family.
//!
//! Called from:
//! - Top-level CLI dispatch (`crate::cli::parse_args`) and `main`.
//!
//! Key details:
//! - Parsing is side-effect free; running delegates to `install`.
//! - Flags mirror `elephc native` (`--target`, `--offline`, `--manifest-path`)
//!   so the two command families read the same.

use std::path::{Path, PathBuf};

use crate::codegen_support::platform::Target;
use crate::native_deps::{NativeError, NativeErrorKind};

use super::install::{self, AddSpec};

/// A validated `elephc extension` subcommand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhpExtCommand {
    Add { spec: AddSpec, options: PhpExtOptions },
    Install { options: PhpExtOptions },
    Remove { name: String, options: PhpExtOptions },
    List { options: PhpExtOptions },
}

/// Flags shared by the subcommands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhpExtOptions {
    pub target: Option<Target>,
    pub manifest_path: Option<PathBuf>,
    pub offline: bool,
}

/// Parser result, including help callers print and exit successfully on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhpExtParseOutcome {
    Command(PhpExtCommand),
    Help(String),
}

/// The command family's synopsis.
pub fn php_ext_help() -> String {
    concat!(
        "Usage:\n",
        "  elephc extension add <name>[@<version>] [--target TARGET] [--offline] [--manifest-path FILE]\n",
        "  elephc extension add <vendor/package>[@<version>] [...]      (a PIE package from Packagist)\n",
        "  elephc extension add <name> --path DIR [...]                  (a local extension source tree)\n",
        "  elephc extension install [--target TARGET] [--offline] [--manifest-path FILE]\n",
        "  elephc extension remove <name> [--manifest-path FILE]\n",
        "  elephc extension list [--target TARGET] [--manifest-path FILE]\n",
        "\n",
        "Hosts real PHP extensions (PECL, PIE, or your own) in compiled programs: each is built\n",
        "from source against Elephc's Zend engine and its functions become callable PHP functions.\n",
    )
    .to_string()
}

fn usage(message: impl Into<String>) -> NativeError {
    NativeError::new(NativeErrorKind::Usage, message)
}

/// Parses the tokens after `extension`.
pub fn parse_php_ext_args(args: &[String]) -> Result<PhpExtParseOutcome, NativeError> {
    let Some(verb) = args.first().map(String::as_str) else {
        return Err(usage("missing extension command"));
    };
    if matches!(verb, "-h" | "--help" | "help") {
        return Ok(PhpExtParseOutcome::Help(php_ext_help()));
    }
    let mut options = PhpExtOptions::default();
    let mut positional = Vec::new();
    let mut path = None;
    let mut index = 1;
    while index < args.len() {
        let arg = args[index].as_str();
        let value = |index: usize| -> Result<String, NativeError> {
            args.get(index + 1).cloned().ok_or_else(|| usage(format!("{arg} needs a value")))
        };
        match arg {
            "-h" | "--help" => return Ok(PhpExtParseOutcome::Help(php_ext_help())),
            "--offline" => options.offline = true,
            "--target" => {
                let text = value(index)?;
                options.target = Some(Target::parse(&text).map_err(usage)?);
                index += 1;
            }
            "--manifest-path" => {
                options.manifest_path = Some(PathBuf::from(value(index)?));
                index += 1;
            }
            "--path" => {
                path = Some(PathBuf::from(value(index)?));
                index += 1;
            }
            other if other.starts_with('-') => return Err(usage(format!("unknown flag '{other}'"))),
            other => positional.push(other.to_string()),
        }
        index += 1;
    }
    let one = |what: &str| -> Result<String, NativeError> {
        match positional.as_slice() {
            [single] => Ok(single.clone()),
            [] => Err(usage(format!("extension {verb} needs {what}"))),
            _ => Err(usage(format!("extension {verb} takes one {what}"))),
        }
    };
    let none = || -> Result<(), NativeError> {
        if positional.is_empty() { Ok(()) } else { Err(usage(format!("extension {verb} takes no arguments"))) }
    };
    if path.is_some() && verb != "add" {
        return Err(usage("--path belongs to extension add"));
    }
    let command = match verb {
        "add" => PhpExtCommand::Add { spec: install::parse_add_spec(&one("an extension")?, path)?, options },
        "install" => {
            none()?;
            PhpExtCommand::Install { options }
        }
        "remove" => PhpExtCommand::Remove { name: one("an extension name")?, options },
        "list" => {
            none()?;
            PhpExtCommand::List { options }
        }
        other => return Err(usage(format!("unknown extension command '{other}'"))),
    };
    Ok(PhpExtParseOutcome::Command(command))
}

/// Runs a parsed command and returns what it prints.
pub fn run_php_ext_command(command: &PhpExtCommand, cwd: &Path) -> Result<String, NativeError> {
    match command {
        PhpExtCommand::Add { spec, options } => {
            install::add(cwd, spec, options.manifest_path.as_deref(), options.target, options.offline)
        }
        PhpExtCommand::Install { options } => {
            install::install(cwd, options.manifest_path.as_deref(), options.target, options.offline)
        }
        PhpExtCommand::Remove { name, options } => install::remove(cwd, name, options.manifest_path.as_deref()),
        PhpExtCommand::List { options } => install::list(cwd, options.manifest_path.as_deref(), options.target),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<PhpExtParseOutcome, NativeError> {
        parse_php_ext_args(&args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn parses_add_with_flags() {
        let outcome = parse(&["add", "apcu@5.1.28", "--offline", "--manifest-path", "x/elephc.toml"]).unwrap();
        let PhpExtParseOutcome::Command(PhpExtCommand::Add { spec, options }) = outcome else { panic!() };
        assert_eq!(spec, AddSpec::Pecl { name: "apcu".into(), version: Some("5.1.28".into()) });
        assert!(options.offline);
        assert_eq!(options.manifest_path, Some(PathBuf::from("x/elephc.toml")));
    }

    #[test]
    fn parses_a_path_source() {
        let outcome = parse(&["add", "demo", "--path", "ext/demo"]).unwrap();
        let PhpExtParseOutcome::Command(PhpExtCommand::Add { spec, .. }) = outcome else { panic!() };
        assert_eq!(spec, AddSpec::Path { name: "demo".into(), path: PathBuf::from("ext/demo") });
    }

    #[test]
    fn rejects_malformed_invocations() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["add"]).is_err());
        assert!(parse(&["install", "extra"]).is_err());
        assert!(parse(&["list", "--path", "x"]).is_err());
        assert!(parse(&["frobnicate"]).is_err());
        assert!(parse(&["add", "x", "--bogus"]).is_err());
    }

    #[test]
    fn help_is_not_an_error() {
        assert!(matches!(parse(&["--help"]).unwrap(), PhpExtParseOutcome::Help(_)));
        assert!(matches!(parse(&["add", "-h"]).unwrap(), PhpExtParseOutcome::Help(_)));
    }
}
