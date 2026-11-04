//! Purpose:
//! Builds the static namespace and class-file index used by the AOT autoload pass.
//! Reads PSR-4, PSR-0, classmap, eager-file, and exclusion rules from supported manifests.
//!
//! Called from:
//! - `crate::autoload::Registry::build()`
//!
//! Key details:
//! - Produces FQN-to-path mappings and eager source entries for compile-time inclusion.
//! - Production and development mapping sections are merged because compiled binaries have one source graph.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::errors::CompileWarning;
use crate::parser::ast::{Stmt, StmtKind};
use crate::span::Span;

/// Whether Composer's eager `autoload.files` list joins the closed world. ON by default;
/// `ELEPHC_COMPOSER_EAGER_FILES=off` leaves it out.
///
/// ON IS THE ONLY HONEST DEFAULT, and the reason is what the switch is FOR. Those files are
/// where a framework's bootstrap lives; with them out, the binary still answers every route
/// byte-identically and merely interprets those includes at runtime instead of running compiled
/// code — which is precisely the cost this project exists to remove. A route gate cannot tell
/// the two apart, so a half-compiled binary would quietly pass as the real thing and every
/// measurement taken from it would describe a program nobody ships. `scratchpad/evaldelta.sh`'s
/// eval-crossing census is the number that does see the difference.
///
/// OFF IS FOR ONE THING: getting a binary to measure something ELSE with, quickly, when the
/// eager set is what is slow or broken. Whoever turns it off has to say so in what they report.
///
/// THE GATE LIVES AT THE CALL SITE, not in `crate::autoload::composer_files`. That reader is
/// pure I/O with unit tests that pin its order and its path folding; gating inside it would
/// make those tests exercise this switch instead. `push_eager_file` is the single point where a
/// path joins the eager set, so one guard here covers every manifest source that ever feeds it.
fn composer_eager_files_enabled() -> bool {
    !matches!(
        std::env::var("ELEPHC_COMPOSER_EAGER_FILES").as_deref(),
        Ok("off") | Ok("0")
    )
}

/// Compiled view of every autoload section found in the project.
#[derive(Default)]
pub struct AutoloadIndex {
    fqn_to_path: HashMap<String, PathBuf>,
    files_to_include: Vec<PathBuf>,
    /// Classmap files the scan could not read; each one hides every class it declares.
    warnings: Vec<CompileWarning>,
}

impl AutoloadIndex {
    /// Builds the index from the nearest supported root manifest and nested dependency manifests.
    /// Returns an empty index when no supported manifest exists at or above the entry directory.
    pub fn from_project_root(project_root: &Path) -> Self {
        Self::from_project_root_excluding(project_root, &HashSet::new())
    }

    /// Like [`Self::from_project_root`], but skips every nested dependency manifest whose
    /// package `name` is in `excluded` (an `elephc test` build leaves the installed PHPUnit out
    /// this way; an ordinary build passes an empty set).
    pub fn from_project_root_excluding(project_root: &Path, excluded: &HashSet<String>) -> Self {
        let Some(project_root) = nearest_autoload_manifest_root(project_root) else {
            return Self {
                fqn_to_path: HashMap::new(),
                files_to_include: Vec::new(),
                warnings: Vec::new(),
            };
        };
        let mut builder = IndexBuilder::default();
        // Composer's GENERATED list comes first, because it is the complete, ordered answer and
        // it already contains the root package's own `files`. The root manifest is still read
        // afterwards, so a project whose autoloader was never generated keeps working; entries
        // already claimed here are deduplicated away by `read_files`.
        if composer_eager_files_enabled() {
            for path in super::composer_files::generated_eager_files(&project_root) {
                builder.push_eager_file(path);
            }
        }
        for manifest in autoload_manifest_paths(&project_root) {
            builder.load_manifest(&manifest, true);
        }
        for manifest in nested_autoload_manifest_paths(&project_root) {
            if !excluded.is_empty()
                && manifest_package_name(&manifest).is_some_and(|name| excluded.contains(&name))
            {
                continue;
            }
            builder.load_manifest(&manifest, false);
        }
        builder.load_composer_runtime_classes(&project_root);
        AutoloadIndex {
            fqn_to_path: builder.fqn_to_path,
            files_to_include: builder.files_to_include,
            warnings: builder.warnings,
        }
    }

    /// Diagnostics collected while building the index, one per unreadable classmap file.
    pub fn warnings(&self) -> &[CompileWarning] {
        &self.warnings
    }

