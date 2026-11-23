//! Purpose:
//! Reads the monitoring tables of the project's `elephc.toml` — `[monitoring]`, `[apm]`,
//! `[budgets]` and their per-entry overrides under `[entry."<path>"]` — so a project states
//! once how its builds are monitored and what `elephc monitor` holds them to, instead of
//! repeating `--with-monitoring`, `--with-apm`, `--apm-collector`, `--apm-transaction` and
//! `--assert` on every command.
//!
//! Called from:
//! - `main` on the compile path (`apply_to_build`), right after the command line is parsed,
//!   as `[ini]` is (`crate::project_ini`): argument parsing itself reads no project file.
//! - `crate::monitor::load_assert_file`, for `[budgets]`.
//!
//! Key details:
//! - The manifest is the one native dependencies and `[ini]` use, found the same way: the
//!   nearest `elephc.toml` walking up from the entry file's directory.
//! - THE COMMAND LINE WINS. A setting the build was given on the command line is kept, and the
//!   manifest only fills what the command line left unset.
//! - An unknown key is refused: a misspelt `exlude` must not silently stop excluding anything.
//! - `[entry."bin/console".apm]` (or `.monitoring`) replaces, key by key, the top-level table
//!   for the build whose entry file is that path, relative to the manifest's directory: one
//!   project, one manifest, a web front and a console monitored differently. A path that names
//!   no file is warned about on every build, so a typo does not silently drop its settings.
//! - `enabled = false` wins over the keys that otherwise imply `enabled` (`functions`,
//!   `collector`, `transactions`): an entry or a project can switch monitoring off without
//!   deleting its settings.
//! - `[budgets] rules` take the `--assert` syntax, one rule per string; ` # text` after a rule
//!   is its label, as in the `.elephc` file this table replaces.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// `[monitoring]`: the profiler a build embeds.
#[derive(Debug, Default, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct MonitoringTable {
    /// `true` embeds the profiler in every function (`--with-monitoring`).
    pub enabled: Option<bool>,
    /// Only these functions (`--with-monitoring=a,b`); implies `enabled` unless it is `false`.
    pub functions: Option<Vec<String>>,
}

/// `[apm]`: transaction monitoring.
#[derive(Debug, Default, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApmTable {
    /// `true` counts transactions (`--with-apm`).
    pub enabled: Option<bool>,
    /// Where the program sends its counters (`--apm-collector`); implies `enabled` unless it
    /// is `false`.
    pub collector: Option<String>,
    /// Named transactions (`--apm-transaction`); implies `enabled` unless it is `false`.
    pub transactions: Option<Vec<String>>,
    /// Route and command patterns counted, and only them.
    pub include: Option<Vec<String>>,
    /// Route and command patterns never counted, within what `include` lets in.
    pub exclude: Option<Vec<String>>,
}

/// `[entry."<path>"]`: one entry file's overrides.
#[derive(Debug, Default, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct EntryTable {
    monitoring: Option<MonitoringTable>,
    apm: Option<ApmTable>,
}

/// `[budgets]`: what `elephc monitor` checks a run against.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetsTable {
    rules: Vec<String>,
}

/// What a project's manifest says about monitoring.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ProjectMonitoring {
    pub monitoring: MonitoringTable,
    pub apm: ApmTable,
    /// The `[budgets]` rules as `(spec, label)`, `None` without the table.
    pub budgets: Option<Vec<(String, Option<String>)>>,
    entries: Vec<(String, EntryTable)>,
}

/// The manifest found for a source, and what it says.
pub(crate) struct Found {
    /// The `elephc.toml` read.
    pub manifest: PathBuf,
    /// Its directory, which `[entry."<path>"]` paths are relative to.
    pub root: PathBuf,
    /// Its monitoring tables.
    pub settings: ProjectMonitoring,
}

