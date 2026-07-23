//! Purpose:
//! Defines structured failures for native dependency parsing, project state, integrity, and tools.
//!
//! Called from:
//! - Every `crate::native_deps` module and top-level CLI integration.
//!
//! Key details:
//! - Errors retain a stable category while presenting actionable human diagnostics.

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
}

impl NativeError {
    /// Constructs an error in `kind` with a user-facing message.
    pub fn new(kind: NativeErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), path: None }
    }

    /// Attaches the path that caused this failure.
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
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
            write!(formatter, "native {} error at '{}': {}", self.kind, path.display(), self.message)
        } else {
            write!(formatter, "native {} error: {}", self.kind, self.message)
        }
    }
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
