//! Purpose:
//! Builds the Composer autoload index used by the AOT autoload pass.
//! Reads PSR-4, PSR-0, classmap, files, and exclude rules from project/vendor composer.json files.
//!
//! Called from:
//! - `crate::autoload::Registry::build()`
//!
//! Key details:
//! - Produces FQN-to-path mappings and `autoload.files` entries for compile-time inclusion.
//! - `autoload` and `autoload-dev` are intentionally merged because compiled binaries have no Composer runtime mode.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::parser::ast::{Stmt, StmtKind};

/// Compiled view of every autoload section found in the project.
pub struct AutoloadIndex {
    fqn_to_path: HashMap<String, PathBuf>,
    files_to_include: Vec<PathBuf>,
}

impl AutoloadIndex {
    /// Build the index by reading `<project_root>/composer.json` and any
    /// `<project_root>/vendor/<vendor>/<pkg>/composer.json`. Empty index
    /// when no composer.json exists.
    pub fn from_project_root(project_root: &Path) -> Self {
        let mut builder = IndexBuilder::default();
        builder.load_composer(project_root);
        let vendor = project_root.join("vendor");
        if vendor.is_dir() {
            for vendor_entry in std::fs::read_dir(&vendor).into_iter().flatten().flatten() {
                let pkg_dir = vendor_entry.path();
                if !pkg_dir.is_dir() {
                    continue;
                }
                for sub in std::fs::read_dir(&pkg_dir).into_iter().flatten().flatten() {
                    let inner = sub.path();
                    if inner.is_dir() {
                        builder.load_composer(&inner);
                    }
                }
            }
        }
        AutoloadIndex {
            fqn_to_path: builder.fqn_to_path,
            files_to_include: builder.files_to_include,
        }
    }

    /// Look up the file path for a given fully-qualified class name.
    pub fn lookup(&self, fqn: &str) -> Option<&Path> {
        let key = fqn.trim_start_matches('\\');
        self.fqn_to_path.get(key).map(PathBuf::as_path)
    }

    /// True when the index has no PSR-4 mappings and no files entries.
    pub fn is_empty(&self) -> bool {
        self.fqn_to_path.is_empty() && self.files_to_include.is_empty()
    }

    /// Files listed under `autoload.files` / `autoload-dev.files`.
    pub fn files(&self) -> &[PathBuf] {
        &self.files_to_include
    }
}

#[derive(Default)]
/// Helper that accumulates autoload index entries while reading composer.json files.
struct IndexBuilder {
    fqn_to_path: HashMap<String, PathBuf>,
    files_to_include: Vec<PathBuf>,
}

impl IndexBuilder {
    /// Load composer.json from a directory and merge its autoload sections.
    fn load_composer(&mut self, dir: &Path) {
        let composer_path = dir.join("composer.json");
        let Ok(content) = std::fs::read_to_string(&composer_path) else {
            return;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
            return;
        };
        // Read both the production and dev autoload sections; the AOT
        // model has no production/test split, so they merge.
        for section_key in ["autoload", "autoload-dev"] {
            if let Some(section) = json.get(section_key) {
                self.load_section(dir, section);
            }
        }
    }

    /// Parse one autoload section (psr-4, psr-0, classmap, files) and update the index.
    fn load_section(&mut self, base_dir: &Path, section: &serde_json::Value) {
        let excludes = section
            .get("exclude-from-classmap")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| normalize_exclude_pattern(base_dir, s)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if let Some(psr4) = section.get("psr-4").and_then(|p| p.as_object()) {
            self.read_psr_namespaced(base_dir, psr4, walk_psr4);
        }
        if let Some(psr0) = section.get("psr-0").and_then(|p| p.as_object()) {
            self.read_psr_namespaced(base_dir, psr0, walk_psr0);
        }
        if let Some(classmap) = section.get("classmap").and_then(|c| c.as_array()) {
            self.read_classmap(base_dir, classmap, &excludes);
        }
        if let Some(files) = section.get("files").and_then(|f| f.as_array()) {
            self.read_files(base_dir, files);
        }
    }

