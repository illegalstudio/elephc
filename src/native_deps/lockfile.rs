//! Purpose:
//! Expands manifests into strict deterministic schema-1 native lockfiles.
//!
//! Called from:
//! - Native state mutation, locked installation, doctor, and compilation resolution.
//!
//! Key details:
//! - All executable metadata comes from the compiled catalog and stale dimensions fail closed.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::catalog;
use super::error::{NativeError, NativeErrorKind};
use super::manifest::ManifestDocument;

/// Strict generated native lockfile schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeLock {
    pub schema: u32,
    #[serde(default)]
    pub package: Vec<LockedPackage>,
}

/// One exact catalog-expanded package in the lock.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    pub recipe: u32,
    pub dependencies: Vec<String>,
    pub provides: Vec<String>,
    pub source: LockedSource,
    pub target: Vec<LockedTarget>,
}

/// Immutable upstream identity recorded by the lock.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedSource {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// Ordered link outputs for one public Elephc target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedTarget {
    pub name: String,
    pub archives: Vec<String>,
    pub system_libraries: Vec<String>,
    pub frameworks: Vec<String>,
}

impl NativeLock {
    /// Expands every manifest dependency from the trusted current catalog.
    pub fn from_manifest(manifest: &ManifestDocument) -> Result<Self, NativeError> {
        let mut package = Vec::new();
        for (name, selected) in manifest.dependencies() {
            let version = catalog::version(name, Some(selected))?;
            package.push(LockedPackage {
                name: name.clone(),
                version: version.version.to_string(),
                recipe: version.recipe_revision,
                dependencies: version.dependencies.iter().map(|value| (*value).to_string()).collect(),
                provides: version.provides.iter().map(|value| (*value).to_string()).collect(),
                source: LockedSource {
                    url: version.source.https_url.to_string(),
                    sha256: version.source.sha256.to_string(),
                    size: version.source.exact_size,
                },
                target: version.supported_targets.iter().map(|target| LockedTarget {
                    name: (*target).to_string(),
                    archives: version.ordered_link_outputs.iter().map(|path| (*path).to_string()).collect(),
                    system_libraries: Vec::new(),
                    frameworks: Vec::new(),
                }).collect(),
            });
        }
        package.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(Self { schema: 1, package })
    }

    /// Parses a strict lock and rejects unknown schemas, fields, and duplicate packages.
    pub fn parse(text: &str) -> Result<Self, NativeError> {
        let lock: Self = toml::from_str(text).map_err(|error| NativeError::new(NativeErrorKind::Lock, format!("invalid native lock: {error}")))?;
        if lock.schema != 1 {
            return Err(NativeError::new(NativeErrorKind::Lock, format!("unsupported native lock schema {}", lock.schema)));
        }
        let mut seen = BTreeMap::new();
        for package in &lock.package {
            if seen.insert(&package.name, ()).is_some() {
                return Err(NativeError::new(NativeErrorKind::Lock, format!("duplicate locked package '{}'", package.name)));
            }
        }
        Ok(lock)
    }

    /// Reads a strict native lock from disk.
    pub fn load(path: &Path) -> Result<Self, NativeError> {
        let text = fs::read_to_string(path).map_err(|error| NativeError::io("read native lock", path, error))?;
        Self::parse(&text).map_err(|error| error.with_path(path))
    }

    /// Validates every lock dimension against a manifest and current catalog expansion.
    pub fn validate_current(&self, manifest: &ManifestDocument) -> Result<(), NativeError> {
        let expected = Self::from_manifest(manifest)?;
        if self != &expected {
            return Err(NativeError::new(
                NativeErrorKind::Lock,
                "native lock is missing or stale; run elephc native install (CI should use install --locked)",
            ));
        }
        Ok(())
    }

    /// Returns one locked package by exact name.
    pub fn package(&self, name: &str) -> Option<&LockedPackage> {
        self.package.iter().find(|package| package.name == name)
    }

    /// Renders deterministic TOML with a generated-file preamble and catalog order.
    pub fn render(&self) -> Result<String, NativeError> {
        let mut normalized = self.clone();
        normalized.package.sort_by(|left, right| left.name.cmp(&right.name));
        let body = toml::to_string(&normalized).map_err(|error| NativeError::new(NativeErrorKind::Lock, format!("cannot render native lock: {error}")))?;
        Ok(format!("# This file is generated by Elephc. Do not edit.\n{body}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the canonical one-package manifest fixture.
    fn manifest() -> ManifestDocument {
        ManifestDocument::parse("[native]\nschema = 1\n[native.dependencies]\npcre2 = \"10.47\"\n").unwrap()
    }

    /// Verifies lock rendering is stable and carries all three ordered target plans.
    #[test]
    fn lock_rendering_is_deterministic() {
        let lock = NativeLock::from_manifest(&manifest()).unwrap();
        let first = lock.render().unwrap();
        let second = NativeLock::parse(&first).unwrap().render().unwrap();
        assert_eq!(first, second);
        assert_eq!(lock.package[0].target.len(), 3);
        assert_eq!(lock.package[0].target[0].archives[0], "lib/libelephc_pcre2_shim.a");
    }

    /// Verifies every stale catalog dimension and unknown field fails closed.
    #[test]
    fn stale_or_extended_lock_is_rejected() {
        let mut lock = NativeLock::from_manifest(&manifest()).unwrap();
        lock.package[0].recipe += 1;
        assert!(lock.validate_current(&manifest()).is_err());
        let text = "schema=1\nunknown=true\n";
        assert!(NativeLock::parse(text).is_err());
    }
}
