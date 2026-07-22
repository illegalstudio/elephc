//! Purpose:
//! Reads and edits schema-1 native manifests while preserving unrelated TOML text and comments.
//!
//! Called from:
//! - Native add, update, remove, install, list, doctor, and compilation resolution.
//!
//! Key details:
//! - The `[native]` section is strict; unrelated top-level sections remain untouched.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::str::FromStr;

use toml_edit::{value, DocumentMut, Item, Table, Value};

use super::catalog;
use super::error::{NativeError, NativeErrorKind};

/// Comment-preserving manifest document and validated dependency selection.
#[derive(Clone, Debug)]
pub struct ManifestDocument {
    document: DocumentMut,
    dependencies: BTreeMap<String, String>,
}

impl ManifestDocument {
    /// Creates a minimal schema-1 manifest in memory.
    pub fn new() -> Self {
        let mut document = DocumentMut::new();
        document["native"] = Item::Table(Table::new());
        document["native"]["schema"] = value(1);
        document["native"]["dependencies"] = Item::Table(Table::new());
        Self { document, dependencies: BTreeMap::new() }
    }

    /// Parses and strictly validates the native section of a manifest.
    pub fn parse(text: &str) -> Result<Self, NativeError> {
        let document = DocumentMut::from_str(text).map_err(|error| NativeError::new(NativeErrorKind::Manifest, format!("invalid TOML: {error}")))?;
        let native = document.get("native").and_then(Item::as_table).ok_or_else(|| NativeError::new(NativeErrorKind::Manifest, "missing [native] table"))?;
        for (key, _) in native.iter() {
            if !matches!(key, "schema" | "dependencies") {
                return Err(NativeError::new(NativeErrorKind::Manifest, format!("unknown key 'native.{key}'")));
            }
        }
        if native.get("schema").and_then(Item::as_integer) != Some(1) {
            return Err(NativeError::new(NativeErrorKind::Manifest, "native.schema is required and must equal 1"));
        }
        let table = native.get("dependencies").and_then(Item::as_table).ok_or_else(|| NativeError::new(NativeErrorKind::Manifest, "missing [native.dependencies] table"))?;
        let mut dependencies = BTreeMap::new();
        let mut folded = BTreeSet::new();
        for (name, item) in table.iter() {
            if !name.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-') {
                return Err(NativeError::new(NativeErrorKind::Manifest, format!("dependency key '{name}' must be lowercase ASCII")));
            }
            if !folded.insert(name.to_ascii_lowercase()) {
                return Err(NativeError::new(NativeErrorKind::Manifest, format!("duplicate case-variant dependency '{name}'")));
            }
            let version = item.as_str().ok_or_else(|| NativeError::new(NativeErrorKind::Manifest, format!("native dependency '{name}' must be an exact-version string")))?;
            catalog::version(name, Some(version))?;
            dependencies.insert(name.to_string(), version.to_string());
        }
        Ok(Self { document, dependencies })
    }

    /// Reads and parses a manifest from disk.
    pub fn load(path: &Path) -> Result<Self, NativeError> {
        let text = fs::read_to_string(path).map_err(|error| NativeError::io("read native manifest", path, error))?;
        Self::parse(&text).map_err(|error| error.with_path(path))
    }

    /// Returns validated dependencies in deterministic package-name order.
    pub fn dependencies(&self) -> &BTreeMap<String, String> {
        &self.dependencies
    }

    /// Declares or replaces one exact catalog version while preserving surrounding formatting.
    pub fn set_dependency(&mut self, name: &str, version: &str) -> Result<(), NativeError> {
        catalog::version(name, Some(version))?;
        let item = &mut self.document["native"]["dependencies"][name];
        if let Some(current) = item.as_value_mut() {
            let decor = current.decor().clone();
            let mut replacement = Value::from(version);
            *replacement.decor_mut() = decor;
            *current = replacement;
        } else {
            *item = value(version);
        }
        self.dependencies.insert(name.to_string(), version.to_string());
        Ok(())
    }

    /// Removes a dependency and reports whether it existed.
    pub fn remove_dependency(&mut self, name: &str) -> bool {
        let removed = self.dependencies.remove(name).is_some();
        if let Some(table) = self.document["native"]["dependencies"].as_table_mut() {
            table.remove(name);
        }
        removed
    }

    /// Renders the comment-preserving manifest document.
    pub fn render(&self) -> String {
        self.document.to_string()
    }
}

impl Default for ManifestDocument {
    /// Creates the minimal schema-1 manifest.
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies edits preserve comments and unrelated top-level sections.
    #[test]
    fn edits_preserve_comments_and_unrelated_sections() {
        let text = "# project note\n[application]\nname = \"demo\" # keep\n\n[native]\nschema = 1\n\n[native.dependencies]\n# dependency note\npcre2 = \"10.47\"\n";
        let mut manifest = ManifestDocument::parse(text).unwrap();
        manifest.set_dependency("pcre2", "10.47").unwrap();
        let rendered = manifest.render();
        assert!(rendered.contains("# project note"));
        assert!(rendered.contains("name = \"demo\" # keep"));
        assert!(rendered.contains("# dependency note"));
    }

    /// Verifies strict native schema validation fails closed.
    #[test]
    fn rejects_unknown_native_keys_and_non_strings() {
        assert!(ManifestDocument::parse("[native]\nschema=1\nscript='oops'\n[native.dependencies]\n").is_err());
        assert!(ManifestDocument::parse("[native]\nschema=1\n[native.dependencies]\npcre2=10\n").is_err());
        assert!(ManifestDocument::parse("[native]\nschema=2\n[native.dependencies]\n").is_err());
    }
}
