//! Purpose:
//! Builds the versioned debug-metadata document from the assembly markers emitted
//! by codegen, writes the `.edbg.json` sidecar, and embeds the same JSON in the
//! debug executable behind `elephc_debug_metadata` / `_len` symbols.
//!
//! Called from:
//! - `crate::pipeline::backend::emit_and_link()` when `--debug` is active.
//!
//! Key details:
//! - The assembly is the source of truth: `@file`, `@fn`, `@src` and `@local`
//!   markers are shared with the source map/DWARF readers.
//! - Source ids are canonicalized by physical path; include variants that share
//!   a path collapse to one source entry.
//! - The embedded JSON is terminated by one NUL and accompanied by an explicit
//!   byte length, so a runtime agent never has to scan it.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::codegen::platform::{Platform, Target};
use crate::source_map::{decode_hex_path, sha256_hex};

/// One physical source file in the metadata document.
struct SourceFile {
    id: usize,
    path: String,
    sha256: Option<String>,
    line_count: Option<usize>,
    entry: bool,
}

/// One emitted function region.
struct FunctionMeta {
    id: usize,
    name: String,
    symbol: String,
    source_id: Option<usize>,
    kind: String,
    flags: Vec<String>,
    synthetic: bool,
    asm_start: usize,
    asm_end: Option<usize>,
}

/// One instruction mapping / debug site.
struct SiteMeta {
    id: usize,
    function_id: Option<usize>,
    source_id: usize,
    line: usize,
    column: usize,
    end_line: Option<usize>,
    end_column: Option<usize>,
    op: Option<String>,
    origin: Option<String>,
}

/// One emitted class and its layout properties.
struct ClassMeta {
    id: usize,
    name: String,
    parent: String,
    is_abstract: bool,
    /// Byte offset of the per-object dynamic-property hash, when the layout reserves it.
    dynamic_offset: Option<usize>,
    /// Static `__debugInfo()` projection or runtime-only marker.
    debug_info: DebugInfoMeta,
    /// Hooked properties keyed by PHP-visible name.
    property_hooks: HashMap<String, PropertyHookMeta>,
    properties: Vec<PropertyMeta>,
}

/// Debugger view of a class's `__debugInfo()` support.
struct DebugInfoMeta {
    /// `none`, `projection` or `runtime`.
    kind: String,
    entries: Vec<DebugInfoEntryMeta>,
}

/// One projected `__debugInfo()` entry: a live declared-property read or a
/// compile-time literal value.
struct DebugInfoEntryMeta {
    key: String,
    /// Declared property supplying the value, when the entry reads the object.
    property: Option<String>,
    /// Display-only literal text, when the entry is a safe compile-time value.
    value: Option<String>,
    /// PHP type of the literal value.
    php_type: Option<String>,
}

/// One source-level conditional branch and its two edge site ids.
struct BranchMeta {
    source_id: usize,
    line: usize,
    column: usize,
    /// `conditional` or `switch`.
    kind: String,
    /// One debug site id per source edge; two for a conditional, cases+1 for a switch.
    edges: Vec<usize>,
}

/// One logical or switch path group.
struct PathMeta {
    function_id: usize,
    source_id: usize,
    line: usize,
    column: usize,
    kind: String,
    /// Whether planning hit the full-prefix cap for this group.
    truncated: bool,
    paths: Vec<PathEntryMeta>,
}

/// One terminal route inside a path group.
struct PathEntryMeta {
    site: usize,
    label: String,
}

/// One declared instance property in a class layout.
struct PropertyMeta {
    name: String,
    offset: usize,
    php_type: String,
    visibility: String,
    by_ref: bool,
    declared: bool,
}

/// Debugger view of one property's hooks.
struct PropertyHookMeta {
    get: bool,
    set: bool,
    get_by_ref: bool,
    is_virtual: bool,
}

/// One named PHP local with its frame offset and PHP type.
struct LocalMeta {
    function_id: usize,
    name: String,
    frame_offset: usize,
    php_type: String,
    kind: String,
    storage: String,
    state_offset: Option<usize>,
    param: Option<usize>,
    by_ref: bool,
    variadic: bool,
    sensitive: bool,
}

