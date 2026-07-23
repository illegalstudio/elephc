//! Purpose:
//! Discovers native project manifests from compilation sources or native command working directories.
//!
//! Called from:
//! - Native command orchestration and compilation-time artifact resolution.
//!
//! Key details:
//! - Explicit manifest paths disable walking; discovered paths are absolute and canonicalized.

use std::fs;
use std::path::{Component, Path, PathBuf};

use super::error::{NativeError, NativeErrorKind};

/// Absolute paths that define one native dependency project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectPaths {
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub lock: PathBuf,
}

/// Discovers a native command project from the invocation working directory.
pub fn discover_for_native(
    cwd: &Path,
    explicit_manifest: Option<&Path>,
    create_when_missing: bool,
) -> Result<Option<ProjectPaths>, NativeError> {
    if let Some(manifest) = explicit_manifest {
        return Ok(Some(from_explicit(cwd, manifest)?));
    }
    let start = canonical_directory(cwd)?;
    if let Some(project) = walk_ancestors(&start)? {
        return Ok(Some(project));
    }
    if create_when_missing {
        return Ok(Some(from_root(start)));
    }
    Ok(None)
}

/// Discovers the nearest project above a PHP source's parent directory.
pub fn discover_for_source(source: &Path) -> Result<Option<ProjectPaths>, NativeError> {
    let absolute = if source.is_absolute() {
        lexical_absolute(source, Path::new("/"))?
    } else {
        let cwd = std::env::current_dir().map_err(|error| NativeError::io("read current directory", Path::new("."), error))?;
        lexical_absolute(source, &cwd)?
    };
    let parent = absolute.parent().ok_or_else(|| NativeError::new(NativeErrorKind::Project, "source path has no parent directory"))?;
    let start = canonical_directory(parent)?;
    walk_ancestors(&start)
}

/// Resolves and validates an explicitly selected `elephc.toml` path.
fn from_explicit(cwd: &Path, manifest: &Path) -> Result<ProjectPaths, NativeError> {
    if manifest.file_name().and_then(|name| name.to_str()) != Some("elephc.toml") {
        return Err(NativeError::new(NativeErrorKind::Project, "--manifest-path must name an elephc.toml file").with_path(manifest));
    }
    let absolute = lexical_absolute(manifest, cwd)?;
    if fs::symlink_metadata(&absolute).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(NativeError::new(NativeErrorKind::Project, "explicit project manifest must not be a symlink").with_path(absolute));
    }
    let root = absolute.parent().ok_or_else(|| NativeError::new(NativeErrorKind::Project, "manifest path has no parent"))?;
    let root = canonical_directory(root)?;
    Ok(from_root(root))
}

/// Canonicalizes a directory and reports symlink or access failures explicitly.
fn canonical_directory(path: &Path) -> Result<PathBuf, NativeError> {
    fs::canonicalize(path).map_err(|error| NativeError::new(
        NativeErrorKind::Project,
        format!("cannot canonicalize project search directory: {error}"),
    ).with_path(path))
}

/// Walks nearest-first for a manifest without reading its contents.
fn walk_ancestors(start: &Path) -> Result<Option<ProjectPaths>, NativeError> {
    for root in start.ancestors() {
        let manifest = root.join("elephc.toml");
        let Ok(metadata) = fs::symlink_metadata(&manifest) else { continue; };
        if metadata.file_type().is_symlink() {
            return Err(NativeError::new(NativeErrorKind::Project, "project manifest must not be a symlink").with_path(manifest));
        }
        if metadata.is_file() {
            return Ok(Some(from_root(root.to_path_buf())));
        }
    }
    Ok(None)
}

/// Constructs project-owned manifest and lock paths for an absolute root.
fn from_root(root: PathBuf) -> ProjectPaths {
    ProjectPaths { manifest: root.join("elephc.toml"), lock: root.join("elephc.lock"), root }
}

/// Lexically absolutizes a path while preventing unresolved parent components above root.
pub(crate) fn lexical_absolute(path: &Path, cwd: &Path) -> Result<PathBuf, NativeError> {
    let joined = if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) };
    let mut result = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return Err(NativeError::new(NativeErrorKind::Project, "path escapes its filesystem root").with_path(path));
                }
            }
            Component::Normal(part) => result.push(part),
        }
    }
    if !result.is_absolute() {
        return Err(NativeError::new(NativeErrorKind::Project, "could not make path absolute").with_path(path));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Creates an isolated directory for project discovery tests.
    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("elephc-native-{label}-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    /// Verifies discovery selects the nearest manifest above native cwd and source parent.
    #[test]
    fn discovery_is_nearest_ancestor() {
        let root = temp_dir("project");
        fs::write(root.join("elephc.toml"), "[native]\nschema = 1\n").unwrap();
        let nested = root.join("a/b");
        fs::create_dir_all(&nested).unwrap();
        let native = discover_for_native(&nested, None, false).unwrap().unwrap();
        let source = discover_for_source(&nested.join("main.php")).unwrap().unwrap();
        assert_eq!(native.root, fs::canonicalize(&root).unwrap());
        assert_eq!(source, native);
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies add-style discovery creates a project at cwd only when no ancestor exists.
    #[test]
    fn discovery_can_select_new_project_root() {
        let root = temp_dir("new");
        let project = discover_for_native(&root, None, true).unwrap().unwrap();
        assert_eq!(project.root, fs::canonicalize(&root).unwrap());
        assert!(!project.manifest.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
