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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// How one table INDEX column renders its instance sub-identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndexKind {
    /// Integer, enum, and counter columns consume one sub-identifier.
    Integer,
    /// Fixed/flexible octet strings are length-prefixed unless IMPLIED.
    String,
    /// IPv4 address columns consume four sub-identifiers.
    IpAddress,
    /// OBJECT IDENTIFIER columns are length-prefixed.
    Oid,
    /// A column whose syntax could not be resolved.
    Unknown,
}

/// One resolved table INDEX column.
#[derive(Clone, Debug)]
struct IndexColumn {
    /// Column value encoding.
    kind: IndexKind,
    /// Whether the column is the final `IMPLIED` variable-length index.
    implied: bool,
}

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
    /// Whether the object has a textual DISPLAY-HINT (e.g. DisplayString).
    hinted: bool,
    /// The `UNITS "..."` clause, if present.
    units: Option<String>,
    /// The first token after `SYNTAX`, used to type table indexes.
    syntax: Option<String>,
    /// `INDEX { ... }` names with their `IMPLIED` flag.
    index_names: Vec<(String, bool)>,
    /// Resolved `INDEX` columns for a table entry.
    index_columns: Vec<IndexColumn>,
}

/// MIB parser option: permit underscores inside SMI identifiers.
static ALLOW_UNDERSCORES: AtomicBool = AtomicBool::new(false);
/// MIB parser option: terminate `--` comments at the next `--`, not just newline.
static COMMENT_TERM: AtomicBool = AtomicBool::new(false);
/// MIB parser option: replace an existing object instead of keeping it.
static REPLACE_MIB: AtomicBool = AtomicBool::new(false);

/// Reports whether underscores are currently allowed in identifiers.
fn allow_underscores() -> bool {
    ALLOW_UNDERSCORES.load(Ordering::Relaxed)
}

/// Reports whether comments are terminated by the next `--` marker.
fn comment_term() -> bool {
    COMMENT_TERM.load(Ordering::Relaxed)
}

/// Stores a MIB parser option (`0` underscores, `1` comment term, `2` replace).
pub(crate) fn set_option(option: i64, enable: bool) {
    match option {
        0 => ALLOW_UNDERSCORES.store(enable, Ordering::Relaxed),
        1 => COMMENT_TERM.store(enable, Ordering::Relaxed),
        2 => REPLACE_MIB.store(enable, Ordering::Relaxed),
        _ => {}
    }
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
        ("netSnmpAgentOIDs", ".1.3.6.1.4.1.8072.3.2", "NET-SNMP-MIB"),
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
            hinted: matches!(
                name,
                "sysDescr" | "sysContact" | "sysName" | "sysLocation"
            ),
            units: None,
            syntax: None,
            index_names: Vec::new(),
            index_columns: Vec::new(),
        };
        registry.oids.insert(oid.to_string(), entry);
        registry.names.insert(name.to_ascii_lowercase(), oid.to_string());
        registry.qualified.insert(
            format!("{}::{}", module.to_ascii_lowercase(), name.to_ascii_lowercase()),
            oid.to_string(),
        );
    }
}