/// Builds the v1 metadata JSON from `asm`.
///
/// `source_path` is the entry file named on the CLI; it is always source id 0.
pub(crate) fn build(asm: &str, source_path: &Path) -> Result<String, String> {
    let root_path = source_path.display().to_string();
    let root = source_file(0, root_path.clone(), true);
    let mut sources: BTreeMap<usize, SourceFile> = BTreeMap::new();
    sources.insert(0, root);
    let mut path_ids: HashMap<String, usize> = HashMap::new();
    path_ids.insert(root_path, 0);
    let mut source_aliases: HashMap<usize, usize> = HashMap::new();
    source_aliases.insert(0, 0);

    let mut functions: Vec<FunctionMeta> = Vec::new();
    let mut sites: Vec<SiteMeta> = Vec::new();
    let mut branches: Vec<BranchMeta> = Vec::new();
    let mut paths: Vec<PathMeta> = Vec::new();
    let mut locals: Vec<LocalMeta> = Vec::new();
    let mut classes: Vec<ClassMeta> = Vec::new();
    let mut class_index: HashMap<usize, usize> = HashMap::new();
    let mut open_function: Option<usize> = None;
    let mut last_line = 0;

    for (idx, line) in asm.lines().enumerate() {
        let asm_line = idx + 1;
        last_line = asm_line;

        if let Some(marker) = line.split("@file ").nth(1) {
            let source_id = parse_marker_number(marker, "id")?;
            let path_hex = marker_value(marker, "path_hex")
                .ok_or_else(|| format!("invalid debug file marker '{}'", line.trim()))?;
            let path = decode_hex_path(path_hex)
                .ok_or_else(|| format!("invalid debug file path '{}'", line.trim()))?;
            let canonical = if let Some(existing) = path_ids.get(&path).copied() {
                existing
            } else {
                sources
                    .entry(source_id)
                    .or_insert_with(|| source_file(source_id, path.clone(), false));
                path_ids.insert(path, source_id);
                source_id
            };
            source_aliases.insert(source_id, canonical);
            continue;
        }

        if let Some(marker) = line.split("@fn ").nth(1) {
            let name = marker_value(marker, "name")
                .ok_or_else(|| format!("invalid debug function marker '{}'", line.trim()))?;
            let symbol = marker_value(marker, "symbol")
                .ok_or_else(|| format!("invalid debug function marker '{}'", line.trim()))?;
            if let Some(open) = open_function.take() {
                functions[open].asm_end = Some(asm_line.saturating_sub(1));
            }
            let raw_source = optional_marker_number(marker, "file")?;
            let source_id = raw_source.map(|id| source_aliases.get(&id).copied().unwrap_or(id));
            let kind = marker_value(marker, "kind").unwrap_or("function").to_string();
            let flags = marker_value(marker, "flags")
                .map(|flags| flags.split(',').map(str::to_string).collect())
                .unwrap_or_default();
            let id = optional_marker_number(marker, "id")?.unwrap_or(functions.len());
            functions.push(FunctionMeta {
                id,
                name: name.to_string(),
                symbol: symbol.to_string(),
                source_id,
                kind,
                flags,
                synthetic: marker_value(marker, "synthetic") == Some("1"),
                asm_start: asm_line,
                asm_end: None,
            });
            open_function = Some(id);
            continue;
        }

        if line.contains("@endfn") {
            if let Some(open) = open_function.take() {
                functions[open].asm_end = Some(asm_line);
            }
            continue;
        }

        if let Some(marker) = line.split("@class ").nth(1) {
            let id = parse_marker_number(marker, "id")?;
            let name_hex = marker_value(marker, "name_hex")
                .ok_or_else(|| format!("invalid debug class marker '{}'", line.trim()))?;
            let name = decode_hex_path(name_hex)
                .ok_or_else(|| format!("invalid debug class name '{}'", line.trim()))?;
            let parent = marker_value(marker, "parent_hex")
                .and_then(decode_hex_path)
                .unwrap_or_default();
            let is_abstract = marker_value(marker, "abstract") == Some("1");
            class_index.insert(id, classes.len());
            let dynamic_offset = marker_value(marker, "dynamic_offset")
                .filter(|value| !value.is_empty())
                .map(|value| {
                    value.parse::<usize>().map_err(|_| {
                        format!("invalid debug class dynamic offset '{}'", line.trim())
                    })
                })
                .transpose()?;
            let debug_info_kind = marker_value(marker, "debuginfo")
                .filter(|value| !value.is_empty())
                .unwrap_or("none")
                .to_string();
            classes.push(ClassMeta {
                id,
                name,
                parent,
                is_abstract,
                dynamic_offset,
                debug_info: DebugInfoMeta {
                    kind: debug_info_kind,
                    entries: Vec::new(),
                },
                property_hooks: HashMap::new(),
                properties: Vec::new(),
            });
            continue;
        }

        if let Some(marker) = line.split("@debuginfo ").nth(1) {
            let class_id = parse_marker_number(marker, "class")?;
            let Some(index) = class_index.get(&class_id).copied() else {
                continue;
            };
            let key_hex = marker_value(marker, "key_hex")
                .ok_or_else(|| format!("invalid debuginfo marker '{}'", line.trim()))?;
            let key = decode_hex_path(key_hex)
                .ok_or_else(|| format!("invalid debuginfo key '{}'", line.trim()))?;
            let property = marker_value(marker, "property_hex")
                .map(|property_hex| {
                    decode_hex_path(property_hex).ok_or_else(|| {
                        format!("invalid debuginfo property '{}'", line.trim())
                    })
                })
                .transpose()?;
            let value = marker_value(marker, "value_hex")
                .map(|value_hex| {
                    decode_hex_path(value_hex)
                        .ok_or_else(|| format!("invalid debuginfo value '{}'", line.trim()))
                })
                .transpose()?;
            let php_type = marker_value(marker, "type_hex")
                .map(|type_hex| {
                    decode_hex_path(type_hex)
                        .ok_or_else(|| format!("invalid debuginfo type '{}'", line.trim()))
                })
                .transpose()?;
            if property.is_none() && value.is_none() {
                return Err(format!("invalid debuginfo marker '{}'", line.trim()));
            }
            classes[index].debug_info.entries.push(DebugInfoEntryMeta {
                key,
                property,
                value,
                php_type,
            });
            continue;
        }

        if let Some(marker) = line.split("@prop ").nth(1) {
            let class_id = parse_marker_number(marker, "class")?;
            let Some(index) = class_index.get(&class_id).copied() else {
                continue;
            };
            let name_hex = marker_value(marker, "name_hex")
                .ok_or_else(|| format!("invalid debug property marker '{}'", line.trim()))?;
            let name = decode_hex_path(name_hex)
                .ok_or_else(|| format!("invalid debug property name '{}'", line.trim()))?;
            let type_hex = marker_value(marker, "type_hex")
                .ok_or_else(|| format!("invalid debug property type '{}'", line.trim()))?;
            let php_type = decode_hex_path(type_hex)
                .ok_or_else(|| format!("invalid debug property type '{}'", line.trim()))?;
            let (get, set) = match marker_value(marker, "hooks").unwrap_or("none") {
                "get" => (true, false),
                "set" => (false, true),
                "get+set" => (true, true),
                _ => (false, false),
            };
            let get_by_ref = marker_value(marker, "get_by_ref") == Some("1");
            if get || set || get_by_ref {
                classes[index].property_hooks.insert(
                    name.clone(),
                    PropertyHookMeta {
                        get,
                        set,
                        get_by_ref,
                        is_virtual: marker_value(marker, "virtual") == Some("1"),
                    },
                );
            }
            classes[index].properties.push(PropertyMeta {
                name,
                offset: parse_marker_number(marker, "offset")?,
                php_type,
                visibility: marker_value(marker, "visibility")
                    .unwrap_or("public")
                    .to_string(),
                by_ref: marker_value(marker, "by_ref") == Some("1"),
                declared: marker_value(marker, "declared") == Some("1"),
            });
            continue;
        }

        if let Some(marker) = line.split("@branch ").nth(1) {
            let source_id = parse_marker_number(marker, "source")?;
            let line_number = parse_marker_number(marker, "line")?;
            let column = parse_marker_number(marker, "col")?;
            let kind = marker_value(marker, "kind")
                .unwrap_or("conditional")
                .to_string();
            let edges = if let Some(edges) = marker_value(marker, "edges") {
                edges
                    .split(',')
                    .map(|edge| {
                        edge.trim()
                            .parse::<usize>()
                            .map_err(|_| format!("invalid branch edge '{edge}'"))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                vec![
                    parse_marker_number(marker, "then_site")?,
                    parse_marker_number(marker, "else_site")?,
                ]
            };
            branches.push(BranchMeta {
                source_id,
                line: line_number,
                column,
                kind,
                edges,
            });
            continue;
        }

        if let Some(marker) = line.split("@pathgroup ").nth(1) {
            let Some(function_id) = open_function else {
                continue;
            };
            let source_id = parse_marker_number(marker, "source")?;
            let line_number = parse_marker_number(marker, "line")?;
            let column = parse_marker_number(marker, "col")?;
            let kind = marker_value(marker, "kind")
                .unwrap_or("logical")
                .to_string();
            paths.push(PathMeta {
                function_id,
                source_id,
                line: line_number,
                column,
                kind,
                truncated: marker_value(marker, "truncated") == Some("1"),
                paths: Vec::new(),
            });
            continue;
        }

        if let Some(marker) = line.split("@path ").nth(1) {
            let Some(function_id) = open_function else {
                continue;
            };
            let source_id = parse_marker_number(marker, "source")?;
            let line_number = parse_marker_number(marker, "line")?;
            let column = parse_marker_number(marker, "col")?;
            let kind = marker_value(marker, "kind")
                .unwrap_or("logical")
                .to_string();
            let site = parse_marker_number(marker, "site")?;
            let label = marker_value(marker, "label")
                .unwrap_or("path")
                .to_string();
            if let Some(last) = paths.last_mut() {
                if last.function_id == function_id
                    && last.source_id == source_id
                    && last.line == line_number
                    && last.column == column
                    && last.kind == kind
                {
                    last.paths.push(PathEntryMeta { site, label });
                    continue;
                }
            }
            // Legacy metadata without `@pathgroup` still groups by position.
            paths.push(PathMeta {
                function_id,
                source_id,
                line: line_number,
                column,
                kind,
                truncated: false,
                paths: vec![PathEntryMeta { site, label }],
            });
            continue;
        }

        if let Some(marker) = line.split("@src ").nth(1) {
            let raw_source = optional_marker_number(marker, "file")?;
            let Some(raw_source) = raw_source else {
                // A legacy marker without a file identity cannot be placed in a
                // physical source; the codegen debug build always emits one.
                continue;
            };
            let source_id = source_aliases.get(&raw_source).copied().unwrap_or(raw_source);
            let Some(site_id) = optional_marker_number(marker, "site")? else {
                // Non-guard instruction mappings stay in the source map; only
                // line-transition guards become debugger sites.
                continue;
            };
            let line_number = parse_marker_number(marker, "line")?;
            let column = parse_marker_number(marker, "col")?;
            let (end_line, end_column) = match marker_value(marker, "end") {
                Some(end) => {
                    let (end_line, end_column) = end
                        .split_once(':')
                        .ok_or_else(|| format!("invalid debug end marker '{}'", end))?;
                    (
                        Some(end_line.parse::<usize>().map_err(|err| {
                            format!("invalid debug end line '{}': {}", end_line, err)
                        })?),
                        Some(end_column.parse::<usize>().map_err(|err| {
                            format!("invalid debug end column '{}': {}", end_column, err)
                        })?),
                    )
                }
                None => (None, None),
            };
            sites.push(SiteMeta {
                id: site_id,
                function_id: open_function,
                source_id,
                line: line_number,
                column,
                end_line,
                end_column,
                op: marker_value(marker, "op").map(str::to_string),
                origin: marker_value(marker, "origin").map(str::to_string),
            });
            continue;
        }

        if let Some(marker) = line.split("@local ").nth(1) {
            let Some(function_id) = open_function else {
                continue;
            };
            let name_hex = marker_value(marker, "name_hex")
                .ok_or_else(|| format!("invalid debug local marker '{}'", line.trim()))?;
            let name = decode_hex_path(name_hex)
                .ok_or_else(|| format!("invalid debug local name '{}'", line.trim()))?;
            let type_hex = marker_value(marker, "type_hex")
                .ok_or_else(|| format!("invalid debug local marker '{}'", line.trim()))?;
            let php_type = decode_hex_path(type_hex)
                .ok_or_else(|| format!("invalid debug local type '{}'", line.trim()))?;
            locals.push(LocalMeta {
                function_id,
                name,
                frame_offset: parse_marker_number(marker, "offset")?,
                php_type,
                kind: marker_value(marker, "kind").unwrap_or("php_local").to_string(),
                storage: marker_value(marker, "storage").unwrap_or("raw").to_string(),
                state_offset: optional_marker_number(marker, "state_offset")?,
                param: optional_marker_number(marker, "param")?,
                by_ref: marker_value(marker, "by_ref") == Some("1"),
                variadic: marker_value(marker, "variadic") == Some("1"),
                sensitive: marker_value(marker, "sensitive") == Some("1"),
            });
            continue;
        }
    }

    if let Some(open) = open_function {
        functions[open].asm_end = Some(last_line);
    }

    render_json(
        source_path,
        &sources,
        &functions,
        &sites,
        &branches,
        &paths,
        &locals,
        &classes,
    )
}

/// Reads a source once and records its hash and line count.
fn source_file(id: usize, path: String, entry: bool) -> SourceFile {
    let bytes = fs::read(&path).ok();
    let sha256 = bytes.as_ref().map(|bytes| sha256_hex(bytes));
    let line_count = bytes.as_ref().map(|bytes| {
        if bytes.is_empty() {
            0
        } else {
            bytes.iter().filter(|byte| **byte == b'\n').count() + 1
        }
    });
    SourceFile {
        id,
        path,
        sha256,
        line_count,
        entry,
    }
}

/// Returns the value of a space-separated `key=value` token.
fn marker_value<'a>(marker: &'a str, key: &str) -> Option<&'a str> {
    marker
        .split_whitespace()
        .find_map(|token| token.strip_prefix(key)?.strip_prefix('='))
}