    /// Look up the file path for a given fully-qualified class name.
    pub fn lookup(&self, fqn: &str) -> Option<&Path> {
        let key = fqn.trim_start_matches('\\');
        self.fqn_to_path.get(key).map(PathBuf::as_path)
    }

    /// Every indexed class under the root manifest's own `autoload` PSR-4 directories, sorted.
    ///
    /// Its `src/`, then: neither its dependencies nor `autoload-dev`, whose test classes bootstrap
    /// by requiring `vendor/autoload.php` and belong to another program.
    pub fn project_classes(&self, project_root: &Path) -> Vec<String> {
        let manifest = std::fs::read_to_string(project_root.join("composer.json")).unwrap_or_default();
        let manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap_or_default();
        let directories: Vec<PathBuf> = manifest["autoload"]["psr-4"]
            .as_object()
            .into_iter()
            .flat_map(|prefixes| prefixes.values())
            .flat_map(|value| match value {
                serde_json::Value::Array(paths) => paths.clone(),
                other => vec![other.clone()],
            })
            .filter_map(|value| value.as_str().map(|path| project_root.join(path)))
            .map(|path| path.canonicalize().unwrap_or(path))
            .collect();
        let mut classes: Vec<String> = self
            .fqn_to_path
            .iter()
            .filter(|(_, path)| {
                let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
                directories.iter().any(|directory| path.starts_with(directory))
            })
            .map(|(name, _)| name.clone())
            .collect();
        classes.sort();
        classes
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

/// Reads the `name` of the package declared by the JSON manifest at `path`, if any.
fn manifest_package_name(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let json = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    json.get("name")?.as_str().map(str::to_owned)
}

/// Finds the nearest ancestor containing a structurally recognized autoload manifest.
///
/// THE WALK MUST START FROM AN ABSOLUTE DIRECTORY, and not doing so made the compiler's answer
/// depend on how the entry file's path was SPELLED. `Path::ancestors` is pure lexical string work:
/// for the relative `public` it yields `["public", ""]` and stops, so the walk never reaches the
/// project root sitting one real directory above, finds no `composer.json`, and builds an EMPTY
/// autoload index. For `examples/symfony-app/public` — the same directory, named from one level
/// up — it yields four ancestors, finds the manifest, and indexes everything.
///
/// Measured on a three-file project outside `examples/`, one `composer.json` with a PSR-4 `App\`
/// mapping, one `src/Thing.php`, one `web/entry.php` doing `new Thing()`. Compiled from INSIDE the
/// project as `web/entry.php` it failed with `error[5:6]: Undefined class: App\Thing`; compiled
/// from the parent as `cwdproj/web/entry.php`, or from inside with the absolute path, it printed
/// `thing;done`. Same file, same bytes, three different spellings, two different answers.
///
/// On the Symfony fixture the same fault was worth an order of magnitude: `--web` from the
/// worktree root produced a 108,317,352-byte binary with 631 class-name symbols, and from inside
/// `examples/symfony-app` an 11,023,640-byte one with 122 — a different program, silently, with a
/// clean exit code.
///
/// PHP HAS NO SUCH BEHAVIOUR: it resolves `__DIR__` and `dirname(__DIR__)` from the script's real
/// path, so where the shell happens to stand is invisible to it. Absolutising here restores that.
/// `canonicalize` is preferred because it also resolves symlinks and `..`; when it fails — the
/// directory does not exist, or permissions deny it — joining the process's current directory is
/// the honest fallback and is still strictly better than walking a bare relative path.
pub(crate) fn nearest_autoload_manifest_root(entry_dir: &Path) -> Option<PathBuf> {
    let absolute_entry_dir = absolute_entry_dir(entry_dir);
    absolute_entry_dir
        .ancestors()
        .find(|candidate| !autoload_manifest_paths(candidate).is_empty())
        .map(Path::to_path_buf)
}

/// Returns `entry_dir` as an absolute path, so a lexical ancestor walk sees the real directories.
fn absolute_entry_dir(entry_dir: &Path) -> PathBuf {
    if let Ok(canonical) = entry_dir.canonicalize() {
        return canonical;
    }
    if entry_dir.is_absolute() {
        return entry_dir.to_path_buf();
    }
    // An empty entry directory is what `Path::new("index.php").parent()` yields, and joining it to
    // the current directory is exactly the "." the caller means by it.
    std::env::current_dir().map_or_else(|_| entry_dir.to_path_buf(), |cwd| cwd.join(entry_dir))
}

/// Returns direct JSON manifests containing a supported autoload section.
fn autoload_manifest_paths(dir: &Path) -> Vec<PathBuf> {
    let mut manifests = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path_has_json_extension(path))
        .filter(|path| manifest_has_autoload_section(path))
        .collect::<Vec<_>>();
    manifests.sort();
    manifests
}

/// Recursively discovers structurally recognized manifests below the project root.
fn nested_autoload_manifest_paths(root: &Path) -> Vec<PathBuf> {
    let mut pending = VecDeque::from([root.to_path_buf()]);
    let mut visited = HashSet::new();
    let mut manifests = Vec::new();
    while let Some(dir) = pending.pop_front() {
        let canonical = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        if !visited.insert(canonical) {
            continue;
        }
        let mut entries = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                pending.push_back(path);
            } else if file_type.is_file()
                && path_has_json_extension(&path)
                && manifest_has_autoload_section(&path)
                && path.parent() != Some(root)
            {
                manifests.push(path);
            }
        }
    }
    manifests
}

