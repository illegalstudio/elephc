//! Purpose:
//! Reads and edits the `[extension]` section of `elephc.toml`, which declares the
//! real PHP extensions a program hosts and the INI directives they start with.
//!
//! Called from:
//! - `elephc extension add/remove/install/list` and extension resolution during
//!   compilation.
//!
//! Key details:
//! - Mirrors `native_deps::manifest`: strict about its own section, blind to
//!   every other one, so `[native]` and hand-written TOML survive an edit intact.
//! - Three sources. A PECL release (`{ version, sha256 }`), a PIE package from
//!   Packagist (`{ pie = "vendor/name", version, sha256 }`), and a local source
//!   tree (`{ path = "ext/demo" }`) for in-house extensions.
//! - Remote sources are pinned by exact version AND by the SHA-256 of the
//!   archive `elephc extension add` downloaded. PECL publishes no checksums, so the
//!   first download is trusted and every later one must match it; a range would
//!   make a build's contents depend on when it ran.
//! - `[extension.ini]` holds directives applied before any extension starts, the
//!   way php.ini is — APCu, for one, does nothing in CLI without
//!   `apc.enable_cli = "1"`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use toml_edit::{value, DocumentMut, InlineTable, Item, Table, Value};

use crate::native_deps::{NativeError, NativeErrorKind};

fn manifest_error(message: impl Into<String>) -> NativeError {
    NativeError::new(NativeErrorKind::Manifest, message)
}

/// Where a hosted extension's source comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtensionSource {
    /// A release from pecl.php.net, `https://pecl.php.net/get/<name>-<version>.tgz`.
    Pecl { version: String, sha256: String },
    /// A PIE package: a Packagist package of type `php-ext`.
    Pie {
        package: String,
        version: String,
        sha256: String,
    },
    /// A source tree on disk, relative to the manifest's directory.
    Path { path: PathBuf },
}

impl ExtensionSource {
    /// A short human description, used by `list` and in diagnostics.
    pub fn describe(&self) -> String {
        match self {
            Self::Pecl { version, .. } => format!("pecl {version}"),
            Self::Pie { package, version, .. } => format!("pie {package} {version}"),
            Self::Path { path } => format!("path {}", path.display()),
        }
    }
}

/// Comment-preserving manifest document and its validated `[extension]` content.
#[derive(Clone, Debug)]
pub struct PhpExtManifest {
    document: DocumentMut,
    extensions: BTreeMap<String, ExtensionSource>,
    ini: BTreeMap<String, String>,
}

impl PhpExtManifest {
    /// An empty manifest with no `[extension]` section yet.
    pub fn new() -> Self {
        Self {
            document: DocumentMut::new(),
            extensions: BTreeMap::new(),
            ini: BTreeMap::new(),
        }
    }

    /// Parses and strictly validates the `[extension]` section.
    ///
    /// A manifest with no `[extension]` section is valid and declares nothing:
    /// hosting is opt-in.
    pub fn parse(text: &str) -> Result<Self, NativeError> {
        let document = DocumentMut::from_str(text)
            .map_err(|error| manifest_error(format!("invalid TOML: {error}")))?;
        let mut manifest = Self {
            document,
            extensions: BTreeMap::new(),
            ini: BTreeMap::new(),
        };
        let Some(section) = manifest.document.get("extension").and_then(Item::as_table) else {
            return Ok(manifest);
        };
        for (key, _) in section.iter() {
            if !matches!(key, "schema" | "dependencies" | "ini") {
                return Err(manifest_error(format!("unknown key 'extension.{key}'")));
            }
        }
        if section.get("schema").and_then(Item::as_integer) != Some(1) {
            return Err(manifest_error("extension.schema is required and must equal 1"));
        }

        let mut folded = BTreeSet::new();
        if let Some(item) = section.get("dependencies") {
            let table = item
                .as_table_like()
                .ok_or_else(|| manifest_error("extension.dependencies must be a table"))?;
            for (name, entry) in table.iter() {
                validate_extension_name(name)?;
                if !folded.insert(name.to_ascii_lowercase()) {
                    return Err(manifest_error(format!("duplicate case-variant extension '{name}'")));
                }
                let source = parse_source(name, entry)?;
                manifest.extensions.insert(name.to_string(), source);
            }
        }
        if let Some(item) = section.get("ini") {
            let table = item
                .as_table_like()
                .ok_or_else(|| manifest_error("extension.ini must be a table"))?;
            for (directive, entry) in table.iter() {
                let setting = entry.as_str().ok_or_else(|| {
                    manifest_error(format!(
                        "extension.ini '{directive}' must be a string, as it would be in php.ini"
                    ))
                })?;
                manifest.ini.insert(directive.to_string(), setting.to_string());
            }
        }
        Ok(manifest)
    }

