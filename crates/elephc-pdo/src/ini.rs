//! Purpose:
//! Runtime discovery and parsing of PHP INI PDO DSN aliases.
//!
//! Called from:
//! - `elephc_pdo_ini_dsn_defined()` and `elephc_pdo_ini_dsn_value()` in the bridge root.
//!
//! Key details:
//! - Main `PHPRC` configuration loads before alphabetically sorted scan fragments.
//! - Directive names are case-sensitive and later `pdo.dsn.*` assignments win.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Returns the process-startup PDO alias map, loading it on first PDO use.
fn aliases() -> &'static HashMap<String, String> {
    static ALIASES: OnceLock<HashMap<String, String>> = OnceLock::new();
    ALIASES.get_or_init(load_aliases)
}

/// Looks up a case-sensitive `pdo.dsn.<name>` alias by its short DSN name.
pub(crate) fn lookup(name: &str) -> Option<&'static str> {
    aliases().get(name).map(String::as_str)
}

/// Loads aliases from the main PHP configuration and every configured scan fragment.
fn load_aliases() -> HashMap<String, String> {
    let phprc = std::env::var_os("PHPRC");
    let scan = std::env::var_os("PHP_INI_SCAN_DIR");
    let mut result = HashMap::new();
    for path in configuration_files(phprc.as_deref(), scan.as_deref()) {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        parse_aliases(&contents, &mut result);
    }
    result
}

/// Resolves the ordered PHP configuration files for explicit portable runtime sources.
fn configuration_files(phprc: Option<&OsStr>, scan: Option<&OsStr>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Some(raw) = phprc.filter(|value| !value.is_empty()) {
        let path = PathBuf::from(raw);
        let candidate = if path.is_dir() { path.join("php.ini") } else { path };
        if candidate.is_file() {
            files.push(candidate);
        }
    }

    if let Some(raw) = scan {
        for directory in std::env::split_paths(raw) {
            if directory.as_os_str().is_empty() {
                continue;
            }
            let Ok(entries) = fs::read_dir(directory) else {
                continue;
            };
            let mut fragments: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && path.extension().and_then(OsStr::to_str) == Some("ini")
                })
                .collect();
            fragments.sort();
            files.extend(fragments);
        }
    }
    files
}

/// Applies PDO alias assignments from one INI document to `aliases`.
fn parse_aliases(contents: &str, aliases: &mut HashMap<String, String>) {
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((raw_key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let key = raw_key.trim();
        let Some(name) = key.strip_prefix("pdo.dsn.") else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        aliases.insert(name.to_string(), parse_value(raw_value));
    }
}

/// Parses the quoted or unquoted scalar subset used by PHP PDO DSN directives.
fn parse_value(raw: &str) -> String {
    let value = raw.trim_start();
    let Some(quote) = value.chars().next().filter(|ch| *ch == '\'' || *ch == '"') else {
        let unquoted = value
            .split_once(';')
            .map_or(value, |(before, _)| before)
            .trim_end();
        return expand_environment(unquoted);
    };

    let mut parsed = String::new();
    let mut escaped = false;
    for ch in value[quote.len_utf8()..].chars() {
        if escaped {
            if ch == quote || ch == '\\' {
                parsed.push(ch);
            } else {
                parsed.push('\\');
                parsed.push(ch);
            }
            escaped = false;
        } else if ch == '\\' && quote == '"' {
            escaped = true;
        } else if ch == quote {
            return if quote == '"' {
                expand_environment(&parsed)
            } else {
                parsed
            };
        } else {
            parsed.push(ch);
        }
    }
    if escaped {
        parsed.push('\\');
    }
    if quote == '"' {
        expand_environment(&parsed)
    } else {
        parsed
    }
}

/// Expands PHP INI `${NAME}` environment references, leaving missing variables empty.
fn expand_environment(value: &str) -> String {
    let mut expanded = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        expanded.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            expanded.push_str(&rest[start..]);
            return expanded;
        };
        expanded.push_str(&std::env::var(&after[..end]).unwrap_or_default());
        rest = &after[end + 1..];
    }
    expanded.push_str(rest);
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses quoted semicolons, ignores unrelated directives, and keeps the last alias.
    #[test]
    fn parses_pdo_aliases_with_last_assignment_winning() {
        let mut aliases = HashMap::new();
        parse_aliases(
            r#"
                pdo.dsn.main = "mysql:host=one;dbname=db"
                unrelated = value
                pdo.dsn.main = 'sqlite::memory:' ; replacement
                PDO.DSN.UPPER = "pgsql:host=wrong"
            "#,
            &mut aliases,
        );
        assert_eq!(aliases.get("main").map(String::as_str), Some("sqlite::memory:"));
        assert!(!aliases.contains_key("UPPER"));
    }

    /// Preserves semicolons inside quotes and strips unquoted trailing comments.
    #[test]
    fn parses_quoted_and_unquoted_values() {
        assert_eq!(parse_value("\"mysql:host=db;dbname=app\" ; note"), "mysql:host=db;dbname=app");
        assert_eq!(parse_value(" sqlite::memory: ; note"), "sqlite::memory:");
        assert_eq!(parse_value("\"sqlite:file\\nname\""), "sqlite:file\\nname");
    }

    /// Orders a PHPRC main file before alphabetically sorted scan fragments.
    #[test]
    fn configuration_order_matches_php_precedence() {
        let root = std::env::temp_dir().join(format!(
            "elephc_pdo_ini_{}_configuration_order",
            std::process::id()
        ));
        let scan = root.join("scan");
        fs::create_dir_all(&scan).unwrap();
        let main = root.join("php.ini");
        fs::write(&main, "pdo.dsn.db=sqlite:main").unwrap();
        fs::write(scan.join("20-last.ini"), "pdo.dsn.db=sqlite:last").unwrap();
        fs::write(scan.join("10-first.ini"), "pdo.dsn.db=sqlite:first").unwrap();
        fs::write(scan.join("ignored.txt"), "pdo.dsn.db=sqlite:ignored").unwrap();

        let paths = configuration_files(Some(main.as_os_str()), Some(scan.as_os_str()));
        assert_eq!(paths, [main, scan.join("10-first.ini"), scan.join("20-last.ini")]);
        let _ = fs::remove_dir_all(root);
    }
}