/// Parses the monitoring tables of one manifest's text; the manifest's other tables (native
/// dependencies, `[ini]`) are another reader's.
pub(crate) fn parse(text: &str) -> Result<ProjectMonitoring, String> {
    let document: toml::Table = toml::from_str(text).map_err(|error| format!("invalid TOML: {error}"))?;
    let mut settings = ProjectMonitoring::default();
    if let Some(value) = document.get("monitoring") {
        settings.monitoring = value.clone().try_into().map_err(|error| format!("[monitoring]: {error}"))?;
    }
    if let Some(value) = document.get("apm") {
        settings.apm = value.clone().try_into().map_err(|error| format!("[apm]: {error}"))?;
        check_patterns(&settings.apm, "[apm]")?;
    }
    if let Some(value) = document.get("budgets") {
        let table: BudgetsTable = value.clone().try_into().map_err(|error| format!("[budgets]: {error}"))?;
        settings.budgets = Some(table.rules.iter().flat_map(|rule| crate::monitor::parse_assert_file(rule)).collect());
    }
    if let Some(value) = document.get("entry") {
        let entries = value.as_table().ok_or("`entry` must be a table of entry files")?;
        for (path, table) in entries {
            let table: EntryTable =
                table.clone().try_into().map_err(|error| format!("[entry.\"{path}\"]: {error}"))?;
            if let Some(apm) = &table.apm {
                check_patterns(apm, &format!("[entry.\"{path}\".apm]"))?;
            }
            settings.entries.push((path.clone(), table));
        }
    }
    Ok(settings)
}

/// Refuses an empty pattern, or one holding a control character: a label never holds one,
/// and the probe reads the patterns one per line.
fn check_patterns(apm: &ApmTable, table: &str) -> Result<(), String> {
    for (key, patterns) in [("include", &apm.include), ("exclude", &apm.exclude)] {
        for pattern in patterns.iter().flatten() {
            if pattern.is_empty() || pattern.chars().any(char::is_control) {
                return Err(format!("{table} {key}: `{}` is not a route or command pattern", pattern.escape_debug()));
            }
        }
    }
    Ok(())
}

impl ProjectMonitoring {
    /// The `[monitoring]` and `[apm]` settings for the build whose entry file is `entry`: the
    /// top-level tables, with the keys of a matching `[entry."<path>"]` (relative to `root`)
    /// replacing theirs. Also returns a warning for each `[entry."<path>"]` whose path names no
    /// file: a mistyped path matches no build, and its settings would otherwise be dropped with
    /// nothing said.
    pub(crate) fn for_entry(&self, root: &Path, entry: &Path) -> (MonitoringTable, ApmTable, Vec<String>) {
        let mut monitoring = self.monitoring.clone();
        let mut apm = self.apm.clone();
        let entry = entry.canonicalize().ok();
        let mut matched = false;
        let mut missing = Vec::new();
        for (path, table) in &self.entries {
            // A directory names no entry file either: `[entry."bin"]` matches no build.
            let Some(file) = root.join(path).canonicalize().ok().filter(|file| file.is_file()) else {
                missing.push(path);
                continue;
            };
            if entry.as_deref() != Some(file.as_path()) {
                continue;
            }
            matched = true;
            if let Some(over) = &table.monitoring {
                monitoring.enabled = over.enabled.or(monitoring.enabled);
                monitoring.functions = over.functions.clone().or(monitoring.functions);
            }
            if let Some(over) = &table.apm {
                apm.enabled = over.enabled.or(apm.enabled);
                apm.collector = over.collector.clone().or(apm.collector);
                apm.transactions = over.transactions.clone().or(apm.transactions);
                apm.include = over.include.clone().or(apm.include);
                apm.exclude = over.exclude.clone().or(apm.exclude);
            }
        }
        let fallback = if matched { "" } else { "; this build uses the top-level [monitoring] and [apm]" };
        let warnings = missing
            .into_iter()
            .map(|path| {
                format!(
                    "[entry.\"{path}\"] names no file under {}, so it applies to no build{fallback}",
                    root.display()
                )
            })
            .collect();
        (monitoring, apm, warnings)
    }
}

/// The nearest `elephc.toml` above `source`, read, or `None` when there is none.
///
/// A source whose directory cannot be resolved has no project, as for `[ini]`; every other
/// failure — an unreadable or invalid manifest — is reported, since building without the
/// project's monitoring settings and no word said would ship a binary nobody can watch.
pub(crate) fn discover(source: &Path) -> Result<Option<Found>, String> {
    let directory = source.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
    if std::fs::canonicalize(directory).is_err() {
        return Ok(None);
    }
    let Some(project) = crate::native_deps::discover_for_source(source).map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&project.manifest)
        .map_err(|error| format!("cannot read {}: {error}", project.manifest.display()))?;
    let settings = parse(&text).map_err(|message| format!("{}: {message}", project.manifest.display()))?;
    Ok(Some(Found { manifest: project.manifest, root: project.root, settings }))
}