    /// Shared driver for `psr-4` and `psr-0`. Sorts prefixes by length
    /// descending so longer prefixes claim FQNs before shorter ones (PHP
    /// composer's longest-prefix-wins rule).
    fn read_psr_namespaced(
        &mut self,
        base_dir: &Path,
        prefix_map: &serde_json::Map<String, serde_json::Value>,
        walker: fn(&Path, &str, &Path, &mut HashMap<String, PathBuf>),
    ) {
        let mut prefixes: Vec<&String> = prefix_map.keys().collect();
        prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
        for prefix in prefixes {
            for dir in extract_paths(&prefix_map[prefix]) {
                let root = base_dir.join(dir);
                walker(&root, prefix, &root, &mut self.fqn_to_path);
            }
        }
    }

    /// Scan a classmap entry and populate the FQN index.
    fn read_classmap(
        &mut self,
        base_dir: &Path,
        entries: &[serde_json::Value],
        excludes: &[String],
    ) {
        for entry in entries {
            let Some(path_str) = entry.as_str() else {
                continue;
            };
            let path = base_dir.join(path_str);
            scan_classmap_path(&path, &mut self.fqn_to_path, excludes);
        }
    }

    /// Read the `files` autoload entries and register files to always include.
    fn read_files(&mut self, base_dir: &Path, entries: &[serde_json::Value]) {
        for entry in entries {
            let Some(path_str) = entry.as_str() else {
                continue;
            };
            let path = base_dir.join(path_str);
            if path.is_file() {
                let canonical = path.canonicalize().unwrap_or(path);
                if !self.files_to_include.contains(&canonical) {
                    self.files_to_include.push(canonical);
                }
            }
        }
    }
}

/// Extract the path or paths from a JSON value (string or array of strings).
fn extract_paths(value: &serde_json::Value) -> Vec<&str> {
    match value {
        serde_json::Value::String(s) => vec![s.as_str()],
        serde_json::Value::Array(a) => a.iter().filter_map(|v| v.as_str()).collect(),
        _ => Vec::new(),
    }
}

// --- PSR-4 walker ---

/// Recursively walk a PSR-4 directory tree, mapping file paths to FQNs.
fn walk_psr4(dir: &Path, ns_prefix: &str, root: &Path, index: &mut HashMap<String, PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_psr4(&path, ns_prefix, root, index);
        } else if path.extension().is_some_and(|ext| ext == "php") {
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let mut parts: Vec<String> = rel
                .components()
                .filter_map(|c| match c {
                    std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect();
            if parts.is_empty() {
                continue;
            }
            if let Some(last) = parts.last_mut() {
                if let Some(stripped) = last.strip_suffix(".php") {
                    *last = stripped.to_string();
                }
            }
            let suffix = parts.join("\\");
            let prefix = ns_prefix.trim_matches('\\');
            let fqn = if prefix.is_empty() {
                suffix
            } else {
                format!("{}\\{}", prefix, suffix)
            };
            let canonical = path.canonicalize().unwrap_or(path);
            index.entry(fqn).or_insert(canonical);
        }
    }
}

// --- PSR-0 walker ---

/// PSR-0 walker. Unlike PSR-4 (where the prefix is stripped before path
/// resolution), PSR-0 treats the directory as containing the full
/// namespace tree — class `Vendor\Pkg\Sub\Item` under prefix
/// `Vendor\Pkg\` mapping to `lib/` lives at `lib/Vendor/Pkg/Sub/Item.php`.
/// The walk derives the FQN directly from the path joined with `\`.
///
/// PSR-0 also supports underscore-style class names: when the prefix has
/// no `\` (e.g. `Twig_` → `lib/`), file `lib/Twig/Loader/Filesystem.php`
/// becomes class `Twig_Loader_Filesystem`. This is signalled by the
/// prefix not containing a backslash.
fn walk_psr0(dir: &Path, ns_prefix: &str, root: &Path, index: &mut HashMap<String, PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_psr0(&path, ns_prefix, root, index);
        } else if path.extension().is_some_and(|ext| ext == "php") {
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let mut parts: Vec<String> = rel
                .components()
                .filter_map(|c| match c {
                    std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect();
            if parts.is_empty() {
                continue;
            }
            if let Some(last) = parts.last_mut() {
                if let Some(stripped) = last.strip_suffix(".php") {
                    *last = stripped.to_string();
                }
            }
            let prefix = ns_prefix.trim_matches('\\');
            let prefix_has_namespace = prefix.contains('\\');

            let fqn = if prefix_has_namespace {
                // Namespaced PSR-0: the directory tree mirrors the full
                // namespace path. Join components with `\`.
                parts.join("\\")
            } else {
                // Underscore-style PSR-0 (e.g. `Twig_` → `lib/`): every
                // path segment becomes part of the underscore-joined
                // class name.
                parts.join("_")
            };
            let canonical = path.canonicalize().unwrap_or(path);
            index.entry(fqn).or_insert(canonical);
        }
    }
}