    /// Reads and parses a manifest from disk.
    pub fn load(path: &Path) -> Result<Self, NativeError> {
        let text = fs::read_to_string(path)
            .map_err(|error| NativeError::io("read extension manifest", path, error))?;
        Self::parse(&text).map_err(|error| error.with_path(path))
    }

    /// Declared extensions in deterministic name order.
    pub fn extensions(&self) -> &BTreeMap<String, ExtensionSource> {
        &self.extensions
    }

    /// INI directives applied before any hosted extension starts.
    pub fn ini(&self) -> &BTreeMap<String, String> {
        &self.ini
    }

    /// Adds or replaces an extension, creating the section when absent.
    pub fn set_extension(&mut self, name: &str, source: ExtensionSource) -> Result<(), NativeError> {
        validate_extension_name(name)?;
        validate_source(name, &source)?;
        self.ensure_section();
        let mut entry = InlineTable::new();
        match &source {
            ExtensionSource::Pecl { version, sha256 } => {
                entry.insert("version", Value::from(version.as_str()));
                entry.insert("sha256", Value::from(sha256.as_str()));
            }
            ExtensionSource::Pie { package, version, sha256 } => {
                entry.insert("pie", Value::from(package.as_str()));
                entry.insert("version", Value::from(version.as_str()));
                entry.insert("sha256", Value::from(sha256.as_str()));
            }
            ExtensionSource::Path { path } => {
                entry.insert("path", Value::from(path.to_string_lossy().as_ref()));
            }
        }
        self.document["extension"]["dependencies"][name] = value(entry);
        self.extensions.insert(name.to_string(), source);
        Ok(())
    }

    /// Removes an extension; returns whether it was declared.
    pub fn remove_extension(&mut self, name: &str) -> bool {
        let removed = self.extensions.remove(name).is_some();
        if removed {
            if let Some(table) = self
                .document
                .get_mut("extension")
                .and_then(Item::as_table_mut)
                .and_then(|section| section.get_mut("dependencies"))
                .and_then(Item::as_table_like_mut)
            {
                table.remove(name);
            }
        }
        removed
    }

    /// Renders the manifest, preserving unrelated sections and comments.
    pub fn render(&self) -> String {
        self.document.to_string()
    }

    fn ensure_section(&mut self) {
        if self.document.get("extension").and_then(Item::as_table).is_none() {
            self.document["extension"] = Item::Table(Table::new());
            self.document["extension"]["schema"] = value(1);
        }
        if self.document["extension"].get("dependencies").is_none() {
            self.document["extension"]["dependencies"] = Item::Table(Table::new());
        }
    }
}

