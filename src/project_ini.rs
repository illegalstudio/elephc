//! Purpose:
//! Reads the `[ini]` table of the project's `elephc.toml`: the INI directives a project sets
//! once, instead of repeating `--ini KEY=VALUE` on every compile.
//!
//! Called from:
//! - `main_inner` on the compile path, which puts these entries AHEAD of the command line's own
//!   `--ini` ones. Not from `crate::cli::parse_args`: argument parsing reads no project file,
//!   so its unit tests stay independent of whatever `elephc.toml` sits above the checkout.
//!
//! Key details:
//! - The file is the one native dependencies already use, found the same way: the nearest
//!   `elephc.toml` walking up from the entry file's directory
//!   (`crate::native_deps::discover_for_source`). A project without one, or without
//!   an `[ini]` table, sets nothing, and a lone `.php` file still needs no manifest.
//! - EACH ENTRY IS EXACTLY ONE `--ini KEY=VALUE`: the same key rules, the value passed
//!   verbatim, the same consumers (OPcache, mbstring startup settings, core defaults). Only the
//!   spelling changes, so nothing downstream knows or cares where a value came from — a
//!   relative path, for instance, is read exactly as it would be from `--ini`.
//! - THE COMMAND LINE WINS. Its `--ini` entries come after these, and every consumer reads the
//!   LAST value of a repeated key.
//! - A dotted key is a nested table in TOML, so `opcache.enable_cli = true` inside `[ini]`
//!   means `ini.opcache.enable_cli`. Nested tables are flattened back with `.`, which makes
//!   that spelling, the quoted `"opcache.enable_cli"` and an `[ini.opcache]` table all name the
//!   same directive, as a php.ini author would expect.
//! - A DIRECTIVE NAMED TWICE IS REFUSED. A quoted `"opcache.enable"` and an `[ini.opcache]`
//!   table holding `enable` are different TOML paths, so TOML accepts both, and keeping both
//!   would let `toml::Table`'s KEY ORDER, not the file, pick the winner.
//! - Values become the strings a php.ini line produces: a string verbatim, an integer or a
//!   float in decimal, `true` as `"1"` and `false` as `""` (the INI scanner's folding of
//!   `On` / `Off`). A TOML number is its value, not its spelling (`0x10` is `"16"`, `1.0` is
//!   `"1"`); a quoted string is the way to hand a directive exact text. An array, a date, `inf`
//!   and `nan` have no INI value and are refused.

use std::collections::BTreeSet;
use std::path::Path;

/// Returns the `[ini]` entries of the nearest `elephc.toml` above `source`, as `--ini` pairs.
///
/// A source whose directory cannot be resolved has no project: the compile reports the
/// unreadable source itself, with the message it always gave. Every OTHER discovery failure is
/// reported, a symlinked manifest among them: swallowing it would compile with none of the
/// project's directives and no word said, and would hide a valid manifest further up too.
pub(crate) fn project_ini_overrides(source: &Path) -> Result<Vec<(String, String)>, String> {
    let directory = source
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if std::fs::canonicalize(directory).is_err() {
        return Ok(Vec::new());
    }
    let Some(project) =
        crate::native_deps::discover_for_source(source).map_err(|error| error.to_string())?
    else {
        return Ok(Vec::new());
    };
    let text = std::fs::read_to_string(&project.manifest)
        .map_err(|error| format!("cannot read {}: {error}", project.manifest.display()))?;
    ini_entries(&text).map_err(|message| format!("{}: {message}", project.manifest.display()))
}

/// Parses the `[ini]` table of one manifest's text into `--ini` pairs.
fn ini_entries(text: &str) -> Result<Vec<(String, String)>, String> {
    let document: toml::Table =
        toml::from_str(text).map_err(|error| format!("invalid TOML: {error}"))?;
    let Some(ini) = document.get("ini") else {
        return Ok(Vec::new());
    };
    let table = ini
        .as_table()
        .ok_or_else(|| "`ini` must be a table of directives".to_string())?;
    let mut entries = Vec::new();
    flatten(table, "", &mut entries, &mut BTreeSet::new())?;
    Ok(entries)
}

/// Appends the directives of `table`, whose keys are prefixed with `prefix`, refusing a name
/// already in `seen`.
fn flatten(
    table: &toml::Table,
    prefix: &str,
    entries: &mut Vec<(String, String)>,
    seen: &mut BTreeSet<String>,
) -> Result<(), String> {
    for (key, value) in table {
        let key = key.trim();
        if key.is_empty() {
            return Err("[ini] has an empty directive name".to_string());
        }
        let name = if prefix.is_empty() {
            key.to_string()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::Table(nested) => flatten(nested, &name, entries, seen)?,
            other => {
                if !seen.insert(name.clone()) {
                    return Err(format!(
                        "[ini] sets {name} twice, under two spellings; keep one"
                    ));
                }
                entries.push((name.clone(), ini_value(&name, other)?));
            }
        }
    }
    Ok(())
}

