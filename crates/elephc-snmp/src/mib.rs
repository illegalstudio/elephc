//! Purpose:
//! Minimal SMIv2 MIB parser and registry for ext/snmp name-based OID rendering.
//!
//! Called from:
//! - `crate::` C ABI wrappers used by the injected `snmp` PHP prelude.
//!
//! Key details:
//! - `snmp_read_mib()` parses top-level SMI definitions, resolving OID parents
//!   against previously loaded modules and the standard root/group names.
//! - OID rendering follows net-snmp's `Suffix`, `Module`, `Full` and `Ucd`
//!   formats; `Numeric`/`None` stay numeric.
//! - Enum labels are stored per object OID and looked up through the longest
//!   registered prefix so scalar instances (`.0`) render their enum.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// One registered MIB object.
#[derive(Clone, Debug)]
struct Entry {
    /// Defining module.
    module: String,
    /// Object name.
    name: String,
    /// Full OID with a leading dot.
    oid: String,
    /// `(value, label)` pairs from `INTEGER { ... }`/`BITS { ... }`.
    enums: Vec<(i64, String)>,
}

/// Process-wide MIB registry.
#[derive(Default)]
struct Registry {
    /// OID -> entry.
    oids: HashMap<String, Entry>,
    /// Lowercase name -> OID.
    names: HashMap<String, String>,
    /// Lowercase `module::name` -> OID.
    qualified: HashMap<String, String>,
    /// MIB search directories.
    dirs: Vec<String>,
}

/// Returns the process-wide registry.
fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

/// Runs `f` with the locked registry.
fn with_registry<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if registry.names.is_empty() {
        seed(&mut registry);
    }
    f(&mut registry)
}

/// Seeds the standard SMI roots and the SNMPv2-MIB system group.
fn seed(registry: &mut Registry) {
    let roots = [
        ("iso", ".1", "SNMPv2-SMI"),
        ("org", ".1.3", "SNMPv2-SMI"),
        ("dod", ".1.3.6", "SNMPv2-SMI"),
        ("internet", ".1.3.6.1", "SNMPv2-SMI"),
        ("directory", ".1.3.6.1.1", "SNMPv2-SMI"),
        ("mgmt", ".1.3.6.1.2", "SNMPv2-SMI"),
        ("mib-2", ".1.3.6.1.2.1", "SNMPv2-SMI"),
        ("transmission", ".1.3.6.1.2.1.10", "SNMPv2-SMI"),
        ("experimental", ".1.3.6.1.3", "SNMPv2-SMI"),
        ("private", ".1.3.6.1.4", "SNMPv2-SMI"),
        ("enterprises", ".1.3.6.1.4.1", "SNMPv2-SMI"),
        ("security", ".1.3.6.1.5", "SNMPv2-SMI"),
        ("snmpV2", ".1.3.6.1.6", "SNMPv2-SMI"),
        ("snmpDomains", ".1.3.6.1.6.1", "SNMPv2-SMI"),
        ("snmpProxys", ".1.3.6.1.6.2", "SNMPv2-SMI"),
        ("snmpModules", ".1.3.6.1.6.3", "SNMPv2-SMI"),
        ("system", ".1.3.6.1.2.1.1", "SNMPv2-MIB"),
        ("interfaces", ".1.3.6.1.2.1.2", "SNMPv2-MIB"),
        ("at", ".1.3.6.1.2.1.3", "SNMPv2-MIB"),
        ("ip", ".1.3.6.1.2.1.4", "SNMPv2-MIB"),
        ("icmp", ".1.3.6.1.2.1.5", "SNMPv2-MIB"),
        ("tcp", ".1.3.6.1.2.1.6", "SNMPv2-MIB"),
        ("udp", ".1.3.6.1.2.1.7", "SNMPv2-MIB"),
        ("egp", ".1.3.6.1.2.1.8", "SNMPv2-MIB"),
        ("cmot", ".1.3.6.1.2.1.9", "SNMPv2-MIB"),
        ("snmp", ".1.3.6.1.2.1.11", "SNMPv2-MIB"),
        ("sysDescr", ".1.3.6.1.2.1.1.1", "SNMPv2-MIB"),
        ("sysObjectID", ".1.3.6.1.2.1.1.2", "SNMPv2-MIB"),
        ("sysUpTime", ".1.3.6.1.2.1.1.3", "SNMPv2-MIB"),
        ("sysContact", ".1.3.6.1.2.1.1.4", "SNMPv2-MIB"),
        ("sysName", ".1.3.6.1.2.1.1.5", "SNMPv2-MIB"),
        ("sysLocation", ".1.3.6.1.2.1.1.6", "SNMPv2-MIB"),
        ("sysServices", ".1.3.6.1.2.1.1.7", "SNMPv2-MIB"),
        ("ifNumber", ".1.3.6.1.2.1.2.1", "SNMPv2-MIB"),
        ("ifTable", ".1.3.6.1.2.1.2.2", "SNMPv2-MIB"),
        ("ifEntry", ".1.3.6.1.2.1.2.2.1", "SNMPv2-MIB"),
        ("snmpEnableAuthenTraps", ".1.3.6.1.2.1.11.30", "SNMPv2-MIB"),
    ];
    for (name, oid, module) in roots {
        let entry = Entry {
            module: module.to_string(),
            name: name.to_string(),
            oid: oid.to_string(),
            enums: if name == "snmpEnableAuthenTraps" {
                vec![(1, "enabled".to_string()), (2, "disabled".to_string())]
            } else {
                Vec::new()
            },
        };
        registry.oids.insert(oid.to_string(), entry);
        registry.names.insert(name.to_ascii_lowercase(), oid.to_string());
        registry.qualified.insert(
            format!("{}::{}", module.to_ascii_lowercase(), name.to_ascii_lowercase()),
            oid.to_string(),
        );
    }
}