// --- classmap scanner ---

/// Recursively scan a classmap path, descending into directories and
/// skipping excluded paths, then index all discovered PHP files.
fn scan_classmap_path(
    path: &Path,
    index: &mut HashMap<String, PathBuf>,
    excludes: &[String],
) {
    if is_excluded(path, excludes) {
        return;
    }
    if path.is_file() {
        scan_classmap_file(path, index);
    } else if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            scan_classmap_path(&entry.path(), index, excludes);
        }
    }
}

/// Match `path` against the configured `exclude-from-classmap` glob
/// patterns. Returns true when any pattern matches.
fn is_excluded(path: &Path, excludes: &[String]) -> bool {
    if excludes.is_empty() {
        return false;
    }
    let canonical = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    let canonical_str = canonical.to_string_lossy();
    for pattern in excludes {
        if glob_match(pattern, &canonical_str) {
            return true;
        }
    }
    false
}

/// Resolve a user-provided pattern to an absolute glob string.
///
/// Relative patterns are joined with `base_dir` and canonicalised. A
/// trailing `/` is rewritten as `/**` so `"tests/"` matches everything
/// inside `tests/`, mirroring composer's directory-shorthand semantic.
fn normalize_exclude_pattern(base_dir: &Path, raw: &str) -> String {
    let trimmed = raw.trim_start_matches("./");
    let with_dirstar = if trimmed.ends_with('/') {
        format!("{}**", trimmed)
    } else {
        trimmed.to_string()
    };
    if std::path::Path::new(&with_dirstar).is_absolute() {
        return with_dirstar;
    }
    let joined = base_dir.join(&with_dirstar);
    // Canonicalize the literal-prefix portion if possible, but keep glob
    // metacharacters intact afterwards. Splitting on the first wildcard
    // segment is the simplest way to do this without a full glob parser.
    let joined_str = joined.to_string_lossy().into_owned();
    if !joined_str.contains('*') && !joined_str.contains('?') {
        return joined
            .canonicalize()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or(joined_str);
    }
    let (literal_prefix, glob_tail) = split_at_first_wildcard(&joined_str);
    let canonical_prefix = std::path::Path::new(literal_prefix)
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| literal_prefix.to_string());
    if glob_tail.is_empty() {
        canonical_prefix
    } else {
        let sep = if canonical_prefix.ends_with('/') {
            ""
        } else {
            "/"
        };
        format!(
            "{}{}{}",
            canonical_prefix,
            sep,
            glob_tail.trim_start_matches('/')
        )
    }
}

/// Split a glob string at the first path segment that contains a
/// wildcard. Used to canonicalise the pure-literal prefix while leaving
/// the glob portion intact.
fn split_at_first_wildcard(input: &str) -> (&str, &str) {
    let mut last_slash = 0usize;
    for (idx, byte) in input.bytes().enumerate() {
        if byte == b'/' {
            last_slash = idx;
        } else if byte == b'*' || byte == b'?' {
            return input.split_at(last_slash);
        }
    }
    (input, "")
}

/// Glob match against a path. Supports:
///   `**` — match any sequence of characters, including `/`
///   `*`  — match any sequence of characters except `/`
///   `?`  — match a single character except `/`
///   any other character — literal
fn glob_match(pattern: &str, path: &str) -> bool {
    glob_match_bytes(pattern.as_bytes(), path.as_bytes())
}