impl Default for PhpExtManifest {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads one `[extension.dependencies]` entry.
fn parse_source(name: &str, item: &Item) -> Result<ExtensionSource, NativeError> {
    if item.is_str() {
        return Err(manifest_error(format!(
            "php extension '{name}' must be a table such as {{ version = \"1.0.0\", sha256 = \"…\" }}; \
             run `elephc extension add {name}@<version>` to pin it"
        )));
    }
    let table = item
        .as_table_like()
        .ok_or_else(|| manifest_error(format!("php extension '{name}' must be a table")))?;
    for (key, _) in table.iter() {
        if !matches!(key, "version" | "sha256" | "pie" | "path") {
            return Err(manifest_error(format!("unknown key '{key}' in php extension '{name}'")));
        }
    }
    let text = |key: &str| -> Result<Option<String>, NativeError> {
        match table.get(key) {
            None => Ok(None),
            Some(entry) => entry.as_str().map(|s| Some(s.to_string())).ok_or_else(|| {
                manifest_error(format!("'{key}' of php extension '{name}' must be a string"))
            }),
        }
    };
    let source = match (text("path")?, text("pie")?) {
        (Some(_), Some(_)) => {
            return Err(manifest_error(format!(
                "php extension '{name}' names both a path and a PIE package"
            )))
        }
        (Some(path), None) => {
            if table.contains_key("version") || table.contains_key("sha256") {
                return Err(manifest_error(format!(
                    "php extension '{name}' comes from a path; a version or sha256 would pin nothing"
                )));
            }
            ExtensionSource::Path { path: PathBuf::from(path) }
        }
        (None, pie) => {
            let version = text("version")?
                .ok_or_else(|| manifest_error(format!("php extension '{name}' has no version")))?;
            let sha256 = text("sha256")?.ok_or_else(|| {
                manifest_error(format!(
                    "php extension '{name}' has no sha256; run `elephc extension add` to pin it"
                ))
            })?;
            match pie {
                Some(package) => ExtensionSource::Pie { package, version, sha256 },
                None => ExtensionSource::Pecl { version, sha256 },
            }
        }
    };
    validate_source(name, &source)?;
    Ok(source)
}

fn validate_source(name: &str, source: &ExtensionSource) -> Result<(), NativeError> {
    match source {
        ExtensionSource::Pecl { version, sha256 } => {
            validate_exact_version(name, version)?;
            validate_sha256(name, sha256)
        }
        ExtensionSource::Pie { package, version, sha256 } => {
            validate_pie_package(name, package)?;
            validate_exact_version(name, version)?;
            validate_sha256(name, sha256)
        }
        ExtensionSource::Path { path } => {
            if path.as_os_str().is_empty() {
                return Err(manifest_error(format!("php extension '{name}' has an empty path")));
            }
            Ok(())
        }
    }
}

/// Extension names follow PECL: lowercase ASCII, digits, underscore.
pub(super) fn validate_extension_name(name: &str) -> Result<(), NativeError> {
    if name.is_empty() {
        return Err(manifest_error("extension name must not be empty"));
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(manifest_error(format!(
            "extension key '{name}' must be lowercase ASCII letters, digits or underscore"
        )));
    }
    Ok(())
}

/// Exact versions only: `5.1.28`, never `^5.1` or `*`.
fn validate_exact_version(name: &str, version: &str) -> Result<(), NativeError> {
    if version.is_empty() {
        return Err(manifest_error(format!("php extension '{name}' has an empty version")));
    }
    // Ranges are also caught by the character check below, but only this branch
    // can say *why* — "not a range" is actionable, "unexpected characters" is not.
    let looks_like_range = version.starts_with(['^', '~', '>', '<', '=', '*'])
        || version.contains(['*', ' ', ',', '|']);
    if looks_like_range {
        return Err(manifest_error(format!(
            "php extension '{name}' version '{version}' must be exact, not a range"
        )));
    }
    if !version
        .bytes()
        .all(|b| b.is_ascii_digit() || b == b'.' || b == b'-' || b.is_ascii_alphabetic())
    {
        return Err(manifest_error(format!(
            "php extension '{name}' version '{version}' has unexpected characters"
        )));
    }
    Ok(())
}

fn validate_sha256(name: &str, sha256: &str) -> Result<(), NativeError> {
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        return Err(manifest_error(format!(
            "php extension '{name}' sha256 must be 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

/// Composer package names: `vendor/name`, lowercase.
fn validate_pie_package(name: &str, package: &str) -> Result<(), NativeError> {
    let valid_part = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b))
    };
    match package.split_once('/') {
        Some((vendor, project)) if valid_part(vendor) && valid_part(project) => Ok(()),
        _ => Err(manifest_error(format!(
            "php extension '{name}' PIE package '{package}' must look like vendor/name"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "c3d9ec5e3a7b8c0f5f5b0f41fca1c1f1bb3b3a8f7e1f0b8f1e8c0f5b0f41fca1";

    #[test]
    fn parses_every_source_kind() {
        let text = format!(
            r#"
[extension]
schema = 1

[extension.dependencies]
simdjson = {{ version = "4.0.0", sha256 = "{SHA}" }}
apcu = {{ pie = "apcu/apcu", version = "5.1.28", sha256 = "{SHA}" }}
demo = {{ path = "ext/demo" }}

[extension.ini]
"apc.enable_cli" = "1"
"#
        );
        let manifest = PhpExtManifest::parse(&text).expect("valid manifest");
        assert_eq!(manifest.extensions().len(), 3);
        assert!(matches!(
            manifest.extensions().get("simdjson"),
            Some(ExtensionSource::Pecl { version, .. }) if version == "4.0.0"
        ));
        assert!(matches!(
            manifest.extensions().get("apcu"),
            Some(ExtensionSource::Pie { package, .. }) if package == "apcu/apcu"
        ));
        assert!(matches!(
            manifest.extensions().get("demo"),
            Some(ExtensionSource::Path { path }) if path == Path::new("ext/demo")
        ));
        assert_eq!(manifest.ini().get("apc.enable_cli").map(String::as_str), Some("1"));
    }

    /// Hosting is opt-in: a project that declares none is not in error.
    #[test]
    fn a_manifest_without_the_section_declares_nothing() {
        let manifest = PhpExtManifest::parse("[native]\nschema = 1\n\n[native.dependencies]\n")
            .expect("valid manifest");
        assert!(manifest.extensions().is_empty());
    }

    /// The whole point of mirroring native_deps: editing one section must not
    /// disturb another, nor drop comments.
    #[test]
    fn editing_preserves_unrelated_sections_and_comments() {
        let text = "# project manifest\n[native]\nschema = 1\n\n[native.dependencies]\nzlib = \"1.3.1\"\n";
        let mut manifest = PhpExtManifest::parse(text).expect("valid");
        manifest
            .set_extension("apcu", ExtensionSource::Pecl { version: "5.1.28".into(), sha256: SHA.into() })
            .expect("accepted");
        let rendered = manifest.render();
        assert!(rendered.contains("# project manifest"), "comment kept");
        assert!(rendered.contains("zlib = \"1.3.1\""), "native entry kept");
        assert!(rendered.contains("[extension.dependencies]"));
        assert!(rendered.contains("apcu = { version = \"5.1.28\""), "new entry written: {rendered}");
        let reparsed = PhpExtManifest::parse(&rendered).expect("round-trips");
        assert_eq!(reparsed.extensions().len(), 1);
    }

    #[test]
    fn removes_a_declared_extension() {
        let mut manifest = PhpExtManifest::new();
        manifest
            .set_extension("ds", ExtensionSource::Path { path: PathBuf::from("ext/ds") })
            .expect("ok");
        assert!(manifest.remove_extension("ds"));
        assert!(!manifest.remove_extension("ds"), "second removal is a no-op");
        assert!(!manifest.render().contains("ds ="));
    }

    /// A bare version string pins no archive digest; refusing it with the
    /// command that fixes it is kinder than trusting whatever downloads next.
    #[test]
    fn a_bare_version_string_is_refused_with_the_fix() {
        let text = "[extension]\nschema = 1\n\n[extension.dependencies]\napcu = \"5.1.28\"\n";
        let error = PhpExtManifest::parse(text).expect_err("unpinned");
        assert!(error.to_string().contains("elephc extension add apcu@<version>"), "{error}");
    }

    /// Ranges must be refused *as ranges*: asserting only that the call fails is
    /// vacuous, since the character check refuses these strings anyway.
    #[test]
    fn rejects_version_ranges_with_an_actionable_message() {
        let mut manifest = PhpExtManifest::new();
        for bad in ["^5.1", "~5.1.0", ">=5.0", "*", "5.1.* ", "5.1 || 5.2"] {
            let error = manifest
                .set_extension("apcu", ExtensionSource::Pecl { version: bad.into(), sha256: SHA.into() })
                .expect_err("a build's ABI surface cannot depend on when it ran");
            assert!(error.to_string().contains("must be exact, not a range"), "'{bad}': {error}");
        }
    }

    #[test]
    fn rejects_a_malformed_digest() {
        let mut manifest = PhpExtManifest::new();
        let error = manifest
            .set_extension("apcu", ExtensionSource::Pecl { version: "5.1.28".into(), sha256: "ABC".into() })
            .expect_err("not a sha256");
        assert!(error.to_string().contains("64 lowercase hexadecimal"));
    }

    #[test]
    fn rejects_a_path_that_also_pins_a_version() {
        let text = "[extension]\nschema = 1\n\n[extension.dependencies]\ndemo = { path = \"ext\", version = \"1.0\" }\n";
        assert!(PhpExtManifest::parse(text).is_err());
    }

    #[test]
    fn rejects_unknown_keys_and_a_wrong_schema() {
        assert!(PhpExtManifest::parse("[extension]\nschema = 1\nnope = true\n").is_err());
        assert!(PhpExtManifest::parse("[extension]\nschema = 2\n").is_err());
    }

    #[test]
    fn rejects_uppercase_names_and_bad_pie_packages() {
        let mut manifest = PhpExtManifest::new();
        let path = || ExtensionSource::Path { path: PathBuf::from("x") };
        assert!(manifest.set_extension("SimdJson", path()).is_err());
        let error = manifest
            .set_extension("apcu", ExtensionSource::Pie { package: "apcu".into(), version: "1.0.0".into(), sha256: SHA.into() })
            .expect_err("no vendor");
        assert!(error.to_string().contains("vendor/name"));
    }

    #[test]
    fn ini_values_must_be_strings() {
        let text = "[extension]\nschema = 1\n\n[extension.ini]\n\"apc.enable_cli\" = 1\n";
        let error = PhpExtManifest::parse(text).expect_err("not a string");
        assert!(error.to_string().contains("as it would be in php.ini"));
    }
}
