//! Purpose:
//! Records manifest-dependent injection sites and rebakes them after autoload.
//!
//! Called from:
//! - The OPcache prelude facade and sibling rendering modules.
//!
//! Key details:
//! - Only recorded synthetic declarations are replaced.

#[allow(unused_imports)]
use super::*;

/// Which manifest-dependent OPcache functions [`inject_if_used`] actually injected, and under
/// which `opcache.restrict_api` verdict — everything [`bake_manifest`] needs to re-render them
/// against the complete script manifest.
///
/// Recording the sites (rather than having `bake_manifest` re-scan for the three names) is what
/// makes baking safe when the program declares its OWN `opcache_get_status()`: `inject_if_used`
/// skips injection in that case, the corresponding flag stays `false`, and the user's function
/// is never touched.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ManifestBakeSites {
    /// `opcache_get_status` was injected (its `scripts` map, cached-script counts, memory
    /// figures and `preload_statistics` block are all manifest-derived).
    pub(super) get_status: bool,
    /// `opcache_is_script_cached` was injected with its NORMAL body (the restricted body
    /// carries no manifest).
    pub(super) is_script_cached: bool,
    /// `opcache_compile_file` was injected. Reference PHP never restricts this one, so its body
    /// is always the manifest-carrying form.
    pub(super) compile_file: bool,
    /// `opcache_invalidate` was injected with its NORMAL body. It reads the manifest to decide
    /// whether a FORCED call records a discard (see `INVALIDATE_TEMPLATE`); the restricted body
    /// carries no manifest.
    pub(super) invalidate: bool,
    /// The compile-time `opcache.restrict_api` verdict ([`restrict_api_denies`]), replayed so
    /// the re-rendered `opcache_get_status` keeps the same gate it was injected with.
    pub(super) restricted: bool,
}

impl ManifestBakeSites {
    /// Whether there is nothing to bake — no manifest-dependent function was injected.
    pub fn is_empty(&self) -> bool {
        !self.get_status && !self.is_script_cached && !self.compile_file && !self.invalidate
    }
}

/// Re-renders the manifest-dependent OPcache functions against the COMPLETE script manifest and
/// substitutes them into the program, replacing the placeholder declarations `inject_if_used`
/// left behind. A no-op when no such function was injected.
///
/// # Why injection and baking are split
///
/// The pipeline order is `resolver::resolve` → `opcache_prelude::inject_if_used` →
/// `name_resolver::resolve` → `autoload::run`. The manifest's third group — the autoloaded
/// files — is produced by `autoload::run`, which runs LAST, because PSR-4 resolution is a
/// fixpoint over CANONICAL class FQNs and those only exist after name resolution. So the
/// manifest is not knowable at the injection point.
///
/// Moving `inject_if_used` after `autoload::run` would fix that and break something worse: a
/// namespaced caller. `name_resolver` resolves an unqualified `opcache_get_status()` written
/// inside `namespace App;` by consulting the symbol table it collected from the program — if
/// the declaration is not there yet, the call does not resolve to the injected function. The
/// DECLARATION must therefore exist before name resolution; only its BODY needs the manifest.
///
/// # The mechanism, and why it is sound
///
/// `inject_if_used` renders each function with the manifest it can see at that point (entry +
/// includes + Composer `autoload.files`) — a valid, parseable, self-consistent placeholder.
/// This pass then re-renders the same functions from the same templates with the full manifest,
/// parses them, and swaps the whole top-level `FunctionDecl` statement in by name.
///
/// The freshly parsed declarations are run through `name_resolver::resolve` in isolation before
/// substitution. That is the same device `autoload::load_autoloaded_file` already uses to splice
/// a file parsed after the main name-resolution pass, and it is exact here for a stronger
/// reason: these bodies live in the GLOBAL namespace with no `use` imports, so every name in
/// them (`realpath`, `in_array`, `date`, `fwrite`, `STDERR`) resolves identically whether it is
/// resolved with the whole program's symbol table or with its own — there is no namespace or
/// import context that could differ. `substitutes_a_name_resolution_identical_body` pins that
/// equality on the real rendered bodies.
///
/// Baking runs BEFORE `optimize::fold_constants` and the type checker, so the substituted
/// literals go through every later pass exactly as the placeholder would have.
///
/// A recorded site that is not found is a COMPILER BUG (something moved or dropped a
/// declaration this module injected), and panics rather than silently shipping a binary whose
/// `scripts` map omits the autoloaded files — the same policy as the `expect`s above.
pub fn bake_manifest(
    program: Program,
    sites: &ManifestBakeSites,
    php_version: PhpVersion,
    web: bool,
    manifest: &[ScriptEntry],
    overrides: &[(String, String)],
    preload: Option<&PreloadStatistics>,
    strict: bool,
) -> Program {
    if sites.is_empty() {
        return program;
    }

    let mut baked: Vec<(&str, Stmt)> = Vec::new();
    crate::synthetic_class::internal_declarations(|| {
        if sites.get_status {
            baked.push((
                GET_STATUS_FN,
                resolve_baked_function(get_status_declaration(
                    php_version,
                    web,
                    manifest,
                    overrides,
                    sites.restricted,
                    preload,
                )),
            ));
        }
        if sites.is_script_cached {
            baked.push((
                IS_SCRIPT_CACHED_FN,
                resolve_baked_function(is_script_cached_declaration(
                    php_version,
                    web,
                    manifest,
                    overrides,
                )),
            ));
        }
        if sites.compile_file {
            baked.push((
                COMPILE_FILE_FN,
                resolve_baked_function(compile_file_declaration(
                    php_version,
                    web,
                    manifest,
                    overrides,
                )),
            ));
        }
        if sites.invalidate {
            baked.push((
                INVALIDATE_FN,
                resolve_baked_function(invalidate_declaration(
                    php_version,
                    web,
                    manifest,
                    overrides,
                    strict,
                )),
            ));
        }
        Vec::new()
    });

    let mut program = program;
    for stmt in program.iter_mut() {
        let StmtKind::FunctionDecl { name, .. } = &stmt.kind else {
            continue;
        };
        // PHP function names are case-insensitive; the templates declare them lowercase.
        let name = name.to_ascii_lowercase();
        let Some(index) = baked.iter().position(|(fn_name, _)| *fn_name == name) else {
            continue;
        };
        *stmt = baked.swap_remove(index).1;
    }

    assert!(
        baked.is_empty(),
        "opcache prelude: injected function(s) {:?} vanished before manifest baking",
        baked.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
    );
    program
}

/// Name-resolves one freshly built OPcache function declaration in isolation, returning the
/// single top-level `FunctionDecl` statement [`bake_manifest`] substitutes.
///
/// Resolving in isolation is the same device `autoload::load_autoloaded_file` uses to splice a
/// file produced after the main name-resolution pass, and it is exact here for a stronger reason:
/// these bodies live in the GLOBAL namespace with no `use` imports, so every name in them
/// (`realpath`, `in_array`, `date`, `fwrite`, `STDERR`) resolves identically whether it is
/// resolved with the whole program's symbol table or with its own.
///
/// The declaration is compiler-built data, so a resolve failure is a compiler bug and panics
/// rather than degrading silently — matching `inject_if_used`.
pub(super) fn resolve_baked_function(declaration: Stmt) -> Stmt {
    let mut resolved = crate::name_resolver::resolve(vec![declaration])
        .expect("opcache prelude must name-resolve");
    assert_eq!(
        resolved.len(),
        1,
        "opcache prelude: a baked function must render exactly one top-level declaration",
    );
    resolved.remove(0)
}