/// Parses a required numeric marker field.
fn parse_marker_number(marker: &str, key: &str) -> Result<usize, String> {
    let value = marker_value(marker, key)
        .ok_or_else(|| format!("invalid debug {} marker '{}'", key, marker.trim()))?;
    value
        .parse::<usize>()
        .map_err(|err| format!("invalid debug {} marker '{}': {}", key, value, err))
}

/// Parses an optional numeric marker field.
fn optional_marker_number(marker: &str, key: &str) -> Result<Option<usize>, String> {
    match marker_value(marker, key) {
        Some(value) => value
            .parse::<usize>()
            .map(Some)
            .map_err(|err| format!("invalid debug {} marker '{}': {}", key, value, err)),
        None => Ok(None),
    }
}

/// Renders the v1 JSON document.
fn render_json(
    source_path: &Path,
    sources: &BTreeMap<usize, SourceFile>,
    functions: &[FunctionMeta],
    sites: &[SiteMeta],
    branches: &[BranchMeta],
    paths: &[PathMeta],
    locals: &[LocalMeta],
    classes: &[ClassMeta],
) -> Result<String, String> {
    let source_values: Vec<serde_json::Value> = sources
        .values()
        .map(|source| {
            serde_json::json!({
                "id": source.id,
                "path": source.path,
                "sha256": source.sha256,
                "lines": source.line_count,
                "entry": source.entry,
            })
        })
        .collect();
    let function_values: Vec<serde_json::Value> = functions
        .iter()
        .map(|function| {
            let (line_start, line_end) = function_lines(function.id, sites);
            let params: Vec<serde_json::Value> = function_params(function.id, locals)
                .into_iter()
                .map(|(name, php_type, by_ref, variadic)| {
                    serde_json::json!({
                        "name": name,
                        "php_type": php_type,
                        "by_ref": by_ref,
                        "variadic": variadic,
                    })
                })
                .collect();
            serde_json::json!({
                "id": function.id,
                "name": function.name,
                "symbol": function.symbol,
                "source_id": function.source_id,
                "kind": function.kind,
                "asm_start": function.asm_start,
                "asm_end": function.asm_end.unwrap_or(function.asm_start),
                "line_start": line_start,
                "line_end": line_end,
                "synthetic": function.synthetic,
                "flags": function.flags,
                "params": params,
            })
        })
        .collect();
    let site_values: Vec<serde_json::Value> = sites
        .iter()
        .map(|site| {
            serde_json::json!({
                "id": site.id,
                "function_id": site.function_id,
                "source_id": site.source_id,
                "line": site.line,
                "column": site.column,
                "end_line": site.end_line,
                "end_column": site.end_column,
                "op": site.op,
                "origin": site.origin,
            })
        })
        .collect();
    let branch_values: Vec<serde_json::Value> = branches
        .iter()
        .map(|branch| {
            let mut value = serde_json::json!({
                "source_id": branch.source_id,
                "line": branch.line,
                "column": branch.column,
                "kind": branch.kind,
                "edges": branch.edges,
            });
            // Keep the v1 conditional spelling for clients that already read it;
            // `edges` is the general form used by switch branch points.
            if branch.kind == "conditional" && branch.edges.len() == 2 {
                value["then_site"] = serde_json::json!(branch.edges[0]);
                value["else_site"] = serde_json::json!(branch.edges[1]);
            }
            value
        })
        .collect();
    let path_values: Vec<serde_json::Value> = paths
        .iter()
        .map(|path| {
            let entries: Vec<serde_json::Value> = path
                .paths
                .iter()
                .map(|entry| {
                    serde_json::json!({
                        "site": entry.site,
                        "label": entry.label,
                    })
                })
                .collect();
            serde_json::json!({
                "function_id": path.function_id,
                "source_id": path.source_id,
                "line": path.line,
                "column": path.column,
                "kind": path.kind,
                "truncated": path.truncated,
                "paths": entries,
            })
        })
        .collect();
    let local_values: Vec<serde_json::Value> = locals
        .iter()
        .map(|local| {
            serde_json::json!({
                "function_id": local.function_id,
                "name": local.name,
                "frame_offset": local.frame_offset,
                "php_type": local.php_type,
                "kind": local.kind,
                "storage": local.storage,
                "state_offset": local.state_offset,
                "param": local.param,
                "by_ref": local.by_ref,
                "variadic": local.variadic,
                "sensitive": local.sensitive,
            })
        })
        .collect();
    let class_values: Vec<serde_json::Value> = classes
        .iter()
        .map(|class| {
            let properties: Vec<serde_json::Value> = class
                .properties
                .iter()
                .map(|property| {
                    serde_json::json!({
                        "name": property.name,
                        "offset": property.offset,
                        "php_type": property.php_type,
                        "visibility": property.visibility,
                        "by_ref": property.by_ref,
                        "declared": property.declared,
                    })
                })
                .collect();
            let debug_info_entries: Vec<serde_json::Value> = class
                .debug_info
                .entries
                .iter()
                .map(|entry| {
                    let mut value = serde_json::json!({ "key": entry.key });
                    if let Some(property) = &entry.property {
                        value["property"] = serde_json::json!(property);
                    }
                    if let Some(text) = &entry.value {
                        value["value"] = serde_json::json!(text);
                    }
                    if let Some(php_type) = &entry.php_type {
                        value["php_type"] = serde_json::json!(php_type);
                    }
                    value
                })
                .collect();
            let mut value = serde_json::json!({
                "id": class.id,
                "name": class.name,
                "parent": class.parent,
                "abstract": class.is_abstract,
                "dynamic_offset": class.dynamic_offset,
                "debug_info": {
                    "kind": class.debug_info.kind,
                    "entries": debug_info_entries,
                },
                "properties": properties,
            });
            if !class.property_hooks.is_empty() {
                let hooks: serde_json::Map<String, serde_json::Value> = class
                    .property_hooks
                    .iter()
                    .map(|(name, hook)| {
                        (
                            name.clone(),
                            serde_json::json!({
                                "get": hook.get,
                                "set": hook.set,
                                "get_by_ref": hook.get_by_ref,
                                "virtual": hook.is_virtual,
                            }),
                        )
                    })
                    .collect();
                value["property_hooks"] = serde_json::Value::Object(hooks);
            }
            value
        })
        .collect();
    let document = serde_json::json!({
        "format": "elephc-debug-metadata",
        "version": 1,
        "source": source_path.display().to_string(),
        "source_sha256": sources.get(&0).and_then(|source| source.sha256.as_ref()),
        "sources": source_values,
        "functions": function_values,
        "sites": site_values,
        "branches": branch_values,
        "paths": path_values,
        "locals": local_values,
        "classes": class_values,
    });
    serde_json::to_string_pretty(&document)
        .map_err(|error| format!("failed to render debug metadata: {error}"))
}