/// Reports whether `token` is an SMI identifier under the active parser options.
fn is_identifier(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || (first == '_' && allow_underscores())) {
        return false;
    }
    if !allow_underscores() && token.contains('_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
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
                if comment_term() && next == '-' && chars.peek() == Some(&'-') {
                    chars.next();
                    break;
                }
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

/// Returns the first token after `SYNTAX`.
fn parse_syntax(tokens: &[String]) -> Option<String> {
    let position = tokens.iter().position(|token| token == "SYNTAX")?;
    tokens.get(position + 1).cloned()
}

/// Returns the `UNITS "..."` clause value.
fn parse_units(tokens: &[String]) -> Option<String> {
    let position = tokens.iter().position(|token| token == "UNITS")?;
    tokens.get(position + 1).cloned()
}

/// Parses `INDEX { name, IMPLIED name }` into `(name, implied)` pairs.
fn parse_index_names(tokens: &[String]) -> Vec<(String, bool)> {
    let Some(position) = tokens.iter().position(|token| token == "INDEX") else {
        return Vec::new();
    };
    let mut index = position + 1;
    while index < tokens.len() && tokens[index] != "{" {
        index += 1;
    }
    if index >= tokens.len() {
        return Vec::new();
    }
    index += 1;
    let mut names = Vec::new();
    let mut implied = false;
    while index < tokens.len() && tokens[index] != "}" {
        let token = &tokens[index];
        if token == "IMPLIED" {
            implied = true;
        } else if token != "," && is_identifier(token) {
            names.push((token.clone(), implied));
            implied = false;
        }
        index += 1;
    }
    names
}

/// Maps a SYNTAX token to its table-index encoding.
fn index_kind(syntax: Option<&str>) -> IndexKind {
    let Some(syntax) = syntax else {
        return IndexKind::Unknown;
    };
    if syntax.eq_ignore_ascii_case("IpAddress") {
        return IndexKind::IpAddress;
    }
    if syntax.eq_ignore_ascii_case("OBJECT")
        || syntax.eq_ignore_ascii_case("OBJECTID")
        || syntax.eq_ignore_ascii_case("OID")
    {
        return IndexKind::Oid;
    }
    if syntax.eq_ignore_ascii_case("DisplayString")
        || syntax.eq_ignore_ascii_case("SnmpAdminString")
        || syntax.eq_ignore_ascii_case("MacAddress")
        || syntax.eq_ignore_ascii_case("PhysAddress")
        || syntax.eq_ignore_ascii_case("OCTET")
        || syntax.eq_ignore_ascii_case("STRING")
    {
        return IndexKind::String;
    }
    if syntax.eq_ignore_ascii_case("INTEGER")
        || syntax.eq_ignore_ascii_case("INTEGER32")
        || syntax.eq_ignore_ascii_case("Unsigned32")
        || syntax.eq_ignore_ascii_case("Gauge32")
        || syntax.eq_ignore_ascii_case("Counter32")
        || syntax.eq_ignore_ascii_case("Counter64")
        || syntax.eq_ignore_ascii_case("TimeTicks")
        || syntax.eq_ignore_ascii_case("BITS")
    {
        return IndexKind::Integer;
    }
    IndexKind::Unknown
}

/// Reports whether the SYNTAX clause declares a textual DISPLAY-HINT.
fn parse_hint(tokens: &[String]) -> bool {
    if tokens.iter().any(|token| token == "DISPLAY-HINT") {
        return true;
    }
    if let Some(position) = tokens.iter().position(|token| token == "SYNTAX") {
        if let Some(next) = tokens.get(position + 1) {
            return next == "DisplayString"
                || next == "SnmpAdminString"
                || next == "MacAddress";
        }
    }
    false
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
        let name_key = name.to_ascii_lowercase();
        if !REPLACE_MIB.load(Ordering::Relaxed)
            && (registry.oids.contains_key(&oid) || registry.names.contains_key(&name_key))
        {
            continue;
        }
        let entry = Entry {
            module: module.clone(),
            name: name.clone(),
            oid: oid.clone(),
            enums,
            hinted: parse_hint(&tokens[..position]),
            units: parse_units(&tokens[..position]),
            syntax: parse_syntax(&tokens[..position]),
            index_names: parse_index_names(&tokens[..position]),
            index_columns: Vec::new(),
        };
        registry.oids.insert(oid.clone(), entry);
        registry.names.insert(name_key, oid.clone());
        registry.qualified.insert(
            format!("{}::{}", module.to_ascii_lowercase(), name.to_ascii_lowercase()),
            oid.clone(),
        );
        loaded.push(oid);
    }
    if !loaded.is_empty() {
        let mut registry = registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut updates = Vec::new();
        for oid in &loaded {
            let index_names = match registry.oids.get(oid) {
                Some(entry) if !entry.index_names.is_empty() => entry.index_names.clone(),
                _ => continue,
            };
            let mut columns = Vec::new();
            let mut resolved = true;
            for (index_name, implied) in index_names {
                let Some(index_oid) = resolve_name(&registry, &index_name) else {
                    resolved = false;
                    break;
                };
                let Some(index_entry) = registry.oids.get(&index_oid) else {
                    resolved = false;
                    break;
                };
                columns.push(IndexColumn {
                    kind: index_kind(index_entry.syntax.as_deref()),
                    implied,
                });
            }
            if resolved {
                updates.push((oid.clone(), columns));
            }
        }
        for (oid, columns) in updates {
            if let Some(entry) = registry.oids.get_mut(&oid) {
                entry.index_columns = columns;
            }
        }
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

/// Reports whether the object at `oid` has a textual DISPLAY-HINT.
pub(crate) fn string_hint(oid: &str) -> bool {
    with_registry(|registry| {
        lookup(registry, oid)
            .map(|(entry, _)| entry.hinted)
            .unwrap_or(false)
    })
}

/// Returns the `UNITS` clause for the longest matching object.
pub(crate) fn units(oid: &str) -> Option<String> {
    with_registry(|registry| {
        lookup(registry, oid).and_then(|(entry, _)| entry.units.clone())
    })
}

/// Renders an OID using net-snmp's output formats.
pub(crate) fn render(oid: &str, format: i64) -> Option<String> {
    render_with_options(oid, format, false, false)
}

/// Renders an OID with net-snmp's numeric/extended table-index options.
pub(crate) fn render_with_options(
    oid: &str,
    format: i64,
    numeric_index: bool,
    extended_index: bool,
) -> Option<String> {
    with_registry(|registry| {
        let (entry, suffix) = lookup(registry, oid)?;
        let rendered_suffix = render_index(registry, entry, &suffix, numeric_index, extended_index);
        match format {
            1 => Some(format!("{}{}", entry.name, rendered_suffix)),
            2 => Some(format!("{}::{}{}", entry.module, entry.name, rendered_suffix)),
            3 => Some(render_full(registry, entry, &rendered_suffix)),
            5 => {
                let parent = parent_name(registry, &entry.oid);
                match parent {
                    Some(parent) => Some(format!("{}.{}{}", parent, entry.name, rendered_suffix)),
                    None => Some(format!("{}{}", entry.name, rendered_suffix)),
                }
            }
            _ => None,
        }
    })
}

/// Builds net-snmp's `Full` symbolic path for a matched object and instance suffix.
fn render_full(registry: &Registry, entry: &Entry, suffix: &str) -> String {
    let arcs: Vec<&str> = entry.oid.trim_start_matches('.').split('.').collect();
    let mut names = Vec::new();
    for end in 1..=arcs.len() {
        let prefix = format!(".{}", arcs[..end].join("."));
        match registry.oids.get(&prefix) {
            Some(entry) => names.push(entry.name.clone()),
            None => names.push(arcs[end - 1].to_string()),
        }
    }
    format!(".{}{}", names.join("."), suffix)
}

/// One decoded table-index value.
enum IndexValue {
    /// Integer, enum, or counter index.
    Integer(i64),
    /// Octet-string index.
    String(String),
    /// IPv4 address index.
    IpAddress(String),
    /// OBJECT IDENTIFIER index with its raw encoded suffix and decoded arcs.
    Oid { encoded: String, arcs: Vec<u64> },
}

/// Finds the nearest ancestor entry that declares table INDEX columns.
fn index_columns_for<'a>(registry: &'a Registry, entry: &Entry) -> Option<&'a [IndexColumn]> {
    let mut arcs: Vec<&str> = entry.oid.trim_start_matches('.').split('.').collect();
    while !arcs.is_empty() {
        let prefix = format!(".{}", arcs.join("."));
        if let Some(candidate) = registry.oids.get(&prefix) {
            if !candidate.index_columns.is_empty() {
                return Some(&candidate.index_columns);
            }
        }
        arcs.pop();
    }
    None
}