/// Reports whether `token` is an SMI identifier.
fn is_identifier(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
}

/// Strips `--` comments while preserving newlines and quoted strings.
fn strip_comments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    while let Some(ch) = chars.next() {
        if quoted {
            output.push(ch);
            if ch == '"' {
                quoted = false;
            }
            continue;
        }
        if ch == '"' {
            quoted = true;
            output.push(ch);
            continue;
        }
        if ch == '-' && chars.peek() == Some(&'-') {
            while let Some(&next) = chars.peek() {
                if next == '\n' {
                    break;
                }
                chars.next();
            }
            continue;
        }
        output.push(ch);
    }
    output
}

/// Splits `text` into top-level definition blocks.
fn blocks(text: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let starts_definition =
            !line.is_empty() && !line.starts_with(' ') && !line.starts_with('\t');
        if starts_definition && !current.trim().is_empty() {
            output.push(std::mem::take(&mut current));
        }
        if starts_definition || !current.is_empty() {
            current.push_str(line);
            current.push('\n');
        }
    }
    if !current.trim().is_empty() {
        output.push(current);
    }
    output
}

/// Tokenises one definition block.
fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch == '"' {
            chars.next();
            let mut value = String::new();
            while let Some(next) = chars.next() {
                if next == '"' {
                    break;
                }
                value.push(next);
            }
            tokens.push(value);
            continue;
        }
        if ch == ':' {
            chars.next();
            if chars.peek() == Some(&':') {
                chars.next();
            }
            if chars.peek() == Some(&'=') {
                chars.next();
                tokens.push("::=".to_string());
            }
            continue;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            let mut value = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' || next == '-' {
                    value.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(value);
            continue;
        }
        tokens.push(ch.to_string());
        chars.next();
    }
    tokens
}

/// Resolves a symbolic name using the registry.
fn resolve_name(registry: &Registry, name: &str) -> Option<String> {
    let key = name.to_ascii_lowercase();
    if let Some(oid) = registry.names.get(&key) {
        return Some(oid.clone());
    }
    if let Some((module, symbol)) = key.split_once("::") {
        return registry
            .qualified
            .get(&format!("{}::{}", module, symbol))
            .cloned();
    }
    None
}