/// Fills what the build's command line left unset from the project's `[monitoring]` and
/// `[apm]` tables, and returns the notes saying what the manifest turned on.
///
/// A setting the command line gave is kept. A library cannot carry the profiler (it has no
/// `main` to initialize it) and a PHP extension cannot count its own transactions (PHP does):
/// a manifest asking for either is skipped for that build with a note, not an error, since the
/// same manifest serves the project's other builds.
pub(crate) fn apply_to_build(
    config: &mut crate::cli::CliConfig,
    monitoring: &MonitoringTable,
    apm: &ApmTable,
    manifest: &str,
) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    // An explicit `enabled = false` wins over the keys that otherwise imply it.
    let wants_profiler =
        monitoring.enabled != Some(false) && (monitoring.enabled == Some(true) || monitoring.functions.is_some());
    if wants_profiler && matches!(config.instrument, crate::codegen::Instrumentation::Off) {
        if config.emit.is_library() {
            notes.push(format!("{manifest}: [monitoring] skipped: a library cannot carry the profiler"));
        } else {
            config.instrument = match &monitoring.functions {
                Some(functions) if functions.is_empty() => {
                    return Err(format!("{manifest}: [monitoring] functions needs at least one function name"));
                }
                Some(functions) => crate::codegen::Instrumentation::Only(functions.clone()),
                None => crate::codegen::Instrumentation::All,
            };
            config.with_crates.extend(crate::cli::MONITORING_BRIDGES.iter().map(|name| name.to_string()));
            notes.push(format!("{manifest}: [monitoring] embeds the profiler (--with-monitoring)"));
        }
    }
    let wants_apm = apm.enabled != Some(false)
        && (apm.enabled == Some(true) || apm.collector.is_some() || apm.transactions.is_some());
    if !wants_apm {
        return Ok(notes);
    }
    if config.emit == crate::codegen::Emit::Phpext {
        notes.push(format!("{manifest}: [apm] skipped: PHP counts a PHP extension's requests"));
        return Ok(notes);
    }
    if config.apm_collector.is_none() {
        if let Some(address) = &apm.collector {
            crate::cli::check_collector_address(address)
                .map_err(|error| format!("{manifest}: [apm] collector {address}: {error}"))?;
            config.apm_collector = Some(address.clone());
        }
    }
    if config.apm_transactions.is_empty() {
        for target in apm.transactions.iter().flatten() {
            crate::apm_prelude::check_transaction_target(target)
                .map_err(|error| format!("{manifest}: [apm] transactions {target}: {error}"))?;
            config.apm_transactions.push(target.clone());
        }
    }
    config.with_crates.insert("probe".to_string());
    notes.push(format!("{manifest}: [apm] counts transactions (--with-apm)"));
    Ok(notes)
}

/// The longest build label a build carries; a longer one is cut.
const BUILD_LABEL_MAX: usize = 80;

/// The identity a probe build carries in its snapshots, `<unix seconds> <label>`, which a
/// collector marks as a deploy the first time a service serves it.
///
/// The time is when the build was made (`SOURCE_DATE_EPOCH` when set, as reproducible builds
/// pin it). The label is `ELEPHC_BUILD_ID` when set — a release name a deploy pipeline
/// knows — otherwise the short commit of the git checkout holding `entry`, otherwise empty.
pub(crate) fn build_identity(entry: &Path) -> String {
    let seconds = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs())
        });
    let label = std::env::var("ELEPHC_BUILD_ID")
        .ok()
        .or_else(|| git_commit(entry))
        .map(|label| clean_build_label(&label))
        .unwrap_or_default();
    if label.is_empty() {
        seconds.to_string()
    } else {
        format!("{seconds} {label}")
    }
}