/// Returns the source-line range covered by one function's sites.
fn function_lines(function_id: usize, sites: &[SiteMeta]) -> (Option<usize>, Option<usize>) {
    let mut start = None;
    let mut end = None;
    for site in sites.iter().filter(|site| site.function_id == Some(function_id)) {
        start = Some(start.map_or(site.line, |value: usize| value.min(site.line)));
        end = Some(end.map_or(site.line, |value: usize| value.max(site.line)));
    }
    (start, end)
}

/// Returns (name, PHP type, by_ref, variadic) for each parameter slot.
fn function_params(function_id: usize, locals: &[LocalMeta]) -> Vec<(String, String, bool, bool)> {
    let mut params: Vec<(usize, String, String, bool, bool)> = locals
        .iter()
        .filter(|local| local.function_id == function_id)
        .filter_map(|local| {
            let index = local.param?;
            Some((
                index,
                local.name.clone(),
                local.php_type.clone(),
                local.by_ref,
                local.variadic,
            ))
        })
        .collect();
    params.sort_by_key(|param| param.0);
    params
        .into_iter()
        .map(|(_, name, php_type, by_ref, variadic)| (name, php_type, by_ref, variadic))
        .collect()
}

/// Appends the metadata JSON to the assembly behind stable symbols.
pub(crate) fn inject_embedded(asm: &str, json: &str, target: Target) -> String {
    let section = match target.platform {
        Platform::MacOS => ".section __TEXT,__const",
        Platform::Linux => ".section .rodata",
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    };
    let metadata = target.extern_symbol("elephc_debug_metadata");
    let length = target.extern_symbol("elephc_debug_metadata_len");
    let escaped = crate::codegen_support::runtime::data::instanceof::escaped_bytes(json.as_bytes());
    let mut out = String::with_capacity(asm.len() + json.len() * 2 + 256);
    out.push_str(asm);
    if !asm.ends_with('\n') {
        out.push('\n');
    }
    let _ = writeln!(out, "{section}");
    let _ = writeln!(out, "    .p2align 3");
    let _ = writeln!(out, "    .globl {metadata}");
    let _ = writeln!(out, "{metadata}:");
    let _ = writeln!(out, "    .ascii \"{escaped}\"");
    let _ = writeln!(out, "    .byte 0");
    let _ = writeln!(out, "    .p2align 3");
    let _ = writeln!(out, "    .globl {length}");
    let _ = writeln!(out, "{length}:");
    let _ = writeln!(out, "    .quad {}", json.len());
    out
}