/// Parses the arcs after `{` in an OID value.
fn parse_oid_value(registry: &Registry, tokens: &[String]) -> Option<String> {
    let mut arcs: Vec<u64> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token == "}" {
            break;
        }
        if token == "," || token == "|" {
            index += 1;
            continue;
        }
        if let Ok(number) = token.parse::<u64>() {
            arcs.push(number);
            index += 1;
            continue;
        }
        if is_identifier(token) {
            if index + 2 < tokens.len() && tokens[index + 1] == "(" {
                let number = tokens[index + 2].parse::<u64>().ok()?;
                arcs.push(number);
                index += 4;
                continue;
            }
            let parent = resolve_name(registry, token)?;
            for arc in parent.trim_start_matches('.').split('.') {
                arcs.push(arc.parse::<u64>().ok()?);
            }
            index += 1;
            continue;
        }
        index += 1;
    }
    if arcs.is_empty() {
        return None;
    }
    Some(format!(
        ".{}",
        arcs.iter().map(u64::to_string).collect::<Vec<_>>().join(".")
    ))
}

/// Parses `SYNTAX INTEGER { ... }` enum pairs.
fn parse_enums(tokens: &[String]) -> Vec<(i64, String)> {
    let Some(position) = tokens.iter().position(|token| token == "SYNTAX") else {
        return Vec::new();
    };
    let mut index = position + 1;
    if index >= tokens.len() {
        return Vec::new();
    }
    if tokens[index] != "INTEGER" && tokens[index] != "INTEGER32" && tokens[index] != "BITS" {
        return Vec::new();
    }
    index += 1;
    if index >= tokens.len() || tokens[index] != "{" {
        return Vec::new();
    }
    index += 1;
    let mut enums = Vec::new();
    while index < tokens.len() && tokens[index] != "}" {
        if is_identifier(&tokens[index])
            && index + 2 < tokens.len()
            && tokens[index + 1] == "("
        {
            if let Ok(value) = tokens[index + 2].parse::<i64>() {
                enums.push((value, tokens[index].clone()));
            }
            index += 3;
            continue;
        }
        index += 1;
    }
    enums
}

/// Parses one MIB file into entries and merges them into the registry.
fn load_file(path: &str) -> bool {
    let text = match std::fs::read(&path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => return false,
    };
    let cleaned = strip_comments(&text);
    let file_module = std::path::Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("MIB")
        .to_string();
    let mut module = file_module;
    let mut loaded = Vec::new();
    for block in blocks(&cleaned) {
        let tokens = tokenize(&block);
        if tokens.is_empty() {
            continue;
        }
        if tokens.get(1).map(String::as_str) == Some("DEFINITIONS") {
            module = tokens[0].clone();
            continue;
        }
        if tokens[0] == "IMPORTS" || tokens[0] == "EXPORTS" || tokens[0] == "END" {
            continue;
        }
        let name = tokens[0].clone();
        if !is_identifier(&name) {
            continue;
        }
        let Some(position) = tokens.iter().position(|token| token == "::=") else {
            continue;
        };
        if tokens.get(position + 1).map(String::as_str) != Some("{") {
            continue;
        }
        let enums = parse_enums(&tokens[..position]);
        let mut registry = registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if registry.names.is_empty() {
            seed(&mut registry);
        }
        let Some(oid) = parse_oid_value(&registry, &tokens[position + 2..]) else {
            continue;
        };
        let entry = Entry {
            module: module.clone(),
            name: name.clone(),
            oid: oid.clone(),
            enums,
        };
        registry.oids.insert(oid.clone(), entry);
        registry
            .names
            .insert(name.to_ascii_lowercase(), oid.clone());
        registry.qualified.insert(
            format!("{}::{}", module.to_ascii_lowercase(), name.to_ascii_lowercase()),
            oid.clone(),
        );
        loaded.push(oid);
    }
    !loaded.is_empty()
}

