//! Purpose:
//! Owns the per-parse source profile shared by the lexer, parser and AST constructors.
//!
//! Called from:
//! - Parser entry points, lexer scanners, and the root crate's physical-source loader.
//!
//! Key details:
//! - `.lfc` is the only tagless physical suffix; every other path is tagged PHP.
//! - `strict_types` is stamped on every AST node created during one physical file's parse.

use std::cell::Cell;
use std::path::Path;

/// Language profile selected for one physical source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceMode {
    /// Tagged PHP-compatible source that requires the normal `<?php` opening tag.
    Php,
    /// Tagless elephc source with every elephc extension available.
    Lfc,
    /// Compiler-generated source that is never subject to the user strict-PHP audit.
    Internal,
}

/// Everything one physical source file contributes to the AST nodes parsed from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceProfile {
    /// Language profile selected from the physical path.
    pub mode: SourceMode,
    /// Whether the file declared `strict_types=1`.
    pub strict_types: bool,
}

impl SourceProfile {
    /// Builds the profile a physical file starts parsing with.
    pub fn new(mode: SourceMode) -> Self {
        Self { mode, strict_types: false }
    }
}

/// Immutable visibility and semantic policy attached to literal eval source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct EvalSourceProfile(u32);

impl EvalSourceProfile {
    const PHP_ONLY_BIT: u32 = 1 << 31;

    /// Joins lexical builtin visibility and semantic guarantees without truncating either.
    pub const fn new(php_only: bool, semantics: elephc_semantics::SemanticProfile) -> Self {
        assert!(semantics.bits() < Self::PHP_ONLY_BIT as u64);
        Self(semantics.bits() as u32 | if php_only { Self::PHP_ONLY_BIT } else { 0 })
    }

    /// Returns the lexical builtin visibility without changing semantic guarantees.
    pub const fn php_only(self) -> bool {
        self.0 & Self::PHP_ONLY_BIT != 0
    }

    /// Recovers the validated neutral profile stored by the constructor.
    pub fn semantics(self) -> elephc_semantics::SemanticProfile {
        elephc_semantics::SemanticProfile::from_bits(u64::from(self.0 & !Self::PHP_ONLY_BIT))
            .expect("source profiles contain only validated semantic bits")
    }

    /// Makes conservative declaration scans explicit about their permissive parser policy.
    pub const fn permissive(php_only: bool) -> Self {
        Self::new(php_only, elephc_semantics::SemanticProfile::PERMISSIVE)
    }
}

thread_local! {
    static CURRENT_PARSE_MODE: Cell<SourceMode> = const { Cell::new(SourceMode::Internal) };
    static CURRENT_STRICT_TYPES: Cell<bool> = const { Cell::new(false) };
}

impl SourceMode {
    /// Classifies a physical path, treating only a case-insensitive `.lfc` suffix as tagless.
    pub fn from_path(path: &Path) -> Self {
        if path.extension().and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lfc"))
        {
            Self::Lfc
        } else {
            Self::Php
        }
    }

    /// Returns whether this source mode requires a user-written `<?php` opening tag.
    pub fn requires_open_tag(self) -> bool {
        matches!(self, Self::Php)
    }

    /// Returns whether an invocation-level PHP-only request applies to this source.
    pub fn php_only_is_effective(self, requested: bool) -> bool {
        requested && matches!(self, Self::Php)
    }
}

/// Returns whether Composer discovery should inspect a path as PHP/LFC source.
pub fn is_composer_source_path(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| {
        extension.eq_ignore_ascii_case("php") || extension.eq_ignore_ascii_case("lfc")
    })
}

/// Removes the recognized source suffix from one Composer-relative path component.
pub fn composer_source_stem(component: &str) -> String {
    Path::new(component).file_stem().and_then(|stem| stem.to_str()).unwrap_or(component).to_string()
}

/// RAII guard restoring the parser's previous source profile on drop.
pub struct ParseModeGuard {
    previous: SourceProfile,
}

impl Drop for ParseModeGuard {
    /// Restores the parser source profile active before the nested parse.
    fn drop(&mut self) {
        CURRENT_PARSE_MODE.with(|cell| cell.set(self.previous.mode));
        CURRENT_STRICT_TYPES.with(|cell| cell.set(self.previous.strict_types));
    }
}

/// Runs `f` while parser-created AST nodes inherit `profile`.
pub fn with_parse_mode<T>(profile: SourceProfile, f: impl FnOnce() -> T) -> T {
    let _guard = scoped_parse_mode(profile);
    f()
}

/// Installs one parser/source reconstruction profile until the returned guard is dropped.
pub fn scoped_parse_mode(profile: SourceProfile) -> ParseModeGuard {
    let mode = CURRENT_PARSE_MODE.with(|cell| cell.replace(profile.mode));
    let strict_types = CURRENT_STRICT_TYPES.with(|cell| cell.replace(profile.strict_types));
    ParseModeGuard { previous: SourceProfile { mode, strict_types } }
}

/// Returns the source mode assigned to AST nodes created at the current parse site.
pub fn current_parse_mode() -> SourceMode {
    CURRENT_PARSE_MODE.with(Cell::get)
}

/// Returns the `strict_types` state assigned to AST nodes created at the current parse site.
pub fn current_strict_types() -> bool {
    CURRENT_STRICT_TYPES.with(Cell::get)
}

/// Records that the file currently being parsed declared `strict_types=<enabled>`.
pub fn declare_strict_types(enabled: bool) {
    CURRENT_STRICT_TYPES.with(|cell| cell.set(enabled));
}