/// Writes the sidecar through the shared artifact-write policy.
pub(crate) fn write_sidecar(path: &Path, json: &str) -> Result<(), String> {
    crate::pipeline::write_artifact(path, json.as_bytes())
        .map_err(|error| format!("failed to write debug metadata '{}': {}", path.display(), error))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASM: &str = "\
    # @file id=0 path_hex=6d61696e2e706870
    # @file id=7 path_hex=6c69622e706870
    # @file id=8 path_hex=6c69622e706870
    # @fn name=main symbol=_main file=0 kind=main flags=main
_main:
    # @local name_hex=78 offset=16 type_hex=696e74 kind=php_local
    # @src file=0 line=2 col=1 site=5 op=echo_value
    # @branch source=0 line=2 col=1 kind=conditional then_site=3 else_site=4 edges=3,4
    # @branch source=0 line=6 col=1 kind=switch edges=7,8,9
    # @pathgroup source=0 line=2 col=1 kind=logical truncated=0
    # @path source=0 line=2 col=1 kind=logical site=3 label=#0=T@2:1
    # @path source=0 line=2 col=1 kind=logical site=4 label=#0=F@2:1
    # @pathgroup source=0 line=6 col=1 kind=switch truncated=0
    # @path source=0 line=6 col=1 kind=switch site=7 label=case0
    # @src file=7 line=9 col=3 site=6 end=9:8 op=call
    # @local name_hex=6974656d73 offset=32 type_hex=61727261793c6d697865643e kind=php_local storage=ref_cell state_offset=40 param=0 sensitive=1
    # @class id=3 name_hex=506f696e74 parent_hex= abstract=0 dynamic_offset= debuginfo=none
    # @prop class=3 name_hex=78 offset=8 type_hex=696e74 visibility=public by_ref=0 declared=1
    # @prop class=3 name_hex=79 offset=24 type_hex=696e74 visibility=public by_ref=0 declared=1
    # @class id=4 name_hex=426167 parent_hex= abstract=0 internal=0 dynamic_offset=24 debuginfo=projection
    # @prop class=4 name_hex=6964 offset=8 type_hex=696e74 visibility=public by_ref=0 declared=1 hooks=get+set get_by_ref=0 virtual=1
    # @debuginfo class=4 order=0 key_hex=6964656e746966696572 property_hex=6964
    # @debuginfo class=4 order=1 key_hex=76657273696f6e value_hex=737472696e672833292022312e3022 type_hex=737472696e67
    # @endfn name=main
";

    /// Verifies sources/functions/sites/locals are emitted from the codegen
    /// markers, including path alias deduplication and parameter extraction.
    #[test]
    fn builds_sources_functions_sites_and_locals() {
        let json = build(ASM, Path::new("main.php")).unwrap();
        assert!(json.contains("\"format\": \"elephc-debug-metadata\""), "{json}");
        assert!(json.contains("\"version\": 1"), "{json}");
        assert_eq!(json.matches("\"path\": \"lib.php\"").count(), 1, "{json}");
        assert!(json.contains("\"kind\": \"main\""), "{json}");
        assert!(json.contains("\"flags\": ["), "{json}");
        assert!(json.contains("\"main\""), "{json}");
        assert!(json.contains("\"name\": \"main\""), "{json}");
        assert!(json.contains("\"name\": \"items\""), "{json}");
        assert!(json.contains("\"frame_offset\": 32"), "{json}");
        assert!(json.contains("\"php_type\": \"array<mixed>\""), "{json}");
        assert!(json.contains("\"param\": 0"), "{json}");
        assert!(json.contains("\"storage\": \"ref_cell\""), "{json}");
        assert!(json.contains("\"name\": \"Point\""), "{json}");
        assert!(json.contains("\"offset\": 8"), "{json}");
        assert!(json.contains("\"offset\": 24"), "{json}");
        assert!(json.contains("\"declared\": true"), "{json}");
        assert!(json.contains("\"branches\": ["), "{json}");
        assert!(json.contains("\"property_hooks\": {"), "{json}");
        assert!(json.contains("\"get\": true"), "{json}");
        assert!(json.contains("\"virtual\": true"), "{json}");
        assert!(json.contains("\"then_site\": 3"), "{json}");
        assert!(json.contains("\"else_site\": 4"), "{json}");
        assert!(json.contains("\"kind\": \"switch\""), "{json}");
        assert!(json.contains("\"paths\": ["), "{json}");
        assert!(json.contains("\"label\": \"#0=T@2:1\""), "{json}");
        assert!(json.contains("\"label\": \"case0\""), "{json}");
        assert!(json.contains("\"edges\": ["), "{json}");
        assert!(json.contains("\"dynamic_offset\": 24"), "{json}");
        assert!(json.contains("\"kind\": \"projection\""), "{json}");
        assert!(json.contains("\"key\": \"identifier\""), "{json}");
        assert!(json.contains("\"property\": \"id\""), "{json}");
        assert!(json.contains("\"key\": \"version\""), "{json}");
        assert!(json.contains("\"value\": \"string(3) \\\"1.0\\\"\""), "{json}");
        assert!(json.contains("\"php_type\": \"string\""), "{json}");
        assert!(json.contains("\"state_offset\": 40"), "{json}");
        assert!(json.contains("\"source_id\": 7"), "{json}");
        assert!(json.contains("\"id\": 5"), "{json}");
        assert!(json.contains("\"line\": 9"), "{json}");
        assert!(json.contains("\"end_column\": 8"), "{json}");
    }

    /// Verifies the embedded JSON gets a target-aware section/symbol and the
    /// explicit length word.
    #[test]
    fn embeds_metadata_with_length() {
        let out = inject_embedded(ASM, "{\"x\":1}", Target::detect_host());
        assert!(out.contains("elephc_debug_metadata:"), "{out}");
        assert!(out.contains("elephc_debug_metadata_len:"), "{out}");
        assert!(out.contains(".quad 7"), "{out}");
        assert!(out.contains(".ascii \"{\\\"x\\\":1}\""), "{out}");
    }
}