/// Decodes instance sub-identifiers using the table's INDEX column types.
fn decode_index(columns: &[IndexColumn], arcs: &[u64]) -> Option<Vec<IndexValue>> {
    let mut values = Vec::new();
    let mut position = 0;
    for column in columns {
        match column.kind {
            IndexKind::Integer => {
                let value = *arcs.get(position)?;
                position += 1;
                values.push(IndexValue::Integer(value as i64));
            }
            IndexKind::String => {
                let end = if column.implied {
                    arcs.len()
                } else {
                    let length = *arcs.get(position)? as usize;
                    let end = position + 1 + length;
                    if end > arcs.len() {
                        return None;
                    }
                    position += 1;
                    end
                };
                let bytes: Vec<u8> = arcs[position..end].iter().map(|arc| *arc as u8).collect();
                position = end;
                values.push(IndexValue::String(String::from_utf8_lossy(&bytes).into_owned()));
            }
            IndexKind::IpAddress => {
                if position + 4 > arcs.len() {
                    return None;
                }
                let address = format!(
                    "{}.{}.{}.{}",
                    arcs[position],
                    arcs[position + 1],
                    arcs[position + 2],
                    arcs[position + 3]
                );
                position += 4;
                values.push(IndexValue::IpAddress(address));
            }
            IndexKind::Oid => {
                let length = *arcs.get(position)? as usize;
                let end = position + 1 + length;
                if end > arcs.len() {
                    return None;
                }
                let encoded = arcs[position..end]
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(".");
                let decoded = arcs[position + 1..end].to_vec();
                position = end;
                values.push(IndexValue::Oid {
                    encoded,
                    arcs: decoded,
                });
            }
            IndexKind::Unknown => return None,
        }
    }
    if position != arcs.len() {
        return None;
    }
    Some(values)
}

/// Renders a table instance suffix using net-snmp's default index syntax.
fn render_index(
    registry: &Registry,
    entry: &Entry,
    suffix: &str,
    numeric_index: bool,
    extended_index: bool,
) -> String {
    if suffix.is_empty() || numeric_index {
        return suffix.to_string();
    }
    let Some(columns) = index_columns_for(registry, entry) else {
        return suffix.to_string();
    };
    let trimmed = suffix.trim_start_matches('.');
    let parts: Vec<&str> = trimmed.split('.').collect();
    let mut arcs = Vec::with_capacity(parts.len());
    for part in &parts {
        let Ok(arc) = part.parse::<u64>() else {
            return suffix.to_string();
        };
        arcs.push(arc);
    }
    let Some(values) = decode_index(columns, &arcs) else {
        return suffix.to_string();
    };
    let mut rendered = String::new();
    for value in &values {
        if extended_index {
            rendered.push_str(&render_extended_index(registry, value));
        } else {
            rendered.push_str(&render_default_index(value));
        }
    }
    rendered
}