/// Returns whether a path uses the JSON extension, case-insensitively.
fn path_has_json_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

/// Returns whether a JSON document exposes at least one supported autoload section.
fn manifest_has_autoload_section(path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    if !content.contains("\"autoload\"") && !content.contains("\"autoload-dev\"") {
        return false;
    }
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };
    ["autoload", "autoload-dev"]
        .into_iter()
        .any(|key| json.get(key).is_some_and(serde_json::Value::is_object))
}

#[derive(Default)]
/// Accumulates static autoload index entries while reading supported manifests.
struct IndexBuilder {
    fqn_to_path: HashMap<String, PathBuf>,
    files_to_include: Vec<PathBuf>,
    warnings: Vec<CompileWarning>,
}

impl IndexBuilder {
    /// Loads one supported manifest, optionally collecting its eager source entries.
    ///
    /// Nested manifests always contribute mappings. Only the root manifest contributes eager
    /// entries directly; nested eager entries remain owned by the loader source the program
    /// actually includes, avoiding duplicate execution and unrelated graph expansion.
    ///
    /// DUPLICATE EXECUTION is no longer the binding half of that reason: a source the compiler
    /// included is now flagged in the native include table, and an `include`/`require` whose
    /// value is discarded skips it instead of running the body twice (`emit_prime` in
    /// `src/codegen/source_units/bridge.rs`, `compiler_included_file` in Magician).
    ///
    /// GRAPH EXPANSION still is, and measurably so. The eager files themselves are the opposite
    /// of unrelated — Composer requires every one of them on every single run — but the CLASS
    /// GRAPH they reach is not. Splicing Composer's own ordered list
    /// (`vendor/composer/autoload_files.php`) into the Symfony fixture pulled in
    /// `polyfill-intl-grapheme/Grapheme.php`, `polyfill-php85/Php85.php` and
    /// `var-dumper/Server/Connection.php`, and the build then failed on two standard-PHP gaps
    /// those files depend on: the missing `PCRE_VERSION` constant, and `stream_socket_client()`'s
    /// by-reference `$error_code`/`$error_message` out-parameters (its contract declares one
    /// parameter and its lowering rejects more than one argument). Both have to land before the
    /// eager files can join the closed world.
    fn load_manifest(&mut self, manifest_path: &Path, include_files: bool) {
        let Some(dir) = manifest_path.parent() else {
            return;
        };
        let Ok(content) = std::fs::read_to_string(manifest_path) else {
            return;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
            return;
        };
        // Read both the production and dev autoload sections; the AOT
        // model has no production/test split, so they merge.
        for section_key in ["autoload", "autoload-dev"] {
            if let Some(section) = json.get(section_key) {
                self.load_section(dir, section, include_files);
            }
        }
    }