/// Spells one TOML value as the string a php.ini line would give the directive.
fn ini_value(name: &str, value: &toml::Value) -> Result<String, String> {
    match value {
        toml::Value::String(text) => Ok(text.clone()),
        toml::Value::Integer(number) => Ok(number.to_string()),
        toml::Value::Float(number) if number.is_finite() => Ok(number.to_string()),
        toml::Value::Float(_) => Err(format!("[ini] {name} must be a finite number")),
        toml::Value::Boolean(true) => Ok("1".to_string()),
        toml::Value::Boolean(false) => Ok(String::new()),
        toml::Value::Array(_) | toml::Value::Datetime(_) | toml::Value::Table(_) => Err(format!(
            "[ini] {name} must be a string, a number or a boolean, as an INI value is"
        )),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins how `[ini]` entries become `--ini` pairs: the value spellings, the flattening of
    //! dotted keys, and the refusals.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - The discovery half is `native_deps`'s and is tested there; the end-to-end
    //!   path, precedence over the command line included, is in `tests/project_ini_tests.rs`.

    use super::*;

    /// Returns the pairs for one manifest text, sorted for comparison.
    fn pairs(text: &str) -> Vec<(String, String)> {
        let mut entries = ini_entries(text).expect("the manifest should be accepted");
        entries.sort();
        entries
    }

    /// Builds an expected pair.
    fn pair(key: &str, value: &str) -> (String, String) {
        (key.to_string(), value.to_string())
    }

    /// Verifies each TOML value spells the string a php.ini line would produce.
    #[test]
    fn values_take_their_php_ini_spelling() {
        assert_eq!(
            pairs(
                "[ini]\n\"opcache.file_cache\" = \"var/opcache\"\n\"opcache.memory_consumption\" = 256\n\
                 \"opcache.jit_prof_threshold\" = 0.005\n\"opcache.enable_cli\" = true\n\
                 \"opcache.validate_timestamps\" = false\n"
            ),
            [
                pair("opcache.enable_cli", "1"),
                pair("opcache.file_cache", "var/opcache"),
                pair("opcache.jit_prof_threshold", "0.005"),
                pair("opcache.memory_consumption", "256"),
                pair("opcache.validate_timestamps", ""),
            ]
        );
    }

    /// Verifies a quoted key, a bare dotted key and a sub-table all name the same directive.
    #[test]
    fn dotted_keys_and_sub_tables_flatten_to_the_directive_name() {
        let expected = [pair("opcache.enable_cli", "1")];
        assert_eq!(pairs("[ini]\n\"opcache.enable_cli\" = true\n"), expected);
        assert_eq!(pairs("[ini]\nopcache.enable_cli = true\n"), expected);
        assert_eq!(pairs("[ini.opcache]\nenable_cli = true\n"), expected);
    }

    /// Verifies a manifest without `[ini]` sets nothing, whatever else it declares.
    #[test]
    fn a_manifest_without_an_ini_table_sets_nothing() {
        assert!(pairs("[native]\nschema = 1\n[native.dependencies]\n").is_empty());
        assert!(pairs("").is_empty());
    }

    /// Verifies the refusals: values with no INI value, a non-table `ini`, an empty name.
    #[test]
    fn values_without_an_ini_spelling_are_refused() {
        for text in [
            "[ini]\n\"opcache.blacklist_filename\" = [\"a\", \"b\"]\n",
            "[ini]\n\"date.timezone\" = 1979-05-27T07:32:00Z\n",
            "[ini]\n\"opcache.jit_prof_threshold\" = inf\n",
            "[ini]\n\"opcache.jit_prof_threshold\" = nan\n",
            "ini = 1\n",
            "[ini]\n\"\" = 1\n",
            "[ini\n",
        ] {
            assert!(ini_entries(text).is_err(), "{text}");
        }
    }

    /// Verifies a directive named twice is refused, whichever two spellings name it.
    ///
    /// Each pair is valid TOML: the quoted key and the sub-table are different paths, and the
    /// two quoted keys differ until trimmed. Accepting both let `toml::Table`'s key order
    /// decide: `"opcache"` sorts before `"opcache.enable"`, so the quoted value always won,
    /// wherever it stood in the file.
    #[test]
    fn a_directive_named_twice_is_refused() {
        for text in [
            "[ini]\n\"opcache.enable\" = 1\n\n[ini.opcache]\nenable = 0\n",
            "[ini.opcache]\nenable = 0\n\n[ini]\n\"opcache.enable\" = 1\n",
            "[ini]\n\"opcache.enable\" = 1\n\" opcache.enable \" = 0\n",
        ] {
            let error = ini_entries(text).expect_err(text);
            assert!(error.contains("opcache.enable twice"), "{error}");
        }
    }

    /// Verifies a discovery failure other than an unresolvable source directory is reported.
    ///
    /// A symlinked `elephc.toml` is refused by project discovery; swallowing that refusal
    /// compiled with none of the file's directives and said nothing.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_manifest_is_reported_not_skipped() {
        let root = std::env::temp_dir().join(format!(
            "elephc-project-ini-symlink-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("shared.toml");
        std::fs::write(&target, "[ini]\n\"opcache.enable_cli\" = true\n").unwrap();
        let manifest = root.join("elephc.toml");
        let _ = std::fs::remove_file(&manifest);
        std::os::unix::fs::symlink(&target, &manifest).unwrap();

        let result = project_ini_overrides(&root.join("main.php"));
        let _ = std::fs::remove_dir_all(&root);

        let error = result.expect_err("a symlinked manifest must not be skipped silently");
        assert!(error.contains("symlink"), "{error}");
    }

    /// Verifies a source whose directory does not exist has no project, rather than an error:
    /// the compile then reports the missing source itself.
    #[test]
    fn an_unresolvable_source_directory_has_no_project() {
        let source = Path::new("/elephc-no-such-directory/main.php");
        assert_eq!(project_ini_overrides(source), Ok(Vec::new()));
    }
}