/// Loads a MIB by path, searching the configured directories for bare names.
pub(crate) fn read_mib(path: &str) -> bool {
    if load_file(path) {
        return true;
    }
    let dirs = with_registry(|registry| registry.dirs.clone());
    for directory in dirs {
        for candidate in [
            format!("{directory}/{path}"),
            format!("{directory}/{path}.txt"),
            format!("{directory}/{path}.mib"),
        ] {
            if load_file(&candidate) {
                return true;
            }
        }
    }
    false
}

/// Sets the MIB search directories (colon-separated).
pub(crate) fn init_dirs(dirs: &str) {
    with_registry(|registry| {
        registry.dirs = dirs
            .split(':')
            .filter(|dir| !dir.is_empty())
            .map(str::to_string)
            .collect();
    });
}

/// Normalises a numeric OID to a leading dot.
fn normalize(oid: &str) -> String {
    if oid.starts_with('.') {
        oid.to_string()
    } else {
        format!(".{oid}")
    }
}

/// Resolves a numeric or symbolic OID.
pub(crate) fn resolve(oid: &str) -> Option<String> {
    let oid = oid.trim();
    if oid.is_empty() {
        return None;
    }
    if oid.chars().all(|ch| ch.is_ascii_digit() || ch == '.') {
        return Some(normalize(oid));
    }
    with_registry(|registry| {
        if let Some((module, rest)) = oid.split_once("::") {
            if let Some((name, suffix)) = split_name_suffix(rest) {
                let key = format!(
                    "{}::{}",
                    module.to_ascii_lowercase(),
                    name.to_ascii_lowercase()
                );
                if let Some(oid) = registry.qualified.get(&key) {
                    return Some(append_suffix(oid, suffix));
                }
            }
            return None;
        }
        let (name, suffix) = split_name_suffix(oid)?;
        resolve_name(registry, name).map(|oid| append_suffix(&oid, suffix))
    })
}

/// Splits `name.suffix` at the first dot.
fn split_name_suffix(value: &str) -> Option<(&str, &str)> {
    let value = value.trim_start_matches('.');
    if value.is_empty() {
        return None;
    }
    match value.split_once('.') {
        Some((name, suffix)) if !name.is_empty() => Some((name, suffix)),
        Some(_) => None,
        None => Some((value, "")),
    }
}

/// Appends a numeric suffix to an OID.
fn append_suffix(oid: &str, suffix: &str) -> String {
    if suffix.is_empty() {
        oid.to_string()
    } else {
        format!("{oid}.{suffix}")
    }
}

/// Finds the longest registered prefix of `oid`, returning the entry and suffix.
fn lookup<'a>(registry: &'a Registry, oid: &str) -> Option<(&'a Entry, String)> {
    let oid = normalize(oid);
    let arcs: Vec<&str> = oid.trim_start_matches('.').split('.').collect();
    for end in (1..=arcs.len()).rev() {
        let prefix = format!(".{}", arcs[..end].join("."));
        if let Some(entry) = registry.oids.get(&prefix) {
            let suffix = if end == arcs.len() {
                String::new()
            } else {
                format!(".{}", arcs[end..].join("."))
            };
            return Some((entry, suffix));
        }
    }
    None
}

/// Renders an OID using net-snmp's output formats.
pub(crate) fn render(oid: &str, format: i64) -> Option<String> {
    with_registry(|registry| {
        let (entry, suffix) = lookup(registry, oid)?;
        match format {
            1 => Some(format!("{}{}", entry.name, suffix)),
            2 => Some(format!("{}::{}{}", entry.module, entry.name, suffix)),
            3 => Some(render_full(registry, oid)),
            5 => {
                let parent = parent_name(registry, &entry.oid);
                match parent {
                    Some(parent) => Some(format!("{}.{}{}", parent, entry.name, suffix)),
                    None => Some(format!("{}{}", entry.name, suffix)),
                }
            }
            _ => None,
        }
    })
}

