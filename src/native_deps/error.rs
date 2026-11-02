//! Purpose:
//! Defines structured failures for native dependency parsing, project state, integrity, and tools.
//!
//! Called from:
//! - Every `crate::native_deps` module and top-level CLI integration.
//!
//! Key details:
//! - Errors retain a stable category plus uniform project and recovery-command context.

use std::fmt;
use std::path::PathBuf;

/// Stable category for a native-dependency failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeErrorKind {
    Usage,
    Project,
    Manifest,
    Lock,
    Catalog,
    Cache,
    Network,
    Integrity,
    Archive,
    Toolchain,
    Build,
    Io,
}

/// Structured native-dependency failure with an optional affected path.
#[derive(Debug)]
pub struct NativeError {
    pub kind: NativeErrorKind,
    pub message: String,
    pub path: Option<PathBuf>,
    pub project: Option<ProjectContext>,
    pub recovery: Option<String>,
}

/// Project discovery context printed alongside actionable recovery commands.
#[derive(Debug)]
pub enum ProjectContext {
    Found(PathBuf),
    Missing { searched_from: PathBuf },
}

impl NativeError {
    /// Constructs an error in `kind` with a user-facing message.
    pub fn new(kind: NativeErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            path: None,
            project: None,
            recovery: None,
        }
    }

    /// Attaches the path that caused this failure.
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Attaches the absolute root of the discovered project.
    pub fn with_project(mut self, root: impl Into<PathBuf>) -> Self {
        self.project = Some(ProjectContext::Found(root.into()));
        self
    }

    /// Records that discovery found no project above the supplied search directory.
    pub fn with_missing_project(mut self, searched_from: impl Into<PathBuf>) -> Self {
        self.project =
            Some(ProjectContext::Missing { searched_from: searched_from.into() });
        self
    }

    /// Attaches the preferred copy-paste recovery command.
    pub fn with_recovery(mut self, command: impl Into<String>) -> Self {
        self.recovery = Some(command.into());
        self
    }

    /// Attaches a fallback recovery command only when a more specific one is absent.
    pub fn with_default_recovery(mut self, command: impl Into<String>) -> Self {
        if self.recovery.is_none() {
            self.recovery = Some(command.into());
        }
        self
    }

    /// Wraps an I/O failure with the attempted action and path.
    pub fn io(action: &str, path: &std::path::Path, error: impl fmt::Display) -> Self {
        Self::new(NativeErrorKind::Io, format!("failed to {action}: {error}"))
            .with_path(path)
    }
}

impl fmt::Display for NativeError {
    /// Formats the stable category, optional path, and actionable message.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(
                formatter,
                "native {} error at '{}': {}",
                self.kind,
                path.display(),
                self.message
            )?;
        } else {
            write!(formatter, "native {} error: {}", self.kind, self.message)?;
        }
        if let Some(project) = &self.project {
            match project {
                ProjectContext::Found(root) => {
                    write!(formatter, "\nproject: {}", root.display())?;
                }
                ProjectContext::Missing { searched_from } if searched_from.as_os_str().is_empty() => {
                    write!(formatter, "\nproject: not found")?;
                }
                ProjectContext::Missing { searched_from } => {
                    write!(
                        formatter,
                        "\nproject: not found (searched from {})",
                        searched_from.display()
                    )?;
                }
            }
        }
        if let Some(command) = &self.recovery {
            write!(
                formatter,
                "\nrecovery: {}",
                format_recovery(self.project.as_ref(), command)
            )?;
        }
        Ok(())
    }
}

/// Formats a recovery command so it can be pasted from any current directory.
pub(crate) fn recovery_from_project(root: &std::path::Path, command: &str) -> String {
    format!("cd -- {} && {command}", shell_quote(root))
}

/// Combines optional project context with the raw recovery command.
fn format_recovery(project: Option<&ProjectContext>, command: &str) -> String {
    match project {
        Some(ProjectContext::Found(root)) => recovery_from_project(root, command),
        Some(ProjectContext::Missing { searched_from })
            if !searched_from.as_os_str().is_empty() =>
        {
            recovery_from_project(searched_from, command)
        }
        Some(ProjectContext::Missing { .. }) | None => command.to_string(),
    }
}

/// Quotes one filesystem path for POSIX-shell copy-paste diagnostics.
fn shell_quote(path: &std::path::Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

impl fmt::Display for NativeErrorKind {
    /// Formats the category as a stable lowercase diagnostic label.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Usage => "usage",
            Self::Project => "project",
            Self::Manifest => "manifest",
            Self::Lock => "lock",
            Self::Catalog => "catalog",
            Self::Cache => "cache",
            Self::Network => "network",
            Self::Integrity => "integrity",
            Self::Archive => "archive",
            Self::Toolchain => "toolchain",
            Self::Build => "build",
            Self::Io => "I/O",
        };
        formatter.write_str(label)
    }
}

impl std::error::Error for NativeError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies actionable errors share project and copy-paste recovery line formatting.
    #[test]
    fn actionable_errors_render_uniform_recovery_context() {
        let error = NativeError::new(NativeErrorKind::Integrity, "artifact is corrupt")
            .with_path("/work/project/cache/receipt.json")
            .with_project("/work/project")
            .with_recovery("elephc native install --locked --target linux-x86_64");
        let rendered = error.to_string();
        assert!(rendered.starts_with("native integrity error at"));
        assert!(rendered.contains("\nproject: /work/project\n"));
        assert!(rendered.contains(
            "recovery: cd -- '/work/project' && elephc native install --locked --target linux-x86_64"
        ));
    }

    /// Verifies missing-project diagnostics retain the searched directory in both context lines.
    #[test]
    fn missing_project_recovery_uses_search_directory() {
        let error = NativeError::new(NativeErrorKind::Project, "no project")
            .with_missing_project("/work/example")
            .with_recovery("elephc native add pcre2");
        let rendered = error.to_string();
        assert!(rendered.contains("project: not found (searched from /work/example)"));
        assert!(rendered.contains(
            "recovery: cd -- '/work/example' && elephc native add pcre2"
        ));
    }
}