/// Renders one index value in net-snmp's default syntax.
fn render_default_index(value: &IndexValue) -> String {
    match value {
        IndexValue::Integer(value) => format!(".{value}"),
        IndexValue::String(value) => format!(".\"{}\"", index_text(value)),
        IndexValue::IpAddress(value) => format!(".{value}"),
        IndexValue::Oid { encoded, .. } => format!(".{encoded}"),
    }
}

/// Renders one index value in net-snmp's extended `[...]` syntax.
fn render_extended_index(registry: &Registry, value: &IndexValue) -> String {
    match value {
        IndexValue::Integer(value) => format!("[{value}]"),
        IndexValue::String(value) => format!("[STRING: {}]", index_text(value)),
        IndexValue::IpAddress(value) => format!("[{value}]"),
        IndexValue::Oid { arcs, .. } => {
            let oid = format!(
                ".{}",
                arcs.iter().map(u64::to_string).collect::<Vec<_>>().join(".")
            );
            match render_symbolic(registry, &oid) {
                Some(rendered) => format!("[{rendered}]"),
                None => format!("[{oid}]"),
            }
        }
    }
}

/// Escapes non-printable index bytes while keeping printable text.
fn index_text(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_control() {
            for byte in ch.to_string().as_bytes() {
                out.push_str(&format!("\\x{byte:02x}"));
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Renders module-qualified symbolic name for an OID, if registered.
fn render_symbolic(registry: &Registry, oid: &str) -> Option<String> {
    let (entry, suffix) = lookup(registry, oid)?;
    Some(format!("{}::{}{}", entry.module, entry.name, suffix))
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

    /// Loads a table MIB and renders units plus string/integer index forms.
    #[test]
    fn loads_units_and_table_indexes() {
        let path = std::env::temp_dir().join("elephc-snmp-unit-index-test.mib");
        std::fs::write(
            &path,
            r#"
ELEPHC-UNIT-MIB DEFINITIONS ::= BEGIN
elephcUnit OBJECT IDENTIFIER ::= { enterprises 424243 }
elephcUnitTable OBJECT-TYPE
    SYNTAX SEQUENCE OF ElephcUnitEntry
    MAX-ACCESS not-accessible
    STATUS current
    ::= { elephcUnit 1 }
elephcUnitEntry OBJECT-TYPE
    SYNTAX ElephcUnitEntry
    MAX-ACCESS not-accessible
    STATUS current
    INDEX { elephcUnitName }
    ::= { elephcUnitTable 1 }
ElephcUnitEntry ::= SEQUENCE { elephcUnitName DisplayString, elephcUnitValue Integer32 }
elephcUnitName OBJECT-TYPE
    SYNTAX DisplayString
    MAX-ACCESS not-accessible
    STATUS current
    ::= { elephcUnitEntry 1 }
elephcUnitValue OBJECT-TYPE
    SYNTAX Integer32
    UNITS "Bytes"
    MAX-ACCESS read-only
    STATUS current
    ::= { elephcUnitEntry 2 }
END
"#,
        )
        .expect("write table MIB");
        assert!(read_mib(path.to_str().expect("utf-8 path")));
        let instance = ".1.3.6.1.4.1.424243.1.1.2.5.97.108.112.104.97";
        assert_eq!(
            resolve("elephcUnitValue.5.97.108.112.104.97").as_deref(),
            Some(instance)
        );
        assert_eq!(units(instance).as_deref(), Some("Bytes"));
        assert_eq!(
            render_with_options(instance, 2, false, false).as_deref(),
            Some("ELEPHC-UNIT-MIB::elephcUnitValue.\"alpha\"")
        );
        assert_eq!(
            render_with_options(instance, 2, true, false).as_deref(),
            Some("ELEPHC-UNIT-MIB::elephcUnitValue.5.97.108.112.104.97")
        );
        assert_eq!(
            render_with_options(instance, 2, false, true).as_deref(),
            Some("ELEPHC-UNIT-MIB::elephcUnitValue[STRING: alpha]")
        );
    }

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
                        hinted: parse_hint(&tokens[..position]),
                        units: parse_units(&tokens[..position]),
                        syntax: parse_syntax(&tokens[..position]),
                        index_names: parse_index_names(&tokens[..position]),
                        index_columns: Vec::new(),
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
