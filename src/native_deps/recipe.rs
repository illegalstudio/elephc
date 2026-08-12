//! Purpose:
//! Defines the injected curated-recipe boundary and dispatches trusted package builds.
//!
//! Called from:
//! - Native artifact materialization after verified extraction.
//!
//! Key details:
//! - Recipe selection is compiled into Elephc and cannot be supplied by manifest or lock data.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::codegen_support::platform::Target;

use super::catalog::PackageVersion;
use super::error::{NativeError, NativeErrorKind};
use super::recipes::{curl, openssl, pcre2, zlib};
use super::toolchain::NativeToolchain;

/// Immutable inputs to one trusted package recipe invocation.
pub struct RecipeRequest<'a> {
    pub package: &'a str,
    pub version: &'a PackageVersion,
    pub target: Target,
    pub source: &'a Path,
    pub staging_prefix: &'a Path,
    pub toolchain: &'a NativeToolchain,
    /// Final artifact prefixes (each containing `include/` and `lib/`) for every catalog
    /// dependency already materialized before this recipe runs, keyed by package name. Empty for
    /// packages with no `PackageVersion::dependencies`.
    pub dependency_prefixes: &'a BTreeMap<String, PathBuf>,
}

/// Injectable curated build executor used by production and network-free tests.
pub trait RecipeRunner {
    /// Produces every catalog-declared retained output below the staging prefix.
    fn build(&self, request: &RecipeRequest<'_>) -> Result<(), NativeError>;
}

/// Production dispatcher containing only reviewed built-in recipes.
pub struct CuratedRecipes;

/// Identifies one reviewed package recipe compiled into Elephc.
enum BuiltInRecipe {
    Pcre2,
    Zlib,
    Openssl,
    Curl,
}

/// Resolves a package and immutable recipe revision to its built-in executor.
fn built_in_recipe(package: &str, revision: u32) -> Option<BuiltInRecipe> {
    match (package, revision) {
        ("pcre2", 2) => Some(BuiltInRecipe::Pcre2),
        ("zlib", 1) => Some(BuiltInRecipe::Zlib),
        ("openssl", 1) => Some(BuiltInRecipe::Openssl),
        ("curl", 1) => Some(BuiltInRecipe::Curl),
        _ => None,
    }
}

impl RecipeRunner for CuratedRecipes {
    /// Dispatches by catalog package name and recipe revision.
    fn build(&self, request: &RecipeRequest<'_>) -> Result<(), NativeError> {
        match built_in_recipe(request.package, request.version.recipe_revision) {
            Some(BuiltInRecipe::Pcre2) => pcre2::build(request),
            Some(BuiltInRecipe::Zlib) => zlib::build(request),
            Some(BuiltInRecipe::Openssl) => openssl::build(request),
            Some(BuiltInRecipe::Curl) => curl::build(request),
            None => Err(NativeError::new(
                NativeErrorKind::Build,
                format!(
                    "no built-in recipe for {} revision {}",
                    request.package, request.version.recipe_revision
                ),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_deps::catalog;

    /// Verifies every current catalog recipe revision has a production dispatcher.
    #[test]
    fn current_catalog_recipe_revisions_have_dispatchers() {
        let names = catalog::known_names();
        for package in names.split(", ") {
            let version = catalog::version(package, None).expect("catalog version");
            assert!(
                built_in_recipe(package, version.recipe_revision).is_some(),
                "missing built-in recipe for {package} revision {}",
                version.recipe_revision
            );
        }
    }

    /// Verifies the revised PCRE2 shim is not reused under its previous recipe identity.
    #[test]
    fn previous_pcre2_recipe_revision_is_not_dispatched() {
        assert!(built_in_recipe("pcre2", 1).is_none());
    }

    /// Verifies the dispatcher recognizes curl and its OpenSSL TLS-backend dependency by exact
    /// catalog recipe revision, independent of `current_catalog_recipe_revisions_have_dispatchers`
    /// so this fails closed even if the catalog entries are ever added without their recipes.
    #[test]
    fn curl_and_openssl_recipe_revisions_have_dispatchers() {
        assert!(
            built_in_recipe("openssl", 1).is_some(),
            "missing built-in recipe for openssl revision 1"
        );
        assert!(
            built_in_recipe("curl", 1).is_some(),
            "missing built-in recipe for curl revision 1"
        );
    }
}