/// Byte-level glob matcher called by `glob_match`. Handles `**`, `*`, `?`
/// meta-characters across path segments.
fn glob_match_bytes(p: &[u8], s: &[u8]) -> bool {
    let mut pi = 0;
    let mut si = 0;
    let mut backtrack: Option<(usize, usize)> = None;
    let mut star_double = false;
    while si < s.len() {
        if pi < p.len() && p[pi] == b'*' {
            // Detect `**` for cross-segment matching.
            star_double = pi + 1 < p.len() && p[pi + 1] == b'*';
            backtrack = Some((pi, si));
            if star_double {
                pi += 2;
                // Skip the optional trailing `/` that conventionally follows
                // `**` (e.g. `**/`).
                if pi < p.len() && p[pi] == b'/' {
                    pi += 1;
                }
            } else {
                pi += 1;
            }
            continue;
        }
        if pi < p.len() && (p[pi] == b'?' || p[pi] == s[si])
            && (p[pi] != b'?' || s[si] != b'/')
        {
            pi += 1;
            si += 1;
            continue;
        }
        if let Some((bp, bs)) = backtrack {
            // For `*`, expansion must not cross `/` characters.
            if !star_double && bs < s.len() && s[bs] == b'/' {
                return false;
            }
            backtrack = Some((bp, bs + 1));
            si = bs + 1;
            pi = if star_double { bp + 2 } else { bp + 1 };
            // Skip optional trailing `/` after `**` again.
            if star_double && pi < p.len() && p[pi] == b'/' {
                pi += 1;
            }
            continue;
        }
        return false;
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    /// Implements the `glob_literal` operation for this module.
    #[test]
    fn glob_literal() {
        assert!(glob_match("/a/b/c.php", "/a/b/c.php"));
        assert!(!glob_match("/a/b/c.php", "/a/b/d.php"));
    }

    /// Implements the `glob_star_within_segment` operation for this module.
    #[test]
    fn glob_star_within_segment() {
        assert!(glob_match("/a/*.php", "/a/foo.php"));
        assert!(!glob_match("/a/*.php", "/a/sub/foo.php"));
    }

    /// Implements the `glob_double_star_crosses_segments` operation for this module.
    #[test]
    fn glob_double_star_crosses_segments() {
        assert!(glob_match("/a/**/foo.php", "/a/foo.php"));
        assert!(glob_match("/a/**/foo.php", "/a/sub/foo.php"));
        assert!(glob_match("/a/**/foo.php", "/a/x/y/foo.php"));
    }

    /// Implements the `glob_directory_shorthand` operation for this module.
    #[test]
    fn glob_directory_shorthand() {
        assert!(glob_match("/a/tests/**", "/a/tests/foo.php"));
        assert!(glob_match("/a/tests/**", "/a/tests/sub/foo.php"));
        assert!(!glob_match("/a/tests/**", "/a/lib/foo.php"));
    }

    /// Implements the `glob_question_single_char` operation for this module.
    #[test]
    fn glob_question_single_char() {
        assert!(glob_match("/a/?.php", "/a/x.php"));
        assert!(!glob_match("/a/?.php", "/a/xx.php"));
        assert!(!glob_match("/a/?.php", "/a//.php"));
    }
}

/// Parse a PHP file and index all class/interface/trait/enum declarations found.
fn scan_classmap_file(path: &Path, index: &mut HashMap<String, PathBuf>) {
    if !path.extension().is_some_and(|ext| ext == "php") {
        return;
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(tokens) = crate::lexer::tokenize(&content) else {
        return;
    };
    let Ok(ast) = crate::parser::parse(&tokens) else {
        return;
    };
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut current_namespace: Option<String> = None;
    for stmt in &ast {
        extract_classmap_decls(stmt, &mut current_namespace, &canonical, index);
    }
}

/// Recursively extract classmap declarations from a statement, tracking current namespace context.
fn extract_classmap_decls(
    stmt: &Stmt,
    current_namespace: &mut Option<String>,
    file_path: &Path,
    index: &mut HashMap<String, PathBuf>,
) {
    match &stmt.kind {
        StmtKind::NamespaceDecl { name } => {
            *current_namespace = name.as_ref().map(|n| n.as_canonical());
        }
        StmtKind::NamespaceBlock { name, body } => {
            let saved = current_namespace.clone();
            *current_namespace = name.as_ref().map(|n| n.as_canonical());
            for inner in body {
                extract_classmap_decls(inner, current_namespace, file_path, index);
            }
            *current_namespace = saved;
        }
        StmtKind::ClassDecl { name, .. }
        | StmtKind::InterfaceDecl { name, .. }
        | StmtKind::TraitDecl { name, .. }
        | StmtKind::EnumDecl { name, .. } => {
            let trimmed = name.trim_start_matches('\\');
            let fqn = match current_namespace.as_deref() {
                Some(ns) if !ns.is_empty() => {
                    format!("{}\\{}", ns.trim_start_matches('\\'), trimmed)
                }
                _ => trimmed.to_string(),
            };
            index.entry(fqn).or_insert_with(|| file_path.to_path_buf());
        }
        _ => {}
    }
}