    /// Indexes the classes Composer GENERATES into `vendor/composer/` (`Composer\InstalledVersions`).
    ///
    /// They belong to no package manifest: only the generated `autoload_classmap.php` names them,
    /// so without this a call such as `InstalledVersions::isInstalled()` named a class the closed
    /// world never saw and every such call fell to the runtime autoloader. Only entries whose
    /// file lives under `vendor/composer/` are taken, so the rest of the generated map (which the
    /// package manifests already describe) does not widen the world.
    fn load_composer_runtime_classes(&mut self, project_root: &Path) {
        let vendor = project_root.join("vendor");
        // Bytes, not UTF-8: php source is a byte string, and Composer writes every class name it
        // scanned as is. `examples/symfony-app`'s generated map has a `'\xa9'` entry (a Latin-1 byte), and
        // `read_to_string` refused the whole file over it, so `InstalledVersions` vanished from
        // the index and the compiled console died including it at run time.
        let Ok(bytes) = std::fs::read(vendor.join("composer").join("autoload_classmap.php")) else {
            return;
        };
        let content = String::from_utf8_lossy(&bytes);
        for line in content.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix('\'') else { continue };
            let Some((class, path)) = rest.split_once("' => $vendorDir . '/composer/") else {
                continue;
            };
            let Some(path) = path.strip_suffix("',") else { continue };
            let class = class.replace("\\\\", "\\");
            self.fqn_to_path
                .entry(class)
                .or_insert_with(|| vendor.join("composer").join(path));
        }
    }

    /// Parse one autoload section (psr-4, psr-0, classmap, files) and update the index.
    fn load_section(
        &mut self,
        base_dir: &Path,
        section: &serde_json::Value,
        include_files: bool,
    ) {
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
        if include_files {
            if let Some(files) = section.get("files").and_then(|f| f.as_array()) {
                self.read_files(base_dir, files);
            }
        }
    }

    /// Shared driver for `psr-4` and `psr-0`. Sorts prefixes by length
    /// descending so longer prefixes claim FQNs before shorter ones (PHP
    /// longest-prefix-wins rule).
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
            scan_classmap_path(&path, &mut self.fqn_to_path, excludes, &mut self.warnings);
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
                self.push_eager_file(path);
            }
        }
    }

    /// Registers one eager source, keeping the FIRST claim on a path so order is stable.
    fn push_eager_file(&mut self, path: PathBuf) {
        let canonical = path.canonicalize().unwrap_or(path);
        if !self.files_to_include.contains(&canonical) {
            self.files_to_include.push(canonical);
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
        } else if crate::source::is_discoverable_source_path(&path) {
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
                *last = crate::source::discoverable_source_stem(last);
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
        } else if crate::source::is_discoverable_source_path(&path) {
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
                *last = crate::source::discoverable_source_stem(last);
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

/// Builds the warning reported for one classmap file the index could not read.
fn classmap_scan_warning(path: &Path, reason: &str) -> CompileWarning {
    // The diagnostic is about a file outside the entry program, so it has no position in it;
    // the message carries the path instead.
    CompileWarning::new(
        Span::dummy(),
        &format!(
            "autoload classmap: '{}' could not be indexed ({}); every class it declares stays \
             undefined and is never compiled",
            path.display(),
            reason
        ),
    )
}

/// Recursively scan a classmap path, descending into directories and
/// skipping excluded paths, then index all discovered PHP files.
fn scan_classmap_path(
    path: &Path,
    index: &mut HashMap<String, PathBuf>,
    excludes: &[String],
    warnings: &mut Vec<CompileWarning>,
) {
    if is_excluded(path, excludes) {
        return;
    }
    if path.is_file() {
        scan_classmap_file(path, index, warnings);
    } else if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            scan_classmap_path(&entry.path(), index, excludes, warnings);
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
/// inside `tests/`, matching the manifest format's directory-shorthand semantic.
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
    use super::{glob_match, AutoloadIndex};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static MANIFEST_TEST_ID: AtomicUsize = AtomicUsize::new(0);

    /// Creates one isolated directory for structural manifest discovery tests.
    fn manifest_test_dir() -> std::path::PathBuf {
        let id = MANIFEST_TEST_ID.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "elephc_autoload_manifest_{}_{}",
            std::process::id(),
            id
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Verifies that manifest contents, rather than a tool-specific filename, select the adapter.
    #[test]
    fn structural_manifest_discovery_accepts_arbitrary_json_name() {
        let dir = manifest_test_dir();
        let entry_dir = dir.join("public");
        let source_dir = dir.join("src");
        std::fs::create_dir_all(&entry_dir).unwrap();
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(
            dir.join("module-layout.json"),
            r#"{"autoload":{"psr-4":{"Demo\\":"src/"}}}"#,
        )
        .unwrap();
        std::fs::write(
            source_dir.join("Thing.php"),
            "<?php namespace Demo; class Thing {}",
        )
        .unwrap();

        let index = AutoloadIndex::from_project_root(&entry_dir);
        assert_eq!(
            index.lookup("Demo\\Thing"),
            Some(source_dir.join("Thing.php").canonicalize().unwrap().as_path())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Verifies the index is the same whether the entry directory is named relatively or absolutely.
    ///
    /// THE WALK IS LEXICAL, so before this was fixed the answer depended on how the path was
    /// SPELLED rather than on where the file is. `Path::ancestors` on the relative `public` yields
    /// `["public", ""]` and stops, so the project root one real directory above was never reached
    /// and the index came back EMPTY; the same directory named from one level up yields enough
    /// ancestors to find the manifest.
    ///
    /// Measured end to end before the fix, on a three-file project: compiled from inside as
    /// `web/entry.php` it failed `Undefined class: App\Thing`, and from the parent as
    /// `cwdproj/web/entry.php` it printed `thing;done`. On the Symfony fixture it was the
    /// difference between a 108,317,352-byte binary with 631 class-name symbols and an
    /// 11,023,640-byte one with 122 — silently, with a clean exit code. PHP resolves `__DIR__`
    /// from the script's real path, so the shell's position is invisible to it.
    ///
    /// The test changes the process directory, which is global, so it restores it before
    /// asserting and never holds it across an assertion that can panic.
    #[test]
    fn a_relative_entry_directory_finds_the_same_root_as_an_absolute_one() {
        let dir = manifest_test_dir().canonicalize().unwrap();
        let entry_dir = dir.join("public");
        let source_dir = dir.join("src");
        std::fs::create_dir_all(&entry_dir).unwrap();
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(
            dir.join("composer.json"),
            r#"{"autoload":{"psr-4":{"Demo\\":"src/"}}}"#,
        )
        .unwrap();
        std::fs::write(
            source_dir.join("Thing.php"),
            "<?php namespace Demo; class Thing {}",
        )
        .unwrap();

        let absolute = AutoloadIndex::from_project_root(&entry_dir);

        // The relative spelling `public`, resolved from inside the project, is the one that used
        // to come back empty.
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let relative = AutoloadIndex::from_project_root(std::path::Path::new("public"));
        std::env::set_current_dir(previous).unwrap();

        assert_eq!(
            relative.lookup("Demo\\Thing"),
            absolute.lookup("Demo\\Thing"),
            "the entry directory's spelling must not change what the autoload index finds"
        );
        assert!(
            relative.lookup("Demo\\Thing").is_some(),
            "the relative spelling must still find the project root"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Verifies a classmap file the scan cannot parse is reported instead of silently dropped.
    ///
    /// The file contributes no classes either way, so without a diagnostic every class it
    /// declares simply stops existing and the only message anyone sees points at a later use
    /// site. The warning has to name the file and say what went wrong there.
    /// Composer's generated classmap is php source, so a byte string: a class name it scanned
    /// with a Latin-1 byte (`examples/symfony-app` has `'\xa9'`) makes it invalid UTF-8. The
    /// generated runtime classes beside it must still be indexed.
    #[test]
    fn a_generated_classmap_with_a_non_utf8_byte_still_indexes_installed_versions() {
        let dir = manifest_test_dir();
        let composer_dir = dir.join("vendor/composer");
        std::fs::create_dir_all(&composer_dir).unwrap();
        std::fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#)
            .unwrap();
        std::fs::write(
            composer_dir.join("InstalledVersions.php"),
            "<?php\nnamespace Composer;\nclass InstalledVersions {}\n",
        )
        .unwrap();
        let mut classmap = b"<?php\n\n$vendorDir = dirname(__DIR__);\n\nreturn array(\n    'Composer\\\\InstalledVersions' => $vendorDir . '/composer/InstalledVersions.php',\n    '".to_vec();
        classmap.push(0xa9);
        classmap.extend_from_slice(b"' => $vendorDir . '/symfony/polyfill/Resources/stubs/Odd.php',\n);\n");
        std::fs::write(composer_dir.join("autoload_classmap.php"), classmap).unwrap();

        let index = AutoloadIndex::from_project_root(&dir);

        let found = index
            .lookup("Composer\\InstalledVersions")
            .and_then(|path| path.canonicalize().ok());
        assert_eq!(found, composer_dir.join("InstalledVersions.php").canonicalize().ok());
    }

    #[test]
    fn unparsable_classmap_file_is_reported_not_swallowed() {
        let dir = manifest_test_dir();
        let source_dir = dir.join("src");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(
            dir.join("composer.json"),
            r#"{"autoload":{"classmap":["src/"]}}"#,
        )
        .unwrap();
        std::fs::write(
            source_dir.join("Broken.php"),
            "<?php\nclass Broken { public function x() { return @@@ ; } }\n",
        )
        .unwrap();
        std::fs::write(source_dir.join("Fine.php"), "<?php\nclass Fine {}\n").unwrap();

        let index = AutoloadIndex::from_project_root(&dir);

        assert!(index.lookup("Broken").is_none());
        assert!(index.lookup("Fine").is_some());
        let messages = index
            .warnings()
            .iter()
            .map(|warning| warning.message.clone())
            .collect::<Vec<_>>();
        assert!(
            messages.iter().any(|message| {
                message.contains("autoload classmap")
                    && message.contains("Broken.php")
                    && message.contains("never compiled")
            }),
            "expected a diagnostic naming the unreadable classmap file, got {messages:?}"
        );
        assert!(
            !messages.iter().any(|message| message.contains("Fine.php")),
            "a file that parsed must not be reported, got {messages:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A classmap declaration under a version guard is mapped, as Composer's scanner maps it
    /// (symfony/polyfill-php86's `SortDirection` stub, which Laravel's `BelongsToMany` uses).
    #[test]
    fn guarded_classmap_declarations_are_indexed() {
        let dir = manifest_test_dir();
        let source_dir = dir.join("stubs");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(
            dir.join("composer.json"),
            r#"{"autoload":{"classmap":["stubs/"]}}"#,
        )
        .unwrap();
        std::fs::write(
            source_dir.join("SortDirection.php"),
            "<?php\nif (\\PHP_VERSION_ID < 80600) {\n    enum SortDirection { case Ascending; }\n}\n",
        )
        .unwrap();
        std::fs::write(
            source_dir.join("Guarded.php"),
            "<?php\nnamespace Lib;\nif (PHP_OS === 'x') {\n} elseif (true) {\n    try {\n        final class Guarded {}\n    } finally {}\n} else {\n    interface Other {}\n}\n",
        )
        .unwrap();

        let index = AutoloadIndex::from_project_root(&dir);

        assert!(index.lookup("SortDirection").is_some());
        assert!(index.lookup("Lib\\Guarded").is_some());
        assert!(index.lookup("Lib\\Other").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

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

/// Parses a PHP/LFC source file and indexes all class/interface/trait/enum declarations found.
///
/// A file this pass cannot read, lex, or parse contributes no classes, and every class it
/// declares then looks undefined to the rest of the compile — a demand for one of them resolves
/// to nothing and the class is simply never compiled. That is invisible at the use site, so each
/// failure is reported as a warning naming the file and the reason instead of being swallowed.
fn scan_classmap_file(
    path: &Path,
    index: &mut HashMap<String, PathBuf>,
    warnings: &mut Vec<CompileWarning>,
) {
    if !crate::source::is_discoverable_source_path(path) {
        return;
    }
    let content = match crate::source::read_physical_source(path) {
        Ok(content) => content,
        Err(error) => {
            warnings.push(classmap_scan_warning(path, &error.to_string()));
            return;
        }
    };
    let mode = crate::source::SourceMode::from_path(path);
    let tokens = match crate::lexer::tokenize_with_mode(&content, mode) {
        Ok(tokens) => tokens,
        Err(error) => {
            warnings.push(classmap_scan_warning(path, &error.message));
            return;
        }
    };
    let ast = match crate::parser::parse_with_mode(&tokens, mode) {
        Ok(ast) => ast,
        Err(error) => {
            warnings.push(classmap_scan_warning(path, &error.message));
            return;
        }
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
        // Composer's classmap scanner finds a declaration wherever it sits, and polyfill stubs
        // declare theirs under a version guard (symfony/polyfill-php86's
        // `if (\PHP_VERSION_ID < 80600 ...) { enum SortDirection ... }`): a guarded class left
        // out of the index is never loaded, though Composer maps it.
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let branches = std::iter::once(then_body)
                .chain(elseif_clauses.iter().map(|(_, body)| body))
                .chain(else_body.iter());
            for body in branches {
                for inner in body {
                    extract_classmap_decls(inner, current_namespace, file_path, index);
                }
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            for inner in then_body.iter().chain(else_body.iter().flatten()) {
                extract_classmap_decls(inner, current_namespace, file_path, index);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let bodies = std::iter::once(try_body)
                .chain(catches.iter().map(|catch| &catch.body))
                .chain(finally_body.iter());
            for body in bodies {
                for inner in body {
                    extract_classmap_decls(inner, current_namespace, file_path, index);
                }
            }
        }
        StmtKind::Synthetic(body) => {
            for inner in body {
                extract_classmap_decls(inner, current_namespace, file_path, index);
            }
        }
        _ => {}
    }
}