/// The short commit of the git checkout holding `entry`, when there is one.
fn git_commit(entry: &Path) -> Option<String> {
    let dir = entry.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--short=12", "HEAD"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// A build label as one line of printable text, at most `BUILD_LABEL_MAX` characters.
fn clean_build_label(label: &str) -> String {
    label
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(BUILD_LABEL_MAX)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A build parsed from `words`, as the compile command line gives it.
    fn build(words: &[&str]) -> crate::cli::CliConfig {
        let args: Vec<String> = std::iter::once("elephc").chain(words.iter().copied()).map(str::to_string).collect();
        crate::cli::parse_compile_args(&args)
    }

    /// Every table is read; a manifest without them says nothing, and the other readers'
    /// tables are left alone.
    #[test]
    fn the_monitoring_tables_are_read_and_others_ignored() {
        let settings = parse(
            r#"
            [ini]
            opcache.enable_cli = true

            [monitoring]
            functions = ["App\\Checkout::*"]

            [apm]
            enabled = true
            collector = "10.0.0.5:9412"
            exclude = ["GET /health"]

            [budgets]
            rules = ["p95:GET /checkout<=150 # the checkout stays snappy", "calls:main<=1000"]
            "#,
        )
        .unwrap();
        assert_eq!(settings.monitoring.functions, Some(vec!["App\\Checkout::*".to_string()]));
        assert_eq!(settings.apm.collector.as_deref(), Some("10.0.0.5:9412"));
        assert_eq!(settings.apm.exclude, Some(vec!["GET /health".to_string()]));
        assert_eq!(
            settings.budgets,
            Some(vec![
                ("p95:GET /checkout<=150".to_string(), Some("the checkout stays snappy".to_string())),
                ("calls:main<=1000".to_string(), None),
            ])
        );
        assert_eq!(parse("[ini]\nmemory_limit = \"1G\"\n").unwrap(), ProjectMonitoring::default());
    }

    /// A misspelt key is refused rather than silently ignored.
    #[test]
    fn an_unknown_key_is_refused() {
        let error = parse("[apm]\nexlude = [\"GET /health\"]\n").unwrap_err();
        assert!(error.contains("[apm]") && error.contains("exlude"), "{error}");
        assert!(parse("[monitoring]\nenable = true\n").is_err());
    }

    /// An entry's table replaces the top-level keys it names for the build of that entry, and
    /// only for it.
    #[test]
    fn an_entry_overrides_the_project_for_its_build() {
        let root = std::env::temp_dir().join(format!("elephc-project-monitoring-{}", std::process::id()));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/console"), "<?php\n").unwrap();
        std::fs::write(root.join("index.php"), "<?php\n").unwrap();
        let settings = parse(
            r#"
            [apm]
            enabled = true
            exclude = ["GET /health"]

            [entry."bin/console".apm]
            exclude = ["CLI console cache:*"]
            "#,
        )
        .unwrap();
        let (_, console, warnings) = settings.for_entry(&root, &root.join("bin/console"));
        assert_eq!(console.exclude, Some(vec!["CLI console cache:*".to_string()]));
        assert_eq!(console.enabled, Some(true), "the keys it does not name stay the project's");
        assert!(warnings.is_empty(), "{warnings:?}");
        let (_, web, warnings) = settings.for_entry(&root, &root.join("index.php"));
        assert_eq!(web.exclude, Some(vec!["GET /health".to_string()]));
        assert!(warnings.is_empty(), "another entry's table is not a typo: {warnings:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An `[entry."<path>"]` whose path names no file, a typo, is warned about with the path,
    /// on the build it was meant for and on every other one (#2499).
    #[test]
    fn an_entry_naming_no_file_is_warned_about() {
        let root = std::env::temp_dir().join(format!("elephc-project-monitoring-typo-{}", std::process::id()));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/console"), "<?php\n").unwrap();
        std::fs::write(root.join("index.php"), "<?php\n").unwrap();
        let settings = parse(
            r#"
            [apm]
            enabled = true
            exclude = ["GET /health"]

            [entry."index.php".apm]
            exclude = ["GET /ping"]

            [entry."bin/consolle".apm]
            exclude = ["CLI console cache:*"]

            [entry."bin".apm]
            exclude = ["CLI console about"]
            "#,
        )
        .unwrap();
        let (_, console, warnings) = settings.for_entry(&root, &root.join("bin/console"));
        assert_eq!(console.exclude, Some(vec!["GET /health".to_string()]), "the typo's table is not applied");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings.iter().any(|warning| warning.contains("[entry.\"bin\"] names no file")), "a directory is no entry file: {warnings:?}");
        assert!(warnings.iter().any(|warning| warning.contains("[entry.\"bin/consolle\"] names no file")), "{warnings:?}");
        assert!(warnings.iter().all(|warning| warning.contains("top-level [monitoring] and [apm]")), "{warnings:?}");
        let (_, web, warnings) = settings.for_entry(&root, &root.join("index.php"));
        assert_eq!(web.exclude, Some(vec!["GET /ping".to_string()]));
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings.iter().any(|warning| warning.contains("bin/consolle")), "{warnings:?}");
        assert!(warnings.iter().all(|warning| !warning.contains("top-level")), "a matched build says nothing of it: {warnings:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An explicit `enabled = false` wins over `functions`, `collector` and `transactions`,
    /// which otherwise imply it (#2499).
    #[test]
    fn enabled_false_wins_over_implied_keys() {
        let monitoring =
            MonitoringTable { enabled: Some(false), functions: Some(vec!["App\\Checkout::*".to_string()]) };
        let apm = ApmTable {
            enabled: Some(false),
            collector: Some("10.0.0.5:9412".to_string()),
            transactions: Some(vec!["App\\Job::run".to_string()]),
            ..ApmTable::default()
        };
        let mut config = build(&["main.php"]);
        let notes = apply_to_build(&mut config, &monitoring, &apm, "elephc.toml").unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert!(matches!(config.instrument, crate::codegen::Instrumentation::Off));
        assert!(!config.with_crates.contains("probe") && !config.with_crates.contains("instrument"));
        assert!(config.apm_collector.is_none() && config.apm_transactions.is_empty());
        let implied = ApmTable { enabled: None, ..apm };
        let mut config = build(&["main.php"]);
        apply_to_build(&mut config, &MonitoringTable::default(), &implied, "elephc.toml").unwrap();
        assert!(config.with_crates.contains("probe"), "without `enabled`, a collector still implies it");
    }

    /// The manifest fills what the command line left unset, and never what it set.
    #[test]
    fn the_command_line_wins_over_the_manifest() {
        let monitoring = MonitoringTable { enabled: Some(true), functions: None };
        let apm = ApmTable {
            enabled: Some(true),
            collector: Some("10.0.0.5:9412".to_string()),
            transactions: Some(vec!["App\\Job::run".to_string()]),
            ..ApmTable::default()
        };
        let mut plain = build(&["main.php"]);
        let notes = apply_to_build(&mut plain, &monitoring, &apm, "elephc.toml").unwrap();
        assert!(matches!(plain.instrument, crate::codegen::Instrumentation::All));
        assert!(plain.with_crates.contains("probe") && plain.with_crates.contains("instrument"));
        assert_eq!(plain.apm_collector.as_deref(), Some("10.0.0.5:9412"));
        assert_eq!(plain.apm_transactions, vec!["App\\Job::run".to_string()]);
        assert_eq!(notes.len(), 2, "{notes:?}");

        let mut explicit = build(&[
            "--with-monitoring=main",
            "--apm-collector",
            "127.0.0.1:9000",
            "--apm-transaction",
            "job",
            "main.php",
        ]);
        apply_to_build(&mut explicit, &monitoring, &apm, "elephc.toml").unwrap();
        assert!(matches!(&explicit.instrument, crate::codegen::Instrumentation::Only(names) if names == &["main"]));
        assert_eq!(explicit.apm_collector.as_deref(), Some("127.0.0.1:9000"));
        assert_eq!(explicit.apm_transactions, vec!["job".to_string()]);
    }

    /// A library skips the profiler and a PHP extension skips APM, with a note: the manifest
    /// serves the project's other builds.
    #[test]
    fn builds_that_cannot_carry_a_setting_skip_it_with_a_note() {
        let monitoring = MonitoringTable { enabled: Some(true), functions: None };
        let apm = ApmTable { enabled: Some(true), ..ApmTable::default() };
        let mut library = build(&["--emit", "cdylib", "lib.php"]);
        let notes = apply_to_build(&mut library, &monitoring, &ApmTable::default(), "elephc.toml").unwrap();
        assert!(matches!(library.instrument, crate::codegen::Instrumentation::Off));
        assert!(notes[0].contains("skipped"), "{notes:?}");
        let mut extension = build(&["--emit", "phpext", "ext.php"]);
        let notes = apply_to_build(&mut extension, &MonitoringTable::default(), &apm, "elephc.toml").unwrap();
        assert!(!extension.with_crates.contains("probe"));
        assert!(notes[0].contains("skipped"), "{notes:?}");
    }

    /// Invalid values are reported with the manifest and the table they came from.
    #[test]
    fn invalid_values_name_their_table() {
        let apm = ApmTable { collector: Some("nowhere".to_string()), ..ApmTable::default() };
        let error = apply_to_build(&mut build(&["main.php"]), &MonitoringTable::default(), &apm, "elephc.toml")
            .unwrap_err();
        assert!(error.contains("[apm] collector nowhere"), "{error}");
        let monitoring = MonitoringTable { enabled: None, functions: Some(Vec::new()) };
        assert!(apply_to_build(&mut build(&["main.php"]), &monitoring, &ApmTable::default(), "elephc.toml").is_err());
    }
}
