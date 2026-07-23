//! Purpose:
//! Defines logical native package requirements independently of linker spelling.
//!
//! Called from:
//! - Runtime feature detection and `crate::native_deps::resolver`.
//!
//! Key details:
//! - A declaration makes a package resolvable but never force-links it.

/// A logical package capability required by a compiled program.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NativeRequirement {
    Package(String),
}

impl NativeRequirement {
    /// Creates a requirement for a catalog package name.
    pub fn package(name: impl Into<String>) -> Self {
        Self::Package(name.into())
    }

    /// Returns the catalog package name represented by this requirement.
    pub fn package_name(&self) -> &str {
        match self {
            Self::Package(name) => name,
        }
    }
}
