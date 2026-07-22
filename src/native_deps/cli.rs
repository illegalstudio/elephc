//! Purpose:
//! Parses the isolated `elephc native` command family without exiting the process.
//!
//! Called from:
//! - Top-level CLI dispatch before compilation argument parsing.
//!
//! Key details:
//! - Parsing is side-effect free and accepts only the frozen v1 verb/flag combinations.

use std::path::PathBuf;

use crate::codegen_support::platform::Target;

use super::error::{NativeError, NativeErrorKind};

/// Common selection flags used by native commands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeOptions {
    pub target: Option<Target>,
    pub manifest_path: Option<PathBuf>,
    pub offline: bool,
}

/// A fully validated native subcommand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeCommand {
    Add { package: String, version: Option<String>, options: NativeOptions },
    Install { locked: bool, options: NativeOptions },
    Update { package: Option<String>, version: Option<String>, options: NativeOptions },
    Remove { package: String, manifest_path: Option<PathBuf> },
    List { options: NativeOptions },
    Doctor { options: NativeOptions },
}

/// Native parser result, including help that callers can print and exit successfully.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeParseOutcome {
    Command(NativeCommand),
    Help(String),
}

/// Returns the stable native command synopsis.
pub fn native_help() -> String {
    concat!(
        "Usage:\n",
        "  elephc native add <package>[@<exact-version>] [--target TARGET] [--offline] [--manifest-path FILE]\n",
        "  elephc native install [--target TARGET] [--locked] [--offline] [--manifest-path FILE]\n",
        "  elephc native update [<package>[@<exact-version>]] [--target TARGET] [--offline] [--manifest-path FILE]\n",
        "  elephc native remove <package> [--manifest-path FILE]\n",
        "  elephc native list [--target TARGET] [--manifest-path FILE]\n",
        "  elephc native doctor [--target TARGET] [--manifest-path FILE]\n",
    ).to_string()
}

/// Returns the one-line synopsis for a validated native verb.
fn native_verb_help(verb: &str) -> Option<String> {
    let synopsis = match verb {
        "add" => "elephc native add <package>[@<exact-version>] [--target TARGET] [--offline] [--manifest-path FILE]",
        "install" => "elephc native install [--target TARGET] [--locked] [--offline] [--manifest-path FILE]",
        "update" => "elephc native update [<package>[@<exact-version>]] [--target TARGET] [--offline] [--manifest-path FILE]",
        "remove" => "elephc native remove <package> [--manifest-path FILE]",
        "list" => "elephc native list [--target TARGET] [--manifest-path FILE]",
        "doctor" => "elephc native doctor [--target TARGET] [--manifest-path FILE]",
        _ => return None,
    };
    Some(format!("Usage:\n  {synopsis}\n"))
}

/// Parses tokens following the top-level `native` selector.
pub fn parse_native_args(args: &[String]) -> Result<NativeParseOutcome, NativeError> {
    let Some(verb) = args.first().map(String::as_str) else {
        return Err(usage("missing native command"));
    };
    if verb == "--help" {
        return Ok(NativeParseOutcome::Help(native_help()));
    }
    let verb_help = native_verb_help(verb).ok_or_else(|| usage(&format!("unknown native command '{verb}'")))?;
    if args.iter().skip(1).any(|arg| arg == "--help") {
        return Ok(NativeParseOutcome::Help(verb_help));
    }

    let mut positional = Vec::new();
    let mut options = NativeOptions::default();
    let mut locked = false;
    let mut target_seen = false;
    let mut manifest_seen = false;
    let mut offline_seen = false;
    let mut locked_seen = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--target" => {
                reject_duplicate(&mut target_seen, "--target")?;
                let value = take_value(args, &mut index, "--target")?;
                let target = Target::parse(value).map_err(|error| usage(&error))?;
                if !target.supports_current_backend() {
                    return Err(usage(&format!("target '{}' is not a supported backend", target.as_str())));
                }
                options.target = Some(target);
            }
            "--manifest-path" => {
                reject_duplicate(&mut manifest_seen, "--manifest-path")?;
                let value = take_value(args, &mut index, "--manifest-path")?;
                options.manifest_path = Some(PathBuf::from(value));
            }
            "--offline" => {
                reject_duplicate(&mut offline_seen, "--offline")?;
                options.offline = true;
            }
            "--locked" => {
                reject_duplicate(&mut locked_seen, "--locked")?;
                locked = true;
            }
            value if value.starts_with('-') => return Err(usage(&format!("unknown native option '{value}'"))),
            value => positional.push(value.to_string()),
        }
        index += 1;
    }

    let command = match verb {
        "add" => {
            require_flags(verb, locked, true, true)?;
            let (package, version) = one_package(&positional, true)?;
            NativeCommand::Add { package, version, options }
        }
        "install" => {
            if !positional.is_empty() {
                return Err(usage("install accepts no package argument"));
            }
            NativeCommand::Install { locked, options }
        }
        "update" => {
            require_flags(verb, locked, true, true)?;
            let (package, version) = optional_package(&positional)?;
            NativeCommand::Update { package, version, options }
        }
        "remove" => {
            if locked || options.offline || options.target.is_some() {
                return Err(usage("remove accepts only --manifest-path"));
            }
            let (package, version) = one_package(&positional, false)?;
            debug_assert!(version.is_none());
            NativeCommand::Remove { package, manifest_path: options.manifest_path }
        }
        "list" | "doctor" => {
            if locked || options.offline || !positional.is_empty() {
                return Err(usage(&format!("{verb} accepts only --target and --manifest-path")));
            }
            if verb == "list" { NativeCommand::List { options } } else { NativeCommand::Doctor { options } }
        }
        _ => unreachable!("verb validated before option parsing"),
    };
    Ok(NativeParseOutcome::Command(command))
}

