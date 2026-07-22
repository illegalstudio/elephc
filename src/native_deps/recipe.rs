//! Purpose:
//! Defines the injected curated-recipe boundary and dispatches trusted package builds.
//!
//! Called from:
//! - Native artifact materialization after verified extraction.
//!
//! Key details:
//! - Recipe selection is compiled into Elephc and cannot be supplied by manifest or lock data.

use std::path::Path;

use crate::codegen_support::platform::Target;

use super::catalog::PackageVersion;
use super::error::{NativeError, NativeErrorKind};
use super::recipes::pcre2;
use super::toolchain::NativeToolchain;

/// Immutable inputs to one trusted package recipe invocation.
pub struct RecipeRequest<'a> {
    pub package: &'a str,
    pub version: &'a PackageVersion,
    pub target: Target,
    pub source: &'a Path,
    pub staging_prefix: &'a Path,
    pub toolchain: &'a NativeToolchain,
}

/// Injectable curated build executor used by production and network-free tests.
pub trait RecipeRunner {
    /// Produces every catalog-declared retained output below the staging prefix.
    fn build(&self, request: &RecipeRequest<'_>) -> Result<(), NativeError>;
}

/// Production dispatcher containing only reviewed built-in recipes.
pub struct CuratedRecipes;

impl RecipeRunner for CuratedRecipes {
    /// Dispatches by catalog package name and recipe revision.
    fn build(&self, request: &RecipeRequest<'_>) -> Result<(), NativeError> {
        match (request.package, request.version.recipe_revision) {
            ("pcre2", 1) => pcre2::build(request),
            (package, revision) => Err(NativeError::new(NativeErrorKind::Build, format!("no built-in recipe for {package} revision {revision}"))),
        }
    }
}