/// Builds net-snmp's `Full` symbolic path.
fn render_full(registry: &Registry, oid: &str) -> String {
    let oid = normalize(oid);
    let arcs: Vec<&str> = oid.trim_start_matches('.').split('.').collect();
    let mut names = Vec::new();
    for end in 1..=arcs.len() {
        let prefix = format!(".{}", arcs[..end].join("."));
        match registry.oids.get(&prefix) {
            Some(entry) => names.push(entry.name.clone()),
            None => names.push(arcs[end - 1].to_string()),
        }
    }
    format!(".{}", names.join("."))
}

/// Returns the parent entry name for UCD rendering.
fn parent_name(registry: &Registry, oid: &str) -> Option<String> {
    let arcs: Vec<&str> = oid.trim_start_matches('.').split('.').collect();
    for end in (1..arcs.len()).rev() {
        let prefix = format!(".{}", arcs[..end].join("."));
        if let Some(entry) = registry.oids.get(&prefix) {
            return Some(entry.name.clone());
        }
    }
    None
}

/// Returns the enum label for `value` at the longest matching object OID.
pub(crate) fn enum_label(oid: &str, value: i64) -> Option<String> {
    with_registry(|registry| {
        let (entry, _) = lookup(registry, oid)?;
        entry
            .enums
            .iter()
            .find(|(candidate, _)| *candidate == value)
            .map(|(_, label)| label.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a small MIB and renders its objects in every format.
    #[test]
    fn parses_and_renders_mib_objects() {
        let text = r#"
ELEPHC-MIB DEFINITIONS ::= BEGIN
elephc OBJECT IDENTIFIER ::= { enterprises 424242 }
elephcName OBJECT-TYPE
    SYNTAX DisplayString
    MAX-ACCESS read-only
    STATUS current
    ::= { elephc 1 }
elephcState OBJECT-TYPE
    SYNTAX INTEGER { up(1), down(2) }
    MAX-ACCESS read-only
    STATUS current
    ::= { elephc 2 }
END
"#;
        with_registry(|registry| {
            let cleaned = strip_comments(text);
            for block in blocks(&cleaned) {
                let tokens = tokenize(&block);
                if tokens.is_empty() || tokens[0] == "ELEPHC-MIB" {
                    continue;
                }
                let name = tokens[0].clone();
                let Some(position) = tokens.iter().position(|token| token == "::=") else {
                    continue;
                };
                if tokens.get(position + 1).map(String::as_str) != Some("{") {
                    continue;
                }
                let oid = parse_oid_value(registry, &tokens[position + 2..]).unwrap();
                registry.oids.insert(
                    oid.clone(),
                    Entry {
                        module: "ELEPHC-MIB".to_string(),
                        name: name.clone(),
                        oid: oid.clone(),
                        enums: parse_enums(&tokens[..position]),
                    },
                );
                registry.names.insert(name.to_ascii_lowercase(), oid);
            }
        });
        assert_eq!(resolve("elephcName.0").as_deref(), Some(".1.3.6.1.4.1.424242.1.0"));
        assert_eq!(
            render(".1.3.6.1.4.1.424242.1.0", 1).as_deref(),
            Some("elephcName.0")
        );
        assert_eq!(
            render(".1.3.6.1.4.1.424242.1.0", 2).as_deref(),
            Some("ELEPHC-MIB::elephcName.0")
        );
        assert_eq!(
            render(".1.3.6.1.4.1.424242.1.0", 3).as_deref(),
            Some(".iso.org.dod.internet.private.enterprises.elephc.elephcName.0")
        );
        assert_eq!(
            render(".1.3.6.1.4.1.424242.2.0", 5).as_deref(),
            Some("elephc.elephcState.0")
        );
        assert_eq!(
            enum_label(".1.3.6.1.4.1.424242.2.0", 2).as_deref(),
            Some("down")
        );
    }
}