/// Rejects a repeated scalar or boolean flag instead of silently applying last-wins semantics.
fn reject_duplicate(seen: &mut bool, flag: &str) -> Result<(), NativeError> {
    if *seen {
        return Err(usage(&format!("option '{flag}' may be specified only once")));
    }
    *seen = true;
    Ok(())
}

/// Consumes and returns the required value following an option.
fn take_value<'a>(args: &'a [String], index: &mut usize, flag: &str) -> Result<&'a str, NativeError> {
    *index += 1;
    args.get(*index).map(String::as_str).ok_or_else(|| usage(&format!("missing value for {flag}")))
}

/// Rejects the `--locked` flag for commands other than install.
fn require_flags(verb: &str, locked: bool, _offline: bool, _target: bool) -> Result<(), NativeError> {
    if locked {
        return Err(usage(&format!("--locked is valid only for install, not {verb}")));
    }
    Ok(())
}

/// Parses exactly one package selector.
fn one_package(values: &[String], allow_version: bool) -> Result<(String, Option<String>), NativeError> {
    if values.len() != 1 {
        return Err(usage("expected exactly one native package"));
    }
    parse_package_selector(&values[0], allow_version)
}

/// Parses zero or one package selector for `update`.
fn optional_package(values: &[String]) -> Result<(Option<String>, Option<String>), NativeError> {
    if values.is_empty() {
        return Ok((None, None));
    }
    let (package, version) = one_package(values, true)?;
    Ok((Some(package), version))
}

/// Validates a lowercase ASCII package and optional exact dotted-numeric version.
fn parse_package_selector(value: &str, allow_version: bool) -> Result<(String, Option<String>), NativeError> {
    let mut parts = value.split('@');
    let package = parts.next().unwrap_or_default();
    let version = parts.next();
    if parts.next().is_some() || package.is_empty() || !is_package_name(package) {
        return Err(usage(&format!("invalid native package selector '{value}'")));
    }
    if version.is_some() && !allow_version {
        return Err(usage("remove accepts a package name without a version"));
    }
    if let Some(version) = version {
        if !is_exact_version(version) {
            return Err(usage(&format!("version '{version}' must be an exact dotted numeric version")));
        }
    }
    Ok((package.to_string(), version.map(str::to_string)))
}

/// Returns whether a package name is a lowercase ASCII catalog identifier.
fn is_package_name(value: &str) -> bool {
    value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Returns whether a version is an exact dotted-numeric token without constraint operators.
fn is_exact_version(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Builds a usage-category error and appends the native synopsis hint.
fn usage(message: &str) -> NativeError {
    NativeError::new(NativeErrorKind::Usage, format!("{message}; run 'elephc native --help'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Converts string literals into parser-owned CLI tokens.
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    /// Verifies every v1 verb and its legal flags produce typed commands.
    #[test]
    fn parses_all_native_verbs() {
        assert!(matches!(parse_native_args(&args(&["add", "pcre2@10.47", "--offline"])), Ok(NativeParseOutcome::Command(NativeCommand::Add { .. }))));
        assert!(matches!(parse_native_args(&args(&["install", "--locked"])), Ok(NativeParseOutcome::Command(NativeCommand::Install { locked: true, .. }))));
        assert!(matches!(parse_native_args(&args(&["update"])), Ok(NativeParseOutcome::Command(NativeCommand::Update { package: None, .. }))));
        assert!(matches!(parse_native_args(&args(&["remove", "pcre2"])), Ok(NativeParseOutcome::Command(NativeCommand::Remove { .. }))));
        assert!(matches!(parse_native_args(&args(&["list"])), Ok(NativeParseOutcome::Command(NativeCommand::List { .. }))));
        assert!(matches!(parse_native_args(&args(&["doctor"])), Ok(NativeParseOutcome::Command(NativeCommand::Doctor { .. }))));
    }

    /// Verifies help does not require a project and invalid combinations fail early.
    #[test]
    fn help_and_invalid_combinations_are_deterministic() {
        assert!(matches!(parse_native_args(&args(&["--help"])), Ok(NativeParseOutcome::Help(_))));
        let help = parse_native_args(&args(&["install", "--help"])).unwrap();
        assert!(matches!(help, NativeParseOutcome::Help(text) if text.contains("native install") && !text.contains("native add")));
        assert!(parse_native_args(&args(&["help"])).is_err());
        assert!(parse_native_args(&args(&["-h"])).is_err());
        assert!(parse_native_args(&args(&["install", "-h"])).is_err());
        assert!(parse_native_args(&args(&["nonsense", "--help"])).is_err());
        let bare = parse_native_args(&[]).unwrap_err().to_string();
        assert!(bare.contains("missing native command"));
        assert!(bare.contains("elephc native --help"));
        assert!(!bare.contains("native add <package>"));
        assert!(parse_native_args(&args(&["add", "PCRE2"])).is_err());
        assert!(parse_native_args(&args(&["add", "pcre2@^10.47"])).is_err());
        assert!(parse_native_args(&args(&["remove", "pcre2", "--offline"])).is_err());
        assert!(parse_native_args(&args(&["list", "--locked"])).is_err());
        assert!(parse_native_args(&args(&["install", "--offline", "--offline"])).is_err());
        assert!(parse_native_args(&args(&["install", "--target", "linux-x86_64", "--target", "linux-x86_64"])).is_err());
    }
}
